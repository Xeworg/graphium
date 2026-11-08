//! The PHP chapter of the book (T3 Batch 2; S2, S4).
//!
//! The chapter keeps the types of a file — the classes, the interfaces, the
//! traits (a `trait` and an `interface` are both a `trait` of the vocabulary)
//! and the enums — with their methods as children, nested under the
//! `namespace` the file declares, which is what names it in the language:
//! `Graphium\Store\DiagramStore`. A file with no `namespace` keeps its
//! directory, the way a Java file with no `package` does. The first namespace
//! of the file wins: a file with two of them is legal PHP and rare, and the
//! book names the chapter once.
//!
//! The relations of this grammar are three of the four kinds of the schema,
//! read syntactically (D3):
//!
//! - `calls` — `open_store()`, `$store->load()`, `DiagramStore::open()` and
//!   `new DiagramStore()`;
//! - `imports` — the last segment of every `use` of the namespace
//!   (`Graphium\Store\DiagramStore` → `DiagramStore`, and the clauses of a
//!   group `use Graphium\Store\{Renderer, Panel}`);
//! - `inherits` — the `extends` of a class, every `implements`, and every
//!   `use TraitName` of a class body (a PHP trait is a mixin, which is the
//!   embedding of Go and the `impl Trait for Type` of Rust);
//! - `uses` — every type of a declaration: a parameter, a return, a property.
//!
//! There is no `uses` for a primitive: `string`, `int` and `void` are not
//! declarations of the book, and a name they do not have cannot resolve. An
//! enum case is not a declaration either (Java's enum constants are kept the
//! same way).
//!
//! What is PHP's alone is here; the declarations the book keeps are the tables
//! of [`crate::lang::registry::PHP`] and the pipeline is
//! [`crate::lang::engine`] (S3, T2). The one hook the tables cannot cover is a
//! `const` group: `const LIMIT = 3, OTHER = 4;` names each declaration in a
//! `const_element` child with no field, so the hook emits one `const` per
//! element.

use tree_sitter::Node;

use crate::lang::emit::{dotted_directories, emit_named, text};
use crate::lang::engine::{Body, Nesting};
use crate::lang::model::ParsedSymbol;
use crate::relations::{push_edges, RawEdge, CALLS, IMPORTS, INHERITS, USES};

/// The language of the chapter, as the table `file` spells it.
pub const LANGUAGE: &str = "php";

/// The tree-sitter grammar of PHP: the grammar that also reads the HTML around
/// a `<?php` block, so a template is a chapter and not a file of nothing.
pub fn language(_path: &str) -> tree_sitter::Language {
    tree_sitter::Language::new(tree_sitter_php::LANGUAGE_PHP)
}

/// The module path of a PHP file: the namespace it declares, or its directory
/// when the file declares none.
pub fn module_path(root: &Node, path: &str, source: &[u8]) -> String {
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if child.kind() != "namespace_definition" {
            continue;
        }
        if let Some(name) = child
            .child_by_field_name("name")
            .and_then(|name| text(&name, source))
        {
            return name;
        }
    }
    dotted_directories(path)
}

/// The declarations of a `const` group, one per `const_element` of the group.
pub(crate) fn constants(
    node: &Node,
    nesting: &Nesting,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) -> Option<Body> {
    let mut cursor = node.walk();
    let elements: Vec<Node> = node
        .named_children(&mut cursor)
        .filter(|child| child.kind() == "const_element")
        .collect();
    for element in elements {
        let mut inner = element.walk();
        let name = element
            .named_children(&mut inner)
            .find(|child| child.kind() == "name")
            .and_then(|child| text(&child, source));
        if let Some(name) = name {
            emit_named(
                &element,
                "const",
                &name,
                nesting.prefix,
                nesting.parent,
                out,
            );
        }
    }
    None
}

/// The relations of one PHP chapter, as the grammar sees them.
pub(crate) fn relations(root: &Node, source: &[u8]) -> Vec<RawEdge> {
    let mut out = Vec::new();
    visit(root, source, &mut out);
    out
}

/// Walks the tree once and records every relation it recognises.
fn visit(node: &Node, source: &[u8], out: &mut Vec<RawEdge>) {
    let byte = node.start_byte();
    match node.kind() {
        // `open_store()`, `$store->load()`, `DiagramStore::open()`: the name
        // the source writes, whichever of the three call forms it is.
        "function_call_expression" | "member_call_expression" | "scoped_call_expression" => {
            let target = node
                .child_by_field_name("function")
                .or_else(|| node.child_by_field_name("name"));
            if let Some(name) = target.and_then(|target| name_of(&target, source)) {
                out.push(RawEdge {
                    byte,
                    kind: CALLS,
                    name,
                });
            }
        }
        // `new DiagramStore(…)`: a call to the constructor by the name written.
        "object_creation_expression" => {
            if let Some(name) = created(node, source) {
                out.push(RawEdge {
                    byte,
                    kind: CALLS,
                    name,
                });
            }
        }
        // `extends Base` and `implements Loader, Countable`.
        "base_clause" | "class_interface_clause" => {
            push_edges(out, byte, INHERITS, names_in(node, source));
        }
        // `use Loggable;` inside a class body: the trait is a mixin.
        "use_declaration" => {
            push_edges(out, byte, INHERITS, names_in(node, source));
        }
        // `use Graphium\Store\DiagramStore;` and the clauses of a group.
        "namespace_use_declaration" => {
            push_edges(out, byte, IMPORTS, imported(node, source));
        }
        // The types of the declarations, read from the fields the grammar
        // types as a type.
        "simple_parameter" | "property_promotion_parameter" | "property_declaration" => {
            push_edges(
                out,
                byte,
                USES,
                declared_types(node.child_by_field_name("type"), source),
            );
        }
        "function_definition" | "method_declaration" => {
            push_edges(
                out,
                byte,
                USES,
                declared_types(node.child_by_field_name("return_type"), source),
            );
        }
        _ => {}
    }

    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    for child in children {
        visit(&child, source, out);
    }
}

/// The last segment of a name written with `\`: `Graphium\Store\Store` →
/// `Store`.
fn tail(text: &str) -> String {
    text.rsplit('\\').next().unwrap_or(text).trim().to_string()
}

/// The name a node of the grammar spells, whichever wrapper it uses.
fn name_of(node: &Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "name" | "qualified_name" | "relative_name" | "namespace_name" => {
            text(node, source).map(|text| tail(&text))
        }
        _ => {
            let mut cursor = node.walk();
            let found = node
                .named_children(&mut cursor)
                .find_map(|child| name_of(&child, source));
            found
        }
    }
}

/// Every name a clause of the grammar lists: a base class, an interface, a
/// trait, or the members of a `use A, B;`.
fn names_in(node: &Node, source: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if let Some(name) = name_of(&child, source) {
            names.push(name);
        }
    }
    names
}

/// The names a `namespace_use_declaration` binds: the last segment of the
/// path, or one per clause of a group.
fn imported(node: &Node, source: &[u8]) -> Vec<String> {
    let mut clauses: Vec<String> = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "namespace_use_clause" {
            if let Some(name) = name_of(&child, source) {
                clauses.push(name);
            }
        } else if child.kind() == "namespace_use_group" {
            let mut inner = child.walk();
            for clause in child.named_children(&mut inner) {
                if let Some(name) = name_of(&clause, source) {
                    clauses.push(name);
                }
            }
        }
    }
    if !clauses.is_empty() {
        return clauses;
    }
    // `use Graphium\Store\DiagramStore;`: the path is the whole binding.
    let mut cursor = node.walk();
    let binding = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == "namespace_name")
        .and_then(|name| name_of(&name, source));
    binding.into_iter().collect()
}

/// The class a `new` instantiates, when the source writes one.
fn created(node: &Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    let target = node.named_children(&mut cursor).next()?;
    match target.kind() {
        // `new DiagramStore()` and `new \Graphium\Store\DiagramStore()`.
        "name" | "qualified_name" | "relative_name" => name_of(&target, source),
        // `new $class()` and `new ($factory())()` name nothing the book can
        // read: the class is a value, not a name.
        _ => None,
    }
}

/// Every type a type expression declares, unions and nullables included:
/// `Renderer|Panel` names `Renderer` and `Panel`.
fn declared_types(node: Option<Node>, source: &[u8]) -> Vec<String> {
    let Some(node) = node else {
        return Vec::new();
    };
    let mut names = Vec::new();
    types_into(&node, source, &mut names);
    names
}

/// Collects the names of one type expression.
fn types_into(node: &Node, source: &[u8], out: &mut Vec<String>) {
    match node.kind() {
        "named_type" | "optional_type" | "union_type" | "intersection_type" => {
            let mut cursor = node.walk();
            let children: Vec<Node> = node.named_children(&mut cursor).collect();
            for child in children {
                types_into(&child, source, out);
            }
        }
        "name" | "qualified_name" | "relative_name" => {
            if let Some(name) = name_of(node, source) {
                out.push(name);
            }
        }
        // `string`, `int`, `void`, `mixed`, `static`: a type of the language,
        // not a declaration of the book.
        _ => {}
    }
}
