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
use gx_index::{LanguageResolution, ParsedFile, SearchOutcome, StructureAnswer};

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

/// S5(a): the report row of one language, measured on a book that holds only
/// the chapter `(path, source)` — the smallest tree that can answer it, so the
/// arithmetic of one runtime name is the whole test.
#[cfg(any(
    feature = "lang-swift",
    feature = "lang-kotlin",
    feature = "lang-dart",
    feature = "lang-scala",
    feature = "lang-r"
))]
fn measured_rate(scratch_name: &str, path: &str, source: &str, lang: &str) -> LanguageResolution {
    let scratch = Scratch::new(scratch_name);
    scratch.write(path, source);
    let book = Arc::new(scratch.open());
    scratch.indexer(&book).run_full().expect("the index");
    let report = book.resolution().expect("the honesty report");
    report
        .iter()
        .find(|row| row.lang == lang)
        .cloned()
        .unwrap_or_else(|| panic!("the report has no `{lang}` row: {report:?}"))
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
#[cfg(feature = "lang-typescript")]
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
#[cfg(feature = "lang-typescript")]
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
#[cfg(feature = "lang-typescript")]
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
#[cfg(feature = "lang-python")]
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
#[cfg(feature = "lang-python")]
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
#[cfg(feature = "lang-go")]
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
#[cfg(feature = "lang-go")]
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

/// S5(b): the two-file Go package of the f10 record — `internal/store` and the
/// `cmd` that imports it. The import names the package the book indexes, so the
/// chapter that is imported has to declare it: `store` is the name the source
/// imports, not the path it spells.
#[cfg(feature = "lang-go")]
const GO_PACKAGE_STORE: &str = r#"
package store

type DiagramStore struct {
	Path string
}

func OpenStore(path string) *DiagramStore {
	return &DiagramStore{Path: path}
}

func (s *DiagramStore) Load(input string) int {
	return len(input)
}
"#;

/// S5(b): the caller of that package: `store.OpenStore()` and `s.Load("y")`,
/// and the type of the package it names in the type of a variable.
#[cfg(feature = "lang-go")]
const GO_PACKAGE_MAIN: &str = r#"
package main

import (
	"fmt"

	"example.com/app/internal/store"
)

var defaultStore *store.DiagramStore

func main() {
	s := store.OpenStore("x")
	fmt.Println(s.Load("y"))
}
"#;

/// S5(b)/D5: the rate is the measurement that justifies this cut. The f10
/// record measured the two-file package at **5/6 = 83,3 %**: the qualified call
/// and the method of the receiver already resolved by name, and the one edge
/// left was `imports store`, because a chapter only declared its module anchor
/// when one of its own file-scope relations needed it. A package is what an
/// `import` names, so the chapter declares it and the same tree measures
/// **6/6 = 100,0 %**.
#[cfg(feature = "lang-go")]
#[test]
fn a_go_import_of_a_package_the_book_indexes_resolves_to_that_package() {
    let scratch = Scratch::new("go-package-import");
    scratch.write("internal/store/store.go", GO_PACKAGE_STORE);
    scratch.write("cmd/main.go", GO_PACKAGE_MAIN);
    let book = Arc::new(scratch.open());
    scratch.indexer(&book).run_full().expect("the index");

    let report = book.resolution().expect("the honesty report");
    let go = report
        .iter()
        .find(|row| row.lang == "go")
        .unwrap_or_else(|| panic!("the report has no `go` row: {report:?}"))
        .clone();
    println!(
        "S5(b) go package import: {}/{} = {:.1} % ({:?})",
        go.resolved,
        go.edges,
        go.percent(),
        go
    );

    // The six relations of the tree: the import, the call and the method of
    // the caller, the two types the store chapter spells and the method of the
    // receiver. Five resolved was the measurement of f10; the sixth is the
    // import of a package the book holds.
    assert_eq!((go.resolved, go.edges), (6, 6), "{go:?}");
    assert_eq!(go.percent(), 100.0, "{go:?}");
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
#[cfg(feature = "lang-java")]
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
#[cfg(feature = "lang-java")]
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
// JavaScript / JSX (T3 Batch 1)
// ---------------------------------------------------------------------------

#[cfg(feature = "lang-javascript")]
const JAVASCRIPT: &str = r#"
import { Store } from "./store";
import legacy from "./legacy";
import * as helper from "./helper";
export { quiet } from "./quiet";

export class ToolBox extends Base {
  static LIMIT = 3;
  store;

  constructor(store) {
    this.store = store;
  }

  load(input) {
    return this.store.load(input);
  }

  renderWith(other) {
    return openStore(this.store);
  }
}

export class Panel extends ToolBox {
  paint(store) {
    return render(store);
  }
}

export function openStore(store) {
  return new ToolBox(store);
}

export const helper2 = (input) => legacy.run(input);

export function View(props) {
  const label = props.store.load("x");
  return <section className="box"><Panel store={openStore(props.store)} /></section>;
}

class Quiet {
  quiet() {
    // A comment that mentions ignoredInComment() must not produce an edge.
    const text = "alsoIgnoredInAString()";
    return text;
  }
}
"#;

/// T3/S2: the JavaScript chapter keeps the functions, the classes with their
/// methods and the module-level constants, and JSX is the same grammar (the
/// `.jsx` file of `javascript.rs` is parsed by this one).
#[cfg(feature = "lang-javascript")]
#[test]
fn javascript_declares_the_symbols_of_a_chapter_with_its_nesting() {
    let parsed = parse("web/tools.js", JAVASCRIPT.as_bytes(), 0).expect("the JavaScript grammar");
    assert_eq!(parsed.lang, "javascript");
    assert_symbols(
        &parsed,
        &[
            ("class", "ToolBox", "web.tools.ToolBox"),
            ("method", "constructor", "web.tools.ToolBox.constructor"),
            ("method", "load", "web.tools.ToolBox.load"),
            ("method", "renderWith", "web.tools.ToolBox.renderWith"),
            ("class", "Panel", "web.tools.Panel"),
            ("method", "paint", "web.tools.Panel.paint"),
            ("fn", "openStore", "web.tools.openStore"),
            ("const", "helper2", "web.tools.helper2"),
            ("fn", "View", "web.tools.View"),
            ("class", "Quiet", "web.tools.Quiet"),
            ("method", "quiet", "web.tools.Quiet.quiet"),
        ],
    );

    // A method hangs from its class, the same as every language of the book.
    let method = parsed
        .symbols
        .iter()
        .find(|symbol| symbol.qname == "web.tools.ToolBox.renderWith")
        .expect("the method");
    let parent = method.parent.expect("the method hangs from its class");
    assert_eq!(parsed.symbols[parent].name, "ToolBox");
}

/// T3/S7: the three relation kinds of the language — the import, the call by
/// name as written and `extends` — never an edge out of prose or out of JSX
/// markup.
#[cfg(feature = "lang-javascript")]
#[test]
fn javascript_relations_are_syntactic_and_never_prose() {
    let parsed = parse("web/tools.js", JAVASCRIPT.as_bytes(), 0).expect("the grammar");

    for (kind, name) in [
        ("imports", "Store"),
        ("imports", "legacy"),
        ("imports", "helper"),
        ("imports", "quiet"),
        ("calls", "load"),
        ("calls", "openStore"),
        ("calls", "ToolBox"),
        ("calls", "render"),
        ("calls", "run"),
        ("inherits", "Base"),
        ("inherits", "ToolBox"),
    ] {
        assert_edge(&parsed, kind, name);
    }

    assert_edge_owned(
        &parsed,
        "calls",
        "openStore",
        "web.tools.ToolBox.renderWith",
    );
    assert_edge_owned(&parsed, "calls", "run", "web.tools.helper2");
    assert_edge_owned(&parsed, "inherits", "ToolBox", "web.tools.Panel");
    assert_edges_are_anchored(&parsed);
    assert_no_prose_edges(&parsed);
    // JSX is markup: the element and its attribute are not relations.
    for markup in ["section", "Panel", "className", "box"] {
        assert!(
            !parsed.edges.iter().any(|edge| edge.dst_name == markup),
            "`{markup}` is JSX markup: {:?}",
            parsed.edges
        );
    }
}

// ---------------------------------------------------------------------------
// C (T3 Batch 1)
// ---------------------------------------------------------------------------

#[cfg(feature = "lang-c")]
const C: &str = r#"
#include <stdio.h>
#include "store.h"

struct DiagramStore {
    char *path;
    int count;
};

typedef struct DiagramStore Store;

const int LIMIT = 3;
static int counter = 0;

struct Loader {
    int (*load)(Store *store);
};

static int parse_config(char *path) {
    return 0;
}

int open_store(Store *store) {
    return parse_config(store->path);
}

void render_with(Store *store, struct Loader *loader) {
    printf("%d", open_store(store));
    loader->load(store);
}

int quiet(void) {
    /* A comment that mentions ignoredInComment() must not produce an edge. */
    const char *text = "alsoIgnoredInAString()";
    return 0;
}
"#;

/// T3/S2: the C chapter keeps the structs, the typedefs, the file-scope
/// variables and the functions — named by their declarators, never by a
/// `name` field — with the file as the module.
#[cfg(feature = "lang-c")]
#[test]
fn c_declares_the_symbols_of_a_chapter_with_its_nesting() {
    let parsed = parse("c/store.c", C.as_bytes(), 0).expect("the C grammar");
    assert_eq!(parsed.lang, "c");
    assert_symbols(
        &parsed,
        &[
            ("type", "DiagramStore", "c.store.DiagramStore"),
            ("type", "Store", "c.store.Store"),
            ("const", "LIMIT", "c.store.LIMIT"),
            ("static", "counter", "c.store.counter"),
            ("type", "Loader", "c.store.Loader"),
            ("fn", "parse_config", "c.store.parse_config"),
            ("fn", "open_store", "c.store.open_store"),
            ("fn", "render_with", "c.store.render_with"),
            ("fn", "quiet", "c.store.quiet"),
        ],
    );

    // A `struct` specifier with no body is a type reference, not a second
    // declaration of the type.
    let stores = parsed
        .symbols
        .iter()
        .filter(|symbol| symbol.name == "DiagramStore")
        .count();
    assert_eq!(stores, 1, "{:?}", rows(&parsed));
}

/// T3/S7: the `#include`, the call by name as written and the type references
/// of the chapter — never an edge out of prose.
#[cfg(feature = "lang-c")]
#[test]
fn c_relations_are_syntactic_and_never_prose() {
    let parsed = parse("c/store.c", C.as_bytes(), 0).expect("the grammar");

    for (kind, name) in [
        ("imports", "stdio"),
        ("imports", "store"),
        ("calls", "parse_config"),
        ("calls", "open_store"),
        ("calls", "printf"),
        ("calls", "load"),
        ("uses", "Store"),
        ("uses", "Loader"),
    ] {
        assert_edge(&parsed, kind, name);
    }

    assert_edge_owned(&parsed, "calls", "open_store", "c.store.render_with");
    assert_edge_owned(&parsed, "calls", "parse_config", "c.store.open_store");
    assert_edges_are_anchored(&parsed);
    assert_no_prose_edges(&parsed);
}

// ---------------------------------------------------------------------------
// C++ (T3 Batch 1)
// ---------------------------------------------------------------------------

#[cfg(feature = "lang-cpp")]
const CPP: &str = r#"
#include <vector>
#include "store.h"

namespace store {

class Base {};

template <typename T>
class Holder {
public:
    T get();
private:
    T value;
};

class DiagramStore : public Base, private Holder<DiagramStore> {
public:
    DiagramStore(const char *path);
    int load(const char *input);
    int render_with(DiagramStore &other) { return open_store().load("x"); }
private:
    std::vector<int> values;
};

int DiagramStore::load(const char *input) {
    return parse_config(input);
}

int open_store() {
    return 0;
}

int parse_config(const char *input) {
    return 0;
}

const int LIMIT = 3;

using Alias = DiagramStore;

int quiet() {
    // A comment that mentions ignoredInComment() must not produce an edge.
    const char *text = "alsoIgnoredInAString()";
    return 0;
}

} // namespace store
"#;

/// T3/S2: the C++ chapter keeps the classes (with their methods and their
/// method declarations), the functions, the type aliases and the constants;
/// the `namespace` names the source, not the `qname`.
#[cfg(feature = "lang-cpp")]
#[test]
fn cpp_declares_the_symbols_of_a_chapter_with_its_nesting() {
    let parsed = parse("cpp/store.cpp", CPP.as_bytes(), 0).expect("the C++ grammar");
    assert_eq!(parsed.lang, "cpp");
    assert_symbols(
        &parsed,
        &[
            ("class", "Base", "cpp.store.Base"),
            ("class", "Holder", "cpp.store.Holder"),
            ("method", "get", "cpp.store.Holder.get"),
            ("class", "DiagramStore", "cpp.store.DiagramStore"),
            (
                "method",
                "DiagramStore",
                "cpp.store.DiagramStore.DiagramStore",
            ),
            ("method", "load", "cpp.store.DiagramStore.load"),
            (
                "method",
                "render_with",
                "cpp.store.DiagramStore.render_with",
            ),
            ("fn", "load", "cpp.store.load"),
            ("fn", "open_store", "cpp.store.open_store"),
            ("fn", "parse_config", "cpp.store.parse_config"),
            ("const", "LIMIT", "cpp.store.LIMIT"),
            ("type", "Alias", "cpp.store.Alias"),
            ("fn", "quiet", "cpp.store.quiet"),
        ],
    );

    // A data field is not a declaration of the book; the method of the class
    // hangs from the class.
    assert!(
        !parsed.symbols.iter().any(|symbol| symbol.name == "values"),
        "a field is not a declaration: {:?}",
        rows(&parsed)
    );
    let method = parsed
        .symbols
        .iter()
        .find(|symbol| symbol.qname == "cpp.store.DiagramStore.render_with")
        .expect("the method");
    let parent = method.parent.expect("the method hangs from its class");
    assert_eq!(parsed.symbols[parent].name, "DiagramStore");
}

/// T3/S7: the `#include`, the call by name as written, the base classes and
/// the type references — never an edge out of prose.
#[cfg(feature = "lang-cpp")]
#[test]
fn cpp_relations_are_the_four_kinds_and_never_prose() {
    let parsed = parse("cpp/store.cpp", CPP.as_bytes(), 0).expect("the grammar");

    for (kind, name) in [
        ("imports", "vector"),
        ("imports", "store"),
        ("calls", "open_store"),
        ("calls", "load"),
        ("calls", "parse_config"),
        ("inherits", "Base"),
        ("inherits", "Holder"),
        ("uses", "DiagramStore"),
        ("uses", "vector"),
    ] {
        assert_edge(&parsed, kind, name);
    }

    assert_edge_owned(
        &parsed,
        "calls",
        "open_store",
        "cpp.store.DiagramStore.render_with",
    );
    assert_edge_owned(&parsed, "inherits", "Base", "cpp.store.DiagramStore");
    assert_edges_are_anchored(&parsed);
    assert_no_prose_edges(&parsed);
}

// ---------------------------------------------------------------------------
// C# (T3 Batch 1)
// ---------------------------------------------------------------------------

#[cfg(feature = "lang-csharp")]
const CSHARP: &str = r#"
using System;
using System.Collections.Generic;

namespace Graphium.Store;

public interface ILoader
{
    int Load(string input);
}

public class Base
{
    protected string name;
}

public class DiagramStore : Base, ILoader
{
    private readonly List<ILoader> loaders;

    public DiagramStore(List<ILoader> loaders)
    {
        this.loaders = loaders;
    }

    public int Load(string input)
    {
        return ParseConfig(input);
    }

    public DiagramStore RenderWith(DiagramStore other)
    {
        return OpenStore().Load("x");
    }

    public static DiagramStore OpenStore()
    {
        return new DiagramStore(new List<ILoader>());
    }
}

public static class Tools
{
    public static int ParseConfig(string input)
    {
        return input.Length;
    }
}

public record Pair(string Left, string Right);

public enum Mode
{
    Fast,
}

public delegate int Handler(string input);

public class Quiet
{
    public string Silent()
    {
        // A comment that mentions ignoredInComment() must not produce an edge.
        string text = "alsoIgnoredInAString()";
        return text;
    }
}
"#;

/// T3/S2: the C# chapter keeps the types of the file — a record is a class, a
/// struct a type, an interface a trait and a delegate a type — under the
/// namespace the file declares.
#[cfg(feature = "lang-csharp")]
#[test]
fn csharp_declares_the_symbols_of_a_chapter_with_its_nesting() {
    let parsed = parse("dotnet/Store.cs", CSHARP.as_bytes(), 0).expect("the C# grammar");
    assert_eq!(parsed.lang, "csharp");
    assert_symbols(
        &parsed,
        &[
            ("trait", "ILoader", "Graphium.Store.ILoader"),
            ("method", "Load", "Graphium.Store.ILoader.Load"),
            ("class", "Base", "Graphium.Store.Base"),
            ("class", "DiagramStore", "Graphium.Store.DiagramStore"),
            (
                "method",
                "DiagramStore",
                "Graphium.Store.DiagramStore.DiagramStore",
            ),
            ("method", "Load", "Graphium.Store.DiagramStore.Load"),
            (
                "method",
                "RenderWith",
                "Graphium.Store.DiagramStore.RenderWith",
            ),
            (
                "method",
                "OpenStore",
                "Graphium.Store.DiagramStore.OpenStore",
            ),
            ("class", "Tools", "Graphium.Store.Tools"),
            ("method", "ParseConfig", "Graphium.Store.Tools.ParseConfig"),
            ("class", "Pair", "Graphium.Store.Pair"),
            ("type", "Mode", "Graphium.Store.Mode"),
            ("type", "Handler", "Graphium.Store.Handler"),
            ("class", "Quiet", "Graphium.Store.Quiet"),
            ("method", "Silent", "Graphium.Store.Quiet.Silent"),
        ],
    );

    // A field or a property is not a declaration of the book.
    for field in ["loaders", "name", "Count"] {
        assert!(
            !parsed.symbols.iter().any(|symbol| symbol.name == field),
            "`{field}` is not a declaration: {:?}",
            rows(&parsed)
        );
    }
    let method = parsed
        .symbols
        .iter()
        .find(|symbol| symbol.qname == "Graphium.Store.DiagramStore.RenderWith")
        .expect("the method");
    let parent = method.parent.expect("the method hangs from its class");
    assert_eq!(parsed.symbols[parent].name, "DiagramStore");
}

/// T3/S7: the `using`, the call by name as written, the base list and the type
/// references — never an edge out of prose.
#[cfg(feature = "lang-csharp")]
#[test]
fn csharp_relations_are_the_four_kinds_and_never_prose() {
    let parsed = parse("dotnet/Store.cs", CSHARP.as_bytes(), 0).expect("the grammar");

    for (kind, name) in [
        ("imports", "System"),
        ("imports", "Generic"),
        ("calls", "ParseConfig"),
        ("calls", "OpenStore"),
        ("calls", "Load"),
        ("calls", "DiagramStore"),
        ("calls", "List"),
        ("inherits", "Base"),
        ("inherits", "ILoader"),
        ("uses", "List"),
        ("uses", "ILoader"),
        ("uses", "DiagramStore"),
    ] {
        assert_edge(&parsed, kind, name);
    }

    assert_edge_owned(
        &parsed,
        "calls",
        "ParseConfig",
        "Graphium.Store.DiagramStore.Load",
    );
    assert_edge_owned(&parsed, "inherits", "Base", "Graphium.Store.DiagramStore");
    assert_edges_are_anchored(&parsed);
    assert_no_prose_edges(&parsed);
}

// ---------------------------------------------------------------------------
// PHP (T3 Batch 2)
// ---------------------------------------------------------------------------

#[cfg(feature = "lang-php")]
const PHP: &str = r#"
<?php

namespace Graphium\Store;

use Graphium\Store\DiagramStore;
use Graphium\Toolkit\{Renderer, Panel};

interface Loader
{
    public function load(string $input): int;
}

interface Panel
{
    public function draw(): void;
}

abstract class Base
{
    protected string $name;
}

trait Loggable
{
    public function log(string $message): void
    {
        open_store();
    }
}

class Renderer implements Panel
{
    public function draw(): void
    {
        render_panel($this);
    }
}

class DiagramStore extends Base implements Loader
{
    use Loggable;

    public const LIMIT = 3;

    private Renderer $renderer;

    public function __construct(Renderer $renderer)
    {
        $this->renderer = $renderer;
    }

    public function load(string $input): int
    {
        return parse_config($input);
    }

    public function render_with(DiagramStore $other): DiagramStore
    {
        return new DiagramStore($other->renderer);
    }
}

function open_store(): DiagramStore
{
    return new DiagramStore(new Renderer());
}

function render_panel(Panel $panel): void
{
    $panel->draw();
}

function parse_config(string $input): int
{
    return strlen($input);
}

// A comment that mentions ignoredInComment() must not produce an edge.
function quiet(): string
{
    $text = "alsoIgnoredInAString()";
    return $text;
}
"#;

/// T3/S2: the PHP chapter keeps the types of the file — a trait and an
/// interface are a trait, an enum is a type — with their methods under the
/// `namespace` the file declares, and a `const` per element of a `const` group.
#[cfg(feature = "lang-php")]
#[test]
fn php_declares_the_symbols_of_a_chapter_with_its_nesting() {
    let parsed = parse("src/store.php", PHP.as_bytes(), 0).expect("the PHP grammar");
    assert_eq!(parsed.lang, "php");
    assert_symbols(
        &parsed,
        &[
            ("trait", "Loader", "Graphium\\Store\\Loader"),
            ("method", "load", "Graphium\\Store\\Loader\\load"),
            ("class", "Base", "Graphium\\Store\\Base"),
            ("trait", "Loggable", "Graphium\\Store\\Loggable"),
            ("method", "log", "Graphium\\Store\\Loggable\\log"),
            ("trait", "Panel", "Graphium\\Store\\Panel"),
            ("method", "draw", "Graphium\\Store\\Panel\\draw"),
            ("class", "Renderer", "Graphium\\Store\\Renderer"),
            ("method", "draw", "Graphium\\Store\\Renderer\\draw"),
            ("class", "DiagramStore", "Graphium\\Store\\DiagramStore"),
            (
                "method",
                "__construct",
                "Graphium\\Store\\DiagramStore\\__construct",
            ),
            ("method", "load", "Graphium\\Store\\DiagramStore\\load"),
            (
                "method",
                "render_with",
                "Graphium\\Store\\DiagramStore\\render_with",
            ),
            ("const", "LIMIT", "Graphium\\Store\\DiagramStore\\LIMIT"),
            ("fn", "open_store", "Graphium\\Store\\open_store"),
            ("fn", "render_panel", "Graphium\\Store\\render_panel"),
            ("fn", "parse_config", "Graphium\\Store\\parse_config"),
            ("fn", "quiet", "Graphium\\Store\\quiet"),
        ],
    );

    // A property is not a declaration of the book.
    assert!(
        !parsed
            .symbols
            .iter()
            .any(|symbol| symbol.name == "renderer"),
        "a property is not a declaration: {:?}",
        rows(&parsed)
    );
    let method = parsed
        .symbols
        .iter()
        .find(|symbol| symbol.qname == "Graphium\\Store\\DiagramStore\\render_with")
        .expect("the method");
    let parent = method.parent.expect("the method hangs from its class");
    assert_eq!(parsed.symbols[parent].name, "DiagramStore");
}

/// T3/S2: the `use` of a namespace, the call by name as written (the three
/// call forms and `new`), the inheritance of a class (its base, its interfaces
/// and its traits) and the types of the declarations — never an edge out of
/// prose.
#[cfg(feature = "lang-php")]
#[test]
fn php_relations_are_the_four_kinds_and_never_prose() {
    let parsed = parse("src/store.php", PHP.as_bytes(), 0).expect("the grammar");

    for (kind, name) in [
        ("imports", "DiagramStore"),
        ("imports", "Renderer"),
        ("imports", "Panel"),
        ("calls", "open_store"),
        ("calls", "parse_config"),
        ("calls", "render_panel"),
        ("calls", "draw"),
        ("calls", "DiagramStore"),
        ("calls", "strlen"),
        ("inherits", "Base"),
        ("inherits", "Loader"),
        ("inherits", "Loggable"),
        ("inherits", "Panel"),
        ("uses", "Renderer"),
        ("uses", "DiagramStore"),
        ("uses", "Panel"),
    ] {
        assert_edge(&parsed, kind, name);
    }

    assert_edge_owned(
        &parsed,
        "calls",
        "parse_config",
        "Graphium\\Store\\DiagramStore\\load",
    );
    assert_edge_owned(&parsed, "inherits", "Base", "Graphium\\Store\\DiagramStore");
    assert_edges_are_anchored(&parsed);
    assert_no_prose_edges(&parsed);
}

// ---------------------------------------------------------------------------
// Ruby (T3 Batch 2)
// ---------------------------------------------------------------------------

#[cfg(feature = "lang-ruby")]
const RUBY: &str = r#"
require "json"
require_relative "./lib"

module Graphium
  class Base
  end

  module Loggable
    def log(message)
      open_store()
    end
  end

  class DiagramStore < Base
    include Loggable

    LIMIT = 3

    def initialize(renderer)
      @renderer = renderer
    end

    def load(input)
      parse_config(input)
    end

    def render_with(other)
      other.load("x")
    end

    def self.open_store
      new(nil)
    end
  end
end

def open_store
  Graphium::DiagramStore.new(nil)
end

def parse_config(input)
  input.length
end

# A comment that mentions ignoredInComment() must not produce an edge.
def quiet
  text = "alsoIgnoredInAString()"
  text
end
"#;

/// T3/S2: the Ruby chapter keeps the classes and the modules with their methods
/// under them, the singleton methods and the constants, with the `::` of the
/// language in the `qname`. A metaprogrammed accessor is a call, not a symbol.
#[cfg(feature = "lang-ruby")]
#[test]
fn ruby_declares_the_symbols_of_a_chapter_with_its_nesting() {
    let parsed = parse("lib/store.rb", RUBY.as_bytes(), 0).expect("the Ruby grammar");
    assert_eq!(parsed.lang, "ruby");
    assert_symbols(
        &parsed,
        &[
            ("module", "Graphium", "lib.store::Graphium"),
            ("class", "Base", "lib.store::Graphium::Base"),
            ("module", "Loggable", "lib.store::Graphium::Loggable"),
            ("method", "log", "lib.store::Graphium::Loggable::log"),
            ("class", "DiagramStore", "lib.store::Graphium::DiagramStore"),
            ("const", "LIMIT", "lib.store::Graphium::DiagramStore::LIMIT"),
            (
                "method",
                "initialize",
                "lib.store::Graphium::DiagramStore::initialize",
            ),
            ("method", "load", "lib.store::Graphium::DiagramStore::load"),
            (
                "method",
                "render_with",
                "lib.store::Graphium::DiagramStore::render_with",
            ),
            (
                "method",
                "open_store",
                "lib.store::Graphium::DiagramStore::open_store",
            ),
            ("fn", "open_store", "lib.store::open_store"),
            ("fn", "parse_config", "lib.store::parse_config"),
            ("fn", "quiet", "lib.store::quiet"),
        ],
    );

    // `@renderer = renderer` is an instance variable, not a constant of the
    // book; `text = …` inside a method is not one either.
    for hidden in ["renderer", "text", "message"] {
        assert!(
            !parsed.symbols.iter().any(|symbol| symbol.name == hidden),
            "`{hidden}` is not a declaration: {:?}",
            rows(&parsed)
        );
    }
    let method = parsed
        .symbols
        .iter()
        .find(|symbol| symbol.qname == "lib.store::Graphium::DiagramStore::load")
        .expect("the method");
    let parent = method.parent.expect("the method hangs from its class");
    assert_eq!(parsed.symbols[parent].name, "DiagramStore");
}

/// T3/S2: the `require` by the stem of its path, the send by its method name
/// (and a `new` by the class it names), the superclass and the modules a class
/// includes — never an edge out of prose. Ruby has no `uses`: no type
/// annotations exist in its tree.
#[cfg(feature = "lang-ruby")]
#[test]
fn ruby_relations_are_syntactic_and_never_prose() {
    let parsed = parse("lib/store.rb", RUBY.as_bytes(), 0).expect("the grammar");

    for (kind, name) in [
        ("imports", "json"),
        ("imports", "lib"),
        ("calls", "open_store"),
        ("calls", "parse_config"),
        ("calls", "load"),
        ("calls", "DiagramStore"),
        ("calls", "length"),
        ("inherits", "Base"),
        ("inherits", "Loggable"),
    ] {
        assert_edge(&parsed, kind, name);
    }

    assert_edge_owned(
        &parsed,
        "calls",
        "parse_config",
        "lib.store::Graphium::DiagramStore::load",
    );
    assert_edge_owned(
        &parsed,
        "inherits",
        "Base",
        "lib.store::Graphium::DiagramStore",
    );
    assert!(
        !parsed.edges.iter().any(|edge| edge.kind == "uses"),
        "Ruby declares no type reference: {:?}",
        parsed.edges
    );
    assert_edges_are_anchored(&parsed);
    assert_no_prose_edges(&parsed);
}

// ---------------------------------------------------------------------------
// SQL (T3 Batch 2)
// ---------------------------------------------------------------------------

#[cfg(feature = "lang-sql")]
const SQL: &str = r#"
-- The schema of the book.

CREATE TABLE diagram_store (
    id BIGINT PRIMARY KEY,
    path VARCHAR(255) NOT NULL
);

CREATE TABLE loader (
    id BIGINT PRIMARY KEY,
    store_id BIGINT NOT NULL REFERENCES diagram_store (id)
);

CREATE INDEX loader_store_idx ON loader (store_id);

CREATE TYPE mode AS ENUM ('fast', 'slow');

CREATE VIEW store_view AS
SELECT s.id, l.id AS loader_id
FROM diagram_store AS s
JOIN loader AS l ON l.store_id = s.id;

CREATE FUNCTION open_store (path TEXT) RETURNS BIGINT AS $$
    SELECT 1;
$$ LANGUAGE sql;

-- A comment that mentions ignoredInComment() must not produce an edge.
"#;

/// T3/S2: the SQL chapter keeps the objects a schema declares — tables, views
/// and types as `type`, functions and triggers as `fn` — and never a column,
/// which is a field of that table and not a declaration of the book.
#[cfg(feature = "lang-sql")]
#[test]
fn sql_declares_the_symbols_of_a_chapter_with_its_nesting() {
    let parsed = parse("db/schema.sql", SQL.as_bytes(), 0).expect("the SQL grammar");
    assert_eq!(parsed.lang, "sql");
    assert_symbols(
        &parsed,
        &[
            ("type", "diagram_store", "db.schema.diagram_store"),
            ("type", "loader", "db.schema.loader"),
            ("type", "mode", "db.schema.mode"),
            ("type", "store_view", "db.schema.store_view"),
            ("fn", "open_store", "db.schema.open_store"),
        ],
    );

    // A column and an index are not declarations of the book.
    for hidden in ["id", "path", "store_id", "loader_store_idx"] {
        assert!(
            !parsed.symbols.iter().any(|symbol| symbol.name == hidden),
            "`{hidden}` is not a declaration: {:?}",
            rows(&parsed)
        );
    }
}

/// T3/S2: SQL has no calls and no imports, so its one relation kind is `uses` —
/// a query reads a table, an index reads the table it is built on and a foreign
/// key reads the table it points to. The decision is documented in `sql.rs`.
#[cfg(feature = "lang-sql")]
#[test]
fn sql_relations_are_the_table_references_and_never_prose() {
    let parsed = parse("db/schema.sql", SQL.as_bytes(), 0).expect("the grammar");

    for (kind, name) in [("uses", "diagram_store"), ("uses", "loader")] {
        assert_edge(&parsed, kind, name);
    }

    // The whole vocabulary of SQL is `uses`: no call, no import, no
    // inheritance exists in the tree of a schema.
    for edge in &parsed.edges {
        assert_eq!(edge.kind, "uses", "SQL declares only table references");
    }
    // The view reads the two tables of its query; the foreign key of `loader`
    // reads `diagram_store`.
    assert_edge_owned(&parsed, "uses", "diagram_store", "db.schema.store_view");
    assert_edge_owned(&parsed, "uses", "loader", "db.schema.store_view");
    assert_edge_owned(&parsed, "uses", "diagram_store", "db.schema.loader");
    assert_edges_are_anchored(&parsed);
    assert_no_prose_edges(&parsed);
}

// ---------------------------------------------------------------------------
// Shell (T3 Batch 2)
// ---------------------------------------------------------------------------

#[cfg(feature = "lang-shell")]
const SHELL: &str = r#"
#!/usr/bin/env bash

source "./lib.sh"

LIMIT=3
VERBOSE=true

parse_config() {
    local input="$1"
    printf "%s" "$input"
}

open_store() {
    parse_config "path"
}

build() {
    open_store
    . "./lib.sh"
    echo "alsoIgnoredInAString()" # a comment that mentions ignoredInComment()
}

deploy() {
    local target="$1"
    build "$target"
}
"#;

#[cfg(feature = "lang-shell")]
const SHELL_LIB: &str = r#"
#!/usr/bin/env bash

LIB_VERSION="1.0"

lib_help() {
    printf "usage: build <target>"
}
"#;

/// T3/S2: the Shell chapter keeps the functions of the script and the variables
/// assigned at its root; a `local` of a function body is not a declaration of
/// the book.
#[cfg(feature = "lang-shell")]
#[test]
fn shell_declares_the_symbols_of_a_chapter_with_its_nesting() {
    let parsed = parse("scripts/build.sh", SHELL.as_bytes(), 0).expect("the Shell grammar");
    assert_eq!(parsed.lang, "shell");
    assert_symbols(
        &parsed,
        &[
            ("const", "LIMIT", "scripts.build.LIMIT"),
            ("const", "VERBOSE", "scripts.build.VERBOSE"),
            ("fn", "parse_config", "scripts.build.parse_config"),
            ("fn", "open_store", "scripts.build.open_store"),
            ("fn", "build", "scripts.build.build"),
            ("fn", "deploy", "scripts.build.deploy"),
        ],
    );

    for hidden in ["input", "target"] {
        assert!(
            !parsed.symbols.iter().any(|symbol| symbol.name == hidden),
            "`{hidden}` is a local of a function: {:?}",
            rows(&parsed)
        );
    }
}

/// T3/S2: every command of the script by the name it writes and the `source`/
/// `.` of another script by the stem of its path — never an edge out of prose.
/// A shell has no types, so there is no `uses` and no `inherits`.
#[cfg(feature = "lang-shell")]
#[test]
fn shell_relations_are_syntactic_and_never_prose() {
    let parsed = parse("scripts/build.sh", SHELL.as_bytes(), 0).expect("the grammar");

    for (kind, name) in [
        ("imports", "lib"),
        ("calls", "parse_config"),
        ("calls", "open_store"),
        ("calls", "build"),
        ("calls", "printf"),
        ("calls", "echo"),
    ] {
        assert_edge(&parsed, kind, name);
    }

    assert_edge_owned(&parsed, "calls", "build", "scripts.build.deploy");
    assert_edge_owned(&parsed, "calls", "open_store", "scripts.build.build");
    for edge in &parsed.edges {
        assert!(
            matches!(edge.kind, "calls" | "imports"),
            "a shell has no type reference: {edge:?}"
        );
    }
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
#[cfg(all(
    feature = "lang-rust",
    feature = "lang-typescript",
    feature = "lang-python",
    feature = "lang-go",
    feature = "lang-java"
))]
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
///
/// Since T6 the rate of the three weakest languages (Python, Go, Java) leaves
/// out the targets their runtime and standard library provide: no chapter of a
/// worktree can ever declare `str`, `len` or `String`, so counting them only
/// measures what the book cannot be (the rule is [`Grammar::builtins`] and the
/// guard is the test below). The floors moved UP with the measurement: Python
/// 14,8 → 40,0 %, Go 17,2 → 50,0 %, Java 29,6 → 66,7 %.
#[cfg(all(
    feature = "lang-rust",
    feature = "lang-typescript",
    feature = "lang-python",
    feature = "lang-go",
    feature = "lang-java"
))]
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
        ("python", 36.0),
        ("go", 46.0),
        ("java", 63.0),
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

/// The Python of the runtime rule: it declares `len` itself, so the edge to
/// `len` is measured like any other name — and `int`, which no chapter
/// declares, is left out of the measure.
const PYTHON_DECLARES_A_RUNTIME_NAME: &str = r#"
def length(x: int) -> int:
    return len(x)


def len(x: int) -> int:
    return x
"#;

/// The Python of the runtime rule without the declaration: `len` and `int` are
/// names of the runtime the book does not hold, and `helper` is a name it does.
const PYTHON_RUNTIME_ONLY: &str = r#"
def length(x: int) -> int:
    return helper(x)


def helper(x: int) -> int:
    return len(x)
"#;

/// D3/T6: the rate counts what a worktree CAN resolve. A target that is a name
/// of the language's runtime or standard library (`Grammar::builtins`) leaves
/// the measure — out of the numerator and out of the denominator — **unless the
/// book declares that name**: a chapter that spells `def len` is a declaration
/// like any other and its edges count, so the rule can never hide a resolution.
#[cfg(feature = "lang-python")]
#[test]
fn the_rate_leaves_out_the_names_the_runtime_provides_and_only_those() {
    // `len` declared: 1 call (resolved) + 2 `int` annotations (out of the
    // measure) = 1 edge of 1 resolved.
    let declared = Scratch::new("runtime-declared");
    declared.write("src/lengths.py", PYTHON_DECLARES_A_RUNTIME_NAME);
    let book = Arc::new(declared.open());
    declared.indexer(&book).run_full().expect("the index");
    let report = book.resolution().expect("the honesty report");
    let row = report
        .iter()
        .find(|row| row.lang == "python")
        .unwrap_or_else(|| panic!("the report has no python row: {report:?}"));
    assert_eq!((row.resolved, row.edges), (1, 1), "{row:?}");
    assert_eq!(row.percent(), 100.0);

    // `len` not declared: it leaves the measure with `int`, and the call to the
    // name the book holds stays.
    let runtime = Scratch::new("runtime-only");
    runtime.write("src/lengths.py", PYTHON_RUNTIME_ONLY);
    let book = Arc::new(runtime.open());
    runtime.indexer(&book).run_full().expect("the index");
    let report = book.resolution().expect("the honesty report");
    let row = report
        .iter()
        .find(|row| row.lang == "python")
        .unwrap_or_else(|| panic!("the report has no python row: {report:?}"));
    assert_eq!((row.resolved, row.edges), (1, 1), "{row:?}");

    // The graph keeps every edge: the rule is the metric's, never the book's.
    let parsed = parse(
        "src/lengths.py",
        PYTHON_DECLARES_A_RUNTIME_NAME.as_bytes(),
        0,
    )
    .expect("the grammar");
    for (kind, name) in [("calls", "len"), ("uses", "int")] {
        assert_edge(&parsed, kind, name);
    }
    assert_eq!(parsed.edges.len(), 3, "the chapter keeps its edges");
}

/// S3/L5: the registry answers the four new extensions with the language the
/// table `file` stores (`.ts`/`.tsx` share one module).
#[cfg(all(
    feature = "lang-rust",
    feature = "lang-typescript",
    feature = "lang-python",
    feature = "lang-go",
    feature = "lang-java"
))]
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
    // One grammar per language of the build; TypeScript covers both of its
    // grammars. The expectations are built with the same features that built
    // the grammars (T4/C3), so a slim build is checked against itself.
    assert_eq!(
        GRAMMARS.len(),
        [
            cfg!(feature = "lang-rust"),
            cfg!(feature = "lang-typescript"),
            cfg!(feature = "lang-python"),
            cfg!(feature = "lang-go"),
            cfg!(feature = "lang-java"),
            cfg!(feature = "lang-javascript"),
            cfg!(feature = "lang-c"),
            cfg!(feature = "lang-cpp"),
            cfg!(feature = "lang-csharp"),
            cfg!(feature = "lang-php"),
            cfg!(feature = "lang-ruby"),
            cfg!(feature = "lang-sql"),
            cfg!(feature = "lang-shell"),
            cfg!(feature = "lang-swift"),
            cfg!(feature = "lang-kotlin"),
            cfg!(feature = "lang-dart"),
            cfg!(feature = "lang-scala"),
            cfg!(feature = "lang-r"),
            cfg!(feature = "lang-lua"),
            cfg!(feature = "lang-zig"),
        ]
        .iter()
        .filter(|enabled| **enabled)
        .count()
    );
}

// ---------------------------------------------------------------------------
// D3 for T3 Batch 1: the four new languages, measured on their own tree
// ---------------------------------------------------------------------------

/// D3/S4: the four languages of T3 Batch 1 measure their resolution on a book
/// that holds only their fixtures, with the same rule and the same report as
/// the five of C2 (`Book::resolution()`, `name_norm`, ≤5 definers). They are a
/// separate tree on purpose: the published numbers of the five are measured on
/// the C2 fixtures, and adding chapters with the same common names would move
/// their fanout — the measurement is per tree, never a shared pool that one
/// batch can move.
#[cfg(all(
    feature = "lang-javascript",
    feature = "lang-c",
    feature = "lang-cpp",
    feature = "lang-csharp"
))]
#[test]
fn the_resolution_rate_of_the_batch_one_languages_is_measured_on_their_fixtures() {
    let scratch = Scratch::new("batch-one-resolution");
    scratch.write("web/tools.js", JAVASCRIPT);
    scratch.write("c/store.c", C);
    scratch.write("cpp/store.cpp", CPP);
    scratch.write("dotnet/Store.cs", CSHARP);
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

    for lang in ["javascript", "c", "cpp", "csharp"] {
        let row = by_lang(lang);
        assert!(row.files >= 1, "`{lang}` has a chapter: {row:?}");
        assert!(row.symbols >= 1, "`{lang}` declares something: {row:?}");
        assert!(row.edges >= 5, "`{lang}` declares relations: {row:?}");
        assert!(
            row.resolved <= row.edges,
            "a rate is never over 100 %: {row:?}"
        );
    }

    println!(
        "D3 resolutions (T3 Batch 1): {}",
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

    // The floors are the published numbers of `docs/PRD.md`, rounded down: a
    // measurement that falls under them fails the suite. They are **measured
    // on these fixtures**, never promises.
    for (lang, floor) in [
        ("javascript", 64.0),
        ("c", 79.0),
        ("cpp", 69.0),
        ("csharp", 64.0),
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

// ---------------------------------------------------------------------------
// D3 for T3 Batch 2: the four new languages, measured on their own tree
// ---------------------------------------------------------------------------

/// D3/S4: the four languages of T3 Batch 2 measure their resolution on a book
/// that holds only their fixtures, with the same rule and the same report as
/// the five of C2 and the four of Batch 1 (`Book::resolution()`, `name_norm`,
/// ≤5 definers). They are a separate tree on purpose: the published numbers of
/// the other languages are measured on their own fixtures, and adding chapters
/// with the same common names would move their fanout.
#[cfg(all(
    feature = "lang-php",
    feature = "lang-ruby",
    feature = "lang-sql",
    feature = "lang-shell"
))]
#[test]
fn the_resolution_rate_of_the_batch_two_languages_is_measured_on_their_fixtures() {
    let scratch = Scratch::new("batch-two-resolution");
    scratch.write("src/store.php", PHP);
    scratch.write("lib/store.rb", RUBY);
    scratch.write("db/schema.sql", SQL);
    scratch.write("scripts/build.sh", SHELL);
    scratch.write("scripts/lib.sh", SHELL_LIB);
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

    for lang in ["php", "ruby", "sql", "shell"] {
        let row = by_lang(lang);
        assert!(row.files >= 1, "`{lang}` has a chapter: {row:?}");
        assert!(row.symbols >= 1, "`{lang}` declares something: {row:?}");
        // SQL's relation set is deliberately small (table references only), so
        // the floor of edges is the one of a language with one relation kind.
        assert!(row.edges >= 4, "`{lang}` declares relations: {row:?}");
        assert!(
            row.resolved <= row.edges,
            "a rate is never over 100 %: {row:?}"
        );
    }

    println!(
        "D3 resolutions (T3 Batch 2): {}",
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

    // The floors are the published numbers of `docs/PRD.md`, rounded down: a
    // measurement that falls under them fails the suite. They are **measured
    // on these fixtures**, never promises.
    for (lang, floor) in [
        ("php", 91.0),
        ("ruby", 63.0),
        ("sql", 96.0),
        ("shell", 34.0),
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

// ---------------------------------------------------------------------------
// Swift (T3 Batch 3)
// ---------------------------------------------------------------------------

#[cfg(feature = "lang-swift")]
const SWIFT: &str = r#"
import Foundation
import struct Store.DiagramStore

protocol Loader {
    func load(input: String) -> Int
}

protocol Printable {
    func describe() -> Int
}

struct Limits {
    let max: Int = 3
}

enum Mode {
    case fast
    case slow
}

class Base {
    func load(input: String) -> Int {
        return 0
    }
}

final class DiagramStore: Base, Loader {
    let limit: Int = 3

    init(limit: Int) {
        self.limit = limit
    }

    func load(input: String) -> Int {
        return openStore(input)
    }

    func renderWith(other: DiagramStore) -> Int {
        return other.load(input: "x")
    }

    deinit {
        cleanUp()
    }
}

extension DiagramStore: Printable {
    func describe() -> Int {
        return load(input: "x")
    }
}

typealias Handler = (String) -> Int

let LIMIT = 3

func openStore(_ input: String) -> Int {
    let opened = DiagramStore(limit: 3)
    return opened.load(input: input)
}

func quiet() -> String {
    // A comment that mentions ignoredInComment() must not produce an edge.
    let text = "alsoIgnoredInAString()"
    return text
}
"#;

/// T3/S2: the Swift chapter keeps the protocol as a `trait`, the struct and the
/// enum as `type`, the classes as `class` and the `typealias` as `type`, with
/// their members under them; the root property is a `const`, the type's ones
/// are fields, and an `extension` is an `impl` block of the type it extends.
#[cfg(feature = "lang-swift")]
#[test]
fn swift_declares_the_symbols_of_a_chapter_with_its_nesting() {
    let parsed = parse("src/store.swift", SWIFT.as_bytes(), 0).expect("the Swift grammar");
    assert_eq!(parsed.lang, "swift");
    assert_symbols(
        &parsed,
        &[
            ("trait", "Loader", "src.store.Loader"),
            ("method", "load", "src.store.Loader.load"),
            ("trait", "Printable", "src.store.Printable"),
            ("method", "describe", "src.store.Printable.describe"),
            ("type", "Limits", "src.store.Limits"),
            ("type", "Mode", "src.store.Mode"),
            ("class", "Base", "src.store.Base"),
            ("method", "load", "src.store.Base.load"),
            ("class", "DiagramStore", "src.store.DiagramStore"),
            ("method", "init", "src.store.DiagramStore.init"),
            ("method", "load", "src.store.DiagramStore.load"),
            ("method", "renderWith", "src.store.DiagramStore.renderWith"),
            ("method", "deinit", "src.store.DiagramStore.deinit"),
            // The extension of the type is an impl block, like Rust's.
            ("impl", "DiagramStore", "src.store.DiagramStore"),
            ("method", "describe", "src.store.DiagramStore.describe"),
            ("type", "Handler", "src.store.Handler"),
            ("const", "LIMIT", "src.store.LIMIT"),
            ("fn", "openStore", "src.store.openStore"),
            ("fn", "quiet", "src.store.quiet"),
        ],
    );

    // A property of a type is a field, an enum case is not a declaration, and
    // a local binding is not one either.
    for hidden in ["max", "limit", "fast", "slow", "opened", "text", "input"] {
        assert!(
            !parsed.symbols.iter().any(|symbol| symbol.name == hidden),
            "`{hidden}` is not a declaration: {:?}",
            rows(&parsed)
        );
    }
    let method = parsed
        .symbols
        .iter()
        .find(|symbol| symbol.qname == "src.store.Base.load")
        .expect("the method");
    let parent = method.parent.expect("the method hangs from its class");
    assert_eq!(parsed.symbols[parent].name, "Base");
}

/// T3/S2: the four kinds of the schema as Swift spells them — the import by the
/// last segment of its module, the superclass and the protocols as `inherits`
/// (the conformance of an extension included), every call by its name, and the
/// types a declaration names as `uses` — and never an edge out of prose.
#[cfg(feature = "lang-swift")]
#[test]
fn swift_relations_are_syntactic_and_never_prose() {
    let parsed = parse("src/store.swift", SWIFT.as_bytes(), 0).expect("the grammar");
    assert_edges_are_anchored(&parsed);
    assert_no_prose_edges(&parsed);

    assert_edge(&parsed, "imports", "Foundation");
    assert_edge(&parsed, "imports", "DiagramStore");
    assert_edge_owned(&parsed, "inherits", "Base", "src.store.DiagramStore");
    assert_edge_owned(&parsed, "inherits", "Loader", "src.store.DiagramStore");
    // The conformance an extension adds is the `inherits` of the extension.
    assert_edge(&parsed, "inherits", "Printable");

    assert_edge_owned(&parsed, "calls", "openStore", "src.store.DiagramStore.load");
    assert_edge(&parsed, "calls", "load");
    assert_edge(&parsed, "calls", "DiagramStore");
    assert_edge(&parsed, "calls", "cleanUp");
    assert_edge(&parsed, "uses", "DiagramStore");
    assert_edge(&parsed, "uses", "String");
}

/// S5(a): the Swift chapter of the runtime rule. `print` is a function the
/// standard library provides and no chapter of a worktree declares it;
/// `helper` is a name the book holds.
#[cfg(feature = "lang-swift")]
const SWIFT_RUNTIME_ONLY: &str = r#"
func report() {
    print(helper())
}

func helper() {}
"#;

/// S5(a)/D5: the rate of Swift counts what a worktree CAN resolve. `print` is a
/// name of the language's runtime and standard library
/// ([`Grammar::builtins`]), so its edge leaves the measure — out of the
/// numerator and out of the denominator — while `helper` stays and resolves.
/// The graph keeps every edge: the rule is the metric's, never the book's.
#[cfg(feature = "lang-swift")]
#[test]
fn the_rate_of_swift_leaves_out_the_names_its_runtime_provides() {
    let row = measured_rate(
        "swift-runtime",
        "src/report.swift",
        SWIFT_RUNTIME_ONLY,
        "swift",
    );
    assert_eq!((row.resolved, row.edges), (1, 1), "{row:?}");
    assert_eq!(row.percent(), 100.0);

    let parsed =
        parse("src/report.swift", SWIFT_RUNTIME_ONLY.as_bytes(), 0).expect("the Swift grammar");
    assert_eq!(
        parsed.edges.len(),
        2,
        "the chapter keeps its edges: {:?}",
        parsed.edges
    );
}

// ---------------------------------------------------------------------------
// Kotlin (T3 Batch 3)
// ---------------------------------------------------------------------------

#[cfg(feature = "lang-kotlin")]
const KOTLIN: &str = r#"
package graphium.store

import graphium.render.Renderer
import graphium.render.panels.*

interface Loader {
    fun load(input: String): Int
}

data class Limits(val max: Int)

enum class Mode {
    Fast,
    Slow,
}

open class Base {
    open fun load(input: String): Int = 0
}

class DiagramStore(private val renderer: Renderer) : Base(), Loader {
    companion object {
        fun open(): DiagramStore {
            return DiagramStore(Renderer())
        }
    }

    override fun load(input: String): Int {
        return parseConfig(input)
    }

    fun renderWith(other: DiagramStore): Int {
        return other.load("x")
    }
}

typealias Handler = (String) -> Int

val LIMIT = 3

fun parseConfig(input: String): Int {
    return DiagramStore(Renderer()).load(input)
}

fun quiet(): String {
    // A comment that mentions ignoredInComment() must not produce an edge.
    val text = "alsoIgnoredInAString()"
    return text
}
"#;

/// T3/S2: the Kotlin chapter is nested under the `package` the file declares,
/// an interface is a `trait`, an enum class a `type`, a data class a `class`
/// (the `data` modifier synthesises members the source never spells) and a
/// companion object a `class` named `Companion` when it declares no name.
#[cfg(feature = "lang-kotlin")]
#[test]
fn kotlin_declares_the_symbols_of_a_chapter_with_its_nesting() {
    let parsed = parse("src/store.kt", KOTLIN.as_bytes(), 0).expect("the Kotlin grammar");
    assert_eq!(parsed.lang, "kotlin");
    assert_symbols(
        &parsed,
        &[
            ("trait", "Loader", "graphium.store.Loader"),
            ("method", "load", "graphium.store.Loader.load"),
            ("class", "Limits", "graphium.store.Limits"),
            ("type", "Mode", "graphium.store.Mode"),
            ("class", "Base", "graphium.store.Base"),
            ("method", "load", "graphium.store.Base.load"),
            ("class", "DiagramStore", "graphium.store.DiagramStore"),
            (
                "class",
                "Companion",
                "graphium.store.DiagramStore.Companion",
            ),
            (
                "method",
                "open",
                "graphium.store.DiagramStore.Companion.open",
            ),
            ("method", "load", "graphium.store.DiagramStore.load"),
            (
                "method",
                "renderWith",
                "graphium.store.DiagramStore.renderWith",
            ),
            ("type", "Handler", "graphium.store.Handler"),
            ("const", "LIMIT", "graphium.store.LIMIT"),
            ("fn", "parseConfig", "graphium.store.parseConfig"),
            ("fn", "quiet", "graphium.store.quiet"),
        ],
    );

    // A constructor `val`, a property of a type and an enum entry are not
    // declarations of the book, and neither is a local binding.
    for hidden in ["max", "renderer", "Fast", "Slow", "input", "text"] {
        assert!(
            !parsed.symbols.iter().any(|symbol| symbol.name == hidden),
            "`{hidden}` is not a declaration: {:?}",
            rows(&parsed)
        );
    }
    let open = parsed
        .symbols
        .iter()
        .find(|symbol| symbol.qname == "graphium.store.DiagramStore.Companion.open")
        .expect("the companion method");
    let parent = open.parent.expect("the member hangs from the companion");
    assert_eq!(parsed.symbols[parent].name, "Companion");
}

/// T3/S2: the import by the last segment of its path, the supertypes of the `:`
/// list as `inherits`, every call by its name and every type a declaration
/// spells as `uses` — never an edge out of prose.
#[cfg(feature = "lang-kotlin")]
#[test]
fn kotlin_relations_are_syntactic_and_never_prose() {
    let parsed = parse("src/store.kt", KOTLIN.as_bytes(), 0).expect("the grammar");
    assert_edges_are_anchored(&parsed);
    assert_no_prose_edges(&parsed);

    assert_edge(&parsed, "imports", "Renderer");
    assert_edge(&parsed, "imports", "panels");
    assert_edge_owned(&parsed, "inherits", "Base", "graphium.store.DiagramStore");
    assert_edge_owned(&parsed, "inherits", "Loader", "graphium.store.DiagramStore");

    assert_edge_owned(
        &parsed,
        "calls",
        "parseConfig",
        "graphium.store.DiagramStore.load",
    );
    assert_edge(&parsed, "calls", "DiagramStore");
    assert_edge(&parsed, "calls", "load");
    assert_edge(&parsed, "calls", "Renderer");
    assert_edge(&parsed, "uses", "DiagramStore");
    assert_edge(&parsed, "uses", "Renderer");
}

/// S5(a): the Kotlin chapter of the runtime rule — `println` is the standard
/// library's, `helper` is the book's.
#[cfg(feature = "lang-kotlin")]
const KOTLIN_RUNTIME_ONLY: &str = r#"
fun report() {
    println(helper())
}

fun helper() {}
"#;

/// S5(a)/D5: the same rule as Swift, asked of Kotlin: `println` is a name of
/// the runtime (`Grammar::builtins`) and leaves the measure, `helper` is a name
/// of the book and stays. The chapter keeps both edges.
#[cfg(feature = "lang-kotlin")]
#[test]
fn the_rate_of_kotlin_leaves_out_the_names_its_runtime_provides() {
    let row = measured_rate(
        "kotlin-runtime",
        "src/report.kt",
        KOTLIN_RUNTIME_ONLY,
        "kotlin",
    );
    assert_eq!((row.resolved, row.edges), (1, 1), "{row:?}");
    assert_eq!(row.percent(), 100.0);

    let parsed =
        parse("src/report.kt", KOTLIN_RUNTIME_ONLY.as_bytes(), 0).expect("the Kotlin grammar");
    assert_eq!(
        parsed.edges.len(),
        2,
        "the chapter keeps its edges: {:?}",
        parsed.edges
    );
}

// ---------------------------------------------------------------------------
// Dart (T3 Batch 3)
// ---------------------------------------------------------------------------

#[cfg(feature = "lang-dart")]
const DART: &str = r#"
import 'package:graphium/render.dart';
import 'dart:async' as async;

mixin Loggable {
  void log(String message) {}
}

abstract class Loader {
  int load(String input);
}

enum Mode { fast, slow }

class Base with Loggable {
  int load(String input) => 0;
}

class DiagramStore extends Base with Loggable implements Loader {
  static const int limit = 3;

  final Renderer renderer;

  DiagramStore(this.renderer);

  @override
  int load(String input) {
    return parseConfig(input);
  }

  int renderWith(DiagramStore other) {
    return other.load('x');
  }
}

typedef Handler = int Function(String input);

const int LIMIT = 3;

int parseConfig(String input) {
  final DiagramStore opened = DiagramStore(Renderer());
  return opened.load(input);
}

String quiet() {
  // A comment that mentions ignoredInComment() must not produce an edge.
  var text = 'alsoIgnoredInAString()';
  return text;
}
"#;

/// T3/S2: the Dart chapter keeps a class as `class`, a mixin as `trait` and an
/// enum and a `typedef` as `type`, with the methods and the constructors of a
/// class under it and the top-level variables as `const`.
#[cfg(feature = "lang-dart")]
#[test]
fn dart_declares_the_symbols_of_a_chapter_with_its_nesting() {
    let parsed = parse("lib/store.dart", DART.as_bytes(), 0).expect("the Dart grammar");
    assert_eq!(parsed.lang, "dart");
    assert_symbols(
        &parsed,
        &[
            ("trait", "Loggable", "lib.store.Loggable"),
            ("method", "log", "lib.store.Loggable.log"),
            ("class", "Loader", "lib.store.Loader"),
            ("method", "load", "lib.store.Loader.load"),
            ("type", "Mode", "lib.store.Mode"),
            ("class", "Base", "lib.store.Base"),
            ("method", "load", "lib.store.Base.load"),
            ("class", "DiagramStore", "lib.store.DiagramStore"),
            (
                "method",
                "DiagramStore",
                "lib.store.DiagramStore.DiagramStore",
            ),
            ("method", "load", "lib.store.DiagramStore.load"),
            ("method", "renderWith", "lib.store.DiagramStore.renderWith"),
            ("type", "Handler", "lib.store.Handler"),
            ("const", "LIMIT", "lib.store.LIMIT"),
            ("fn", "parseConfig", "lib.store.parseConfig"),
            ("fn", "quiet", "lib.store.quiet"),
        ],
    );

    // A field of a class, an enum entry and a local binding are not
    // declarations of the book.
    for hidden in [
        "limit", "renderer", "fast", "slow", "opened", "text", "input",
    ] {
        assert!(
            !parsed.symbols.iter().any(|symbol| symbol.name == hidden),
            "`{hidden}` is not a declaration: {:?}",
            rows(&parsed)
        );
    }
}

/// T3/S2: an import by the stem of its URI (or its alias), `extends`, `with`
/// (a mixin is an embedding: PHP's trait and Ruby's `include`) and `implements`
/// as `inherits`, every call by its name and every type a declaration spells as
/// `uses` — never an edge out of prose.
#[cfg(feature = "lang-dart")]
#[test]
fn dart_relations_are_syntactic_and_never_prose() {
    let parsed = parse("lib/store.dart", DART.as_bytes(), 0).expect("the grammar");
    assert_edges_are_anchored(&parsed);
    assert_no_prose_edges(&parsed);

    assert_edge(&parsed, "imports", "render");
    assert_edge(&parsed, "imports", "async");
    assert_edge_owned(&parsed, "inherits", "Base", "lib.store.DiagramStore");
    assert_edge_owned(&parsed, "inherits", "Loggable", "lib.store.DiagramStore");
    assert_edge_owned(&parsed, "inherits", "Loader", "lib.store.DiagramStore");

    assert_edge_owned(
        &parsed,
        "calls",
        "parseConfig",
        "lib.store.DiagramStore.load",
    );
    assert_edge(&parsed, "calls", "DiagramStore");
    assert_edge(&parsed, "calls", "load");
    assert_edge(&parsed, "calls", "Renderer");
    assert_edge(&parsed, "uses", "DiagramStore");
    assert_edge(&parsed, "uses", "Renderer");
}

/// S5(c): `class DiagramStore = Base with Loggable;` — the form the grammar
/// names `mixin_application_class`, the mixin application **class** the book's
/// note deferred for rarity — is a declaration like every other one: the name
/// the document gives it, the kind the source spells and the span of the
/// declaration, with the `with` clause read as the embedding it is.
///
/// The literal `mixin C = Base with M;` spelling is not Dart syntax: the
/// grammar recovers it as a `top_level_variable_declaration` (a `const` named
/// `C` of a type spelled `mixin`), which is what the note of this chapter was
/// pointing at. The node the book has to read is the one the grammar defines.
#[cfg(feature = "lang-dart")]
const DART_MIXIN_APPLICATION: &str = r#"
class Base {
  int load(String input) => 0;
}

mixin Loggable {
  void log(String message) {}
}

class DiagramStore = Base with Loggable;
"#;

/// S5(c)/D5: the mixin application is a declaration of the book. It was the one
/// Dart form the chapter did not read: the grammar gives the node no `name`
/// field (`class_declaration` wraps a `mixin_application_class` that holds the
/// identifier), so the walk found no name and the declaration — with the
/// `inherits Loggable` of its `with` clause — was lost.
#[cfg(feature = "lang-dart")]
#[test]
fn a_dart_mixin_application_is_a_declaration() {
    let parsed =
        parse("lib/store.dart", DART_MIXIN_APPLICATION.as_bytes(), 0).expect("the grammar");
    assert_eq!(parsed.lang, "dart");

    assert_symbols(
        &parsed,
        &[("class", "DiagramStore", "lib.store.DiagramStore")],
    );
    // The mixin it applies is the relation of the declaration, not of the
    // chapter: the embedding hangs from `DiagramStore`.
    assert_edge_owned(&parsed, "inherits", "Loggable", "lib.store.DiagramStore");

    // One declaration of the name the document spells, and not the `const` a
    // top-level variable would be (a field is never a declaration).
    let declared: Vec<&str> = parsed
        .symbols
        .iter()
        .filter(|symbol| symbol.qname == "lib.store.DiagramStore")
        .map(|symbol| symbol.kind)
        .collect();
    assert_eq!(declared, ["class"], "{:?}", rows(&parsed));

    // The book of the same file: the declaration is measured like every other
    // relation of the language, and this is the number the cut moves (D5).
    let row = measured_rate(
        "dart-mixin-application",
        "lib/store.dart",
        DART_MIXIN_APPLICATION,
        "dart",
    );
    println!(
        "S5(c) dart mixin application: {}/{} = {:.1} % ({row:?})",
        row.resolved,
        row.edges,
        row.percent()
    );
}

/// S5(a): the Dart chapter of the runtime rule — `print` is `dart:core`'s,
/// `helper` is the book's.
#[cfg(feature = "lang-dart")]
const DART_RUNTIME_ONLY: &str = r#"
void report() {
  print(helper());
}

void helper() {}
"#;

/// S5(a)/D5: Dart measured `7/27` before its runtime list; `print` is a name of
/// `dart:core` (`Grammar::builtins`) and leaves the measure, `helper` stays.
/// The chapter keeps both edges.
#[cfg(feature = "lang-dart")]
#[test]
fn the_rate_of_dart_leaves_out_the_names_its_runtime_provides() {
    let row = measured_rate("dart-runtime", "lib/report.dart", DART_RUNTIME_ONLY, "dart");
    assert_eq!((row.resolved, row.edges), (1, 1), "{row:?}");
    assert_eq!(row.percent(), 100.0);

    let parsed =
        parse("lib/report.dart", DART_RUNTIME_ONLY.as_bytes(), 0).expect("the Dart grammar");
    assert_eq!(
        parsed.edges.len(),
        2,
        "the chapter keeps its edges: {:?}",
        parsed.edges
    );
}

// ---------------------------------------------------------------------------
// D3 for T3 Batch 3: the three new languages, measured on their own tree
// ---------------------------------------------------------------------------

/// D3/S4: the three languages of T3 Batch 3 measure their resolution on a book
/// that holds only their fixtures, with the same rule and the same report as the
/// five of C2 and the four of each earlier batch (`Book::resolution()`,
/// `name_norm`, ≤5 definers). They are a separate tree on purpose: the
/// published numbers of the other languages are measured on their own fixtures,
/// and adding chapters with the same common names would move their fanout.
#[cfg(all(feature = "lang-swift", feature = "lang-kotlin", feature = "lang-dart"))]
#[test]
fn the_resolution_rate_of_the_batch_three_languages_is_measured_on_their_fixtures() {
    let scratch = Scratch::new("batch-three-resolution");
    scratch.write("src/store.swift", SWIFT);
    scratch.write("src/store.kt", KOTLIN);
    scratch.write("lib/store.dart", DART);
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

    for lang in ["swift", "kotlin", "dart"] {
        let row = by_lang(lang);
        assert!(row.files >= 1, "`{lang}` has a chapter: {row:?}");
        assert!(row.symbols >= 1, "`{lang}` declares something: {row:?}");
        assert!(row.edges >= 10, "`{lang}` declares relations: {row:?}");
        assert!(
            row.resolved <= row.edges,
            "a rate is never over 100 %: {row:?}"
        );
    }

    println!(
        "D3 resolutions (T3 Batch 3): {}",
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

    // The floors are the measured rate of these fixtures, rounded down: a
    // measurement that falls under them fails the suite, and `docs/PRD.md`
    // publishes the same numbers (T5 moves its line for S5(a)). They are
    // **measured on these fixtures**, never promises. Since S5(a) the three
    // languages carry the list of the names their runtime provides, so these
    // are the measurements of the smaller denominator — Swift 7/29 = 24,1 % →
    // 7/12 = 58,3 %, Kotlin 7/26 = 26,9 % → 7/14 = 50,0 %, Dart 7/27 = 25,9 %
    // → 7/13 = 53,8 % — rounded down and never up.
    for (lang, floor) in [("swift", 58.0), ("kotlin", 50.0), ("dart", 53.0)] {
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

// ---------------------------------------------------------------------------
// Scala (T3 Batch 4)
// ---------------------------------------------------------------------------

#[cfg(feature = "lang-scala")]
const SCALA: &str = r#"
package graphium.store

import graphium.render.Renderer
import graphium.paint.{Painter, Spray => Renamed}

trait Loader {
  def load(input: String): Int
}

case class Limits(max: Int) {
  def doubled: Int = max * 2
}

enum Mode {
  case Fast, Slow
}

class DiagramStore(renderer: Renderer) extends Base with Loader {
  val limit: Int = 3

  def load(input: String): Int = {
    parseConfig(input)
  }

  def renderWith(other: DiagramStore): Int = other.load("x")
}

object Defaults extends Loader {
  def load(input: String): Int = 3
}

object Store {
  val Limit: Int = 3

  def open(renderer: Renderer): DiagramStore = new DiagramStore(renderer)
}

type Alias = DiagramStore

def parseConfig(input: String): DiagramStore = Store.open(Renderer())

// A comment that mentions ignoredInComment() must not produce an edge.
def quiet(): String = "alsoIgnoredInAString()"
"#;

/// T3/S2: the Scala chapter keeps a `class`, a `case class` as `class`, an
/// `object` as `class` (a singleton object is a class with one instance), a
/// `trait` as `trait` and an `enum` as `type`, nested under the `package` the
/// file declares, with the `def`s of a type under it.
#[cfg(feature = "lang-scala")]
#[test]
fn scala_declares_the_symbols_of_a_chapter_with_its_nesting() {
    let parsed = parse("scala/store.scala", SCALA.as_bytes(), 0).expect("the Scala grammar");
    assert_eq!(parsed.lang, "scala");
    assert_symbols(
        &parsed,
        &[
            ("trait", "Loader", "graphium.store.Loader"),
            ("method", "load", "graphium.store.Loader.load"),
            ("class", "Limits", "graphium.store.Limits"),
            ("method", "doubled", "graphium.store.Limits.doubled"),
            ("type", "Mode", "graphium.store.Mode"),
            ("class", "DiagramStore", "graphium.store.DiagramStore"),
            ("method", "load", "graphium.store.DiagramStore.load"),
            (
                "method",
                "renderWith",
                "graphium.store.DiagramStore.renderWith",
            ),
            ("class", "Defaults", "graphium.store.Defaults"),
            ("method", "load", "graphium.store.Defaults.load"),
            ("class", "Store", "graphium.store.Store"),
            ("method", "open", "graphium.store.Store.open"),
            ("type", "Alias", "graphium.store.Alias"),
            ("fn", "parseConfig", "graphium.store.parseConfig"),
            ("fn", "quiet", "graphium.store.quiet"),
        ],
    );

    // A class parameter, a field, an enum case and a parameter of a `def` are
    // not declarations of the book.
    for hidden in ["max", "limit", "Fast", "Slow", "input", "renderer"] {
        assert!(
            !parsed.symbols.iter().any(|symbol| symbol.name == hidden),
            "`{hidden}` is not a declaration: {:?}",
            rows(&parsed)
        );
    }

    // The containment tree: a method of an `object` hangs from the object,
    // which the book keeps as the class it is.
    let open = parsed
        .symbols
        .iter()
        .find(|symbol| symbol.qname == "graphium.store.Store.open")
        .expect("the method");
    let parent = open.parent.expect("the method hangs from its object");
    assert_eq!(parsed.symbols[parent].kind, "class");
    assert_eq!(parsed.symbols[parent].name, "Store");
}

/// T3/S2: `imports` by the last segment (or the alias of a renamed import),
/// every call by its name, `extends`/`with` as `inherits` and every type a
/// declaration spells as `uses` — never an edge out of prose.
#[cfg(feature = "lang-scala")]
#[test]
fn scala_relations_are_syntactic_and_never_prose() {
    let parsed = parse("scala/store.scala", SCALA.as_bytes(), 0).expect("the grammar");
    assert_edges_are_anchored(&parsed);
    assert_no_prose_edges(&parsed);

    assert_edge(&parsed, "imports", "Renderer");
    assert_edge(&parsed, "imports", "Painter");
    assert_edge(&parsed, "imports", "Renamed");

    assert_edge(&parsed, "calls", "parseConfig");
    assert_edge(&parsed, "calls", "load");
    assert_edge(&parsed, "calls", "open");
    assert_edge(&parsed, "calls", "Renderer");
    assert_edge(&parsed, "calls", "DiagramStore");

    assert_edge(&parsed, "inherits", "Base");
    assert_edge(&parsed, "inherits", "Loader");

    assert_edge(&parsed, "uses", "Renderer");
    assert_edge(&parsed, "uses", "DiagramStore");
    assert_edge(&parsed, "uses", "Int");

    assert_edge_owned(
        &parsed,
        "calls",
        "parseConfig",
        "graphium.store.DiagramStore.load",
    );
    assert_edge_owned(
        &parsed,
        "calls",
        "load",
        "graphium.store.DiagramStore.renderWith",
    );
    assert_edge_owned(&parsed, "calls", "open", "graphium.store.parseConfig");
    assert_edge_owned(&parsed, "inherits", "Loader", "graphium.store.DiagramStore");
    assert_edge_owned(&parsed, "inherits", "Loader", "graphium.store.Defaults");
}

/// S5(a): the Scala chapter of the runtime rule — `println` is `scala.Predef`'s,
/// `helper` is the book's.
#[cfg(feature = "lang-scala")]
const SCALA_RUNTIME_ONLY: &str = r#"
def report() = {
  println(helper())
}

def helper() = {}
"#;

/// S5(a)/D5: Scala measured `16/33` before its runtime list; `println` is a
/// name of the standard library (`Grammar::builtins`) and leaves the measure,
/// `helper` stays. The chapter keeps both edges.
#[cfg(feature = "lang-scala")]
#[test]
fn the_rate_of_scala_leaves_out_the_names_its_runtime_provides() {
    let row = measured_rate(
        "scala-runtime",
        "scala/report.scala",
        SCALA_RUNTIME_ONLY,
        "scala",
    );
    assert_eq!((row.resolved, row.edges), (1, 1), "{row:?}");
    assert_eq!(row.percent(), 100.0);

    let parsed =
        parse("scala/report.scala", SCALA_RUNTIME_ONLY.as_bytes(), 0).expect("the Scala grammar");
    assert_eq!(
        parsed.edges.len(),
        2,
        "the chapter keeps its edges: {:?}",
        parsed.edges
    );
}

// ---------------------------------------------------------------------------
// R (T3 Batch 4)
// ---------------------------------------------------------------------------

#[cfg(feature = "lang-r")]
const R_SOURCE: &str = r#"
# A comment that mentions ignoredInComment() must not produce an edge.
LIMIT <- 3

loadStore <- function(input) {
  input
}

openStore <- function(path) {
  loadStore(path)
}

parseConfig <- function(input) {
  store <- openStore(input)
  store$load(input)
}

render <- function(store) {
  print(store)
}

source("R/render.R")
library(stats)
"#;

/// T3/S2: an R script declares functions — `f <- function(…)` is a `fn` — and
/// every other top-level binding is a `const`; a binding of a body is not a
/// declaration of the chapter.
#[cfg(feature = "lang-r")]
#[test]
fn r_declares_a_function_per_function_binding() {
    let parsed = parse("R/store.R", R_SOURCE.as_bytes(), 0).expect("the R grammar");
    assert_eq!(parsed.lang, "r");
    assert_symbols(
        &parsed,
        &[
            ("const", "LIMIT", "R.store.LIMIT"),
            ("fn", "loadStore", "R.store.loadStore"),
            ("fn", "openStore", "R.store.openStore"),
            ("fn", "parseConfig", "R.store.parseConfig"),
            ("fn", "render", "R.store.render"),
        ],
    );
    // Parameters and the bindings of a body are not declarations of the book.
    // (`store` appears as the `module` anchor of the chapter, which is not a
    // declaration of the script.)
    for hidden in ["input", "path", "store"] {
        assert!(
            !parsed
                .symbols
                .iter()
                .any(|symbol| symbol.kind != "module" && symbol.name == hidden),
            "`{hidden}` is not a declaration: {:?}",
            rows(&parsed)
        );
    }
}

/// T3/S2: every call by the name the source writes (`store$load` calls `load`)
/// and the file a `source()` reads or a `library()` loads as `imports`; R has no
/// `inherits` and no `uses` (see `crates/gx-index/src/lang/r.rs`).
#[cfg(feature = "lang-r")]
#[test]
fn r_relations_are_calls_and_the_files_a_script_reads() {
    let parsed = parse("R/store.R", R_SOURCE.as_bytes(), 0).expect("the grammar");
    assert_edges_are_anchored(&parsed);
    assert_no_prose_edges(&parsed);

    assert_edge(&parsed, "calls", "loadStore");
    assert_edge(&parsed, "calls", "openStore");
    assert_edge(&parsed, "calls", "print");
    assert_edge(&parsed, "calls", "source");
    assert_edge(&parsed, "calls", "library");
    assert_edge(&parsed, "imports", "render");
    assert_edge(&parsed, "imports", "stats");

    assert_edge_owned(&parsed, "calls", "loadStore", "R.store.openStore");
    assert_edge_owned(&parsed, "calls", "openStore", "R.store.parseConfig");
    assert_edge_owned(&parsed, "calls", "load", "R.store.parseConfig");
    assert_edge_owned(&parsed, "calls", "print", "R.store.render");

    // R has no inheritance and no type annotations: the two kinds it never has
    // are not invented for it.
    for kind in ["inherits", "uses"] {
        assert!(
            !parsed.edges.iter().any(|edge| edge.kind == kind),
            "R has no `{kind}`: {:?}",
            parsed.edges
        );
    }
}

/// S5(a): the R chapter of the runtime rule — `print` is `base`'s, `helper` is
/// the book's.
#[cfg(feature = "lang-r")]
const R_RUNTIME_ONLY: &str = r#"
report <- function() {
  print(helper())
}

helper <- function() {}
"#;

/// S5(a)/D5: R measured `4/8` before its runtime list; `print` is a function of
/// the base package (`Grammar::builtins`) and leaves the measure, `helper`
/// stays. The chapter keeps both edges.
#[cfg(feature = "lang-r")]
#[test]
fn the_rate_of_r_leaves_out_the_names_its_runtime_provides() {
    let row = measured_rate("r-runtime", "R/report.R", R_RUNTIME_ONLY, "r");
    assert_eq!((row.resolved, row.edges), (1, 1), "{row:?}");
    assert_eq!(row.percent(), 100.0);

    let parsed = parse("R/report.R", R_RUNTIME_ONLY.as_bytes(), 0).expect("the R grammar");
    assert_eq!(
        parsed.edges.len(),
        2,
        "the chapter keeps its edges: {:?}",
        parsed.edges
    );
}

// ---------------------------------------------------------------------------
// Lua (T3 Batch 4)
// ---------------------------------------------------------------------------

#[cfg(feature = "lang-lua")]
const LUA: &str = r#"
-- A comment that mentions ignoredInComment() must not produce an edge.
local Store = {}

local LIMIT = 3

local function parse_config(input)
  local text = "alsoIgnoredInAString()"
  return input
end

function Store.load(input)
  return parse_config(input)
end

function Store:render(input)
  return self.load(input)
end

Store.new = function(path)
  return { path = path }
end

function Store.open(path)
  return Store.new(path)
end

local painter = require("render")
"#;

/// T3/S2: a table at the root of a Lua file is its `module` and the functions it
/// declares nest under it — a `fn` for `M.f` and a `method` for `M:m`, the
/// second spelling (`M.f = function`) included — and the other bindings are
/// `const`s.
#[cfg(feature = "lang-lua")]
#[test]
fn a_lua_table_is_the_module_of_its_functions() {
    let parsed = parse("lua/store.lua", LUA.as_bytes(), 0).expect("the Lua grammar");
    assert_eq!(parsed.lang, "lua");
    assert_symbols(
        &parsed,
        &[
            ("module", "Store", "lua.store.Store"),
            ("const", "LIMIT", "lua.store.LIMIT"),
            ("fn", "parse_config", "lua.store.parse_config"),
            ("fn", "load", "lua.store.Store.load"),
            ("method", "render", "lua.store.Store.render"),
            ("fn", "new", "lua.store.Store.new"),
            ("fn", "open", "lua.store.Store.open"),
            ("const", "painter", "lua.store.painter"),
        ],
    );
    // A parameter, a local of a body and a field of a table are not
    // declarations of the book.
    for hidden in ["input", "path", "text"] {
        assert!(
            !parsed.symbols.iter().any(|symbol| symbol.name == hidden),
            "`{hidden}` is not a declaration: {:?}",
            rows(&parsed)
        );
    }

    let open = parsed
        .symbols
        .iter()
        .find(|symbol| symbol.qname == "lua.store.Store.open")
        .expect("the fn");
    let parent = open.parent.expect("the fn hangs from its table");
    assert_eq!(parsed.symbols[parent].kind, "module");
    assert_eq!(parsed.symbols[parent].name, "Store");
}

/// T3/S2: every call by the name the source writes (`self.load` calls `load`)
/// and `require` by the module it names; Lua has no `inherits` and no `uses`.
#[cfg(feature = "lang-lua")]
#[test]
fn lua_relations_are_calls_and_the_modules_a_file_requires() {
    let parsed = parse("lua/store.lua", LUA.as_bytes(), 0).expect("the grammar");
    assert_edges_are_anchored(&parsed);
    assert_no_prose_edges(&parsed);

    assert_edge(&parsed, "calls", "parse_config");
    assert_edge(&parsed, "calls", "load");
    assert_edge(&parsed, "calls", "new");
    assert_edge(&parsed, "calls", "require");
    assert_edge(&parsed, "imports", "render");

    assert_edge_owned(&parsed, "calls", "parse_config", "lua.store.Store.load");
    assert_edge_owned(&parsed, "calls", "load", "lua.store.Store.render");
    assert_edge_owned(&parsed, "calls", "new", "lua.store.Store.open");
    assert_edge_owned(&parsed, "imports", "render", "lua.store.painter");

    for kind in ["inherits", "uses"] {
        assert!(
            !parsed.edges.iter().any(|edge| edge.kind == kind),
            "Lua has no `{kind}`: {:?}",
            parsed.edges
        );
    }
}

// ---------------------------------------------------------------------------
// Zig (T3 Batch 4)
// ---------------------------------------------------------------------------

#[cfg(feature = "lang-zig")]
const ZIG: &str = r#"
const std = @import("std");
const Painter = @import("render.zig");

const LIMIT: usize = 3;

pub const DiagramStore = struct {
    path: []const u8,

    pub fn init(path: []const u8) DiagramStore {
        return .{ .path = path };
    }

    pub fn load(self: *DiagramStore, input: []const u8) usize {
        return parseConfig(input, self.path);
    }
};

pub const Mode = enum { fast, slow };

pub fn parseConfig(input: []const u8, path: []const u8) usize {
    _ = path;
    return input.len;
}

pub fn openStore(path: []const u8) DiagramStore {
    return DiagramStore.init(path);
}

// A comment that mentions ignoredInComment() must not produce an edge.
pub fn quiet(painter: Painter) []const u8 {
    std.debug.print("{s}", .{painter.title});
    return "alsoIgnoredInAString()";
}
"#;

/// T3/S2: a Zig `const` that holds a type of the language is a `type` whose
/// `fn`s are its methods, and any other `const` is a `const` of the file.
#[cfg(feature = "lang-zig")]
#[test]
fn a_zig_const_that_holds_a_type_declares_the_members_of_that_type() {
    let parsed = parse("src/store.zig", ZIG.as_bytes(), 0).expect("the Zig grammar");
    assert_eq!(parsed.lang, "zig");
    assert_symbols(
        &parsed,
        &[
            ("const", "std", "src.store.std"),
            ("const", "Painter", "src.store.Painter"),
            ("const", "LIMIT", "src.store.LIMIT"),
            ("type", "DiagramStore", "src.store.DiagramStore"),
            ("method", "init", "src.store.DiagramStore.init"),
            ("method", "load", "src.store.DiagramStore.load"),
            ("type", "Mode", "src.store.Mode"),
            ("fn", "parseConfig", "src.store.parseConfig"),
            ("fn", "openStore", "src.store.openStore"),
            ("fn", "quiet", "src.store.quiet"),
        ],
    );
    // A field of a struct, an enum member and a parameter are not declarations.
    for hidden in ["path", "fast", "slow", "input", "painter"] {
        assert!(
            !parsed.symbols.iter().any(|symbol| symbol.name == hidden),
            "`{hidden}` is not a declaration: {:?}",
            rows(&parsed)
        );
    }

    let load = parsed
        .symbols
        .iter()
        .find(|symbol| symbol.qname == "src.store.DiagramStore.load")
        .expect("the method");
    let parent = load.parent.expect("the method hangs from its type");
    assert_eq!(parsed.symbols[parent].kind, "type");
    assert_eq!(parsed.symbols[parent].name, "DiagramStore");
}

/// T3/S2: every call by its name, `@import` by the stem of the file it names and
/// every type a declaration spells as `uses`; Zig has no `inherits`.
#[cfg(feature = "lang-zig")]
#[test]
fn zig_relations_are_calls_imports_and_types() {
    let parsed = parse("src/store.zig", ZIG.as_bytes(), 0).expect("the grammar");
    assert_edges_are_anchored(&parsed);
    assert_no_prose_edges(&parsed);

    assert_edge(&parsed, "calls", "parseConfig");
    assert_edge(&parsed, "calls", "init");
    assert_edge(&parsed, "calls", "print");
    assert_edge(&parsed, "imports", "render");
    assert_edge(&parsed, "uses", "DiagramStore");
    assert_edge(&parsed, "uses", "Painter");

    assert_edge_owned(
        &parsed,
        "calls",
        "parseConfig",
        "src.store.DiagramStore.load",
    );
    assert_edge_owned(&parsed, "calls", "init", "src.store.openStore");
    assert_edge_owned(&parsed, "calls", "print", "src.store.quiet");
    assert_edge_owned(
        &parsed,
        "uses",
        "DiagramStore",
        "src.store.DiagramStore.load",
    );

    // Zig is not an object-oriented language: the kind is not invented for it.
    assert!(
        !parsed.edges.iter().any(|edge| edge.kind == "inherits"),
        "Zig has no `inherits`: {:?}",
        parsed.edges
    );
}

/// The chapter a Scala file imports: a real tree has one, and D3 is measured on
/// a tree, so the types the store names are declarations of the book here.
#[cfg(all(
    feature = "lang-scala",
    feature = "lang-r",
    feature = "lang-lua",
    feature = "lang-zig"
))]
const SCALA_RENDER: &str = r#"
package graphium.render

trait Painter {
  def spray(input: Int): Int
}

class Renderer extends Painter {
  def spray(input: Int): Int = input
}
"#;

/// The script an R file reads: `source("R/render.R")` names its stem.
#[cfg(all(
    feature = "lang-scala",
    feature = "lang-r",
    feature = "lang-lua",
    feature = "lang-zig"
))]
const R_RENDER: &str = r#"
render <- function(store) {
  store
}
"#;

/// The module a Lua file requires.
#[cfg(all(
    feature = "lang-scala",
    feature = "lang-r",
    feature = "lang-lua",
    feature = "lang-zig"
))]
const LUA_RENDER: &str = r#"
local render = {}

function render.title(store)
  return store.name
end
"#;

// ---------------------------------------------------------------------------
// D3 for T3 Batch 4: the four new languages, measured on their own tree
// ---------------------------------------------------------------------------

/// D3/S4: the four languages of T3 Batch 4 measure their resolution on a book
/// that holds only their fixtures, with the same rule and the same report as the
/// five of C2 and the four of each earlier batch (`Book::resolution()`,
/// `name_norm`, ≤5 definers). They are a separate tree on purpose: the published
/// numbers of the other languages are measured on their own fixtures, and adding
/// chapters with the same common names would move their fanout. This is the last
/// batch of the approved list: the twenty languages of L6 are all here.
#[cfg(all(
    feature = "lang-scala",
    feature = "lang-r",
    feature = "lang-lua",
    feature = "lang-zig"
))]
#[test]
fn the_resolution_rate_of_the_batch_four_languages_is_measured_on_their_fixtures() {
    let scratch = Scratch::new("batch-four-resolution");
    scratch.write("scala/store.scala", SCALA);
    scratch.write("scala/render.scala", SCALA_RENDER);
    scratch.write("R/store.R", R_SOURCE);
    scratch.write("R/render.R", R_RENDER);
    scratch.write("lua/store.lua", LUA);
    scratch.write("lua/render.lua", LUA_RENDER);
    scratch.write("src/store.zig", ZIG);
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

    for lang in ["scala", "r", "lua", "zig"] {
        let row = by_lang(lang);
        assert!(row.files >= 1, "`{lang}` has a chapter: {row:?}");
        assert!(row.symbols >= 1, "`{lang}` declares something: {row:?}");
        // R and Lua have two relation kinds by decision (no types, no
        // supertypes), so the floor of edges is the one of a language with two.
        assert!(row.edges >= 4, "`{lang}` declares relations: {row:?}");
        assert!(
            row.resolved <= row.edges,
            "a rate is never over 100 %: {row:?}"
        );
    }

    println!(
        "D3 resolutions (T3 Batch 4): {}",
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

    // The floors are the measured rate of these fixtures, rounded down: a
    // measurement that falls under them fails the suite, and `docs/PRD.md`
    // publishes the same numbers (T5 moves its line for S5(a)). They are
    // **measured on these fixtures**, never promises. Since S5(a) Scala and R
    // carry the list of the names their runtime provides, so their floors are
    // the measurement of the smaller denominator — Scala 16/33 = 48,5 % →
    // 16/18 = 88,9 %, R 4/8 = 50,0 % → 4/5 = 80,0 % — rounded down and never
    // up; Lua (4/5) and Zig (7/8) keep their measurement, byte by byte, because
    // neither language has a runtime list.
    for (lang, floor) in [("scala", 88.0), ("r", 80.0), ("lua", 76.0), ("zig", 83.0)] {
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
