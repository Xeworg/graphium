//! The exit criterion of C0, end to end (feature doc `f9-libro`, Tasks):
//!
//! 1. a symbol search answers in **< 50 ms** at the scale of this repository,
//! 2. the watcher follows an edit of an already-indexed file **without
//!    rebuilding everything** (two writes to the same file).
//!
//! The first test indexes this repository (read-only: the book lives in a
//! temporary books root), the second one uses a scratch Rust tree and the real
//! `notify` watcher, with no shortcut in front of the debounce.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use gx_index::service::{self, BooksConfig};
use gx_index::{SearchOutcome, SearchRequest};

/// A temporary directory that removes itself.
struct Scratch {
    base: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let base = std::env::temp_dir().join(format!(
            "gx-index-e2e-{name}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&base).expect("the scratch dir");
        Self { base }
    }

    fn worktree(&self) -> PathBuf {
        let root = self.base.join("worktree");
        fs::create_dir_all(root.join("src")).expect("the worktree");
        root
    }

    fn books(&self) -> PathBuf {
        self.base.join("books")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

/// The root of this repository, from the manifest of the crate.
fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the workspace root")
        .to_path_buf()
}

/// Writes a Rust file, creating its directory.
fn write(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("the parent");
    }
    fs::write(path, text).expect("the file");
}

/// Polls a search until it answers what the test waits for.
fn wait_for(
    handle: &gx_index::service::BookHandle,
    query: &str,
    predicate: impl Fn(&SearchOutcome) -> bool,
    what: &str,
) -> SearchOutcome {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut last = SearchOutcome::Empty;
    while Instant::now() < deadline {
        last = handle
            .book()
            .search(&SearchRequest {
                query: query.to_string(),
            })
            .expect("the search runs");
        if predicate(&last) {
            return last;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("the book never answered {what}; last answer: {last:?}");
}

/// The first half of the exit criterion: a symbol search answers in < 50 ms at
/// the scale of this repository.
#[test]
fn a_symbol_search_answers_in_under_fifty_milliseconds_at_this_scale() {
    let scratch = Scratch::new("scale");
    let config = BooksConfig::at(scratch.books());
    let handle = service::ensure(&config, &repository_root()).expect("the book of this repo");

    let progress = wait_for(
        &handle,
        "render_diagram",
        |outcome| !matches!(outcome, SearchOutcome::Empty),
        "the first index of the repository",
    );
    // The index is not empty and it is not a one-file accident.
    let (files, symbols) = handle.book().counts().expect("the counts");
    assert!(
        files > 20,
        "the repository has more than twenty Rust files: {files}"
    );
    assert!(symbols > 100, "the repository has symbols: {symbols}");
    assert_ne!(
        format!("{progress:?}"),
        "Empty",
        "the repository is searchable"
    );

    // The search of a name that this repository declares exactly once.
    let hits = wait_for(
        &handle,
        "tool_schema",
        |outcome| matches!(outcome, SearchOutcome::Hits(_)),
        "one unique symbol",
    );
    let SearchOutcome::Hits(hits) = hits else {
        panic!("expected one answer");
    };
    assert_eq!(hits[0].name, "tool_schema");
    assert!(hits[0].file.ends_with("dispatch.rs"), "{}", hits[0].file);
    assert!(!hits[0].content_hash.is_empty());

    // Cold connection per search (the tool opens a reader), median of 9.
    let mut timings: Vec<Duration> = Vec::new();
    for _ in 0..9 {
        let started = Instant::now();
        let outcome = handle
            .book()
            .search(&SearchRequest {
                query: "tool_schema".to_string(),
            })
            .expect("the search runs");
        timings.push(started.elapsed());
        assert!(matches!(outcome, SearchOutcome::Hits(_)));
    }
    timings.sort();
    let median = timings[timings.len() / 2];
    println!(
        "code_search over {files} files and {symbols} symbols: median {:?}, worst {:?}",
        median,
        timings.last().expect("a timing")
    );
    assert!(
        median < Duration::from_millis(50),
        "the exit criterion is < 50 ms: median {median:?}"
    );

    // Only the book of this repository: closing every book would stop the
    // watcher of a test that is still running in parallel (T4/C3).
    service::shutdown(&repository_root());
}

/// The second half: two writes to the same file, followed by the watcher,
/// without rebuilding the rest of the book.
#[test]
fn the_watcher_follows_two_writes_to_the_same_file() {
    let scratch = Scratch::new("watcher");
    let root = scratch.worktree();
    write(&root, "src/a.rs", "pub fn first_generation() {}\n");
    write(&root, "src/b.rs", "pub fn untouched_neighbour() {}\n");

    let config = BooksConfig::at(scratch.books());
    let handle = service::ensure(&config, &root).expect("the book of the scratch tree");
    let book = handle.book();

    wait_for(
        &handle,
        "first_generation",
        |outcome| matches!(outcome, SearchOutcome::Hits(_)),
        "the first index",
    );
    let neighbour_before: (i64, i64) = book
        .with_writer(|connection| {
            connection.query_row(
                "SELECT id, indexed_at FROM file WHERE path = 'src/b.rs'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
        })
        .expect("the neighbour row");

    // Write one: a new symbol, the old one gone.
    write(&root, "src/a.rs", "pub fn second_generation() {}\n");
    wait_for(
        &handle,
        "second_generation",
        |outcome| matches!(outcome, SearchOutcome::Hits(_)),
        "the first write",
    );
    wait_for(
        &handle,
        "first_generation",
        |outcome| matches!(outcome, SearchOutcome::Empty),
        "the old symbols to be gone",
    );

    // Write two: the same file again.
    write(&root, "src/a.rs", "pub fn third_generation() {}\n");
    wait_for(
        &handle,
        "third_generation",
        |outcome| matches!(outcome, SearchOutcome::Hits(_)),
        "the second write",
    );
    wait_for(
        &handle,
        "second_generation",
        |outcome| matches!(outcome, SearchOutcome::Empty),
        "the second generation to be gone",
    );

    // The book followed the two writes without rebuilding: two chapters, one
    // generation of symbols per chapter, and the neighbour untouched.
    let (files, symbols) = book.counts().expect("the counts");
    assert_eq!(files, 2, "still one chapter per file");
    assert_eq!(symbols, 2, "one symbol per chapter, never three");
    let neighbour_after: (i64, i64) = book
        .with_writer(|connection| {
            connection.query_row(
                "SELECT id, indexed_at FROM file WHERE path = 'src/b.rs'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
        })
        .expect("the neighbour row");
    assert_eq!(
        neighbour_before, neighbour_after,
        "the neighbour file was not touched"
    );

    service::shutdown(&root);
}

// ---------------------------------------------------------------------------
// D2 hard exclusions (found live, 2026-11-06): a git worktree carries `.git`
// as a FILE, and under that condition the `ignore` crate let `target/` in —
// 31 generated `.rs` files of build scripts polluted the book of this very
// repository. D2 says `target/` and `node_modules` are implicit exclusions,
// so they must never depend on how well `.gitignore` happens to parse.
// ---------------------------------------------------------------------------

/// The cold walk never enters `target/` or `node_modules/`, not even in a
/// worktree-like tree where `.git` is a file and `.gitignore` anchors
/// `/target` (D2, S4).
#[test]
fn the_walk_never_enters_target_or_node_modules() {
    let scratch = Scratch::new("hard-exclusions");
    let root = scratch.worktree();
    fs::write(root.join(".gitignore"), "/target\nnode_modules/\n").expect("gitignore");
    // A worktree's `.git` is a file, not a directory.
    fs::write(root.join(".git"), "gitdir: /somewhere/else\n").expect("git file");
    fs::write(root.join("src").join("main.rs"), "fn main() {}\n").expect("src");
    let generated = root
        .join("target")
        .join("debug")
        .join("build")
        .join("x")
        .join("out");
    fs::create_dir_all(&generated).expect("target tree");
    fs::write(generated.join("generated.rs"), "pub fn generated() {}\n").expect("generated");
    let modules = root.join("node_modules").join("pkg");
    fs::create_dir_all(&modules).expect("node_modules tree");
    fs::write(modules.join("index.js"), "module.exports = 1;\n").expect("js");

    let files = gx_index::watcher::scan_tree(&root, &gx_index::IndexOptions::default());
    let paths: Vec<&str> = files.iter().map(|file| file.relative.as_str()).collect();
    assert_eq!(paths, vec!["src/main.rs"], "only the source file is walked");
}
