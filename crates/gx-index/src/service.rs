//! The books of the process: one per canonical worktree root (D4).
//!
//! The owner is the single writer (ideas §3, «adopta 6»): a book is opened
//! once, its watcher is started once, and every tool call reads through it.
//! Nothing here reads the environment twice or guesses a root: the caller
//! hands the project of the session (D4: the session record, never
//! `current_dir()`).

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub use crate::watcher::RenderGuard;
use crate::watcher::{IndexOptions, Indexer, WatcherHandle};

use crate::book::{Book, BookError};
use crate::location::BooksRoot;

/// Where the books live and whether the feature is on.
///
/// The flag is `GRAPHIUM_BOOK=on`, read once by the owner and shared by the
/// watcher and the tool: a project without the flag gets no watcher and the
/// tool answers `book_disabled` instead of pretending.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BooksConfig {
    enabled: bool,
    root: Option<PathBuf>,
}

impl BooksConfig {
    /// The configuration of this process, from the environment.
    pub fn from_env() -> Self {
        Self {
            enabled: flag_from_value(std::env::var_os("GRAPHIUM_BOOK")),
            root: BooksRoot::from_env().map(|root| root.path().to_path_buf()),
        }
    }

    /// The production configuration at an explicit books root (tests and a
    /// headless run).
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self {
            enabled: true,
            root: Some(root.into()),
        }
    }

    /// The configuration with the feature off: the disabled path.
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            root: None,
        }
    }

    /// Whether the book is on.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// The books root, when there is one.
    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// The root as a [`BooksRoot`].
    fn books_root(&self) -> Option<BooksRoot> {
        self.root.clone().map(BooksRoot::at)
    }
}

/// The values of `GRAPHIUM_BOOK` that turn the book on. Everything else is
/// off, including an unset variable: the tools stay invisible to a project
/// that did not ask for them (S5, condition 4 of P3).
pub fn flag_from_value(raw: Option<OsString>) -> bool {
    let Some(raw) = raw else {
        return false;
    };
    matches!(
        raw.to_string_lossy().trim().to_ascii_lowercase().as_str(),
        "on" | "1" | "true" | "yes"
    )
}

/// One open book and the thread that keeps it current.
#[derive(Debug)]
pub struct BookHandle {
    book: Arc<Book>,
    indexer: Arc<Indexer>,
    watcher: Mutex<Option<WatcherHandle>>,
}

impl BookHandle {
    /// The book, for the searches and the answers.
    pub fn book(&self) -> &Arc<Book> {
        &self.book
    }

    /// The indexer, for a synchronous pass.
    pub fn indexer(&self) -> &Arc<Indexer> {
        &self.indexer
    }

    /// Stops the watcher thread (idempotent).
    pub fn stop(&self) {
        let watcher = self.watcher.lock().expect("the watcher handle lock").take();
        if let Some(watcher) = watcher {
            watcher.stop();
        }
    }
}

/// Every book of the process, by canonical root.
static BOOKS: Mutex<BTreeMap<String, Arc<BookHandle>>> = Mutex::new(BTreeMap::new());

/// Opens the book of `project_root`, or answers the one already open.
///
/// The first pass runs inside the watcher thread, so this call returns as soon
/// as the file is probed (a few milliseconds); the answer a tool gets while it
/// runs carries `progress.state == "indexing"`.
pub fn ensure(config: &BooksConfig, project_root: &Path) -> Result<Arc<BookHandle>, BookError> {
    if !config.enabled {
        return Err(BookError::Disabled);
    }
    let books_root = config.books_root().ok_or(BookError::NoRoot)?;
    let canonical = std::fs::canonicalize(project_root)?;
    let key = canonical.to_string_lossy().replace('\\', "/");
    {
        let books = BOOKS.lock().expect("the books lock");
        if let Some(handle) = books.get(&key) {
            return Ok(handle.clone());
        }
    }

    let location = books_root.locate(&canonical)?;
    let book = Arc::new(Book::open(location, &books_root.registry_path())?);
    if let Some(reason) = book.report().rebuilt.clone() {
        eprintln!(
            "gx-index: rebuilt the book of `{key}`: {}",
            Book::rebuild_message(&reason)
        );
    }
    let indexer = Arc::new(Indexer::new(
        book.clone(),
        canonical.clone(),
        IndexOptions::default(),
    ));
    let watcher = indexer.clone().start()?;
    let handle = Arc::new(BookHandle {
        book,
        indexer,
        watcher: Mutex::new(Some(watcher)),
    });

    let mut books = BOOKS.lock().expect("the books lock");
    // Another thread may have opened the same book while this one was
    // probing: the first one wins, so there is one watcher per root.
    if let Some(existing) = books.get(&key) {
        handle.stop();
        return Ok(existing.clone());
    }
    books.insert(key, handle.clone());
    Ok(handle)
}

/// Keeps a book running for every project of the open sessions (S9).
///
/// Called from the poll loop of the owner: opening a book is idempotent, so
/// this is a map lookup for every project that already has one. A project
/// that cannot be indexed is reported once, never on every poll.
pub fn maintain(config: &BooksConfig, projects: &[String]) {
    if !config.enabled {
        return;
    }
    for project in projects {
        if !Path::new(project).is_dir() {
            continue;
        }
        if let Err(error) = ensure(config, Path::new(project)) {
            let mut reported = REPORTED.lock().expect("the reported lock");
            if reported.insert(project.clone()) {
                eprintln!("gx-index: no book for `{project}`: {error}");
            }
        }
    }
}

/// The projects whose failure was already reported, so the poll loop does not
/// repeat it.
static REPORTED: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// Stops every watcher, closes every book cleanly and forgets them.
///
/// This is the idle exit of the owner: a `TRUNCATE` checkpoint and the
/// `clean_shutdown` marker, so the next start skips `quick_check`
/// (ideas §10.1, «Cold start»).
pub fn shutdown_all() {
    let handles: Vec<Arc<BookHandle>> = {
        let mut books = BOOKS.lock().expect("the books lock");
        std::mem::take(&mut *books).into_values().collect()
    };
    for handle in handles {
        handle.stop();
        if let Err(error) = handle.book.close_clean() {
            eprintln!(
                "gx-index: the book `{}` did not close cleanly: {error}",
                handle.book.key()
            );
        }
    }
}

/// Whether a book is open for a root (tests and maintenance).
pub fn is_open(project_root: &Path) -> bool {
    let Ok(canonical) = std::fs::canonicalize(project_root) else {
        return false;
    };
    let key = canonical.to_string_lossy().replace('\\', "/");
    BOOKS.lock().expect("the books lock").contains_key(&key)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::{flag_from_value, BooksConfig};

    /// The switch is `GRAPHIUM_BOOK=on`; "off", an empty value and no value
    /// are all the disabled path (S5, condition 4 of P3).
    #[test]
    fn the_flag_is_on_only_for_the_documented_values() {
        for on in ["on", "ON", "1", "true", "yes", " on "] {
            assert!(
                flag_from_value(Some(OsString::from(on))),
                "`{on}` has to turn the book on"
            );
        }
        for off in ["off", "0", "false", "", " ", "maybe"] {
            assert!(
                !flag_from_value(Some(OsString::from(off))),
                "`{off}` has to leave the book off"
            );
        }
        assert!(!flag_from_value(None));
    }

    #[test]
    fn the_disabled_configuration_carries_no_root() {
        let config = BooksConfig::disabled();
        assert!(!config.is_enabled());
        assert!(config.root().is_none());
        assert!(BooksConfig::at("/tmp/books").is_enabled());
    }
}
