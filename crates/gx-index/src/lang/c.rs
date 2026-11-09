//! The C chapter of the book (T3 Batch 1; S2, S4).
//!
//! The chapter keeps the declarations of a translation unit — the functions,
//! the structs, the unions, the enums, the typedefs and the file-scope
//! variables (a `const` when the declaration says so, the storage otherwise) —
//! with the path of the file as the prefix of every `qname`
//! (`src.store.open_store`). A function body is not walked: the declarations of
//! a function are local names, and the book keeps the declarations of the unit.
//!
//! The relations of this grammar are three of the four kinds of the schema,
//! read syntactically (D3):
//!
//! - `calls` — `open_store()`, `store->load()`, `(*handler)()`;
//! - `imports` — the header an `#include` names (`<stdio.h>` → `stdio.h`);
//! - `uses` — every name the grammar types as a type: a parameter, a return,
//!   a cast, the tag of a `struct`.
//!
//! C has no inheritance, so `inherits` does not exist here. A header is not a
//! declaration of the chapter: the `imports` target is the name the source
//! spells, and it resolves only when the book declares something with that
//! name (D3 is honest about it).
//!
//! What is C's alone is here; the declarations the book keeps are the tables of
//! [`crate::lang::registry::C`] and the pipeline is [`crate::lang::engine`]
//! (S3, T2). The hooks are the ones the tables cannot express: a
//! `function_definition` is named by its declarator (never by a `name` field),
//! a `struct`/`union`/`enum` specifier is a declaration only when it has a
//! body (the same node is a type reference when it has none), and a
//! `declaration` is one symbol per declarator.

use crate::lang::emit::{contains_kind, declarator_name, emit_named, text};
use crate::lang::engine::{Body, Nesting};
use crate::lang::model::ParsedSymbol;
use crate::relations::{push_edges, RawEdge, CALLS, IMPORTS, USES};

/// The language of the chapter, as the table `file` spells it.
pub const LANGUAGE: &str = "c";

/// The tree-sitter grammar of C: one grammar answers every C path. A header
/// (`.h`) is C here, always: the extension is answered by one grammar of the
/// build and never depends on which features a build enabled (C++ headers are
/// `.hpp`, `.hh`, `.hxx` or `.h++`).
pub fn language(_path: &str) -> tree_sitter::Language {
    tree_sitter::Language::new(tree_sitter_c::LANGUAGE)
}

/// The module path of a C file: its directories and its name without the
/// extension, dotted. C has no module system, so the file is the module.
pub fn module_path(relative: &str) -> String {
    crate::lang::emit::dotted_module(relative, &[])
}

/// A `function_definition`: named by its declarator (`int *parse(char *)`
/// names `parse`) and with no body to walk — the declarations of a function
/// are local names of the language, not declarations of the chapter.
pub(crate) fn function_definition(
    node: &tree_sitter::Node,
    nesting: &Nesting,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) -> Option<Body> {
    let declarator = node.child_by_field_name("declarator")?;
    let name = declarator_name(&declarator, source)?;
    emit_named(node, "fn", &name, nesting.prefix, nesting.parent, out);
    None
}

/// A `struct`/`union`/`enum` specifier: a declaration only when it has a body.
/// The very same node is a type reference in `struct Store *store` and has no
/// body there, so emitting it unconditionally would invent a declaration for
/// every use of the type.
pub(crate) fn specifier(
    node: &tree_sitter::Node,
    nesting: &Nesting,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) -> Option<Body> {
    node.child_by_field_name("body")?;
    let index = crate::lang::emit::emit(node, "type", nesting.prefix, nesting.parent, source, out)?;
    let prefix = format!("{}{}.", nesting.prefix, out[index].name);
    Some(Body {
        index,
        prefix,
        container: "type",
    })
}

/// A `typedef`: the name is the declarator it introduces
/// (`typedef struct Store Store;` names `Store`), which no table can read.
pub(crate) fn type_definition(
    node: &tree_sitter::Node,
    nesting: &Nesting,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) -> Option<Body> {
    let declarator = node.child_by_field_name("declarator")?;
    let name = declarator_name(&declarator, source)?;
    emit_named(node, "type", &name, nesting.prefix, nesting.parent, out);
    None
}

/// A file-scope `declaration`: one symbol per declarator, `fn` for a prototype,
/// `const` when the declaration says `const`, the storage kind otherwise.
pub(crate) fn declaration(
    node: &tree_sitter::Node,
    nesting: &Nesting,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) -> Option<Body> {
    let constant = qualified_const(node, source);
    let mut cursor = node.walk();
    let children: Vec<tree_sitter::Node> = node.named_children(&mut cursor).collect();
    for child in children {
        if !declarator_kind(child.kind()) {
            continue;
        }
        let Some(name) = declarator_name(&child, source) else {
            continue;
        };
        let kind = if contains_kind(&child, "function_declarator") {
            "fn"
        } else if constant {
            "const"
        } else {
            "static"
        };
        emit_named(&child, kind, &name, nesting.prefix, nesting.parent, out);
    }
    None
}

/// Whether a declaration carries a `const` qualifier.
fn qualified_const(node: &tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "type_qualifier" && text(&child, source).as_deref() == Some("const") {
            return true;
        }
    }
    false
}

/// Whether a node kind is a declarator of a C declaration.
fn declarator_kind(kind: &str) -> bool {
    matches!(
        kind,
        "init_declarator"
            | "identifier"
            | "pointer_declarator"
            | "array_declarator"
            | "function_declarator"
            | "parenthesized_declarator"
    )
}

/// The relations of one C chapter, as the grammar sees them.
pub(crate) fn relations(root: &tree_sitter::Node, source: &[u8]) -> Vec<RawEdge> {
    let mut out = Vec::new();
    visit(root, source, &mut out);
    out
}

/// Walks the tree once and records every relation it recognises.
fn visit(node: &tree_sitter::Node, source: &[u8], out: &mut Vec<RawEdge>) {
    let byte = node.start_byte();
    match node.kind() {
        "preproc_include" => {
            if let Some(name) = include_name(node, source) {
                push_edges(out, byte, IMPORTS, vec![name]);
            }
        }
        "call_expression" => {
            if let Some(name) = node
                .child_by_field_name("function")
                .and_then(|function| called(&function, source))
            {
                out.push(RawEdge {
                    byte,
                    kind: CALLS,
                    name,
                });
            }
        }
        // A type reference in any position the grammar types as a type: a
        // parameter, a return, a cast, the tag of a `struct`.
        "type_identifier" => {
            if let Some(name) = text(node, source) {
                out.push(RawEdge {
                    byte,
                    kind: USES,
                    name,
                });
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    let children: Vec<tree_sitter::Node> = node.named_children(&mut cursor).collect();
    for child in children {
        visit(&child, source, out);
    }
}

/// The header an `#include` names, without its delimiters: `<stdio.h>` and
/// `"store.h"` name `stdio.h` and `store.h`.
fn include_name(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    let path = node.child_by_field_name("path")?;
    let raw = text(&path, source)?;
    let spelled = raw
        .trim_matches(|char| char == '<' || char == '>' || char == '"')
        .trim();
    let file = spelled.rsplit('/').next().unwrap_or(spelled);
    // The book stores identifiers, never a file name: `<sys/types.h>` names
    // `types` and `<vector>` names `vector` (see `relations::clean_name`).
    let name = file.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(file);
    (!name.is_empty()).then(|| name.to_string())
}

/// The name a call expression targets, when it has one.
fn called(function: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    match function.kind() {
        "identifier" => text(function, source),
        // `store->load()`: the field is the name of the call.
        "field_expression" => function
            .child_by_field_name("field")
            .and_then(|field| text(&field, source)),
        "qualified_identifier" | "template_function" => function
            .child_by_field_name("name")
            .and_then(|name| called(&name, source)),
        // `(*handler)()` and `(open_store)()`: the call is wrapped.
        "parenthesized_expression" | "pointer_expression" => function
            .child_by_field_name("argument")
            .or_else(|| function.named_child(0))
            .and_then(|inner| called(&inner, source)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::module_path;
    use crate::lang::registry::C;

    /// The module path is the prefix of every `qname`: the file is the module.
    #[test]
    fn the_module_path_of_a_c_file_is_the_path_that_names_it() {
        assert_eq!(module_path("src/store.c"), "src.store");
        assert_eq!(module_path("include/store.h"), "include.store");
        assert_eq!(module_path("main.c"), "main");
    }

    /// A `struct` specifier is a declaration with a body and a type reference
    /// without one: the same node kind must not declare the type twice.
    #[test]
    fn a_struct_specifier_declares_only_when_it_has_a_body() {
        let source =
            "struct Store { char *path; };\n\nint use(struct Store *store) { return 0; }\n";
        let parsed = C
            .parse("src/store.c", source.as_bytes(), 0)
            .expect("parses");
        let stores = parsed
            .symbols
            .iter()
            .filter(|symbol| symbol.name == "Store")
            .count();
        assert_eq!(stores, 1, "{:?}", parsed.symbols);
        assert!(parsed
            .edges
            .iter()
            .any(|edge| edge.kind == "uses" && edge.dst_name == "Store"));
    }
}
