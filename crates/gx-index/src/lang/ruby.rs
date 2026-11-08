//! The Ruby chapter of the book (T3 Batch 2; S2, S4).
//!
//! The chapter keeps the classes, the modules (a Ruby `module` is a namespace
//! with methods, so it is a `module` of the vocabulary and nests its methods
//! like a class does) and the methods of both, with the nesting of the language
//! in the `qname` (`Graphium::DiagramStore::load`); a `def` outside a class or
//! a module is a `fn` of the file. A constant assigned at the root of the file
//! or of a class body is a `const`.
//!
//! **Scope (honest, syntactic).** Ruby is metaprogrammed: `attr_accessor
//! :name`, `define_method`, `class_eval` and `include` of a module that adds
//! methods declare things the syntax tree does not spell as a declaration. The
//! book keeps what the tree gives — a `def` is a method, an assignment to a
//! `Constant` is a const — and never synthesises a symbol from a call, because
//! that would be a semantic pass, not a syntactic one (D3). `attr_accessor` is
//! therefore read as a call (it is one), not as two accessor methods.
//!
//! The relations are three of the four kinds of the schema, read
//! syntactically:
//!
//! - `calls` — `open_store()`, `other.load("x")`,
//!   `Graphium::DiagramStore.new(nil)` (a call to the constructor by the class
//!   the source writes) and every other `send` by its method name;
//! - `imports` — `require`, `require_relative` and `load` **without a
//!   receiver**, by the stem of the path they name (`require_relative "./lib"`
//!   → `lib`, the convention of a C header and of a Java import); with a
//!   receiver (`YAML.load(file)`) the same name is an ordinary send, so it
//!   stays a call;
//! - `inherits` — the superclass of a `class` and every `include`/`extend`/
//!   `prepend` of a module (a mixin is the embedding of Go and the
//!   `impl Trait for Type` of Rust).
//!
//! There is no `uses`: Ruby has no type annotations, so a type reference does
//! not exist in its tree and the book does not invent one (JavaScript's case).
//!
//! What is Ruby's alone is here; the declarations the book keeps are the tables
//! of [`crate::lang::registry::RUBY`] and the pipeline is
//! [`crate::lang::engine`] (S3, T2). The one hook the tables cannot cover is
//! the constant: `LIMIT = 3` is an `assignment` whose left is a `constant`, so
//! the hook emits it by that left side and nothing else.

use tree_sitter::Node;

use crate::lang::emit::{dotted_module, emit_named, file_stem, text};
use crate::lang::engine::{Body, Nesting};
use crate::lang::model::ParsedSymbol;
use crate::relations::{push_edges, RawEdge, CALLS, IMPORTS, INHERITS};

/// The language of the chapter, as the table `file` spells it.
pub const LANGUAGE: &str = "ruby";

/// The tree-sitter grammar of Ruby: one grammar answers every Ruby path.
pub fn language(_path: &str) -> tree_sitter::Language {
    tree_sitter::Language::new(tree_sitter_ruby::LANGUAGE)
}

/// The module path of a Ruby file: its directories and its name without the
/// extension, dotted (`lib.store`), the idiom of the path-based languages of
/// the book. The symbols nest with the `::` of the language.
pub fn module_path(relative: &str) -> String {
    dotted_module(relative, &[])
}

/// A constant assigned in the scope of the file or of a class body:
/// `LIMIT = 3` is one `const` and `@renderer = renderer` is none.
pub(crate) fn constant(
    node: &Node,
    nesting: &Nesting,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) -> Option<Body> {
    let left = node.child_by_field_name("left")?;
    if left.kind() != "constant" {
        return None;
    }
    let name = text(&left, source)?;
    emit_named(node, "const", &name, nesting.prefix, nesting.parent, out);
    None
}

/// The relations of one Ruby chapter, as the grammar sees them.
pub(crate) fn relations(root: &Node, source: &[u8]) -> Vec<RawEdge> {
    let mut out = Vec::new();
    visit(root, source, &mut out);
    out
}

/// The calls of the language that are a relation of another kind: the file a
/// `require` names, the module an `include` mixes in and the class a `new`
/// instantiates.
const REQUIRES: [&str; 3] = ["require", "require_relative", "load"];
const MIXINS: [&str; 3] = ["include", "extend", "prepend"];

/// Walks the tree once and records every relation it recognises.
fn visit(node: &Node, source: &[u8], out: &mut Vec<RawEdge>) {
    let byte = node.start_byte();
    match node.kind() {
        "call" => {
            let method = node
                .child_by_field_name("method")
                .and_then(|method| text(&method, source));
            let sent = node.child_by_field_name("receiver").is_some();
            match method.as_deref() {
                // Without a receiver the three are the calls that read a file;
                // with one (`YAML.load(file)`) they are an ordinary send.
                Some(method) if REQUIRES.contains(&method) && !sent => {
                    push_edges(out, byte, IMPORTS, required(node, source));
                }
                Some(method) if MIXINS.contains(&method) && !sent => {
                    push_edges(out, byte, INHERITS, mixed_in(node, source));
                }
                // `Graphium::DiagramStore.new(nil)`: the constructor is a call
                // to the class the receiver names. A bare `new(nil)` names no
                // class the syntax knows, so it stays an ordinary call.
                Some("new") => {
                    if let Some(name) = receiver(node, source) {
                        out.push(RawEdge {
                            byte,
                            kind: CALLS,
                            name,
                        });
                    }
                }
                Some(method) => out.push(RawEdge {
                    byte,
                    kind: CALLS,
                    name: method.to_string(),
                }),
                None => {}
            }
        }
        // `class DiagramStore < Base`: the superclass.
        "superclass" => {
            let mut cursor = node.walk();
            let names: Vec<String> = node
                .named_children(&mut cursor)
                .filter_map(|child| text(&child, source))
                .collect();
            push_edges(out, byte, INHERITS, names);
        }
        _ => {}
    }

    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    for child in children {
        visit(&child, source, out);
    }
}

/// The stem of the path a `require` names.
fn required(node: &Node, source: &[u8]) -> Vec<String> {
    let Some(arguments) = node.child_by_field_name("arguments") else {
        return Vec::new();
    };
    let mut cursor = arguments.walk();
    let names: Vec<String> = arguments
        .named_children(&mut cursor)
        .filter_map(|argument| path_text(&argument, source))
        .filter_map(|path| file_stem(&path))
        .collect();
    names
}

/// The modules an `include`/`extend`/`prepend` mixes in.
fn mixed_in(node: &Node, source: &[u8]) -> Vec<String> {
    let Some(arguments) = node.child_by_field_name("arguments") else {
        return Vec::new();
    };
    let mut cursor = arguments.walk();
    arguments
        .named_children(&mut cursor)
        .filter_map(|argument| text(&argument, source))
        .collect()
}

/// The class a call is sent to, when the receiver spells one.
fn receiver(node: &Node, source: &[u8]) -> Option<String> {
    let receiver = node.child_by_field_name("receiver")?;
    match receiver.kind() {
        "constant" | "scope_resolution" => text(&receiver, source),
        _ => None,
    }
}

/// The text of a path argument: a string without its quotes, or a bare word.
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
