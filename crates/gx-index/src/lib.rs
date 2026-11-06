//! `gx-index` — the book of the project (feature `f9-libro`, cortes C0 y C1).
//!
//! The book is a **derived** index of the symbols of one worktree: the grammars
//! of [`lang`] prune the syntax tree down to the declarations, and SQLite keeps
//! them with FTS5 over the names (ideas §2, §10.1). C1 adds the connections:
//! four **syntactic** relation kinds (`calls`, `imports`, `inherits`, `uses`,
//! D3) and the bounded walk that answers them. It is never a source of truth:
//! anything that fails the cold start is deleted and rebuilt (D1).
//!
//! This crate is the only place that talks to tree-sitter — through [`lang`],
//! one module per language (S3, L5) — and to rusqlite (ideas §5, riesgo 2).

pub mod book;
pub mod cut;
pub mod envelope;
pub mod impact;
pub mod lang;
pub mod location;
pub mod relations;
pub mod resolution;
pub mod schema;
pub mod search;
pub mod service;
pub mod structure;
pub mod watcher;

pub use book::{Book, BookError, Progress};
pub use cut::{CutReason, Truncation};
pub use envelope::{Envelope, Pack, ANSWER_OVERHEAD_CHARS};
pub use impact::{
    Direction, FileImpact, FileImpactRequest, FileLink, Impact, ImpactEdge, ImpactNode,
    ImpactRequest, Seed, WalkLimits,
};
pub use lang::{ParsedFile, ParsedSymbol};
pub use location::{
    book_bytes, book_key, prune_candidates, BookLocation, BookSummary, BooksRoot, LocationError,
    PruneCandidate, PruneOutcome, PruneReason, KEEP_NEWEST_BOOKS,
};
pub use relations::{ParsedEdge, RELATION_KINDS};
pub use resolution::{resolution_by_language, LanguageResolution};
pub use schema::{OpenReport, RebuildReason};
pub use search::{SearchOutcome, SearchRequest, SymbolCandidate, SymbolHit};
pub use service::{BooksConfig, RenderGuard};
pub use structure::{Collapsed, FileStructure, StructureAnswer, StructureRequest, StructureSymbol};
pub use watcher::{IndexOptions, IndexSummary, SourceFile};
