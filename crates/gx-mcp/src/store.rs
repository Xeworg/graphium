//! The in-memory model of the diagrams on the plan (PRD §8.6.1, §11.1).
//!
//! The frontend is never the source of truth: the [`DiagramStore`] keeps the
//! summaries and `list_diagrams` reads this same store. The map is a
//! `BTreeMap` (never a `HashMap`) so `list` has a stable, deterministic order
//! (PRD §8.5), and `diagram_id` is monotonic and never reused (PRD §11.1,
//! RF28.1).

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Summary of a diagram placed on the plan (PRD §8.6.1, §11).
#[derive(Debug, Clone, PartialEq)]
pub struct DiagramSummary {
    pub diagram_id: String,
    pub title: String,
    pub kind: String,
    /// ISO-8601 timestamp in UTC, e.g. `2026-11-03T12:34:56Z`.
    pub updated_at: String,
    pub session_id: String,
    /// Monotonic revision: grows on every accepted update (PRD §11.1).
    pub revision: u64,
    /// Top-left corner of the object on the plan, in plan units
    /// (PRD §7.2). `None` until the window places it.
    pub pos: Option<gx_core::geom::Point>,
    /// Size of the object with its padding, in plan units (PRD §7.2).
    pub bbox: Option<gx_core::geom::Size>,
}

/// A diagram with its source document, for `get_diagram` (PRD §11).
#[derive(Debug, Clone, PartialEq)]
pub struct DiagramRecord {
    pub summary: DiagramSummary,
    /// The source document, kept as JSON text so `get_diagram` returns what
    /// the agent sent (normalized) instead of a Rust struct.
    pub document: String,
    /// Whether the diagram is on the plan. Closing it takes it out of the
    /// list and invalidates its id for the tools, but the document stays in
    /// memory until the window closes or `clear_canvas` runs (PRD §11.1).
    pub open: bool,
}

/// How many versions of its source document a diagram keeps (RF14; feature
/// doc `f2-cierre.md`, S3).
///
/// The oldest one is discarded when the cap is reached: the history is a
/// navigation aid, not an archive. The memory is cheap because a version is
/// the JSON text the agent sent, not an IR or a drawing.
pub const MAX_VERSIONS: usize = 20;

/// One version of the source document, without the document itself: what
/// `list_diagram_versions` reports (RF14).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionSummary {
    /// Monotonic number of the version inside its diagram, from `1`. It keeps
    /// counting after the oldest version is discarded, so a number always
    /// names the same document.
    pub version: u64,
    /// The revision of the diagram when this version was written.
    pub revision: u64,
    /// ISO-8601 timestamp of the render or update that created it.
    pub updated_at: String,
    /// Size of the document in bytes.
    pub bytes: usize,
}

/// A version with its document, for `get_diagram_version` (RF14).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagramVersion {
    pub summary: VersionSummary,
    /// The source document, normalized, exactly as `get_diagram` would return
    /// it. Read-only: restoring a version is a render or an update that
    /// creates a new version, never a tool that rewrites the history.
    pub document: String,
}

impl DiagramVersion {
    fn new(summary: &DiagramSummary, document: &str, version: u64) -> Self {
        Self {
            summary: VersionSummary {
                version,
                revision: summary.revision,
                updated_at: summary.updated_at.clone(),
                bytes: document.len(),
            },
            document: document.to_string(),
        }
    }
}

/// A diagram of the store: its public record and its read-only history.
#[derive(Debug)]
struct StoredDiagram {
    record: DiagramRecord,
    /// Oldest first; the last one is the current document (RF14).
    versions: VecDeque<DiagramVersion>,
}

impl StoredDiagram {
    fn new(record: DiagramRecord) -> Self {
        let mut versions = VecDeque::new();
        note_version(&mut versions, &record.summary, &record.document);
        Self { record, versions }
    }

    /// Writes one more version, keeping the cap and discarding the oldest.
    fn push_version(&mut self) {
        note_version(
            &mut self.versions,
            &self.record.summary,
            &self.record.document,
        );
    }
}

/// Appends a version of `document`, numbered after the last one, and keeps at
/// most [`MAX_VERSIONS`].
fn note_version(versions: &mut VecDeque<DiagramVersion>, summary: &DiagramSummary, document: &str) {
    let number = versions.back().map_or(0, |last| last.summary.version) + 1;
    versions.push_back(DiagramVersion::new(summary, document, number));
    while versions.len() > MAX_VERSIONS {
        versions.pop_front();
    }
}

/// The diagrams of the plan.
#[derive(Debug, Default)]
pub struct DiagramStore {
    documents: Mutex<BTreeMap<String, StoredDiagram>>,
    /// Last id handed out. Ids are never reused, so it only grows.
    last_id: AtomicU64,
}

impl DiagramStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocates the next id (`d-0001`, `d-0002`, …), never an id twice.
    pub fn next_id(&self) -> String {
        let number = self.last_id.fetch_add(1, Ordering::SeqCst) + 1;
        format_id(number)
    }

    /// Stores a diagram with its document, stamped with the current UTC time.
    /// The document is the first version of its history (RF14).
    pub fn insert(
        &self,
        diagram_id: String,
        title: String,
        kind: String,
        session_id: String,
        document: String,
    ) -> DiagramSummary {
        // Keep the counter ahead of any id that arrives from outside (restored
        // state) so that a later `next_id` can never hand it out again.
        if let Some(number) = parse_id(&diagram_id) {
            self.last_id.fetch_max(number, Ordering::SeqCst);
        }

        let summary = DiagramSummary {
            diagram_id,
            title,
            kind,
            updated_at: now_iso8601(),
            session_id,
            revision: 1,
            pos: None,
            bbox: None,
        };
        let record = DiagramRecord {
            summary: summary.clone(),
            document,
            open: true,
        };
        self.documents
            .lock()
            .expect("the diagram store is not poisoned")
            .insert(summary.diagram_id.clone(), StoredDiagram::new(record));
        // The session learns which diagrams it has without scanning the store
        // (D4): this is where a diagram starts existing, so every path that
        // creates one — a tool of either transport, a restored plan — is
        // covered.
        crate::session::session_registry().note_diagram(&summary.session_id, &summary.diagram_id);
        summary
    }

    /// Replaces the document of a diagram and grows its revision.
    ///
    /// The new document is one more version of its history (RF14); the oldest
    /// one is discarded past [`MAX_VERSIONS`].
    ///
    /// Returns the new summary, or `None` when the diagram does not exist.
    pub fn replace(
        &self,
        diagram_id: &str,
        title: String,
        document: String,
    ) -> Option<DiagramSummary> {
        let mut documents = self
            .documents
            .lock()
            .expect("the diagram store is not poisoned");
        let stored = documents.get_mut(diagram_id)?;
        stored.record.summary.revision += 1;
        stored.record.summary.updated_at = now_iso8601();
        stored.record.summary.title = title;
        stored.record.document = document;
        // Every accepted update is a version of the source document (RF14).
        stored.push_version();
        Some(stored.record.summary.clone())
    }

    /// Every diagram **on the plan**, ordered by `diagram_id`.
    ///
    /// A closed diagram is not on the plan any more: its document stays in
    /// memory but no tool sees it (PRD §11.1).
    pub fn list(&self) -> Vec<DiagramSummary> {
        self.documents
            .lock()
            .expect("the diagram store is not poisoned")
            .values()
            .filter(|stored| stored.record.open)
            .map(|stored| stored.record.summary.clone())
            .collect()
    }

    /// How many diagrams are on the plan (PRD §11.1, RF13).
    pub fn open_count(&self) -> usize {
        self.documents
            .lock()
            .expect("the diagram store is not poisoned")
            .values()
            .filter(|stored| stored.record.open)
            .count()
    }

    /// Whether the plan is at its cap of open diagrams (PRD §11.1, RF13).
    pub fn is_full(&self) -> bool {
        self.open_count() >= MAX_OPEN_DIAGRAMS
    }

    /// Places a diagram on the plan: its corner and the size of its object.
    pub fn place(
        &self,
        diagram_id: &str,
        pos: gx_core::geom::Point,
        bbox: gx_core::geom::Size,
    ) -> Option<DiagramSummary> {
        let mut documents = self
            .documents
            .lock()
            .expect("the diagram store is not poisoned");
        let stored = documents.get_mut(diagram_id)?;
        stored.record.summary.pos = Some(pos);
        stored.record.summary.bbox = Some(bbox);
        Some(stored.record.summary.clone())
    }

    /// Takes a diagram out of the plan without deleting its document.
    ///
    /// Closing frees the slot, never the identifier: the id stays invalid for
    /// the tools and is never handed out again (PRD §11.1, RF28). It also takes
    /// the diagram out of the list of its session (D4): what the session holds
    /// is what is on the plan.
    pub fn close(&self, diagram_id: &str) -> bool {
        let session_id = {
            let mut documents = self
                .documents
                .lock()
                .expect("the diagram store is not poisoned");
            match documents.get_mut(diagram_id) {
                Some(stored) if stored.record.open => {
                    stored.record.open = false;
                    Some(stored.record.summary.session_id.clone())
                }
                _ => None,
            }
        };
        let Some(session_id) = session_id else {
            return false;
        };
        crate::session::session_registry().forget_diagram(&session_id, diagram_id);
        true
    }

    /// Every diagram of one session, ordered by `diagram_id` (PRD §11.1).
    pub fn list_for_session(&self, session_id: &str) -> Vec<DiagramSummary> {
        self.list()
            .into_iter()
            .filter(|summary| summary.session_id == session_id)
            .collect()
    }

    /// A diagram by id, `None` when it does not exist.
    pub fn get(&self, diagram_id: &str) -> Option<DiagramSummary> {
        self.documents
            .lock()
            .expect("the diagram store is not poisoned")
            .get(diagram_id)
            .filter(|stored| stored.record.open)
            .map(|stored| stored.record.summary.clone())
    }

    /// A diagram with its document.
    pub fn record(&self, diagram_id: &str) -> Option<DiagramRecord> {
        self.documents
            .lock()
            .expect("the diagram store is not poisoned")
            .get(diagram_id)
            .filter(|stored| stored.record.open)
            .map(|stored| stored.record.clone())
    }

    /// The versions of the source document of a diagram, oldest first (RF14).
    ///
    /// `None` when the diagram is not on the plan: a closed diagram is
    /// invisible to the tools, its history included. The documents are not
    /// copied here; `list_diagram_versions` needs their metadata only.
    pub fn versions(&self, diagram_id: &str) -> Option<Vec<VersionSummary>> {
        self.documents
            .lock()
            .expect("the diagram store is not poisoned")
            .get(diagram_id)
            .filter(|stored| stored.record.open)
            .map(|stored| {
                stored
                    .versions
                    .iter()
                    .map(|version| version.summary.clone())
                    .collect()
            })
    }

    /// One version of the source document, with its document (RF14).
    pub fn version(&self, diagram_id: &str, version: u64) -> Option<DiagramVersion> {
        self.documents
            .lock()
            .expect("the diagram store is not poisoned")
            .get(diagram_id)
            .filter(|stored| stored.record.open)
            .and_then(|stored| {
                stored
                    .versions
                    .iter()
                    .find(|candidate| candidate.summary.version == version)
                    .cloned()
            })
    }

    /// The ids currently on the plan, for the `known_ids[]` of the errors.
    pub fn known_ids(&self) -> Vec<String> {
        self.documents
            .lock()
            .expect("the diagram store is not poisoned")
            .values()
            .filter(|stored| stored.record.open)
            .map(|stored| stored.record.summary.diagram_id.clone())
            .collect()
    }

    /// Removes every diagram of one session and returns how many were removed.
    ///
    /// The session forgets them too (D4): a cleared canvas holds no diagram.
    pub fn clear_session(&self, session_id: &str) -> usize {
        let removed: Vec<String> = {
            let mut documents = self
                .documents
                .lock()
                .expect("the diagram store is not poisoned");
            let removed: Vec<String> = documents
                .iter()
                .filter(|(_, stored)| stored.record.summary.session_id == session_id)
                .map(|(diagram_id, _)| diagram_id.clone())
                .collect();
            documents.retain(|_, stored| stored.record.summary.session_id != session_id);
            removed
        };
        let registry = crate::session::session_registry();
        for diagram_id in &removed {
            registry.forget_diagram(session_id, diagram_id);
        }
        removed.len()
    }

    /// Removes every diagram of the plan and returns how many were removed.
    ///
    /// Every session forgets the ones that were its own (D4).
    pub fn clear_all(&self) -> usize {
        let removed: Vec<(String, String)> = {
            let mut documents = self
                .documents
                .lock()
                .expect("the diagram store is not poisoned");
            let removed: Vec<(String, String)> = documents
                .values()
                .map(|stored| {
                    (
                        stored.record.summary.session_id.clone(),
                        stored.record.summary.diagram_id.clone(),
                    )
                })
                .collect();
            documents.clear();
            removed
        };
        let registry = crate::session::session_registry();
        for (session_id, diagram_id) in &removed {
            registry.forget_diagram(session_id, diagram_id);
        }
        removed.len()
    }

    /// Empties the store for a test. Test-only: in production nothing recycles
    /// an id (PRD §11.1).
    #[cfg(test)]
    pub(crate) fn reset_for_tests(&self) {
        self.documents
            .lock()
            .expect("the diagram store is not poisoned")
            .clear();
        self.last_id.store(0, Ordering::SeqCst);
    }
}

/// How many diagrams the plan holds at once (PRD §11.1, RF13: default 12).
pub const MAX_OPEN_DIAGRAMS: usize = 12;

/// The single store of the process.
///
/// Reading the store is also the cheap tick of the end of session (D4, S2):
/// every path that touches the plan — the tools of both transports and the
/// state the panel reads — passes through here, so a session whose grace
/// finished is deleted with its diagrams on the next call, without a thread
/// per session and without a timer. The check is a lock and a scan of the
/// sessions, and it does nothing while no grace has run out.
pub fn diagram_store() -> &'static DiagramStore {
    static STORE: OnceLock<DiagramStore> = OnceLock::new();
    let store = STORE.get_or_init(DiagramStore::new);
    crate::session::end_expired_sessions(
        crate::session::session_registry(),
        store,
        SystemTime::now(),
    );
    store
}

/// `d-0001` style id from its number.
fn format_id(number: u64) -> String {
    format!("d-{number:04}")
}

/// The number behind a `d-0001` style id.
fn parse_id(diagram_id: &str) -> Option<u64> {
    diagram_id
        .strip_prefix("d-")
        .and_then(|rest| rest.parse().ok())
}

/// The current UTC time as ISO-8601.
fn now_iso8601() -> String {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    iso8601_utc(since_epoch.as_secs())
}

/// Formats seconds since the Unix epoch as ISO-8601 UTC.
fn iso8601_utc(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let second_of_day = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        second_of_day / 3600,
        (second_of_day % 3600) / 60,
        second_of_day % 60
    )
}

/// Days since 1970-01-01 to a civil `(year, month, day)` (Howard Hinnant).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_position = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_position + 2) / 5 + 1) as u32;
    let month = if month_position < 10 {
        month_position + 3
    } else {
        month_position - 9
    } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::{iso8601_utc, DiagramStore, MAX_OPEN_DIAGRAMS};

    fn summary(store: &DiagramStore, id: &str) -> super::DiagramSummary {
        store.insert(
            id.to_string(),
            "t".to_string(),
            "graph".to_string(),
            "s".to_string(),
            "{}".to_string(),
        )
    }

    #[test]
    fn ids_are_monotonic_and_never_reused() {
        let store = DiagramStore::new();
        assert_eq!(store.next_id(), "d-0001");
        assert_eq!(store.next_id(), "d-0002");
        assert_eq!(store.next_id(), "d-0003");
        // Allocating does not insert; the next id is still new.
        let inserted = summary(&store, "d-0009");
        assert_eq!(inserted.diagram_id, "d-0009");
        assert_eq!(store.next_id(), "d-0010");
    }

    #[test]
    fn the_store_lists_what_it_inserted() {
        let store = DiagramStore::new();
        summary(&store, "d-0002");
        summary(&store, "d-0001");

        let listed = store.list();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].diagram_id, "d-0001");
        assert_eq!(listed[1].diagram_id, "d-0002");
        assert_eq!(listed[0].kind, "graph");
        assert_eq!(listed[0].session_id, "s");
    }

    #[test]
    fn get_returns_none_for_an_unknown_id() {
        let store = DiagramStore::new();
        summary(&store, "d-0001");

        assert_eq!(
            store.get("d-0001").map(|diagram| diagram.diagram_id),
            Some("d-0001".to_string())
        );
        assert_eq!(store.get("d-0999"), None);
    }

    #[test]
    fn updated_at_is_an_iso_8601_timestamp_in_utc() {
        let store = DiagramStore::new();
        let inserted = summary(&store, "d-0001");

        assert_eq!(inserted.updated_at.len(), 20, "{}", inserted.updated_at);
        assert_eq!(inserted.updated_at.as_bytes()[10], b'T');
        assert!(
            inserted.updated_at.ends_with('Z'),
            "{}",
            inserted.updated_at
        );
    }

    #[test]
    fn the_timestamp_formats_known_instants() {
        assert_eq!(iso8601_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso8601_utc(946_684_800), "2000-01-01T00:00:00Z");
        assert_eq!(iso8601_utc(1_790_000_000), "2026-09-21T14:13:20Z");
    }

    /// PRD §11.1 (RF28): cerrar saca el diagrama del plano e invalida su id,
    /// pero **no** borra el documento: eso no es lo mismo que borrar.
    #[test]
    fn closing_takes_the_diagram_out_of_the_plan_and_keeps_the_document() {
        let store = DiagramStore::new();
        summary(&store, "d-0001");
        assert_eq!(store.open_count(), 1);

        assert!(store.close("d-0001"));

        assert!(store.list().is_empty(), "el panel no lo lista más");
        assert!(store.known_ids().is_empty(), "el id ya no está en el plano");
        assert_eq!(store.get("d-0001"), None);
        assert_eq!(store.record("d-0001"), None);
        // El documento sigue en memoria hasta que se vacíe el plano.
        assert_eq!(store.clear_all(), 1);
        assert!(!store.close("d-0001"), "cerrar dos veces no es un cierre");
    }

    #[test]
    fn a_closed_id_is_never_handed_out_again() {
        let store = DiagramStore::new();
        summary(&store, "d-0001");
        store.close("d-0001");

        assert_eq!(store.next_id(), "d-0002");
    }

    /// PRD §11.1 (RF13): el plano tiene tope, y una vez lleno lo dice.
    #[test]
    fn the_plan_holds_at_most_twelve() {
        let store = DiagramStore::new();
        assert_eq!(MAX_OPEN_DIAGRAMS, 12);
        for _ in 0..MAX_OPEN_DIAGRAMS {
            let id = store.next_id();
            summary(&store, &id);
        }

        assert!(store.is_full());
        assert_eq!(store.open_count(), MAX_OPEN_DIAGRAMS);

        // Cerrar uno libera el slot, no el identificador.
        store.close("d-0003");
        assert!(!store.is_full());
        assert_eq!(store.next_id(), "d-0013");
    }

    /// D4: the session learns which diagrams it has where they start and stop
    /// existing, without scanning the store: the store notes the insert and
    /// forgets the close.
    #[test]
    fn the_store_notes_and_forgets_the_diagrams_of_a_session() {
        let registry = crate::session::session_registry();
        let key = format!("store-note-{}", std::process::id());
        let identity = crate::session::resolve(
            &crate::session::Declared {
                client: Some("pi".to_string()),
                session: Some(key.clone()),
                project: None,
            },
            &crate::session::ClientFingerprint::default(),
            &crate::session::ProcessFingerprint::default(),
        );
        let key = registry.connect("store-note-transport", &identity).id;

        let store = DiagramStore::new();
        store.insert(
            "d-9001".to_string(),
            "t".to_string(),
            "graph".to_string(),
            key.clone(),
            "{}".to_string(),
        );
        assert_eq!(
            registry
                .get(&key)
                .expect("the session is registered")
                .diagram_ids,
            vec!["d-9001".to_string()],
            "a diagram on the plan is a diagram of its session"
        );

        assert!(store.close("d-9001"));
        assert!(
            registry
                .get(&key)
                .expect("the session is registered")
                .diagram_ids
                .is_empty(),
            "a closed diagram is not one of the diagrams of the session"
        );

        store.insert(
            "d-9002".to_string(),
            "t".to_string(),
            "graph".to_string(),
            key.clone(),
            "{}".to_string(),
        );
        assert_eq!(
            store.clear_session(&key),
            2,
            "the closed diagram is still in memory and goes too"
        );
        assert!(
            registry
                .get(&key)
                .expect("the session is registered")
                .diagram_ids
                .is_empty(),
            "a cleared canvas leaves no diagram of the session behind"
        );
    }

    /// PRD §7.2 y §8.6.1: el estado que el panel consume lleva dónde está
    /// cada objeto y cuánto mide.
    #[test]
    fn a_placed_diagram_reports_where_it_is() {
        let store = DiagramStore::new();
        summary(&store, "d-0001");
        assert_eq!(store.get("d-0001").expect("existe").pos, None);

        let pos = gx_core::geom::Point::new(120.0, -40.0);
        let bbox = gx_core::geom::Size::new(320.0, 180.0);
        let placed = store.place("d-0001", pos, bbox).expect("colocado");

        assert_eq!(placed.pos, Some(pos));
        assert_eq!(placed.bbox, Some(bbox));
        assert_eq!(store.get("d-0001").expect("existe").pos, Some(pos));
        assert!(store.place("d-9999", pos, bbox).is_none());
    }

    /// RF14 (S3 of `f2-cierre.md`): cada render y cada update del documento
    /// fuente deja una versión, numerada desde 1 y con su revisión.
    #[test]
    fn every_render_and_every_update_writes_a_version() {
        let store = DiagramStore::new();
        summary(&store, "d-0001");

        let versions = store
            .versions("d-0001")
            .expect("el diagrama está en el plano");
        assert_eq!(versions.len(), 1, "el insert es la primera versión");
        assert_eq!(versions[0].version, 1);
        assert_eq!(versions[0].revision, 1);
        assert_eq!(versions[0].bytes, 2, "`{{}}` mide dos bytes");
        assert_eq!(store.version("d-0001", 1).expect("existe").document, "{}");

        store
            .replace("d-0001", "t2".to_string(), "{\"title\":2}".to_string())
            .expect("el diagrama existe");

        let versions = store.versions("d-0001").expect("sigue en el plano");
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[1].version, 2);
        assert_eq!(versions[1].revision, 2);
        assert_eq!(
            store.version("d-0001", 2).expect("existe").document,
            "{\"title\":2}"
        );
        // La última versión es el documento actual.
        assert_eq!(
            store.record("d-0001").expect("existe").document,
            "{\"title\":2}"
        );
        assert!(
            store.version("d-0001", 3).is_none(),
            "una versión que no existe no se inventa"
        );
    }

    /// RF14: el historial guarda como máximo 20 versiones por diagrama y
    /// descarta la más vieja; la numeración no vuelve atrás, así que un número
    /// siempre nombra el mismo documento.
    #[test]
    fn the_history_keeps_twenty_versions_and_discards_the_oldest() {
        let store = DiagramStore::new();
        summary(&store, "d-0001");
        assert_eq!(super::MAX_VERSIONS, 20);

        for number in 2..=super::MAX_VERSIONS as u64 + 2 {
            store
                .replace("d-0001", "t".to_string(), format!("{{\"n\":{number}}}"))
                .expect("el diagrama existe");
        }

        let versions = store.versions("d-0001").expect("sigue en el plano");
        assert_eq!(versions.len(), super::MAX_VERSIONS);
        assert_eq!(
            versions.first().expect("hay versiones").version,
            3,
            "las dos más viejas se descartaron"
        );
        assert_eq!(
            versions.last().expect("hay versiones").version,
            22,
            "la numeración sigue contando"
        );
        assert!(store.version("d-0001", 2).is_none(), "la vieja se fue");
        assert!(store.version("d-0001", 3).is_some(), "la tercera queda");
        assert_eq!(
            store.version("d-0001", 22).expect("existe").document,
            "{\"n\":22}"
        );
        assert_eq!(store.record("d-0001").expect("existe").summary.revision, 22);
    }

    /// RF14: leer el historial no escribe nada y un diagrama cerrado no deja
    /// ver sus versiones, igual que no deja ver su documento (PRD §11.1).
    #[test]
    fn reading_the_history_changes_nothing_and_a_closed_diagram_hides_it() {
        let store = DiagramStore::new();
        summary(&store, "d-0001");
        store
            .replace("d-0001", "t2".to_string(), "{\"v\":2}".to_string())
            .expect("el diagrama existe");
        let before = store.record("d-0001").expect("existe");

        let _ = store.versions("d-0001");
        let _ = store.version("d-0001", 1);

        let after = store.record("d-0001").expect("existe");
        assert_eq!(before, after, "leer no cambia el documento ni la revisión");

        assert!(store.close("d-0001"));
        assert!(store.versions("d-0001").is_none(), "cerrado no se lista");
        assert!(
            store.version("d-0001", 1).is_none(),
            "cerrado no se lee ni su historial"
        );
    }
}
