//! The Shell chapter of the book (T3 Batch 2; S2, S4).
//!
//! The chapter keeps what a shell script declares: the functions
//! (`build() { … }` and `function build { … }` are the same declaration) and
//! the variables assigned at the root of the file, which are the `const` of the
//! script the way a module-level `const` is the const of a JavaScript module. A
//! variable of a function body (`local target="$1"`) is not a declaration of
//! the book: a function is a leaf, so its body is not walked.
//!
//! The chapter is the file: a shell script has no namespace, so the module path
//! is its path (`scripts.build`) and its declarations are flat.
//!
//! The relations are two of the four kinds of the schema, read syntactically:
//!
//! - `calls` — every command of the script by the name it writes:
//!   `open_store`, `build "$target"`, `printf`. A shell has no distinction
//!   between a function of the script and a program of the system — the
//!   grammar sees a command either way — so the book records the name and D3
//!   reports honestly how many of them a declaration of the worktree answers;
//! - `imports` — `source <path>` and `. <path>`, by the stem of the path they
//!   name (`source "./lib.sh"` → `lib`, the convention of a C header and of a
//!   Java import).
//!
//! There is no `inherits` and no `uses`: a shell has no types, so there is no
//! type reference in its tree and no base class to name (JavaScript's case for
//! `uses`).
//!
//! What is Shell's alone is here; the declarations the book keeps are the
//! tables of [`crate::lang::registry::SHELL`] and the pipeline is
//! [`crate::lang::engine`] (S3, T2). Shell needs no hook: a `function_definition`
//! and a `variable_assignment` both spell their name in a field of the grammar.

use tree_sitter::Node;

use crate::lang::emit::{dotted_module, file_stem, text};
use crate::relations::{push_edges, RawEdge, CALLS, IMPORTS};

/// The language of the chapter, as the table `file` spells it.
pub const LANGUAGE: &str = "shell";

/// The tree-sitter grammar of Shell: the grammar of bash answers `.sh` and
/// `.bash`, the two extensions of the language of the book.
pub fn language(_path: &str) -> tree_sitter::Language {
    tree_sitter::Language::new(tree_sitter_bash::LANGUAGE)
}

/// The module path of a shell script: its directories and its name without the
/// extension, dotted (`scripts.build`).
pub fn module_path(relative: &str) -> String {
    dotted_module(relative, &[])
}

/// The relations of one Shell chapter, as the grammar sees them.
pub(crate) fn relations(root: &Node, source: &[u8]) -> Vec<RawEdge> {
    let mut out = Vec::new();
    visit(root, source, &mut out);
    out
}

/// The two commands that read another script instead of running a program.
const SOURCES: [&str; 2] = ["source", "."];

/// Walks the tree once and records every relation it recognises.
fn visit(node: &Node, source: &[u8], out: &mut Vec<RawEdge>) {
    let byte = node.start_byte();
    if node.kind() == "command" {
        let name = node
            .child_by_field_name("name")
            .and_then(|name| text(&name, source));
        match name.as_deref() {
            Some(name) if SOURCES.contains(&name) => {
                push_edges(out, byte, IMPORTS, sourced(node, source));
            }
            Some(name) => out.push(RawEdge {
                byte,
                kind: CALLS,
                name: name.to_string(),
            }),
            None => {}
        }
    }

    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    for child in children {
        visit(&child, source, out);
    }
}

/// The stem of the path a `source`/`.` names.
fn sourced(node: &Node, source: &[u8]) -> Vec<String> {
    let mut cursor = node.walk();
    let path = node
        .named_children(&mut cursor)
        .filter(|child| child.kind() != "command_name")
        .find_map(|argument| path_text(&argument, source));
    path.and_then(|path| file_stem(&path)).into_iter().collect()
}

/// The text of a path argument: a quoted string without its quotes, or a bare
/// word.
fn path_text(node: &Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "string" => {
            let mut cursor = node.walk();
            let content = node
                .named_children(&mut cursor)
                .find(|child| child.kind() == "string_content")
                .and_then(|child| text(&child, source));
            content
        }
        _ => text(node, source),
    }
}
