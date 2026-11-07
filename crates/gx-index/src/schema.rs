//! The schema v1.1 of the book and the procedure that opens it (ideas §10.1).
//!
//! Three rules drive everything here, and they are not interchangeable:
//!
//! 1. **The book is derived** (D1): any doubt about the file is answered by
//!    deleting it and indexing again, never by repairing it.
//! 2. **The cold start is a `meta` probe**, not an `integrity_check`
//!    (ideas §10.1, «Cold start»): 6–10 minutes on a 10 GB file is not a
//!    startup. `quick_check` runs only after an unclean shutdown.
//! 3. **The pragmas are set in code**, not inherited from whatever SQLite the
//!    build linked: `foreign_keys` is OFF by default and `rusqlite` bundled
//!    does not change that (ideas §10.1, delta 4).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OpenFlags};

/// The version of the schema, mirrored in `PRAGMA user_version` and in
/// `meta.schema_version`. A mismatch is a delete-and-reindex, never a
/// migration ladder (ideas §10).
pub const SCHEMA_VERSION: i64 = 1;

/// The languages the book knows in C0. A `CHECK` instead of a table: the set
/// changes with a release, not with the data (ideas §10.1).
pub const LANGUAGE_PACK: &str = "rust";

/// Page size of the file, fixed before any table and before the probe:
/// the pragma only applies to an empty database (ideas §10.1).
pub const PAGE_SIZE: i64 = 4096;

/// Read/write timeout of every connection, in milliseconds.
pub const BUSY_TIMEOUT_MS: i64 = 5_000;

/// Ceiling of the journal kept next to the book, in bytes.
pub const JOURNAL_SIZE_LIMIT: i64 = 64 * 1024 * 1024;

/// Cache of every connection, in KiB (negative = KiB, not pages).
pub const CACHE_SIZE_KIB: i64 = -65_536;

/// `mmap_size` of the readers only (writers keep the default).
pub const MMAP_SIZE: i64 = 256 * 1024 * 1024;

/// `analysis_limit` of `PRAGMA optimize` at close.
pub const ANALYSIS_LIMIT: i64 = 400;

/// The journal mode chosen by the startup probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalMode {
    /// The root is local: the book can keep a write-ahead log.
    Wal,
    /// The root is remote or synced: no WAL (ideas §8, the mitigation of A).
    Delete,
}

impl JournalMode {
    /// The name of the mode as SQLite spells it.
    pub fn as_str(&self) -> &'static str {
        match self {
            JournalMode::Wal => "wal",
            JournalMode::Delete => "delete",
        }
    }

    /// `synchronous` follows the probe: WAL tolerates NORMAL, a rollback
    /// journal needs FULL (ideas §10.1, «Pragmas»).
    pub fn synchronous(&self) -> &'static str {
        match self {
            JournalMode::Wal => "normal",
            JournalMode::Delete => "full",
        }
    }
}

/// The result of the filesystem probe of a book directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalProbe {
    pub mode: JournalMode,
    /// Why the probe stepped down to `DELETE`, when it did.
    pub reason: Option<String>,
}

/// Decides the journal mode of a book directory.
///
/// WAL is invalid on network filesystems and replicates badly from a folder
/// synced to the cloud (ideas §8, transversal 2). The probe reads the mount
/// type on Linux and falls back to the shape of the path elsewhere; a
/// filesystem it cannot identify is trusted as local, which is the same bet
/// SQLite itself makes.
pub fn probe_journal_mode(books_dir: &Path) -> JournalProbe {
    if let Some(reason) = synced_folder_reason(books_dir) {
        return JournalProbe {
            mode: JournalMode::Delete,
            reason: Some(reason),
        };
    }
    if let Some(fstype) = mount_fstype(books_dir) {
        if is_remote_fstype(&fstype) {
            return JournalProbe {
                mode: JournalMode::Delete,
                reason: Some(format!(
                    "the books root is on `{fstype}`, a remote filesystem"
                )),
            };
        }
    }
    JournalProbe {
        mode: JournalMode::Wal,
        reason: None,
    }
}

/// Whether a filesystem type is remote (NFS, SMB, SSHFS, 9p, FUSE mounts of
/// a network path…). The list is deliberately generous: a false positive
/// costs throughput, a false negative corrupts a WAL.
pub fn is_remote_fstype(fstype: &str) -> bool {
    let fstype = fstype.to_ascii_lowercase();
    matches!(
        fstype.as_str(),
        "nfs"
            | "nfs4"
            | "cifs"
            | "smb"
            | "smbfs"
            | "smb3"
            | "afs"
            | "9p"
            | "davfs"
            | "fuse.sshfs"
            | "fuse.rclone"
            | "fuse.davfs2"
            | "glusterfs"
            | "ceph"
            | "lustre"
            | "beegfs"
            | "ocfs2"
    ) || fstype.starts_with("nfs")
        || fstype.starts_with("fuse.sshfs")
}

/// Why a path looks like a folder synced to the cloud, if it does.
///
/// iCloud Drive, OneDrive, Dropbox, Google Drive and Box replicate a live
/// `-wal` file badly, so they get the rollback journal (ideas §8).
pub fn synced_folder_reason(path: &Path) -> Option<String> {
    const MARKERS: [&str; 7] = [
        "icloud drive",
        "library/mobile documents",
        "onedrive",
        "dropbox",
        "google drive",
        "googledrive",
        "box sync",
    ];
    let lowered = path.to_string_lossy().to_ascii_lowercase();
    MARKERS
        .iter()
        .find(|marker| lowered.contains(**marker))
        .map(|marker| {
            format!(
                "the books root lives in a synced folder (`{marker}`): a live WAL would be replicated"
            )
        })
}

/// The filesystem type of the mount that contains `path`, when it can be
/// read. Linux reads `/proc/mounts`; other platforms answer `None` and the
/// probe falls back to the shape of the path.
pub fn mount_fstype(path: &Path) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let mounts = fs::read_to_string("/proc/mounts").ok()?;
        let mut best: Option<(usize, String)> = None;
        for line in mounts.lines() {
            let mut fields = line.split_whitespace();
            let _device = fields.next()?;
            let mount_point = fields.next()?;
            let fstype = fields.next()?;
            let mount_point = unescape_mount_field(mount_point);
            if canonical.starts_with(&mount_point) {
                let length = mount_point.as_os_str().len();
                if best.as_ref().is_none_or(|(longest, _)| length >= *longest) {
                    best = Some((length, fstype.to_string()));
                }
            }
        }
        best.map(|(_, fstype)| fstype)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = path;
        None
    }
}

/// `/proc/mounts` escapes spaces as `\040` (and a few other bytes).
#[cfg(target_os = "linux")]
fn unescape_mount_field(field: &str) -> PathBuf {
    let mut out = String::with_capacity(field.len());
    let mut chars = field.chars();
    while let Some(char) = chars.next() {
        if char != '\\' {
            out.push(char);
            continue;
        }
        let octal: String = chars.by_ref().take(3).collect();
        match u8::from_str_radix(&octal, 8) {
            Ok(byte) => out.push(byte as char),
            Err(_) => {
                out.push('\\');
                out.push_str(&octal);
            }
        }
    }
    PathBuf::from(out)
}

/// The identity of the book, stored in `meta` and mirrored in the registry.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Identity {
    /// Canonical root of the worktree, with `/` separators.
    pub worktree_root: String,
    /// Absolute git dir, or an empty string when the root is not a repo.
    pub git_dir: String,
    /// `HEAD` at the moment the book was opened.
    pub head_sha: String,
    /// The root commit of the repository: survives a `mv` (ideas §9).
    pub first_root_commit: String,
    /// Unix epoch milliseconds of the creation of this book.
    pub created_at: i64,
    /// Trust-on-first-byte marker: without it the file is not the book.
    pub nonce: String,
    /// uid of the process that created the book.
    pub owner_uid: String,
}

/// Reads the git identity of a root, without failing when there is no git or
/// no repository: a project without git still gets a book (ideas §8-B).
pub fn git_identity(root: &Path) -> (String, String, String) {
    let git_dir = run_git(root, &["rev-parse", "--absolute-git-dir"]).unwrap_or_default();
    let head_sha = run_git(root, &["rev-parse", "HEAD"]).unwrap_or_default();
    let first_root_commit = run_git(root, &["rev-list", "--max-parents=0", "HEAD"])
        .and_then(|out| out.lines().next().map(str::trim).map(str::to_string))
        .unwrap_or_default();
    (git_dir, head_sha, first_root_commit)
}

/// Runs a read-only `git` query in `root`.
fn run_git(root: &Path, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// The random marker of a new book. `RandomState` is seeded by the OS, so two
/// calls do not repeat; no extra dependency is needed for it.
pub fn new_nonce() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut first = std::collections::hash_map::RandomState::new().build_hasher();
    first.write_i64(unix_now_ms());
    first.write_u32(std::process::id());
    let mut second = std::collections::hash_map::RandomState::new().build_hasher();
    second.write_i64(unix_now_ms());
    second.write_u32(std::process::id());
    format!("{:016x}{:016x}", first.finish(), second.finish())
}

/// Unix epoch milliseconds.
pub fn unix_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

/// The uid of the process that owns the book, as text.
pub fn owner_uid() -> String {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        std::env::current_dir()
            .ok()
            .and_then(|dir| fs::metadata(dir).ok())
            .map(|metadata| metadata.uid().to_string())
            .unwrap_or_default()
    }
    #[cfg(not(unix))]
    {
        String::new()
    }
}

/// The outcome of the meta probe of an existing file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeOutcome {
    /// The file is the book of this root and can be opened.
    Ready { unclean: bool },
    /// The file cannot be trusted: delete and index again (D1).
    Rebuild(RebuildReason),
    /// Another process holds the file: it is **not** deleted, because it is a
    /// live book and not a stranger.
    InUse(String),
}

/// Why a book is thrown away instead of opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RebuildReason {
    /// No row in `meta`: a foreign or seeded file.
    MissingMarker,
    /// `meta.nonce` is empty or does not match the registry.
    InvalidNonce(String),
    /// A different root was indexed here.
    RootMismatch { found: String, expected: String },
    /// A different schema.
    SchemaMismatch { found: String, expected: String },
    /// `PRAGMA user_version` is not the one of this build.
    UserVersionMismatch { found: i64, expected: i64 },
    /// `quick_check` did not answer `ok`.
    QuickCheckFailed(String),
    /// The file exists but is not a SQLite database.
    Unreadable(String),
}

impl RebuildReason {
    /// The reason as one line, for the warning and the registry.
    pub fn message(&self) -> String {
        match self {
            RebuildReason::MissingMarker => {
                "the book has no `meta` marker: it is a foreign or seeded file".to_string()
            }
            RebuildReason::InvalidNonce(found) => format!(
                "the book nonce `{found}` is not the one of the registry: it is not this book"
            ),
            RebuildReason::RootMismatch { found, expected } => {
                format!("the book indexes `{found}` and this worktree is `{expected}`")
            }
            RebuildReason::SchemaMismatch { found, expected } => {
                format!("the book schema is `{found}` and this build needs `{expected}`")
            }
            RebuildReason::UserVersionMismatch { found, expected } => {
                format!("the book user_version is {found} and this build needs {expected}")
            }
            RebuildReason::QuickCheckFailed(detail) => {
                format!("quick_check failed after an unclean shutdown: {detail}")
            }
            RebuildReason::Unreadable(detail) => format!("the book is not readable: {detail}"),
        }
    }
}

/// The outcome of opening a book file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenReport {
    /// Whether the file already existed (and was probed) at open time.
    pub probed: bool,
    /// Whether `quick_check` ran (only after an unclean shutdown).
    pub quick_checked: bool,
    /// Whether the file was deleted and created again, and why.
    pub rebuilt: Option<RebuildReason>,
}

/// The pragmas every writer connection of the book gets.
pub fn apply_writer_pragmas(conn: &Connection, journal: JournalMode) -> rusqlite::Result<()> {
    conn.busy_timeout(std::time::Duration::from_millis(BUSY_TIMEOUT_MS as u64))?;
    // Explicit, never inherited: the default is OFF (ideas §10.1, delta 4).
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "journal_mode", journal.as_str())?;
    conn.pragma_update(None, "synchronous", journal.synchronous())?;
    // No automatic checkpoint: the writer decides when the queue is empty.
    conn.pragma_update(None, "wal_autocheckpoint", 0)?;
    conn.pragma_update(None, "journal_size_limit", JOURNAL_SIZE_LIMIT)?;
    conn.pragma_update(None, "cache_size", CACHE_SIZE_KIB)?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    conn.pragma_update(None, "analysis_limit", ANALYSIS_LIMIT)?;
    Ok(())
}

/// The pragmas of a read-only connection (one per lookup thread: a
/// `Connection` is not `Sync`, ideas §10.1).
pub fn apply_reader_pragmas(conn: &Connection) -> rusqlite::Result<()> {
    conn.busy_timeout(std::time::Duration::from_millis(BUSY_TIMEOUT_MS as u64))?;
    conn.pragma_update(None, "query_only", "ON")?;
    conn.pragma_update(None, "cache_size", CACHE_SIZE_KIB)?;
    conn.pragma_update(None, "mmap_size", MMAP_SIZE)?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    Ok(())
}

/// The DDL of schema v1.1 (ideas §10.1), one statement per line group.
///
/// Deltas from the `§10` draft that are visible here and are not style:
/// `search_text` and `name_norm` are real columns of `symbol`,
/// `edge.exact`/`edge.file_id` are gone, `dst_id` is `ON DELETE SET NULL`,
/// `indexed_at` is an integer, and the FTS is kept by triggers — the special
/// `('delete', …)` command is the only single-row operation FTS5 guarantees
/// against external content, so the per-file cycle never writes to `symbol_fts`
/// directly (ideas §10.1, deltas 1, 2, 3, 4, 7, 9).
pub const SCHEMA_SQL: &str = r#"
CREATE TABLE meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE file (
  id           INTEGER PRIMARY KEY,
  path         TEXT NOT NULL UNIQUE,
  lang         TEXT NOT NULL CHECK (lang IN ('rust', 'typescript', 'python', 'go', 'java')),
  content_hash TEXT NOT NULL,
  size         INTEGER NOT NULL CHECK (size >= 0),
  mtime_ns     INTEGER NOT NULL,
  indexed_at   INTEGER NOT NULL
);

CREATE TABLE symbol (
  id           INTEGER PRIMARY KEY,
  file_id      INTEGER NOT NULL REFERENCES file(id) ON DELETE CASCADE,
  parent_id    INTEGER REFERENCES symbol(id),
  kind         TEXT NOT NULL CHECK (kind IN ('fn', 'method', 'type', 'class', 'trait', 'impl', 'module', 'const', 'static')),
  name         TEXT NOT NULL,
  qname        TEXT NOT NULL,
  search_text  TEXT NOT NULL,
  name_norm    TEXT NOT NULL,
  start_line   INTEGER NOT NULL CHECK (start_line >= 1),
  start_col    INTEGER NOT NULL CHECK (start_col >= 1),
  end_line     INTEGER NOT NULL CHECK (end_line >= start_line),
  end_col      INTEGER NOT NULL CHECK (end_col >= 0),
  start_byte   INTEGER NOT NULL CHECK (start_byte >= 0),
  end_byte     INTEGER NOT NULL CHECK (end_byte >= start_byte),
  CHECK (end_line > start_line OR end_col >= start_col),
  UNIQUE (file_id, qname)
);
CREATE INDEX symbol_file      ON symbol(file_id, start_byte);
CREATE INDEX symbol_name      ON symbol(name);
CREATE INDEX symbol_name_norm ON symbol(name_norm);
CREATE INDEX symbol_qname     ON symbol(qname);
CREATE INDEX symbol_parent    ON symbol(parent_id);

CREATE TABLE edge (
  id       INTEGER PRIMARY KEY,
  src_id   INTEGER NOT NULL REFERENCES symbol(id) ON DELETE CASCADE,
  dst_id   INTEGER REFERENCES symbol(id) ON DELETE SET NULL,
  dst_name TEXT NOT NULL,
  kind     TEXT NOT NULL CHECK (kind IN ('calls', 'imports', 'inherits', 'uses', 'type_ref'))
);
CREATE INDEX edge_src      ON edge(src_id, dst_id, kind);
CREATE INDEX edge_dst      ON edge(dst_id);
CREATE INDEX edge_dst_name ON edge(dst_name);

CREATE VIRTUAL TABLE symbol_fts USING fts5(
  name, qname, search_text,
  content='symbol', content_rowid='id',
  tokenize='unicode61'
);

CREATE TRIGGER symbol_fts_ai AFTER INSERT ON symbol BEGIN
  INSERT INTO symbol_fts(rowid, name, qname, search_text)
  VALUES (new.id, new.name, new.qname, new.search_text);
END;

CREATE TRIGGER symbol_fts_ad AFTER DELETE ON symbol BEGIN
  INSERT INTO symbol_fts(symbol_fts, rowid, name, qname, search_text)
  VALUES ('delete', old.id, old.name, old.qname, old.search_text);
END;

CREATE TRIGGER symbol_fts_au AFTER UPDATE ON symbol BEGIN
  INSERT INTO symbol_fts(symbol_fts, rowid, name, qname, search_text)
  VALUES ('delete', old.id, old.name, old.qname, old.search_text);
  INSERT INTO symbol_fts(rowid, name, qname, search_text)
  VALUES (new.id, new.name, new.qname, new.search_text);
END;
"#;

/// Creates a brand-new book file and writes its marker.
///
/// `page_size` and `auto_vacuum` come **before** the tables: both apply to an
/// empty database only, which is why they never appear on the open path
/// (ideas §10.1, «Pragmas»).
pub fn create_database(
    path: &Path,
    journal: JournalMode,
    identity: &Identity,
) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "page_size", PAGE_SIZE)?;
    conn.pragma_update(None, "auto_vacuum", "INCREMENTAL")?;
    apply_writer_pragmas(&conn, journal)?;
    conn.execute_batch(SCHEMA_SQL)?;
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    let mut write = conn.prepare("INSERT INTO meta (key, value) VALUES (?1, ?2)")?;
    let created_at = identity.created_at.to_string();
    for (key, value) in [
        ("schema_version", SCHEMA_VERSION.to_string()),
        ("worktree_root", identity.worktree_root.clone()),
        ("git_dir", identity.git_dir.clone()),
        ("head_sha", identity.head_sha.clone()),
        ("first_root_commit", identity.first_root_commit.clone()),
        ("created_at", created_at),
        ("nonce", identity.nonce.clone()),
        ("owner_uid", identity.owner_uid.clone()),
        ("lang_pack", LANGUAGE_PACK.to_string()),
        // The book is open now: if the process dies before `close`, the next
        // start knows the shutdown was not clean (ideas §10.1).
        ("clean_shutdown", "0".to_string()),
    ] {
        write.execute((key, value))?;
    }
    drop(write);
    Ok(conn)
}

/// Whether an error says the file is held by another connection: the probe
/// answers `InUse` instead of deleting a live book.
pub fn is_busy(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(inner, _)
            if inner.code == rusqlite::ErrorCode::DatabaseBusy
                || inner.code == rusqlite::ErrorCode::DatabaseLocked
    )
}

/// Reads a `meta` value from an open connection.
pub fn read_meta(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
        row.get::<_, String>(0)
    })
    .ok()
}

/// Writes a `meta` value.
pub fn write_meta(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        (key, value),
    )?;
    Ok(())
}

/// The cold start: a `meta` probe over the existing file (ideas §10.1).
///
/// It never runs `integrity_check`. `quick_check` runs only when the previous
/// shutdown was not clean, and the FTS is checked (and rebuilt, not
/// re-parsed) only in that same path. A file held by another process is
/// `InUse`, never a candidate for deletion.
pub fn probe_database(
    path: &Path,
    expected_root: &str,
    expected_nonce: Option<&str>,
) -> (ProbeOutcome, Option<Connection>) {
    let opened = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    );
    let conn = match opened {
        Ok(conn) => conn,
        Err(error) if is_busy(&error) => {
            return (ProbeOutcome::InUse(error.to_string()), None);
        }
        Err(error) => {
            return (
                ProbeOutcome::Rebuild(RebuildReason::Unreadable(error.to_string())),
                None,
            );
        }
    };
    // `page_size` is fixed before any table and before the probe (ideas
    // §10.1): on an existing file the pragma is a no-op, and on the empty file
    // the creation path writes it before the DDL.
    let _ = conn.pragma_update(None, "page_size", PAGE_SIZE);
    if conn
        .busy_timeout(std::time::Duration::from_millis(BUSY_TIMEOUT_MS as u64))
        .is_err()
    {
        return (
            ProbeOutcome::Rebuild(RebuildReason::Unreadable(
                "cannot set the busy timeout".to_string(),
            )),
            None,
        );
    }

    match probe_meta(&conn, expected_root, expected_nonce) {
        Ok(ProbeOutcome::Ready { unclean }) => (ProbeOutcome::Ready { unclean }, Some(conn)),
        Ok(outcome) => (outcome, None),
        Err(error) if is_busy(&error) => (ProbeOutcome::InUse(error.to_string()), None),
        Err(error) => (
            ProbeOutcome::Rebuild(RebuildReason::Unreadable(error.to_string())),
            None,
        ),
    }
}

/// The reads of the probe, kept in one place so a busy file is told apart
/// from a foreign one.
fn probe_meta(
    conn: &Connection,
    expected_root: &str,
    expected_nonce: Option<&str>,
) -> rusqlite::Result<ProbeOutcome> {
    let user_version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if user_version != SCHEMA_VERSION {
        return Ok(ProbeOutcome::Rebuild(RebuildReason::UserVersionMismatch {
            found: user_version,
            expected: SCHEMA_VERSION,
        }));
    }

    let schema_version = read_meta_checked(conn, "schema_version")?;
    let Some(schema_version) = schema_version else {
        return Ok(ProbeOutcome::Rebuild(RebuildReason::MissingMarker));
    };
    if schema_version != SCHEMA_VERSION.to_string() {
        return Ok(ProbeOutcome::Rebuild(RebuildReason::SchemaMismatch {
            found: schema_version,
            expected: SCHEMA_VERSION.to_string(),
        }));
    }

    let nonce = read_meta_checked(conn, "nonce")?.unwrap_or_default();
    let expected_nonce = expected_nonce.unwrap_or_default();
    if nonce.is_empty() || expected_nonce.is_empty() || nonce != expected_nonce {
        return Ok(ProbeOutcome::Rebuild(RebuildReason::InvalidNonce(nonce)));
    }

    let found_root = read_meta_checked(conn, "worktree_root")?.unwrap_or_default();
    if found_root != expected_root {
        return Ok(ProbeOutcome::Rebuild(RebuildReason::RootMismatch {
            found: found_root,
            expected: expected_root.to_string(),
        }));
    }

    let unclean = read_meta_checked(conn, "clean_shutdown")?.as_deref() != Some("1");
    if unclean {
        let result: String = conn.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
        if result != "ok" {
            return Ok(ProbeOutcome::Rebuild(RebuildReason::QuickCheckFailed(
                result,
            )));
        }
        // The FTS is checked against its external content (`1` is required),
        // and rebuilt from `symbol` — no re-parse — before accepting the file.
        if let Err(error) = verify_fts(conn, true) {
            return Ok(ProbeOutcome::Rebuild(RebuildReason::QuickCheckFailed(
                error.to_string(),
            )));
        }
    }
    Ok(ProbeOutcome::Ready { unclean })
}

/// Reads a `meta` value, telling «the row is not there» apart from «the read
/// failed».
pub fn read_meta_checked(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
        row.get::<_, String>(0)
    })
    .map(Some)
    .or_else(|error| match error {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other),
    })
}

/// Runs the FTS integrity check against the external content, optionally
/// rebuilding the index from `symbol` when it is out of sync.
///
/// This is the documented recovery **before** delete-and-reindex
/// (ideas §10.1, delta 9): `rebuild` re-reads `symbol`, so no file is parsed
/// again, and `.recover` does not exist in the bundled SQLite.
pub fn verify_fts(conn: &Connection, rebuild_on_desync: bool) -> rusqlite::Result<()> {
    let check: rusqlite::Result<usize> = conn.execute(
        "INSERT INTO symbol_fts(symbol_fts, rank) VALUES ('integrity-check', 1)",
        [],
    );
    match (check, rebuild_on_desync) {
        (Ok(_), _) => Ok(()),
        (Err(error), false) => Err(error),
        (Err(_), true) => {
            conn.execute("INSERT INTO symbol_fts(symbol_fts) VALUES ('rebuild')", [])?;
            Ok(())
        }
    }
}

/// Deletes every file that belongs to the book, so it can be created again
/// (D1: the recovery is always delete-and-reindex).
pub fn delete_database_files(path: &Path) -> std::io::Result<()> {
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let candidate = PathBuf::from(format!("{}{suffix}", path.display()));
        match fs::remove_file(candidate) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use rusqlite::Connection;

    use super::{
        is_remote_fstype, probe_journal_mode, synced_folder_reason, JournalMode, RebuildReason,
        SCHEMA_SQL, SCHEMA_VERSION,
    };

    /// S3/T2: the versions and the languages of the book are data, not a second
    /// list to keep in sync. Every grammar of the build writes a `lang` the
    /// schema accepts, a language the engine has no grammar for is still
    /// refused, and the schema stayed v1: a book written before the engine of
    /// T2 is read by it and never reindexed (D1 is for real doubts, not for a
    /// refactor).
    #[test]
    fn the_schema_keeps_the_languages_and_the_version_of_the_stored_books() {
        assert_eq!(SCHEMA_VERSION, 1, "T2 must not bump the schema");

        let connection = Connection::open_in_memory().expect("in memory");
        connection.execute_batch(SCHEMA_SQL).expect("the schema");
        for grammar in crate::lang::GRAMMARS {
            let extension = grammar.extensions.first().copied().unwrap_or("txt");
            connection
                .execute(
                    "INSERT INTO file (path, lang, content_hash, size, mtime_ns, indexed_at)
                     VALUES (?1, ?2, 'hash', 0, 0, 0)",
                    (format!("src/one.{extension}"), grammar.lang),
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "the schema must accept a chapter of `{}`: {error}",
                        grammar.lang
                    )
                });
        }

        // A language the engine cannot parse is not a chapter of the book.
        assert!(
            connection
                .execute(
                    "INSERT INTO file (path, lang, content_hash, size, mtime_ns, indexed_at)
                     VALUES ('src/one.rb', 'ruby', 'hash', 0, 0, 0)",
                    [],
                )
                .is_err(),
            "the schema accepted a language the book has no grammar for"
        );
    }

    /// Ideas §8, transversal 2 and D4: WAL is invalid on a network filesystem
    /// and replicates badly from a synced folder, so the probe steps down to
    /// the rollback journal instead of moving the book.
    #[test]
    fn the_journal_probe_keeps_wal_off_remote_and_synced_roots() {
        for remote in ["nfs", "nfs4", "cifs", "smb3", "fuse.sshfs", "9p", "davfs"] {
            assert!(is_remote_fstype(remote), "`{remote}` is remote");
        }
        for local in ["ext4", "btrfs", "tmpfs", "apfs", "xfs", "overlay"] {
            assert!(!is_remote_fstype(local), "`{local}` is local");
        }

        let synced = Path::new("/home/u/Library/Mobile Documents/com~apple~CloudDocs/books");
        assert!(synced_folder_reason(synced).is_some());
        assert!(synced_folder_reason(Path::new("/home/u/OneDrive/books")).is_some());
        assert!(synced_folder_reason(Path::new("/tmp/graphium-books")).is_none());

        // The probe answers the mode and, when it steps down, why.
        let local = probe_journal_mode(&std::env::temp_dir());
        assert_eq!(local.mode, JournalMode::Wal, "{local:?}");
        assert!(local.reason.is_none());

        let fallback = probe_journal_mode(Path::new("/tmp/OneDrive/graphium-books"));
        assert_eq!(fallback.mode, JournalMode::Delete);
        assert!(fallback
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("synced")));
    }

    /// Ideas §10.1: `synchronous` follows the probe, and a book that is thrown
    /// away says why in one line.
    #[test]
    fn the_synchronous_mode_follows_the_journal_mode() {
        assert_eq!(JournalMode::Wal.synchronous(), "normal");
        assert_eq!(JournalMode::Delete.synchronous(), "full");
        let reason = RebuildReason::SchemaMismatch {
            found: "0".to_string(),
            expected: "1".to_string(),
        };
        assert!(reason.message().contains("schema"));
    }
}
