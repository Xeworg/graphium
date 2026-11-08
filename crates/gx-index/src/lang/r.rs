//! The R chapter of the book (T3 Batch 4; S2, S4).
//!
//! An R file is a script: it declares no types, no modules and no visibility —
//! it evaluates expressions at the top level and binds names with `<-` (or `=`,
//! `<<-`, and their right-hand forms `->`, `->>`). So the chapter keeps the two
//! kinds of declaration the language really has, and nothing else:
//!
//! - **a function** — `parseConfig <- function(input) { … }` is a `fn`: the
//!   assignment binds a name to a first-class function object, which is what the
//!   language calls a function and what the rest of the script calls by name. The
//!   grammar spells the keyword (`function`) in the `name` field of the
//!   definition and never the bound identifier, so the name comes from the
//!   assignment — that is the one hook R needs.
//! - **a binding** — `LIMIT <- 3` is a `const`. Every top-level assignment whose
//!   value is not a function is one, whatever the value is: R has no types, so
//!   there is no `uses` and no `class` to tell a value apart.
//!
//! **What the book does not keep, and why.** `assign("x", 3)` and
//! `x <<- 3` inside a body are calls and assignments of a scope the chapter does
//! not walk (a function body is not a scope of the book in any language); a
//! `setClass("Foo", …)`, an R6 `R6Class("Foo", …)` or a `setRefClass("Foo", …)`
//! **stay calls**: a class synthesised from a call is a semantic pass over a
//! string argument, and the book publishes what the syntax tree spells (Ruby's
//! metaprogramming rule of L8, verbatim).
//!
//! **The two relations R has.**
//!
//! - `calls` — every `call`: `parseConfig(input)`, `store$load(input)` (the name
//!   after `$`/`@` is the member the source calls) and `pkg::fun(input)`.
//! - `imports` — the file a `source()`/`sys.source()` reads, by the stem of the
//!   path it names (`source("R/store.R")` → `store`), and the package a
//!   `library()`/`require()`/`requireNamespace()` loads, by the name it writes.
//!   It is how a script composes other files, and it is read from the callee's
//!   name, exactly as the Shell chapter reads `source`/`.` (L8).
//!
//! R has **no `inherits` and no `uses`**: inheritance lives in string arguments
//! of calls (`setClass("B", contains = "A")`) and a type is a runtime attribute,
//! never a piece of syntax this grammar marks. The fixture measures two relation
//! kinds and the published number says so.
//!
//! What is R's alone is here; the declarations the book keeps are the tables of
//! [`crate::lang::registry::R`] and the pipeline is [`crate::lang::engine`]
//! (S3, T2).

use tree_sitter::Node;

use crate::lang::emit::{dotted_module, emit_named, file_stem, text};
use crate::lang::engine::{Body, Nesting};
use crate::lang::model::ParsedSymbol;
use crate::relations::{push_edges, RawEdge, CALLS, IMPORTS};

/// The language of the chapter, as the table `file` spells it.
pub const LANGUAGE: &str = "r";

/// The tree-sitter grammar of R.
pub fn language(_path: &str) -> tree_sitter::Language {
    tree_sitter::Language::new(tree_sitter_r::LANGUAGE)
}

/// The module path of an R file: its path, because R declares no module
/// (`R/store.R` → `R.store`).
pub fn module_path(path: &str) -> String {
    dotted_module(path, &[])
}

/// A top-level assignment: the name it binds, as a `fn` when the value is a
/// function and as a `const` otherwise.
///
/// The grammar has no `name` on either side of the statement — the operator is a
/// field (`<-`, `=`, `<<-`, `->`, `->>`) and the name is one operand of it — so
/// this is the one hook R needs. An assignment inside a body is never reached
/// (a function is a leaf of the walk), and an assignment to something that is
/// not a plain identifier (`names(x)[1] <- "a"`) binds no name of the book.
pub(crate) fn assignment(
    node: &Node,
    nesting: &Nesting,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) -> Option<Body> {
    if nesting.container.is_some() {
        return None;
    }
    let operator = node.child_by_field_name("operator")?;
    let operator = text(&operator, source)?;
    // `x <- value` on the left, `value -> x` on the right.
    let (name, value) = match operator.as_str() {
        "<-" | "<<-" | "=" | ":=" => (
            node.child_by_field_name("lhs"),
            node.child_by_field_name("rhs"),
        ),
        "->" | "->>" => (
            node.child_by_field_name("rhs"),
            node.child_by_field_name("lhs"),
        ),
        _ => return None,
    };
    let name = name?;
    if name.kind() != "identifier" {
        return None;
    }
    let name = text(&name, source)?;
    let kind = match value {
        Some(value) if value.kind() == "function_definition" => "fn",
        _ => "const",
    };
    emit_named(node, kind, &name, nesting.prefix, nesting.parent, out);
    None
}

/// The relations of one R chapter, as the grammar sees them.
pub(crate) fn relations(root: &Node, source: &[u8]) -> Vec<RawEdge> {
    let mut out = Vec::new();
    visit(root, source, &mut out);
    out
}

/// Walks the tree once and records every relation it recognises.
fn visit(node: &Node, source: &[u8], out: &mut Vec<RawEdge>) {
    let byte = node.start_byte();
    if node.kind() == "call" {
        if let Some(name) = called(node, source) {
            if let Some(import) = imported(&name, node, source) {
                push_edges(out, byte, IMPORTS, vec![import]);
            }
            push_edges(out, byte, CALLS, vec![name]);
        }
    }

    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    for child in children {
        visit(&child, source, out);
    }
}

/// The name a call targets: `parseConfig`, the member of `store$load`, the
/// function of `pkg::fun`. A call whose target the source does not name
/// (`(function(x) x)(1)`) has none.
fn called(node: &Node, source: &[u8]) -> Option<String> {
    let target = node.child_by_field_name("function")?;
    match target.kind() {
        "identifier" => text(&target, source),
        // `store$load(input)` and `pkg::fun(input)` name the member in `rhs`.
        "extract_operator" | "namespace_operator" => target
            .child_by_field_name("rhs")
            .and_then(|rhs| text(&rhs, source)),
        _ => None,
    }
}

/// The file or package a call reads, when the callee is one of the composition
/// functions of the language.
fn imported(callee: &str, node: &Node, source: &[u8]) -> Option<String> {
    let path = match callee {
        "source" | "sys.source" => true,
        "library" | "require" | "requireNamespace" => false,
        _ => return None,
    };
    let mut cursor = node.walk();
    let arguments: Vec<Node> = node
        .children_by_field_name("arguments", &mut cursor)
        .collect();
    let arguments = arguments.first()?;
    let mut inner = arguments.walk();
    let first = arguments.named_children(&mut inner).next()?;
    let value = first.child_by_field_name("value").unwrap_or(first);
    let name = match value.kind() {
        "string" => string_value(&value, source)?,
        "identifier" => text(&value, source)?,
        _ => return None,
    };
    if path {
        file_stem(&name)
    } else {
        Some(name)
    }
}

/// The content of a string literal, without its quotes.
fn string_value(node: &Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    let parts: Vec<String> = node
        .named_children(&mut cursor)
        .filter(|child| child.kind() == "string_content")
        .filter_map(|child| text(&child, source))
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.concat())
    }
}

#[cfg(test)]
mod tests {
    use crate::lang::registry::R;

    /// The two declarations of an R script: a function bound by an assignment is
    /// a `fn`, any other top-level binding a `const`.
    #[test]
    fn a_function_bound_by_an_assignment_is_a_fn() {
        let source = r#"
LIMIT <- 3

parseConfig <- function(input) {
  store <- openStore(input)
  store$load(input)
}

openStore <- function(path = ".") {
  DiagramStore$new(".")
}
"#;
        let parsed = R.parse("R/store.R", source.as_bytes(), 0).expect("parses");
        let rows: Vec<(&str, &str, &str)> = parsed
            .symbols
            .iter()
            .map(|symbol| (symbol.kind, symbol.name.as_str(), symbol.qname.as_str()))
            .collect();
        for expected in [
            ("const", "LIMIT", "R.store.LIMIT"),
            ("fn", "parseConfig", "R.store.parseConfig"),
            ("fn", "openStore", "R.store.openStore"),
        ] {
            assert!(rows.contains(&expected), "missing {expected:?}: {rows:?}");
        }
        // A binding inside a body is not a declaration of the chapter.
        assert!(
            !parsed.symbols.iter().any(|symbol| symbol.name == "store"),
            "a local binding is not a declaration: {rows:?}"
        );
    }

    /// The right-assignment forms bind the name the source writes on the right.
    /// A function written that way parses as a function whose body holds the
    /// operator, so it declares no `fn` — the honest limit of the grammar, which
    /// is why the fixture binds functions with `<-`.
    #[test]
    fn a_right_assignment_binds_the_name_it_writes() {
        let parsed = R.parse("R/store.R", b"3 -> LIMIT\n", 0).expect("parses");
        assert!(
            parsed.symbols.iter().any(|symbol| symbol.kind == "const"
                && symbol.name == "LIMIT"
                && symbol.qname == "R.store.LIMIT"),
            "{:?}",
            parsed.symbols
        );
    }
}
