//! The Rust chapter of the book: Tree-sitter pruned to the declarations.
//!
//! Only the top level survives (ideas §3, «descarte del CST completo»): the
//! functions, the types, the traits, the `impl` blocks with their methods, the
//! modules and the constants, each with its span. No CST is stored, and no
//! content of the file is stored: the book keeps the names and where they are.
//!
//! `search_text` is **derived at insert time** because `unicode61` does not
//! split CamelCase (ideas §7, the queries explorer): the column carries the name,
//! its words separated and its CamelCase form, so `renderWith`, `render_with`
//! and `render with` all find the same symbol.

use sha2::{Digest, Sha256};

/// The language of the chapter (C0 has one; C2 adds the rest of D2).
pub const LANGUAGE: &str = "rust";

/// Files bigger than this are skipped with a warning (D2): a Rust source that
/// big is generated, and parsing it costs more than it answers.
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// What can go wrong parsing one file.
#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    /// Tree-sitter could not build a parser for the language.
    #[error("cannot load the Rust grammar: {0}")]
    Grammar(String),
    /// The parser did not return a tree.
    #[error("the Rust parser returned no tree")]
    NoTree,
    /// The shell around tree-sitter failed to compile.
    #[error("the file cannot be read as Rust source")]
    Unreadable,
}

/// One symbol of a file, with its span and its derived search text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSymbol {
    /// Index of the container in the same vector, when it has one.
    pub parent: Option<usize>,
    /// `fn`, `method`, `type`, `trait`, `impl`, `module`, `const` or `static`.
    pub kind: &'static str,
    /// The simple name: `render_with`.
    pub name: String,
    /// The qualified name: `gx_mcp::tools::render_with`.
    pub qname: String,
    /// The derived text FTS5 indexes (ideas §10.1, delta 1).
    pub search_text: String,
    /// The unified collation for edge resolution (ideas §10.1, delta 6).
    pub name_norm: String,
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
    pub start_byte: u32,
    pub end_byte: u32,
}

/// A file ready to be committed to the book.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedFile {
    /// Path relative to the root of the worktree, with `/` separators.
    pub path: String,
    /// The language of the chapter.
    pub lang: &'static str,
    /// sha256 of the bytes that were parsed.
    pub content_hash: String,
    /// Size of the file, in bytes.
    pub size: i64,
    /// Modification time, in nanoseconds since the epoch.
    pub mtime_ns: i64,
    /// The symbols, parents before children.
    pub symbols: Vec<ParsedSymbol>,
}

/// The language of a path, when the book knows it (only Rust in C0).
pub fn language_for(path: &std::path::Path) -> Option<&'static str> {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("rs") => Some(LANGUAGE),
        _ => None,
    }
}

/// Whether a file looks binary: a NUL byte in the first 8 KiB. The book never
/// parses binary content, and skipping it is a warning, not an error (D2).
pub fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|byte| *byte == 0)
}

/// sha256 of the bytes, as lowercase hex.
pub fn content_hash(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// Parses one Rust file into the pruned symbols of the book.
pub fn parse_rust(path: &str, bytes: &[u8], mtime_ns: i64) -> Result<ParsedFile, ExtractError> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter::Language::new(tree_sitter_rust::LANGUAGE))
        .map_err(|error| ExtractError::Grammar(error.to_string()))?;
    let tree = parser.parse(bytes, None).ok_or(ExtractError::NoTree)?;

    let module = module_path(path);
    let mut symbols = Vec::new();
    let prefix = if module.is_empty() {
        String::new()
    } else {
        module
    };
    walk(&tree.root_node(), &prefix, None, None, bytes, &mut symbols);

    Ok(ParsedFile {
        path: path.to_string(),
        lang: LANGUAGE,
        content_hash: content_hash(bytes),
        size: bytes.len() as i64,
        mtime_ns,
        symbols,
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
                emit(child, kind, prefix, parent, source, out);
            }
            "struct_item" | "enum_item" | "union_item" | "type_item" => {
                emit(child, "type", prefix, parent, source, out);
            }
            "trait_item" => {
                if let Some(index) = emit(child, "trait", prefix, parent, source, out) {
                    let own = qualified(prefix, &out[index].name);
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
                if let Some(index) = emit_named(child, "impl", &name, prefix, parent, out) {
                    let own = qualified(prefix, &name);
                    walk_body(&child, &own, Some(index), "impl", source, out);
                }
            }
            "mod_item" => {
                if let Some(index) = emit(child, "module", prefix, parent, source, out) {
                    let own = qualified(prefix, &out[index].name);
                    walk_body(&child, &own, Some(index), "mod", source, out);
                }
            }
            "const_item" => {
                emit(child, "const", prefix, parent, source, out);
            }
            "static_item" => {
                emit(child, "static", prefix, parent, source, out);
            }
            // `use`, attributes, comments and macro calls are not entries of
            // the book: C1 uses them for edges, C0 does not store them.
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

/// Emits a symbol whose name comes from the `name` field.
fn emit(
    node: tree_sitter::Node,
    kind: &'static str,
    prefix: &str,
    parent: Option<usize>,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) -> Option<usize> {
    let name = node
        .child_by_field_name("name")
        .and_then(|name| name.utf8_text(source).ok())
        .map(str::trim)
        .filter(|name| !name.is_empty())?
        .to_string();
    emit_named(node, kind, &name, prefix, parent, out)
}

/// Emits a symbol with an explicit name (`impl` blocks have no `name` field).
fn emit_named(
    node: tree_sitter::Node,
    kind: &'static str,
    name: &str,
    prefix: &str,
    parent: Option<usize>,
    out: &mut Vec<ParsedSymbol>,
) -> Option<usize> {
    let start = node.start_position();
    let end = node.end_position();
    let qname = qualified(prefix, name);
    let index = out.len();
    out.push(ParsedSymbol {
        parent,
        kind,
        name: name.to_string(),
        qname,
        search_text: search_text_for(name),
        name_norm: name_norm(name),
        start_line: (start.row as u32).saturating_add(1),
        start_col: (start.column as u32).saturating_add(1),
        end_line: (end.row as u32).saturating_add(1),
        end_col: end.column as u32,
        start_byte: node.start_byte() as u32,
        end_byte: node.end_byte() as u32,
    });
    Some(index)
}

/// `prefix::name`, or just `name` at the root.
fn qualified(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}::{name}")
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

/// The collation of every resolution that is not FTS (ideas §10.1, delta 6).
pub fn name_norm(name: &str) -> String {
    name.to_lowercase()
}

/// The derived search text of a name: the name, its words and its CamelCase.
///
/// `renderWith` → `renderWith render with RenderWith` (ideas §10.1);
/// `render_with` → `render_with render with RenderWith`. Both forms keep the
/// snake_case name whole in the first segment, which is what a query for
/// `render_with` hits.
pub fn search_text_for(name: &str) -> String {
    let words = words(name);
    let mut segments: Vec<String> = vec![name.to_string()];
    let spaced = words.join(" ");
    if !spaced.is_empty() {
        segments.push(spaced);
    }
    let camel: String = words
        .iter()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect();
    if !camel.is_empty() {
        segments.push(camel);
    }
    segments.dedup();
    segments
        .into_iter()
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The lowercase words of an identifier: split at `_`, at non-alphanumerics
/// and at the CamelCase boundaries (including the end of an acronym).
fn words(name: &str) -> Vec<String> {
    let chars: Vec<char> = name.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    for (index, char) in chars.iter().copied().enumerate() {
        if !char.is_alphanumeric() {
            flush(&mut current, &mut out);
            continue;
        }
        let previous = index.checked_sub(1).map(|previous| chars[previous]);
        let next = chars.get(index + 1).copied();
        let boundary = char.is_uppercase()
            && match previous {
                Some(previous) if previous.is_lowercase() || previous.is_ascii_digit() => true,
                Some(previous) if previous.is_uppercase() => {
                    next.is_some_and(|next| next.is_lowercase())
                }
                _ => false,
            };
        if boundary {
            flush(&mut current, &mut out);
        }
        current.extend(char.to_lowercase());
    }
    flush(&mut current, &mut out);
    out
}

/// Pushes the pending word, if any.
fn flush(current: &mut String, out: &mut Vec<String>) {
    if !current.is_empty() {
        out.push(std::mem::take(current));
    }
}

#[cfg(test)]
mod tests {
    use super::{
        content_hash, looks_binary, module_path, name_norm, parse_rust, search_text_for, words,
    };

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
        let parsed = parse_rust("src/demo.rs", SOURCE.as_bytes(), 0).expect("parses");
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

    /// Ideas §10.1, delta 1: `search_text` is derived at insert time.
    #[test]
    fn the_search_text_splits_camel_case_and_snake_case() {
        assert_eq!(
            search_text_for("renderWith"),
            "renderWith render with RenderWith"
        );
        assert_eq!(
            search_text_for("render_with"),
            "render_with render with RenderWith"
        );
        assert_eq!(name_norm("renderWith"), "renderwith");
        assert_eq!(words("HTTPServer"), vec!["http", "server"]);
        assert_eq!(words("gx_index_v2"), vec!["gx", "index", "v2"]);
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
    }

    #[test]
    fn a_binary_file_is_detected_by_its_nul_byte() {
        assert!(looks_binary(b"\x7fELF\0rest"));
        assert!(!looks_binary(b"pub fn main() {}\n"));
        assert_eq!(content_hash(b"abc").len(), 64);
    }

    /// Tree-sitter is error tolerant: a file with a syntax error still yields
    /// the declarations the grammar recovers, which is what a worktree with a
    /// half-saved file needs.
    #[test]
    fn a_file_with_a_syntax_error_still_yields_what_the_grammar_recovers() {
        let broken = "pub fn one() {}\nfn two( {\n";
        let parsed = parse_rust("src/broken.rs", broken.as_bytes(), 0).expect("parses");
        assert!(
            parsed.symbols.iter().any(|symbol| symbol.name == "one"),
            "{:?}",
            parsed.symbols
        );
    }
}
