//! `gx-index` — the book of the project (feature `f9-libro`, corte C0).
//!
//! The book is a **derived** index of the symbols of one worktree: Tree-sitter
//! prunes the syntax tree down to the top-level declarations and SQLite keeps
//! them, with FTS5 over the names (ideas §2, §10.1). It is never a source of
//! truth: anything that fails the cold start is deleted and rebuilt (D1).
//!
//! This crate is the only place that talks to tree-sitter and to rusqlite
//! (ideas §5, riesgo 2).

pub mod book;
pub mod extract;
pub mod location;
pub mod schema;
pub mod search;
pub mod service;
pub mod watcher;

pub use book::{Book, BookError, Progress};
pub use extract::{ParsedFile, ParsedSymbol};
pub use location::{book_key, BookLocation, BooksRoot, LocationError};
pub use schema::{OpenReport, RebuildReason};
pub use search::{SearchOutcome, SearchRequest, SymbolCandidate, SymbolHit};
pub use service::{BooksConfig, RenderGuard};
pub use watcher::{IndexOptions, IndexSummary, SourceFile};
