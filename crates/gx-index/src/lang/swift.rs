//! The Swift chapter of the book (T3 Batch 3; S2, S4).
//!
//! The chapter keeps the declarations of a file — the classes and actors as
//! `class`, the structs and enums as `type`, the protocols as `trait` (the
//! interface of the vocabulary) and the `typealias`es as `type` — with their
//! methods as children, and the properties declared at the root of the file as
//! `const`. Swift spells the first four with one node kind
//! (`class_declaration`, told apart by its `declaration_kind` keyword), which is
//! the one hook the tables cannot cover. The module of the chapter is the path
//! of the file (`src.store`), because Swift names no package in the source.
//!
//! **The extension decision.** `extension DiagramStore: Printable { … }` adds
//! members (and a conformance) to a type another declaration owns, which is
//! exactly what a Rust `impl Trait for Type` block does: it is a symbol of kind
//! `impl` — named after the type it extends, with its members nested under it —
//! and the conformance it lists is an `inherits` edge, the same relation the
//! declaration itself would spell for that protocol. The book records what the
//! syntax says: the extension is the declaration that conforms, and the edge
//! hangs from it. A type and its extensions in one chapter share a `qname`, and
//! the book numbers the collision the way it numbers two `impl` blocks of one
//! type (see `Book::commit_batch`).
//!
//! The relations of this grammar are the four kinds of the schema, read
//! syntactically (D3):
//!
//! - `calls` — `openStore(input)`, `other.load(input:)`, `DiagramStore(…)`;
//! - `imports` — the last segment of an `import` (`import struct
//!   Store.DiagramStore` → `DiagramStore`, `import Foundation` → `Foundation`);
//! - `inherits` — every `inheritance_specifier` of a declaration: the
//!   superclass, the protocols of a class and the conformance an extension
//!   adds;
//! - `uses` — every type a declaration spells: a parameter, a return type, a
//!   property type, the type of a `typealias`.
//!
//! A `where T: Loader` clause is a constraint on a generic parameter, not a
//! declaration inheriting from `Loader`, so it is not read as `inherits`; an
//! enum case is not a declaration (Java's and C#'s enum constants are kept the
//! same way), a property inside a type is a field and not a declaration of the
//! book, and a function body is not walked.
//!
//! What is Swift's alone is here; the declarations the book keeps are the
//! tables of [`crate::lang::registry::SWIFT`] and the pipeline is
//! [`crate::lang::engine`] (S3, T2).

use tree_sitter::Node;

use crate::lang::emit::{dotted_module, emit_named, text};
use crate::lang::engine::{Body, Nesting};
use crate::lang::model::ParsedSymbol;
use crate::relations::{push_edges, RawEdge, CALLS, IMPORTS, INHERITS, USES};

/// The language of the chapter, as the table `file` spells it.
pub const LANGUAGE: &str = "swift";

/// The tree-sitter grammar of Swift: one grammar answers every Swift path.
pub fn language(_path: &str) -> tree_sitter::Language {
    tree_sitter::Language::new(tree_sitter_swift::LANGUAGE)
}

/// The module path of a Swift file: its directories and its stem
/// (`src/store.swift` → `src.store`). Swift declares no package in the source.
pub fn module_path(relative: &str) -> String {
    dotted_module(relative, &[])
}

/// A `class_declaration`, which is five declarations of the language told apart
/// by the keyword the grammar keeps in `declaration_kind`: a `class` and an
/// `actor` are a class, a `struct` and an `enum` a type, and an `extension` an
/// `impl` block of the type it extends.
pub(crate) fn type_declaration(
    node: &Node,
    nesting: &Nesting,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) -> Option<Body> {
    let kind = declared_kind(node)?;
    let name = declared_name(node, source)?;
    let index = emit_named(node, kind, &name, nesting.prefix, nesting.parent, out)?;
    let prefix = format!("{}{}.", nesting.prefix, out[index].name);
    Some(Body {
        index,
        prefix,
        container: kind,
    })
}

/// The kind of a `class_declaration` in the vocabulary of the book.
fn declared_kind(node: &Node) -> Option<&'static str> {
    let keyword = node.child_by_field_name("declaration_kind")?;
    match keyword.kind() {
        "class" | "actor" => Some("class"),
        "struct" | "enum" => Some("type"),
        // An extension adds members to a type: an impl block of the book.
        "extension" => Some("impl"),
        _ => None,
    }
}

/// The name a `class_declaration` declares: a type name, and for an extension
/// the type it extends (`extension DiagramStore` names `DiagramStore`).
fn declared_name(node: &Node, source: &[u8]) -> Option<String> {
    let name = node.child_by_field_name("name")?;
    match name.kind() {
        "type_identifier" => text(&name, source),
        // `extension Array<Element>`: the extended type, without its arguments.
        "user_type" => name
            .named_children(&mut name.walk())
            .find(|child| child.kind() == "type_identifier")
            .and_then(|child| text(&child, source)),
        _ => text(&name, source),
    }
}

/// A `property_declaration`: a `const` at the root of the file and a field
/// everywhere else. A field is not a declaration of the book (Java, C# and C++
/// keep theirs the same way).
pub(crate) fn property(
    node: &Node,
    nesting: &Nesting,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) -> Option<Body> {
    if nesting.container.is_some() {
        return None;
    }
    let pattern = node.child_by_field_name("name")?;
    let name = bound_name(&pattern, source)?;
    emit_named(node, "const", &name, nesting.prefix, nesting.parent, out);
    None
}

/// The identifier a binding pattern declares: `let LIMIT = 3` names `LIMIT`.
fn bound_name(pattern: &Node, source: &[u8]) -> Option<String> {
    if pattern.kind() == "simple_identifier" {
        return text(pattern, source);
    }
    let mut cursor = pattern.walk();
    let found = pattern
        .named_children(&mut cursor)
        .find(|child| child.kind() == "bound_identifier")
        .and_then(|child| bound_name(&child, source));
    found.or_else(|| text(pattern, source))
}

/// A declaration that names itself with a keyword and not with a field: a
/// `deinit` and a `subscript` are methods of the type that owns them, and
/// Swift spells their name in the source but never in a `name` field.
pub(crate) fn member(
    node: &Node,
    nesting: &Nesting,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) -> Option<Body> {
    let _ = source;
    let name = match node.kind() {
        "deinit_declaration" => "deinit",
        "subscript_declaration" => "subscript",
        _ => return None,
    };
    let kind = if nesting.container.is_some() {
        "method"
    } else {
        "fn"
    };
    emit_named(node, kind, name, nesting.prefix, nesting.parent, out);
    None
}

/// The relations of one Swift chapter, as the grammar sees them.
pub(crate) fn relations(root: &Node, source: &[u8]) -> Vec<RawEdge> {
    let mut out = Vec::new();
    visit(root, source, &mut out);
    out
}

/// Walks the tree once and records every relation it recognises.
fn visit(node: &Node, source: &[u8], out: &mut Vec<RawEdge>) {
    let byte = node.start_byte();
    match node.kind() {
        // `import Foundation`, `import struct Store.DiagramStore`: the name the
        // source binds is the last segment of the module path.
        "import_declaration" => {
            if let Some(name) = imported(node, source) {
                push_edges(out, byte, IMPORTS, vec![name]);
            }
        }
        // `class DiagramStore: Base, Loader` and `extension DiagramStore:
        // Printable`: the superclass first, then every conformance.
        "inheritance_specifier" => {
            if let Some(inherits) = node.child_by_field_name("inherits_from") {
                push_edges(out, byte, INHERITS, type_names(&inherits, source));
            }
        }
        // `openStore(input)`, `other.load(input:)`, `DiagramStore(…)`.
        "call_expression" => {
            if let Some(name) = called(node, source) {
                push_edges(out, byte, CALLS, vec![name]);
            }
        }
        // The types a declaration spells: a parameter, a return type, a
        // property type, the underlying type of a `typealias`.
        "parameter"
        | "property_declaration"
        | "function_declaration"
        | "protocol_function_declaration"
        | "init_declaration"
        | "typealias_declaration" => {
            push_edges(out, byte, USES, declared_types(node, source));
        }
        _ => {}
    }

    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    for child in children {
        visit(&child, source, out);
    }
}

/// The last segment of the module an `import` names.
fn imported(node: &Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    let identifier = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == "identifier")?;
    let path = text(&identifier, source)?;
    let segment = path.rsplit('.').next().unwrap_or(&path).trim();
    if segment.is_empty() {
        None
    } else {
        Some(segment.to_string())
    }
}

/// The name a call expression targets, when the source spells one.
fn called(node: &Node, source: &[u8]) -> Option<String> {
    let target = node.named_child(0)?;
    match target.kind() {
        "simple_identifier" => text(&target, source),
        // `other.load(…)`: the last segment of the navigation.
        "navigation_expression" => {
            let suffix = target.child_by_field_name("suffix")?;
            let name = suffix.child_by_field_name("suffix")?;
            if name.kind() == "simple_identifier" {
                text(&name, source)
            } else {
                // `tuple.0` names an element, never a declaration.
                None
            }
        }
        // A chained call (`f()()`) calls what the inner expression answered,
        // and `self`/`super` are not declarations of the book.
        "call_expression" => called(&target, source),
        _ => None,
    }
}

/// Whether a node kind is a type expression of the Swift grammar.
fn is_type(kind: &str) -> bool {
    matches!(
        kind,
        "user_type"
            | "optional_type"
            | "array_type"
            | "dictionary_type"
            | "tuple_type"
            | "tuple_type_item"
            | "function_type"
            | "lambda_function_type"
            | "protocol_composition_type"
            | "existential_type"
            | "opaque_type"
            | "metatype"
            | "type_annotation"
            | "type_arguments"
            | "type_parameters"
            | "type_parameter"
    )
}

/// Every type a declaration names in its type positions: the parameters, the
/// return type, the property annotation.
fn declared_types(node: &Node, source: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if is_type(child.kind()) {
            type_names_into(&child, source, &mut names);
        }
    }
    names.dedup();
    names
}

/// Every name a type expression spells, generics included.
fn type_names(node: &Node, source: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    type_names_into(node, source, &mut names);
    names.dedup();
    names
}

/// Collects the names of one type expression.
fn type_names_into(node: &Node, source: &[u8], out: &mut Vec<String>) {
    match node.kind() {
        // `Store.DiagramStore<String>` names `DiagramStore` and `String`.
        "user_type" => {
            let mut cursor = node.walk();
            let children: Vec<Node> = node.named_children(&mut cursor).collect();
            for child in children {
                if child.kind() == "type_identifier" {
                    if let Some(path) = text(&child, source) {
                        let name = path.rsplit('.').next().unwrap_or(&path).trim();
                        if !name.is_empty() {
                            out.push(name.to_string());
                        }
                    }
                } else if child.kind() == "type_arguments" {
                    type_names_into(&child, source, out);
                }
            }
        }
        "type_identifier" => {
            if let Some(path) = text(node, source) {
                let name = path.rsplit('.').next().unwrap_or(&path).trim();
                if !name.is_empty() {
                    out.push(name.to_string());
                }
            }
        }
        kind if is_type(kind) => {
            let mut cursor = node.walk();
            let children: Vec<Node> = node.named_children(&mut cursor).collect();
            for child in children {
                type_names_into(&child, source, out);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use crate::lang::registry::SWIFT;

    /// The five declarations Swift spells with one node kind map to the
    /// vocabulary of the book, and an extension is an `impl` of the type it
    /// extends — not a second declaration of that type.
    #[test]
    fn the_declarations_of_swift_map_to_the_vocabulary_of_the_book() {
        let source = r#"
protocol Loader {
    func load(input: String) -> Int
}

struct Limits {
    let max: Int = 3
}

enum Mode {
    case fast
}

actor Counter {
    func bump() -> Int { return bump() }
}

class DiagramStore: Loader {
    func load(input: String) -> Int { return 0 }
}

extension DiagramStore: Printable {
    func describe() -> Int { return load(input: "x") }
}

typealias Handler = (String) -> Int

let LIMIT = 3
"#;
        let parsed = SWIFT
            .parse("src/store.swift", source.as_bytes(), 0)
            .expect("parses");
        let rows: Vec<(&str, &str, &str)> = parsed
            .symbols
            .iter()
            .map(|symbol| (symbol.kind, symbol.name.as_str(), symbol.qname.as_str()))
            .collect();
        for expected in [
            ("trait", "Loader", "src.store.Loader"),
            ("method", "load", "src.store.Loader.load"),
            ("type", "Limits", "src.store.Limits"),
            ("type", "Mode", "src.store.Mode"),
            ("class", "Counter", "src.store.Counter"),
            ("method", "bump", "src.store.Counter.bump"),
            ("class", "DiagramStore", "src.store.DiagramStore"),
            ("method", "load", "src.store.DiagramStore.load"),
            ("impl", "DiagramStore", "src.store.DiagramStore"),
            ("method", "describe", "src.store.DiagramStore.describe"),
            ("type", "Handler", "src.store.Handler"),
            ("const", "LIMIT", "src.store.LIMIT"),
        ] {
            assert!(rows.contains(&expected), "missing {expected:?}: {rows:?}");
        }
        // A property inside a type is a field; an enum case is not a
        // declaration of the book.
        for hidden in ["max", "fast"] {
            assert!(
                !parsed.symbols.iter().any(|symbol| symbol.name == hidden),
                "`{hidden}` is not a declaration: {rows:?}"
            );
        }
    }

    /// An extension is an `impl` block: its conformance is inherited from the
    /// declaration, and the members the type declares elsewhere are not moved.
    #[test]
    fn an_extension_is_an_impl_block_of_the_type_it_extends() {
        let source = r#"
protocol Printable {
    func describe() -> Int
}

struct Point {
    let x: Int = 0
}

extension Point: Printable {
    func describe() -> Int { return 0 }
}
"#;
        let parsed = SWIFT
            .parse("src/point.swift", source.as_bytes(), 0)
            .expect("parses");
        let extension = parsed
            .symbols
            .iter()
            .find(|symbol| symbol.kind == "impl")
            .expect("the extension is an impl");
        assert_eq!(extension.name, "Point");
        let parent = extension.parent;
        assert_eq!(parent, None, "the extension hangs from the file");
        let method = parsed
            .symbols
            .iter()
            .find(|symbol| symbol.qname == "src.point.Point.describe")
            .expect("the member of the extension");
        assert_eq!(
            parsed.symbols[method.parent.expect("a parent")].kind,
            "impl",
            "the member of an extension hangs from the extension"
        );
        let inherits: Vec<&str> = parsed
            .edges
            .iter()
            .filter(|edge| edge.kind == "inherits")
            .map(|edge| edge.dst_name.as_str())
            .collect();
        assert!(
            inherits.contains(&"Printable"),
            "the conformance of the extension is inherits: {inherits:?}"
        );
    }
}
