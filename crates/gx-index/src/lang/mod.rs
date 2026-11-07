//! The grammars of the book: one module per language (S3, L5) over one shared
//! engine (S3, T2).
//!
//! This directory is the whole of Tree-sitter in the engine. A language
//! contributes what only it knows — the tree-sitter grammar, the module path of
//! a file, the relations it reads and the few hooks its grammar needs
//! ([`engine`]) — and its node-kind → kind tables, its transparent kinds and its
//! containers live in one registry next to the tables of the other four
//! ([`registry`]). Nothing outside this directory moves: the schema, the
//! queries, the watcher and the tools speak of [`ParsedFile`], whichever grammar
//! produced it, and the relation vocabulary of the edges is the schema's, not
//! the language's.
//!
//! Adding a language (T3) is therefore: a grammar dependency and a cargo feature
//! in `Cargo.toml`, a module next to the others with its `language()`,
//! `module_path()` and `relations()` (plus a hook only where its grammar names a
//! declaration in a way the tables cannot), its entry in [`registry::GRAMMARS`].
//! The pipeline (`engine::extract`) and the queries are not touched.
//!
//! Each language is also a **cargo feature** of the engine (T4/C3, the lever of
//! RNF1-quater): `lang-rust`, `lang-typescript`, `lang-python`, `lang-go` and
//! `lang-java`, all five **on by default**, so nothing changes for a build that
//! does not ask. A slim build drops a grammar together with its dependency —
//! `--no-default-features --features "lang-rust,lang-typescript"` — and this
//! directory is the only place that knows: [`GRAMMARS`] simply lists the
//! grammars of the build, and everything outside here speaks of [`ParsedFile`].

mod emit;
pub mod engine;
#[cfg(feature = "lang-go")]
pub mod go;
#[cfg(feature = "lang-java")]
pub mod java;
pub mod model;
#[cfg(feature = "lang-python")]
pub mod python;
pub mod registry;
#[cfg(feature = "lang-rust")]
pub mod rust;
#[cfg(feature = "lang-typescript")]
pub mod typescript;

pub use crate::relations::ParsedEdge;
pub use engine::Grammar;
pub use model::{
    content_hash, looks_binary, name_norm, search_text_for, ExtractError, ParsedFile, ParsedSymbol,
};
pub use registry::GRAMMARS;

/// Files bigger than this are skipped with a warning (D2): a source that big is
/// generated, and parsing it costs more than it answers.
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;

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
    grammar.parse(path, bytes, mtime_ns)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{grammar, language_for, GRAMMARS};

    /// S3/L5: the registry answers by extension, and the language it answers is
    /// the one the table `file` stores. C2 adds the four languages after Rust,
    /// TypeScript/TSX sharing one module and one language name. The expectations
    /// are built with the same features that built the grammars (T4/C3), so a
    /// slim build is checked against itself and never against five languages it
    /// does not have.
    #[test]
    fn the_registry_knows_the_languages_of_the_build() {
        #[allow(unused_mut)]
        let mut expected: Vec<(&str, &str)> = Vec::new();
        #[cfg(feature = "lang-rust")]
        expected.push(("src/main.rs", "rust"));
        #[cfg(feature = "lang-typescript")]
        {
            expected.push(("src/main.ts", "typescript"));
            expected.push(("src/main.tsx", "typescript"));
        }
        #[cfg(feature = "lang-python")]
        expected.push(("src/main.py", "python"));
        #[cfg(feature = "lang-go")]
        expected.push(("src/main.go", "go"));
        #[cfg(feature = "lang-java")]
        expected.push(("src/main.java", "java"));

        for (path, lang) in expected {
            assert_eq!(language_for(Path::new(path)), Some(lang), "`{path}`");
            assert!(grammar(lang).is_some(), "the registry knows `{lang}`");
        }
        // One grammar per language of the build (S3), and nothing else.
        assert_eq!(
            GRAMMARS.len(),
            [
                cfg!(feature = "lang-rust"),
                cfg!(feature = "lang-typescript"),
                cfg!(feature = "lang-python"),
                cfg!(feature = "lang-go"),
                cfg!(feature = "lang-java"),
            ]
            .iter()
            .filter(|enabled| **enabled)
            .count(),
            "one grammar per enabled language"
        );
        assert_eq!(language_for(Path::new("src/notes.txt")), None);
        assert!(grammar("ruby").is_none());
        #[cfg(feature = "lang-typescript")]
        {
            assert_eq!(
                grammar("typescript").map(|grammar| grammar.extensions),
                Some(&["ts", "tsx"][..]),
                "one module covers both TypeScript grammars"
            );
        }
    }

    /// A path without a grammar is an error and never a silent empty chapter.
    #[cfg(feature = "lang-rust")]
    #[test]
    fn a_path_without_a_grammar_is_an_error() {
        use super::parse;

        assert!(parse("src/notes.txt", b"hello", 0).is_err());
        let parsed = parse("src/one.rs", b"pub fn one() {}\n", 0).expect("the Rust grammar");
        assert_eq!(parsed.lang, "rust");
        assert!(parsed.symbols.iter().any(|symbol| symbol.name == "one"));
    }
}
