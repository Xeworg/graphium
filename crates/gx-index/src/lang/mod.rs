//! The grammars of the book: one module per language (S3, L5).
//!
//! This directory is the whole of Tree-sitter in the engine. A language is one
//! module that exports a [`Grammar`] — the `lang` it writes to the table
//! `file`, the extensions it answers to and its `parse` — and one entry in
//! [`GRAMMARS`]. Nothing outside this directory moves: the schema, the queries,
//! the watcher and the tools speak of [`ParsedFile`], whichever grammar
//! produced it, and the relation vocabulary of the edges is the schema's, not
//! the language's.
//!
//! Adding a language (T3/C2) is therefore: a grammar dependency in
//! `Cargo.toml`, a module next to [`rust`], and its line in [`GRAMMARS`].

pub mod model;
pub mod rust;

pub use crate::relations::ParsedEdge;
pub use model::{
    content_hash, looks_binary, name_norm, search_text_for, ExtractError, ParsedFile, ParsedSymbol,
};

/// Files bigger than this are skipped with a warning (D2): a source that big is
/// generated, and parsing it costs more than it answers.
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// One grammar of the book.
#[derive(Debug, Clone, Copy)]
pub struct Grammar {
    /// The language as the table `file` spells it: `rust`, `typescript`…
    pub lang: &'static str,
    /// The extensions the grammar answers to, without the dot.
    pub extensions: &'static [&'static str],
    /// Parses one file into the pruned chapter of the book.
    pub parse: fn(&str, &[u8], i64) -> Result<ParsedFile, ExtractError>,
}

/// Every grammar this build ships, in the order the book lists them.
pub const GRAMMARS: &[Grammar] = &[rust::GRAMMAR];

/// The grammar of a language, by the `lang` name.
pub fn grammar(lang: &str) -> Option<&'static Grammar> {
    GRAMMARS.iter().find(|grammar| grammar.lang == lang)
}

/// The grammar of a path, when the book knows it.
pub fn grammar_for(path: &std::path::Path) -> Option<&'static Grammar> {
    let extension = path.extension().and_then(|extension| extension.to_str())?;
    GRAMMARS
        .iter()
        .find(|grammar| grammar.extensions.contains(&extension))
}

/// The language of a path, when the book knows it.
pub fn language_for(path: &std::path::Path) -> Option<&'static str> {
    grammar_for(path).map(|grammar| grammar.lang)
}

/// Parses one file with the grammar of its path.
pub fn parse(path: &str, bytes: &[u8], mtime_ns: i64) -> Result<ParsedFile, ExtractError> {
    let grammar = grammar_for(std::path::Path::new(path)).ok_or(ExtractError::Unreadable {
        language: "unknown",
    })?;
    (grammar.parse)(path, bytes, mtime_ns)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{grammar, grammar_for, language_for, parse, GRAMMARS};

    /// S3/L5: the registry answers by extension, and the language it answers is
    /// the one the table `file` stores.
    #[test]
    fn the_registry_knows_the_languages_of_the_build() {
        assert_eq!(language_for(Path::new("src/main.rs")), Some("rust"));
        assert_eq!(language_for(Path::new("src/main.py")), None);
        assert!(grammar("rust").is_some());
        assert!(grammar("python").is_none());
        assert_eq!(GRAMMARS.len(), 1);
        assert_eq!(
            grammar_for(Path::new("x.rs")).map(|grammar| grammar.lang),
            Some("rust")
        );
    }

    /// A path without a grammar is an error and never a silent empty chapter.
    #[test]
    fn a_path_without_a_grammar_is_an_error() {
        assert!(parse("src/notes.txt", b"hello", 0).is_err());
        let parsed = parse("src/one.rs", b"pub fn one() {}\n", 0).expect("the Rust grammar");
        assert_eq!(parsed.lang, "rust");
        assert!(parsed.symbols.iter().any(|symbol| symbol.name == "one"));
    }
}
