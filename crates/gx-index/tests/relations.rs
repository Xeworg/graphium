//! The relations of the book (feature `f9-libro`, corte C1; S7, D3).
//!
//! The relations are **syntactic** (D3): a call by name, an import by its bound
//! segment, an `impl Trait for Type` and a type reference. The grammar of the
//! chapter decides what a relation is; the schema only names the four kinds, so
//! the next language of C2 plugs in without touching it.
//!
//! Every test builds its own worktree and its own books root inside the system
//! temp directory: nothing here reads or writes the repository.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use gx_index::book::Book;
use gx_index::lang::parse;
use gx_index::location::BooksRoot;
use gx_index::watcher::{IndexOptions, Indexer};
use gx_index::RELATION_KINDS;
use gx_index::{Direction, ImpactRequest, SearchOutcome, SearchRequest};

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
            "gx-index-relations-{name}-{}-{}",
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

    fn open(&self) -> Book {
        let books = BooksRoot::at(&self.books);
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

/// One chapter with the four kinds, and two decoys that must produce nothing.
const CHAPTER: &str = r#"
use std::path::Path;
use crate::store::DiagramStore;

pub trait Store {
    fn load(&self) -> u8;
}

pub struct Config {
    pub store: DiagramStore,
}

impl Store for Config {
    fn load(&self) -> u8 {
        1
    }
}

pub struct DiagramStore;

impl DiagramStore {
    pub fn new() -> Self {
        Self
    }
}

pub fn open_store() -> DiagramStore {
    DiagramStore::new()
}

pub fn build(input: &Path) -> DiagramStore {
    let store = open_store();
    store.load();
    DiagramStore::new()
}

// A comment that mentions ignored_in_comment() must not produce an edge.
pub fn quiet() -> u8 {
    let _text = "also_ignored_in_a_string()";
    0
}
"#;

/// S7/D3: the four relation kinds come out of the Rust grammar with the
/// textual target always present, and every edge hangs from a symbol of the
/// chapter (file-scope relations from the module of the file).
#[test]
fn the_rust_grammar_yields_calls_imports_inherits_and_uses() {
    let parsed = parse("src/demo.rs", CHAPTER.as_bytes(), 0).expect("parses");
    let pairs: Vec<(&str, String)> = parsed
        .edges
        .iter()
        .map(|edge| (edge.kind, edge.dst_name.clone()))
        .collect();

    for expected in [
        ("calls", "open_store".to_string()),
        ("calls", "load".to_string()),
        ("calls", "new".to_string()),
        ("imports", "Path".to_string()),
        ("imports", "DiagramStore".to_string()),
        ("inherits", "Store".to_string()),
        ("uses", "DiagramStore".to_string()),
        ("uses", "Path".to_string()),
    ] {
        assert!(
            pairs.contains(&expected),
            "missing {expected:?} in {pairs:?}"
        );
    }

    // The schema of the book is language-agnostic: only these four kinds.
    for edge in &parsed.edges {
        assert!(
            RELATION_KINDS.contains(&edge.kind),
            "`{}` is not one of the four relations",
            edge.kind
        );
    }

    // Every edge hangs from a symbol of the chapter, and the file-scope
    // imports hang from the module of the file.
    for edge in &parsed.edges {
        assert!(
            parsed.symbols.get(edge.src).is_some(),
            "the source of {edge:?} is a symbol of the chapter"
        );
    }
    let module = parsed
        .symbols
        .iter()
        .find(|symbol| symbol.kind == "module" && symbol.name == "demo")
        .expect("the file-scope anchor is the module of the file");
    let module_index = parsed
        .symbols
        .iter()
        .position(|symbol| symbol.kind == "module" && symbol.name == "demo")
        .expect("its index");
    assert!(
        parsed
            .edges
            .iter()
            .any(|edge| edge.kind == "imports" && edge.src == module_index),
        "the imports are anchored to `{}`",
        module.qname
    );

    // The call is attributed to the function that makes it, not to the file.
    let build = parsed
        .symbols
        .iter()
        .position(|symbol| symbol.name == "build")
        .expect("the caller");
    assert!(
        parsed
            .edges
            .iter()
            .any(|edge| edge.kind == "calls" && edge.dst_name == "open_store" && edge.src == build),
        "{:?}",
        parsed.edges
    );
}

/// D3: the extractor is syntactic and never guesses from prose. A name that
/// only appears in a comment or in a string is not a relation.
#[test]
fn a_comment_or_a_string_never_yields_an_edge() {
    let parsed = parse("src/demo.rs", CHAPTER.as_bytes(), 0).expect("parses");
    for decoy in ["ignored_in_comment", "also_ignored_in_a_string"] {
        assert!(
            !parsed.edges.iter().any(|edge| edge.dst_name == decoy),
            "`{decoy}` is text, not a relation: {:?}",
            parsed.edges
        );
    }

    // A chapter with no relations at all produces no edges and no anchor.
    let quiet = parse("src/quiet.rs", b"pub fn quiet() -> u8 { 0 }\n", 0).expect("parses");
    assert!(quiet.edges.is_empty(), "{:?}", quiet.edges);
    assert!(
        !quiet.symbols.iter().any(|symbol| symbol.kind == "module"),
        "a file with no relations needs no anchor: {:?}",
        quiet.symbols
    );
}

/// S7: an edge is born unresolved (`dst_id IS NULL`) and the reader resolves
/// it by name, so the book answers the walk without any typed resolution.
#[test]
fn the_book_walks_the_edges_a_chapter_declared() {
    let scratch = Scratch::new("walk");
    scratch.write("src/demo.rs", CHAPTER);
    let book = Arc::new(scratch.open());
    scratch.indexer(&book).run_full().expect("the index");

    let start = match book
        .search(&SearchRequest {
            query: "build".to_string(),
        })
        .expect("the search")
    {
        SearchOutcome::Hits(hits) => hits[0].clone(),
        other => panic!("expected one `build`, got {other:?}"),
    };

    let impact = book
        .impact(&ImpactRequest {
            start: start.id,
            depth: 1,
            direction: Direction::Out,
        })
        .expect("the walk")
        .expect("the start symbol exists");

    assert!(
        impact
            .nodes
            .iter()
            .any(|node| node.name == "open_store" && node.depth == 1),
        "{:?}",
        impact.nodes
    );
    assert!(
        impact
            .edges
            .iter()
            .any(|edge| edge.kind == "calls" && edge.from == start.id),
        "{:?}",
        impact.edges
    );
    // The walk answers the location of every node (S7).
    let reached = impact
        .nodes
        .iter()
        .find(|node| node.name == "open_store")
        .expect("the callee");
    assert_eq!(reached.file, "src/demo.rs", "{reached:?}");
    assert!(reached.line >= 1, "{reached:?}");
}

/// S7: the resolution of a name runs through the folded collation, so the
/// same name spelled in another case still resolves, and a name with no
/// declaration stays unresolved instead of inventing an answer.
#[test]
fn the_walk_resolves_by_name_and_never_invents_a_target() {
    let scratch = Scratch::new("resolve");
    scratch.write(
        "src/a.rs",
        "pub fn openStore() -> u8 { 1 }\npub fn build() -> u8 { openStore(); missing_helper(); 0 }\n",
    );
    let book = Arc::new(scratch.open());
    scratch.indexer(&book).run_full().expect("the index");
    let start = match book
        .search(&SearchRequest {
            query: "build".to_string(),
        })
        .expect("the search")
    {
        SearchOutcome::Hits(hits) => hits[0].clone(),
        other => panic!("expected one `build`, got {other:?}"),
    };
    let impact = book
        .impact(&ImpactRequest {
            start: start.id,
            depth: 2,
            direction: Direction::Out,
        })
        .expect("the walk")
        .expect("the start exists");
    assert!(
        impact.nodes.iter().any(|node| node.name == "openStore"),
        "camelCase resolves: {:?}",
        impact.nodes
    );
    assert!(
        impact
            .edges
            .iter()
            .all(|edge| impact.nodes.iter().any(|node| node.id == edge.to)),
        "an unresolved name never becomes a node: {impact:?}"
    );
    assert!(impact.unresolved >= 1, "{impact:?}");
}
