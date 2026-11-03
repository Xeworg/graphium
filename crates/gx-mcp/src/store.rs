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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagramSummary {
    pub diagram_id: String,
    pub title: String,
    pub kind: String,
    /// ISO-8601 timestamp in UTC, e.g. `2026-11-03T12:34:56Z`.
    pub updated_at: String,
    pub session_id: String,
}

/// The diagrams of the plan.
#[derive(Debug, Default)]
pub struct DiagramStore {
    diagrams: Mutex<BTreeMap<String, DiagramSummary>>,
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

    /// Stores a summary stamped with the current UTC time.
    pub fn insert(
        &self,
        diagram_id: String,
        title: String,
        kind: String,
        session_id: String,
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
        };
        self.diagrams
            .lock()
            .expect("the diagram store is not poisoned")
            .insert(summary.diagram_id.clone(), summary.clone());
        summary
    }

    /// Every diagram on the plan, ordered by `diagram_id`.
    pub fn list(&self) -> Vec<DiagramSummary> {
        self.diagrams
            .lock()
            .expect("the diagram store is not poisoned")
            .values()
            .cloned()
            .collect()
    }

    /// A diagram by id, `None` when it does not exist.
    pub fn get(&self, diagram_id: &str) -> Option<DiagramSummary> {
        self.diagrams
            .lock()
            .expect("the diagram store is not poisoned")
            .get(diagram_id)
            .cloned()
    }

    /// Empties the store for a test. Test-only: in production nothing recycles
    /// an id (PRD §11.1).
    #[cfg(test)]
    pub(crate) fn reset_for_tests(&self) {
        self.diagrams
            .lock()
            .expect("the diagram store is not poisoned")
            .clear();
        self.last_id.store(0, Ordering::SeqCst);
    }
}

/// The single store of the process.
pub fn diagram_store() -> &'static DiagramStore {
    static STORE: OnceLock<DiagramStore> = OnceLock::new();
    STORE.get_or_init(DiagramStore::new)
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
    use super::{iso8601_utc, DiagramStore};

    fn summary(store: &DiagramStore, id: &str) -> super::DiagramSummary {
        store.insert(
            id.to_string(),
            "t".to_string(),
            "graph".to_string(),
            "s".to_string(),
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
}
