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
//!
//! The **key** of a session is the id its client declared, else the id of its
//! transport (D4). A client that declares `GRAPHIUM_SESSION` keeps one record
//! and one set of diagrams across transports, so `/mcp reconnect` or a client
//! restart does not lose the plan; the grace applies to the transport that
//! closed, not to the session: while another transport holds the same declared
//! session, the session stays active.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use serde::Serialize;

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

impl SessionState {
    /// The wire string of the state: `active|in_grace`.
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionState::Active => "active",
            SessionState::InGrace => "in_grace",
        }
    }
}

/// What the registry answers to a session that arrives (D6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionAdmission {
    /// The session is in the registry; a reconnect refreshes the record it
    /// already had instead of opening a second one.
    Accepted(SessionRecord),
    /// There is no room: `open` sessions are active, the owner's included.
    Refused { open: usize },
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

    /// The session as the agent answers it (S3, D2): the id, the family, the
    /// label and the project the human sees in the selector, and whether it
    /// is active or in grace. It never carries another session (D3).
    pub fn view(&self) -> SessionView {
        SessionView {
            id: self.id.clone(),
            family: self.family.as_str(),
            label: self.label.clone(),
            project: self.project.clone(),
            state: self.state.as_str(),
        }
    }
}

/// The session as the tools report it: the `session` object of
/// `register_session`, `render_diagram` and `list_diagrams` (S3, D2).
///
/// `family` and `state` are the wire strings of [`SessionFamily`] and
/// [`SessionState`], so an invalid one cannot travel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionView {
    pub id: String,
    pub family: &'static str,
    pub label: String,
    /// The directory the client declared, or the `cwd` of the process (D1).
    pub project: Option<String>,
    pub state: &'static str,
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

/// The sessions of the owner process, with the transports that hold them.
#[derive(Debug, Default)]
pub struct SessionRegistry {
    sessions: Mutex<Sessions>,
}

/// The state of the registry, under its one lock.
#[derive(Debug, Default)]
struct Sessions {
    /// The sessions, by key: the id the client declared, else the id of the
    /// transport (D4).
    records: BTreeMap<String, SessionRecord>,
    /// Which session each transport is holding. A declared session can be
    /// held by two transports at once, and closing one of them is not the end
    /// of the session (D4).
    transports: BTreeMap<String, String>,
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers the session of a transport, or refreshes it when the same
    /// session arrives again (D4).
    ///
    /// The key is the id the client declared, else the id of the transport
    /// (`session_key`): the same declared id arriving on a second transport
    /// recovers the record it had instead of opening a second one. A reconnect
    /// keeps the diagrams and a label the agent set, and clears the grace,
    /// because the session is back.
    pub fn connect(&self, transport_id: &str, identity: &ResolvedIdentity) -> SessionRecord {
        let key = session_key(transport_id, identity);
        let mut sessions = self.lock();
        let record = insert_or_refresh(&mut sessions.records, &key, identity);
        hold(&mut sessions, transport_id, &key, SystemTime::now());
        record
    }

    /// Registers the owner's own stdio transport under its cerco.
    ///
    /// The key of this transport is `stdio` and not the id its client declares
    /// (PRD §11.1): the tools of the owner answer with that string, and it is
    /// the one transport that cannot come back — it ends with the process — so
    /// no declared id has a reconnect to recover there.
    fn connect_own_transport(&self, id: &str, identity: &ResolvedIdentity) -> SessionRecord {
        insert_or_refresh(&mut self.lock().records, id, identity)
    }

    /// Registers a session only when there is room for it (D6).
    ///
    /// The quota counts the **active** sessions, the owner's stdio session
    /// included. A session in grace already ended for the quota: counting it
    /// would lock a client out for the 45 s of D4 after every restart, and the
    /// slot it frees is exactly what makes `too_many_sessions` about live
    /// work instead of about history.
    ///
    /// A reconnect of a registered session always lands: it refreshes the
    /// record that was already there instead of opening a new one, and a second
    /// transport of a declared session spends no slot of its own.
    pub fn admit(
        &self,
        transport_id: &str,
        identity: &ResolvedIdentity,
        limit: usize,
    ) -> SessionAdmission {
        let key = session_key(transport_id, identity);
        let mut sessions = self.lock();
        if !sessions.records.contains_key(&key) {
            let open = active_count(&sessions.records);
            if open >= limit {
                return SessionAdmission::Refused { open };
            }
        }
        let record = insert_or_refresh(&mut sessions.records, &key, identity);
        hold(&mut sessions, transport_id, &key, SystemTime::now());
        SessionAdmission::Accepted(record)
    }

    /// How many sessions are active: the number the quota spends (D6).
    pub fn active_count(&self) -> usize {
        active_count(&self.lock().records)
    }

    /// Sets the label of a session; an empty label goes back to the
    /// automatic name (D2).
    pub fn set_label(&self, id: &str, label: &str) -> Option<SessionRecord> {
        let mut sessions = self.lock();
        let record = sessions.records.get_mut(id)?;
        record.label = match non_empty(Some(label)) {
            Some(label) => label.to_string(),
            None => auto_label(record.family, record.project.as_deref(), &record.id),
        };
        Some(record.clone())
    }

    /// Notes that a diagram belongs to a session. Idempotent.
    pub fn note_diagram(&self, id: &str, diagram_id: &str) -> bool {
        let mut sessions = self.lock();
        let Some(record) = sessions.records.get_mut(id) else {
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
        let Some(record) = sessions.records.get_mut(id) else {
            return false;
        };
        record.diagram_ids.retain(|known| known != diagram_id);
        true
    }

    /// Puts the session of `id` into the grace of D4 until `now + SESSION_GRACE`.
    ///
    /// `id` is the transport that closed, or the key of the session when the
    /// caller has no transport to name. The session only goes into grace when
    /// no other transport of the same declared session is still holding it:
    /// what ended is a connection, not the session.
    pub fn begin_grace(&self, id: &str, now: SystemTime) -> Option<SessionRecord> {
        let mut sessions = self.lock();
        let key = sessions
            .transports
            .remove(id)
            .unwrap_or_else(|| id.to_string());
        if !sessions.transports.values().any(|held| held == &key) {
            begin_grace_in(&mut sessions, &key, now);
        }
        sessions.records.get(&key).cloned()
    }

    /// The ids of the sessions whose grace ended at `now`, in order.
    ///
    /// The registry only reports them: [`drain_expired`] is the one that ends
    /// them.
    pub fn expired(&self, now: SystemTime) -> Vec<String> {
        expired_ids(&self.lock().records, now)
    }

    /// Ends every session whose grace finished at `now` and returns their
    /// ids, in order (D4).
    ///
    /// Removing is the ending: the session leaves the registry, so the id is
    /// free and its slot is back, and no transport points at it any more. It
    /// is one step under one lock, so a reconnect cannot slip between the
    /// check and the deletion.
    pub fn drain_expired(&self, now: SystemTime) -> Vec<String> {
        let mut sessions = self.lock();
        let ended = expired_ids(&sessions.records, now);
        for id in &ended {
            sessions.records.remove(id);
            sessions.transports.retain(|_, held| held != id);
        }
        ended
    }

    /// One session, if it is registered.
    pub fn get(&self, id: &str) -> Option<SessionRecord> {
        self.lock().records.get(id).cloned()
    }

    /// The record behind a transport or behind a key (D4).
    ///
    /// A client that declared its id is registered under that id, and the
    /// tools name the transport it came in through: this is the one place
    /// that answers one with the other, so a session with a declared id is
    /// found by whichever of the two the caller has.
    pub fn resolve(&self, id: &str) -> Option<SessionRecord> {
        let sessions = self.lock();
        if let Some(record) = sessions.records.get(id) {
            return Some(record.clone());
        }
        let key = sessions.transports.get(id)?;
        sessions.records.get(key).cloned()
    }

    /// Every session, in a deterministic order.
    pub fn list(&self) -> Vec<SessionRecord> {
        self.lock().records.values().cloned().collect()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Sessions> {
        self.sessions
            .lock()
            .expect("the session registry is not poisoned")
    }
}

/// The key a session is registered under (D4): the id its client declared,
/// else the id of its transport.
///
/// This is the one place the key is chosen, so `connect` and `admit` cannot
/// disagree about which record a transport lands on: a client that declares
/// `GRAPHIUM_SESSION` keeps its session, and one that declares nothing keeps
/// the id of the connection it came in through.
fn session_key(transport_id: &str, identity: &ResolvedIdentity) -> String {
    identity
        .id
        .clone()
        .unwrap_or_else(|| transport_id.to_string())
}

/// Binds a transport to the session it holds, letting go of the key it held
/// before (D4).
///
/// A client that declares its id after its connection was admitted moves from
/// the key of its transport to the key of its declared id. Nothing is lost: the
/// registration it leaves behind has no diagrams — it never said who it was — so
/// it is dropped instead of going into grace, where it would be a second, empty
/// session in the selector. If it did draw before declaring, the grace keeps its
/// diagrams as D4 says.
fn hold(sessions: &mut Sessions, transport_id: &str, key: &str, now: SystemTime) {
    let previous = sessions
        .transports
        .insert(transport_id.to_string(), key.to_string());
    let Some(previous) = previous else { return };
    if previous == key || sessions.transports.values().any(|held| held == &previous) {
        return;
    }
    let undrawn = sessions
        .records
        .get(&previous)
        .is_some_and(|record| record.diagram_ids.is_empty());
    if undrawn {
        sessions.records.remove(&previous);
    } else {
        begin_grace_in(sessions, &previous, now);
    }
}

/// Moves one session of the map into the grace of D4.
fn begin_grace_in(sessions: &mut Sessions, key: &str, now: SystemTime) {
    let Some(record) = sessions.records.get_mut(key) else {
        return;
    };
    record.state = SessionState::InGrace;
    record.grace_deadline = Some(now + SESSION_GRACE);
    record.last_seen = now;
}

/// Registers a session in the map, or refreshes the one with the same id.
///
/// Every path that opens a session goes through here, under the lock the
/// caller already holds: `connect` and `admit` cannot disagree about what a
/// reconnect means (D4).
fn insert_or_refresh(
    sessions: &mut BTreeMap<String, SessionRecord>,
    id: &str,
    identity: &ResolvedIdentity,
) -> SessionRecord {
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

/// How many sessions of the map are active (D6).
fn active_count(sessions: &BTreeMap<String, SessionRecord>) -> usize {
    sessions
        .values()
        .filter(|record| record.state == SessionState::Active)
        .count()
}

/// The ids of the sessions whose grace finished at `now`, in order.
fn expired_ids(sessions: &BTreeMap<String, SessionRecord>, now: SystemTime) -> Vec<String> {
    sessions
        .values()
        .filter(|record| record.grace_expired(now))
        .map(|record| record.id.clone())
        .collect()
}

/// Ends the sessions whose grace finished at `now` (D4, S2): the session
/// leaves the registry and the store deletes every diagram of it.
///
/// The store calls this on its entry point ([`crate::store::diagram_store`]),
/// which is where both transports and the panel already land: the expiry needs
/// no thread per session and no timer, and the plan is up to date on the next
/// call that touches it. The check costs a registry lock and a scan of the
/// sessions, and the ones whose grace is still running are left alone.
///
/// The diagrams go with the session, so a session that ended does not leave
/// its plan behind (S2). Returns the ids that ended, in order.
pub fn end_expired_sessions(
    registry: &SessionRegistry,
    store: &crate::store::DiagramStore,
    now: SystemTime,
) -> Vec<String> {
    let ended = registry.drain_expired(now);
    for id in &ended {
        store.clear_session(id);
    }
    ended
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

/// Registers the session of the owner's own stdio transport and returns the
/// identity it was registered with.
pub fn register_transport_session(id: &str) -> ResolvedIdentity {
    let identity = current_identity();
    session_registry().connect_own_transport(id, &identity);
    identity
}

/// The session behind a transport, as the tools answer it (S3, D2).
///
/// A transport the registry does not know — a forwarded socket before it is
/// registered, a server built by hand — still answers as itself: its own id,
/// the family and the project this process declares, the automatic label of
/// that identity and the active state. A read never opens a session: the
/// registry is written by the transports and by [`name_session`].
pub fn current_view(id: &str) -> SessionView {
    session_registry()
        .resolve(id)
        .map_or_else(|| derived_view(id), |record| record.view())
}

/// The view of a transport the registry does not know yet.
fn derived_view(id: &str) -> SessionView {
    let identity = current_identity();
    SessionView {
        id: id.to_string(),
        family: identity.family.as_str(),
        label: auto_label(identity.family, identity.project.as_deref(), id),
        project: identity.project,
        state: SessionState::Active.as_str(),
    }
}

/// Names the session of a transport and answers it (S3, D2).
///
/// The label arrives trimmed and validated by the caller: an empty one is its
/// error, never the reset [`SessionRegistry::set_label`] performs. A session
/// the registry does not know is registered first — naming a session is the
/// one call that says "I am here" — keyed like its transport, so the owner's
/// `stdio` keeps its cerco (PRD §11.1). If the session ended between the two
/// steps the answer is the record as it is, without inventing a label.
pub fn name_session(transport_id: &str, label: &str) -> SessionView {
    let registry = session_registry();
    let known = registry.resolve(transport_id).unwrap_or_else(|| {
        let identity = current_identity();
        if transport_id == STDIO_SESSION_ID {
            registry.connect_own_transport(transport_id, &identity)
        } else {
            registry.connect(transport_id, &identity)
        }
    });
    registry
        .set_label(&known.id, label)
        .map_or_else(|| known.view(), |record| record.view())
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

    /// A diagram of one session, in a store the test owns.
    fn insert_diagram(store: &crate::store::DiagramStore, id: &str, session: &str) -> String {
        store
            .insert(
                id.to_string(),
                "t".to_string(),
                "graph".to_string(),
                session.to_string(),
                "{}".to_string(),
            )
            .diagram_id
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
        let undeclared = resolve(
            &declared(Some("opencode"), None, None),
            &ClientFingerprint::default(),
            &ProcessFingerprint::default(),
        );
        registry.connect("a-session", &undeclared);
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

    /// D4/S2: when the grace of a session finishes, its diagrams are deleted
    /// and it leaves the registry; the session next door is untouched.
    #[test]
    fn ending_a_session_deletes_its_diagrams_after_the_grace() {
        let registry = SessionRegistry::new();
        let store = crate::store::DiagramStore::new();
        let identity = resolve(
            &declared(Some("pi"), Some("pi-1"), Some("/work/graphium")),
            &ClientFingerprint::default(),
            &ProcessFingerprint::default(),
        );

        registry.connect("pi-1", &identity);
        // The session next door declares no id, so its key is its transport id
        // — with a declared id it would be the same session as `pi-1` (D4).
        let neighbour = resolve(
            &declared(Some("claude"), None, Some("/work/graphium")),
            &ClientFingerprint::default(),
            &ProcessFingerprint::default(),
        );
        registry.connect("claude-1", &neighbour);
        insert_diagram(&store, "d-0001", "pi-1");
        insert_diagram(&store, "d-0002", "claude-1");

        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(2_000_000);
        let closed = registry.begin_grace("pi-1", now).expect("the session");
        assert_eq!(closed.state, SessionState::InGrace);

        // Inside the grace the session still has its diagrams.
        assert!(end_expired_sessions(
            &registry,
            &store,
            now + SESSION_GRACE - Duration::from_millis(1)
        )
        .is_empty());
        assert_eq!(store.list_for_session("pi-1").len(), 1);
        assert!(registry.get("pi-1").is_some());

        // Past the deadline the session ends: its diagrams go with it.
        assert_eq!(
            end_expired_sessions(
                &registry,
                &store,
                now + SESSION_GRACE + Duration::from_millis(1)
            ),
            vec!["pi-1".to_string()]
        );
        assert!(store.record("d-0001").is_none(), "the diagram is gone");
        assert!(store.list_for_session("pi-1").is_empty());
        assert!(
            registry.get("pi-1").is_none(),
            "the session left the registry"
        );

        // The other session survives untouched.
        assert!(store.record("d-0002").is_some());
        assert_eq!(store.list_for_session("claude-1").len(), 1);
        assert!(registry.get("claude-1").is_some());
        assert!(end_expired_sessions(
            &registry,
            &store,
            now + SESSION_GRACE + Duration::from_secs(600)
        )
        .is_empty());
    }

    /// D4: a reconnect with the same declared id inside the grace keeps the
    /// session and its diagrams: no deadline is left to run out.
    #[test]
    fn a_reconnect_inside_the_grace_keeps_the_diagrams() {
        let registry = SessionRegistry::new();
        let store = crate::store::DiagramStore::new();
        let identity = resolve(
            &declared(Some("pi"), Some("pi-4824"), Some("/work/graphium")),
            &ClientFingerprint::default(),
            &ProcessFingerprint::default(),
        );

        registry.connect("pi-4824", &identity);
        insert_diagram(&store, "d-0001", "pi-4824");

        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(3_000_000);
        registry.begin_grace("pi-4824", now);
        let reconnected = registry.connect("pi-4824", &identity);
        assert_eq!(reconnected.state, SessionState::Active);
        assert_eq!(reconnected.grace_deadline, None);

        assert!(end_expired_sessions(
            &registry,
            &store,
            now + SESSION_GRACE + Duration::from_secs(600)
        )
        .is_empty());
        assert!(store.record("d-0001").is_some(), "the diagram stays");
        assert_eq!(store.list_for_session("pi-4824").len(), 1);
        assert!(registry.get("pi-4824").is_some());
    }

    /// D4/S4: the key of a session is the id its client declared, so two
    /// transports that declare the same one are one session with one set of
    /// diagrams; the first to close does not end it.
    #[test]
    fn two_transports_of_one_declared_session_share_one_record_and_its_diagrams() {
        let registry = SessionRegistry::new();
        let store = crate::store::DiagramStore::new();
        let identity = resolve(
            &declared(Some("pi"), Some("pi-4824"), Some("/work/graphium")),
            &ClientFingerprint::default(),
            &ProcessFingerprint::default(),
        );

        let first = registry.connect("sock-1", &identity);
        let diagram = insert_diagram(&store, "d-0001", &first.id);
        assert!(registry.note_diagram(&first.id, &diagram));

        let second = registry.connect("sock-2", &identity);
        assert_eq!(
            registry.list().len(),
            1,
            "one record for one declared session, not one per transport"
        );
        assert_eq!(second.id, first.id, "the declared id is the key");
        assert_eq!(
            second.diagram_ids,
            vec![diagram.clone()],
            "both transports see the diagrams of the session"
        );
        assert_eq!(store.list_for_session(&second.id).len(), 1);
        assert_eq!(
            registry.active_count(),
            1,
            "the quota counts the session once, not once per transport"
        );

        // The first transport closes: the second still holds the session, so
        // it stays active (D4).
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(6_000_000);
        let held = registry.begin_grace("sock-1", now).expect("the session");
        assert_eq!(held.state, SessionState::Active);
        assert_eq!(held.grace_deadline, None);
        assert_eq!(held.diagram_ids, vec![diagram.clone()]);

        // The last transport closes: now the grace starts, with the diagrams.
        let closed = registry.begin_grace("sock-2", now).expect("the session");
        assert_eq!(closed.state, SessionState::InGrace);
        assert_eq!(closed.diagram_ids, vec![diagram]);
    }

    /// D4/S4: the transport closes and the session waits in grace with its
    /// diagrams; a reconnect that declares the same id recovers the record,
    /// so the deadline has nothing to delete.
    #[test]
    fn a_reconnect_with_the_declared_id_recovers_the_session_and_its_diagrams() {
        let registry = SessionRegistry::new();
        let store = crate::store::DiagramStore::new();
        let identity = resolve(
            &declared(Some("pi"), Some("pi-4824"), Some("/work/graphium")),
            &ClientFingerprint::default(),
            &ProcessFingerprint::default(),
        );

        let first = registry.connect("sock-1", &identity);
        let diagram = insert_diagram(&store, "d-0001", &first.id);
        assert!(registry.note_diagram(&first.id, &diagram));

        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(5_000_000);
        let closed = registry.begin_grace("sock-1", now).expect("the session");
        assert_eq!(closed.state, SessionState::InGrace);
        assert_eq!(closed.diagram_ids, vec![diagram.clone()]);

        // The client reconnects on another socket and declares the same id.
        let reconnected = registry.connect("sock-2", &identity);
        assert_eq!(
            reconnected.diagram_ids,
            vec![diagram.clone()],
            "the reconnect must recover the diagrams of the declared session"
        );
        assert_eq!(reconnected.id, first.id, "the declared id is the key");
        assert_eq!(reconnected.state, SessionState::Active);
        assert_eq!(reconnected.grace_deadline, None);

        // The session came back: the deadline has nothing to end.
        assert!(end_expired_sessions(
            &registry,
            &store,
            now + SESSION_GRACE + Duration::from_secs(600)
        )
        .is_empty());
        assert!(
            store.record(&diagram).is_some(),
            "the diagram survives the deadline because the session came back"
        );
        assert_eq!(registry.list().len(), 1, "one session, not two");
    }

    /// D4/S2: a session with nothing declared keeps its transport id as key
    /// and dies after the grace with its diagrams; the diagrams of the session
    /// next door survive (D3).
    #[test]
    fn a_session_without_a_declared_id_dies_after_the_grace_with_its_diagrams() {
        let registry = SessionRegistry::new();
        let store = crate::store::DiagramStore::new();
        // Nothing declared: the protocol only names the family (D1).
        let undeclared = resolve(
            &Declared::default(),
            &client(Some("opencode"), None),
            &process(Some("opencode"), None),
        );
        let named = resolve(
            &declared(Some("pi"), Some("pi-1"), Some("/work/graphium")),
            &ClientFingerprint::default(),
            &ProcessFingerprint::default(),
        );

        let unknown = registry.connect("sock-7", &undeclared);
        assert_eq!(
            unknown.id, "sock-7",
            "without a declared id the transport id is the key"
        );
        let ephemeral = insert_diagram(&store, "d-0001", &unknown.id);
        assert!(registry.note_diagram(&unknown.id, &ephemeral));

        let neighbour = registry.connect("sock-8", &named);
        let kept = insert_diagram(&store, "d-0002", &neighbour.id);
        assert!(registry.note_diagram(&neighbour.id, &kept));

        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(7_000_000);
        registry.begin_grace("sock-7", now).expect("the session");

        // Inside the grace the diagrams are still there.
        assert!(end_expired_sessions(
            &registry,
            &store,
            now + SESSION_GRACE - Duration::from_millis(1)
        )
        .is_empty());
        assert!(store.record(&ephemeral).is_some());

        // Past the deadline the session ends and its diagrams go with it.
        assert_eq!(
            end_expired_sessions(
                &registry,
                &store,
                now + SESSION_GRACE + Duration::from_millis(1)
            ),
            vec!["sock-7".to_string()]
        );
        assert!(store.record(&ephemeral).is_none());
        assert!(registry.get("sock-7").is_none());

        // The session next door keeps its record and its diagram.
        assert!(store.record(&kept).is_some());
        assert_eq!(store.list_for_session(&neighbour.id).len(), 1);
        assert!(registry.get(&neighbour.id).is_some());
    }

    /// D6: the quota is spent by the active sessions, the owner's stdio one
    /// included; a reconnect of a known id always lands.
    #[test]
    fn the_quota_counts_the_active_sessions_and_admits_a_reconnect() {
        let registry = SessionRegistry::new();
        let identity = resolve(
            &declared(Some("pi"), None, None),
            &ClientFingerprint::default(),
            &ProcessFingerprint::default(),
        );
        let limit = 3;
        registry.connect(STDIO_SESSION_ID, &identity);
        registry.connect("sock-1", &identity);
        registry.connect("sock-2", &identity);
        assert_eq!(registry.active_count(), limit);

        assert_eq!(
            registry.admit("sock-ninth", &identity, limit),
            SessionAdmission::Refused { open: limit }
        );
        assert!(registry.get("sock-ninth").is_none());

        // A session in grace already ended for the quota: its slot is free.
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(4_000_000);
        registry.begin_grace("sock-1", now);
        assert_eq!(registry.active_count(), limit - 1);
        assert!(matches!(
            registry.admit("sock-new", &identity, limit),
            SessionAdmission::Accepted(_)
        ));
        assert!(matches!(
            registry.admit("sock-1", &identity, limit),
            SessionAdmission::Accepted(_)
        ));
        assert_eq!(
            registry.get("sock-1").expect("the session").state,
            SessionState::Active,
            "a reconnect brings the session back"
        );
    }

    /// D4/S2: the check runs where the plan is already touched, without a
    /// thread per session: reading the store ends the sessions whose grace
    /// finished.
    #[test]
    fn the_store_entry_point_ends_the_sessions_whose_grace_finished() {
        let _guard = crate::test_support::lock_renders_blocking();
        let id = format!("wire-end-{}", std::process::id());
        let registry = session_registry();
        let identity = resolve(
            &declared(Some("pi"), Some(&id), Some("/work/graphium")),
            &ClientFingerprint::default(),
            &ProcessFingerprint::default(),
        );
        registry.connect(&id, &identity);

        let store = crate::store::diagram_store();
        let diagram = insert_diagram(store, &store.next_id(), &id);
        assert!(store.record(&diagram).is_some());

        // The transport closed long ago: the grace already ran out.
        registry.begin_grace(
            &id,
            SystemTime::now() - SESSION_GRACE - Duration::from_secs(1),
        );

        // The next touch of the store is the check: the session ends here.
        let store = crate::store::diagram_store();
        assert!(
            registry.get(&id).is_none(),
            "the store entry point ended the session"
        );
        assert!(
            store.list_for_session(&id).is_empty(),
            "and deleted its diagrams"
        );
    }

    /// PRESERVE: the owner's own stdio transport is keyed by `stdio` and never
    /// by the id its client declares, so the cerco of a local call (PRD §11.1)
    /// and the diagrams the panel lists keep that string.
    #[test]
    fn the_owner_stdio_session_keeps_the_stdio_key_when_its_client_declares_one() {
        let identity = register_transport_session(STDIO_SESSION_ID);

        let record = session_registry()
            .get(STDIO_SESSION_ID)
            .expect("the owner's stdio session is registered");
        assert_eq!(record.id, STDIO_SESSION_ID);
        assert_eq!(record.family, identity.family);
        assert_eq!(record.state, SessionState::Active);
        if let Some(declared) = identity.id.as_deref() {
            assert_ne!(declared, STDIO_SESSION_ID);
            assert!(
                session_registry().get(declared).is_none(),
                "the declared id does not become a session of the owner"
            );
        }
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

    /// D2/D3: a transport the registry does not know answers as itself — its
    /// own id, the family this process declares — and reading never registers
    /// a session: only a transport or `register_session` do.
    #[test]
    fn an_unknown_transport_answers_as_itself_without_registering() {
        let id = format!("never-registered-{}", std::process::id());
        let view = current_view(&id);

        assert_eq!(view.id, id);
        assert_eq!(view.family, current_identity().family.as_str());
        assert_eq!(view.state, "active");
        assert!(
            session_registry().resolve(&id).is_none(),
            "a read never opens a session"
        );
    }

    /// S3/D2/D4: naming a session registers it when it is not there and keys
    /// it like its transport, so `stdio` keeps the cerco of PRD §11.1.
    #[test]
    fn naming_an_unknown_session_registers_it_and_answers_it() {
        let id = format!("named-{}", std::process::id());
        let view = name_session(&id, "presupuesto API");

        assert_eq!(view.label, "presupuesto API");
        assert!(
            session_registry().resolve(&id).is_some(),
            "naming a session registers it"
        );
        assert_eq!(
            session_registry()
                .get(&view.id)
                .expect("the record is there")
                .label,
            "presupuesto API"
        );
    }
}
