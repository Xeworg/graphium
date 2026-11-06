//! `graphium books list|prune|hook` (F9, T4/C3; S8, D4).
//!
//! The books of the user live in `~/.graphium/books` and are a **cache of
//! worktrees** (D1): this is the human door to them. `list` shows what is there
//! and marks the books whose worktree no longer exists; `prune` deletes exactly
//! those (and the registry lines whose directory is gone), never the book of a
//! live project; `hook` prints the optional `post-checkout`/`post-merge` hook of
//! ideas P8, which only ever runs the prune with `--dry-run`.
//!
//! The lines are built by pure functions and printed by `main`, so the words a
//! human reads are checked by tests without capturing a child process.

use std::error::Error;
use std::path::PathBuf;

use gx_mcp::book::{BooksRoot, PruneOutcome, KEEP_NEWEST_BOOKS};

/// What `graphium books` answers when it does not understand the request.
pub const USAGE: &str = "Uso:\n  \
     graphium books list                  lista los libros del usuario\n  \
     graphium books prune [--dry-run]     borra los libros de worktrees que ya no existen\n  \
     graphium books hook                  imprime el hook de git (post-checkout/post-merge)";

/// The snippet `graphium books hook` prints (T4/C3, ideas P8).
///
/// A `post-checkout`/`post-merge` that refreshes the book of the worktree and,
/// with `--dry-run`, only *reports* the books left behind by worktrees that no
/// longer exist: installing a hook can never delete a book by surprise.
pub const HOOK_SNIPPET: &str = "#!/bin/sh\n\
# graphium (F9): guardá este archivo como .git/hooks/post-checkout y como\n\
# .git/hooks/post-merge, con `chmod +x`. Tras un checkout o un merge el\n\
# worktree cambia y el libro del proyecto se refresca solo; el prune con\n\
# --dry-run solo reporta los libros de worktrees que ya no existen, así que\n\
# instalarlo nunca borra nada.\n\
graphium books prune --dry-run\n";

/// The lines `graphium books <arguments>` prints.
///
/// `books_root` is a parameter (and not read from the environment here) so the
/// output and the deletions can be checked against a books root of the test.
pub fn lines(books_root: &BooksRoot, arguments: &[String]) -> Result<Vec<String>, Box<dyn Error>> {
    match arguments.first().map(String::as_str) {
        Some("list") => {
            if let Some(extra) = arguments.get(1) {
                return Err(format!("unknown argument for books list: {extra}").into());
            }
            Ok(list_lines(books_root))
        }
        Some("prune") => {
            let mut dry_run = false;
            for argument in &arguments[1..] {
                match argument.as_str() {
                    "--dry-run" => dry_run = true,
                    other => {
                        return Err(
                            format!("graphium: unknown argument for books prune: {other}").into(),
                        )
                    }
                }
            }
            let outcome = books_root.prune(&protected_roots(), KEEP_NEWEST_BOOKS, dry_run)?;
            Ok(prune_lines(&outcome))
        }
        Some("hook") => Ok(HOOK_SNIPPET.lines().map(str::to_string).collect()),
        Some(other) => Err(format!("unknown books command: {other}\n{USAGE}").into()),
        None => Err(format!("`books` necesita un comando\n{USAGE}").into()),
    }
}

/// The books the prune never touches: the one of the directory the human is
/// standing in. The idle exit of the owner protects the books it has open, and
/// the CLI cannot know them (they live in another process), so it protects the
/// root of the current directory — the book a distracted prune must not delete
/// while it is being used.
fn protected_roots() -> Vec<PathBuf> {
    std::env::current_dir().ok().into_iter().collect()
}

/// The table of `graphium books list`: key, root, the totals of the last open,
/// the bytes on disk and the state of each book.
pub fn list_lines(books_root: &BooksRoot) -> Vec<String> {
    let mut lines = vec![format!(
        "{:<34} {:>8} {:>9} {:>9}  {:<20}  {}",
        "LIBRO", "ARCHIVOS", "SÍMBOLOS", "BYTES", "ÚLTIMA APERTURA", "RAÍZ"
    )];
    let books = books_root.list();
    if books.is_empty() {
        lines.push("(no hay libros)".to_string());
        return lines;
    }
    for summary in books {
        let entry = &summary.entry;
        let state = if summary.dir_missing {
            "  (falta el directorio)"
        } else if summary.root_missing {
            "  (falta la raíz)"
        } else {
            ""
        };
        lines.push(format!(
            "{:<34} {:>8} {:>9} {:>9}  {:<20}  {}{}",
            entry.book_key,
            entry.files,
            entry.symbols,
            bytes_text(summary.bytes),
            if entry.last_opened_at > 0 {
                gx_mcp::book::iso8601_utc(entry.last_opened_at)
            } else {
                "-".to_string()
            },
            if entry.root.is_empty() {
                "-"
            } else {
                &entry.root
            },
            state
        ));
    }
    lines
}

/// What a prune did (or would do): one line per book and one summary line.
pub fn prune_lines(outcome: &PruneOutcome) -> Vec<String> {
    let mut lines: Vec<String> = outcome
        .removed
        .iter()
        .map(|candidate| {
            format!(
                "{}  {:<9}  {:>9}  {}",
                candidate.entry.book_key,
                candidate.reason.as_str(),
                bytes_text(candidate.bytes),
                candidate.entry.root
            )
        })
        .collect();
    lines.push(format!(
        "graphium: {} libro(s) {}, {} en el registro ({})",
        outcome.removed.len(),
        if outcome.dry_run {
            "se borrarían"
        } else {
            "borrados"
        },
        outcome.kept,
        bytes_text(outcome.bytes_freed())
    ));
    lines
}

/// Bytes as a human reads them (binary units: the books are files).
fn bytes_text(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    let bytes_f = bytes as f64;
    if bytes_f >= MIB {
        format!("{:.1} MB", bytes_f / MIB)
    } else if bytes_f >= KIB {
        format!("{:.1} kB", bytes_f / KIB)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use gx_mcp::book::{BookEntry, BookRegistry, BooksRoot, KEEP_NEWEST_BOOKS};

    use super::{lines, list_lines, prune_lines, HOOK_SNIPPET};

    /// A books root of its own, removed when the test ends.
    struct Scratch {
        base: PathBuf,
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let base = std::env::temp_dir().join(format!(
                "gx-app-books-{name}-{}-{:?}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|elapsed| elapsed.as_nanos())
                    .unwrap_or(0)
            ));
            fs::create_dir_all(&base).expect("the books root");
            Self { base }
        }

        fn books(&self) -> BooksRoot {
            BooksRoot::at(&self.base)
        }

        fn entry(&self, name: &str, bytes: usize) -> BookEntry {
            let root = self.base.join("worktrees").join(name);
            fs::create_dir_all(&root).expect("the worktree");
            let dir = self.base.join(format!("{name}-0123456789abcdef"));
            fs::create_dir_all(&dir).expect("the book dir");
            fs::write(dir.join("libro.sqlite"), vec![0u8; bytes]).expect("the database");
            BookEntry {
                book_key: format!("{name}-0123456789abcdef"),
                root: root.to_string_lossy().to_string(),
                dir: dir.to_string_lossy().to_string(),
                nonce: "n".to_string(),
                last_opened_at: 1_700_000_000_000,
                files: 12,
                symbols: 345,
                ..BookEntry::default()
            }
        }

        fn save(&self, entries: Vec<BookEntry>) {
            let mut registry = BookRegistry::default();
            for entry in entries {
                registry.record(entry);
            }
            registry
                .save(&self.books().registry_path())
                .expect("the registry");
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }

    /// T4/C3: `graphium books list` is a table over the registry — key, root,
    /// the totals of the last open, the bytes on disk — and the books whose
    /// worktree is gone are marked instead of looking healthy.
    #[test]
    fn the_list_prints_the_totals_and_marks_the_missing_root() {
        let scratch = Scratch::new("list");
        let live = scratch.entry("live", 2048);
        let gone = scratch.entry("gone", 16);
        fs::remove_dir_all(std::path::Path::new(&gone.root)).expect("the worktree disappears");
        scratch.save(vec![live.clone(), gone.clone()]);

        let table = list_lines(&scratch.books());
        let body = table.join("\n");
        assert!(body.contains("LIBRO"), "{body}");
        assert!(body.contains("RAÍZ"), "{body}");
        let live_line = table
            .iter()
            .find(|line| line.contains(&live.book_key))
            .expect("the live book");
        assert!(live_line.contains("12"), "{live_line}");
        assert!(live_line.contains("345"), "{live_line}");
        assert!(live_line.contains("2.0 kB"), "{live_line}");
        assert!(!live_line.contains("falta"), "{live_line}");
        let gone_line = table
            .iter()
            .find(|line| line.contains(&gone.book_key))
            .expect("the stale book");
        assert!(
            gone_line.contains("falta la raíz"),
            "a deleted worktree is marked: {gone_line}"
        );
        // An empty books root says so instead of printing a bare header.
        let empty = list_lines(&BooksRoot::at(scratch.base.join("nothing-here")));
        assert_eq!(empty.len(), 2, "{empty:?}");
        assert!(empty[1].contains("no hay libros"), "{empty:?}");
    }

    /// T4/C3: `graphium books prune` prints what it removed, why, and how much
    /// it freed; `--dry-run` says the same in the conditional.
    #[test]
    fn the_prune_prints_what_went_away_and_what_is_left() {
        let scratch = Scratch::new("prune");
        let live = scratch.entry("live", 64);
        let gone = scratch.entry("gone", 512);
        fs::remove_dir_all(std::path::Path::new(&gone.root)).expect("the worktree disappears");
        scratch.save(vec![live.clone(), gone.clone()]);

        let dry = lines(
            &scratch.books(),
            &["prune".to_string(), "--dry-run".to_string()],
        )
        .expect("the dry run");
        let dry_text = dry.join("\n");
        assert!(dry_text.contains(&gone.book_key), "{dry_text}");
        assert!(dry_text.contains("root-gone"), "{dry_text}");
        assert!(dry_text.contains("se borrarían"), "{dry_text}");
        assert!(dry_text.contains("1 libro(s)"), "{dry_text}");
        assert!(
            std::path::Path::new(&gone.dir).exists(),
            "a dry run deletes nothing"
        );

        let real = lines(&scratch.books(), &["prune".to_string()]).expect("the prune");
        let real_text = real.join("\n");
        assert!(real_text.contains(&gone.book_key), "{real_text}");
        assert!(real_text.contains("borrados"), "{real_text}");
        assert!(real_text.contains("512 B"), "{real_text}");
        assert!(
            !std::path::Path::new(&gone.dir).exists(),
            "the book is gone"
        );
        assert!(
            std::path::Path::new(&live.dir).exists(),
            "the live book is untouched"
        );

        // A prune with nothing to do still answers with its summary line.
        let again = lines(&scratch.books(), &["prune".to_string()]).expect("the second prune");
        assert_eq!(again.len(), 1, "{again:?}");
        assert!(again[0].contains("0 libro(s)"), "{again:?}");
        assert!(
            again[0].contains(&format!("{} en el registro", 1)),
            "the live book is still there: {again:?}"
        );
    }

    /// T4/C3: the cap of the idle exit reaches the CLI too, and the summary
    /// says how many books survive it.
    #[test]
    fn the_prune_applies_the_cap_of_the_idle_exit() {
        let scratch = Scratch::new("cap");
        let entries: Vec<BookEntry> = (0..KEEP_NEWEST_BOOKS + 2)
            .map(|index| {
                let mut entry = scratch.entry(&format!("b{index}"), 8);
                entry.last_opened_at = index as i64;
                entry
            })
            .collect();
        scratch.save(entries);

        let out = lines(&scratch.books(), &["prune".to_string()]).expect("the prune");
        let text = out.join("\n");
        assert!(text.contains("over-cap"), "the cap is reported: {text}");
        assert!(text.contains("2 libro(s) borrados"), "{text}");
        assert!(
            text.contains(&format!("{KEEP_NEWEST_BOOKS} en el registro")),
            "{text}"
        );
    }

    /// T4/C3 (ideas P8): the hook is a `post-checkout`/`post-merge` snippet that
    /// only reports — `--dry-run` — so installing it never deletes a book.
    #[test]
    fn the_hook_is_a_short_report_only_snippet() {
        let printed = lines(&BooksRoot::at("/tmp/none"), &["hook".to_string()]).expect("the hook");
        let text = printed.join("\n");
        assert!(text.starts_with("#!/bin/sh\n"), "{text}");
        assert!(text.contains("post-checkout"), "{text}");
        assert!(text.contains("post-merge"), "{text}");
        assert!(text.contains("graphium books prune --dry-run"), "{text}");
        assert!(HOOK_SNIPPET.lines().count() <= 30, "the hook stays short");
    }

    /// A request the command does not understand is an error with the usage,
    /// never a silent success.
    #[test]
    fn a_missing_or_unknown_command_is_an_error_with_the_usage() {
        let scratch = Scratch::new("usage");
        for arguments in [
            vec![],
            vec!["nope".to_string()],
            vec!["list".to_string(), "extra".to_string()],
            vec!["prune".to_string(), "--force".to_string()],
        ] {
            let error = lines(&scratch.books(), &arguments)
                .expect_err("the command is refused")
                .to_string();
            assert!(
                error.contains("Uso:") || error.contains("unknown") || error.contains("necesita"),
                "{arguments:?}: {error}"
            );
        }
        // And the summary of an empty prune is still a line, not an error.
        let empty = prune_lines(&gx_mcp::book::PruneOutcome {
            removed: Vec::new(),
            kept: 0,
            dry_run: false,
        });
        assert_eq!(empty.len(), 1);
        assert!(empty[0].contains("0 libro(s) borrados"), "{empty:?}");
    }
}
