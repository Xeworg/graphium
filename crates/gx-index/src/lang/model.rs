//! The model every grammar of the book produces (S3, L5).
//!
//! A grammar prunes its syntax tree down to the declarations and the relations
//! of one file; **nothing of the grammar's shape survives here**. A chapter is
//! a path, a language, a hash and two lists — symbols and edges — which is
//! exactly what the schema stores, so the next language added to `lang/` writes
//! into the same tables and the same queries without a schema change.
//!
//! The derivation of the search text lives here too: it is a property of the
//! book (`unicode61` does not split CamelCase, ideas §7) and not of the
//! language that named the symbol.

use sha2::{Digest, Sha256};

use crate::relations::ParsedEdge;

/// What can go wrong parsing one file.
#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    /// Tree-sitter could not build a parser for the language.
    #[error("cannot load the grammar of `{language}`: {detail}")]
    Grammar {
        language: &'static str,
        detail: String,
    },
    /// The parser did not return a tree.
    #[error("the parser of `{language}` returned no tree")]
    NoTree { language: &'static str },
    /// The book has no grammar for the path (or the shell around tree-sitter
    /// failed to compile the file).
    #[error("the file cannot be read as `{language}` source")]
    Unreadable { language: &'static str },
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
    /// The language of the chapter: the `lang` column of the table `file`.
    pub lang: &'static str,
    /// sha256 of the bytes that were parsed.
    pub content_hash: String,
    /// Size of the file, in bytes.
    pub size: i64,
    /// Modification time, in nanoseconds since the epoch.
    pub mtime_ns: i64,
    /// The symbols, parents before children.
    pub symbols: Vec<ParsedSymbol>,
    /// The relations of the chapter (S7): syntactic edges with their target as
    /// the source spells it, and the symbol that owns each one.
    pub edges: Vec<ParsedEdge>,
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
    use super::{looks_binary, name_norm, search_text_for, words};

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

    #[test]
    fn a_binary_file_is_detected_by_its_nul_byte() {
        assert!(looks_binary(b"\x7fELF\0rest"));
        assert!(!looks_binary(b"pub fn main() {}\n"));
    }
}
