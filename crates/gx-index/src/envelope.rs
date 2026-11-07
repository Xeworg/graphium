//! The character envelope of an answer (F9, T4/C3; S7, L7).
//!
//! The node and edge caps bound the **walk**; the envelope bounds the
//! **answer**, which is what the agent's context window pays for. The ladder
//! is the size of the book — the indexed files — the hard ceiling is 1.5× the
//! rung, and `max_chars` is the caller's own knob over the rung, clamped to the
//! ceiling.
//!
//! The [`Pack`] measures each serialized row **as it is built** and stops at
//! the envelope. The answer is never serialized whole and then cut: the same
//! traversal that ranks the rows decides where the full rows end and the
//! pointer tail begins.

/// The rung of a small book: at most [`SMALL_BOOK_FILES`] indexed files.
pub const LADDER_SMALL: usize = 13_000;

/// The rung of a medium book: at most [`MEDIUM_BOOK_FILES`] indexed files.
pub const LADDER_MEDIUM: usize = 18_000;

/// The rung of a big book: more than [`MEDIUM_BOOK_FILES`] indexed files.
pub const LADDER_LARGE: usize = 24_000;

/// The indexed-file count where the ladder leaves its first rung.
pub const SMALL_BOOK_FILES: i64 = 150;

/// The indexed-file count where the ladder leaves its second rung.
pub const MEDIUM_BOOK_FILES: i64 = 500;

/// The hard ceiling of any answer, as a percentage of the rung (1.5×).
const CEILING_PERCENT: usize = 150;

/// What every answer spends before its first row: the book identity, the
/// project, the seed, the walk's own numbers and the notice. Reserving it
/// keeps the estimate conservative without serializing the answer twice, and
/// it is also the floor of an envelope: no answer can be smaller than this.
pub const ANSWER_OVERHEAD_CHARS: usize = 1_200;

/// The envelope of one answer: the rung of the book it was asked of, the hard
/// ceiling of that rung, and the envelope in effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Envelope {
    /// The indexed files the ladder read.
    pub files: i64,
    /// The rung of the ladder for that many files: the default envelope.
    pub ladder_chars: usize,
    /// The hard ceiling of that rung: 1.5× the rung.
    pub ceiling_chars: usize,
    /// The envelope this answer uses, after `max_chars` and the ceiling.
    pub max_chars: usize,
}

impl Envelope {
    /// The rung of the ladder for a book of `files` indexed chapters.
    pub fn ladder(files: i64) -> usize {
        if files <= SMALL_BOOK_FILES {
            LADDER_SMALL
        } else if files <= MEDIUM_BOOK_FILES {
            LADDER_MEDIUM
        } else {
            LADDER_LARGE
        }
    }

    /// The hard ceiling of a book of `files` chapters: 1.5× its rung.
    pub fn ceiling(files: i64) -> usize {
        Self::ladder(files).saturating_mul(CEILING_PERCENT) / 100
    }

    /// The envelope of one answer: the rung of the book, or `max_chars`
    /// clamped to the ceiling. An override never goes below the fixed overhead
    /// of an answer, because below it no answer exists.
    pub fn resolve(files: i64, requested: Option<usize>) -> Self {
        let ladder_chars = Self::ladder(files);
        let ceiling_chars = Self::ceiling(files);
        let max_chars = requested.map_or(ladder_chars, |asked| {
            asked.clamp(ANSWER_OVERHEAD_CHARS, ceiling_chars)
        });
        Self {
            files,
            ladder_chars,
            ceiling_chars,
            max_chars,
        }
    }

    /// Whether the envelope can still grow: the notice of a budget cut uses it
    /// to propose a wider call instead of a narrower seed.
    pub fn can_grow(&self) -> bool {
        self.max_chars < self.ceiling_chars
    }

    /// The room of this answer, with the fixed overhead already reserved.
    pub fn pack(&self) -> Pack {
        Pack::new(self.max_chars)
    }
}

/// The room one answer has left, measured on the serialized rows as they are
/// built (L7): every row is charged once, with its separator, and the answer
/// stops at the envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pack {
    room: usize,
    spent: usize,
}

impl Pack {
    /// A pack for an answer of at most `max_chars` characters. The fixed
    /// overhead of the answer is reserved first; the rows share the rest.
    pub fn new(max_chars: usize) -> Self {
        Self {
            room: max_chars.saturating_sub(ANSWER_OVERHEAD_CHARS),
            spent: 0,
        }
    }

    /// The characters the rows may spend between all of them.
    pub fn room(&self) -> usize {
        self.room
    }

    /// The characters the rows spent, separators included.
    pub fn spent(&self) -> usize {
        self.spent
    }

    /// The characters the whole answer spends: the fixed overhead plus its
    /// rows. This is the `chars` the notice reports.
    pub fn used(&self) -> usize {
        ANSWER_OVERHEAD_CHARS + self.spent
    }

    /// Charges one row when it still fits under `ceiling` — a share of the
    /// room — and answers whether it did.
    pub fn take_under<T>(&mut self, row: &T, ceiling: usize) -> bool
    where
        T: serde::Serialize + ?Sized,
    {
        let chars = measure(row);
        let limit = ceiling.min(self.room);
        if self.spent.saturating_add(chars).saturating_add(1) > limit {
            return false;
        }
        self.spent = self.spent.saturating_add(chars).saturating_add(1);
        true
    }

    /// Charges one row when the whole room still holds it: the pointer tail
    /// spends what the full rows left.
    pub fn take<T>(&mut self, row: &T) -> bool
    where
        T: serde::Serialize + ?Sized,
    {
        self.take_under(row, self.room)
    }
}

/// The serialized length of one row, in characters: what the agent's window
/// pays for it. A row that cannot be serialized costs the whole room, so it is
/// never packed.
pub fn measure<T>(value: &T) -> usize
where
    T: serde::Serialize + ?Sized,
{
    serde_json::to_string(value)
        .map(|text| text.chars().count())
        .unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::{
        measure, Envelope, Pack, ANSWER_OVERHEAD_CHARS, LADDER_LARGE, LADDER_MEDIUM, LADDER_SMALL,
    };

    /// L7: the ladder is the indexed-file count — 13,000 up to 150 chapters,
    /// 18,000 up to 500, 24,000 beyond — and the ceiling is 1.5× the rung.
    #[test]
    fn the_ladder_switches_by_indexed_file_count() {
        for (files, rung) in [
            (0, LADDER_SMALL),
            (1, LADDER_SMALL),
            (150, LADDER_SMALL),
            (151, LADDER_MEDIUM),
            (500, LADDER_MEDIUM),
            (501, LADDER_LARGE),
            (24_872, LADDER_LARGE),
        ] {
            assert_eq!(Envelope::ladder(files), rung, "{files} files");
        }
        assert_eq!(
            (LADDER_SMALL, LADDER_MEDIUM, LADDER_LARGE),
            (13_000, 18_000, 24_000)
        );
        assert_eq!(Envelope::ceiling(1), 19_500);
        assert_eq!(Envelope::ceiling(151), 27_000);
        assert_eq!(Envelope::ceiling(501), 36_000);
    }

    /// L7: the answer's envelope is the rung, or `max_chars` clamped to the
    /// hard ceiling, and it never goes below the fixed overhead of an answer.
    #[test]
    fn the_envelope_is_the_rung_or_the_override_clamped_to_the_ceiling() {
        let ladder = Envelope::resolve(100, None);
        assert_eq!(ladder.max_chars, LADDER_SMALL);
        assert_eq!(ladder.ladder_chars, LADDER_SMALL);
        assert_eq!(ladder.ceiling_chars, 19_500);
        assert!(ladder.can_grow());

        let tight = Envelope::resolve(100, Some(4_000));
        assert_eq!(tight.max_chars, 4_000);
        assert!(tight.can_grow());

        let huge = Envelope::resolve(100, Some(1_000_000));
        assert_eq!(huge.max_chars, 19_500, "the ceiling of a small book");
        assert!(!huge.can_grow());

        let big = Envelope::resolve(900, Some(36_001));
        assert_eq!(big.max_chars, LADDER_LARGE * 3 / 2);
        assert_eq!(Envelope::resolve(900, Some(25_000)).max_chars, 25_000);

        let floor = Envelope::resolve(100, Some(10));
        assert_eq!(
            floor.max_chars, ANSWER_OVERHEAD_CHARS,
            "no answer is smaller than its own overhead"
        );
    }

    /// L7: the pack reserves the fixed overhead, charges each row with its
    /// separator, and measures characters of the serialized JSON.
    #[test]
    fn the_pack_measures_the_rows_and_reserves_the_answer_overhead() {
        let mut pack = Pack::new(ANSWER_OVERHEAD_CHARS + 13);
        assert_eq!(pack.room(), 13, "the overhead is reserved first");
        assert_eq!(pack.used(), ANSWER_OVERHEAD_CHARS);

        let row = serde_json::json!({ "name": "a" });
        assert_eq!(measure(&row), 12, "{{\"name\":\"a\"}}");
        assert!(pack.take(&row));
        assert_eq!(pack.spent(), 13, "the row and its separator");
        assert_eq!(pack.used(), ANSWER_OVERHEAD_CHARS + 13);

        // A row that no longer fits is not packed, and the pack keeps what it
        // already charged.
        assert!(!pack.take(&row), "a second row does not fit in 13");
        assert_eq!(pack.spent(), 13);
    }

    /// L7: `take_under` is the share of the envelope a kind of row owns; the
    /// pointer tail (`take`) spends what the full rows left.
    #[test]
    fn the_shares_of_the_envelope_are_respected() {
        let mut pack = Pack::new(ANSWER_OVERHEAD_CHARS + 100);
        let row = serde_json::json!({ "name": "a" });
        assert!(pack.take_under(&row, 40));
        assert!(pack.take_under(&row, 40));
        assert!(pack.take_under(&row, 40));
        assert!(
            !pack.take_under(&row, 40),
            "the fourth row would pass the share"
        );
        assert!(pack.take(&row), "the tail still has the rest of the room");
        assert_eq!(pack.spent(), 4 * 13);
    }

    /// L7: the measure is characters, not bytes: an accented label costs one
    /// per character, which is what the window reads.
    #[test]
    fn the_measure_counts_characters_and_not_bytes() {
        let row = serde_json::json!({ "label": "función" });
        let text = serde_json::to_string(&row).expect("the row is JSON");
        assert_eq!(measure(&row), text.chars().count());
        assert!(text.len() > text.chars().count(), "the ñ is two bytes");
    }
}
