//! The Rust chapter of the book (S3: one module per language, L5).
//!
//! Only the top level survives (ideas §3, «descarte del CST completo»): the
//! functions, the types, the traits, the `impl` blocks with their methods, the
//! modules and the constants, each with its span. No CST is stored, and no
//! content of the file is stored: the book keeps the names and where they are.
//!
//! The relations of this grammar are the four kinds of the schema, read
//! syntactically (D3):
//!
//! - `calls` — `open_store()`, `store.load()`, `DiagramStore::new()`;
//! - `imports` — the segment a `use` binds, one per imported item;
//! - `inherits` — the trait of an `impl Trait for Type`;
//! - `uses` — every type reference the grammar types as a type.
//!
//! This module is the whole of Rust in the engine: a language of C2 is another
//! file next to it that exposes a [`Grammar`] and nothing else.

use crate::lang::emit::{anchor_symbol, emit, emit_named, file_stem, prefix_of};
use crate::lang::model::{content_hash, ExtractError, ParsedFile, ParsedSymbol};
use crate::lang::Grammar;
use crate::relations::{attach, RawEdge, CALLS, IMPORTS, INHERITS, USES};

/// The Rust grammar of the book.
pub const GRAMMAR: Grammar = Grammar {
    lang: LANGUAGE,
    extensions: &["rs"],
    parse,
};

/// The language of the chapter, as the table `file` spells it.
pub const LANGUAGE: &str = "rust";

/// Parses one Rust file into the pruned symbols and the relations of the book.
pub fn parse(path: &str, bytes: &[u8], mtime_ns: i64) -> Result<ParsedFile, ExtractError> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter::Language::new(tree_sitter_rust::LANGUAGE))
        .map_err(|error| ExtractError::Grammar {
            language: LANGUAGE,
            detail: error.to_string(),
        })?;
    let tree = parser
        .parse(bytes, None)
        .ok_or(ExtractError::NoTree { language: LANGUAGE })?;

    let module = module_path(path);
    let mut symbols = Vec::new();
    let prefix = prefix_of(&module, "::");
    walk(&tree.root_node(), &prefix, None, None, bytes, &mut symbols);

    // The relations are attached after the declarations exist: the ownership
    // of an edge is decided by the spans of the chapter (ideas §10.1).
    let raw = rust_relations(&tree.root_node(), bytes);
    let anchor = anchor_symbol(&module, &anchor_name(path, &module));
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

/// Walks the named children of a node and emits the declarations it knows.
///
/// A container (`mod`, `impl`, `trait`) contributes one symbol and its body is
/// walked with the container as parent, so `parent_id` is the containment
/// tree of the file and `impl` methods are `method` children.
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
            // The body of a `mod`/`impl`/`trait`: the prefix and the parent
            // are the ones of the container that owns it.
            "declaration_list" | "field_declaration_list" => {
                walk(&child, prefix, parent, container, source, out);
            }
            "function_item" => {
                // A fn of an `impl` or of a `trait` is a method; a fn of a
                // `mod` is a top-level fn of that module.
                let kind = match container {
                    Some("impl") | Some("trait") => "method",
                    _ => "fn",
                };
                emit(&child, kind, prefix, parent, source, out);
            }
            "struct_item" | "enum_item" | "union_item" | "type_item" => {
                emit(&child, "type", prefix, parent, source, out);
            }
            "trait_item" => {
                if let Some(index) = emit(&child, "trait", prefix, parent, source, out) {
                    let own = format!("{prefix}{}::", out[index].name);
                    walk_body(&child, &own, Some(index), "trait", source, out);
                }
            }
            "impl_item" => {
                // The name of an `impl` is the type it implements: it is what
                // makes two `impl` blocks of the same file distinguishable.
                let name = child
                    .child_by_field_name("type")
                    .and_then(|node| node.utf8_text(source).ok())
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                    .unwrap_or("impl")
                    .to_string();
                if let Some(index) = emit_named(&child, "impl", &name, prefix, parent, out) {
                    let own = format!("{prefix}{name}::");
                    walk_body(&child, &own, Some(index), "impl", source, out);
                }
            }
            "mod_item" => {
                if let Some(index) = emit(&child, "module", prefix, parent, source, out) {
                    let own = format!("{prefix}{}::", out[index].name);
                    walk_body(&child, &own, Some(index), "mod", source, out);
                }
            }
            "const_item" => {
                emit(&child, "const", prefix, parent, source, out);
            }
            "static_item" => {
                emit(&child, "static", prefix, parent, source, out);
            }
            // A `use` is not an entry of the book: C1 turns it into an
            // `imports` edge, and a comment or a string into nothing at all.
            _ => {}
        }
    }
}

/// Walks the body of a container (`impl`, `trait`, inline `mod`).
fn walk_body(
    node: &tree_sitter::Node,
    prefix: &str,
    parent: Option<usize>,
    container: &'static str,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "declaration_list" {
            walk(&child, prefix, parent, Some(container), source, out);
        }
    }
}

/// The name of the anchor of a Rust chapter: a Rust chapter **is** the module,
/// so its name is the whole module path (the stem of the file at the root).
fn anchor_name(path: &str, module: &str) -> String {
    if module.is_empty() {
        file_stem(path).unwrap_or_else(|| path.to_string())
    } else {
        module.to_string()
    }
}

/// The module path of a file, used as the prefix of every `qname`.
///
/// `crates/gx-mcp/src/tools.rs` → `gx_mcp::tools`; `src/lib.rs` → `` (the
/// crate root); a file outside a `src/` keeps its directories.
pub fn module_path(relative: &str) -> String {
    let parts: Vec<&str> = relative
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    if parts.is_empty() {
        return String::new();
    }
    let mut modules: Vec<String> = Vec::new();
    let mut file_parts: Vec<&str> = Vec::new();
    if let Some(src_index) = parts.iter().rposition(|part| *part == "src") {
        if src_index > 0 {
            modules.push(normalise_identifier(parts[src_index - 1]));
        }
        file_parts.extend_from_slice(&parts[src_index + 1..]);
    } else {
        file_parts.extend_from_slice(&parts);
    }
    for part in file_parts {
        let stem = part.strip_suffix(".rs").unwrap_or(part);
        if stem.is_empty() || stem == "lib" || stem == "main" {
            continue;
        }
        // `mod.rs` names its directory, not a module of its own.
        if stem == "mod" {
            continue;
        }
        modules.push(normalise_identifier(stem));
    }
    modules
        .into_iter()
        .filter(|module| !module.is_empty())
        .collect::<Vec<_>>()
        .join("::")
}

/// Keeps `[a-z0-9_]`, folding the rest to `_`: the name of a Rust module.
fn normalise_identifier(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for char in text.chars() {
        if char.is_ascii_alphanumeric() {
            out.push(char.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

/// The relations of one Rust chapter, as the grammar sees them.
fn rust_relations(root: &tree_sitter::Node, source: &[u8]) -> Vec<RawEdge> {
    let mut out = Vec::new();
    visit(root, source, &mut out);
    out
}

/// Walks the tree once and records every relation it recognises.
fn visit(node: &tree_sitter::Node, source: &[u8], out: &mut Vec<RawEdge>) {
    match node.kind() {
        "call_expression" => {
            if let Some(name) = node
                .child_by_field_name("function")
                .and_then(|function| called_name(&function, source))
            {
                out.push(RawEdge {
                    byte: node.start_byte(),
                    kind: CALLS,
                    name,
                });
            }
        }
        "use_declaration" => {
            let mut names = Vec::new();
            let argument = node
                .child_by_field_name("argument")
                .or_else(|| node.named_child(node.named_child_count().saturating_sub(1)));
            if let Some(argument) = argument {
                use_names(&argument, source, &mut names);
            }
            for name in names {
                out.push(RawEdge {
                    byte: node.start_byte(),
                    kind: IMPORTS,
                    name,
                });
            }
        }
        "impl_item" => {
            if let Some(name) = node
                .child_by_field_name("trait")
                .and_then(|trait_node| type_name(&trait_node, source))
            {
                out.push(RawEdge {
                    byte: node.start_byte(),
                    kind: INHERITS,
                    name,
                });
            }
        }
        // A type reference in any position the grammar types as a type: a
        // parameter, a return type, a field, a generic argument.
        "type_identifier" => {
            if let Ok(text) = node.utf8_text(source) {
                out.push(RawEdge {
                    byte: node.start_byte(),
                    kind: USES,
                    name: text.to_string(),
                });
            }
        }
        // Comments, strings, attributes and macros carry prose: no relation.
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        visit(&child, source, out);
    }
}

/// The name a call expression targets, when it has one.
fn called_name(function: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    let name = match function.kind() {
        "identifier" | "type_identifier" => Some(*function),
        // `store.load()`, `self.helper()`: the method name.
        "field_expression" => function.child_by_field_name("field"),
        // `DiagramStore::new()`: the last segment.
        "scoped_identifier" | "scoped_type_identifier" => function.child_by_field_name("name"),
        // `open::<u8>()`: the function is wrapped in its type arguments.
        "generic_function" => {
            return function
                .child_by_field_name("function")
                .and_then(|inner| called_name(&inner, source));
        }
        _ => None,
    }?;
    name.utf8_text(source).ok().map(str::to_string)
}

/// The names a `use` declaration binds, one per imported item.
fn use_names(node: &tree_sitter::Node, source: &[u8], out: &mut Vec<String>) {
    match node.kind() {
        // `use a::{b, c}`: the names are the list, not the module.
        "scoped_use_list" => {
            if let Some(list) = node.child_by_field_name("list") {
                use_names(&list, source, out);
            }
        }
        "use_list" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                use_names(&child, source, out);
            }
        }
        // `use a::b as c`: the imported item is `b`.
        "use_as_clause" => {
            if let Some(path) = node.child_by_field_name("path") {
                use_names(&path, source, out);
            }
        }
        // `use a::b::*`: the path's last segment.
        "use_wildcard" => {
            if let Some(path) = node.named_child(0) {
                push_last_segment(&path, source, out);
            }
        }
        "scoped_identifier" | "identifier" | "type_identifier" => {
            push_last_segment(node, source, out);
        }
        _ => {}
    }
}

/// Pushes the last `::` segment of a node's text.
fn push_last_segment(node: &tree_sitter::Node, source: &[u8], out: &mut Vec<String>) {
    if let Ok(text) = node.utf8_text(source) {
        out.push(text.to_string());
    }
}

/// The name of a type node: `Trait<T>` → `Trait`, `a::Trait` → `Trait`.
fn type_name(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    let node = if node.kind() == "generic_type" {
        node.child_by_field_name("type")?
    } else {
        *node
    };
    node.utf8_text(source).ok().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::{module_path, parse};

    const SOURCE: &str = r#"
use std::path::Path;

pub const LIMIT: usize = 3;
static COUNTER: u8 = 0;

pub struct Config {
    pub name: String,
}

pub enum Mode {
    Fast,
}

pub type Alias = Config;

pub trait Store {
    fn load(&self) -> u8;
}

impl Store for Config {
    fn load(&self) -> u8 {
        1
    }

    pub fn renderWith(&self) -> u8 {
        2
    }
}

mod inner {
    pub fn helper() {}
}

pub fn render_with_all(input: &Path) -> usize {
    input.as_os_str().len()
}
"#;

    /// S6/C0: the book keeps the declarations of the chapter with their spans,
    /// and `impl` methods hang from the `impl` (containment, `parent_id`).
    #[test]
    fn extracts_the_declarations_of_a_rust_file_with_spans() {
        let parsed = parse("src/demo.rs", SOURCE.as_bytes(), 0).expect("parses");
        let names: Vec<(&str, &str)> = parsed
            .symbols
            .iter()
            .map(|symbol| (symbol.kind, symbol.name.as_str()))
            .collect();

        for expected in [
            ("const", "LIMIT"),
            ("static", "COUNTER"),
            ("type", "Config"),
            ("type", "Mode"),
            ("type", "Alias"),
            ("trait", "Store"),
            ("impl", "Config"),
            ("module", "inner"),
            ("fn", "helper"),
            ("fn", "render_with_all"),
        ] {
            assert!(
                names.contains(&expected),
                "missing {expected:?} in {names:?}"
            );
        }
        assert!(
            names.contains(&("method", "renderWith")),
            "a fn inside an impl is a method: {names:?}"
        );

        let render = parsed
            .symbols
            .iter()
            .find(|symbol| symbol.name == "render_with_all")
            .expect("the fn");
        assert_eq!(render.qname, "demo::render_with_all");
        let expected_line = SOURCE
            .lines()
            .position(|line| line.contains("pub fn render_with_all"))
            .expect("the fn is in the source") as u32
            + 1;
        assert_eq!(render.start_line, expected_line, "1-based line of the fn");
        assert!(render.start_col >= 1);
        assert!(render.end_line >= render.start_line);
        assert!(render.end_byte > render.start_byte);

        let method = parsed
            .symbols
            .iter()
            .find(|symbol| symbol.name == "renderWith")
            .expect("the method");
        let parent = method.parent.expect("the method hangs from the impl");
        assert_eq!(parsed.symbols[parent].kind, "impl");
        assert_eq!(method.qname, "demo::Config::renderWith");
    }

    /// The module path is the prefix of every `qname` and the anchor of the
    /// ambiguity report: the same name in two files stays distinguishable.
    #[test]
    fn the_module_path_of_a_file_names_its_crate_and_its_directories() {
        assert_eq!(module_path("crates/gx-mcp/src/tools.rs"), "gx_mcp::tools");
        assert_eq!(module_path("crates/gx-index/src/lib.rs"), "gx_index");
        assert_eq!(
            module_path("crates/gx-index/src/book/mod.rs"),
            "gx_index::book"
        );
        assert_eq!(module_path("src/main.rs"), "");
        assert_eq!(module_path("tests/end_to_end.rs"), "tests::end_to_end");
        assert_eq!(module_path(""), "");
    }

    /// C1 published the anchor of a chapter as the **whole module path**: a
    /// Rust chapter is the module, so `crates/x/src/demo.rs` anchors as
    /// `x::demo` and not as `demo`.
    #[test]
    fn the_anchor_of_a_nested_chapter_is_its_module_path() {
        let parsed =
            parse("crates/gx-index/src/demo.rs", b"use std::path::Path;\n", 0).expect("parses");
        let anchor = parsed
            .symbols
            .iter()
            .find(|symbol| symbol.kind == "module")
            .expect("the file-scope import needs an anchor");
        assert_eq!(anchor.name, "gx_index::demo");
        assert_eq!(anchor.qname, "gx_index::demo");
    }

    /// Tree-sitter is error tolerant: a file with a syntax error still yields
    /// the declarations the grammar recovers, which is what a worktree with a
    /// half-saved file needs.
    #[test]
    fn a_file_with_a_syntax_error_still_yields_what_the_grammar_recovers() {
        let broken = "pub fn one() {}\nfn two( {\n";
        let parsed = parse("src/broken.rs", broken.as_bytes(), 0).expect("parses");
        assert!(
            parsed.symbols.iter().any(|symbol| symbol.name == "one"),
            "{:?}",
            parsed.symbols
        );
    }
}
