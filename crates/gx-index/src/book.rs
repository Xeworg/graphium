//! The book: one SQLite file per worktree, written by one thread at a time.
//!
//! The write cycle of a chapter is **delete → insert**, never `REPLACE`
//! (ideas §10.1, delta 9): with external content, FTS5 reads the values to
//! forget from `symbol`, and the `('delete', old.id, …)` command of the
//! trigger is the only single-row operation it guarantees. The chapter is
//! deleted from `symbol` **before** the `file` row, so the trigger always has
//! its old values, and the file row goes last.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension};

use crate::extract::ParsedFile;
use crate::location::{path_string, BookEntry, BookLocation, BookRegistry};
use crate::schema::{self, Identity, JournalMode, OpenReport, RebuildReason};
use crate::search::{self, SearchOutcome, SearchRequest};

/// What can go wrong opening or writing the book.
#[derive(Debug, thiserror::Error)]
pub enum BookError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Location(#[from] crate::location::LocationError),
    /// `GRAPHIUM_BOOK` is not on: the book is disabled (S5).
    #[error("the book is disabled: GRAPHIUM_BOOK is not `on`")]
    Disabled,
    /// There is no home directory and no books root override.
    #[error("no books root: HOME is not set and GRAPHIUM_BOOKS_ROOT is empty")]
    NoRoot,
    /// Another process holds the book: it is live and is not deleted.
    #[error("the book is in use by another process: {0}")]
    InUse(String),
}

/// How far the first index is (the `meta.progress` row of S9).
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Progress {
    /// `indexing`, `indexed` or `idle`.
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub files_total: i64,
    #[serde(default)]
    pub files_done: i64,
    #[serde(default)]
    pub files_failed: i64,
    #[serde(default)]
    pub symbols: i64,
    /// Unix epoch milliseconds.
    #[serde(default)]
    pub started_at: i64,
    /// Unix epoch milliseconds.
    #[serde(default)]
    pub updated_at: i64,
    /// Files skipped as binary or oversized (D2).
    #[serde(default)]
    pub warnings: i64,
}

impl Progress {
    /// Whether the first pass is still running.
    pub fn is_indexing(&self) -> bool {
        self.state == "indexing"
    }
}

/// What a batch did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommitReport {
    /// Files whose symbols were replaced.
    pub files_written: usize,
    /// Files whose content hash did not change.
    pub files_skipped: usize,
    /// Symbols written.
    pub symbols: i64,
    /// Chapters removed because the file is gone.
    pub files_removed: usize,
}

/// The identity and the state of one open book.
#[derive(Debug)]
pub struct Book {
    location: BookLocation,
    registry_path: std::path::PathBuf,
    journal: JournalMode,
    writer: Mutex<Connection>,
    report: OpenReport,
}

impl Book {
    /// Opens (or creates) the book of `location`, with the cold-start probe.
    ///
    /// Any rejection of the file is a delete-and-reindex (D1): the reason is
    /// in [`OpenReport::rebuilt`] so the watcher can log it once.
    pub fn open(location: BookLocation, registry_path: &Path) -> Result<Self, BookError> {
        let journal_probe = schema::probe_journal_mode(&location.dir);
        let journal = journal_probe.mode;
        let mut registry = BookRegistry::load(registry_path);
        let root = location.root_string();
        let mut report = OpenReport {
            probed: false,
            quick_checked: false,
            rebuilt: None,
        };

        let connection = if location.database.exists() {
            report.probed = true;
            let (outcome, existing) = schema::probe_database(
                &location.database,
                &root,
                registry.nonce(&location.key).as_deref(),
            );
            match outcome {
                schema::ProbeOutcome::Ready { unclean } => {
                    report.quick_checked = unclean;
                    existing.expect("a ready probe hands the connection")
                }
                schema::ProbeOutcome::Rebuild(reason) => {
                    report.rebuilt = Some(reason);
                    drop(existing);
                    schema::delete_database_files(&location.database)?;
                    schema::create_database(
                        &location.database,
                        journal,
                        &fresh_identity(&location),
                    )?
                }
                schema::ProbeOutcome::InUse(detail) => {
                    return Err(BookError::InUse(detail));
                }
            }
        } else {
            schema::create_database(&location.database, journal, &fresh_identity(&location))?
        };

        schema::apply_writer_pragmas(&connection, journal)?;
        // The book is open: a process that dies from here on did not shut down
        // cleanly, and the next start runs `quick_check` (ideas §10.1).
        schema::write_meta(&connection, "clean_shutdown", "0")?;
        refresh_identity(&connection, &location)?;
        // Before the first pass starts there is a row to read: a search that
        // arrives now says `idle` instead of an empty string.
        if schema::read_meta(&connection, "progress").is_none() {
            let idle = Progress {
                state: "idle".to_string(),
                ..Progress::default()
            };
            schema::write_meta(
                &connection,
                "progress",
                &serde_json::to_string(&idle).unwrap_or_default(),
            )?;
        }

        let nonce = schema::read_meta(&connection, "nonce").unwrap_or_default();
        let created_at: i64 = schema::read_meta(&connection, "created_at")
            .and_then(|value| value.parse().ok())
            .unwrap_or_else(schema::unix_now_ms);
        let (_git_dir, head_sha, first_root_commit) = schema::git_identity(&location.root);
        let (files, symbols) = counts(&connection)?;
        registry.record(BookEntry {
            book_key: location.key.clone(),
            root: root.clone(),
            dir: path_string(&location.dir),
            nonce,
            first_root_commit,
            head_sha,
            created_at,
            last_opened_at: schema::unix_now_ms(),
            files,
            symbols,
        });
        registry.save(registry_path)?;

        Ok(Self {
            location,
            registry_path: registry_path.to_path_buf(),
            journal,
            writer: Mutex::new(connection),
            report,
        })
    }

    /// Where this book lives.
    pub fn location(&self) -> &BookLocation {
        &self.location
    }

    /// The journal mode chosen by the startup probe (D4).
    pub fn journal(&self) -> JournalMode {
        self.journal
    }

    /// What the cold start did.
    pub fn report(&self) -> &OpenReport {
        &self.report
    }

    /// The `book_key` of every answer (S7).
    pub fn key(&self) -> &str {
        &self.location.key
    }

    /// `(files, symbols)` of the book.
    pub fn counts(&self) -> Result<(i64, i64), BookError> {
        let writer = self.writer.lock().expect("the book writer lock");
        Ok(counts(&writer)?)
    }

    /// Writes one batch: every file of the batch in one `BEGIN IMMEDIATE`,
    /// each chapter in its own `SAVEPOINT` (ideas §10.1, «Lotes»).
    ///
    /// A file whose content hash is unchanged is skipped before the savepoint:
    /// the watcher sends whatever it could not filter, and the engine is the
    /// last word on doing the work once.
    pub fn commit_batch(
        &self,
        parsed: &[ParsedFile],
        deletions: &[String],
        indexed_at: i64,
    ) -> Result<CommitReport, BookError> {
        let writer = self.writer.lock().expect("the book writer lock");
        writer.execute_batch("BEGIN IMMEDIATE")?;
        let mut report = CommitReport::default();
        let mut next_id = next_symbol_id(&writer)?;

        for (index, file) in parsed.iter().enumerate() {
            let savepoint = format!("chapter_{index}");
            writer.execute_batch(&format!("SAVEPOINT {savepoint}"))?;
            match write_chapter(&writer, file, indexed_at, &mut next_id) {
                Ok(chapter) => {
                    writer.execute_batch(&format!("RELEASE {savepoint}"))?;
                    report.files_written += usize::from(chapter.written);
                    report.files_skipped += usize::from(!chapter.written);
                    report.symbols += chapter.symbols;
                }
                Err(error) => {
                    // One bad chapter costs a chapter, never the batch.
                    writer
                        .execute_batch(&format!("ROLLBACK TO {savepoint}; RELEASE {savepoint}"))?;
                    eprintln!("gx-index: the chapter `{}` was skipped: {error}", file.path);
                    report.files_skipped += 1;
                }
            }
        }

        for path in deletions {
            if remove_chapter(&writer, path)? {
                report.files_removed += 1;
            }
        }

        writer.execute_batch("COMMIT")?;
        Ok(report)
    }

    /// The content hash the book has for a path, when it has the chapter.
    pub fn content_hash_of(&self, path: &str) -> Result<Option<String>, BookError> {
        let writer = self.writer.lock().expect("the book writer lock");
        Ok(writer
            .query_row(
                "SELECT content_hash FROM file WHERE path = ?1",
                [path],
                |row| row.get::<_, String>(0),
            )
            .optional()?)
    }

    /// The paths of the book whose parent is exactly `dir`.
    ///
    /// The watcher syncs one directory at a time with a depth-1 scan, so the
    /// deletions it computes are the direct children of that directory and
    /// cannot reach a nested chapter by accident.
    pub fn paths_directly_under(&self, dir: &str) -> Result<BTreeSet<String>, BookError> {
        let writer = self.writer.lock().expect("the book writer lock");
        let mut statement = writer.prepare("SELECT path FROM file")?;
        let paths = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let prefix = if dir.is_empty() {
            String::new()
        } else {
            format!("{}/", dir.trim_end_matches('/'))
        };
        Ok(paths
            .into_iter()
            .filter(|path| {
                path.strip_prefix(&prefix)
                    .is_some_and(|rest| !rest.is_empty() && !rest.contains('/'))
            })
            .collect())
    }

    /// Every path of the book, sorted: the map the watcher diffs against.
    pub fn all_paths(&self) -> Result<BTreeSet<String>, BookError> {
        let writer = self.writer.lock().expect("the book writer lock");
        let mut statement = writer.prepare("SELECT path FROM file")?;
        let paths = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<BTreeSet<_>, _>>()?;
        Ok(paths)
    }

    /// Reads the progress of the first index (S9).
    pub fn progress(&self) -> Result<Progress, BookError> {
        let writer = self.writer.lock().expect("the book writer lock");
        let value = schema::read_meta(&writer, "progress");
        Ok(value
            .and_then(|text| serde_json::from_str::<Progress>(&text).ok())
            .unwrap_or_default())
    }

    /// Writes the progress row (the «indexing 40%» of S9).
    pub fn set_progress(&self, progress: &Progress) -> Result<(), BookError> {
        let writer = self.writer.lock().expect("the book writer lock");
        let text = serde_json::to_string(progress).unwrap_or_default();
        schema::write_meta(&writer, "progress", &text)?;
        Ok(())
    }

    /// Searches the book with a read-only connection of its own.
    ///
    /// A `Connection` is not `Sync`, so the readers are per call; the writer
    /// keeps its own connection behind the mutex (ideas §10.1, «Pragmas»).
    pub fn search(&self, request: &SearchRequest) -> Result<SearchOutcome, BookError> {
        let reader = Connection::open_with_flags(
            &self.location.database,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        schema::apply_reader_pragmas(&reader)?;
        Ok(search::search(&reader, request)?)
    }

    /// A checkpoint that never blocks a reader: only when the queue of the
    /// indexer is empty (ideas §10.1, `wal_autocheckpoint=0`).
    pub fn checkpoint_passive(&self) -> Result<(), BookError> {
        let writer = self.writer.lock().expect("the book writer lock");
        writer.query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |_row| Ok(()))?;
        Ok(())
    }

    /// The `TRUNCATE` checkpoint of the idle exit.
    pub fn checkpoint_truncate(&self) -> Result<(), BookError> {
        let writer = self.writer.lock().expect("the book writer lock");
        writer.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_row| Ok(()))?;
        Ok(())
    }

    /// Closes the book cleanly: `optimize`, a `TRUNCATE` checkpoint and the
    /// `clean_shutdown` marker the cold start reads.
    pub fn close_clean(&self) -> Result<(), BookError> {
        let writer = self.writer.lock().expect("the book writer lock");
        let (files, symbols) = counts(&writer)?;
        writer.execute_batch("PRAGMA optimize")?;
        schema::write_meta(&writer, "clean_shutdown", "1")?;
        let _ = writer.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_row| Ok(()));

        let mut registry = BookRegistry::load(&self.registry_path);
        let nonce = schema::read_meta(&writer, "nonce").unwrap_or_default();
        let created_at: i64 = schema::read_meta(&writer, "created_at")
            .and_then(|value| value.parse().ok())
            .unwrap_or_default();
        let (_git_dir, head_sha, first_root_commit) = schema::git_identity(&self.location.root);
        registry.record(BookEntry {
            book_key: self.location.key.clone(),
            root: self.location.root_string(),
            dir: path_string(&self.location.dir),
            nonce,
            first_root_commit,
            head_sha,
            created_at,
            last_opened_at: schema::unix_now_ms(),
            files,
            symbols,
        });
        registry.save(&self.registry_path)?;
        Ok(())
    }

    /// Runs a statement against the writer connection (tests and maintenance).
    pub fn with_writer<T>(
        &self,
        action: impl FnOnce(&Connection) -> rusqlite::Result<T>,
    ) -> Result<T, BookError> {
        let writer = self.writer.lock().expect("the book writer lock");
        Ok(action(&writer)?)
    }

    /// The reason a book was rebuilt, as one line for the log.
    pub fn rebuild_message(reason: &RebuildReason) -> String {
        reason.message()
    }
}

/// The identity of a brand-new book.
fn fresh_identity(location: &BookLocation) -> Identity {
    let (git_dir, head_sha, first_root_commit) = schema::git_identity(&location.root);
    Identity {
        worktree_root: location.root_string(),
        git_dir,
        head_sha,
        first_root_commit,
        created_at: schema::unix_now_ms(),
        nonce: schema::new_nonce(),
        owner_uid: schema::owner_uid(),
    }
}

/// Refreshes the git lines of `meta`: the head and the git dir move with the
/// worktree, and the book has to say which checkout it indexed (D4).
fn refresh_identity(connection: &Connection, location: &BookLocation) -> Result<(), BookError> {
    let (git_dir, head_sha, first_root_commit) = schema::git_identity(&location.root);
    for (key, value) in [
        ("git_dir", git_dir),
        ("head_sha", head_sha),
        ("first_root_commit", first_root_commit),
    ] {
        schema::write_meta(connection, key, &value)?;
    }
    Ok(())
}

/// `(files, symbols)` of an open connection.
fn counts(connection: &Connection) -> rusqlite::Result<(i64, i64)> {
    let files = connection.query_row("SELECT COUNT(*) FROM file", [], |row| row.get(0))?;
    let symbols = connection.query_row("SELECT COUNT(*) FROM symbol", [], |row| row.get(0))?;
    Ok((files, symbols))
}

/// The next free symbol id of the book. Ids are monotone inside a batch and
/// **not** a contiguous block per file (ideas §10.1, delta 8): nothing may
/// assume that the symbols of a chapter are a range.
fn next_symbol_id(connection: &Connection) -> rusqlite::Result<i64> {
    connection.query_row("SELECT COALESCE(MAX(id), 0) + 1 FROM symbol", [], |row| {
        row.get(0)
    })
}

/// What one chapter wrote.
struct ChapterReport {
    written: bool,
    symbols: i64,
}

/// Replaces the chapter of one file inside the caller's transaction.
fn write_chapter(
    connection: &Connection,
    file: &ParsedFile,
    indexed_at: i64,
    next_id: &mut i64,
) -> rusqlite::Result<ChapterReport> {
    let existing: Option<(i64, String)> = connection
        .query_row(
            "SELECT id, content_hash FROM file WHERE path = ?1",
            [&file.path],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;

    let file_id = match existing {
        Some((_, hash)) if hash == file.content_hash => {
            return Ok(ChapterReport {
                written: false,
                symbols: 0,
            });
        }
        Some((id, _)) => {
            // The chapter goes first: the trigger of `symbol` needs the old
            // values to delete its tokens, and the `file` row goes last.
            connection.execute("DELETE FROM symbol WHERE file_id = ?1", [id])?;
            connection.execute(
                "UPDATE file SET lang = ?2, content_hash = ?3, size = ?4, mtime_ns = ?5,
                 indexed_at = ?6 WHERE id = ?1",
                rusqlite::params![
                    id,
                    file.lang,
                    file.content_hash,
                    file.size,
                    file.mtime_ns,
                    indexed_at
                ],
            )?;
            id
        }
        None => connection.query_row(
            "INSERT INTO file (path, lang, content_hash, size, mtime_ns, indexed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) RETURNING id",
            rusqlite::params![
                file.path,
                file.lang,
                file.content_hash,
                file.size,
                file.mtime_ns,
                indexed_at
            ],
            |row| row.get::<_, i64>(0),
        )?,
    };

    let mut ids: Vec<i64> = Vec::with_capacity(file.symbols.len());
    // `UNIQUE(file_id, qname)` is an invariant of the schema (ideas §10):
    // the ordinary case produces the natural qname, and the rare collision
    // (a struct and its inherent `impl`, two `impl` blocks of one type) is
    // numbered instead of dropped.
    let mut used_qnames: std::collections::BTreeMap<String, u32> =
        std::collections::BTreeMap::new();
    for symbol in &file.symbols {
        let id = *next_id;
        *next_id += 1;
        let parent = symbol.parent.and_then(|index| ids.get(index).copied());
        let qname = unique_qname(&mut used_qnames, &symbol.qname);
        connection.execute(
            "INSERT INTO symbol (id, file_id, parent_id, kind, name, qname, search_text,
                                 name_norm, start_line, start_col, end_line, end_col,
                                 start_byte, end_byte)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            rusqlite::params![
                id,
                file_id,
                parent,
                symbol.kind,
                symbol.name,
                qname,
                symbol.search_text,
                symbol.name_norm,
                symbol.start_line,
                symbol.start_col,
                symbol.end_line,
                symbol.end_col,
                symbol.start_byte,
                symbol.end_byte
            ],
        )?;
        ids.push(id);
    }

    Ok(ChapterReport {
        written: true,
        symbols: file.symbols.len() as i64,
    })
}

/// The qname of a symbol inside one chapter: `a::Config`, `a::Config#2`.
fn unique_qname(used: &mut std::collections::BTreeMap<String, u32>, qname: &str) -> String {
    let count = used.entry(qname.to_string()).or_insert(0);
    *count += 1;
    if *count == 1 {
        qname.to_string()
    } else {
        format!("{qname}#{count}")
    }
}

/// Removes the chapter of a file that is no longer in the worktree.
fn remove_chapter(connection: &Connection, path: &str) -> rusqlite::Result<bool> {
    let existing: Option<i64> = connection
        .query_row("SELECT id FROM file WHERE path = ?1", [path], |row| {
            row.get(0)
        })
        .optional()?;
    let Some(file_id) = existing else {
        return Ok(false);
    };
    connection.execute("DELETE FROM symbol WHERE file_id = ?1", [file_id])?;
    connection.execute("DELETE FROM file WHERE id = ?1", [file_id])?;
    Ok(true)
}

/// Unix epoch milliseconds as ISO-8601 UTC, the only place the book leaves
/// the integer form (ideas §10.1, delta 7).
pub fn iso8601_utc(ms: i64) -> String {
    let seconds = ms.div_euclid(1_000);
    let millis = ms.rem_euclid(1_000);
    let days = seconds.div_euclid(86_400);
    let second_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = second_of_day / 3_600;
    let minute = (second_of_day % 3_600) / 60;
    let second = second_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

/// Days since 1970-01-01 as a civil date (Howard Hinnant's algorithm, the one
/// every standard library uses; no dependency is needed for it).
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
    use super::iso8601_utc;

    /// Delta 7: `indexed_at` is an integer in the book and ISO-8601 only at
    /// the edge of the tool.
    #[test]
    fn the_index_time_is_formatted_as_iso8601_utc() {
        assert_eq!(iso8601_utc(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso8601_utc(1_700_000_000_000), "2023-11-14T22:13:20.000Z");
        assert_eq!(iso8601_utc(1_700_000_000_123), "2023-11-14T22:13:20.123Z");
    }
}
