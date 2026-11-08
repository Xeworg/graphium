//! The book through its public surface: schema v1.1, its procedures, the
//! per-file cycle and the search (F9, corte C0; ideas §10.1).
//!
//! Every test builds its own worktree and its own books root inside the system
//! temp directory: nothing here reads or writes the repository, and nothing
//! depends on the environment of the machine.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use gx_index::book::Book;
use gx_index::location::BooksRoot;
use gx_index::schema::{verify_fts, SCHEMA_VERSION};
use gx_index::service::flag_from_value;
use gx_index::watcher::{IndexOptions, Indexer};
use gx_index::{book_key, SearchOutcome, SearchRequest, SymbolCandidate, SymbolHit};

/// A worktree with its books root, removed when the test ends.
struct Scratch {
    base: PathBuf,
    root: PathBuf,
    books: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let base = std::env::temp_dir().join(format!(
            "gx-index-{name}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let root = base.join("worktree");
        let books = base.join("books");
        fs::create_dir_all(root.join("src")).expect("the worktree");
        Self { base, root, books }
    }

    fn write(&self, relative: &str, text: &str) {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("the parent dir");
        }
        fs::write(path, text).expect("the file");
    }

    fn canonical_root(&self) -> PathBuf {
        fs::canonicalize(&self.root).expect("canonical root")
    }

    fn books_root(&self) -> BooksRoot {
        BooksRoot::at(&self.books)
    }

    fn open(&self) -> Book {
        let books = self.books_root();
        let location = books.locate(&self.root).expect("locate");
        Book::open(location, &books.registry_path()).expect("open the book")
    }

    fn indexer(&self, book: &Arc<Book>) -> Indexer {
        Indexer::new(book.clone(), self.canonical_root(), IndexOptions::default())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn search(book: &Book, query: &str) -> SearchOutcome {
    book.search(&SearchRequest {
        query: query.to_string(),
    })
    .expect("the search runs")
}

fn hits(outcome: SearchOutcome) -> Vec<SymbolHit> {
    match outcome {
        SearchOutcome::Hits(hits) => hits,
        other => panic!("expected hits, got {other:?}"),
    }
}

fn candidates(outcome: SearchOutcome) -> Vec<SymbolCandidate> {
    match outcome {
        SearchOutcome::Ambiguous(candidates) => candidates,
        other => panic!("expected candidates, got {other:?}"),
    }
}

fn meta(book: &Book, key: &str) -> Option<String> {
    book.with_writer(|connection| {
        Ok(connection
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
                row.get::<_, String>(0)
            })
            .ok())
    })
    .expect("the writer runs")
}

// ---------------------------------------------------------------------------
// S6 — the schema and its procedures
// ---------------------------------------------------------------------------

/// S6/ideas §10.1: the file is created with the identity marker, the pragmas
/// of the schema and a foreign key enforcement that is explicit, never
/// inherited.
#[test]
fn the_book_is_created_with_schema_v1_1_and_the_identity_marker() {
    let scratch = Scratch::new("schema");
    let book = scratch.open();

    let root = book
        .with_writer(|connection| {
            let user_version: i64 =
                connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
            let page_size: i64 =
                connection.pragma_query_value(None, "page_size", |row| row.get(0))?;
            let auto_vacuum: i64 =
                connection.pragma_query_value(None, "auto_vacuum", |row| row.get(0))?;
            let foreign_keys: i64 =
                connection.pragma_query_value(None, "foreign_keys", |row| row.get(0))?;
            let synchronous: i64 =
                connection.pragma_query_value(None, "synchronous", |row| row.get(0))?;
            let wal_autocheckpoint: i64 =
                connection.pragma_query_value(None, "wal_autocheckpoint", |row| row.get(0))?;
            let journal: String =
                connection.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
            Ok((
                user_version,
                page_size,
                auto_vacuum,
                foreign_keys,
                synchronous,
                wal_autocheckpoint,
                journal,
            ))
        })
        .expect("the pragmas");

    assert_eq!(root.0, SCHEMA_VERSION, "PRAGMA user_version");
    assert_eq!(root.1, 4096, "page_size before any table");
    assert_eq!(root.2, 2, "auto_vacuum=INCREMENTAL at creation");
    assert_eq!(root.3, 1, "foreign_keys=ON is explicit");
    assert_eq!(root.4, 1, "synchronous=NORMAL follows the WAL probe");
    assert_eq!(root.5, 0, "the writer decides when to checkpoint");
    assert_eq!(root.6, "wal", "a local books root gets WAL");

    let expected_root = scratch.canonical_root().to_string_lossy().to_string();
    assert_eq!(meta(&book, "schema_version").as_deref(), Some("1"));
    assert_eq!(
        meta(&book, "worktree_root").as_deref(),
        Some(expected_root.as_str())
    );
    for key in [
        "git_dir",
        "head_sha",
        "first_root_commit",
        "created_at",
        "nonce",
        "owner_uid",
    ] {
        assert!(
            meta(&book, key).is_some(),
            "the marker has to carry `{key}`"
        );
    }
    assert!(
        meta(&book, "nonce").is_some_and(|nonce| nonce.len() >= 16),
        "the nonce is the trust-on-first-byte marker"
    );

    // Foreign keys are enforced: an edge to a symbol that does not exist fails.
    let orphan = book.with_writer(|connection| {
        connection.execute(
            "INSERT INTO edge (src_id, dst_name, kind) VALUES (999, 'nowhere', 'calls')",
            [],
        )
    });
    assert!(orphan.is_err(), "foreign_keys=ON has to reject the orphan");

    // `edge` is created and empty: edges arrive in C1.
    let edges: i64 = book
        .with_writer(|connection| {
            connection.query_row("SELECT COUNT(*) FROM edge", [], |row| row.get(0))
        })
        .expect("the count");
    assert_eq!(edges, 0, "C0 does not populate edges");

    // The FTS is the external-content one, with the real columns of `symbol`.
    let columns: Vec<String> = book
        .with_writer(|connection| {
            let mut statement =
                connection.prepare("SELECT name FROM pragma_table_info('symbol')")?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .expect("the columns");
    for column in ["name", "qname", "search_text", "name_norm"] {
        assert!(columns.contains(&column.to_string()), "{columns:?}");
    }

    // The two decisions of ideas §10.1 that a column list cannot see: the FTS
    // is external-content and its tokenizer does **not** keep `_`, and the
    // delete goes through the special `('delete', old.id, …)` command.
    let ddl: Vec<(String, String)> = book
        .with_writer(|connection| {
            let mut statement =
                connection.prepare("SELECT name, sql FROM sqlite_master WHERE sql IS NOT NULL")?;
            let rows = statement
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .expect("the schema");
    let fts = ddl
        .iter()
        .find(|(name, _)| name == "symbol_fts")
        .map(|(_, sql)| sql.clone())
        .expect("the FTS table");
    assert!(fts.contains("content='symbol'"), "{fts}");
    assert!(fts.contains("content_rowid='id'"), "{fts}");
    assert!(
        !fts.contains("tokenchars"),
        "the tokenizer must not keep `_` whole: {fts}"
    );
    let delete_trigger = ddl
        .iter()
        .find(|(name, _)| name == "symbol_fts_ad")
        .map(|(_, sql)| sql.clone())
        .expect("the delete trigger");
    assert!(
        delete_trigger.contains("('delete', old.id, old.name, old.qname, old.search_text)"),
        "{delete_trigger}"
    );
}

/// D4/ideas §8-A: a file that is not the book of this root is thrown away and
/// indexed again. The identity is the registry nonce, never the inode.
#[test]
fn a_seeded_database_without_the_marker_is_rejected_and_rebuilt() {
    let scratch = Scratch::new("seeded");
    let location = scratch.books_root().locate(&scratch.root).expect("locate");
    fs::create_dir_all(&location.dir).expect("the book dir");
    // A stranger SQLite file sitting exactly where the book would be.
    let seeded = rusqlite::Connection::open(&location.database).expect("seed");
    seeded
        .execute_batch("CREATE TABLE stolen (secret TEXT); INSERT INTO stolen VALUES ('x');")
        .expect("the seed");
    drop(seeded);

    let book = scratch.open();
    assert!(
        book.report().rebuilt.is_some(),
        "a database without the marker is not the book: {:?}",
        book.report()
    );
    let stolen: Result<i64, _> = book.with_writer(|connection| {
        connection.query_row("SELECT COUNT(*) FROM stolen", [], |row| row.get(0))
    });
    assert!(stolen.is_err(), "the seeded table is gone");

    // The registry remembers the nonce: a book whose registry entry is lost
    // is a stranger too, and it is indexed again instead of trusted.
    book.close_clean().expect("clean close");
    fs::remove_file(scratch.books_root().registry_path()).expect("remove the registry");
    let reopened = scratch.open();
    assert!(
        reopened.report().rebuilt.is_some(),
        "without the registry nonce the file cannot be trusted: {:?}",
        reopened.report()
    );
}

/// Ideas §10.1, «Cold start»: the fast path is a `meta` probe;
/// `quick_check` runs only after a shutdown that was not clean.
#[test]
fn the_cold_start_probes_meta_and_only_quick_checks_after_an_unclean_shutdown() {
    let scratch = Scratch::new("cold");
    let book = scratch.open();
    assert!(
        !book.report().quick_checked,
        "a fresh book has nothing to check"
    );
    book.close_clean().expect("clean close");

    let clean = scratch.open();
    assert!(clean.report().probed, "the file existed");
    assert!(
        !clean.report().quick_checked,
        "a clean shutdown skips quick_check"
    );
    assert!(clean.report().rebuilt.is_none());
    // Dropping the book without `close_clean` leaves `clean_shutdown = 0`.
    drop(clean);

    let unclean = scratch.open();
    assert!(
        unclean.report().quick_checked,
        "an unclean shutdown runs quick_check"
    );
    assert!(
        unclean.report().rebuilt.is_none(),
        "a healthy file survives the quick_check: {:?}",
        unclean.report()
    );
}

/// D4: the same canonical root is the same key; another root is another book.
#[test]
fn the_key_of_a_book_is_the_hash_of_its_canonical_root() {
    let first = Scratch::new("key-a");
    let second = Scratch::new("key-b");
    let first_key = book_key(&first.canonical_root());
    assert_eq!(first_key, book_key(&first.canonical_root()));
    assert_ne!(first_key, book_key(&second.canonical_root()));
    assert!(
        first_key.starts_with("worktree-"),
        "the key keeps a readable slug: {first_key}"
    );
    let hex = first_key.rsplit('-').next().expect("the hash");
    assert_eq!(hex.len(), 16, "16 hex characters of the root hash");
    assert!(hex.chars().all(|char| char.is_ascii_hexdigit()));
}

// ---------------------------------------------------------------------------
// S9 / S7 — the per-file cycle, the FTS order and the search
// ---------------------------------------------------------------------------

const FIRST: &str = r#"
pub fn alpha_one() {}

pub fn renderWith(_input: u8) -> u8 { 1 }

pub struct Config;

impl Config {
    pub fn load_now(&self) {}
}
"#;

const SECOND: &str = r#"
pub fn beta_two() {}

pub fn renderWith(_input: u8) -> u8 { 2 }
"#;

/// S9 + S7: a second write to one file replaces its chapter and only its
/// chapter; the other files keep their rows, their ids and their timestamps.
#[test]
fn a_second_write_to_the_same_file_does_not_rebuild_the_book() {
    let scratch = Scratch::new("cycle");
    scratch.write("src/a.rs", FIRST);
    scratch.write("src/b.rs", "pub fn untouched() {}\n");
    let book = Arc::new(scratch.open());
    let indexer = scratch.indexer(&book);
    indexer.run_full().expect("the first index");

    let ids_before: Vec<(String, i64, i64)> = book
        .with_writer(|connection| {
            let mut statement =
                connection.prepare("SELECT path, id, indexed_at FROM file ORDER BY path")?;
            let rows = statement
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .expect("the file rows");
    assert_eq!(ids_before.len(), 2);

    // Two writes to the same file: the first one changes it, the second one
    // changes it again. The book follows both without rebuilding anything.
    for source in [SECOND, "pub fn gamma_three() {}\n"] {
        std::thread::sleep(std::time::Duration::from_millis(5));
        scratch.write("src/a.rs", source);
        let summary = indexer
            .sync_dirs(&[scratch.canonical_root().join("src")])
            .expect("the incremental sync");
        assert_eq!(summary.files, 1, "only the edited file was written");
        assert_eq!(summary.removed, 0, "no chapter was removed");
    }

    let ids_after: Vec<(String, i64, i64)> = book
        .with_writer(|connection| {
            let mut statement =
                connection.prepare("SELECT path, id, indexed_at FROM file ORDER BY path")?;
            let rows = statement
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .expect("the file rows");

    assert_eq!(ids_after.len(), 2, "two chapters, never a rebuild");
    let untouched_before = ids_before.iter().find(|row| row.0 == "src/b.rs").unwrap();
    let untouched_after = ids_after.iter().find(|row| row.0 == "src/b.rs").unwrap();
    assert_eq!(
        untouched_before, untouched_after,
        "the other file is untouched"
    );
    let edited_before = ids_before.iter().find(|row| row.0 == "src/a.rs").unwrap();
    let edited_after = ids_after.iter().find(|row| row.0 == "src/a.rs").unwrap();
    assert_eq!(
        edited_before.1, edited_after.1,
        "the chapter keeps its file id"
    );
    assert!(
        edited_after.2 >= edited_before.2,
        "the chapter says when it was indexed"
    );

    assert!(
        matches!(search(&book, "alpha_one"), SearchOutcome::Empty),
        "the old symbol is gone"
    );
    assert!(
        matches!(search(&book, "gamma_three"), SearchOutcome::Hits(_)),
        "the new symbol is there"
    );
    assert!(
        matches!(search(&book, "untouched"), SearchOutcome::Hits(_)),
        "the other chapter is still there"
    );
}

/// Ideas §10.1, deltas 1 and 9: the chapter is deleted from `symbol` first, so
/// the `('delete', old.id, …)` trigger has the old values, and the FTS never
/// keeps the tokens of a symbol that is gone.
#[test]
fn the_per_file_cycle_keeps_fts_in_sync_without_stale_tokens() {
    let scratch = Scratch::new("fts");
    scratch.write("src/a.rs", FIRST);
    let book = Arc::new(scratch.open());
    let indexer = scratch.indexer(&book);
    indexer.run_full().expect("the first index");

    let hits = hits(search(&book, "renderWith"));
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].file, "src/a.rs");
    assert_eq!(hits[0].line, 4, "1-based line of the declaration");

    scratch.write("src/a.rs", SECOND);
    indexer
        .sync_dirs(&[scratch.canonical_root().join("src")])
        .expect("the sync");

    // The integrity check of FTS5 against external content (`1`) is the one
    // that catches a stale index entry; it must pass without a rebuild.
    book.with_writer(|connection| verify_fts(connection, false))
        .expect("the FTS agrees with `symbol`");

    assert!(
        matches!(search(&book, "alpha_one"), SearchOutcome::Empty),
        "the tokens of the deleted symbol are gone"
    );
    assert!(
        matches!(search(&book, "Config"), SearchOutcome::Empty),
        "a deleted type leaves no token"
    );
    assert!(
        matches!(search(&book, "renderWith"), SearchOutcome::Hits(_)),
        "the new generation is indexed"
    );
}

/// Ideas §7 (the queries explorer) + delta 5: CamelCase and snake_case hit,
/// and two tokens are an AND, never a concatenated phrase.
#[test]
fn the_search_finds_camel_case_and_snake_case_and_ands_the_tokens() {
    let scratch = Scratch::new("search");
    scratch.write(
        "src/a.rs",
        r#"
pub fn renderWith() {}

pub fn render_with_all() {}

pub fn parse_config() {}
"#,
    );
    scratch.write("src/lib.rs", "pub fn render_diagram() {}\n");
    let book = Arc::new(scratch.open());
    scratch.indexer(&book).run_full().expect("the index");

    // The full snake_case name identifies one symbol: the phrase of its
    // tokens is unique in the book.
    for query in ["render_with_all", "render with all", "RENDER_WITH_ALL"] {
        let found = hits(search(&book, query));
        assert_eq!(found.len(), 1, "`{query}` -> {found:?}");
        assert_eq!(found[0].name, "render_with_all");
    }

    // The CamelCase name is one token of the index, so it is an answer.
    let camel = hits(search(&book, "renderWith"));
    assert_eq!(camel.len(), 1, "{camel:?}");
    assert_eq!(camel[0].name, "renderWith");

    // A query whose words only cover part of two symbols is ambiguous, and
    // the candidates say which qname disambiguates (P3, condition 2).
    for query in ["render_with", "render with"] {
        let ambiguous = candidates(search(&book, query));
        assert!(ambiguous.len() >= 2, "`{query}` -> {ambiguous:?}");
        assert!(ambiguous
            .iter()
            .any(|candidate| candidate.name == "renderWith"));
        assert!(ambiguous
            .iter()
            .any(|candidate| candidate.name == "render_with_all"));
    }

    let anded = hits(search(&book, "render diagram"));
    assert_eq!(anded.len(), 1, "{anded:?}");
    assert_eq!(anded[0].name, "render_diagram");
    assert_eq!(
        anded[0].qname, "render_diagram",
        "`src/lib.rs` is the crate root"
    );
    assert!(!anded[0].content_hash.is_empty());
    assert!(anded[0].indexed_at > 0);

    assert!(matches!(
        search(&book, "parse_config"),
        SearchOutcome::Hits(_)
    ));
    assert!(
        matches!(search(&book, "nothing_like_this"), SearchOutcome::Empty),
        "an unknown name is empty, not an error"
    );
}

/// P3, condition 2: a name that several declarations carry gets candidates,
/// never one confident answer.
#[test]
fn an_ambiguous_name_answers_candidates_instead_of_one_answer() {
    let scratch = Scratch::new("ambiguous");
    scratch.write("src/one.rs", "pub fn parse() {}\n");
    scratch.write("src/two.rs", "pub fn parse() {}\n");
    let book = Arc::new(scratch.open());
    scratch.indexer(&book).run_full().expect("the index");

    let found = candidates(search(&book, "parse"));
    assert_eq!(found.len(), 2, "{found:?}");
    let files: Vec<&str> = found
        .iter()
        .map(|candidate| candidate.file.as_str())
        .collect();
    assert!(files.contains(&"src/one.rs"), "{found:?}");
    assert!(files.contains(&"src/two.rs"), "{found:?}");
    assert_eq!(
        found[0].occurrences, 2,
        "the candidate says how many declarations carry the name"
    );
    assert!(found.iter().all(|candidate| candidate.name == "parse"));
    assert_ne!(found[0].qname, found[1].qname, "the qname disambiguates");

    // A qualified name that only one file declares is an answer.
    assert!(matches!(search(&book, "one parse"), SearchOutcome::Hits(_)));
}

/// Ideas §10.1, delta 5: the input of `MATCH` is always escaped, so no query
/// is an operator and nothing panics on quoting.
#[test]
fn the_match_input_is_escaped_so_no_query_is_an_operator() {
    let scratch = Scratch::new("escape");
    scratch.write("src/a.rs", "pub fn parse_config() {}\n");
    let book = Arc::new(scratch.open());
    scratch.indexer(&book).run_full().expect("the index");

    for query in [
        "AND OR NOT",
        "\"",
        "parse*",
        "a\"b",
        "NEAR(parse, 2)",
        ":::",
    ] {
        let outcome = book
            .search(&SearchRequest {
                query: query.to_string(),
            })
            .unwrap_or_else(|error| panic!("`{query}` has to be answered, not rejected: {error}"));
        // Whatever the answer is, it is one of the three honest ones.
        match outcome {
            SearchOutcome::Hits(_) | SearchOutcome::Ambiguous(_) | SearchOutcome::Empty => {}
        }
    }
}

/// D1/D2: a file that disappears loses its chapter, and its symbols stop
/// being found. Nothing assumes the ids of a chapter are a contiguous block.
#[test]
fn removing_a_chapter_removes_its_symbols_and_keeps_the_others() {
    let scratch = Scratch::new("remove");
    scratch.write("src/gone.rs", "pub fn vanished_one() {}\n");
    scratch.write("src/stays.rs", "pub fn staying_one() {}\n");
    let book = Arc::new(scratch.open());
    let indexer = scratch.indexer(&book);
    indexer.run_full().expect("the index");
    assert!(matches!(
        search(&book, "vanished_one"),
        SearchOutcome::Hits(_)
    ));

    fs::remove_file(scratch.root.join("src/gone.rs")).expect("remove the file");
    let summary = indexer
        .sync_dirs(&[scratch.canonical_root().join("src")])
        .expect("the sync");
    assert_eq!(summary.removed, 1, "{summary:?}");
    assert!(matches!(
        search(&book, "vanished_one"),
        SearchOutcome::Empty
    ));
    assert!(matches!(
        search(&book, "staying_one"),
        SearchOutcome::Hits(_)
    ));

    // A new chapter gets a fresh id; no code may assume a range per file.
    scratch.write("src/new.rs", "pub fn arrived_one() {}\n");
    indexer
        .sync_dirs(&[scratch.canonical_root().join("src")])
        .expect("the sync");
    let (files, symbols) = book.counts().expect("the counts");
    assert_eq!(files, 2);
    assert_eq!(symbols, 2);
}

/// S9: `meta.progress` says how far the first index went, and it ends in
/// `indexed` with the totals of the book.
#[test]
fn the_progress_row_follows_the_first_index() {
    let scratch = Scratch::new("progress");
    scratch.write("src/a.rs", "pub fn one() {}\npub fn two() {}\n");
    scratch.write("src/b.rs", "pub struct Three;\n");
    let book = Arc::new(scratch.open());
    scratch.indexer(&book).run_full().expect("the index");

    let progress = book.progress().expect("the progress row");
    assert_eq!(progress.state, "indexed");
    assert_eq!(progress.files_total, 2);
    assert_eq!(progress.files_done, 2);
    assert_eq!(progress.symbols, 3);
    assert!(progress.updated_at > 0);
}

/// D2: the walk respects `.gitignore` and the hard exclusions, so the book
/// never spends the user's resources on `target/` or on `.git`.
#[test]
fn the_walk_respects_gitignore_and_the_hard_exclusions() {
    let scratch = Scratch::new("ignore");
    scratch.write(".gitignore", "ignored/\ngenerated.rs\n");
    scratch.write("src/kept.rs", "pub fn kept_one() {}\n");
    scratch.write("ignored/hidden.rs", "pub fn ignored_one() {}\n");
    scratch.write("generated.rs", "pub fn generated_one() {}\n");
    scratch.write(".git/objects/blob.rs", "pub fn git_object() {}\n");
    let book = Arc::new(scratch.open());
    let summary = scratch.indexer(&book).run_full().expect("the index");

    assert_eq!(summary.files, 1, "{summary:?}");
    assert!(matches!(search(&book, "kept_one"), SearchOutcome::Hits(_)));
    for query in ["ignored_one", "generated_one", "git_object"] {
        assert!(
            matches!(search(&book, query), SearchOutcome::Empty),
            "`{query}` must not be in the book"
        );
    }
}

/// D2: a binary or oversized file is skipped, and the pass says so instead of
/// failing.
#[test]
fn a_binary_file_is_skipped_with_a_warning() {
    let scratch = Scratch::new("binary");
    scratch.write("src/ok.rs", "pub fn visible_one() {}\n");
    fs::write(scratch.root.join("src/blob.rs"), b"pub fn hidden\x00\x01").expect("the binary");
    let book = Arc::new(scratch.open());
    let summary = scratch.indexer(&book).run_full().expect("the index");

    assert_eq!(summary.files, 1, "{summary:?}");
    assert_eq!(summary.skipped, 1, "{summary:?}");
    assert!(matches!(
        search(&book, "visible_one"),
        SearchOutcome::Hits(_)
    ));
    let progress = book.progress().expect("the progress");
    assert_eq!(progress.files_failed, 1);
}

/// The switch of S5 and the disabled path never open a book.
#[test]
fn the_disabled_gate_is_the_explicit_opt_out() {
    assert!(
        flag_from_value(None),
        "no GRAPHIUM_BOOK at all keeps the book on (S5, L5)"
    );
    assert!(flag_from_value(Some("on".into())));
    assert!(!flag_from_value(Some("off".into())));
    let scratch = Scratch::new("disabled");
    let error = gx_index::service::ensure(&gx_index::BooksConfig::disabled(), &scratch.root)
        .expect_err("the disabled gate answers an error");
    assert!(matches!(error, gx_index::BookError::Disabled));
    assert!(
        !scratch.books.exists(),
        "the disabled path never touches the books root"
    );
}

/// D4/ideas §8: a books root inside a synced folder steps down to the
/// rollback journal, with `synchronous=FULL`, and says so.
#[test]
fn a_synced_books_root_uses_the_rollback_journal() {
    let scratch = Scratch::new("synced");
    // The probe reads the shape of the path, so the marker is enough to put
    // the books root inside a synced folder without touching the machine.
    let synced = BooksRoot::at(scratch.base.join("OneDrive").join("graphium-books"));
    let location = synced.locate(&scratch.root).expect("locate");
    let book = Book::open(location, &synced.registry_path()).expect("open");

    assert_eq!(book.journal(), gx_index::schema::JournalMode::Delete);
    let (journal, synchronous): (String, i64) = book
        .with_writer(|connection| {
            let journal = connection.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
            let synchronous =
                connection.pragma_query_value(None, "synchronous", |row| row.get(0))?;
            Ok((journal, synchronous))
        })
        .expect("the pragmas");
    assert_eq!(journal, "delete", "no WAL on a synced folder");
    assert_eq!(synchronous, 2, "a rollback journal needs synchronous=FULL");
}

/// Ideas §10.1, «Backpressure»: one parsing thread while a render is in
/// flight, four otherwise.
#[test]
fn the_indexer_parses_with_one_thread_while_a_render_is_in_flight() {
    let scratch = Scratch::new("backpressure");
    let book = Arc::new(scratch.open());
    let indexer = scratch.indexer(&book);
    assert_eq!(indexer.parse_threads(), 4, "the default budget");
    {
        let _render = gx_index::service::RenderGuard::enter();
        assert_eq!(indexer.parse_threads(), 1, "a render in flight wins");
    }
    assert_eq!(indexer.parse_threads(), 4, "the budget comes back");
}

/// D4/ideas §10: the book records the identity of the checkout it indexes.
#[test]
fn the_book_records_the_git_identity_of_the_worktree() {
    let scratch = Scratch::new("git");
    scratch.write("src/a.rs", "pub fn tracked() {}\n");
    let run = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(&scratch.root)
            .args([
                "-c",
                "user.email=test@example.invalid",
                "-c",
                "user.name=Test",
            ])
            .args(args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("git runs");
        assert!(status.success(), "git {args:?}");
    };
    run(&["init", "--quiet"]);
    run(&["add", "."]);
    run(&[
        "-c",
        "commit.gpgsign=false",
        "commit",
        "--quiet",
        "-m",
        "one",
    ]);

    let book = scratch.open();
    let head = meta(&book, "head_sha").expect("head_sha");
    assert_eq!(head.len(), 40, "a sha-1 head: {head}");
    assert!(head.chars().all(|char| char.is_ascii_hexdigit()));
    assert_eq!(
        meta(&book, "first_root_commit").as_deref(),
        Some(head.as_str()),
        "a one-commit repository has its root commit as head"
    );
    let git_dir = meta(&book, "git_dir").expect("git_dir");
    assert!(git_dir.ends_with(".git"), "{git_dir}");
}

// ---------------------------------------------------------------------------
// T4/C3 — the passes of the book and the settle a harness waits on
// ---------------------------------------------------------------------------

/// T4/C3 (the flake of `progress`): the watcher runs its first pass on its own
/// thread, so a synchronous pass of the caller does not end the first index and
/// the `progress` row keeps moving after `run_full` returns. Reading the row
/// was a race; the counter is the fact — two completed passes (the caller's and
/// the watcher's) with none in flight leave the row at `indexed` for good.
#[test]
fn the_passes_are_counted_and_the_book_settles_without_racing() {
    use std::time::{Duration, Instant};

    let scratch = Scratch::new("passes");
    scratch.write("src/one.rs", "pub fn one() {}\n");
    let book = Arc::new(scratch.open());
    let indexer = Arc::new(scratch.indexer(&book));
    assert_eq!(indexer.passes_done(), 0, "a fresh indexer has no passes");
    assert!(!indexer.is_indexing(), "a fresh indexer writes nothing");

    let watcher = Arc::clone(&indexer).start().expect("the watcher starts");
    indexer.run_full().expect("the synchronous pass");
    assert!(indexer.passes_done() >= 1, "the caller's pass is counted");

    let deadline = Instant::now() + Duration::from_secs(10);
    while indexer.passes_done() < 2 || indexer.is_indexing() {
        assert!(
            Instant::now() < deadline,
            "the watcher's own pass never finished: passes_done={}",
            indexer.passes_done()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        indexer.passes_done(),
        2,
        "one synchronous pass and the watcher's own first pass"
    );
    assert!(!indexer.is_indexing());
    let progress = book.progress().expect("the progress row");
    assert_eq!(
        progress.state, "indexed",
        "the settled row is the one a tool reads"
    );
    assert_eq!(progress.files_total, 1);
    watcher.stop();
}

/// T4/C3: the teardown of one worktree closes **its** book and leaves the book
/// of another worktree open. Closing every book from the end of one test is
/// what made `progress` flake in the suite of the tools: the next call reopened
/// a book mid-test and its first pass published `indexing` again.
#[test]
fn the_teardown_of_one_book_leaves_the_other_open() {
    let scratch = Scratch::new("teardown");
    let base = scratch.base.clone();
    // Two worktrees under one books root: the registry and the map of open
    // books are the process's, the roots are not.
    let books = scratch.books.clone();
    let one = base.join("one");
    let other = base.join("other");
    for root in [&one, &other] {
        fs::create_dir_all(root.join("src")).expect("the worktree");
    }
    fs::write(one.join("src/a.rs"), "pub fn a() {}\n").expect("the file");
    fs::write(other.join("src/b.rs"), "pub fn b() {}\n").expect("the file");

    let config = gx_index::BooksConfig::from_parts(None, Some(books));
    gx_index::service::ensure(&config, &one).expect("the first book");
    gx_index::service::ensure(&config, &other).expect("the second book");
    assert!(gx_index::service::is_open(&one));
    assert!(gx_index::service::is_open(&other));

    gx_index::service::shutdown(&one);
    assert!(
        !gx_index::service::is_open(&one),
        "the book that was torn down is closed"
    );
    assert!(
        gx_index::service::is_open(&other),
        "the other book is untouched"
    );

    // Forgetting a root with no open book is not an error.
    gx_index::service::shutdown(&one);
    gx_index::service::shutdown_all();
    assert!(
        !gx_index::service::is_open(&other),
        "the idle exit closes all"
    );
    let _ = fs::remove_dir_all(&base);
}

/// T4/C3 (S8, D4): the idle exit of the watcher — the `IDLE_TRUNCATE` moment —
/// prunes the books of worktrees that no longer exist, and never the book that
/// is being written.
#[test]
fn the_idle_exit_prunes_the_books_of_gone_worktrees() {
    use std::time::{Duration, Instant};

    use gx_index::location::{BookEntry, BookRegistry};

    let scratch = Scratch::new("idle-prune");
    scratch.write("src/one.rs", "pub fn one() {}\n");
    let book = Arc::new(scratch.open());

    // A stale book in the same books root: a database and a registry line whose
    // worktree does not exist.
    let stale_key = "stale-0123456789abcdef";
    let stale_dir = scratch.books.join(stale_key);
    fs::create_dir_all(&stale_dir).expect("the stale book dir");
    fs::write(stale_dir.join("libro.sqlite"), vec![0u8; 64]).expect("the database");
    let registry_path = scratch.books_root().registry_path();
    let mut registry = BookRegistry::load(&registry_path);
    registry.record(BookEntry {
        book_key: stale_key.to_string(),
        root: scratch
            .base
            .join("gone-worktree")
            .to_string_lossy()
            .to_string(),
        dir: stale_dir.to_string_lossy().to_string(),
        ..BookEntry::default()
    });
    registry.save(&registry_path).expect("the registry");
    assert!(
        registry.entry(book.key()).is_some(),
        "the live book is registered too"
    );

    let indexer = Arc::new(Indexer::new(
        Arc::clone(&book),
        scratch.canonical_root(),
        IndexOptions {
            debounce: Duration::from_millis(10),
            idle_truncate: Duration::from_millis(50),
            ..IndexOptions::default()
        },
    ));
    let watcher = Arc::clone(&indexer).start().expect("the watcher starts");
    let deadline = Instant::now() + Duration::from_secs(10);
    while stale_dir.exists() {
        assert!(
            Instant::now() < deadline,
            "the idle exit never pruned the book of a gone worktree"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    watcher.stop();

    let after = BookRegistry::load(&registry_path);
    assert!(
        after.entry(stale_key).is_none(),
        "the stale line is gone from the registry"
    );
    assert!(
        after.entry(book.key()).is_some(),
        "the book that was being written keeps its line"
    );
    let live_dir = scratch.books.join(book.key());
    assert!(
        live_dir.join("libro.sqlite").exists(),
        "the live book's database is untouched"
    );
    assert!(
        book.all_paths()
            .expect("the book reads")
            .contains("src/one.rs"),
        "the live book still answers"
    );
}

/// T3/S4: the languages of the book are the registry of the build. A chapter
/// whose language has no grammar in this build is refused at the one insert
/// path — the DDL carries no list that could refuse it (see the schema unit
/// test) — and the rest of the batch is written as usual.
#[test]
fn the_book_refuses_a_chapter_of_a_language_the_registry_does_not_know() {
    use gx_index::lang::GRAMMARS;
    use gx_index::ParsedFile;

    // `None` of the table: one valid chapter of the build and one of a
    // language no grammar of the registry answers.
    let (lang, path, text) = if let Some(grammar) = GRAMMARS.first() {
        let extension = grammar.extensions.first().copied().unwrap_or("txt");
        (
            grammar.lang,
            format!("src/valid.{extension}"),
            "fn one() {}\n",
        )
    } else {
        panic!("a build without a grammar cannot open a book");
    };
    let valid = gx_index::lang::parse(&path, text.as_bytes(), 0).expect("a chapter of the build");
    let unknown = ParsedFile {
        path: "src/one.rb".to_string(),
        lang: "ruby",
        content_hash: "hash-of-ruby".to_string(),
        size: 10,
        mtime_ns: 0,
        symbols: Vec::new(),
        edges: Vec::new(),
    };

    let scratch = Scratch::new("languages");
    let book = scratch.open();
    let report = book
        .commit_batch(&[valid, unknown], &[], 1)
        .expect("the batch");

    assert_eq!(report.files_written, 1, "the valid chapter is written");
    assert_eq!(report.files_skipped, 1, "`ruby` is refused, not written");
    assert_eq!(book.counts().expect("the counts").0, 1);
    let langs: Vec<String> = book
        .with_writer(|connection| {
            let mut statement = connection.prepare("SELECT lang FROM file")?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .expect("the languages");
    assert_eq!(langs, vec![lang.to_string()]);
}

/// T3/S4: a book written before the languages became data still carries the v1
/// `CHECK` in `sqlite_master`. Opening it relaxes the file in place — the same
/// `SCHEMA_VERSION`, every row and id kept, nothing parsed again — so a chapter
/// of a language the v1 DDL did not name can be written to it.
#[test]
fn a_v1_book_relaxes_its_language_check_when_it_is_opened() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    // The `file` table of a book written by the previous binary.
    const V1_FILE_DDL: &str = "
PRAGMA foreign_keys=OFF;
BEGIN;
CREATE TABLE file__v1 (
  id           INTEGER PRIMARY KEY,
  path         TEXT NOT NULL UNIQUE,
  lang         TEXT NOT NULL CHECK (lang IN ('rust', 'typescript', 'python', 'go', 'java')),
  content_hash TEXT NOT NULL,
  size         INTEGER NOT NULL CHECK (size >= 0),
  mtime_ns     INTEGER NOT NULL,
  indexed_at   INTEGER NOT NULL
);
INSERT INTO file__v1 (id, path, lang, content_hash, size, mtime_ns, indexed_at)
  SELECT id, path, lang, content_hash, size, mtime_ns, indexed_at FROM file;
DROP TABLE file;
ALTER TABLE file__v1 RENAME TO file;
COMMIT;
PRAGMA foreign_keys=ON;
";
    // The counters make two books of the same test name unique.
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let scratch = Scratch::new(&format!(
        "v1-migrated-{}",
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    scratch.write("src/one.rs", "pub fn one() {}\n");
    let book = Arc::new(scratch.open());
    scratch.indexer(&book).run_full().expect("the index");
    let before = book.counts().expect("the counts");
    assert!(before.0 >= 1 && before.1 >= 1, "the book has a chapter");

    // Leave the book as the previous binary would have left it.
    book.with_writer(|connection| connection.execute_batch(V1_FILE_DDL))
        .expect("the v1 table");
    drop(book);

    let migrated = scratch.open();
    assert!(
        migrated.report().migrated,
        "the open relaxed the v1 book in place"
    );
    assert!(
        migrated.report().rebuilt.is_none(),
        "a check that only restricted INSERTs is not a reason to reindex"
    );
    assert_eq!(migrated.counts().expect("the counts"), before);
    let ddl: String = migrated
        .with_writer(|connection| {
            connection.query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'file'",
                [],
                |row| row.get(0),
            )
        })
        .expect("the stored DDL");
    assert!(
        !ddl.contains("CHECK (lang IN"),
        "the stored table carries no language list: {ddl}"
    );

    // The step is once per book: the next open finds it done.
    drop(migrated);
    let again = scratch.open();
    assert!(!again.report().migrated, "the migration is idempotent");
    assert_eq!(again.counts().expect("the counts"), before);
}
