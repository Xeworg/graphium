//! The visible sessions of the owner process (feature `f3-sesiones`, S4/S6,
//! D1/D4).
//!
//! A session is no longer a bare string: it has a family (`pi`, `claude`,
//! `opencode`, `ui`, `other`), the name the client declared, the project it
//! works on, a readable label and a lifecycle (`Active` / `InGrace`). The
//! registry is a `BTreeMap` (never a `HashMap`) so `list` has a deterministic
//! order (PRD §8.5).
//!
//! Identity is resolved in three levels and **none of them guesses**: what the
//! client declared (`GRAPHIUM_CLIENT`/`GRAPHIUM_SESSION`/`GRAPHIUM_PROJECT`),
//! what the protocol reported (`clientInfo.name`/`version`) and what the
//! process says (parent name and `cwd`). [`resolve`] is a pure function of the
//! three, so the matrix is tested without touching the environment; the
//! adapters at the bottom read the real sources.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

/// The `session_id` of the stdio session of the owner (PRD §11.1).
///
/// The tools keep tagging their diagrams with this same string: the registry
/// describes the session, it does not replace the cerco.
pub const STDIO_SESSION_ID: &str = "stdio";

/// How long a session whose transport closed stays before it is deleted
/// (D4: 45 s, so a reconnect or a client restart does not erase the plan).
pub const SESSION_GRACE: Duration = Duration::from_secs(45);

/// The kind of agent behind a session (S4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionFamily {
    Pi,
    Claude,
    OpenCode,
    Ui,
    Other,
}

impl SessionFamily {
    /// The wire string of the family: `pi|claude|opencode|ui|other`.
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionFamily::Pi => "pi",
            SessionFamily::Claude => "claude",
            SessionFamily::OpenCode => "opencode",
            SessionFamily::Ui => "ui",
            SessionFamily::Other => "other",
        }
    }

    /// The family named by a client name, or [`SessionFamily::Other`].
    ///
    /// Case-insensitive, trimmed, and the version suffix is ignored:
    /// `pi*` → `Pi`, `claude*` (also `claude-ai`, `claude-code`) → `Claude`,
    /// `opencode*` → `OpenCode`, anything else → `Other`.
    pub fn from_client_name(name: &str) -> Self {
        let normalized = name.trim().to_lowercase();
        // A version arrives attached as `pi/0.9.1`, `claude-ai 2.0` or
        // `opencode@1.2.3`: the name is the token before it.
        let head = normalized
            .split(['/', ' ', '@', ':'])
            .next()
            .unwrap_or_default();
        if head.starts_with("claude") {
            SessionFamily::Claude
        } else if head.starts_with("opencode") {
            SessionFamily::OpenCode
        } else if head.starts_with("pi") {
            SessionFamily::Pi
        } else if head.starts_with("ui") {
            SessionFamily::Ui
        } else {
            SessionFamily::Other
        }
    }

    /// The name the human reads in the selector (`Pi`, `Claude`, …).
    pub fn display_name(&self) -> &'static str {
        match self {
            SessionFamily::Pi => "Pi",
            SessionFamily::Claude => "Claude",
            SessionFamily::OpenCode => "OpenCode",
            SessionFamily::Ui => "Interfaz",
            SessionFamily::Other => "Otro",
        }
    }
}

/// Where a session is in its life (D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Active,
    InGrace,
}

/// One session of the owner process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRecord {
    pub id: String,
    pub family: SessionFamily,
    pub client_name: Option<String>,
    pub client_version: Option<String>,
    pub project: Option<String>,
    pub label: String,
    pub state: SessionState,
    /// When the grace ends; `None` while the session is active.
    pub grace_deadline: Option<SystemTime>,
    pub diagram_ids: Vec<String>,
    pub connected_at: SystemTime,
    pub last_seen: SystemTime,
}

impl SessionRecord {
    /// Whether the grace of this session ended at `now`.
    ///
    /// Only a session in grace expires: an active session has no deadline.
    pub fn grace_expired(&self, now: SystemTime) -> bool {
        matches!(self.state, SessionState::InGrace)
            && self.grace_deadline.is_some_and(|deadline| now >= deadline)
    }
}

/// The identity the client declares through its environment (D1, level 1).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Declared {
    pub client: Option<String>,
    pub session: Option<String>,
    pub project: Option<String>,
}

/// The identity the protocol reports in `initialize` (D1, level 2).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClientFingerprint {
    pub name: Option<String>,
    pub version: Option<String>,
}

/// The identity inferred from the process (D1, level 3).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProcessFingerprint {
    pub parent: Option<String>,
    pub cwd: Option<String>,
}

/// The level that answered the family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentitySource {
    Declared,
    ClientInfo,
    Process,
}

impl IdentitySource {
    /// The wire string of the level: `declared|clientInfo|process`.
    pub fn as_str(&self) -> &'static str {
        match self {
            IdentitySource::Declared => "declared",
            IdentitySource::ClientInfo => "clientInfo",
            IdentitySource::Process => "process",
        }
    }
}

/// The identity of one session, resolved from the three levels (D1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedIdentity {
    /// The id the client declared, for recovering the session on reconnect.
    /// `None` when nothing was declared: the transport supplies its own id.
    pub id: Option<String>,
    pub family: SessionFamily,
    pub client_name: Option<String>,
    pub client_version: Option<String>,
    pub project: Option<String>,
    pub source: IdentitySource,
}

/// Resolves the identity from the three levels, in precedence order (D1).
///
/// The session id and the project come from the declared level first; the
/// family from the declared client name, then from `clientInfo.name`, then
/// from the parent process name. The project falls back to the `cwd`. When
/// nothing names the agent the family is [`SessionFamily::Other`]: a wrongly
/// labelled agent is worse than an unlabelled one.
pub fn resolve(
    declared: &Declared,
    client: &ClientFingerprint,
    process: &ProcessFingerprint,
) -> ResolvedIdentity {
    let declared_client = non_empty(declared.client.as_deref());
    let client_name = non_empty(client.name.as_deref());
    let parent = non_empty(process.parent.as_deref());

    // The family and its version come from the same level: mixing the family
    // of one source with the version of another would invent an identity.
    let (family, client_name, client_version, source) = if let Some(name) = declared_client {
        (
            SessionFamily::from_client_name(name),
            Some(name.to_string()),
            None,
            IdentitySource::Declared,
        )
    } else if let Some(name) = client_name {
        (
            SessionFamily::from_client_name(name),
            Some(name.to_string()),
            non_empty(client.version.as_deref()).map(str::to_string),
            IdentitySource::ClientInfo,
        )
    } else {
        (
            parent.map_or(SessionFamily::Other, SessionFamily::from_client_name),
            parent.map(str::to_string),
            None,
            IdentitySource::Process,
        )
    };

    ResolvedIdentity {
        id: non_empty(declared.session.as_deref()).map(str::to_string),
        family,
        client_name,
        client_version,
        project: non_empty(declared.project.as_deref())
            .or_else(|| non_empty(process.cwd.as_deref()))
            .map(str::to_string),
        source,
    }
}

/// A string that carries a value: trimmed and not empty.
fn non_empty(value: Option<&str>) -> Option<&str> {
    let value = value?.trim();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

/// The sessions of the owner process.
#[derive(Debug, Default)]
pub struct SessionRegistry {
    sessions: Mutex<BTreeMap<String, SessionRecord>>,
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a session, or refreshes it when the same id reconnects.
    ///
    /// A reconnect keeps the diagrams and a label the agent set; it clears
    /// the grace, because the transport is back (D4).
    pub fn connect(&self, id: &str, identity: &ResolvedIdentity) -> SessionRecord {
        let mut sessions = self.lock();
        let now = SystemTime::now();

        if let Some(record) = sessions.get_mut(id) {
            // Only an automatic label follows the identity: a label the agent
            // set survives a reconnect (D2).
            let keeps_label =
                record.label != auto_label(record.family, record.project.as_deref(), &record.id);
            record.family = identity.family;
            record.client_name = identity.client_name.clone();
            record.client_version = identity.client_version.clone();
            record.project = identity.project.clone();
            record.state = SessionState::Active;
            record.grace_deadline = None;
            record.last_seen = now;
            if !keeps_label {
                record.label = auto_label(record.family, record.project.as_deref(), &record.id);
            }
            return record.clone();
        }

        let record = SessionRecord {
            id: id.to_string(),
            family: identity.family,
            client_name: identity.client_name.clone(),
            client_version: identity.client_version.clone(),
            project: identity.project.clone(),
            label: auto_label(identity.family, identity.project.as_deref(), id),
            state: SessionState::Active,
            grace_deadline: None,
            diagram_ids: Vec::new(),
            connected_at: now,
            last_seen: now,
        };
        sessions.insert(id.to_string(), record.clone());
        record
    }

    /// Sets the label of a session; an empty label goes back to the
    /// automatic name (D2).
    pub fn set_label(&self, id: &str, label: &str) -> Option<SessionRecord> {
        let mut sessions = self.lock();
        let record = sessions.get_mut(id)?;
        record.label = match non_empty(Some(label)) {
            Some(label) => label.to_string(),
            None => auto_label(record.family, record.project.as_deref(), &record.id),
        };
        Some(record.clone())
    }

    /// Notes that a diagram belongs to a session. Idempotent.
    pub fn note_diagram(&self, id: &str, diagram_id: &str) -> bool {
        let mut sessions = self.lock();
        let Some(record) = sessions.get_mut(id) else {
            return false;
        };
        if !record.diagram_ids.iter().any(|known| known == diagram_id) {
            record.diagram_ids.push(diagram_id.to_string());
        }
        true
    }

    /// Forgets a diagram of a session. Idempotent.
    pub fn forget_diagram(&self, id: &str, diagram_id: &str) -> bool {
        let mut sessions = self.lock();
        let Some(record) = sessions.get_mut(id) else {
            return false;
        };
        record.diagram_ids.retain(|known| known != diagram_id);
        true
    }

    /// Moves a session to the grace state until `now + SESSION_GRACE` (D4).
    pub fn begin_grace(&self, id: &str, now: SystemTime) -> Option<SessionRecord> {
        let mut sessions = self.lock();
        let record = sessions.get_mut(id)?;
        record.state = SessionState::InGrace;
        record.grace_deadline = Some(now + SESSION_GRACE);
        record.last_seen = now;
        Some(record.clone())
    }

    /// The ids of the sessions whose grace ended at `now`, in order.
    ///
    /// The registry only reports them: deleting their diagrams belongs to the
    /// next task (D4).
    pub fn expired(&self, now: SystemTime) -> Vec<String> {
        self.lock()
            .values()
            .filter(|record| record.grace_expired(now))
            .map(|record| record.id.clone())
            .collect()
    }

    /// One session, if it is registered.
    pub fn get(&self, id: &str) -> Option<SessionRecord> {
        self.lock().get(id).cloned()
    }

    /// Every session, in a deterministic order.
    pub fn list(&self) -> Vec<SessionRecord> {
        self.lock().values().cloned().collect()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, SessionRecord>> {
        self.sessions
            .lock()
            .expect("the session registry is not poisoned")
    }
}

/// The automatic name of a session: `<Client> · <project> · <short id>` (D2).
fn auto_label(family: SessionFamily, project: Option<&str>, id: &str) -> String {
    let mut parts = vec![family.display_name().to_string()];
    if let Some(project) = project.and_then(project_name) {
        parts.push(project);
    }
    parts.push(short_id(id));
    parts.join(" · ")
}

/// The last component of a project path: `/home/x/graphium` → `graphium`.
fn project_name(project: &str) -> Option<String> {
    Path::new(project)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
}

/// The tail of an id, short enough for a label (`pi-4824` → `4824`).
fn short_id(id: &str) -> String {
    let chars: Vec<char> = id.chars().collect();
    chars[chars.len().saturating_sub(4)..].iter().collect()
}

/// The single registry of the process, next to the diagram store.
pub fn session_registry() -> &'static SessionRegistry {
    static REGISTRY: OnceLock<SessionRegistry> = OnceLock::new();
    REGISTRY.get_or_init(SessionRegistry::new)
}

/// The identity declared through `GRAPHIUM_CLIENT`, `GRAPHIUM_SESSION` and
/// `GRAPHIUM_PROJECT` (D1, level 1).
pub fn declared_from_env() -> Declared {
    Declared {
        client: env_value("GRAPHIUM_CLIENT"),
        session: env_value("GRAPHIUM_SESSION"),
        project: env_value("GRAPHIUM_PROJECT"),
    }
}

/// The fingerprint of this process: its parent name and its `cwd` (D1).
pub fn process_fingerprint() -> ProcessFingerprint {
    ProcessFingerprint {
        parent: parent_process_name(),
        cwd: std::env::current_dir()
            .ok()
            .map(|path| path.to_string_lossy().into_owned()),
    }
}

/// The identity of this process, resolved from the real sources.
///
/// The `clientInfo` of the handshake is not available before the transport
/// starts, so a transport begins with the declared level and the process.
pub fn current_identity() -> ResolvedIdentity {
    resolve(
        &declared_from_env(),
        &ClientFingerprint::default(),
        &process_fingerprint(),
    )
}

/// Registers the session of a transport in the process registry and returns
/// its resolved identity.
pub fn register_transport_session(id: &str) -> ResolvedIdentity {
    let identity = current_identity();
    session_registry().connect(id, &identity);
    identity
}

/// An environment variable with a value: absent and empty are both `None`.
fn env_value(name: &str) -> Option<String> {
    non_empty(std::env::var(name).ok().as_deref()).map(str::to_string)
}

/// The name of the parent process: `/proc/<ppid>/comm`, else the file name of
/// `/proc/<ppid>/exe`. On other platforms there is no parent name (D1).
#[cfg(target_os = "linux")]
fn parent_process_name() -> Option<String> {
    let ppid = std::os::unix::process::parent_id();
    if let Ok(comm) = std::fs::read_to_string(format!("/proc/{ppid}/comm")) {
        if let Some(name) = non_empty(Some(&comm)) {
            return Some(name.to_string());
        }
    }
    let exe = std::fs::read_link(format!("/proc/{ppid}/exe")).ok()?;
    non_empty(exe.file_name()?.to_str()).map(str::to_string)
}

#[cfg(not(target_os = "linux"))]
fn parent_process_name() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    fn declared(client: Option<&str>, session: Option<&str>, project: Option<&str>) -> Declared {
        Declared {
            client: client.map(str::to_string),
            session: session.map(str::to_string),
            project: project.map(str::to_string),
        }
    }

    fn client(name: Option<&str>, version: Option<&str>) -> ClientFingerprint {
        ClientFingerprint {
            name: name.map(str::to_string),
            version: version.map(str::to_string),
        }
    }

    fn process(parent: Option<&str>, cwd: Option<&str>) -> ProcessFingerprint {
        ProcessFingerprint {
            parent: parent.map(str::to_string),
            cwd: cwd.map(str::to_string),
        }
    }

    /// S4/D1: the wire strings, and every client name the normalizer accepts.
    #[test]
    fn the_family_normalizer_maps_every_client_name() {
        assert_eq!(SessionFamily::Pi.as_str(), "pi");
        assert_eq!(SessionFamily::Claude.as_str(), "claude");
        assert_eq!(SessionFamily::OpenCode.as_str(), "opencode");
        assert_eq!(SessionFamily::Ui.as_str(), "ui");
        assert_eq!(SessionFamily::Other.as_str(), "other");

        let cases = [
            ("pi", SessionFamily::Pi),
            ("Pi", SessionFamily::Pi),
            ("  PI  ", SessionFamily::Pi),
            ("pi-coding-agent", SessionFamily::Pi),
            ("pi/0.9.1", SessionFamily::Pi),
            ("claude-ai", SessionFamily::Claude),
            ("claude-code", SessionFamily::Claude),
            ("Claude", SessionFamily::Claude),
            ("opencode", SessionFamily::OpenCode),
            ("OpenCode 1.2.3", SessionFamily::OpenCode),
            ("vscode", SessionFamily::Other),
            ("", SessionFamily::Other),
        ];
        for (name, expected) in cases {
            assert_eq!(SessionFamily::from_client_name(name), expected, "{name:?}");
        }
    }

    /// D1: the declared level wins over the protocol and the process.
    #[test]
    fn the_declared_identity_wins_over_the_protocol_and_the_process() {
        let identity = resolve(
            &declared(Some("pi"), Some("pi-4824"), Some("/work/graphium")),
            &client(Some("claude-ai"), Some("2.0.0")),
            &process(Some("node"), Some("/somewhere/else")),
        );

        assert_eq!(identity.id.as_deref(), Some("pi-4824"));
        assert_eq!(identity.family, SessionFamily::Pi);
        assert_eq!(identity.client_name.as_deref(), Some("pi"));
        assert_eq!(
            identity.client_version, None,
            "the version comes from the same level as the family"
        );
        assert_eq!(identity.project.as_deref(), Some("/work/graphium"));
        assert_eq!(identity.source, IdentitySource::Declared);
    }

    /// D1: without env vars the `clientInfo` of the handshake answers.
    #[test]
    fn the_protocol_answers_when_nothing_is_declared() {
        let identity = resolve(
            &Declared::default(),
            &client(Some("claude-code"), Some("1.4.0")),
            &process(Some("node"), Some("/work/api")),
        );

        assert_eq!(identity.id, None);
        assert_eq!(identity.family, SessionFamily::Claude);
        assert_eq!(identity.client_name.as_deref(), Some("claude-code"));
        assert_eq!(identity.client_version.as_deref(), Some("1.4.0"));
        assert_eq!(
            identity.project.as_deref(),
            Some("/work/api"),
            "the project falls back to the cwd"
        );
        assert_eq!(identity.source, IdentitySource::ClientInfo);
    }

    /// D1/S4: the parent process is the last resort and no family is invented.
    #[test]
    fn the_process_is_the_last_resort_and_nothing_is_invented() {
        let from_parent = resolve(
            &Declared::default(),
            &ClientFingerprint::default(),
            &process(Some(" opencode\n"), Some("/work/web")),
        );
        assert_eq!(from_parent.family, SessionFamily::OpenCode);
        assert_eq!(from_parent.client_name.as_deref(), Some("opencode"));
        assert_eq!(from_parent.project.as_deref(), Some("/work/web"));
        assert_eq!(from_parent.source, IdentitySource::Process);

        let unheard = resolve(
            &Declared::default(),
            &ClientFingerprint::default(),
            &process(Some("bash"), None),
        );
        assert_eq!(
            unheard.family,
            SessionFamily::Other,
            "an unknown parent is `other`, never a guess"
        );
        assert_eq!(unheard.client_name.as_deref(), Some("bash"));
        assert_eq!(unheard.id, None);
        assert_eq!(unheard.project, None);
        assert_eq!(unheard.source, IdentitySource::Process);

        let empty = resolve(
            &Declared::default(),
            &ClientFingerprint::default(),
            &ProcessFingerprint::default(),
        );
        assert_eq!(empty.family, SessionFamily::Other);
        assert_eq!(empty.source, IdentitySource::Process);
    }

    /// D2: the label falls back to a readable automatic name.
    #[test]
    fn the_label_falls_back_to_a_readable_automatic_name() {
        let registry = SessionRegistry::new();
        let pi = resolve(
            &declared(Some("pi"), Some("pi-4824"), Some("/home/x/graphium")),
            &ClientFingerprint::default(),
            &ProcessFingerprint::default(),
        );

        let record = registry.connect("pi-4824", &pi);
        assert_eq!(record.label, "Pi · graphium · 4824");

        let renamed = registry
            .set_label("pi-4824", "  presupuesto API  ")
            .expect("the session exists");
        assert_eq!(renamed.label, "presupuesto API");

        let cleared = registry
            .set_label("pi-4824", "   ")
            .expect("the session exists");
        assert_eq!(
            cleared.label, "Pi · graphium · 4824",
            "an empty label goes back to the automatic name"
        );
        assert!(registry.set_label("nobody", "x").is_none());

        let no_project = resolve(
            &declared(Some("opencode"), Some("oc-1"), None),
            &ClientFingerprint::default(),
            &ProcessFingerprint::default(),
        );
        assert_eq!(
            registry.connect("oc-1", &no_project).label,
            "OpenCode · oc-1"
        );

        let other = resolve(
            &declared(Some("vscode"), Some("vs-7777"), Some("/work/graphium")),
            &ClientFingerprint::default(),
            &ProcessFingerprint::default(),
        );
        assert_eq!(
            registry.connect("vs-7777", &other).label,
            "Otro · graphium · 7777"
        );
    }

    /// D4: activate, grace, expiry, and a reconnect that keeps the diagrams.
    #[test]
    fn the_registry_lifecycle_keeps_the_diagrams_and_expires_the_grace() {
        let registry = SessionRegistry::new();
        let identity = resolve(
            &declared(Some("pi"), Some("pi-4824"), Some("/work/graphium")),
            &ClientFingerprint::default(),
            &ProcessFingerprint::default(),
        );

        let record = registry.connect("pi-4824", &identity);
        assert_eq!(record.state, SessionState::Active);
        assert_eq!(record.grace_deadline, None);
        assert!(record.diagram_ids.is_empty());
        assert_eq!(record.family, SessionFamily::Pi);

        assert!(registry.note_diagram("pi-4824", "d-0001"));
        assert!(registry.note_diagram("pi-4824", "d-0002"));
        assert!(
            registry.note_diagram("pi-4824", "d-0001"),
            "noting the same diagram twice is harmless"
        );
        assert!(!registry.note_diagram("nobody", "d-0003"));
        let record = registry.get("pi-4824").expect("the session is there");
        assert_eq!(record.diagram_ids, vec!["d-0001", "d-0002"]);

        // Two sessions list in a deterministic order, not in insertion order.
        registry.connect("a-session", &identity);
        let ids: Vec<String> = registry.list().into_iter().map(|s| s.id).collect();
        assert_eq!(ids, vec!["a-session", "pi-4824"]);

        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let record = registry
            .begin_grace("pi-4824", now)
            .expect("the session is there");
        assert_eq!(record.state, SessionState::InGrace);
        assert_eq!(record.grace_deadline, Some(now + SESSION_GRACE));
        assert_eq!(
            record.diagram_ids,
            vec!["d-0001", "d-0002"],
            "the grace keeps the diagrams"
        );
        assert!(registry.begin_grace("nobody", now).is_none());

        assert!(registry.expired(now).is_empty());
        assert!(
            registry
                .expired(now + SESSION_GRACE - Duration::from_millis(1))
                .is_empty(),
            "before the deadline nothing expired"
        );
        assert_eq!(
            registry.expired(now + SESSION_GRACE + Duration::from_millis(1)),
            vec!["pi-4824"],
            "only the session in grace is reported, not the active one"
        );

        // Reconnecting with the same id clears the grace and keeps the diagrams.
        let record = registry.connect("pi-4824", &identity);
        assert_eq!(record.state, SessionState::Active);
        assert_eq!(record.grace_deadline, None);
        assert_eq!(record.diagram_ids, vec!["d-0001", "d-0002"]);
        assert!(
            registry
                .expired(now + SESSION_GRACE + Duration::from_secs(1))
                .is_empty(),
            "an active session never expires"
        );

        assert!(registry.forget_diagram("pi-4824", "d-0001"));
        assert!(
            registry.forget_diagram("pi-4824", "d-0001"),
            "forgetting twice is harmless"
        );
        assert!(!registry.forget_diagram("nobody", "d-0002"));
        assert_eq!(
            registry
                .get("pi-4824")
                .expect("the session is there")
                .diagram_ids,
            vec!["d-0002"]
        );
    }

    /// The wire-in: a transport that registers lands in the process registry
    /// with the identity resolved from the real sources.
    #[test]
    fn a_transport_registers_its_session_in_the_process_registry() {
        let id = format!("wire-{}", std::process::id());
        let identity = register_transport_session(&id);

        let record = session_registry()
            .get(&id)
            .expect("the transport session is registered");
        assert_eq!(record.id, id);
        assert_eq!(record.state, SessionState::Active);
        assert_eq!(record.family, identity.family);
        assert_eq!(record.project, identity.project);
        assert_eq!(record.grace_deadline, None);
        assert!(record.diagram_ids.is_empty());
        assert!(
            record.label.contains(&id[id.len().saturating_sub(4)..]),
            "{}",
            record.label
        );
    }
}
