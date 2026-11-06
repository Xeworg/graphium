//! The languages of C2: TypeScript/TSX, Python, Go and Java (feature
//! `f9-libro`, corte C2; S3, D3, T3).
//!
//! Every grammar of `lang/` answers the same chapter: declarations with their
//! spans and their containment tree, and the four syntactic relations of the
//! schema (`calls`, `imports`, `inherits`, `uses`). The tests below are the
//! same question asked five times — one module per language, one extension to
//! answer to, one chapter to prune, one negative case (a comment or a string is
//! text and never a relation) — plus the two book-level ones: a mixed worktree
//! answers `code_structure`/`code_impact` across languages, and the resolved
//! percentage of D3 is measured on these very fixtures.
//!
//! Every test builds its own worktree and its own books root inside the system
//! temp directory: nothing here reads or writes the repository.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use gx_index::book::Book;
use gx_index::impact::{Direction, ImpactRequest};
use gx_index::lang::{parse, GRAMMARS};
use gx_index::location::BooksRoot;
use gx_index::structure::StructureRequest;
use gx_index::watcher::{IndexOptions, Indexer};
use gx_index::{ParsedFile, SearchOutcome, StructureAnswer};

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
            "gx-index-languages-{name}-{}-{}",
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

/// The declarations, the nesting and the relations of one chapter, as the test
/// asserts them: `(kind, name, qname)`.
type Row<'a> = (&'a str, &'a str, &'a str);

/// Every chapter a grammar produced, as `(kind, name, qname)`.
fn rows(parsed: &ParsedFile) -> Vec<(String, String, String)> {
    parsed
        .symbols
        .iter()
        .map(|symbol| {
            (
                symbol.kind.to_string(),
                symbol.name.clone(),
                symbol.qname.clone(),
            )
        })
        .collect()
}

/// Asserts that a chapter declares every expected row.
fn assert_symbols(parsed: &ParsedFile, expected: &[Row]) {
    let actual = rows(parsed);
    for (kind, name, qname) in expected {
        let wanted = (kind.to_string(), name.to_string(), qname.to_string());
        assert!(
            actual.contains(&wanted),
            "missing {wanted:?} in the chapter of `{}`: {actual:?}",
            parsed.path
        );
    }
}

/// Asserts that a relation was read with the target the source spells.
fn assert_edge(parsed: &ParsedFile, kind: &str, dst_name: &str) {
    let found: Vec<(&str, &str)> = parsed
        .edges
        .iter()
        .map(|edge| (edge.kind, edge.dst_name.as_str()))
        .collect();
    assert!(
        found.contains(&(kind, dst_name)),
        "missing the relation `{kind}` -> `{dst_name}` in `{}`: {found:?}",
        parsed.path
    );
}

/// Asserts that a relation was read and hangs from the declaration named by
/// its `qname` (two overloads of one name are two declarations).
fn assert_edge_owned(parsed: &ParsedFile, kind: &str, dst_name: &str, src_qname: &str) {
    assert_edge(parsed, kind, dst_name);
    let src = parsed
        .symbols
        .iter()
        .position(|symbol| symbol.qname == src_qname)
        .unwrap_or_else(|| panic!("`{src_qname}` is not declared in `{}`", parsed.path));
    assert!(
        parsed
            .edges
            .iter()
            .any(|edge| edge.kind == kind && edge.dst_name == dst_name && edge.src == src),
        "`{kind}` -> `{dst_name}` must hang from `{src_qname}`: {:?}",
        parsed.edges
    );
}

/// D3: every edge names a symbol of the chapter it hangs from, and the
/// vocabulary is the four kinds of the schema.
fn assert_edges_are_anchored(parsed: &ParsedFile) {
    for edge in &parsed.edges {
        assert!(
            parsed.symbols.get(edge.src).is_some(),
            "the source of {edge:?} is not a symbol of `{}`",
            parsed.path
        );
        assert!(
            gx_index::RELATION_KINDS.contains(&edge.kind),
            "`{}` is not one of the four relations",
            edge.kind
        );
    }
}

/// D3: a name that only appears in prose is not a relation, in any language.
fn assert_no_prose_edges(parsed: &ParsedFile) {
    for decoy in ["ignoredInComment", "alsoIgnoredInAString"] {
        assert!(
            !parsed.edges.iter().any(|edge| edge.dst_name == decoy),
            "`{decoy}` is prose, not a relation: {:?}",
            parsed.edges
        );
    }
}

// ---------------------------------------------------------------------------
// TypeScript / TSX — one module, two extensions
// ---------------------------------------------------------------------------

const TYPESCRIPT: &str = r#"
import { DiagramStore } from "./store";
import legacy = require("./legacy");
import * as pathTool from "./path";

export { Renderer } from "./render";

export interface Loader {
  load(input: string): number;
}

export class ToolBox implements Loader {
  private store: DiagramStore;

  constructor(store: DiagramStore) {
    this.store = store;
  }

  load(input: string): number {
    return this.store.load(input);
  }

  renderWith(other: ToolBox): number {
    return openStore(this.store);
  }

  reload(): number {
    return this.store.load("x");
  }
}

export class SpecialBox extends ToolBox {
  pick<T>(items: Array<ToolBox>): T {
    return items[0];
  }
}

export type Alias = ToolBox;

export enum Mode {
  Fast,
}

export function openStore(store: DiagramStore): DiagramStore {
  return new DiagramStore();
}

export const helper = (input: string): number => legacy.run(input);

// A comment that mentions ignoredInComment() must not produce an edge.
export function quiet(): string {
  const text = "alsoIgnoredInAString()";
  return text;
}
"#;

const TSX: &str = r#"
import { Store } from "./store";

export interface Props {
  store: Store;
}

export function View(props: Props) {
  const label = props.store.load("x");
  return <div className="box">{label}</div>;
}

export class Panel extends View {
  render(store: Store) {
    return <View store={store} />;
  }
}
"#;

/// S3/L5: the TypeScript module prunes the declarations of a chapter, with the
/// language's nesting (`Module.Class.method`) and a span the book can answer.
#[test]
fn typescript_declares_the_symbols_of_a_chapter_with_its_nesting() {
    let parsed = parse("src/tools.ts", TYPESCRIPT.as_bytes(), 0).expect("the TypeScript grammar");
    assert_eq!(parsed.lang, "typescript", "the `lang` of the table `file`");
    assert_symbols(
        &parsed,
        &[
            ("trait", "Loader", "src.tools.Loader"),
            ("method", "load", "src.tools.Loader.load"),
            ("class", "ToolBox", "src.tools.ToolBox"),
            ("method", "constructor", "src.tools.ToolBox.constructor"),
            ("method", "load", "src.tools.ToolBox.load"),
            ("method", "renderWith", "src.tools.ToolBox.renderWith"),
            ("class", "SpecialBox", "src.tools.SpecialBox"),
            ("method", "pick", "src.tools.SpecialBox.pick"),
            ("type", "Alias", "src.tools.Alias"),
            ("type", "Mode", "src.tools.Mode"),
            ("fn", "openStore", "src.tools.openStore"),
            ("const", "helper", "src.tools.helper"),
            ("fn", "quiet", "src.tools.quiet"),
        ],
    );

    // The containment tree: a method hangs from its class.
    let method = parsed
        .symbols
        .iter()
        .find(|symbol| symbol.qname == "src.tools.ToolBox.renderWith")
        .expect("the method");
    let parent = method.parent.expect("the method hangs from its class");
    assert_eq!(parsed.symbols[parent].kind, "class");
    assert_eq!(parsed.symbols[parent].name, "ToolBox");

    let open_store = parsed
        .symbols
        .iter()
        .find(|symbol| symbol.name == "openStore")
        .expect("the fn");
    let line = TYPESCRIPT
        .lines()
        .position(|line| line.contains("function openStore"))
        .expect("the fn is in the source") as u32
        + 1;
    assert_eq!(open_store.start_line, line, "1-based line of the fn");
    assert!(open_store.end_byte > open_store.start_byte, "the span");
}

/// S7/D3: the four relation kinds out of TypeScript — the import, the call by
/// name as written, `extends`/`implements` and the type references — and never
/// an edge out of a comment or a string.
#[test]
fn typescript_relations_are_the_four_kinds_and_never_prose() {
    let parsed = parse("src/tools.ts", TYPESCRIPT.as_bytes(), 0).expect("the grammar");

    for (kind, name) in [
        ("imports", "DiagramStore"),
        ("imports", "legacy"),
        ("imports", "pathTool"),
        ("imports", "Renderer"),
        ("calls", "load"),
        ("calls", "openStore"),
        ("calls", "DiagramStore"),
        ("calls", "run"),
        ("inherits", "Loader"),
        ("inherits", "ToolBox"),
        ("uses", "DiagramStore"),
        ("uses", "ToolBox"),
        ("uses", "Array"),
    ] {
        assert_edge(&parsed, kind, name);
    }

    // The file-scope imports hang from the chapter anchor, named the way the
    // language names the module it lives in.
    let anchor = parsed
        .symbols
        .iter()
        .position(|symbol| symbol.kind == "module")
        .expect("the imports need an anchor");
    assert_eq!(parsed.symbols[anchor].name, "tools");
    assert_eq!(parsed.symbols[anchor].qname, "src.tools");
    assert!(
        parsed
            .edges
            .iter()
            .any(|edge| edge.kind == "imports" && edge.src == anchor),
        "the imports hang from the chapter: {:?}",
        parsed.edges
    );

    assert_edge_owned(&parsed, "calls", "load", "src.tools.ToolBox.reload");
    assert_edge_owned(
        &parsed,
        "calls",
        "openStore",
        "src.tools.ToolBox.renderWith",
    );
    assert_edge_owned(&parsed, "calls", "run", "src.tools.helper");
    assert_edge_owned(&parsed, "inherits", "Loader", "src.tools.ToolBox");
    assert_edge_owned(&parsed, "inherits", "ToolBox", "src.tools.SpecialBox");
    assert_edges_are_anchored(&parsed);
    assert_no_prose_edges(&parsed);
}

/// S3: the same module answers `.tsx`, and the JSX of the chapter is markup,
/// not a relation.
#[test]
fn typescript_answers_tsx_and_jsx_is_not_a_relation() {
    let parsed = parse("src/view.tsx", TSX.as_bytes(), 0).expect("the TSX grammar");
    assert_eq!(parsed.lang, "typescript", "one module covers both grammars");
    assert_symbols(
        &parsed,
        &[
            ("trait", "Props", "src.view.Props"),
            ("fn", "View", "src.view.View"),
            ("class", "Panel", "src.view.Panel"),
            ("method", "render", "src.view.Panel.render"),
        ],
    );
    assert_edge(&parsed, "imports", "Store");
    assert_edge(&parsed, "calls", "load");
    assert_edge(&parsed, "inherits", "View");
    assert_edge(&parsed, "uses", "Props");
    // A JSX element name and an attribute are markup: no call, no use.
    for markup in ["div", "box", "className"] {
        assert!(
            !parsed.edges.iter().any(|edge| edge.dst_name == markup),
            "`{markup}` is JSX markup: {:?}",
            parsed.edges
        );
    }
    assert!(
        !parsed
            .edges
            .iter()
            .any(|edge| edge.kind == "calls" && edge.dst_name == "View"),
        "a JSX element is not a call: {:?}",
        parsed.edges
    );
    assert_edges_are_anchored(&parsed);
}

// ---------------------------------------------------------------------------
// Python
// ---------------------------------------------------------------------------

const PYTHON: &str = r#"
import os
import os.path
from pathlib import Path, PurePath
from .store import DiagramStore
from .wild import *

LIMIT = 3


class Loader:
    def load(self, input: str) -> int:
        return 1


class DiagramStore(Loader):
    def __init__(self, path: Path) -> None:
        self.path = path

    def load(self, input: str) -> int:
        return parseConfig(self.path)


def parseConfig(path: Path) -> int:
    return openStore().load(str(path))


def openStore() -> DiagramStore:
    return DiagramStore(Path("."))


def batching(items: list[DiagramStore]) -> dict[str, Loader]:
    return {}


def quiet() -> str:
    # A comment that mentions ignoredInComment() must not produce an edge.
    text = "alsoIgnoredInAString()"
    return text
"#;

/// S3/L5: the Python module prunes the declarations of a chapter — the class,
/// its methods (one of them decorated) and the module-level constant — with the
/// language's nesting (`Module.Class.method`).
#[test]
fn python_declares_the_symbols_of_a_chapter_with_its_nesting() {
    let parsed = parse("src/store.py", PYTHON.as_bytes(), 0).expect("the Python grammar");
    assert_eq!(parsed.lang, "python");
    assert_symbols(
        &parsed,
        &[
            ("const", "LIMIT", "src.store.LIMIT"),
            ("class", "Loader", "src.store.Loader"),
            ("method", "load", "src.store.Loader.load"),
            ("class", "DiagramStore", "src.store.DiagramStore"),
            ("method", "__init__", "src.store.DiagramStore.__init__"),
            ("method", "load", "src.store.DiagramStore.load"),
            ("fn", "parseConfig", "src.store.parseConfig"),
            ("fn", "openStore", "src.store.openStore"),
            ("fn", "batching", "src.store.batching"),
            ("fn", "quiet", "src.store.quiet"),
        ],
    );

    let method = parsed
        .symbols
        .iter()
        .find(|symbol| symbol.qname == "src.store.DiagramStore.load")
        .expect("the method");
    let parent = method.parent.expect("the method hangs from its class");
    assert_eq!(parsed.symbols[parent].name, "DiagramStore");
    assert!(
        !parsed
            .symbols
            .iter()
            .any(|symbol| symbol.kind == "const" && symbol.name == "path"),
        "an attribute of a method is not a module constant: {:?}",
        rows(&parsed)
    );
}

/// S7/D3: `import`/`from … import`, the call by name as written, the base class
/// and the annotations of the chapter — and never an edge out of prose.
#[test]
fn python_relations_are_the_four_kinds_and_never_prose() {
    let parsed = parse("src/store.py", PYTHON.as_bytes(), 0).expect("the grammar");

    for (kind, name) in [
        ("imports", "os"),
        ("imports", "path"),
        ("imports", "Path"),
        ("imports", "PurePath"),
        ("imports", "DiagramStore"),
        ("imports", "wild"),
        ("calls", "parseConfig"),
        ("calls", "openStore"),
        ("calls", "load"),
        ("calls", "DiagramStore"),
        ("calls", "Path"),
        ("inherits", "Loader"),
        ("uses", "Path"),
        ("uses", "DiagramStore"),
        ("uses", "Loader"),
    ] {
        assert_edge(&parsed, kind, name);
    }

    assert_edge_owned(&parsed, "calls", "openStore", "src.store.parseConfig");
    assert_edge_owned(&parsed, "inherits", "Loader", "src.store.DiagramStore");
    assert_edge_owned(&parsed, "uses", "DiagramStore", "src.store.openStore");
    assert_edges_are_anchored(&parsed);
    assert_no_prose_edges(&parsed);
}

// ---------------------------------------------------------------------------
// Go
// ---------------------------------------------------------------------------

const GO: &str = r#"
package store

import (
	"fmt"
	aliasstore "net/http"
	. "os"
)

const Limit = 3

var Counter = 0

type Loader interface {
	Load(input string) int
}

type Embedded interface {
	fmt.Stringer
}

type Base struct {
	Name string
}

type DiagramStore struct {
	Base
	path string
	next *DiagramStore
}

func OpenStore(path string) *DiagramStore {
	return &DiagramStore{path: path}
}

func (d *DiagramStore) Load(input string) int {
	return parseConfig(d.path)
}

func (d DiagramStore) RenderWith(other *DiagramStore) int {
	fmt.Println(aliasstore.StatusOK)
	return OpenStore("x").Load("y")
}

func parseConfig(path string) int {
	return len(path)
}

func batching(items []DiagramStore) map[string]Loader {
	return nil
}

func quiet() string {
	// A comment that mentions ignoredInComment() must not produce an edge.
	text := "alsoIgnoredInAString()"
	return text
}
"#;

/// S3/L5: the Go module prunes the package scope — funcs, methods hanging from
/// their receiver, the declared types and the package constants — with the
/// package as the prefix of every `qname`.
#[test]
fn go_declares_the_symbols_of_a_chapter_with_its_nesting() {
    let parsed = parse("internal/store/store.go", GO.as_bytes(), 0).expect("the Go grammar");
    assert_eq!(parsed.lang, "go");
    assert_symbols(
        &parsed,
        &[
            ("const", "Limit", "internal.store.Limit"),
            ("static", "Counter", "internal.store.Counter"),
            ("trait", "Loader", "internal.store.Loader"),
            ("method", "Load", "internal.store.Loader.Load"),
            ("trait", "Embedded", "internal.store.Embedded"),
            ("type", "Base", "internal.store.Base"),
            ("type", "DiagramStore", "internal.store.DiagramStore"),
            ("fn", "OpenStore", "internal.store.OpenStore"),
            ("method", "Load", "internal.store.DiagramStore.Load"),
            (
                "method",
                "RenderWith",
                "internal.store.DiagramStore.RenderWith",
            ),
            ("fn", "parseConfig", "internal.store.parseConfig"),
            ("fn", "batching", "internal.store.batching"),
            ("fn", "quiet", "internal.store.quiet"),
        ],
    );

    // The receiver decides the parent: a method hangs from its type, whichever
    // order the file declares them in.
    let method = parsed
        .symbols
        .iter()
        .find(|symbol| symbol.qname == "internal.store.DiagramStore.RenderWith")
        .expect("the method");
    let parent = method
        .parent
        .expect("the method hangs from its receiver type");
    assert_eq!(parsed.symbols[parent].kind, "type");
    assert_eq!(parsed.symbols[parent].name, "DiagramStore");
}

/// S7/D3: the import path segment, the call by name as written, the struct and
/// interface embedding, and the type references — never an edge out of prose.
#[test]
fn go_relations_are_the_four_kinds_and_never_prose() {
    let parsed = parse("internal/store/store.go", GO.as_bytes(), 0).expect("the grammar");

    for (kind, name) in [
        ("imports", "fmt"),
        ("imports", "http"),
        ("imports", "os"),
        ("calls", "parseConfig"),
        ("calls", "OpenStore"),
        ("calls", "Println"),
        ("calls", "Load"),
        ("calls", "len"),
        ("inherits", "Base"),
        ("inherits", "Stringer"),
        ("uses", "DiagramStore"),
        ("uses", "Loader"),
        ("uses", "Stringer"),
    ] {
        assert_edge(&parsed, kind, name);
    }

    assert_edge_owned(
        &parsed,
        "calls",
        "parseConfig",
        "internal.store.DiagramStore.Load",
    );
    assert_edge_owned(&parsed, "inherits", "Base", "internal.store.DiagramStore");
    assert_edges_are_anchored(&parsed);
    assert_no_prose_edges(&parsed);
}

// ---------------------------------------------------------------------------
// Java
// ---------------------------------------------------------------------------

const JAVA: &str = r#"
package com.graphium.store;

import java.util.List;
import java.util.ArrayList;
import static java.util.Objects.requireNonNull;

public interface Loader {
    int load(String input);
}

public abstract class Base {
    protected String name;
}

public class DiagramStore extends Base implements Loader, Comparable<DiagramStore> {
    private final List<Loader> loaders;
    private final DiagramStore next;

    public DiagramStore(List<Loader> loaders) {
        this.loaders = loaders;
        this.next = requireNonNull(null);
    }

    @Override
    public int load(String input) {
        return parseConfig(input);
    }

    public DiagramStore renderWith(DiagramStore other) {
        return openStore().load("x");
    }

    public static DiagramStore openStore() {
        return new DiagramStore(new ArrayList<Loader>());
    }

    public int load(int input) {
        return input;
    }
}

public enum Mode {
    FAST
}

public record Pair(String left, String right) {}

final class Quiet {
    String quiet() {
        // A comment that mentions ignoredInComment() must not produce an edge.
        String text = "alsoIgnoredInAString()";
        return text;
    }
}
"#;

/// S3/L5: the Java module prunes the types of a chapter — the interface, the
/// classes with their constructors and methods, the enum and the record — and
/// the prefix of every `qname` is the `package` the file declares.
#[test]
fn java_declares_the_symbols_of_a_chapter_with_its_nesting() {
    let parsed = parse("src/Store.java", JAVA.as_bytes(), 0).expect("the Java grammar");
    assert_eq!(parsed.lang, "java");
    assert_symbols(
        &parsed,
        &[
            ("trait", "Loader", "com.graphium.store.Loader"),
            ("method", "load", "com.graphium.store.Loader.load"),
            ("class", "Base", "com.graphium.store.Base"),
            ("class", "DiagramStore", "com.graphium.store.DiagramStore"),
            (
                "method",
                "DiagramStore",
                "com.graphium.store.DiagramStore.DiagramStore",
            ),
            ("method", "load", "com.graphium.store.DiagramStore.load"),
            (
                "method",
                "renderWith",
                "com.graphium.store.DiagramStore.renderWith",
            ),
            (
                "method",
                "openStore",
                "com.graphium.store.DiagramStore.openStore",
            ),
            ("type", "Mode", "com.graphium.store.Mode"),
            ("class", "Pair", "com.graphium.store.Pair"),
            ("class", "Quiet", "com.graphium.store.Quiet"),
            ("method", "quiet", "com.graphium.store.Quiet.quiet"),
        ],
    );

    // Two overloads are two declarations of the same name: the chapter keeps
    // both, and the book is the one that numbers the colliding qname.
    let loads = parsed
        .symbols
        .iter()
        .filter(|symbol| symbol.qname == "com.graphium.store.DiagramStore.load")
        .count();
    assert_eq!(loads, 2, "the overloads are kept: {:?}", rows(&parsed));

    let constructor = parsed
        .symbols
        .iter()
        .find(|symbol| symbol.qname == "com.graphium.store.DiagramStore.DiagramStore")
        .expect("the constructor");
    let parent = constructor
        .parent
        .expect("the constructor hangs from its class");
    assert_eq!(parsed.symbols[parent].kind, "class");
}

/// S7/D3: the import, the call by name as written, `extends`/`implements` and
/// the type references — never an edge out of a comment or a string literal.
#[test]
fn java_relations_are_the_four_kinds_and_never_prose() {
    let parsed = parse("src/Store.java", JAVA.as_bytes(), 0).expect("the grammar");

    for (kind, name) in [
        ("imports", "List"),
        ("imports", "ArrayList"),
        ("imports", "requireNonNull"),
        ("calls", "requireNonNull"),
        ("calls", "parseConfig"),
        ("calls", "openStore"),
        ("calls", "load"),
        ("calls", "DiagramStore"),
        ("calls", "ArrayList"),
        ("inherits", "Base"),
        ("inherits", "Loader"),
        ("inherits", "Comparable"),
        ("uses", "String"),
        ("uses", "List"),
        ("uses", "Loader"),
        ("uses", "DiagramStore"),
        ("uses", "Comparable"),
        ("uses", "ArrayList"),
    ] {
        assert_edge(&parsed, kind, name);
    }

    assert_edge_owned(
        &parsed,
        "calls",
        "parseConfig",
        "com.graphium.store.DiagramStore.load",
    );
    assert_edge_owned(
        &parsed,
        "inherits",
        "Base",
        "com.graphium.store.DiagramStore",
    );
    assert_edges_are_anchored(&parsed);
    assert_no_prose_edges(&parsed);
}

// ---------------------------------------------------------------------------
// The book (S3, D3): a mixed tree answers across languages
// ---------------------------------------------------------------------------

const MIXED_TS: &str = "\
export class Store {
  load(input: string): number {
    return input.length;
  }
}

export function openStore(): Store {
  return new Store();
}
";

const MIXED_PY: &str = "\
from shared import openStore


def build():
    return openStore()
";

const MIXED_GO: &str = "\
package tool

func Use() int {
	return openStore()
}
";

const MIXED_JAVA: &str = "\
package com.graphium.app;

public class App {
    public int run() {
        return openStore();
    }
}
";

const MIXED_RS: &str = "\
pub fn call_open_store() -> u8 {
    openStore()
}
";

/// S3/D3: the book of a worktree with one chapter per language answers the
/// containment tree of each one and walks an edge **across** languages, by
/// name: the call in Python/Go/Java/Rust resolves to the TypeScript
/// declaration, and the answer says in which file each caller lives.
#[test]
fn a_mixed_tree_answers_across_the_five_languages() {
    let scratch = Scratch::new("mixed");
    scratch.write("src/shared.ts", MIXED_TS);
    scratch.write("src/caller.py", MIXED_PY);
    scratch.write("src/tool.go", MIXED_GO);
    scratch.write("src/App.java", MIXED_JAVA);
    scratch.write("src/check.rs", MIXED_RS);
    let book = Arc::new(scratch.open());
    scratch.indexer(&book).run_full().expect("the index");

    // `code_structure`: each chapter answers its own language.
    for (path, lang) in [
        ("src/shared.ts", "typescript"),
        ("src/caller.py", "python"),
        ("src/tool.go", "go"),
        ("src/App.java", "java"),
        ("src/check.rs", "rust"),
    ] {
        let structure = book
            .structure_at_path(path)
            .expect("the read")
            .unwrap_or_else(|| panic!("`{path}` is a chapter of the book"));
        assert_eq!(structure.lang, lang, "the language of `{path}`");
        let answer: StructureAnswer = structure.answer(&StructureRequest::default());
        assert!(!answer.symbols.is_empty(), "`{path}` declares something");
    }

    // `code_impact` from the TypeScript declaration: every other language
    // calls it, and the syntactic resolution by name is what connects them.
    let store = match book
        .search(&gx_index::SearchRequest {
            query: "openStore".to_string(),
        })
        .expect("the search")
    {
        SearchOutcome::Hits(hits) => hits[0].clone(),
        other => panic!("expected one `openStore`, got {other:?}"),
    };
    assert_eq!(store.file, "src/shared.ts");

    let impact = book
        .impact(&ImpactRequest::symbol(store.id, 1, Direction::In))
        .expect("the walk")
        .expect("the start symbol exists");
    let callers: Vec<&str> = impact
        .nodes
        .iter()
        .filter(|node| node.depth == 1)
        .map(|node| node.file.as_str())
        .collect();
    for expected in [
        "src/caller.py",
        "src/tool.go",
        "src/App.java",
        "src/check.rs",
    ] {
        assert!(
            callers.contains(&expected),
            "`{expected}` calls `openStore()` and the walk must reach it: {callers:?}"
        );
    }
    assert_eq!(
        impact.unresolved, 0,
        "every caller resolves by name: {:?}",
        impact.edges
    );
}

// ---------------------------------------------------------------------------
// D3: the resolution rate of each language, measured on these fixtures
// ---------------------------------------------------------------------------

const RUST: &str = r#"
use std::path::Path;

pub trait Store {
    fn load(&self) -> u8;
}

pub struct DiagramStore {
    pub path: Path,
}

impl Store for DiagramStore {
    fn load(&self) -> u8 {
        parse_config()
    }
}

pub fn open_store() -> DiagramStore {
    DiagramStore { path: Path::new(".") }
}

pub const LIMIT: u8 = 3;

pub fn parse_config() -> u8 {
    open_store().load()
}

pub fn close(store: &DiagramStore) -> u8 {
    store.load()
}
"#;

/// D3: the book measures, per language, how many of its syntactic edges resolve
/// to a declaration. The numbers below are the ones published in `docs/PRD.md`
/// — **measured on these fixtures, not promises** — and the floors keep a
/// regression from passing silently.
#[test]
fn the_resolution_rate_of_each_language_is_measured_on_the_fixtures() {
    let scratch = Scratch::new("resolution");
    scratch.write("src/tools.ts", TYPESCRIPT);
    scratch.write("src/view.tsx", TSX);
    scratch.write("src/store.py", PYTHON);
    scratch.write("internal/store/store.go", GO);
    scratch.write("src/Store.java", JAVA);
    scratch.write("src/check.rs", RUST);
    let book = Arc::new(scratch.open());
    scratch.indexer(&book).run_full().expect("the index");

    let report = book.resolution().expect("the honesty report");
    let by_lang = |lang: &str| {
        report
            .iter()
            .find(|row| row.lang == lang)
            .unwrap_or_else(|| panic!("the report has no `{lang}` row: {report:?}"))
            .clone()
    };

    // Files, symbols and edges come from the book; `resolved` is the number of
    // targets whose name has at most the fanout of a resolution (D3).
    for lang in ["typescript", "python", "go", "java", "rust"] {
        let row = by_lang(lang);
        assert!(row.files >= 1, "`{lang}` has a chapter: {row:?}");
        assert!(row.symbols >= 1, "`{lang}` declares something: {row:?}");
        assert!(row.edges >= 10, "`{lang}` declares relations: {row:?}");
        assert!(
            row.resolved <= row.edges,
            "a rate is never over 100 %: {row:?}"
        );
    }

    // The published percentages (docs/PRD.md, D3): measured on these fixtures
    // with the same rule `code_impact` uses (≤5 definers, `name_norm`).
    println!(
        "D3 resolutions: {}",
        report
            .iter()
            .map(|row| format!(
                "{} {}/{} = {:.1} % ({} files, {} symbols)",
                row.lang,
                row.resolved,
                row.edges,
                row.percent(),
                row.files,
                row.symbols
            ))
            .collect::<Vec<_>>()
            .join(" | ")
    );

    for (lang, floor) in [
        ("typescript", 45.0),
        ("python", 12.0),
        ("go", 15.0),
        ("java", 27.0),
    ] {
        let row = by_lang(lang);
        assert!(
            row.percent() >= floor,
            "`{lang}` resolves {:.1} % of {} edges, under the published floor of {} %: {row:?}",
            row.percent(),
            row.edges,
            floor
        );
    }
}

/// S3/L5: the registry answers the four new extensions with the language the
/// table `file` stores (`.ts`/`.tsx` share one module).
#[test]
fn the_registry_answers_the_four_new_languages() {
    for (path, lang) in [
        ("src/a.ts", "typescript"),
        ("src/a.tsx", "typescript"),
        ("src/a.py", "python"),
        ("src/a.go", "go"),
        ("src/a.java", "java"),
        ("src/a.rs", "rust"),
    ] {
        let parsed = parse(path, b"\n", 0).unwrap_or_else(|error| {
            panic!("`{path}` is a chapter of the book: {error}");
        });
        assert_eq!(parsed.lang, lang, "the language of `{path}`");
    }
    // Five languages, one grammar each; TypeScript covers both grammars.
    assert_eq!(GRAMMARS.len(), 5);
}
