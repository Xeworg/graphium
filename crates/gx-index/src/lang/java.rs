//! The Java chapter of the book (S3, L5).
//!
//! The chapter keeps the types of a file — the classes (a record is a class),
//! the interfaces, the enums and the annotation types — with their constructors
//! and their methods as children, and the nesting of the language in the
//! `qname`. The prefix is the `package` the file declares, which is what names
//! it in the language: `com.graphium.store.DiagramStore.renderWith`.
//!
//! The relations of this grammar are the four kinds of the schema, read
//! syntactically (D3):
//!
//! - `calls` — `openStore()`, `store.load()`, `new DiagramStore()`;
//! - `imports` — the last segment of the imported path
//!   (`java.util.List` → `List`, `import static …` included);
//! - `inherits` — the superclass and every implemented interface;
//! - `uses` — every type the grammar types as a type, generics included.
//!
//! Two overloads of one method are two declarations of the same name: the
//! chapter keeps both and the book numbers the colliding `qname`.

use crate::lang::emit::{
    anchor_symbol, chapter_anchor_name, dotted_directories, emit, last_segment, prefix_of, text,
};
use crate::lang::model::{content_hash, ExtractError, ParsedFile, ParsedSymbol};
use crate::lang::Grammar;
use crate::relations::{attach, push_edges, RawEdge, CALLS, IMPORTS, INHERITS, USES};

/// The Java grammar of the book.
pub const GRAMMAR: Grammar = Grammar {
    lang: LANGUAGE,
    extensions: &["java"],
    parse,
};

/// The language of the chapter, as the table `file` spells it.
pub const LANGUAGE: &str = "java";

/// Parses one Java file into the chapter of the book.
pub fn parse(path: &str, bytes: &[u8], mtime_ns: i64) -> Result<ParsedFile, ExtractError> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter::Language::new(tree_sitter_java::LANGUAGE))
        .map_err(|error| ExtractError::Grammar {
            language: LANGUAGE,
            detail: error.to_string(),
        })?;
    let tree = parser
        .parse(bytes, None)
        .ok_or(ExtractError::NoTree { language: LANGUAGE })?;

    let module = module_path(&tree.root_node(), path, bytes);
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

/// The module path of a Java file: the `package` it declares, or its directory
/// when the file declares none.
pub fn module_path(root: &tree_sitter::Node, path: &str, source: &[u8]) -> String {
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if child.kind() != "package_declaration" {
            continue;
        }
        if let Some(name) = child
            .named_child(0)
            .and_then(|name| text(&name, source))
            .filter(|name| !name.is_empty())
        {
            return name;
        }
    }
    dotted_directories(path)
}

/// Walks the types of a file; the body of a type is walked with the type as the
/// parent, so a nested class or a method is a child of its container.
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
            "class_declaration" | "record_declaration" => {
                if let Some(index) = emit(&child, "class", prefix, parent, source, out) {
                    let own = format!("{prefix}{}.", out[index].name);
                    walk(&child, &own, Some(index), source, out);
                }
            }
            "interface_declaration" | "annotation_type_declaration" => {
                if let Some(index) = emit(&child, "trait", prefix, parent, source, out) {
                    let own = format!("{prefix}{}.", out[index].name);
                    walk(&child, &own, Some(index), source, out);
                }
            }
            "enum_declaration" => {
                if let Some(index) = emit(&child, "type", prefix, parent, source, out) {
                    let own = format!("{prefix}{}.", out[index].name);
                    walk(&child, &own, Some(index), source, out);
                }
            }
            // The body of a type: a method, a constructor, a nested type.
            "class_body"
            | "interface_body"
            | "enum_body"
            | "enum_body_declarations"
            | "annotation_type_body" => walk(&child, prefix, parent, source, out),
            "method_declaration" | "constructor_declaration" => {
                emit(&child, "method", prefix, parent, source, out);
            }
            _ => {}
        }
    }
}

/// The relations of one Java chapter, as the grammar sees them.
fn relations(root: &tree_sitter::Node, source: &[u8]) -> Vec<RawEdge> {
    let mut out = Vec::new();
    visit(root, source, &mut out);
    out
}

/// Walks the tree once and records every relation it recognises.
fn visit(node: &tree_sitter::Node, source: &[u8], out: &mut Vec<RawEdge>) {
    let byte = node.start_byte();
    match node.kind() {
        "method_invocation" => {
            if let Some(name) = node
                .child_by_field_name("name")
                .and_then(|name| text(&name, source))
            {
                out.push(RawEdge {
                    byte,
                    kind: CALLS,
                    name,
                });
            }
        }
        // `new DiagramStore()`: a call to the constructor by the name written.
        "object_creation_expression" => {
            if let Some(name) = node
                .child_by_field_name("type")
                .and_then(|declared| type_name(&declared, source))
            {
                out.push(RawEdge {
                    byte,
                    kind: CALLS,
                    name,
                });
            }
        }
        "import_declaration" => {
            let mut cursor = node.walk();
            let imported = node
                .named_children(&mut cursor)
                .find(|child| matches!(child.kind(), "scoped_identifier" | "identifier"))
                .and_then(|child| text(&child, source))
                .map(|whole| last_segment(&whole));
            push_edges(out, byte, IMPORTS, imported.into_iter().collect());
        }
        "superclass" => {
            if let Some(name) = node
                .named_child(0)
                .and_then(|extended| type_name(&extended, source))
            {
                out.push(RawEdge {
                    byte,
                    kind: INHERITS,
                    name,
                });
            }
        }
        "super_interfaces" => {
            let mut cursor = node.walk();
            for interfaces in node.named_children(&mut cursor) {
                let mut inner = interfaces.walk();
                for implemented in interfaces.named_children(&mut inner) {
                    if let Some(name) = type_name(&implemented, source) {
                        out.push(RawEdge {
                            byte,
                            kind: INHERITS,
                            name,
                        });
                    }
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

/// The name of a type node: `List<Loader>` → `List`, `a.b.C` → `C`.
fn type_name(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "type_identifier" | "identifier" => text(node, source),
        "generic_type" => node
            .named_child(0)
            .and_then(|base| type_name(&base, source)),
        "scoped_type_identifier" => node
            .child_by_field_name("name")
            .and_then(|name| text(&name, source))
            .or_else(|| text(node, source).map(|whole| last_segment(&whole))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::parse;

    /// The prefix of every `qname` is the `package` of the file, and a file
    /// without one keeps its directory.
    #[test]
    fn the_package_of_a_java_file_names_its_chapter() {
        let packed = "package com.graphium.store;\n\nclass One {}\n";
        let parsed = parse("src/One.java", packed.as_bytes(), 0).expect("parses");
        assert_eq!(parsed.lang, "java");
        assert!(
            parsed
                .symbols
                .iter()
                .any(|symbol| symbol.qname == "com.graphium.store.One"),
            "{:?}",
            parsed.symbols
        );

        let loose = "class Two {}\n";
        let parsed = parse("src/loose/Two.java", loose.as_bytes(), 0).expect("parses");
        assert_eq!(parsed.symbols[0].qname, "src.loose.Two");
    }

    /// Tree-sitter is error tolerant: a half-saved file still yields the
    /// declarations the grammar recovers.
    #[test]
    fn a_java_file_with_a_syntax_error_still_yields_what_it_recovers() {
        let broken = "class One { int value() { return 1; } }\nclass Two {\n";
        let parsed = parse("src/Broken.java", broken.as_bytes(), 0).expect("parses");
        assert!(
            parsed.symbols.iter().any(|symbol| symbol.name == "One"),
            "{:?}",
            parsed.symbols
        );
    }
}
