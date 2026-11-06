//! Where the book of a worktree lives (S2, D4; ideas §9).
//!
//! `~/.graphium/books/<slug>-<16hex of sha256(canonical root)>/libro.sqlite`,
//! one directory per book so the `-wal`/`-shm` siblings stay contained, plus a
//! registry at `books/index.json` with the nonce that makes the file the book
//! of this root and not a seeded stranger.
//!
//! `GRAPHIUM_BOOKS_ROOT` moves the books root, mirroring the precedent of
//! `GRAPHIUM_EXPORTS_ROOT` (`crates/gx-core/src/export.rs`): the tests and a
//! headless run use it, the agent never chooses it.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The name of the database inside the directory of a book.
pub const DATABASE_NAME: &str = "libro.sqlite";

/// The name of the registry, inside the books root.
pub const REGISTRY_NAME: &str = "index.json";

/// How many hex characters of the root hash name the directory (D4).
const KEY_HEX_LEN: usize = 16;

/// The longest slug that names a book directory.
const SLUG_MAX_LEN: usize = 40;

/// What can go wrong locating a book.
#[derive(Debug, thiserror::Error)]
pub enum LocationError {
    /// The root does not exist or cannot be canonicalized.
    #[error("cannot canonicalize the project root `{path}`: {source}")]
    Root {
        path: String,
        #[source]
        source: io::Error,
    },
    /// There is no home directory and no override.
    #[error("no books root: HOME is not set and GRAPHIUM_BOOKS_ROOT is empty")]
    NoRoot,
    /// The directory of the book cannot be created.
    #[error("cannot create the book directory `{path}`: {source}")]
    Directory {
        path: String,
        #[source]
        source: io::Error,
    },
}

/// The root of every book of the user (D4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BooksRoot {
    path: PathBuf,
}

impl BooksRoot {
    /// The books root of this process: `GRAPHIUM_BOOKS_ROOT` when it is set
    /// and not empty, else `~/.graphium/books`.
    pub fn from_env() -> Option<Self> {
        if let Some(root) = std::env::var_os("GRAPHIUM_BOOKS_ROOT") {
            if !root.is_empty() {
                return Some(Self {
                    path: PathBuf::from(root),
                });
            }
        }
        let home = std::env::var_os("HOME")?;
        Some(Self {
            path: PathBuf::from(home).join(".graphium").join("books"),
        })
    }

    /// The books root at an explicit path (tests and headless runs).
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The path of the books root.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The registry of every book of the user.
    pub fn registry_path(&self) -> PathBuf {
        self.path.join(REGISTRY_NAME)
    }

    /// Every book the registry remembers, with what the disk says about each
    /// one, newest first (`graphium books list`, T4/C3).
    ///
    /// The totals (`files`, `symbols`) are the ones of the last open — the
    /// registry already carries them, so listing never opens a database.
    pub fn list(&self) -> Vec<BookSummary> {
        let registry = BookRegistry::load(&self.registry_path());
        let mut summaries: Vec<BookSummary> = registry
            .books
            .iter()
            .map(|entry| BookSummary {
                entry: entry.clone(),
                bytes: book_bytes(Path::new(&entry.dir)),
                root_missing: entry.root.is_empty() || !Path::new(&entry.root).is_dir(),
                dir_missing: !Path::new(&entry.dir).is_dir(),
            })
            .collect();
        summaries.sort_by(|left, right| newest_first(&left.entry, &right.entry));
        summaries
    }

    /// Removes the books [`prune_candidates`] selects: the directory on disk and
    /// the registry line (T4/C3). With `dry_run` it reports and writes nothing.
    ///
    /// `protected` is never touched, and the active book is always in it: the
    /// idle exit of the owner passes the roots of the books it has open, and the
    /// CLI passes the directory the user is standing in.
    pub fn prune(
        &self,
        protected: &[PathBuf],
        keep_newest: usize,
        dry_run: bool,
    ) -> io::Result<PruneOutcome> {
        let path = self.registry_path();
        let mut registry = BookRegistry::load(&path);
        let removed = prune_candidates(&registry, protected, keep_newest);
        if !dry_run && !removed.is_empty() {
            for candidate in &removed {
                let dir = Path::new(&candidate.entry.dir);
                if dir.is_dir() {
                    fs::remove_dir_all(dir)?;
                }
            }
            registry.books.retain(|entry| {
                !removed
                    .iter()
                    .any(|candidate| candidate.entry.book_key == entry.book_key)
            });
            registry.save(&path)?;
        }
        Ok(PruneOutcome {
            kept: registry.books.len(),
            removed,
            dry_run,
        })
    }

    /// Locates (and creates) the directory of the book of `project_root`.
    ///
    /// The key is the canonical root, so two worktrees of the same repository
    /// are two books and a moved repository gets a new one; the registry keeps
    /// `first_root_commit` so C3 can re-key it instead (ideas §9).
    pub fn locate(&self, project_root: &Path) -> Result<BookLocation, LocationError> {
        let root = fs::canonicalize(project_root).map_err(|source| LocationError::Root {
            path: project_root.display().to_string(),
            source,
        })?;
        let root = normalise_path(&root);
        let key = book_key(&root);
        let dir = self.path.join(&key);
        fs::create_dir_all(&dir).map_err(|source| LocationError::Directory {
            path: dir.display().to_string(),
            source,
        })?;
        Ok(BookLocation {
            root,
            key,
            dir: dir.clone(),
            database: dir.join(DATABASE_NAME),
        })
    }
}

/// One book: the canonical root it indexes and the file that holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookLocation {
    /// Canonical root of the worktree, with `/` separators.
    pub root: PathBuf,
    /// `<slug>-<16hex>`: the name of the directory and the `book_key` of every
    /// answer of the tools (S7).
    pub key: String,
    /// The directory that holds the database and its journal siblings.
    pub dir: PathBuf,
    /// `<dir>/libro.sqlite`.
    pub database: PathBuf,
}

impl BookLocation {
    /// The canonical root as the string stored in `meta` and in the registry.
    pub fn root_string(&self) -> String {
        path_string(&self.root)
    }
}

/// The `book_key` of a canonical root: `<slug>-<16 hex of sha256(root)>`.
pub fn book_key(root: &Path) -> String {
    let canonical = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let root_string = normalise_path(&canonical);
    let mut hasher = Sha256::new();
    hasher.update(path_string(&root_string).as_bytes());
    let digest = hasher.finalize();
    let hex: String = digest
        .iter()
        .flat_map(|byte| format!("{byte:02x}").chars().collect::<Vec<_>>())
        .take(KEY_HEX_LEN)
        .collect();
    format!("{}-{hex}", slug_for(&root_string))
}

/// The human-readable part of the key: the last component of the root, folded
/// to `[a-z0-9-]`. Never empty, so the directory always has a name.
pub fn slug_for(root: &Path) -> String {
    let raw = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "root".to_string());
    let mut slug = String::with_capacity(raw.len());
    for char in raw.chars() {
        if char.is_ascii_alphanumeric() {
            slug.push(char.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-').to_string();
    let slug: String = slug.chars().take(SLUG_MAX_LEN).collect();
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "root".to_string()
    } else {
        slug
    }
}

/// The root with `/` separators and without a trailing slash, so the hash of
/// the same directory is the same on every platform.
fn normalise_path(path: &Path) -> PathBuf {
    PathBuf::from(path_string(path))
}

/// The path as a `/`-separated string (lossy for non-UTF-8 components: the
/// identity has to be stable, and the registry is JSON).
pub fn path_string(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    if text.len() > 1 {
        text.trim_end_matches('/').to_string()
    } else {
        text
    }
}

/// One line of the registry: how to find a book again and how to trust it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookEntry {
    /// `<slug>-<16hex>`.
    pub book_key: String,
    /// Canonical root of the worktree.
    pub root: String,
    /// Directory that holds the database.
    pub dir: String,
    /// Trust-on-first-byte marker of the file (ideas §8-A, D4).
    #[serde(default)]
    pub nonce: String,
    /// The first root commit of the repository, for the re-key of C3.
    #[serde(default)]
    pub first_root_commit: String,
    /// `HEAD` at the last open.
    #[serde(default)]
    pub head_sha: String,
    /// Unix epoch milliseconds.
    #[serde(default)]
    pub created_at: i64,
    /// Unix epoch milliseconds.
    #[serde(default)]
    pub last_opened_at: i64,
    /// Files in the book, at the last open.
    #[serde(default)]
    pub files: i64,
    /// Symbols in the book, at the last open.
    #[serde(default)]
    pub symbols: i64,
}

/// The registry of every book, `books/index.json`.
///
/// A registry that cannot be read is an empty registry: the books it cannot
/// vouch for are rebuilt, which costs a reindex and nothing else (D1).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookRegistry {
    #[serde(default = "registry_version")]
    pub version: u32,
    #[serde(default)]
    pub books: Vec<BookEntry>,
}

fn registry_version() -> u32 {
    1
}

impl BookRegistry {
    /// Loads the registry of a books root, tolerating a missing or broken file.
    pub fn load(path: &Path) -> Self {
        let Ok(text) = fs::read_to_string(path) else {
            return Self {
                version: registry_version(),
                books: Vec::new(),
            };
        };
        let mut registry: Self = serde_json::from_str(&text).unwrap_or_default();
        registry.version = registry_version();
        registry
    }

    /// The entry of a book, by key.
    pub fn entry(&self, key: &str) -> Option<&BookEntry> {
        self.books.iter().find(|entry| entry.book_key == key)
    }

    /// The nonce the registry remembers for a book, when it remembers one.
    pub fn nonce(&self, key: &str) -> Option<String> {
        self.entry(key)
            .map(|entry| entry.nonce.clone())
            .filter(|nonce| !nonce.is_empty())
    }

    /// Adds or replaces the entry of a book.
    pub fn record(&mut self, entry: BookEntry) {
        match self
            .books
            .iter_mut()
            .find(|existing| existing.book_key == entry.book_key)
        {
            Some(existing) => *existing = entry,
            None => self.books.push(entry),
        }
    }

    /// Writes the registry atomically: a temp file next to it and a rename,
    /// so a crash never leaves a half-written JSON.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let temp = parent.join(format!(
            "{}.{}.tmp",
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| REGISTRY_NAME.to_string()),
            std::process::id()
        ));
        let text = serde_json::to_string_pretty(self)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        fs::write(&temp, text)?;
        fs::rename(&temp, path)
    }
}

// ---------------------------------------------------------------------------
// T4/C3 — the books of the user: list, prune, and the cap of the idle exit
// ---------------------------------------------------------------------------

/// How many books the idle exit keeps, the newest by last open first (T4/C3).
///
/// The books root is a cache of worktrees (D1), so a project deleted a week ago
/// is residue; a user with more than eight live projects is not the case this
/// budget is for, and the cap is what keeps the directory from growing with
/// every `git worktree` of a month of afternoons.
pub const KEEP_NEWEST_BOOKS: usize = 8;

/// Why a book is on its way out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PruneReason {
    /// The worktree the book indexed is gone: the book is what is left of it.
    RootGone,
    /// The registry remembers a book whose directory is gone.
    DirGone,
    /// The books root has more live books than the cap keeps.
    OverCap,
}

impl PruneReason {
    /// The reason as the CLI spells it.
    pub fn as_str(&self) -> &'static str {
        match self {
            PruneReason::RootGone => "root-gone",
            PruneReason::DirGone => "dir-gone",
            PruneReason::OverCap => "over-cap",
        }
    }
}

/// One book a prune removes, with the bytes that go with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PruneCandidate {
    /// The registry line that dies with it.
    pub entry: BookEntry,
    /// Why it is on its way out.
    pub reason: PruneReason,
    /// Bytes on disk (`libro.sqlite` and its journal siblings).
    pub bytes: u64,
}

/// One book as the registry and the disk describe it (`graphium books list`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookSummary {
    /// The registry line: key, root, directory and the totals of the last open.
    pub entry: BookEntry,
    /// Bytes on disk.
    pub bytes: u64,
    /// The worktree it indexed no longer exists.
    pub root_missing: bool,
    /// Its directory is gone: there is no database to report.
    pub dir_missing: bool,
}

/// What a prune did (or would do, with `dry_run`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PruneOutcome {
    /// The books that went away, each with its reason and its bytes.
    pub removed: Vec<PruneCandidate>,
    /// Books still in the registry after the prune.
    pub kept: usize,
    /// Whether nothing was deleted and nothing was written.
    pub dry_run: bool,
}

impl PruneOutcome {
    /// The bytes the removed books were holding.
    pub fn bytes_freed(&self) -> u64 {
        self.removed.iter().map(|candidate| candidate.bytes).sum()
    }
}

/// Which books a prune removes, and why (T4/C3; S8, D4).
///
/// Three rules, in this order, and the order is the point:
///
/// 1. **The protected books never die.** A root in `protected` is the book of a
///    session the owner is serving (or the directory a user is standing in):
///    deleting a database another process is writing is corruption, so the
///    protection also stands over the cap.
/// 2. **A book whose reason is not the cap dies.** A registry entry with no
///    directory is a memory of nothing (`DirGone`), and a book whose worktree
///    was deleted has nothing left to answer about (`RootGone`).
/// 3. **The cap keeps the newest [`KEEP_NEWEST_BOOKS`] live books**, ordered by
///    last open (then creation, then key, so the selection is deterministic).
///
/// The selection is a pure function of the registry and the disk so the cap can
/// be pinned by a test instead of inferred from a filesystem after the fact.
pub fn prune_candidates(
    registry: &BookRegistry,
    protected: &[PathBuf],
    keep_newest: usize,
) -> Vec<PruneCandidate> {
    // The protection answers to both identities of a book: the key it is
    // stored under and the root it says it indexes. A registry line whose two
    // disagree is broken, and a broken line is never a reason to delete the
    // book another process has open.
    let protected: Vec<(String, String)> = protected
        .iter()
        .map(|root| {
            let canonical = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
            (book_key(&canonical), path_string(&canonical))
        })
        .collect();
    let mut candidates: Vec<PruneCandidate> = Vec::new();
    let mut live: Vec<&BookEntry> = Vec::new();

    for entry in &registry.books {
        if protected
            .iter()
            .any(|(key, root)| *key == entry.book_key || *root == entry.root)
        {
            continue;
        }
        let dir = Path::new(&entry.dir);
        if !dir.is_dir() {
            candidates.push(PruneCandidate {
                entry: entry.clone(),
                reason: PruneReason::DirGone,
                bytes: 0,
            });
            continue;
        }
        if entry.root.is_empty() || !Path::new(&entry.root).is_dir() {
            candidates.push(PruneCandidate {
                entry: entry.clone(),
                reason: PruneReason::RootGone,
                bytes: book_bytes(dir),
            });
            continue;
        }
        live.push(entry);
    }

    live.sort_by(|left, right| newest_first(left, right));
    for entry in live.into_iter().skip(keep_newest) {
        candidates.push(PruneCandidate {
            entry: entry.clone(),
            reason: PruneReason::OverCap,
            bytes: book_bytes(Path::new(&entry.dir)),
        });
    }
    candidates
}

/// The newest book first: last open, then creation, then key.
fn newest_first(left: &BookEntry, right: &BookEntry) -> std::cmp::Ordering {
    right
        .last_opened_at
        .cmp(&left.last_opened_at)
        .then_with(|| right.created_at.cmp(&left.created_at))
        .then_with(|| left.book_key.cmp(&right.book_key))
}

/// The bytes of one book on disk: the database and its journal siblings.
pub fn book_bytes(dir: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter_map(|entry| entry.metadata().ok())
        .filter(|metadata| metadata.is_file())
        .map(|metadata| metadata.len())
        .sum()
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{book_key, slug_for, BookEntry, BookRegistry};

    /// D4: the slug is readable, folded to `[a-z0-9-]`, and never empty.
    #[test]
    fn the_slug_of_a_root_is_readable_and_filesystem_safe() {
        assert_eq!(slug_for(Path::new("/home/u/my-project")), "my-project");
        assert_eq!(slug_for(Path::new("/home/u/My Project!")), "my-project");
        assert_eq!(slug_for(Path::new("/home/u/GraPhium")), "graphium");
        assert_eq!(slug_for(Path::new("/")), "root");
        assert!(!slug_for(Path::new("/home/u/---")).is_empty());
        assert!(slug_for(Path::new("/home/u/---"))
            .chars()
            .all(|c| c.is_ascii_alphanumeric()));
    }

    /// D4: the key is `<slug>-<16 hex>` and it is stable for the same root.
    #[test]
    fn the_key_is_the_slug_and_sixteen_hex_characters() {
        let key = book_key(Path::new("/home/u/my-project"));
        assert!(key.starts_with("my-project-"), "{key}");
        assert_eq!(key, book_key(Path::new("/home/u/my-project")));
        assert_ne!(key, book_key(Path::new("/home/u/other-project")));
        let hex = key.rsplit('-').next().expect("the hash");
        assert_eq!(hex.len(), 16);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }

    /// D4: the registry remembers the nonce and the identity of each book, and
    /// a registry that cannot be read is empty instead of fatal.
    #[test]
    fn the_registry_round_trips_and_tolerates_a_broken_file() {
        let dir = std::env::temp_dir().join(format!(
            "gx-index-registry-{}-{}",
            std::process::id(),
            super::super::schema::new_nonce()
        ));
        std::fs::create_dir_all(&dir).expect("the dir");
        let path: PathBuf = dir.join("index.json");

        let mut registry = BookRegistry::load(&path);
        assert!(registry.books.is_empty(), "a missing registry is empty");
        registry.record(BookEntry {
            book_key: "project-0123456789abcdef".to_string(),
            root: "/work/project".to_string(),
            dir: "/books/project-0123456789abcdef".to_string(),
            nonce: "n1".to_string(),
            ..BookEntry::default()
        });
        registry.record(BookEntry {
            book_key: "other-fedcba9876543210".to_string(),
            nonce: "n2".to_string(),
            ..BookEntry::default()
        });
        registry.save(&path).expect("the registry is written");

        let loaded = BookRegistry::load(&path);
        assert_eq!(loaded.books.len(), 2);
        assert_eq!(
            loaded.nonce("project-0123456789abcdef").as_deref(),
            Some("n1")
        );
        assert_eq!(
            loaded
                .entry("other-fedcba9876543210")
                .map(|entry| entry.root.as_str()),
            Some("")
        );

        std::fs::write(&path, "{ not json").expect("the broken registry");
        assert!(
            BookRegistry::load(&path).books.is_empty(),
            "a broken registry costs a reindex, never a crash"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod prune_tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::{
        book_bytes, prune_candidates, BookEntry, BookRegistry, BooksRoot, PruneReason,
        KEEP_NEWEST_BOOKS,
    };

    /// A books root of its own, removed when the test ends.
    struct Scratch {
        base: PathBuf,
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let base = std::env::temp_dir().join(format!(
                "gx-index-prune-{name}-{}-{}",
                std::process::id(),
                super::super::schema::new_nonce()
            ));
            fs::create_dir_all(&base).expect("the books root");
            Self { base }
        }

        fn books(&self) -> BooksRoot {
            BooksRoot::at(&self.base)
        }

        /// A live worktree with a book directory that has a database of `bytes`.
        fn book(&self, name: &str, bytes: usize, opened_at: i64) -> BookEntry {
            let root = self.base.join("worktrees").join(name);
            fs::create_dir_all(&root).expect("the worktree");
            let entry = BookEntry {
                book_key: format!("{name}-0123456789abcdef"),
                root: super::path_string(&root),
                dir: String::new(),
                nonce: "n".to_string(),
                last_opened_at: opened_at,
                ..BookEntry::default()
            };
            let dir = self.base.join(&entry.book_key);
            fs::create_dir_all(&dir).expect("the book dir");
            fs::write(dir.join(super::DATABASE_NAME), vec![0u8; bytes]).expect("the database");
            BookEntry {
                dir: super::path_string(&dir),
                ..entry
            }
        }

        /// A registry entry with no directory at all.
        fn registry_only(&self, name: &str) -> BookEntry {
            BookEntry {
                book_key: format!("{name}-fedcba9876543210"),
                root: super::path_string(&self.base.join("worktrees").join(name)),
                dir: super::path_string(&self.base.join(format!("{name}-fedcba9876543210"))),
                ..BookEntry::default()
            }
        }

        fn save(&self, entries: Vec<BookEntry>) -> BookRegistry {
            let mut registry = BookRegistry::default();
            for entry in entries {
                registry.record(entry);
            }
            registry
                .save(&self.books().registry_path())
                .expect("the registry");
            registry
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }

    /// T4/C3 (S8, D4): the prune removes the book of a worktree that no longer
    /// exists — directory and registry entry — and never the book of a live
    /// root, whose bytes stay on disk.
    #[test]
    fn the_prune_removes_a_gone_root_and_keeps_a_live_one() {
        let scratch = Scratch::new("root-gone");
        let live = scratch.book("live", 64, 1);
        let gone = scratch.book("gone", 128, 2);
        // The worktree of `gone` is deleted; the book is what is left of it.
        fs::remove_dir_all(Path::new(&gone.root)).expect("the worktree disappears");
        let registry = scratch.save(vec![live.clone(), gone.clone()]);
        assert_eq!(registry.books.len(), 2);

        let outcome = scratch
            .books()
            .prune(&[], KEEP_NEWEST_BOOKS, false)
            .expect("the prune runs");
        assert_eq!(outcome.removed.len(), 1, "{outcome:?}");
        assert_eq!(outcome.removed[0].entry.book_key, gone.book_key);
        assert_eq!(outcome.removed[0].reason, PruneReason::RootGone);
        assert_eq!(outcome.removed[0].bytes, 128);
        assert!(!Path::new(&gone.dir).exists(), "the book directory is gone");
        assert!(Path::new(&live.dir).exists(), "the live book stays");
        let after = BookRegistry::load(&scratch.books().registry_path());
        assert_eq!(after.books.len(), 1, "the registry lost only the gone book");
        assert_eq!(after.books[0].book_key, live.book_key);
    }

    /// T4/C3: a registry entry whose directory is gone costs no filesystem
    /// work — the entry is what dies.
    #[test]
    fn the_prune_drops_a_registry_entry_whose_directory_is_gone() {
        let scratch = Scratch::new("dir-gone");
        let live = scratch.book("live", 64, 1);
        let orphan = scratch.registry_only("orphan");
        assert!(!Path::new(&orphan.dir).exists());
        scratch.save(vec![live.clone(), orphan.clone()]);

        let outcome = scratch
            .books()
            .prune(&[], KEEP_NEWEST_BOOKS, false)
            .expect("the prune runs");
        assert_eq!(outcome.removed.len(), 1, "{outcome:?}");
        assert_eq!(outcome.removed[0].entry.book_key, orphan.book_key);
        assert_eq!(outcome.removed[0].reason, PruneReason::DirGone);
        assert_eq!(outcome.removed[0].bytes, 0);
        assert!(Path::new(&live.dir).exists());
        let after = BookRegistry::load(&scratch.books().registry_path());
        assert_eq!(after.books.len(), 1);
        assert_eq!(after.books[0].book_key, live.book_key);
    }

    /// T4/C3: the cap keeps the `keep_newest` most recently opened books and
    /// the protected book of the owner is never one of the removed ones, even
    /// when it is the oldest of all.
    #[test]
    fn the_cap_keeps_the_newest_books_and_never_the_protected_one() {
        let scratch = Scratch::new("cap");
        let mut entries: Vec<BookEntry> = (0..10)
            .map(|index| scratch.book(&format!("b{index}"), 10, index as i64))
            .collect();
        // The oldest book is the one the owner has open right now.
        let protected = PathBuf::from(&entries[0].root);
        entries.sort_by_key(|entry| entry.last_opened_at);
        scratch.save(entries.clone());

        let candidates = prune_candidates(
            &BookRegistry::load(&scratch.books().registry_path()),
            std::slice::from_ref(&protected),
            KEEP_NEWEST_BOOKS,
        );
        assert_eq!(
            candidates.len(),
            1,
            "nine live books over a cap of eight remove one: {candidates:?}"
        );
        assert_eq!(candidates[0].reason, PruneReason::OverCap);
        assert_eq!(
            candidates[0].entry.book_key, entries[1].book_key,
            "the oldest book that is not protected is the one over the cap"
        );
        assert!(
            candidates
                .iter()
                .all(|candidate| candidate.entry.root != super::path_string(&protected)),
            "the protected book is never a candidate"
        );
        // And the same selection through the prune: the oldest book survives.
        scratch
            .books()
            .prune(std::slice::from_ref(&protected), KEEP_NEWEST_BOOKS, false)
            .expect("the prune runs");
        let after = BookRegistry::load(&scratch.books().registry_path());
        assert_eq!(after.books.len(), 9);
        assert!(
            after
                .books
                .iter()
                .any(|entry| entry.root == super::path_string(&protected)),
            "the book of the owner survived the cap"
        );
        assert!(
            !Path::new(&entries[1].dir).exists(),
            "the book over the cap was deleted"
        );
    }

    /// T4/C3: `--dry-run` reports what it would remove and changes nothing.
    #[test]
    fn a_dry_run_reports_without_deleting_anything() {
        let scratch = Scratch::new("dry-run");
        let live = scratch.book("live", 64, 1);
        let gone = scratch.book("gone", 256, 2);
        fs::remove_dir_all(Path::new(&gone.root)).expect("the worktree disappears");
        scratch.save(vec![live.clone(), gone.clone()]);

        let outcome = scratch
            .books()
            .prune(&[], KEEP_NEWEST_BOOKS, true)
            .expect("the dry run");
        assert!(outcome.dry_run);
        assert_eq!(outcome.removed.len(), 1);
        assert_eq!(outcome.bytes_freed(), 256);
        assert!(
            Path::new(&gone.dir).exists(),
            "a dry run deletes no directory"
        );
        let after = BookRegistry::load(&scratch.books().registry_path());
        assert_eq!(after.books.len(), 2, "a dry run rewrites no registry");
    }

    /// T4/C3 (`graphium books list`): the list reports the bytes of each book,
    /// the totals of its last open, and marks the books whose root is gone.
    #[test]
    fn the_list_reports_the_bytes_the_totals_and_the_missing_roots() {
        let scratch = Scratch::new("list");
        let live = BookEntry {
            files: 95,
            symbols: 2472,
            last_opened_at: 1_700_000_000_000,
            ..scratch.book("live", 96, 1)
        };
        let gone = scratch.book("gone", 32, 2);
        fs::remove_dir_all(Path::new(&gone.root)).expect("the worktree disappears");
        scratch.save(vec![live.clone(), gone.clone()]);

        let books = scratch.books().list();
        assert_eq!(books.len(), 2);
        let live_summary = books
            .iter()
            .find(|summary| summary.entry.book_key == live.book_key)
            .expect("the live book");
        assert_eq!(live_summary.bytes, 96);
        assert!(!live_summary.root_missing);
        assert!(!live_summary.dir_missing);
        assert_eq!(live_summary.entry.files, 95);
        assert_eq!(live_summary.entry.symbols, 2472);
        let gone_summary = books
            .iter()
            .find(|summary| summary.entry.book_key == gone.book_key)
            .expect("the stale book");
        assert!(gone_summary.root_missing, "a deleted worktree is marked");
        assert!(!gone_summary.dir_missing);
        assert_eq!(book_bytes(Path::new(&gone.dir)), 32);
    }
}
