//! `code_search`: FTS5 → ranked rows → one name or candidates (S7, ideas §10).
//!
//! The tokenizer is `unicode61` **without** `tokenchars '_'` (ideas §10.1,
//! delta 5): `_` is a separator, so snake_case is split for free and the
//! derived `search_text` covers CamelCase. The query is never a concatenated
//! phrase: every token of the user is an AND with an explicit column filter,
//! and every token is escaped before it reaches `MATCH`.

use rusqlite::Connection;

/// How many FTS rows are read before deciding whether the answer is unique.
pub const ROW_LIMIT: usize = 200;

/// How many candidates an ambiguous answer names (P3, condition 2).
pub const CANDIDATE_LIMIT: usize = 5;

/// The longest query the tool accepts.
pub const MAX_QUERY_CHARS: usize = 200;

/// The most tokens one query may carry.
pub const MAX_TOKENS: usize = 8;

/// What the caller asks the book.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRequest {
    /// The text the agent sent.
    pub query: String,
}

/// One place where the symbol is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolHit {
    /// The `symbol.id` of the book: what the structure and impact readers take.
    pub id: i64,
    /// Path relative to the root of the worktree.
    pub file: String,
    /// 1-based line of the declaration.
    pub line: u32,
    /// 1-based column of the declaration.
    pub column: u32,
    pub kind: String,
    pub name: String,
    pub qname: String,
    /// sha256 of the file at the moment it was indexed (S7).
    pub content_hash: String,
    /// Unix epoch milliseconds of the chapter index (S7).
    pub indexed_at: i64,
}

/// One name that the query fits, when more than one name fits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolCandidate {
    pub name: String,
    pub kind: String,
    pub qname: String,
    pub file: String,
    pub line: u32,
    /// How many declarations carry that name.
    pub occurrences: u32,
}

/// The three honest answers of a search: one name, several names, or nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchOutcome {
    /// Every ranked row belongs to the same name: the symbol is unambiguous.
    Hits(Vec<SymbolHit>),
    /// Several names fit: never a single confident answer (P3).
    Ambiguous(Vec<SymbolCandidate>),
    /// Nothing matched.
    Empty,
}

/// Builds the `MATCH` expression of a query, escaping every token.
///
/// Tokens that carry no alphanumeric character are dropped: a quoted FTS5
/// phrase with no token is a syntax error, and `AND`/`OR`/`NEAR` typed by the
/// agent are matched as words, never executed as operators.
pub fn match_expression(query: &str) -> String {
    let tokens: Vec<&str> = query
        .split_whitespace()
        .filter(|token| token.chars().any(|char| char.is_alphanumeric()))
        .take(MAX_TOKENS)
        .collect();
    tokens
        .iter()
        .map(|token| {
            format!(
                "{{name qname search_text}} : \"{}\"",
                token.replace('"', "\"\"")
            )
        })
        .collect::<Vec<_>>()
        .join(" AND ")
}

/// Runs the search against an open, read-only connection.
///
/// One row is an answer; more than one is an ambiguity, because the query did
/// not identify a symbol and a single confident answer would be a guess (P3,
/// condition 2). The candidates are one per qualified name, in rank order.
pub fn search(connection: &Connection, request: &SearchRequest) -> rusqlite::Result<SearchOutcome> {
    let expression = match_expression(&request.query);
    if expression.is_empty() {
        return Ok(SearchOutcome::Empty);
    }

    let mut statement = connection.prepare(
        "SELECT s.kind, s.name, s.name_norm, s.qname, s.start_line, s.start_col,
                f.path, f.content_hash, f.indexed_at, s.id
         FROM symbol_fts
         JOIN symbol s ON s.id = symbol_fts.rowid
         JOIN file f ON f.id = s.file_id
         WHERE symbol_fts MATCH ?1
         ORDER BY rank
         LIMIT ?2",
    )?;
    let rows = statement
        .query_map(rusqlite::params![expression, ROW_LIMIT as i64], |row| {
            Ok(RankedRow {
                hit: SymbolHit {
                    kind: row.get(0)?,
                    name: row.get(1)?,
                    qname: row.get(3)?,
                    line: row.get::<_, i64>(4)? as u32,
                    column: row.get::<_, i64>(5)? as u32,
                    file: row.get(6)?,
                    content_hash: row.get(7)?,
                    indexed_at: row.get(8)?,
                    id: row.get(9)?,
                },
                name_norm: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    match rows.len() {
        0 => Ok(SearchOutcome::Empty),
        1 => Ok(SearchOutcome::Hits(
            rows.into_iter().map(|row| row.hit).collect(),
        )),
        _ => {
            let mut candidates: Vec<SymbolCandidate> = Vec::new();
            for row in &rows {
                let occurrences = rows
                    .iter()
                    .filter(|other| other.name_norm == row.name_norm)
                    .count() as u32;
                match candidates
                    .iter_mut()
                    .find(|candidate| candidate.qname == row.hit.qname)
                {
                    Some(candidate) => candidate.occurrences = occurrences,
                    None => candidates.push(SymbolCandidate {
                        name: row.hit.name.clone(),
                        kind: row.hit.kind.clone(),
                        qname: row.hit.qname.clone(),
                        file: row.hit.file.clone(),
                        line: row.hit.line,
                        occurrences,
                    }),
                }
                if candidates.len() >= CANDIDATE_LIMIT {
                    break;
                }
            }
            Ok(SearchOutcome::Ambiguous(candidates))
        }
    }
}

/// One FTS row with the collation of its name.
struct RankedRow {
    hit: SymbolHit,
    name_norm: String,
}

#[cfg(test)]
mod tests {
    use super::{match_expression, MAX_TOKENS};

    /// Ideas §10.1, delta 5: the tokens are ANDed with a column filter and
    /// every token is quoted, so no input is an FTS5 operator.
    #[test]
    fn the_match_expression_ands_escaped_tokens_over_three_columns() {
        assert_eq!(
            match_expression("render with"),
            "{name qname search_text} : \"render\" AND {name qname search_text} : \"with\""
        );
        assert_eq!(
            match_expression("café \"crème\""),
            "{name qname search_text} : \"café\" AND {name qname search_text} : \"\"\"crème\"\"\""
        );
    }

    #[test]
    fn operators_and_punctuation_are_never_executed() {
        assert_eq!(
            match_expression("AND OR NOT NEAR"),
            "{name qname search_text} : \"AND\" AND {name qname search_text} : \"OR\" AND \
             {name qname search_text} : \"NOT\" AND {name qname search_text} : \"NEAR\""
        );
        assert_eq!(match_expression("::: ---"), "");
        assert_eq!(match_expression("   "), "");
    }

    #[test]
    fn the_number_of_tokens_is_bounded() {
        let query = (0..40)
            .map(|index| format!("token{index}"))
            .collect::<Vec<_>>()
            .join(" ");
        let expression = match_expression(&query);
        assert_eq!(expression.matches("AND").count(), MAX_TOKENS - 1);
    }
}
