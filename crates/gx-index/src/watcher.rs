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
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use notify::{RecursiveMode, Watcher};

use crate::book::{Book, BookError, Progress};
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
pub const HARD_EXCLUSIONS: [&str; 3] = [".git", ".graphium", ".graphium-book"];

/// Renders in flight in this process: the watcher parses with one thread only
/// while there is one (ideas §10.1, «Backpressure»).
static RENDERS_IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

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
}

impl Indexer {
    /// A new indexer for a book rooted at `root` (the canonical worktree).
    pub fn new(book: Arc<Book>, root: PathBuf, options: IndexOptions) -> Self {
        Self {
            book,
            root,
            options,
        }
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
        let started = Instant::now();
        let files = scan_tree(&self.root, &self.options);
        let on_disk: BTreeSet<String> = files.iter().map(|file| file.relative.clone()).collect();
        let known = self.book.all_paths()?;
        let deletions: Vec<String> = known.difference(&on_disk).cloned().collect();

        let mut progress = Progress {
            state: "indexing".to_string(),
            files_total: files.len() as i64,
            files_done: 0,
            files_failed: 0,
            symbols: 0,
            started_at: unix_ms(),
            updated_at: unix_ms(),
            warnings: 0,
        };
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
            let sources = self.load_batch(&batch, false, &mut summary)?;
            let parsed = self.parse_batch(&sources, &mut summary);
            let report = self.book.commit_batch(&parsed, &[], unix_ms())?;
            summary.files += report.files_written;
            summary.symbols += report.symbols;
            summary.skipped += report.files_skipped;
            progress.files_done += batch.len() as i64;
            progress.files_failed = summary.skipped as i64;
            progress.symbols += report.symbols;
            progress.updated_at = unix_ms();
            self.book.set_progress(&progress)?;
            // The queue is empty here, between batches: this is the only
            // moment a PASSIVE checkpoint is allowed (ideas §10.1).
            let _ = self.book.checkpoint_passive();
        }

        progress.state = "indexed".to_string();
        progress.updated_at = unix_ms();
        self.book.set_progress(&progress)?;
        summary.elapsed_ms = started.elapsed().as_millis();
        Ok(summary)
    }

    /// Reindexes the direct children of the given directories.
    ///
    /// This is the path of an edit: only the files whose content hash changed
    /// are parsed, and a file that no longer exists loses its chapter. Nothing
    /// else is touched, so an edit never rebuilds the book.
    pub fn sync_dirs(&self, dirs: &[PathBuf]) -> Result<IndexSummary, BookError> {
        let started = Instant::now();
        let mut summary = IndexSummary::default();
        let mut candidates: Vec<SourceFile> = Vec::new();
        let mut seen: BTreeSet<String> = BTreeSet::new();

        for dir in dirs {
            let Ok(relative_dir) = dir.strip_prefix(&self.root) else {
                continue;
            };
            let relative_dir = relative_path(relative_dir);
            let on_disk: Vec<SourceFile> = scan_dir(dir, &self.root, &self.options);
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
            let sources = self.load_batch(&batch, true, &mut summary)?;
            if sources.is_empty() {
                continue;
            }
            let parsed = self.parse_batch(&sources, &mut summary);
            let report = self.book.commit_batch(&parsed, &[], unix_ms())?;
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
    ) -> Result<Vec<(SourceFile, Vec<u8>)>, BookError> {
        let mut sources = Vec::with_capacity(batch.len());
        for file in batch {
            let bytes = match std::fs::read(&file.absolute) {
                Ok(bytes) => bytes,
                Err(error) => {
                    eprintln!("gx-index: cannot read `{}`: {error}", file.relative);
                    summary.skipped += 1;
                    continue;
                }
            };
            if lang::looks_binary(&bytes) {
                eprintln!("gx-index: skipping `{}`: it looks binary", file.relative);
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
    ) -> Vec<ParsedFile> {
        let threads = self.parse_threads().min(sources.len()).max(1);
        if threads == 1 || sources.len() == 1 {
            return sources
                .iter()
                .filter_map(|(file, bytes)| match parse_source(file, bytes) {
                    Ok(parsed) => Some(parsed),
                    Err(reason) => {
                        eprintln!("gx-index: {reason}");
                        summary.skipped += 1;
                        None
                    }
                })
                .collect();
        }

        let chunk_size = sources.len().div_ceil(threads);
        let results: Vec<Vec<Option<ParsedFile>>> = std::thread::scope(|scope| {
            let handles: Vec<_> = sources
                .chunks(chunk_size)
                .map(|chunk| {
                    scope.spawn(move || {
                        chunk
                            .iter()
                            .map(|(file, bytes)| parse_source(file, bytes).ok())
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
            for file in chunk {
                match file {
                    Some(file) => parsed.push(file),
                    None => summary.skipped += 1,
                }
            }
        }
        parsed
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
    let mut builder = ignore::WalkBuilder::new(root);
    builder
        .standard_filters(true)
        // The book works without git, so `.gitignore` applies outside a repo
        // too: D3 is about what git ignores, not about needing git.
        .require_git(false)
        .follow_links(false)
        .filter_entry(|entry| !is_hard_excluded(entry.path()));
    collect(builder, root, options, None)
}

/// Walks the direct children of one directory (the incremental path).
pub fn scan_dir(dir: &Path, root: &Path, options: &IndexOptions) -> Vec<SourceFile> {
    if !dir.exists() {
        return Vec::new();
    }
    let mut builder = ignore::WalkBuilder::new(dir);
    builder
        .standard_filters(true)
        .require_git(false)
        .follow_links(false)
        .max_depth(Some(1))
        .filter_entry(|entry| !is_hard_excluded(entry.path()));
    collect(builder, root, options, Some(dir))
}

/// Collects the accepted files of a walk.
fn collect(
    builder: ignore::WalkBuilder,
    root: &Path,
    options: &IndexOptions,
    only_dir: Option<&Path>,
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
        if lang::language_for(path).is_none() {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.len() > options.max_file_bytes {
            eprintln!(
                "gx-index: skipping `{}`: {} bytes is over the ceiling of {}",
                path.display(),
                metadata.len(),
                options.max_file_bytes
            );
            continue;
        }
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        files.push(SourceFile {
            absolute: path.to_path_buf(),
            relative: relative_path(relative),
            size: metadata.len(),
            mtime_ns: mtime_ns(&metadata),
        });
    }
    files
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
    use std::path::PathBuf;

    use super::{batches, is_hard_excluded, SourceFile};

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
}
