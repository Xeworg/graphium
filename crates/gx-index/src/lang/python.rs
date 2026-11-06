//! The Python chapter of the book (S3, L5).
//!
//! The chapter keeps the declarations of the module — the functions, the
//! classes with their methods (decorated ones included), the nested classes and
//! the module-level constants — with the nesting of the language in the `qname`
//! (`src.store.DiagramStore.load`). A function body is not a container of the
//! book: a closure is not a declaration of the file, exactly as a Rust closure
//! is not.
//!
//! The relations of this grammar are the four kinds of the schema, read
//! syntactically (D3):
//!
//! - `calls` — `openStore()`, `store.load()`, `str(path)`;
//! - `imports` — the segment an `import`/`from … import` binds (`from .wild
//!   import *` binds the module);
//! - `inherits` — every base class of a `class Store(Base):`;
//! - `uses` — every name a type annotation mentions, generics included.
//!
//! A string is never a type: a forward reference in quotes adds no relation.

use crate::lang::emit::{
    anchor_symbol, chapter_anchor_name, emit, emit_named, last_segment, prefix_of, text,
};
use crate::lang::model::{content_hash, ExtractError, ParsedFile, ParsedSymbol};
use crate::lang::Grammar;
use crate::relations::{attach, push_edges, RawEdge, CALLS, IMPORTS, INHERITS, USES};

/// The Python grammar of the book.
pub const GRAMMAR: Grammar = Grammar {
    lang: LANGUAGE,
    extensions: &["py"],
    parse,
};

/// The language of the chapter, as the table `file` spells it.
pub const LANGUAGE: &str = "python";

/// Parses one Python file into the chapter of the book.
pub fn parse(path: &str, bytes: &[u8], mtime_ns: i64) -> Result<ParsedFile, ExtractError> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter::Language::new(tree_sitter_python::LANGUAGE))
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
    walk(&tree.root_node(), &prefix, None, None, bytes, &mut symbols);

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

/// The module path of a Python file: its directories and its name without the
/// extension, dotted, with `__init__` naming its directory (the idiom of the
/// language).
pub fn module_path(relative: &str) -> String {
    crate::lang::emit::dotted_module(relative, &["__init__"])
}

/// Walks the declarations of a node; `container` says whether the node is the
/// body of a class, which is what makes a `def` a method.
fn walk(
    node: &tree_sitter::Node,
    prefix: &str,
    parent: Option<usize>,
    container: Option<&'static str>,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            // A decorator wraps a `def` or a `class`: the declaration is the
            // one the book keeps.
            "decorated_definition" => walk(&child, prefix, parent, container, source, out),
            "block" => walk(&child, prefix, parent, container, source, out),
            "class_definition" => {
                if let Some(index) = emit(&child, "class", prefix, parent, source, out) {
                    let own = format!("{prefix}{}.", out[index].name);
                    walk(&child, &own, Some(index), Some("class"), source, out);
                }
            }
            "function_definition" => {
                let kind = match container {
                    Some("class") => "method",
                    _ => "fn",
                };
                emit(&child, kind, prefix, parent, source, out);
            }
            // A module-level assignment is the constant of the language; the
            // attribute of a method is not a declaration.
            "expression_statement" if container.is_none() => {
                constant(&child, prefix, parent, source, out)
            }
            _ => {}
        }
    }
}

/// Emits a module-level assignment as the constant it is.
fn constant(
    node: &tree_sitter::Node,
    prefix: &str,
    parent: Option<usize>,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() != "assignment" {
            continue;
        }
        if let Some(name) = child
            .child_by_field_name("left")
            .and_then(|left| text(&left, source))
        {
            emit_named(&child, "const", &name, prefix, parent, out);
        }
    }
}

/// The relations of one Python chapter, as the grammar sees them.
fn relations(root: &tree_sitter::Node, source: &[u8]) -> Vec<RawEdge> {
    let mut out = Vec::new();
    visit(root, source, &mut out);
    out
}

/// Walks the tree once and records every relation it recognises.
fn visit(node: &tree_sitter::Node, source: &[u8], out: &mut Vec<RawEdge>) {
    let byte = node.start_byte();
    match node.kind() {
        "call" => {
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
        "import_statement" => {
            push_edges(out, byte, IMPORTS, import_names(node, source));
        }
        "import_from_statement" => {
            push_edges(out, byte, IMPORTS, from_names(node, source));
        }
        // Every base class of `class Store(Base):`.
        "class_definition" => {
            if let Some(superclasses) = node.child_by_field_name("superclasses") {
                let mut cursor = superclasses.walk();
                for child in superclasses.named_children(&mut cursor) {
                    // `metaclass=…` is a keyword of the declaration, not a base.
                    if child.kind() == "keyword_argument" {
                        continue;
                    }
                    if let Some(name) = text(&child, source).map(|whole| last_segment(&whole)) {
                        out.push(RawEdge {
                            byte: child.start_byte(),
                            kind: INHERITS,
                            name,
                        });
                    }
                }
            }
        }
        // A type annotation: a parameter, a return type, a field, a generic.
        "type" => {
            let mut names = Vec::new();
            type_names(node, source, &mut names);
            push_edges(out, byte, USES, names);
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        visit(&child, source, out);
    }
}

/// The name a call targets, when it has one.
fn called(function: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    match function.kind() {
        "identifier" => text(function, source),
        // `store.load()`: the method name.
        "attribute" => function
            .child_by_field_name("attribute")
            .and_then(|attribute| text(&attribute, source)),
        _ => None,
    }
}

/// The names an `import` statement binds: the last segment of each module.
fn import_names(node: &tree_sitter::Node, source: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "dotted_name" => {
                if let Some(name) = text(&child, source).map(|whole| last_segment(&whole)) {
                    names.push(name);
                }
            }
            // `import os as operating_system`: the module, not the alias.
            "aliased_import" => {
                if let Some(name) = child
                    .child_by_field_name("name")
                    .and_then(|name| text(&name, source))
                    .map(|whole| last_segment(&whole))
                {
                    names.push(name);
                }
            }
            _ => {}
        }
    }
    names
}

/// The names a `from … import` statement binds: the imported items, or the
/// module itself when the statement imports everything.
fn from_names(node: &tree_sitter::Node, source: &[u8]) -> Vec<String> {
    let module = node.child_by_field_name("module_name");
    let module_last = module
        .as_ref()
        .and_then(|module| module_name(module, source));
    let mut names = Vec::new();
    let mut wildcard = false;
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if module
            .as_ref()
            .is_some_and(|module| module.id() == child.id())
        {
            continue;
        }
        match child.kind() {
            "dotted_name" => {
                if let Some(name) = text(&child, source).map(|whole| last_segment(&whole)) {
                    names.push(name);
                }
            }
            // `from y import a as b`: the imported name, not the alias.
            "aliased_import" => {
                if let Some(name) = child
                    .child_by_field_name("name")
                    .and_then(|name| text(&name, source))
                    .map(|whole| last_segment(&whole))
                {
                    names.push(name);
                }
            }
            "wildcard_import" => wildcard = true,
            _ => {}
        }
    }
    if wildcard {
        return module_last.into_iter().collect();
    }
    names
}

/// The last segment of the module of a `from` statement. A relative import
/// spells it after its dots (`from .wild import *` → `wild`).
fn module_name(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    if node.kind() == "relative_import" {
        let mut cursor = node.walk();
        return node
            .named_children(&mut cursor)
            .find(|child| child.kind() == "dotted_name")
            .and_then(|child| text(&child, source))
            .map(|whole| last_segment(&whole));
    }
    text(node, source).map(|whole| last_segment(&whole))
}

/// Every name a type node mentions, descending the generics and keeping only
/// the name of a dotted reference (never the module that qualifies it).
fn type_names(node: &tree_sitter::Node, source: &[u8], out: &mut Vec<String>) {
    match node.kind() {
        "identifier" => {
            if let Some(name) = text(node, source) {
                out.push(name);
            }
        }
        "attribute" => {
            if let Some(name) = node
                .child_by_field_name("attribute")
                .and_then(|name| text(&name, source))
            {
                out.push(name);
            }
        }
        // A forward reference is prose until the name it quotes is a name the
        // book can check.
        "string" | "concatenated_string" => {}
        _ => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                type_names(&child, source, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{module_path, parse};

    /// The module path is the import path of the file: its directories and its
    /// name, with `__init__` naming its package.
    #[test]
    fn the_module_path_of_a_python_file_is_its_import_path() {
        assert_eq!(module_path("src/store.py"), "src.store");
        assert_eq!(module_path("src/pkg/__init__.py"), "src.pkg");
        assert_eq!(module_path("src/pkg/deep/one.py"), "src.pkg.deep.one");
        assert_eq!(module_path("setup.py"), "setup");
    }

    /// Tree-sitter is error tolerant: a half-saved module still yields the
    /// declarations the grammar recovers.
    #[test]
    fn a_python_file_with_a_syntax_error_still_yields_what_it_recovers() {
        let broken = "def one():\n    return 1\n\nclass Two(\n";
        let parsed = parse("src/broken.py", broken.as_bytes(), 0).expect("parses");
        assert!(
            parsed.symbols.iter().any(|symbol| symbol.name == "one"),
            "{:?}",
            parsed.symbols
        );
    }
}
