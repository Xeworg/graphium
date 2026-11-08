//! The Zig chapter of the book (T3 Batch 4; S2, S4).
//!
//! **The Zig decision: a `const` that holds a type is a type, and the
//! declarations of that type live inside the `const`.**
//!
//! Zig has no class and no namespace keyword: a declaration is `const x = …` or
//! `fn f() …`, and the declarations of a type are written *inside the value of a
//! `const`* —
//!
//! ```zig
//! pub const DiagramStore = struct {
//!     path: []const u8,
//!     pub fn load(self: *DiagramStore) usize { … }
//! };
//! ```
//!
//! So the walk reads one node — `variable_declaration`, the one the grammar
//! spells without any field at all, which is why it is the hook — and decides by
//! the value it holds:
//!
//! - a `struct`, `union`, `enum` or `opaque` value is a **`type`** container: it
//!   is a type of the program (the kind C and C++ give their structs and enums)
//!   and its body is the body of the container, so the `pub fn`s inside nest
//!   under it (`src.store.DiagramStore.load`) and are `method`s;
//! - any other value is a **`const`** of the file (`const LIMIT: usize = 3`),
//!   which is the binding the language refers to by name.
//!
//! A `fn` at the root of the file is a `fn`; the same node inside a type is a
//! `method`. A `container_field` (`path: []const u8`) is not a declaration: it is
//! a field of the language, and no chapter of the book keeps one (Java, C#, C++,
//! Kotlin and Dart all agree). Zig's `test` blocks are not declarations either.
//!
//! **No `inherits`**: Zig is not an object-oriented language — it has no
//! supertypes and no vtables of the language — so the kind is not invented for
//! it. Its composition idiom (`usingnamespace`, a field of a struct type) is a
//! field, and a field is not a declaration.
//!
//! The relations of this grammar (D3):
//!
//! - `calls` — `parseConfig(input, path)`, `store.load(input)` and
//!   `DiagramStore.init(path)`;
//! - `imports` — `@import("render.zig")` by the stem of the path it names
//!   (`@import("std")` → `std`, `@import("render.zig")` → `render`): a builtin
//!   whose name is `@import` is the import statement the language gives;
//! - `uses` — every type a declaration spells: a parameter, a return type, the
//!   type of a `const`, the type of a field.
//!
//! What is Zig's alone is here; the declarations the book keeps are the tables
//! of [`crate::lang::registry::ZIG`] and the pipeline is
//! [`crate::lang::engine`] (S3, T2).

use tree_sitter::Node;

use crate::lang::emit::{dotted_module, emit_named, file_stem, text};
use crate::lang::engine::{Body, Nesting};
use crate::lang::model::ParsedSymbol;
use crate::relations::{push_edges, RawEdge, CALLS, IMPORTS, USES};

/// The language of the chapter, as the table `file` spells it.
pub const LANGUAGE: &str = "zig";

/// The tree-sitter grammar of Zig.
pub fn language(_path: &str) -> tree_sitter::Language {
    tree_sitter::Language::new(tree_sitter_zig::LANGUAGE)
}

/// The module path of a Zig file: its path, because Zig declares no module in
/// the source (`src/store.zig` → `src.store`).
pub fn module_path(path: &str) -> String {
    dotted_module(path, &[])
}

/// A `const` declaration: a `type` container when it holds a type of the
/// language, a `const` of the file otherwise.
///
/// The grammar gives `variable_declaration` no field name at all — the first
/// named child is the identifier and the last is the value — so this is the one
/// hook Zig needs.
pub(crate) fn declaration(
    node: &Node,
    nesting: &Nesting,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) -> Option<Body> {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    let name_node = children.iter().find(|child| child.kind() == "identifier")?;
    let name = text(name_node, source)?;
    // The value is the last named child unless the declaration is only a type
    // (`const LIMIT: usize = 3` has its value after the type, so it is still the
    // last one; a `const x: usize` with no value has none).
    let holds_a_type = children
        .iter()
        .rev()
        .find(|child| !matches!(child.kind(), "identifier" | "builtin_type"))
        .is_some_and(|child| is_container_kind(child.kind()));
    if !holds_a_type {
        emit_named(node, "const", &name, nesting.prefix, nesting.parent, out);
        return None;
    }
    let index = emit_named(node, "type", &name, nesting.prefix, nesting.parent, out)?;
    let prefix = format!("{}{}.", nesting.prefix, out[index].name);
    Some(Body {
        index,
        prefix,
        container: "type",
    })
}

/// Whether a node kind is a type of the language whose body declares members.
fn is_container_kind(kind: &str) -> bool {
    matches!(
        kind,
        "struct_declaration" | "union_declaration" | "enum_declaration" | "opaque_declaration"
    )
}

/// The relations of one Zig chapter, as the grammar sees them.
pub(crate) fn relations(root: &Node, source: &[u8]) -> Vec<RawEdge> {
    let mut out = Vec::new();
    visit(root, source, &mut out);
    out
}

/// Walks the tree once and records every relation it recognises.
fn visit(node: &Node, source: &[u8], out: &mut Vec<RawEdge>) {
    let byte = node.start_byte();
    match node.kind() {
        // `parseConfig(input, path)`, `store.load(input)`.
        "call_expression" => {
            if let Some(name) = called(node, source) {
                push_edges(out, byte, CALLS, vec![name]);
            }
        }
        // `@import("render.zig")` is the import statement of the language.
        "builtin_function" => {
            if let Some(name) = builtin(node, source) {
                if name == "@import" {
                    if let Some(import) = imported(node, source) {
                        push_edges(out, byte, IMPORTS, vec![import]);
                    }
                }
            }
        }
        // The types a declaration spells.
        "parameter" | "container_field" | "variable_declaration" => {
            push_edges(out, byte, USES, types_of_field(node, "type", source));
        }
        "function_declaration" => {
            push_edges(out, byte, USES, types_of_field(node, "type", source));
        }
        _ => {}
    }

    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    for child in children {
        visit(&child, source, out);
    }
}

/// The name of a builtin function, `@import` included.
fn builtin(node: &Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    let identifier = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == "builtin_identifier")?;
    text(&identifier, source)
}

/// The name a call targets: `parseConfig` or the member of `store.load`.
fn called(node: &Node, source: &[u8]) -> Option<String> {
    let target = node.child_by_field_name("function")?;
    match target.kind() {
        "identifier" => text(&target, source),
        "field_expression" => target
            .child_by_field_name("member")
            .and_then(|member| text(&member, source)),
        _ => None,
    }
}

/// The module an `@import(...)` names: the stem of the path it writes.
fn imported(node: &Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    let arguments = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == "arguments")?;
    let mut inner = arguments.walk();
    let string = arguments
        .named_children(&mut inner)
        .find(|child| child.kind() == "string")?;
    let mut content = string.walk();
    let parts: Vec<String> = string
        .named_children(&mut content)
        .filter(|child| child.kind() == "string_content")
        .filter_map(|child| text(&child, source))
        .collect();
    let path = parts.concat();
    if path.is_empty() {
        return None;
    }
    file_stem(&path)
}

/// The names the types of one field spell, recursing into the type expression
/// of the grammar (`?*DiagramStore`, `[]const Renderer`).
fn types_of_field(node: &Node, field: &str, source: &[u8]) -> Vec<String> {
    let mut cursor = node.walk();
    let mut names = Vec::new();
    for child in node.children_by_field_name(field, &mut cursor) {
        type_names_into(&child, source, &mut names);
    }
    names.dedup();
    names
}

/// Collects the names of one type expression: a user type is a plain
/// `identifier` in this grammar, and a builtin one (`usize`, `u8`) is not a
/// declaration of the book.
fn type_names_into(node: &Node, source: &[u8], out: &mut Vec<String>) {
    match node.kind() {
        "identifier" => {
            if let Some(name) = text(node, source) {
                out.push(name);
            }
        }
        // A field of a type is not a declaration and neither is its default
        // value: only the type matters here.
        "container_field" => {}
        _ => {
            let mut cursor = node.walk();
            let children: Vec<Node> = node.named_children(&mut cursor).collect();
            for child in children {
                type_names_into(&child, source, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::lang::registry::ZIG;

    /// A `const` that holds a struct is a type, and the `fn`s inside it are its
    /// methods; any other `const` is a `const` of the file.
    #[test]
    fn a_zig_const_that_holds_a_type_declares_the_members_of_that_type() {
        let source = r#"
const std = @import("std");

const LIMIT: usize = 3;

pub const DiagramStore = struct {
    path: []const u8,

    pub fn load(self: *DiagramStore) usize {
        return self.path.len;
    }
};

pub fn parseConfig(input: []const u8) usize {
    return input.len;
}
"#;
        let parsed = ZIG
            .parse("src/store.zig", source.as_bytes(), 0)
            .expect("parses");
        let rows: Vec<(&str, &str, &str)> = parsed
            .symbols
            .iter()
            .map(|symbol| (symbol.kind, symbol.name.as_str(), symbol.qname.as_str()))
            .collect();
        for expected in [
            ("const", "LIMIT", "src.store.LIMIT"),
            ("type", "DiagramStore", "src.store.DiagramStore"),
            ("method", "load", "src.store.DiagramStore.load"),
            ("fn", "parseConfig", "src.store.parseConfig"),
        ] {
            assert!(rows.contains(&expected), "missing {expected:?}: {rows:?}");
        }
        // A `const` bound to an import is a binding of the file (`std`), and a
        // field of the struct is not a declaration.
        assert!(
            rows.contains(&("const", "std", "src.store.std")),
            "an import binding is a `const` of the file: {rows:?}"
        );
        assert!(
            !parsed.symbols.iter().any(|symbol| symbol.name == "path"),
            "a field is not a declaration: {rows:?}"
        );
        let load = parsed
            .symbols
            .iter()
            .find(|symbol| symbol.qname == "src.store.DiagramStore.load")
            .expect("the method");
        let parent = load.parent.expect("the method hangs from its type");
        assert_eq!(parsed.symbols[parent].kind, "type");
    }

    /// A `test` block declares no name of the book, and a nested `const` of a
    /// type nests under it.
    #[test]
    fn a_zig_test_block_is_not_a_declaration() {
        let source = r#"
const Store = struct {
    const DEFAULT: usize = 3;

    pub fn open() usize {
        return DEFAULT;
    }
};

test "opens a store" {
    _ = Store.open();
}
"#;
        let parsed = ZIG
            .parse("src/store.zig", source.as_bytes(), 0)
            .expect("parses");
        let rows: Vec<(&str, &str, &str)> = parsed
            .symbols
            .iter()
            .map(|symbol| (symbol.kind, symbol.name.as_str(), symbol.qname.as_str()))
            .collect();
        assert!(
            rows.contains(&("type", "Store", "src.store.Store")),
            "{rows:?}"
        );
        assert!(
            rows.contains(&("const", "DEFAULT", "src.store.Store.DEFAULT")),
            "{rows:?}"
        );
        assert!(
            rows.contains(&("method", "open", "src.store.Store.open")),
            "{rows:?}"
        );
        // The `module` anchor is the one symbol a chapter adds for the edges of
        // the file; a `test` block is not one.
        let declared: Vec<(&str, &str)> = parsed
            .symbols
            .iter()
            .filter(|symbol| symbol.kind != "module")
            .map(|symbol| (symbol.kind, symbol.name.as_str()))
            .collect();
        assert_eq!(
            declared,
            [("type", "Store"), ("const", "DEFAULT"), ("method", "open")],
            "a test block declares nothing: {rows:?}"
        );
    }
}
