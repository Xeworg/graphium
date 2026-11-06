//! The TypeScript chapter of the book (S3, L5).
//!
//! One module covers both grammars: `.ts` parses with the TypeScript grammar
//! and `.tsx` with the TSX one, and both write the same language
//! (`typescript`) to the table `file`. The chapter keeps the declarations of
//! the file — the functions, the classes with their constructors and methods,
//! the interfaces, the type aliases, the enums, the namespaces and the
//! module-level constants — with the nesting of the language in the `qname`
//! (`src.tools.ToolBox.renderWith`).
//!
//! The relations of this grammar are the four kinds of the schema, read
//! syntactically (D3):
//!
//! - `calls` — `openStore()`, `this.store.load()`, `new DiagramStore()`;
//! - `imports` — the name an `import`/`import … = require(…)`/`export … from`
//!   binds;
//! - `inherits` — the value of `extends` and every type of `implements`;
//! - `uses` — every type reference the grammar types as a type, generics
//!   included.
//!
//! JSX is markup: a JSX element name is not a call and an attribute is not a
//! relation.

use crate::lang::emit::{anchor_symbol, chapter_anchor_name, emit, last_segment, prefix_of, text};
use crate::lang::model::{content_hash, ExtractError, ParsedFile, ParsedSymbol};
use crate::lang::Grammar;
use crate::relations::{attach, push_edges, RawEdge, CALLS, IMPORTS, INHERITS, USES};

/// The TypeScript grammar of the book: `.ts` and `.tsx` in one module.
pub const GRAMMAR: Grammar = Grammar {
    lang: LANGUAGE,
    extensions: &["ts", "tsx"],
    parse,
};

/// The language of the chapter, as the table `file` spells it.
pub const LANGUAGE: &str = "typescript";

/// Parses one TypeScript (or TSX) file into the chapter of the book.
pub fn parse(path: &str, bytes: &[u8], mtime_ns: i64) -> Result<ParsedFile, ExtractError> {
    let language = if path.ends_with(".tsx") {
        tree_sitter_typescript::LANGUAGE_TSX
    } else {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT
    };
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter::Language::new(language))
        .map_err(|error| ExtractError::Grammar {
            language: LANGUAGE,
            detail: error.to_string(),
        })?;
    let tree = parser
        .parse(bytes, None)
        .ok_or(ExtractError::NoTree { language: LANGUAGE })?;

    let module = module_path(path);
    let prefix = prefix_of(&module, ".");
    let mut symbols = Vec::new();
    walk(&tree.root_node(), &prefix, None, bytes, &mut symbols);

    let raw = relations(&tree.root_node(), bytes);
    let anchor = anchor_symbol(&module, &chapter_anchor_name(path, &module));
    let edges = attach(raw, &mut symbols, anchor);

    Ok(ParsedFile {
        path: path.to_string(),
        lang: LANGUAGE,
        content_hash: content_hash(bytes),
        size: bytes.len() as i64,
        mtime_ns,
        symbols,
        edges,
    })
}

/// The module path of a TypeScript file: its directories and its name without
/// the extension, dotted, with `index` naming its directory (the idiom of the
/// language).
pub fn module_path(relative: &str) -> String {
    crate::lang::emit::dotted_module(relative, &["index"])
}

/// Walks the declarations of a node; a body is walked with the container as the
/// parent, so the containment tree of the chapter is the tree of the file.
fn walk(
    node: &tree_sitter::Node,
    prefix: &str,
    parent: Option<usize>,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            // `export`/`declare` wrap a declaration: the declaration is what
            // the book keeps, at the same nesting.
            "export_statement" | "ambient_declaration" => {
                walk(&child, prefix, parent, source, out);
            }
            // The body of a class, an interface or a namespace: the nesting is
            // the container that owns it.
            "class_body" | "interface_body" | "statement_block" => {
                walk(&child, prefix, parent, source, out);
            }
            "class_declaration" | "abstract_class_declaration" => {
                if let Some(index) = emit(&child, "class", prefix, parent, source, out) {
                    let own = format!("{prefix}{}.", out[index].name);
                    walk(&child, &own, Some(index), source, out);
                }
            }
            "interface_declaration" => {
                if let Some(index) = emit(&child, "trait", prefix, parent, source, out) {
                    let own = format!("{prefix}{}.", out[index].name);
                    walk(&child, &own, Some(index), source, out);
                }
            }
            // A namespace (`namespace Tools {}`) is the module of the language.
            "internal_module" => {
                if let Some(index) = emit(&child, "module", prefix, parent, source, out) {
                    let own = format!("{prefix}{}.", out[index].name);
                    walk(&child, &own, Some(index), source, out);
                }
            }
            "function_declaration" => {
                emit(&child, "fn", prefix, parent, source, out);
            }
            "method_definition" | "method_signature" | "abstract_method_signature" => {
                emit(&child, "method", prefix, parent, source, out);
            }
            "type_alias_declaration" | "enum_declaration" => {
                emit(&child, "type", prefix, parent, source, out);
            }
            // A module-level `const`/`let`/`var` is a declaration of the file;
            // the arrow function it holds is part of its span, so the calls of
            // the closure hang from it.
            "lexical_declaration" | "variable_declaration" => {
                let mut declarators = child.walk();
                for declarator in child.named_children(&mut declarators) {
                    if declarator.kind() == "variable_declarator" {
                        emit(&declarator, "const", prefix, parent, source, out);
                    }
                }
            }
            _ => {}
        }
    }
}

/// The relations of one TypeScript chapter, as the grammar sees them.
fn relations(root: &tree_sitter::Node, source: &[u8]) -> Vec<RawEdge> {
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
        // `new DiagramStore()`: a call to the constructor by the name written.
        "new_expression" => {
            if let Some(name) = node
                .child_by_field_name("constructor")
                .and_then(|constructor| property(&constructor, source))
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
        "extends_clause" => {
            let target = node
                .named_child(0)
                .and_then(|value| property(&value, source));
            if let Some(name) = target {
                out.push(RawEdge {
                    byte,
                    kind: INHERITS,
                    name,
                });
            }
        }
        "implements_clause" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
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
        // parameter, a return type, a field, a generic argument.
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
    for child in node.named_children(&mut cursor) {
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

/// The last name of a value that can be spelled as a property path.
fn property(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "identifier" | "property_identifier" => text(node, source),
        "member_expression" => node
            .child_by_field_name("property")
            .and_then(|property| text(&property, source)),
        "nested_type_identifier" | "scoped_type_identifier" => node
            .child_by_field_name("name")
            .and_then(|name| text(&name, source))
            .or_else(|| text(node, source).map(|whole| last_segment(&whole))),
        _ => None,
    }
}

/// The name of a type node: `Loader` → `Loader`, `Box<ToolBox>` → `Box`.
fn type_name(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "type_identifier" => text(node, source),
        "generic_type" => node
            .named_child(0)
            .and_then(|base| type_name(&base, source)),
        "nested_type_identifier" | "scoped_type_identifier" => node
            .child_by_field_name("name")
            .and_then(|name| text(&name, source))
            .or_else(|| text(node, source).map(|whole| last_segment(&whole))),
        _ => None,
    }
}

/// The names an `import` statement binds.
fn import_names(node: &tree_sitter::Node, source: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "import_clause" => import_clause(&child, source, &mut names),
            // `import legacy = require("./legacy")`: no field, the identifier
            // is the first named child.
            "import_require_clause" => {
                if let Some(name) = child.named_child(0).and_then(|name| text(&name, source)) {
                    names.push(name);
                }
            }
            _ => {}
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
    use super::{module_path, parse};

    /// The module path is the prefix of every `qname`: the path of the file
    /// without the extension, and `index` naming its directory.
    #[test]
    fn the_module_path_of_a_typescript_file_is_the_path_that_names_it() {
        assert_eq!(module_path("src/tools.ts"), "src.tools");
        assert_eq!(module_path("src/view.tsx"), "src.view");
        assert_eq!(module_path("src/store/index.ts"), "src.store");
        assert_eq!(module_path("index.ts"), "");
    }

    /// Tree-sitter is error tolerant: a half-saved `.tsx` still yields the
    /// declarations the grammar recovers.
    #[test]
    fn a_typescript_file_with_a_syntax_error_still_yields_what_it_recovers() {
        let broken = "export function one() {}\nexport class Two {\n";
        let parsed = parse("src/broken.tsx", broken.as_bytes(), 0).expect("parses");
        assert!(
            parsed.symbols.iter().any(|symbol| symbol.name == "one"),
            "{:?}",
            parsed.symbols
        );
    }
}
