//! The C++ chapter of the book (T3 Batch 1; S2, S4).
//!
//! The chapter keeps the declarations of a translation unit — the classes, the
//! structs, the unions, the enums, the type aliases, the functions at namespace
//! scope and the methods and declarations of a class (a `fn` declared in a
//! class body is a `method`), plus the file-scope variables (a `const` when the
//! declaration says so, the storage otherwise) — with the path of the file as
//! the prefix of every `qname` (`src.store.DiagramStore.load`). A namespace is
//! part of the naming of the file, not a symbol of the book: the path already
//! names it (the file `store.cpp` is `src.store`), so a `namespace store { … }`
//! is transparent and does not nest the `qname` twice.
//!
//! The relations of this grammar are the four kinds of the schema, read
//! syntactically (D3):
//!
//! - `calls` — `open_store()`, `store.load()`, `Store::open()`;
//! - `imports` — the header an `#include` names (`<vector>` → `vector`);
//! - `inherits` — every base of a `class`/`struct` (`: public Base`);
//! - `uses` — every name the grammar types as a type: a parameter, a return,
//!   a cast, a template argument.
//!
//! A function body is not walked: the declarations of a function are local
//! names, and a nested class inside a function body is not a declaration of
//! the unit.
//!
//! What is C++'s alone is here; the declarations the book keeps are the tables
//! of [`crate::lang::registry::CPP`] and the pipeline is [`crate::lang::engine`]
//! (S3, T2). The hooks are the ones the tables cannot express: a
//! `function_definition` and a `declaration` are named by their declarator
//! (never by a `name` field) and are a `method` or a `fn` by the body that owns
//! them, and a `class`/`struct`/`union`/`enum` specifier is a declaration only
//! when it has a body.

use crate::lang::emit::{contains_kind, declarator_name, emit, emit_named, text};
use crate::lang::engine::{Body, Nesting};
use crate::lang::model::ParsedSymbol;
use crate::relations::{push_edges, RawEdge, CALLS, IMPORTS, INHERITS, USES};

/// The language of the chapter, as the table `file` spells it.
pub const LANGUAGE: &str = "cpp";

/// The tree-sitter grammar of C++: one grammar answers every C++ path. The
/// header extensions are the C++ ones on purpose (`.hpp`, `.hh`, `.hxx`,
/// `.h++`): `.h` belongs to C, so which grammar answers a path never depends on
/// the features a build enabled.
pub fn language(_path: &str) -> tree_sitter::Language {
    tree_sitter::Language::new(tree_sitter_cpp::LANGUAGE)
}

/// The module path of a C++ file: its directories and its name without the
/// extension, dotted. C++ has no module system before C++20, so the file is the
/// module and a `namespace` is the naming of the source, not of the book.
pub fn module_path(relative: &str) -> String {
    crate::lang::emit::dotted_module(relative, &[])
}

/// A `function_definition`: named by its declarator (`int *parse(char *)`
/// names `parse`, `int Store::load()` names `load`), a `method` when the class
/// body owns it and a `fn` at namespace scope.
pub(crate) fn function_definition(
    node: &tree_sitter::Node,
    nesting: &Nesting,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) -> Option<Body> {
    let declarator = node.child_by_field_name("declarator")?;
    let name = declarator_name(&declarator, source)?;
    let kind = if in_container(nesting) {
        "method"
    } else {
        "fn"
    };
    emit_named(node, kind, &name, nesting.prefix, nesting.parent, out);
    None
}

/// A `field_declaration`: a data field of a class is not a declaration of the
/// book, but a method prototype the grammar files under the same kind
/// (`int load(const char *input);`) is a `method`, and it is only visible
/// through the `function_declarator` it wraps.
pub(crate) fn field_declaration(
    node: &tree_sitter::Node,
    nesting: &Nesting,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) -> Option<Body> {
    let declarator = node.child_by_field_name("declarator")?;
    if !contains_kind(&declarator, "function_declarator") {
        return None;
    }
    let name = declarator_name(&declarator, source)?;
    emit_named(
        &declarator,
        "method",
        &name,
        nesting.prefix,
        nesting.parent,
        out,
    );
    None
}

/// A `class`/`struct`/`union`/`enum` specifier: a declaration only when it has
/// a body. The same node is a type reference in a base clause or in a
/// parameter and has no body there.
pub(crate) fn specifier(
    node: &tree_sitter::Node,
    nesting: &Nesting,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) -> Option<Body> {
    node.child_by_field_name("body")?;
    let kind = if node.kind() == "class_specifier" {
        "class"
    } else {
        "type"
    };
    let index = emit(node, kind, nesting.prefix, nesting.parent, source, out)?;
    let prefix = format!("{}{}.", nesting.prefix, out[index].name);
    Some(Body {
        index,
        prefix,
        container: kind,
    })
}

/// A `declaration`: inside a class body its function declarators are method
/// declarations and its fields are not declarations of the book; at namespace
/// scope it is one symbol per declarator — `fn` for a prototype, `const` when
/// the declaration says so, the storage otherwise.
pub(crate) fn declaration(
    node: &tree_sitter::Node,
    nesting: &Nesting,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) -> Option<Body> {
    let constant = qualified_const(node, source);
    let inside = in_container(nesting);
    let mut cursor = node.walk();
    let children: Vec<tree_sitter::Node> = node.named_children(&mut cursor).collect();
    for child in children {
        if !declarator_kind(child.kind()) {
            continue;
        }
        let Some(name) = declarator_name(&child, source) else {
            continue;
        };
        let function = contains_kind(&child, "function_declarator");
        let kind = match (inside, function, constant) {
            (true, true, _) => "method",
            (true, false, _) => continue,
            (false, true, _) => "fn",
            (false, false, true) => "const",
            (false, false, false) => "static",
        };
        emit_named(&child, kind, &name, nesting.prefix, nesting.parent, out);
    }
    None
}

/// Whether the declaration is inside a class/struct body of the book.
fn in_container(nesting: &Nesting) -> bool {
    matches!(nesting.container, Some("class") | Some("type"))
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

/// Whether a node kind is a declarator of a C++ declaration.
fn declarator_kind(kind: &str) -> bool {
    matches!(
        kind,
        "init_declarator"
            | "identifier"
            | "field_identifier"
            | "pointer_declarator"
            | "reference_declarator"
            | "array_declarator"
            | "function_declarator"
            | "parenthesized_declarator"
            | "qualified_identifier"
    )
}

/// The relations of one C++ chapter, as the grammar sees them.
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
        // `class DiagramStore : public Base, private Holder { … }`: every base
        // is an inheritance, whichever access it names.
        "base_class_clause" => {
            let mut cursor = node.walk();
            let children: Vec<tree_sitter::Node> = node.named_children(&mut cursor).collect();
            for child in children {
                if let Some(name) = type_name(&child, source) {
                    out.push(RawEdge {
                        byte,
                        kind: INHERITS,
                        name,
                    });
                }
            }
        }
        // A type reference in any position the grammar types as a type: a
        // parameter, a return, a cast, a template argument, a `new`.
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

/// The header an `#include` names, without its delimiters: `<vector>` and
/// `"store.h"` name `vector` and `store.h`.
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
        // `store->load()` and `store.load()`: the field is the name.
        "field_expression" => function
            .child_by_field_name("field")
            .and_then(|field| text(&field, source)),
        // `Store::open()` and `make<int>()`.
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

/// The name a type node spells: `Base` → `Base`, `std::vector<int>` →
/// `vector`, `Holder<T>` → `Holder`.
fn type_name(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "type_identifier" => text(node, source),
        "qualified_identifier" | "template_type" => node
            .child_by_field_name("name")
            .and_then(|name| type_name(&name, source)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::module_path;
    use crate::lang::registry::CPP;

    /// The module path is the prefix of every `qname`: the file is the module.
    #[test]
    fn the_module_path_of_a_cpp_file_is_the_path_that_names_it() {
        assert_eq!(module_path("src/store.cpp"), "src.store");
        assert_eq!(module_path("include/store.hpp"), "include.store");
        assert_eq!(module_path("main.cc"), "main");
    }

    /// A method of a class body is a `method` and its free neighbour is a `fn`;
    /// an out-of-class definition is named by its declarator.
    #[test]
    fn a_class_body_says_which_declarations_are_methods() {
        let source = r#"
namespace store {
class DiagramStore {
public:
    int load(const char *input);
    int render() { return open_store(); }
};
int DiagramStore::close() { return 0; }
int open_store() { return 0; }
}
"#;
        let parsed = CPP
            .parse("src/store.cpp", source.as_bytes(), 0)
            .expect("parses");
        let names: Vec<(&str, &str)> = parsed
            .symbols
            .iter()
            .map(|symbol| (symbol.kind, symbol.name.as_str()))
            .collect();
        for expected in [
            ("class", "DiagramStore"),
            ("method", "load"),
            ("method", "render"),
            ("fn", "close"),
            ("fn", "open_store"),
        ] {
            assert!(names.contains(&expected), "missing {expected:?}: {names:?}");
        }
    }
}
