//! The watcher of the book (S9, D2; ideas §7 y §10.1).
//!
//! The rules the analysis fixed and this module implements:
//!
//! - `.gitignore` is respected through the `ignore` crate, plus the hard
//!   exclusions `.git` and `.graphium` (S4/D2: what git ignores, the book does
//!   not spend the user's resources on).
//! - Binary and oversized files are skipped with a warning (D2).
//! - Parsing runs in parallel and the commits are serial, in batches bounded
//!   by bytes (≤2 MB) and co-located by `(dir, path)`, size-ascending inside a
//!   directory (ideas §10.1, «Lotes»).
//! - Backpressure: at most four parsing threads, and one while a render is in
//!   flight.
//! - `meta.progress` says how far the first index is.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use notify::{RecursiveMode, Watcher};

use crate::book::{Book, BookError, Progress, SkipReason, SkippedFile, MAX_SKIPS_PER_REASON};
use crate::lang::{self, ParsedFile};

/// The byte ceiling of one batch (ideas §10.1: `cache_spill` takes an
/// EXCLUSIVE lock, so the batch decides how long the writer holds it).
pub const MAX_BATCH_BYTES: usize = 2 * 1024 * 1024;

/// The most parsing threads the watcher uses.
pub const MAX_PARSE_THREADS: usize = 4;

/// The consolidation window of the events of an editor.
pub const DEBOUNCE: Duration = Duration::from_millis(400);

/// How long the watcher waits with nothing to do before the `TRUNCATE`
/// checkpoint of the idle exit (ideas §10.1, «Pragmas»).
pub const IDLE_TRUNCATE: Duration = Duration::from_secs(30);

/// The directories the book never walks, whatever `.gitignore` says (D2).
///
/// `target` and `node_modules` are hard exclusions because the build artifacts
/// they hold leak through the event path: rust-analyzer (or any build) writes
/// `target/` while the watcher is armed, the debounced event names a deep
/// directory, and whether the gitignore of the root applies to a walk that
/// starts there must never decide what the book keeps (found live
/// 2026-11-06: 31 generated `.rs` files of build scripts entered the book of
/// this very repository).
pub const HARD_EXCLUSIONS: [&str; 5] = [
    ".git",
    ".graphium",
    ".graphium-book",
    "target",
    "node_modules",
];

/// Renders in flight in this process: the watcher parses with one thread only
/// while there is one (ideas §10.1, «Backpressure»).
static RENDERS_IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

/// The skips of one pass (f28, D3): every file that did not reach the book,
/// with the stable code of its reason, and how many of each.
///
/// Cloning it shares the rows: the filter of a walk is `'static` and records the
/// directories it prunes **while the walk reads**, which is the only place a
/// pruned directory is visible — the walker never yields an entry it skips.
#[derive(Debug, Clone, Default)]
pub struct SkipLog(Arc<Mutex<SkipRows>>);

#[derive(Debug, Default)]
struct SkipRows {
    rows: Vec<SkippedFile>,
    counts: BTreeMap<SkipReason, (usize, i64)>,
}

impl SkipLog {
    /// Records one file that did not reach the book.
    ///
    /// The counts cover every skip; the rows stop at [`MAX_SKIPS_PER_REASON`]
    /// per reason, because the census travels inside the `progress` row and a
    /// tree full of `.md` files is not a finding.
    pub fn record(&self, path: &str, reason: SkipReason) {
        let mut rows = self.0.lock().expect("the skip log lock");
        let entry = rows.counts.entry(reason).or_insert((0, 0));
        entry.1 += 1;
        if entry.0 < MAX_SKIPS_PER_REASON {
            entry.0 += 1;
            rows.rows.push(SkippedFile {
                path: path.to_string(),
                reason,
            });
        }
    }

    /// How many files the pass saw that did not reach the book.
    pub fn total(&self) -> i64 {
        let rows = self.0.lock().expect("the skip log lock");
        rows.counts.values().map(|(_, total)| total).sum()
    }

    /// The rows, capped per reason, in the order the pass found them.
    pub fn rows(&self) -> Vec<SkippedFile> {
        self.0.lock().expect("the skip log lock").rows.clone()
    }

    /// How many skips each code counted, the four always present (D3).
    pub fn by_reason(&self) -> BTreeMap<String, i64> {
        let rows = self.0.lock().expect("the skip log lock");
        SkipReason::ALL
            .iter()
            .map(|reason| {
                let total = rows.counts.get(reason).map_or(0, |(_, total)| *total);
                (reason.code().to_string(), total)
            })
            .collect()
    }

    /// The skips of D2's warning: the unreadable and the oversized ones. A file
    /// with no grammar and a pruned directory are not warnings, they are the
    /// census.
    pub fn warnings(&self) -> i64 {
        let rows = self.0.lock().expect("the skip log lock");
        [SkipReason::TooLarge, SkipReason::Unreadable]
            .iter()
            .map(|reason| rows.counts.get(reason).map_or(0, |(_, total)| *total))
            .sum()
    }
}

/// Publishes the skips of the pass into its `progress` row (f28, D3).
///
/// The row is written once before the first batch and again after each one, so
/// a tool that reads it while the pass runs sees the skips found so far.
fn publish_skips(progress: &mut Progress, skips: &SkipLog) {
    progress.files_failed = skips.total();
    progress.warnings = skips.warnings();
    progress.skipped_by_reason = skips.by_reason();
    progress.skips = skips.rows();
}

/// How many renders are in flight.
pub fn renders_in_flight() -> usize {
    RENDERS_IN_FLIGHT.load(Ordering::SeqCst)
}

/// Marks a render as in flight for as long as it lives (ideas §10.1).
#[derive(Debug)]
pub struct RenderGuard(());

impl RenderGuard {
    /// The render started.
    pub fn enter() -> Self {
        RENDERS_IN_FLIGHT.fetch_add(1, Ordering::SeqCst);
        Self(())
    }
}

impl Drop for RenderGuard {
    fn drop(&mut self) {
        RENDERS_IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Marks a pass as in flight for as long as it lives.
///
/// A pass writes `meta.progress`, so while one is in flight the row a tool
/// reads is honest but not final. The count is what [`Indexer::is_indexing`]
/// answers, and it is the piece that makes a wait on a settling book
/// deterministic instead of a race between a tester's synchronous pass and the
/// watcher's own first pass (the flake of T4/C3).
struct PassInFlight<'a>(&'a Indexer);

impl<'a> PassInFlight<'a> {
    fn enter(indexer: &'a Indexer) -> Self {
        indexer.passes_in_flight.fetch_add(1, Ordering::SeqCst);
        Self(indexer)
    }
}

impl Drop for PassInFlight<'_> {
    fn drop(&mut self) {
        self.0.passes_in_flight.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The knobs of the indexer. `Default` is the production budget.
#[derive(Debug, Clone)]
pub struct IndexOptions {
    pub max_batch_bytes: usize,
    pub max_file_bytes: u64,
    pub parse_threads: usize,
    pub debounce: Duration,
    pub idle_truncate: Duration,
}

impl Default for IndexOptions {
    fn default() -> Self {
        Self {
            max_batch_bytes: MAX_BATCH_BYTES,
            max_file_bytes: lang::MAX_FILE_BYTES,
            parse_threads: MAX_PARSE_THREADS,
            debounce: DEBOUNCE,
            idle_truncate: IDLE_TRUNCATE,
        }
    }
}

/// One file the walk accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    /// Absolute path on disk.
    pub absolute: PathBuf,
    /// Path relative to the root, with `/` separators.
    pub relative: String,
    pub size: u64,
    pub mtime_ns: i64,
}

/// What one pass of the indexer did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IndexSummary {
    pub files: usize,
    pub symbols: i64,
    pub skipped: usize,
    pub removed: usize,
    pub elapsed_ms: u128,
}

/// The indexer of one book: scan, parse, commit, watch.
#[derive(Debug)]
pub struct Indexer {
    book: Arc<Book>,
    root: PathBuf,
    options: IndexOptions,
    /// Full passes ([`Indexer::run_full`]) this indexer has completed.
    passes_done: AtomicU64,
    /// Passes writing the book right now.
    passes_in_flight: AtomicUsize,
}

impl Indexer {
    /// A new indexer for a book rooted at `root` (the canonical worktree).
    pub fn new(book: Arc<Book>, root: PathBuf, options: IndexOptions) -> Self {
        Self {
            book,
            root,
            options,
            passes_done: AtomicU64::new(0),
            passes_in_flight: AtomicUsize::new(0),
        }
    }

    /// How many full passes have finished, counting the one the watcher runs
    /// on its own thread.
    ///
    /// A synchronous pass by a caller does not stop the watcher's first pass:
    /// it runs exactly once per indexer, and the two overlap. A caller that
    /// wants the settled `progress` row waits until this counter has grown by
    /// two and [`Indexer::is_indexing`] is false — after that no pass is left
    /// to overwrite it.
    pub fn passes_done(&self) -> u64 {
        self.passes_done.load(Ordering::SeqCst)
    }

    /// Whether a pass is writing the book (and its `meta.progress` row) right
    /// now.
    pub fn is_indexing(&self) -> bool {
        self.passes_in_flight.load(Ordering::SeqCst) > 0
    }

    /// The options in use.
    pub fn options(&self) -> &IndexOptions {
        &self.options
    }

    /// The book this indexer writes.
    pub fn book(&self) -> &Arc<Book> {
        &self.book
    }

    /// How many threads parse right now.
    pub fn parse_threads(&self) -> usize {
        if renders_in_flight() > 0 {
            1
        } else {
            self.options.parse_threads.clamp(1, MAX_PARSE_THREADS)
        }
    }

    /// Indexes the whole tree: the first pass and the rebuild path.
    pub fn run_full(&self) -> Result<IndexSummary, BookError> {
        let _in_flight = PassInFlight::enter(self);
        let started = Instant::now();
        let skips = SkipLog::default();
        let files = scan_tree_logged(&self.root, &self.options, &skips);
        let on_disk: BTreeSet<String> = files.iter().map(|file| file.relative.clone()).collect();
        let known = self.book.all_paths()?;
        let deletions: Vec<String> = known.difference(&on_disk).cloned().collect();

        let mut progress = Progress {
            state: "indexing".to_string(),
            files_total: files.len() as i64,
            started_at: unix_ms(),
            updated_at: unix_ms(),
            ..Progress::default()
        };
        // D3: what the walk pruned is published before the first chapter is
        // written, so a book with nothing to index still answers why.
        publish_skips(&mut progress, &skips);
        self.book.set_progress(&progress)?;

        let mut summary = IndexSummary {
            removed: deletions.len(),
            ..IndexSummary::default()
        };
        if !deletions.is_empty() {
            let report = self.book.commit_batch(&[], &deletions, unix_ms())?;
            summary.removed = report.files_removed;
        }

        for batch in batches(files, self.options.max_batch_bytes) {
            let sources = self.load_batch(&batch, false, &mut summary, &skips)?;
            let parsed = self.parse_batch(&sources, &mut summary, &skips);
            let report = self.book.commit_batch(&parsed, &[], unix_ms())?;
            record_committed(&skips, &report);
            summary.files += report.files_written;
            summary.symbols += report.symbols;
            summary.skipped += report.files_skipped;
            progress.files_done += batch.len() as i64;
            progress.symbols += report.symbols;
            progress.updated_at = unix_ms();
            publish_skips(&mut progress, &skips);
            self.book.set_progress(&progress)?;
            // The queue is empty here, between batches: this is the only
            // moment a PASSIVE checkpoint is allowed (ideas §10.1).
            let _ = self.book.checkpoint_passive();
        }

        // The row reports the book, not what this pass happened to write: a
        // second pass finds every chapter unchanged and would publish zero
        // symbols, and the census reads this row (f28, D2). A census cannot
        // contradict the count beside it.
        progress.symbols = self.book.counts()?.1;
        progress.state = "indexed".to_string();
        progress.updated_at = unix_ms();
        publish_skips(&mut progress, &skips);
        self.book.set_progress(&progress)?;
        // Counted after the row says `indexed`: a waiter that sees this pass
        // counted sees a pass that has already published its state.
        self.passes_done.fetch_add(1, Ordering::SeqCst);
        summary.elapsed_ms = started.elapsed().as_millis();
        Ok(summary)
    }

    /// Reindexes the direct children of the given directories.
    ///
    /// This is the path of an edit: only the files whose content hash changed
    /// are parsed, and a file that no longer exists loses its chapter. Nothing
    /// else is touched, so an edit never rebuilds the book.
    pub fn sync_dirs(&self, dirs: &[PathBuf]) -> Result<IndexSummary, BookError> {
        // An incremental sync writes `meta.progress` too, so it counts as a
        // pass in flight even though it is not a full one.
        let _in_flight = PassInFlight::enter(self);
        let started = Instant::now();
        let mut summary = IndexSummary::default();
        let mut candidates: Vec<SourceFile> = Vec::new();
        let mut seen: BTreeSet<String> = BTreeSet::new();
        // An incremental sync records its skips but publishes no census: the
        // `progress` census is the one of the last full pass, and replacing it
        // with the handful of files one edit touched would hide the rest.
        let skips = SkipLog::default();

        for dir in dirs {
            let Ok(relative_dir) = dir.strip_prefix(&self.root) else {
                continue;
            };
            let relative_dir = relative_path(relative_dir);
            let on_disk: Vec<SourceFile> = scan_dir_logged(dir, &self.root, &self.options, &skips);
            let local: BTreeSet<String> =
                on_disk.iter().map(|file| file.relative.clone()).collect();

            // A file that was here and is not anymore loses its chapter. Only
            // the direct children of `dir` are considered: the scan above has
            // the same depth.
            let known = self.book.paths_directly_under(&relative_dir)?;
            let deletions: Vec<String> = known.difference(&local).cloned().collect();
            if !deletions.is_empty() {
                let report = self.book.commit_batch(&[], &deletions, unix_ms())?;
                summary.removed += report.files_removed;
            }

            for file in on_disk {
                if seen.insert(file.relative.clone()) {
                    candidates.push(file);
                }
            }
        }

        for batch in batches(candidates, self.options.max_batch_bytes) {
            // Only the files whose bytes changed are parsed: the book stores
            // the content hash, so an edit that does not change the bytes is
            // free and a save without changes costs nothing.
            let sources = self.load_batch(&batch, true, &mut summary, &skips)?;
            if sources.is_empty() {
                continue;
            }
            let parsed = self.parse_batch(&sources, &mut summary, &skips);
            let report = self.book.commit_batch(&parsed, &[], unix_ms())?;
            record_committed(&skips, &report);
            summary.files += report.files_written;
            summary.symbols += report.symbols;
            summary.skipped += report.files_skipped;
            let _ = self.book.checkpoint_passive();
        }

        // The totals of `progress` are the ones of the first pass; an
        // incremental sync only says that the book is current again.
        let mut progress = self.book.progress()?;
        progress.state = "indexed".to_string();
        progress.updated_at = unix_ms();
        self.book.set_progress(&progress)?;
        summary.elapsed_ms = started.elapsed().as_millis();
        Ok(summary)
    }

    /// Reads the bytes of a batch, skipping the binary files with a warning.
    ///
    /// With `skip_unchanged` the book's content hash decides first: a file
    /// whose bytes did not change is never parsed, which is what keeps an edit
    /// from rebuilding the book.
    fn load_batch(
        &self,
        batch: &[SourceFile],
        skip_unchanged: bool,
        summary: &mut IndexSummary,
        skips: &SkipLog,
    ) -> Result<Vec<(SourceFile, Vec<u8>)>, BookError> {
        let mut sources = Vec::with_capacity(batch.len());
        for file in batch {
            let bytes = match std::fs::read(&file.absolute) {
                Ok(bytes) => bytes,
                Err(error) => {
                    eprintln!("gx-index: cannot read `{}`: {error}", file.relative);
                    skips.record(&file.relative, SkipReason::Unreadable);
                    summary.skipped += 1;
                    continue;
                }
            };
            if lang::looks_binary(&bytes) {
                eprintln!("gx-index: skipping `{}`: it looks binary", file.relative);
                skips.record(&file.relative, SkipReason::Unreadable);
                summary.skipped += 1;
                continue;
            }
            if skip_unchanged {
                let existing = self.book.content_hash_of(&file.relative)?;
                if existing.as_deref() == Some(lang::content_hash(&bytes).as_str()) {
                    continue;
                }
            }
            sources.push((file.clone(), bytes));
        }
        Ok(sources)
    }

    /// Parses a batch, in parallel when there is more than one file, and
    /// returns the results in the order of the batch: commits stay serial.
    fn parse_batch(
        &self,
        sources: &[(SourceFile, Vec<u8>)],
        summary: &mut IndexSummary,
        skips: &SkipLog,
    ) -> Vec<ParsedFile> {
        let threads = self.parse_threads().min(sources.len()).max(1);
        if threads == 1 || sources.len() == 1 {
            return sources
                .iter()
                .filter_map(|(file, bytes)| match parse_source(file, bytes) {
                    Ok(parsed) => Some(parsed),
                    Err(reason) => {
                        eprintln!("gx-index: {reason}");
                        skips.record(&file.relative, SkipReason::Unreadable);
                        summary.skipped += 1;
                        None
                    }
                })
                .collect();
        }

        let chunk_size = sources.len().div_ceil(threads);
        let results: Vec<Vec<(String, Result<ParsedFile, String>)>> = std::thread::scope(|scope| {
            let handles: Vec<_> = sources
                .chunks(chunk_size)
                .map(|chunk| {
                    scope.spawn(move || {
                        chunk
                            .iter()
                            .map(|(file, bytes)| (file.relative.clone(), parse_source(file, bytes)))
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap_or_default())
                .collect()
        });

        let mut parsed = Vec::with_capacity(sources.len());
        for chunk in results {
            for (path, file) in chunk {
                match file {
                    Ok(file) => parsed.push(file),
                    Err(reason) => {
                        eprintln!("gx-index: {reason}");
                        skips.record(&path, SkipReason::Unreadable);
                        summary.skipped += 1;
                    }
                }
            }
        }
        parsed
    }

    /// Prunes the books of the user at the idle exit (T4/C3; S8, D4).
    ///
    /// The books root is a cache of worktrees (D1), so when the watcher has had
    /// nothing to do for [`IndexOptions::idle_truncate`] the books of worktrees
    /// that no longer exist are residue. The cap is [`KEEP_NEWEST_BOOKS`] and the
    /// books this process has open are never touched — a book being written has
    /// readers on every tool call — and the fact is **one line on stderr**,
    /// because stdout is JSON-RPC.
    fn prune_stale_books(&self) {
        let Some(books_root) = self.book.books_root() else {
            return;
        };
        let mut protected = crate::service::open_roots();
        if !protected.iter().any(|root| root == &self.root) {
            protected.push(self.root.clone());
        }
        match books_root.prune(&protected, crate::location::KEEP_NEWEST_BOOKS, false) {
            Ok(outcome) if !outcome.removed.is_empty() => {
                let keys: Vec<&str> = outcome
                    .removed
                    .iter()
                    .map(|candidate| candidate.entry.book_key.as_str())
                    .collect();
                eprintln!(
                    "gx-index: pruned {} book(s) of worktrees that no longer exist ({} bytes): {}",
                    outcome.removed.len(),
                    outcome.bytes_freed(),
                    keys.join(", ")
                );
            }
            Ok(_) => {}
            Err(error) => eprintln!("gx-index: cannot prune the stale books: {error}"),
        }
    }

    /// Runs the first pass and reports a failure once, on stderr (stdout is
    /// JSON-RPC, PRD §8.6.1).
    fn first_pass(&self) {
        if let Err(error) = self.run_full() {
            eprintln!("gx-index: the first index failed: {error}");
        }
    }

    /// Watches the root until the handle is stopped.
    ///
    /// The first full pass runs here, inside the thread, so opening a book
    /// never blocks a tool call; the searches that arrive meanwhile see the
    /// `progress` row and know the answer is partial.
    pub fn start(self: Arc<Self>) -> Result<WatcherHandle, BookError> {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();

        let join = std::thread::Builder::new()
            .name("gx-index-watcher".to_string())
            .spawn(move || {
                // The watcher is registered before the first pass, so an edit
                // that lands while the tree is being indexed is queued instead
                // of lost, and `ensure` does not wait for the whole pass.
                let (tx, rx) = mpsc::channel();
                let watcher = notify::recommended_watcher(move |event| {
                    let _ = tx.send(event);
                });
                let mut watcher = match watcher {
                    Ok(watcher) => watcher,
                    Err(error) => {
                        // Without a watcher the book is still worth having:
                        // it is indexed once and stays as it is.
                        let _ = ready_tx.send(Err(error.to_string()));
                        self.first_pass();
                        return;
                    }
                };
                if let Err(error) = watcher.watch(&self.root, RecursiveMode::Recursive) {
                    let _ = ready_tx.send(Err(error.to_string()));
                    self.first_pass();
                    return;
                }
                let _ = ready_tx.send(Ok(()));

                self.first_pass();

                let mut pending: BTreeSet<PathBuf> = BTreeSet::new();
                let mut last_work = Instant::now();
                while !thread_stop.load(Ordering::SeqCst) {
                    match rx.recv_timeout(self.options.debounce) {
                        Ok(Ok(event)) => {
                            if !changes_content(&event.kind) {
                                continue;
                            }
                            for path in event.paths {
                                pending.insert(path);
                            }
                        }
                        Ok(Err(error)) => {
                            eprintln!("gx-index: the watcher reported an error: {error}");
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            if pending.is_empty() {
                                if last_work.elapsed() >= self.options.idle_truncate {
                                    let _ = self.book.checkpoint_truncate();
                                    // The idle exit also prunes the books of the
                                    // worktrees that no longer exist (T4/C3).
                                    self.prune_stale_books();
                                    last_work = Instant::now();
                                }
                                continue;
                            }
                            let dirs = parent_dirs(&self.root, &pending);
                            pending.clear();
                            if let Err(error) = self.sync_dirs(&dirs) {
                                eprintln!("gx-index: the incremental sync failed: {error}");
                            }
                            last_work = Instant::now();
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
                drop(watcher);
                let _ = self.book.checkpoint_truncate();
            })
            .map_err(BookError::Io)?;

        match ready_rx.recv_timeout(Duration::from_secs(30)) {
            Ok(Ok(())) => {}
            Ok(Err(error)) => eprintln!("gx-index: the watcher could not start: {error}"),
            Err(_) => eprintln!("gx-index: the watcher did not announce itself in time"),
        }

        Ok(WatcherHandle {
            stop,
            join: Some(join),
        })
    }
}

/// The handle that stops a watcher thread.
#[derive(Debug)]
pub struct WatcherHandle {
    stop: Arc<AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl WatcherHandle {
    /// Asks the thread to stop and waits for it.
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// Walks the whole root, respecting `.gitignore` (D3).
pub fn scan_tree(root: &Path, options: &IndexOptions) -> Vec<SourceFile> {
    scan_tree_logged(root, options, &SkipLog::default())
}

/// Walks the whole root and records every file it prunes (f28, D3).
pub fn scan_tree_logged(root: &Path, options: &IndexOptions, skips: &SkipLog) -> Vec<SourceFile> {
    let mut builder = ignore::WalkBuilder::new(root);
    builder
        .standard_filters(true)
        // The book works without git, so `.gitignore` applies outside a repo
        // too: D3 is about what git ignores, not about needing git.
        .require_git(false)
        .follow_links(false)
        .filter_entry(prune_filter(root, skips));
    collect(builder, root, options, None, skips)
}

/// Walks the direct children of one directory (the incremental path).
pub fn scan_dir(dir: &Path, root: &Path, options: &IndexOptions) -> Vec<SourceFile> {
    scan_dir_logged(dir, root, options, &SkipLog::default())
}

/// Walks one directory and records every file it prunes (f28, D3).
pub fn scan_dir_logged(
    dir: &Path,
    root: &Path,
    options: &IndexOptions,
    skips: &SkipLog,
) -> Vec<SourceFile> {
    if !dir.exists() {
        return Vec::new();
    }
    let mut builder = ignore::WalkBuilder::new(dir);
    builder
        .standard_filters(true)
        .require_git(false)
        .follow_links(false)
        .max_depth(Some(1))
        .filter_entry(prune_filter(root, skips));
    collect(builder, root, options, Some(dir), skips)
}

/// The filter of a walk: the hard exclusions, recorded as `ignored` (f28, D3).
///
/// The walker never yields an entry it prunes, so this closure is the only
/// place a pruned directory is visible. The row is the directory, never one row
/// per generated file inside it: `target/` is one skip, not the thirty-one
/// build scripts behind it.
fn prune_filter(
    root: &Path,
    skips: &SkipLog,
) -> impl Fn(&ignore::DirEntry) -> bool + Send + Sync + 'static {
    let root = root.to_path_buf();
    let skips = skips.clone();
    move |entry| {
        if !is_hard_excluded(entry.path()) {
            return true;
        }
        let path = entry
            .path()
            .strip_prefix(&root)
            .map(relative_path)
            .unwrap_or_else(|_| relative_path(entry.path()));
        skips.record(&path, SkipReason::Ignored);
        false
    }
}

/// Records the chapters a commit refused, so the census names them too.
fn record_committed(skips: &SkipLog, report: &crate::book::CommitReport) {
    for skip in &report.skips {
        skips.record(&skip.path, skip.reason);
    }
}

/// Collects the accepted files of a walk.
fn collect(
    builder: ignore::WalkBuilder,
    root: &Path,
    options: &IndexOptions,
    only_dir: Option<&Path>,
    skips: &SkipLog,
) -> Vec<SourceFile> {
    let mut files = Vec::new();
    for entry in builder.build().flatten() {
        let path = entry.path();
        if let Some(dir) = only_dir {
            if path == dir {
                continue;
            }
        }
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let Ok(relative_to_root) = path.strip_prefix(root) else {
            continue;
        };
        let relative = relative_path(relative_to_root);
        // D3: a file the build has no grammar for is not a silent hole; it
        // carries `unsupported_language` like the rest of the census.
        if lang::language_for(path).is_none() {
            skips.record(&relative, SkipReason::UnsupportedLanguage);
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            skips.record(&relative, SkipReason::Unreadable);
            continue;
        };
        if metadata.len() > options.max_file_bytes {
            eprintln!(
                "gx-index: skipping `{}`: {} bytes is over the ceiling of {}",
                path.display(),
                metadata.len(),
                options.max_file_bytes
            );
            skips.record(&relative, SkipReason::TooLarge);
            continue;
        }
        files.push(SourceFile {
            absolute: path.to_path_buf(),
            relative,
            size: metadata.len(),
            mtime_ns: mtime_ns(&metadata),
        });
    }
    files
}

/// Whether an event changes what the book should hold (T4/C3).
///
/// An **access is not an edit**: opening a file changes nothing. It matters
/// because the watcher's own scan opens every directory and file it walks, so
/// the access events of one sync scheduled the next one — the loop never
/// drained, and the idle exit (`IDLE_TRUNCATE`, where the prune of T4/C3 lives)
/// never happened. Found with the idle prune, fixed here: the book follows
/// writes, creates and removals, and ignores readers.
pub fn changes_content(kind: &notify::EventKind) -> bool {
    !matches!(kind, notify::EventKind::Access(_))
}

/// Whether a path component is one of the hard exclusions (D2).
pub fn is_hard_excluded(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(
            component.as_os_str().to_str(),
            Some(name) if HARD_EXCLUSIONS.contains(&name)
        )
    })
}

/// A path with `/` separators, relative to the root.
fn relative_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// `mtime` in nanoseconds since the epoch.
fn mtime_ns(metadata: &std::fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|elapsed| elapsed.as_nanos() as i64)
        .unwrap_or(0)
}

/// Unix epoch milliseconds.
fn unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

/// Packs the files into batches bounded by bytes, co-located by directory and
/// size-ascending inside each directory (ideas §10.1, «Lotes»).
pub fn batches(files: Vec<SourceFile>, max_batch_bytes: usize) -> Vec<Vec<SourceFile>> {
    let mut by_dir: BTreeMap<PathBuf, Vec<SourceFile>> = BTreeMap::new();
    for file in files {
        let dir = file
            .absolute
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        by_dir.entry(dir).or_default().push(file);
    }
    let mut ordered: Vec<SourceFile> = Vec::new();
    for (_, mut group) in by_dir {
        group.sort_by(|left, right| {
            left.size
                .cmp(&right.size)
                .then_with(|| left.relative.cmp(&right.relative))
        });
        ordered.extend(group);
    }

    let mut batches: Vec<Vec<SourceFile>> = Vec::new();
    let mut current: Vec<SourceFile> = Vec::new();
    let mut current_bytes = 0usize;
    for file in ordered {
        let size = file.size as usize;
        if !current.is_empty() && current_bytes + size > max_batch_bytes {
            batches.push(std::mem::take(&mut current));
            current_bytes = 0;
        }
        current_bytes += size;
        current.push(file);
    }
    if !current.is_empty() {
        batches.push(current);
    }
    batches
}

/// Parses the bytes already read for one source file.
fn parse_source(file: &SourceFile, bytes: &[u8]) -> Result<ParsedFile, String> {
    match lang::parse(&file.relative, bytes, file.mtime_ns) {
        Ok(parsed) => Ok(parsed),
        Err(error) => Err(format!("cannot parse `{}`: {error}", file.relative)),
    }
}

/// The parent directories of the event paths, deduplicated, inside the root.
fn parent_dirs(root: &Path, paths: &BTreeSet<PathBuf>) -> Vec<PathBuf> {
    let mut dirs: BTreeSet<PathBuf> = BTreeSet::new();
    for path in paths {
        let absolute = if path.is_absolute() {
            path.clone()
        } else {
            root.join(path)
        };
        if !absolute.starts_with(root) {
            continue;
        }
        // A build artifact never schedules a sync, even as an event.
        if is_hard_excluded(&absolute) {
            continue;
        }
        let parent = if absolute.is_dir() {
            absolute.clone()
        } else {
            absolute
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| root.to_path_buf())
        };
        // The root itself is a valid target: a file at the top of the
        // worktree is a direct child of it.
        if parent.starts_with(root) {
            dirs.insert(parent);
        }
    }
    dirs.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;

    use super::{batches, is_hard_excluded, IndexOptions, Indexer, SourceFile};
    use crate::book::{Book, Progress};
    use crate::location::BooksRoot;

    fn file(dir: &str, name: &str, size: u64) -> SourceFile {
        SourceFile {
            absolute: PathBuf::from(dir).join(name),
            relative: format!("{}/{name}", dir.trim_start_matches('/')),
            size,
            mtime_ns: 0,
        }
    }

    /// Ideas §10.1, «Lotes»: co-located by directory and size-ascending inside
    /// it, bounded by bytes.
    #[test]
    fn the_batches_group_by_directory_and_bound_the_bytes() {
        let files = vec![
            file("/r/b", "big.rs", 900),
            file("/r/a", "two.rs", 400),
            file("/r/a", "one.rs", 100),
            file("/r/a", "three.rs", 500),
        ];
        let batches = batches(files, 1000);
        let order: Vec<String> = batches
            .iter()
            .flat_map(|batch| batch.iter().map(|file| file.relative.clone()))
            .collect();
        assert_eq!(
            order,
            vec!["r/a/one.rs", "r/a/two.rs", "r/a/three.rs", "r/b/big.rs"]
        );
        for batch in &batches {
            let bytes: u64 = batch.iter().map(|file| file.size).sum();
            assert!(bytes <= 1000 || batch.len() == 1, "{bytes} in one batch");
        }
    }

    #[test]
    fn a_file_larger_than_the_batch_still_gets_a_batch_of_its_own() {
        let batches = batches(vec![file("/r", "huge.rs", 5000)], 1000);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 1);
    }

    /// T4/C3: the watcher follows writes, creates and removals; a reader is not
    /// an editor, and the watcher's own scan reads every file it walks.
    #[test]
    fn an_access_is_not_an_edit() {
        use notify::event::{
            AccessKind, AccessMode, CreateKind, DataChange, ModifyKind, RemoveKind,
        };
        use notify::EventKind;

        for access in [
            AccessKind::Open(AccessMode::Any),
            AccessKind::Read,
            AccessKind::Close(AccessMode::Write),
        ] {
            assert!(
                !super::changes_content(&EventKind::Access(access)),
                "an access changes nothing: {access:?}"
            );
        }
        for changed in [
            EventKind::Create(CreateKind::File),
            EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            EventKind::Remove(RemoveKind::File),
        ] {
            assert!(super::changes_content(&changed), "{changed:?} is an edit");
        }
    }

    #[test]
    fn the_hard_exclusions_are_never_indexed() {
        assert!(is_hard_excluded(
            PathBuf::from("/r/.git/objects/x.rs").as_path()
        ));
        assert!(is_hard_excluded(
            PathBuf::from("/r/.graphium/libro.sqlite").as_path()
        ));
        assert!(!is_hard_excluded(PathBuf::from("/r/src/main.rs").as_path()));
    }

    // -----------------------------------------------------------------
    // f28 — the census of the book and why a file is missing (D1, D3)
    // -----------------------------------------------------------------

    /// A worktree with its books root inside the system temp directory,
    /// removed when the test ends (nothing here touches the repository).
    struct Scratch {
        base: PathBuf,
        root: PathBuf,
        books: PathBuf,
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static COUNTER: AtomicUsize = AtomicUsize::new(0);
            let base = std::env::temp_dir().join(format!(
                "gx-census-{name}-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::SeqCst)
            ));
            let root = base.join("worktree");
            fs::create_dir_all(root.join("src")).expect("the worktree");
            Self {
                books: base.join("books"),
                base,
                root,
            }
        }

        fn write(&self, relative: &str, text: &str) {
            self.write_bytes(relative, text.as_bytes());
        }

        fn write_bytes(&self, relative: &str, bytes: &[u8]) {
            let path = self.root.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("the parent dir");
            }
            fs::write(path, bytes).expect("the file");
        }

        fn canonical_root(&self) -> PathBuf {
            fs::canonicalize(&self.root).expect("canonical root")
        }

        fn open(&self) -> Book {
            let books = BooksRoot::at(&self.books);
            let location = books.locate(&self.root).expect("locate");
            Book::open(location, &books.registry_path()).expect("open the book")
        }

        fn indexer(&self, book: &Arc<Book>) -> Indexer {
            Indexer::new(book.clone(), self.canonical_root(), IndexOptions::default())
        }

        fn index(&self, book: &Arc<Book>) {
            self.indexer(book).run_full().expect("the first pass");
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }

    /// The stable code of the reason one path carries, or a panic that names
    /// what the pass did publish.
    fn reason_of(progress: &Progress, path: &str) -> &'static str {
        progress
            .skips
            .iter()
            .find(|skip| skip.path == path)
            .unwrap_or_else(|| panic!("`{path}` is not in the skips: {:?}", progress.skips))
            .reason
            .code()
    }

    /// D3: a file over the 1 MiB ceiling of the walk does not reach the book,
    /// and the pass says so with a stable code instead of a silent hole.
    #[test]
    fn a_file_over_the_ceiling_is_published_as_too_large() {
        let scratch = Scratch::new("too-large");
        scratch.write("src/kept.rs", "pub fn kept() {}\n");
        scratch.write_bytes(
            "src/generated.rs",
            &vec![b'/'; crate::lang::MAX_FILE_BYTES as usize + 1],
        );
        let book = Arc::new(scratch.open());
        scratch.index(&book);

        let progress = book.progress().expect("the progress row");
        assert_eq!(reason_of(&progress, "src/generated.rs"), "too_large");
        assert_eq!(
            progress.files_failed, 1,
            "one skip, and the chapter that fits is not one: {progress:?}"
        );
        assert_eq!(book.counts().expect("the counts").0, 1);
    }

    /// D3: a file whose extension no grammar of this build answers is not a
    /// silent hole either: it carries `unsupported_language`.
    #[test]
    fn a_file_with_no_grammar_is_published_as_unsupported_language() {
        let scratch = Scratch::new("unsupported");
        scratch.write("src/kept.rs", "pub fn kept() {}\n");
        scratch.write("docs/notes.md", "# a file no grammar of this build reads\n");
        let book = Arc::new(scratch.open());
        scratch.index(&book);

        let progress = book.progress().expect("the progress row");
        assert_eq!(
            reason_of(&progress, "docs/notes.md"),
            "unsupported_language"
        );
        assert_eq!(progress.files_failed, 1, "{progress:?}");
    }

    /// D3: a `target/` subtree is one row of the census, never one row per
    /// generated file: the walk prunes it whole and the code says why.
    #[test]
    fn a_directory_the_book_never_walks_is_published_as_ignored() {
        let scratch = Scratch::new("ignored");
        scratch.write("src/kept.rs", "pub fn kept() {}\n");
        scratch.write("target/generated.rs", "pub fn generated() {}\n");
        let book = Arc::new(scratch.open());
        scratch.index(&book);

        let progress = book.progress().expect("the progress row");
        assert_eq!(reason_of(&progress, "target"), "ignored");
        assert_eq!(
            progress.files_failed, 1,
            "the pruned directory is one skip: {progress:?}"
        );
    }

    /// D3: a chapter the book could not turn into source keeps a code too. A
    /// binary that reached the batch is the deterministic case; a file that no
    /// longer reads is the same code.
    #[test]
    fn a_binary_chapter_is_published_as_unreadable() {
        let scratch = Scratch::new("unreadable");
        scratch.write_bytes("src/blob.rs", b"pub fn hidden\x00\x01");
        let book = Arc::new(scratch.open());
        scratch.index(&book);

        let progress = book.progress().expect("the progress row");
        assert_eq!(reason_of(&progress, "src/blob.rs"), "unreadable");
        assert_eq!(progress.files_failed, 1, "{progress:?}");
    }

    /// D1/S3: the rate publishes whether it may be compared at all. `shell`
    /// carries no runtime-name list, so its rate is measured on a different
    /// denominator than `python`'s, and the answer says which is which.
    #[test]
    fn the_rate_says_which_languages_are_comparable() {
        let scratch = Scratch::new("comparable");
        scratch.write("src/run.sh", "main() {\n  echo hi\n}\n");
        scratch.write("src/app.py", "def main():\n    return len('x')\n");
        let book = Arc::new(scratch.open());
        scratch.index(&book);

        let report = book.resolution().expect("the honesty report");
        let row = |lang: &str| {
            report
                .iter()
                .find(|row| row.lang == lang)
                .unwrap_or_else(|| panic!("the report has no `{lang}` row: {report:?}"))
                .clone()
        };
        // S3: no runtime list, so the rate is over a wider denominator and may
        // not be compared with the languages that carry one.
        let shell = row("shell");
        assert!(!shell.comparable, "`shell` carries no list: {shell:?}");
        // `python` carries the list of its runtime.
        let python = row("python");
        assert!(python.comparable, "`python` carries the list: {python:?}");
        for row in [&shell, &python] {
            assert!((0.0..=100.0).contains(&row.percent()), "{row:?}");
        }
    }

    /// f28/D2: `progress.symbols` is the symbols of the book, not what the last
    /// pass happened to write.
    ///
    /// The live product answered `{ files_total: 163, files_done: 163, state:
    /// "indexed", symbols: 0 }`: the watcher's own first pass runs right after
    /// any pass of the caller, finds every chapter unchanged (`write_chapter`
    /// reports `symbols: 0` for an unchanged hash) and published a zero. The
    /// row a tool copies into every book answer has to count the book.
    #[test]
    fn the_progress_row_counts_the_symbols_of_the_book_once_it_is_indexed() {
        let scratch = Scratch::new("symbols");
        scratch.write("src/a.rs", "pub fn one() {}\npub fn two() {}\n");
        scratch.write("src/b.rs", "pub struct Three;\n");
        let book = Arc::new(scratch.open());
        let indexer = scratch.indexer(&book);
        indexer.run_full().expect("the first pass");
        // The live shape: a second full pass, with every chapter unchanged.
        indexer.run_full().expect("the second pass");

        let (_, symbols) = book.counts().expect("the counts");
        assert_eq!(symbols, 3, "the fixture declares three symbols");
        let progress = book.progress().expect("the progress row");
        assert_eq!(
            progress.symbols, symbols,
            "the row counts the book, not this pass: {progress:?}"
        );
        assert!(progress.symbols > 0, "{progress:?}");
    }

    /// D3: the sample of `progress.skips` is capped per reason, and the count
    /// keeps covering every skip, so the cap never hides a problem.
    #[test]
    fn the_skips_of_one_reason_are_published_up_to_the_cap() {
        let scratch = Scratch::new("cap");
        let total = crate::book::MAX_SKIPS_PER_REASON + 1;
        for index in 0..total {
            scratch.write(&format!("docs/note-{index}.md"), "# notes\n");
        }
        let book = Arc::new(scratch.open());
        scratch.index(&book);

        let progress = book.progress().expect("the progress row");
        assert_eq!(progress.files_failed, total as i64, "{progress:?}");
        assert_eq!(progress.skips.len(), crate::book::MAX_SKIPS_PER_REASON);
        assert_eq!(
            progress.skipped_by_reason.get("unsupported_language"),
            Some(&(total as i64)),
            "{progress:?}"
        );
    }
}
