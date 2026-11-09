//! D3: the honesty of the relations, measured per language.
//!
//! The relations of the book are **syntactic** (`calls`, `imports`,
//! `inherits`, `uses`): a target is a name, not a type. A name resolves only
//! while the book holds at most [`MAX_NAME_FANOUT`] declarations of it — the
//! very rule the walk of `code_impact` applies — so the rate of resolution is a
//! property of the language and of the tree, never a promise of the book.
//!
//! The feature doc (D3) asks the book to **publish** that rate per language.
//! A published number that cannot be reproduced is a rumour, so the
//! measurement lives here: [`resolution_by_language`] answers it from the same
//! tables the tools read, and the test that guards the numbers of `docs/PRD.md`
//! runs on the fixtures of `tests/languages.rs`.
//!
//! **The runtime rule (T6).** A relation whose target is a name the language's
//! runtime or standard library provides (`str`, `len`, `path` in Python;
//! `fmt`, `string` in Go; `String`, `List` in Java — the list is the
//! [`crate::lang::Grammar::builtins`] data of each registry entry) can never
//! resolve: the book indexes a worktree, and no worktree declares the runtime.
//! Counting those targets measures what the book cannot be, so the rate leaves
//! them out — out of the numerator and out of the denominator — **only while no
//! chapter of the book declares that name**: a book that spells `def len`
//! measures it like any other declaration, so the rule can never hide a
//! resolution. The book keeps every edge either way; the rule is the metric's,
//! never the graph's.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::Connection;

use crate::impact::MAX_NAME_FANOUT;
use crate::lang::{name_norm, GRAMMARS};

/// What one language of the book resolved: its chapters, its declarations, its
/// relations and how many of those relations reach a declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageResolution {
    /// The language as the table `file` spells it.
    pub lang: String,
    /// How many chapters of this language the book holds.
    pub files: i64,
    /// How many declarations they wrote.
    pub symbols: i64,
    /// How many syntactic relations they declared.
    pub edges: i64,
    /// How many of those relations resolve to a declaration (D3).
    pub resolved: i64,
}

impl LanguageResolution {
    /// The published rate: `resolved / edges`, in percent, `0` for a language
    /// with no relations (a rate over an empty set is not a lie, it is NaN).
    pub fn percent(&self) -> f64 {
        if self.edges == 0 {
            return 0.0;
        }
        (self.resolved as f64) * 100.0 / (self.edges as f64)
    }
}

/// The resolution report of the book, one row per language, ordered by name.
///
/// The definers of a name are counted over the **whole book**: two chapters of
/// the worktree are the same namespace, whichever language declared them, which
/// is also how `code_impact` resolves a cross-language call.
///
/// A relation that targets a name of its language's runtime — and that no
/// chapter of the book declares — is not measured at all (T6).
pub fn resolution_by_language(
    connection: &Connection,
) -> rusqlite::Result<Vec<LanguageResolution>> {
    let definers = definers(connection)?;
    let runtime = runtime_names();
    let mut report = BTreeMap::new();

    let mut statement = connection.prepare(
        "SELECT f.lang, COUNT(DISTINCT f.id), COUNT(s.id)
         FROM file f LEFT JOIN symbol s ON s.file_id = f.id
         GROUP BY f.lang",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(LanguageResolution {
            lang: row.get(0)?,
            files: row.get(1)?,
            symbols: row.get(2)?,
            edges: 0,
            resolved: 0,
        })
    })?;
    for row in rows {
        let row = row?;
        report.insert(row.lang.clone(), row);
    }

    let mut statement = connection.prepare(
        "SELECT f.lang, e.dst_name
         FROM edge e
         JOIN symbol s ON s.id = e.src_id
         JOIN file f ON f.id = s.file_id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (lang, dst_name) = row?;
        let Some(row) = report.get_mut(&lang) else {
            continue;
        };
        let norm = name_norm(&dst_name);
        // The runtime rule (T6): `str` is not coverage the book can win, and a
        // name a chapter of the book declares is measured as usual.
        if !definers.contains_key(&norm) && provides_the_runtime(&runtime, &lang, &norm) {
            continue;
        }
        row.edges += 1;
        if definers
            .get(&norm)
            .is_some_and(|count| (1..=MAX_NAME_FANOUT as i64).contains(count))
        {
            row.resolved += 1;
        }
    }

    Ok(report.into_values().collect())
}

/// The runtime names of every grammar of the build, one set per language: the
/// data of [`crate::lang::Grammar::builtins`], ready to be asked once per
/// relation.
fn runtime_names() -> BTreeMap<&'static str, BTreeSet<&'static str>> {
    GRAMMARS
        .iter()
        .map(|grammar| {
            (
                grammar.lang,
                grammar.builtins.iter().copied().collect::<BTreeSet<_>>(),
            )
        })
        .collect()
}

/// Whether the language's runtime provides the folded name of a target.
fn provides_the_runtime(
    runtime: &BTreeMap<&'static str, BTreeSet<&'static str>>,
    lang: &str,
    norm: &str,
) -> bool {
    runtime.get(lang).is_some_and(|names| names.contains(norm))
}

/// How many declarations of the book carry each folded name.
fn definers(connection: &Connection) -> rusqlite::Result<BTreeMap<String, i64>> {
    let mut statement =
        connection.prepare("SELECT name_norm, COUNT(*) FROM symbol GROUP BY name_norm")?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut definers = BTreeMap::new();
    for row in rows {
        let (norm, count) = row?;
        definers.insert(norm, count);
    }
    Ok(definers)
}

#[cfg(test)]
mod tests {
    use super::LanguageResolution;

    /// D3: a language with no relations answers `0 %`, never NaN, and the rate
    /// keeps one decimal of meaning.
    #[test]
    fn the_rate_of_a_language_is_the_share_of_its_resolved_relations() {
        let row = LanguageResolution {
            lang: "python".to_string(),
            files: 1,
            symbols: 4,
            edges: 10,
            resolved: 3,
        };
        assert_eq!(row.percent(), 30.0);
        assert_eq!(
            LanguageResolution {
                edges: 0,
                resolved: 0,
                ..row
            }
            .percent(),
            0.0
        );
    }
}
