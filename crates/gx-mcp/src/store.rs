//! The in-memory model of the diagrams on the plan (PRD §8.6.1, §11.1).
//!
//! The frontend is never the source of truth: the [`DiagramStore`] keeps the
//! summaries and `list_diagrams` reads this same store. The map is a
//! `BTreeMap` (never a `HashMap`) so `list` has a stable, deterministic order
//! (PRD §8.5), and `diagram_id` is monotonic and never reused (PRD §11.1,
//! RF28.1).

use std::collections::BTreeMap;
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

/// The diagrams of the plan.
#[derive(Debug, Default)]
pub struct DiagramStore {
    documents: Mutex<BTreeMap<String, DiagramRecord>>,
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
            .insert(summary.diagram_id.clone(), record);
        summary
    }

    /// Replaces the document of a diagram and grows its revision.
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
        let record = documents.get_mut(diagram_id)?;
        record.summary.revision += 1;
        record.summary.updated_at = now_iso8601();
        record.summary.title = title;
        record.document = document;
        Some(record.summary.clone())
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
            .filter(|record| record.open)
            .map(|record| record.summary.clone())
            .collect()
    }

    /// How many diagrams are on the plan (PRD §11.1, RF13).
    pub fn open_count(&self) -> usize {
        self.documents
            .lock()
            .expect("the diagram store is not poisoned")
            .values()
            .filter(|record| record.open)
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
        let record = documents.get_mut(diagram_id)?;
        record.summary.pos = Some(pos);
        record.summary.bbox = Some(bbox);
        Some(record.summary.clone())
    }

    /// Takes a diagram out of the plan without deleting its document.
    ///
    /// Closing frees the slot, never the identifier: the id stays invalid for
    /// the tools and is never handed out again (PRD §11.1, RF28).
    pub fn close(&self, diagram_id: &str) -> bool {
        let mut documents = self
            .documents
            .lock()
            .expect("the diagram store is not poisoned");
        match documents.get_mut(diagram_id) {
            Some(record) if record.open => {
                record.open = false;
                true
            }
            _ => false,
        }
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
            .filter(|record| record.open)
            .map(|record| record.summary.clone())
    }

    /// A diagram with its document.
    pub fn record(&self, diagram_id: &str) -> Option<DiagramRecord> {
        self.documents
            .lock()
            .expect("the diagram store is not poisoned")
            .get(diagram_id)
            .filter(|record| record.open)
            .cloned()
    }

    /// The ids currently on the plan, for the `known_ids[]` of the errors.
    pub fn known_ids(&self) -> Vec<String> {
        self.documents
            .lock()
            .expect("the diagram store is not poisoned")
            .values()
            .filter(|record| record.open)
            .map(|record| record.summary.diagram_id.clone())
            .collect()
    }

    /// Removes every diagram of one session and returns how many were removed.
    pub fn clear_session(&self, session_id: &str) -> usize {
        let mut documents = self
            .documents
            .lock()
            .expect("the diagram store is not poisoned");
        let before = documents.len();
        documents.retain(|_, record| record.summary.session_id != session_id);
        before - documents.len()
    }

    /// Removes every diagram of the plan and returns how many were removed.
    pub fn clear_all(&self) -> usize {
        let mut documents = self
            .documents
            .lock()
            .expect("the diagram store is not poisoned");
        let before = documents.len();
        documents.clear();
        before
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
}
