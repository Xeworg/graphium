//! The JavaScript chapter of the book (T3 Batch 1; S2, S4).
//!
//! One grammar answers `.js`, `.jsx`, `.mjs` and `.cjs`: the grammar of
//! JavaScript is also the grammar of JSX, so there is no second module and no
//! second language for the markup half of the language. TypeScript keeps its
//! own module (`typescript.rs`) because its grammar is a different crate and its
//! declarations are not these ones; nothing of it is duplicated here.
//!
//! The chapter keeps the declarations of the module — the functions, the
//! classes with their methods, and the module-level `const`/`let`/`var` by
//! each of its declarators — with the nesting of the language in the `qname`
//! (`src.tools.ToolBox.renderWith`).
//!
//! The relations of this grammar are three of the four kinds of the schema,
//! read syntactically (D3):
//!
//! - `calls` — `openStore()`, `this.store.load()`, `new ToolBox()`;
//! - `imports` — the name an `import`/`export … from` binds;
//! - `inherits` — the value of `extends`.
//!
//! There is no `uses`: JavaScript has no type annotations, so a type
//! reference does not exist in its tree and the book does not invent one. JSX
//! is markup: a JSX element name is not a call and an attribute is not a
//! relation.
//!
//! What is JavaScript's alone is here; the declarations the book keeps are the
//! tables of [`crate::lang::registry::JAVASCRIPT`] and the pipeline is
//! [`crate::lang::engine`] (S3, T2). The one hook the tables cannot cover is
//! the declaration group: a module-level `const`/`let`/`var` is the `const` of
//! the language by each of its declarators. The import and call helpers repeat
//! the ones of `typescript.rs` on purpose: the two grammars share the
//! ECMAScript node kinds, but the two cargo features are independent and a
//! build with only one of them must not compile the other.

use crate::lang::emit::{dotted_module, emit, text};
use crate::lang::engine::{Body, Nesting};
use crate::lang::model::ParsedSymbol;
use crate::relations::{push_edges, RawEdge, CALLS, IMPORTS, INHERITS};

/// The language of the chapter, as the table `file` spells it.
pub const LANGUAGE: &str = "javascript";

/// The tree-sitter grammar of JavaScript: one grammar answers `.js`, `.jsx`,
/// `.mjs` and `.cjs`, so JSX needs no second language of the book.
pub fn language(_path: &str) -> tree_sitter::Language {
    tree_sitter::Language::new(tree_sitter_javascript::LANGUAGE)
}

/// The module path of a JavaScript file: its directories and its name without
/// the extension, dotted, with `index` naming its directory (the idiom of the
/// language).
pub fn module_path(relative: &str) -> String {
    dotted_module(relative, &["index"])
}

/// A module-level `const`/`let`/`var`: the declaration of the book is each
/// `variable_declarator` of the group, and the arrow function it holds is part
/// of its span, so the calls of the closure hang from it.
pub(crate) fn declarators(
    node: &tree_sitter::Node,
    nesting: &Nesting,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) -> Option<Body> {
    let mut cursor = node.walk();
    for declarator in node.named_children(&mut cursor) {
        if declarator.kind() != "variable_declarator" {
            continue;
        }
        // `const { a, b } = source` declares no single name: the pattern is
        // not a declaration of the book (a symbol called `{ a, b }` is not a
        // symbol).
        let Some(name) = declarator.child_by_field_name("name") else {
            continue;
        };
        if name.kind() != "identifier" {
            continue;
        }
        emit(
            &declarator,
            "const",
            nesting.prefix,
            nesting.parent,
            source,
            out,
        );
    }
    None
}

/// The relations of one JavaScript chapter, as the grammar sees them.
pub(crate) fn relations(root: &tree_sitter::Node, source: &[u8]) -> Vec<RawEdge> {
    let mut out = Vec::new();
    visit(root, source, &mut out);
    out
}

/// Walks the tree once and records every relation it recognises.
fn visit(node: &tree_sitter::Node, source: &[u8], out: &mut Vec<RawEdge>) {
    let byte = node.start_byte();
    match node.kind() {
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
        // `new ToolBox()`: a call to the constructor by the name written.
        "new_expression" => {
            if let Some(name) = node
                .child_by_field_name("constructor")
                .and_then(|constructor| named(&constructor, source))
            {
                out.push(RawEdge {
                    byte,
                    kind: CALLS,
                    name,
                });
            }
        }
        "import_statement" => {
            push_edges(out, byte, IMPORTS, import_names(node, source));
        }
        // Only `export … from "./x"` binds a name here: an `export` of a
        // declaration is that declaration, not an import.
        "export_statement" if node.child_by_field_name("source").is_some() => {
            push_edges(out, byte, IMPORTS, export_names(node, source));
        }
        "class_heritage" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if let Some(name) = named(&child, source) {
                    out.push(RawEdge {
                        byte,
                        kind: INHERITS,
                        name,
                    });
                }
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

/// The name a call expression targets, when it has one.
fn called(function: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    match function.kind() {
        "identifier" | "property_identifier" | "private_property_identifier" => {
            text(function, source)
        }
        // `this.store.load()`: the method name.
        "member_expression" => function
            .child_by_field_name("property")
            .and_then(|property| text(&property, source)),
        // `openStore!()`, `(openStore)()`: the call is wrapped.
        "non_null_expression" | "parenthesized_expression" => function
            .named_child(0)
            .and_then(|inner| called(&inner, source)),
        _ => None,
    }
}

/// The last name of a value that can be spelled as a property path: the parent
/// of an `extends` clause and the constructor of a `new`.
fn named(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "identifier" | "property_identifier" => text(node, source),
        "member_expression" => node
            .child_by_field_name("property")
            .and_then(|property| text(&property, source)),
        "parenthesized_expression" | "non_null_expression" => {
            node.named_child(0).and_then(|inner| named(&inner, source))
        }
        _ => None,
    }
}

/// The names an `import` statement binds.
fn import_names(node: &tree_sitter::Node, source: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "import_clause" {
            import_clause(&child, source, &mut names);
        }
    }
    names
}

/// The names an `import_clause` binds: the default import, the namespace and
/// every named specifier (by its name, never by its alias).
fn import_clause(node: &tree_sitter::Node, source: &[u8], out: &mut Vec<String>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "identifier" => {
                if let Some(name) = text(&child, source) {
                    out.push(name);
                }
            }
            "namespace_import" => {
                if let Some(name) = child.named_child(0).and_then(|name| text(&name, source)) {
                    out.push(name);
                }
            }
            "named_imports" => specifiers(&child, "import_specifier", source, out),
            _ => {}
        }
    }
}

/// The names an `export … from` binds.
fn export_names(node: &tree_sitter::Node, source: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "export_clause" => specifiers(&child, "export_specifier", source, &mut names),
            "namespace_export" => {
                if let Some(name) = child.named_child(0).and_then(|name| text(&name, source)) {
                    names.push(name);
                }
            }
            _ => {}
        }
    }
    names
}

/// The `name` of every specifier of a clause (a list of `{ a, b as c }`).
fn specifiers(node: &tree_sitter::Node, kind: &str, source: &[u8], out: &mut Vec<String>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() != kind {
            continue;
        }
        if let Some(name) = child
            .child_by_field_name("name")
            .and_then(|name| text(&name, source))
        {
            out.push(name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::module_path;
    use crate::lang::registry::JAVASCRIPT;

    /// The module path is the prefix of every `qname`: the path of the file
    /// without the extension, and `index` naming its directory.
    #[test]
    fn the_module_path_of_a_javascript_file_is_the_path_that_names_it() {
        assert_eq!(module_path("src/tools.js"), "src.tools");
        assert_eq!(module_path("src/view.jsx"), "src.view");
        assert_eq!(module_path("src/store/index.js"), "src.store");
        assert_eq!(module_path("index.mjs"), "");
    }

    /// JSX is parsed by the same grammar and the same chapter: a `.jsx` file
    /// yields the declarations of the file and never a relation out of markup.
    #[test]
    fn a_jsx_element_is_markup_and_not_a_relation() {
        let source = r#"
export function View(props) {
  return <section className="box"><Panel store={openStore()} /></section>;
}
"#;
        let parsed = JAVASCRIPT
            .parse("src/view.jsx", source.as_bytes(), 0)
            .expect("parses");
        assert!(
            parsed.symbols.iter().any(|symbol| symbol.name == "View"),
            "{:?}",
            parsed.symbols
        );
        // `<Panel …/>` is not a call; `openStore()` is.
        assert!(
            parsed.edges.iter().any(|edge| edge.dst_name == "openStore"),
            "{:?}",
            parsed.edges
        );
        assert!(
            !parsed.edges.iter().any(|edge| edge.dst_name == "Panel"),
            "JSX markup is not a relation: {:?}",
            parsed.edges
        );
    }
}
