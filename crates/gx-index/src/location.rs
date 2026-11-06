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
