//! The grammars of the book: one module per language (S3, L5) over one shared
//! engine (S3, T2).
//!
//! This directory is the whole of Tree-sitter in the engine. A language
//! contributes what only it knows — the tree-sitter grammar, the module path of
//! a file, the relations it reads and the few hooks its grammar needs
//! ([`engine`]) — and its node-kind → kind tables, its transparent kinds and its
//! containers live in one registry next to the tables of the others
//! ([`registry`]). Nothing outside this directory moves: the schema, the
//! queries, the watcher and the tools speak of [`ParsedFile`], whichever grammar
//! produced it, and the relation vocabulary of the edges is the schema's, not
//! the language's.
//!
//! Adding a language (T3) is therefore: a grammar dependency and a cargo feature
//! in `Cargo.toml`, a module next to the others with its `language()`,
//! `module_path()` and `relations()` (plus a hook only where its grammar names a
//! declaration in a way the tables cannot), its entry in [`registry::GRAMMARS`],
//! and its fixtures and floor in `tests/languages.rs`. The pipeline
//! (`engine::extract`), the queries and the schema are not touched: the `lang`
//! values of the table `file` are the entries of the registry.
//!
//! Each language is also a **cargo feature** of the engine (T4/C3, the lever of
//! RNF1-quater): `lang-rust`, `lang-typescript`, `lang-python`, `lang-go` and
//! `lang-java` were the five original ones; since the single-binary decision
//! (L9) the **default build ships every language of the book**, `lang-javascript`,
//! `lang-c`, `lang-cpp`, `lang-csharp` (T3 Batch 1), `lang-php`, `lang-ruby`,
//! `lang-sql`, `lang-shell` (T3 Batch 2) and `lang-swift`, `lang-kotlin`,
//! `lang-dart` (T3 Batch 3) included. A slim build
//! drops a grammar together with its dependency —
//! `--no-default-features --features "lang-rust,lang-typescript"` — and this
//! directory is the only place that knows: [`GRAMMARS`] simply lists the
//! grammars of the build, and everything outside here speaks of [`ParsedFile`].

#[cfg(feature = "lang-c")]
pub mod c;
#[cfg(feature = "lang-cpp")]
pub mod cpp;
#[cfg(feature = "lang-csharp")]
pub mod csharp;
#[cfg(feature = "lang-dart")]
pub mod dart;
mod emit;
pub mod engine;
#[cfg(feature = "lang-go")]
pub mod go;
#[cfg(feature = "lang-java")]
pub mod java;
#[cfg(feature = "lang-javascript")]
pub mod javascript;
#[cfg(feature = "lang-kotlin")]
pub mod kotlin;
#[cfg(feature = "lang-lua")]
pub mod lua;
pub mod model;
#[cfg(feature = "lang-php")]
pub mod php;
#[cfg(feature = "lang-python")]
pub mod python;
#[cfg(feature = "lang-r")]
pub mod r;
pub mod registry;
#[cfg(feature = "lang-ruby")]
pub mod ruby;
#[cfg(feature = "lang-rust")]
pub mod rust;
#[cfg(feature = "lang-scala")]
pub mod scala;
#[cfg(feature = "lang-shell")]
pub mod shell;
#[cfg(feature = "lang-sql")]
pub mod sql;
#[cfg(feature = "lang-swift")]
pub mod swift;
#[cfg(feature = "lang-typescript")]
pub mod typescript;
#[cfg(feature = "lang-zig")]
pub mod zig;

pub use crate::relations::ParsedEdge;
pub use engine::Grammar;
pub use model::{
    content_hash, looks_binary, name_norm, search_text_for, ExtractError, ParsedFile, ParsedSymbol,
};
pub use registry::GRAMMARS;

/// Files bigger than this are skipped with a warning (D2): a source that big is
/// generated, and parsing it costs more than it answers. The census of the book
/// publishes that skip as `too_large` (f28, D3), with this ceiling in `limits`.
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
        #[cfg(feature = "lang-javascript")]
        {
            expected.push(("src/main.js", "javascript"));
            expected.push(("src/main.jsx", "javascript"));
            expected.push(("src/main.mjs", "javascript"));
        }
        #[cfg(feature = "lang-c")]
        {
            expected.push(("src/main.c", "c"));
            expected.push(("src/main.h", "c"));
        }
        #[cfg(feature = "lang-cpp")]
        {
            expected.push(("src/main.cpp", "cpp"));
            expected.push(("src/main.hpp", "cpp"));
        }
        #[cfg(feature = "lang-csharp")]
        expected.push(("src/main.cs", "csharp"));
        #[cfg(feature = "lang-php")]
        expected.push(("src/main.php", "php"));
        #[cfg(feature = "lang-ruby")]
        expected.push(("src/main.rb", "ruby"));
        #[cfg(feature = "lang-sql")]
        expected.push(("src/main.sql", "sql"));
        #[cfg(feature = "lang-shell")]
        {
            expected.push(("src/main.sh", "shell"));
            expected.push(("src/main.bash", "shell"));
        }
        #[cfg(feature = "lang-swift")]
        expected.push(("src/main.swift", "swift"));
        #[cfg(feature = "lang-kotlin")]
        {
            expected.push(("src/main.kt", "kotlin"));
            expected.push(("src/main.kts", "kotlin"));
        }
        #[cfg(feature = "lang-dart")]
        expected.push(("src/main.dart", "dart"));
        #[cfg(feature = "lang-scala")]
        {
            expected.push(("src/main.scala", "scala"));
            expected.push(("src/main.sc", "scala"));
        }
        #[cfg(feature = "lang-r")]
        {
            expected.push(("src/main.r", "r"));
            expected.push(("src/main.R", "r"));
        }
        #[cfg(feature = "lang-lua")]
        expected.push(("src/main.lua", "lua"));
        #[cfg(feature = "lang-zig")]
        expected.push(("src/main.zig", "zig"));

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
                cfg!(feature = "lang-javascript"),
                cfg!(feature = "lang-c"),
                cfg!(feature = "lang-cpp"),
                cfg!(feature = "lang-csharp"),
                cfg!(feature = "lang-php"),
                cfg!(feature = "lang-ruby"),
                cfg!(feature = "lang-sql"),
                cfg!(feature = "lang-shell"),
                cfg!(feature = "lang-swift"),
                cfg!(feature = "lang-kotlin"),
                cfg!(feature = "lang-dart"),
                cfg!(feature = "lang-scala"),
                cfg!(feature = "lang-r"),
                cfg!(feature = "lang-lua"),
                cfg!(feature = "lang-zig"),
            ]
            .iter()
            .filter(|enabled| **enabled)
            .count(),
            "one grammar per enabled language"
        );
        assert_eq!(language_for(Path::new("src/notes.txt")), None);
        // The archetype of a language no build ships is no language of the
        // approved list (L6): all twenty are grammars of the default build now.
        assert!(grammar("julia").is_none());
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
