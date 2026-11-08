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
//!
//! What is Python's alone is here; the declarations the book keeps are the
//! tables of [`crate::lang::registry::PYTHON`] and the pipeline is
//! [`crate::lang::engine`] (S3, T2). The one hook the tables cannot cover is the
//! module constant: an assignment at the root of the file is a `const`, and an
//! attribute of a method is not a declaration (which is why the hook asks the
//! nesting first).

use crate::lang::emit::{emit_named, last_segment, text};
use crate::lang::engine::{Body, Nesting};
use crate::lang::model::ParsedSymbol;
use crate::relations::{push_edges, RawEdge, CALLS, IMPORTS, INHERITS, USES};

/// The language of the chapter, as the table `file` spells it.
pub const LANGUAGE: &str = "python";

/// The tree-sitter grammar of Python: one grammar answers every Python path.
pub fn language(_path: &str) -> tree_sitter::Language {
    tree_sitter::Language::new(tree_sitter_python::LANGUAGE)
}

/// A module-level assignment: the constant of the language. `self.x` is an
/// attribute of a method, never a declaration of the book, so the hook only
/// answers outside a container (T2: this one stays imperative).
pub(crate) fn module_constant(
    node: &tree_sitter::Node,
    nesting: &Nesting,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) -> Option<Body> {
    if nesting.container.is_some() {
        return None;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() != "assignment" {
            continue;
        }
        if let Some(name) = child
            .child_by_field_name("left")
            .and_then(|left| text(&left, source))
        {
            emit_named(&child, "const", &name, nesting.prefix, nesting.parent, out);
        }
    }
    None
}

/// The module path of a Python file: its directories and its name without the
/// extension, dotted, with `__init__` naming its directory (the idiom of the
/// language).
pub fn module_path(relative: &str) -> String {
    crate::lang::emit::dotted_module(relative, &["__init__"])
}

/// The relations of one Python chapter, as the grammar sees them.
pub(crate) fn relations(root: &tree_sitter::Node, source: &[u8]) -> Vec<RawEdge> {
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
    use super::module_path;
    use crate::lang::registry::PYTHON;

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
        let parsed = PYTHON
            .parse("src/broken.py", broken.as_bytes(), 0)
            .expect("parses");
        assert!(
            parsed.symbols.iter().any(|symbol| symbol.name == "one"),
            "{:?}",
            parsed.symbols
        );
    }
}
