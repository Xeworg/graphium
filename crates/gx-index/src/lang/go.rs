//! The Go chapter of the book (S3, L5).
//!
//! The chapter keeps the package scope of a file — the functions, the methods
//! (hanging from their receiver type, whichever order the file declares them
//! in), the declared types, the constants and the package variables — with the
//! package as the prefix of every `qname`
//! (`internal.store.DiagramStore.RenderWith`). The package of a Go file is its
//! directory, so that is what the book uses as the module path.
//!
//! The relations of this grammar are the four kinds of the schema, read
//! syntactically (D3):
//!
//! - `calls` — `OpenStore()`, `fmt.Println()`, `store.Load()`;
//! - `imports` — the path segment an `import` names (`net/http` → `http`);
//! - `inherits` — the embedding of a struct and of an interface;
//! - `uses` — every type the grammar types as a type, generics included.
//!
//! A `var` of a package is the storage of the language: the book keeps it with
//! the `static` kind of the vocabulary, the one it has for storage that is not
//! a constant.
//!
//! **The package is a declaration (S5-b).** In Go the package is what an
//! `import` of the directory binds, so the chapter declares it: one `module`
//! symbol named `store` in `internal/store/store.go`, qualified by the module
//! path (`internal.store`). Without it an `import` of a package the book itself
//! indexes has no target — the edge is read and lost — and with it the two-file
//! package of the f10 record measures 6/6 = 100,0 % of its relations instead of
//! 5/6 = 83,3 %. The declaration is the shape [`crate::relations::attach`]
//! appends for file-scope relations, and the anchor it feeds is the same one:
//! the chapter declares its package once, never twice.
//!
//! What is Go's alone is here; the entry of the language is
//! [`crate::lang::registry::GO`] and the pipeline is [`crate::lang::engine`]
//! (S3, T2). Go is the language whose scope is not a tree of containers — a
//! method hangs from the type of its receiver whichever order the file declares
//! them in — so its walk is its own and the second pass over the receivers
//! stays imperative.

use crate::lang::emit::{anchor_symbol, dotted_directories, emit, text};
use crate::lang::model::ParsedSymbol;
use crate::relations::{RawEdge, CALLS, IMPORTS, INHERITS, USES};

/// The language of the chapter, as the table `file` spells it.
pub const LANGUAGE: &str = "go";

/// The tree-sitter grammar of Go: one grammar answers every Go path.
pub fn language(_path: &str) -> tree_sitter::Language {
    tree_sitter::Language::new(tree_sitter_go::LANGUAGE)
}

/// Walks the package scope of one Go file, and re-parents its methods once the
/// whole file is read (T2: the second pass of the receivers stays imperative).
///
/// The package the file declares is a declaration of the chapter (S5-b): in Go
/// the package is the name an `import` of the directory binds, so the chapter
/// that is imported has to declare it, or every `import` of a package the book
/// itself indexes is an edge with no target.
pub(crate) fn walk(
    root: &tree_sitter::Node,
    prefix: &str,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) {
    let methods = scope(root, prefix, source, out);
    attach_receivers(out, &methods);
    declare_package(root, prefix, source, out);
}

/// The module path of a Go file: the package path, which in Go is the
/// directory.
pub fn module_path(relative: &str) -> String {
    dotted_directories(relative)
}

/// Declares the package of the chapter: `package store` is one `module` symbol
/// named `store` and qualified by the module path (`internal.store`), the one
/// shape [`crate::relations::attach`] appends for the file-scope relations of a
/// chapter — so a chapter that already declares its module keeps one anchor and
/// not two.
///
/// It is declared **always** and not only when a relation of the chapter needs
/// it: the `import` that needs it lives in another file, and the chapter cannot
/// know whether the book holds one. The span is empty, so it can never win an
/// ownership question against a declaration of the file.
fn declare_package(
    root: &tree_sitter::Node,
    prefix: &str,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) {
    let Some(name) = package_name(root, source) else {
        return;
    };
    let module = prefix.strip_suffix('.').unwrap_or(prefix);
    out.push(anchor_symbol(module, &name));
}

/// The package a Go file declares: the `package_identifier` of its package
/// clause. A file with no recoverable clause (a half-saved one) declares no
/// package the book could name.
fn package_name(root: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    let mut cursor = root.walk();
    let clause = root
        .named_children(&mut cursor)
        .find(|child| child.kind() == "package_clause")?;
    let mut inner = clause.walk();
    let identifier = clause
        .named_children(&mut inner)
        .find(|child| child.kind() == "package_identifier")?;
    text(&identifier, source)
}

/// Walks the package scope of a file and returns the methods with the receiver
/// type they name, so their parent and their `qname` can wait until every type
/// of the file is declared.
fn scope(
    node: &tree_sitter::Node,
    prefix: &str,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) -> Vec<(usize, String)> {
    let mut methods = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "function_declaration" => {
                emit(&child, "fn", prefix, None, source, out);
            }
            "method_declaration" => {
                let receiver = child
                    .child_by_field_name("receiver")
                    .and_then(|receiver| receiver_name(&receiver, source));
                if let Some(index) = emit(&child, "method", prefix, None, source, out) {
                    if let Some(receiver) = receiver {
                        methods.push((index, receiver));
                    }
                }
            }
            "type_declaration" => types(&child, prefix, source, out),
            "const_declaration" => specs(&child, "const", prefix, source, out),
            "var_declaration" => specs(&child, "static", prefix, source, out),
            _ => {}
        }
    }
    methods
}

/// Emits every type of a `type` declaration, and the methods of an interface
/// as children of it.
fn types(node: &tree_sitter::Node, prefix: &str, source: &[u8], out: &mut Vec<ParsedSymbol>) {
    let mut cursor = node.walk();
    for spec in node.named_children(&mut cursor) {
        if !matches!(spec.kind(), "type_spec" | "type_alias") {
            continue;
        }
        let declared = spec.child_by_field_name("type");
        let kind = match declared.as_ref().map(|node| node.kind()) {
            Some("struct_type") => "type",
            Some("interface_type") => "trait",
            _ => "type",
        };
        let Some(index) = emit(&spec, kind, prefix, None, source, out) else {
            continue;
        };
        // A method of an interface is declared inside the interface: it is a
        // child of it, and its name is the one a call may reach.
        let Some(interface) = declared.filter(|node| node.kind() == "interface_type") else {
            continue;
        };
        let own = format!("{prefix}{}.", out[index].name);
        let mut inner = interface.walk();
        for member in interface.named_children(&mut inner) {
            if member.kind() == "method_elem" {
                emit(&member, "method", &own, Some(index), source, out);
            }
        }
    }
}

/// Emits the names of a `const`/`var` declaration, which may be a group.
fn specs(
    node: &tree_sitter::Node,
    kind: &'static str,
    prefix: &str,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) {
    let mut cursor = node.walk();
    for spec in node.named_children(&mut cursor) {
        if matches!(spec.kind(), "const_spec" | "var_spec") {
            emit(&spec, kind, prefix, None, source, out);
        }
    }
}

/// Makes every method a child of the type of its receiver and gives it the
/// `qname` of the language: `pkg.Type.Method`.
fn attach_receivers(symbols: &mut [ParsedSymbol], methods: &[(usize, String)]) {
    for (index, receiver) in methods {
        let owner = symbols.iter().position(|symbol| {
            symbol.name == *receiver
                && matches!(symbol.kind, "type" | "trait")
                && symbol.parent.is_none()
        });
        let Some(owner) = owner else {
            continue;
        };
        let qname = format!("{}.{}", symbols[owner].qname, symbols[*index].name);
        symbols[*index].parent = Some(owner);
        symbols[*index].qname = qname;
    }
}

/// The type name of a method's receiver: `(d *DiagramStore)` → `DiagramStore`.
fn receiver_name(receiver: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    let parameter = receiver.named_child(0)?;
    let declared = parameter.child_by_field_name("type")?;
    type_identifier(&declared, source)
}

/// The last `type_identifier` of a type expression: `*DiagramStore` →
/// `DiagramStore`, `fmt.Stringer` → `Stringer`.
fn type_identifier(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "type_identifier" => text(node, source),
        "pointer_type" | "parenthesized_type" | "generic_type" | "slice_type" => node
            .named_child(0)
            .and_then(|inner| type_identifier(&inner, source)),
        // A qualified type names its package first and the type last.
        "qualified_type" => {
            let mut cursor = node.walk();
            node.named_children(&mut cursor)
                .filter(|child| child.kind() == "type_identifier")
                .last()
                .and_then(|name| text(&name, source))
        }
        _ => None,
    }
}

/// The relations of one Go chapter, as the grammar sees them.
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
        "import_spec" => {
            if let Some(name) = node
                .child_by_field_name("path")
                .and_then(|path| text(&path, source))
                .map(|path| package_segment(&path))
            {
                out.push(RawEdge {
                    byte,
                    kind: IMPORTS,
                    name,
                });
            }
        }
        // A field declaration without a name is an embedded type: the
        // inheritance of the language.
        "field_declaration" if node.child_by_field_name("name").is_none() => {
            if let Some(name) = node
                .child_by_field_name("type")
                .and_then(|declared| type_identifier(&declared, source))
            {
                out.push(RawEdge {
                    byte,
                    kind: INHERITS,
                    name,
                });
            }
        }
        // An interface embeds another interface the same way.
        "type_elem" => {
            if let Some(name) = node
                .named_child(0)
                .and_then(|element| type_identifier(&element, source))
            {
                out.push(RawEdge {
                    byte,
                    kind: INHERITS,
                    name,
                });
            }
        }
        // A type reference in any position the grammar types as a type.
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

/// The name a call targets, when it has one.
fn called(function: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    match function.kind() {
        "identifier" => text(function, source),
        // `fmt.Println()`, `store.Load()`: the field of the selector.
        "selector_expression" => function
            .child_by_field_name("field")
            .and_then(|field| text(&field, source)),
        // `fn()()`: no name to reach.
        _ => None,
    }
}

/// The package of an import path: `net/http` → `http`. Go spells a package
/// path with `/`, so the package is the segment after the last slash — not the
/// one a dotted or `::` path would answer.
fn package_segment(path: &str) -> String {
    let path = path.trim_matches('"').trim_matches('`');
    path.rsplit('/').next().unwrap_or(path).to_string()
}

#[cfg(test)]
mod tests {
    use super::{module_path, package_segment};
    use crate::lang::registry::GO;

    /// The package of a Go file is its directory: that is the module path.
    /// The import path of Go is spelled with `/`: the package is its last
    /// segment.
    #[test]
    fn the_import_path_of_go_yields_its_package() {
        assert_eq!(package_segment("\"net/http\""), "http");
        assert_eq!(package_segment("`github.com/x/store`"), "store");
        assert_eq!(package_segment("fmt"), "fmt");
    }

    #[test]
    fn the_module_path_of_a_go_file_is_its_package() {
        assert_eq!(module_path("internal/store/store.go"), "internal.store");
        assert_eq!(module_path("cmd/graphium/main.go"), "cmd.graphium");
        assert_eq!(module_path("main.go"), "");
    }

    /// S5(b): the chapter declares the package it is, with the name the source
    /// spells and the module path of the directory, and with no span — the
    /// anchor can own no relation of the file.
    #[test]
    fn a_go_chapter_declares_its_package() {
        let source = "package store\n\nfunc OpenStore() int { return 1 }\n";
        let parsed = GO
            .parse("internal/store/store.go", source.as_bytes(), 0)
            .expect("parses");
        let package = parsed
            .symbols
            .iter()
            .find(|symbol| symbol.kind == "module")
            .expect("the package is declared");
        assert_eq!(package.name, "store");
        assert_eq!(package.qname, "internal.store");
        assert_eq!(
            package.start_byte, package.end_byte,
            "an anchor owns nothing"
        );
    }

    /// A file with no recoverable package clause declares no package: the
    /// grammar is error tolerant, not inventive.
    #[test]
    fn a_go_file_with_no_package_clause_declares_no_package() {
        let parsed = GO
            .parse(
                "internal/store/broken.go",
                b"func One() int { return 1 }\n",
                0,
            )
            .expect("parses");
        assert!(
            !parsed.symbols.iter().any(|symbol| symbol.kind == "module"),
            "{:?}",
            parsed.symbols
        );
    }

    /// Tree-sitter is error tolerant: a half-saved file still yields the
    /// declarations the grammar recovers.
    #[test]
    fn a_go_file_with_a_syntax_error_still_yields_what_it_recovers() {
        let broken = "package store\n\nfunc One() int { return 1 }\n\nfunc Two( {\n";
        let parsed = GO
            .parse("store/broken.go", broken.as_bytes(), 0)
            .expect("parses");
        assert!(
            parsed.symbols.iter().any(|symbol| symbol.name == "One"),
            "{:?}",
            parsed.symbols
        );
    }
}
