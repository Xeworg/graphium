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
/// The book is **on by default** (S5, L5: the human asked for `GRAPHIUM_BOOK=on`
/// to stop being necessary): the owner keeps a book for every project of its
/// sessions, and `GRAPHIUM_BOOK=off` is the explicit opt-out. The flag is read
/// once by the owner and shared by the watcher and the tools, so a project that
/// opted out answers `book_disabled` instead of pretending.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BooksConfig {
    enabled: bool,
    root: Option<PathBuf>,
}

impl BooksConfig {
    /// The configuration of this process, from the environment.
    pub fn from_env() -> Self {
        Self::from_parts(
            std::env::var_os("GRAPHIUM_BOOK"),
            BooksRoot::from_env().map(|root| root.path().to_path_buf()),
        )
    }

    /// The configuration of its two values: the raw flag and the books root.
    ///
    /// This is what [`BooksConfig::from_env`] does, exposed so a test (or a
    /// headless run) can answer for an environment that is not this machine's
    /// without racing the process environment.
    pub fn from_parts(flag: Option<OsString>, root: Option<PathBuf>) -> Self {
        Self {
            enabled: flag_from_value(flag),
            root,
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

/// Whether `GRAPHIUM_BOOK` leaves the book on (S5, L5).
///
/// **Absent means on**: the book starts with the owner, because the human asked
/// for `GRAPHIUM_BOOK=on` to stop being necessary. Only the explicit opt-out
/// turns it off — `off`, `0`, `false`, `no` — and everything else (including an
/// empty value, which is what an unset variable in a config file looks like)
/// keeps the default.
pub fn flag_from_value(raw: Option<OsString>) -> bool {
    let Some(raw) = raw else {
        return true;
    };
    !matches!(
        raw.to_string_lossy().trim().to_ascii_lowercase().as_str(),
        "off" | "0" | "false" | "no"
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
    if book.report().migrated {
        eprintln!(
            "gx-index: relaxed the `lang` check of the book of `{key}` in place: every chapter kept"
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

/// Stops the watcher of **one** book and closes it cleanly, forgetting it.
///
/// The targeted sibling of [`shutdown_all`], and the one a harness that owns a
/// single worktree must use: closing every book of the process from the end of
/// one test tears down the books of the tests that are still answering, and the
/// next tool call reopens one, whose first pass publishes `progress` from
/// scratch — the flake of T4/C3, where the row a tool read said `indexing` with
/// `files_done == 0` for a book that had already been indexed.
///
/// A root with no open book is not an error: the caller says «forget this one»
/// and there is nothing to forget.
pub fn shutdown(project_root: &Path) {
    let Ok(canonical) = std::fs::canonicalize(project_root) else {
        return;
    };
    let key = canonical.to_string_lossy().replace('\\', "/");
    let handle = BOOKS.lock().expect("the books lock").remove(&key);
    if let Some(handle) = handle {
        handle.stop();
        if let Err(error) = handle.book.close_clean() {
            eprintln!(
                "gx-index: the book `{}` did not close cleanly: {error}",
                handle.book.key()
            );
        }
    }
}

/// Stops every watcher, closes every book cleanly and forgets them.
///
/// This is the idle exit of the owner: a `TRUNCATE` checkpoint and the
/// `clean_shutdown` marker, so the next start skips `quick_check`
/// (ideas §10.1, «Cold start»). A test harness that owns one worktree uses
/// [`shutdown`] instead: this one closes the books of the whole process.
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

/// The canonical roots of the books open in this process (T4/C3).
///
/// The idle prune protects every one of them: a book another session is using
/// is never residue, not even over the cap.
pub fn open_roots() -> Vec<PathBuf> {
    BOOKS
        .lock()
        .expect("the books lock")
        .keys()
        .map(PathBuf::from)
        .collect()
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

    /// S5/L5: no `GRAPHIUM_BOOK` at all means **on**; only the explicit opt-out
    /// turns the book off, and an accidental value never does it silently.
    #[test]
    fn the_book_is_on_unless_the_opt_out_is_explicit() {
        assert!(
            flag_from_value(None),
            "an absent GRAPHIUM_BOOK keeps the book on"
        );
        for off in ["off", "OFF", "0", "false", "no", " off "] {
            assert!(
                !flag_from_value(Some(OsString::from(off))),
                "`{off}` is the explicit opt-out"
            );
        }
        for on in ["on", "1", "true", "yes", ""] {
            assert!(
                flag_from_value(Some(OsString::from(on))),
                "`{on}` keeps the book on"
            );
        }
        // The explicit configuration: absent flag and a root answers enabled,
        // the opt-out answers disabled without touching the root.
        let config = BooksConfig::from_parts(None, Some("/tmp/books".into()));
        assert!(config.is_enabled());
        assert!(config.root().is_some());
        let opted_out = BooksConfig::from_parts(Some("off".into()), Some("/tmp/books".into()));
        assert!(!opted_out.is_enabled());
    }

    #[test]
    fn the_disabled_configuration_carries_no_root() {
        let config = BooksConfig::disabled();
        assert!(!config.is_enabled());
        assert!(config.root().is_none());
        assert!(BooksConfig::at("/tmp/books").is_enabled());
    }
}
