//! Why an answer is a slice of a bigger one (F9, T5; S7, L7).
//!
//! The book never answers an unbounded list: `code_structure`, `code_impact`
//! and `code_diagram` answer a summary and say so. This module is the one
//! shape of that notice — the reason, what the answer shows and how many items
//! the same question has — so the three tools cannot invent three vocabularies
//! for the same fact. The follow-up call (`next`) is composed by the tool,
//! which is the only layer that knows its own arguments.
//!
//! A `total` of `None` is honest: the hop cap cuts a walk whose deeper answer
//! is not knowable without asking again, so the book says «no sé» instead of
//! inventing a number.

/// Why the answer stopped short of the complete one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutReason {
    /// The list was longer than the `limit` of the answer.
    Nodes,
    /// The walk had more edges than its edge budget.
    Edges,
    /// The caller asked for more hops than the book follows, and the walk
    /// still had somewhere to go.
    Depth,
    /// The answer is the outline: the children of the declarations were left
    /// for the follow-up call.
    Collapsed,
    /// The document was trimmed to the density its kind accepts, so it
    /// renders without a repair.
    Density,
}

impl CutReason {
    /// The reason as the tool spells it (`truncated.reason`).
    pub fn as_str(&self) -> &'static str {
        match self {
            CutReason::Nodes => "nodes",
            CutReason::Edges => "edges",
            CutReason::Depth => "depth",
            CutReason::Collapsed => "collapsed",
            CutReason::Density => "density",
        }
    }
}

/// The machine-readable notice of a cut: `{reason, shown, total, next}` at the
/// edge of the tool, where `next` is the exact follow-up call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Truncation {
    pub reason: CutReason,
    /// How many items the answer carries (nodes, edges or rows).
    pub shown: usize,
    /// How many the same question has, when that number is knowable.
    pub total: Option<usize>,
}

impl Truncation {
    pub fn new(reason: CutReason, shown: usize, total: Option<usize>) -> Self {
        Self {
            reason,
            shown,
            total,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CutReason, Truncation};

    /// T5: the five reasons are the vocabulary the three tools share, and the
    /// notice keeps the three numbers the agent branches on.
    #[test]
    fn the_reasons_are_the_five_the_tools_speak() {
        for (reason, text) in [
            (CutReason::Nodes, "nodes"),
            (CutReason::Edges, "edges"),
            (CutReason::Depth, "depth"),
            (CutReason::Collapsed, "collapsed"),
            (CutReason::Density, "density"),
        ] {
            assert_eq!(reason.as_str(), text);
            assert_eq!(Truncation::new(reason, 1, Some(2)).reason, reason);
        }
        assert_eq!(Truncation::new(CutReason::Depth, 4, None).total, None);
    }
}
