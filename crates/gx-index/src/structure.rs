//! `code_structure`: the containment tree of one chapter (S5, S7).
//!
//! The book keeps the declarations of a file with their spans and their
//! `parent_id`, and this module answers them ordered by span: the tool rebuilds
//! the tree from the parent links instead of storing a second shape of the same
//! file (ideas §10, «Cómo se sirven las tres tools»).
//!
//! The answer is bounded (T5): the default is the **outline** — the rows whose
//! parent is `NULL` — and it carries the count of the whole chapter and the
//! cursor that reads the hidden children. `after_line` + `limit` walk the same
//! span-ordered list by range, so reading the whole tree is a sequence of
//! monotone pages and never a shape flag.

use std::collections::BTreeSet;

use rusqlite::{Connection, OptionalExtension};

use crate::cut::{CutReason, Truncation};

/// How many rows one answer shows when the caller does not say.
pub const DEFAULT_LIMIT: usize = 200;

/// The hard ceiling of `limit`: an outline is a summary, not the file.
pub const MAX_LIMIT: usize = 200;

/// One declaration of a chapter, with the parent that contains it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructureSymbol {
    pub id: i64,
    /// The id of the containing declaration, when it has one.
    pub parent: Option<i64>,
    pub kind: String,
    pub name: String,
    pub qname: String,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

/// The chapter of one file: its identity (S7) and its declarations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStructure {
    pub file: String,
    pub lang: String,
    pub content_hash: String,
    /// Unix epoch milliseconds of the chapter index (S7: ISO-8601 at the edge).
    pub indexed_at: i64,
    /// Every declaration of the chapter, ordered by span.
    pub symbols: Vec<StructureSymbol>,
}

/// What the caller asks of a chapter (T5): a range of the span-ordered list,
/// a filter of kinds, or neither — then the answer is the outline.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StructureRequest {
    /// The line to start after, strictly: the cursor of the answer.
    /// `Some(0)` asks for the whole list from its first row.
    pub after_line: Option<i64>,
    /// How many rows to show: [`DEFAULT_LIMIT`] when absent, clamped to
    /// [`MAX_LIMIT`].
    pub limit: Option<usize>,
    /// The declaration kinds to answer; empty answers the whole vocabulary.
    pub kinds: Vec<String>,
    /// T4/C3: the character envelope of the answer (L7). `None` is the ladder
    /// of the book; it never changes which rows the range asks for.
    pub max_chars: Option<usize>,
}

impl StructureRequest {
    /// The declaration kinds of the book: the vocabulary of the schema.
    pub const KINDS: [&'static str; 9] = [
        "fn", "method", "type", "class", "trait", "impl", "module", "const", "static",
    ];

    /// Whether the book declares a kind with this name.
    pub fn kind_is_known(kind: &str) -> bool {
        Self::KINDS.contains(&kind)
    }

    /// The effective limit of the answer.
    pub fn limit(&self) -> usize {
        self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
    }

    /// Whether a declaration kind survives the filter.
    fn accepts(&self, kind: &str) -> bool {
        self.kinds.is_empty() || self.kinds.iter().any(|wanted| wanted == kind)
    }
}

/// What the outline hid, and the exact cursor that reads it (T5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collapsed {
    /// The rows of this question the answer did not show.
    pub hidden_symbols: usize,
    /// How many of the shown declarations contain others.
    pub declarations_with_children: usize,
    /// The line of the first hidden declaration: the `after_line` of the next
    /// call.
    pub after_line: u32,
}

/// The rows one answer shows, with the count of the whole chapter and the
/// notice of what was left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructureAnswer {
    pub symbols: Vec<StructureSymbol>,
    /// Every declaration of the chapter, filter included: a filter is never
    /// read as a smaller file.
    pub total: usize,
    pub collapsed: Option<Collapsed>,
    pub truncated: Option<Truncation>,
}

/// The structure of a file, or `None` when the path is not a chapter.
pub fn file_structure(
    connection: &Connection,
    path: &str,
) -> rusqlite::Result<Option<FileStructure>> {
    let file: Option<(i64, String, String, i64)> = connection
        .query_row(
            "SELECT id, lang, content_hash, indexed_at FROM file WHERE path = ?1",
            [path],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let Some((file_id, lang, content_hash, indexed_at)) = file else {
        return Ok(None);
    };
    Ok(Some(FileStructure {
        file: path.to_string(),
        lang,
        content_hash,
        indexed_at,
        symbols: symbols_of(connection, file_id)?,
    }))
}

/// The structure of the chapter that declares `symbol_id`, or `None` when the
/// symbol is not in the book.
pub fn structure_of_symbol(
    connection: &Connection,
    symbol_id: i64,
) -> rusqlite::Result<Option<FileStructure>> {
    let path: Option<String> = connection
        .query_row(
            "SELECT f.path FROM symbol s JOIN file f ON f.id = s.file_id WHERE s.id = ?1",
            [symbol_id],
            |row| row.get(0),
        )
        .optional()?;
    match path {
        Some(path) => file_structure(connection, &path),
        None => Ok(None),
    }
}

impl FileStructure {
    /// The rows of one answer (T5): the outline by default, or the range
    /// `request` asks for, always with the notice of what it left out.
    pub fn answer(&self, request: &StructureRequest) -> StructureAnswer {
        let total = self.symbols.len();
        let filtered: Vec<&StructureSymbol> = self
            .symbols
            .iter()
            .filter(|row| request.accepts(&row.kind))
            .collect();
        let limit = request.limit();

        let Some(after_line) = request.after_line else {
            return self.outline(&filtered, total, limit);
        };

        let first = filtered
            .iter()
            .position(|row| i64::from(row.line) > after_line)
            .unwrap_or(filtered.len());
        let end = first.saturating_add(limit).min(filtered.len());
        let symbols: Vec<StructureSymbol> = filtered[first..end]
            .iter()
            .map(|row| (*row).clone())
            .collect();
        // A range answers rows, never a tree: the rows of the chapter are the
        // list, and the caller asks for the slice it needs.
        let truncated = (end < filtered.len())
            .then(|| Truncation::new(CutReason::Nodes, symbols.len(), Some(filtered.len())));
        StructureAnswer {
            symbols,
            total,
            collapsed: None,
            truncated,
        }
    }

    /// The outline: the declarations whose parent is `NULL`, and the cursor
    /// that reads the rest of the span-ordered list.
    fn outline(
        &self,
        filtered: &[&StructureSymbol],
        total: usize,
        limit: usize,
    ) -> StructureAnswer {
        let outline: Vec<&StructureSymbol> = filtered
            .iter()
            .copied()
            .filter(|row| row.parent.is_none())
            .collect();
        let cut = outline.len() > limit;
        let shown: Vec<&StructureSymbol> = outline.iter().copied().take(limit).collect();
        let hidden = filtered.len().saturating_sub(shown.len());
        // A top-level row left out by the limit is a range cut, not a
        // collapse: the follow-up call asks for the next slice, not for the
        // children of what it already read.
        let collapsed = if cut || hidden == 0 {
            None
        } else {
            let shown_ids: BTreeSet<i64> = shown.iter().map(|row| row.id).collect();
            let first_hidden = filtered.iter().find(|row| !shown_ids.contains(&row.id));
            Some(Collapsed {
                hidden_symbols: hidden,
                declarations_with_children: shown
                    .iter()
                    .filter(|row| self.has_children(row.id))
                    .count(),
                after_line: first_hidden.map(|row| row.line).unwrap_or_default(),
            })
        };
        let truncated = match &collapsed {
            Some(_) => Some(Truncation::new(
                CutReason::Collapsed,
                shown.len(),
                Some(filtered.len()),
            )),
            None if cut => Some(Truncation::new(
                CutReason::Nodes,
                shown.len(),
                Some(filtered.len()),
            )),
            None => None,
        };
        StructureAnswer {
            symbols: shown.into_iter().cloned().collect(),
            total,
            collapsed,
            truncated,
        }
    }

    /// Whether a declaration of the chapter contains at least one other.
    fn has_children(&self, id: i64) -> bool {
        self.symbols.iter().any(|row| row.parent == Some(id))
    }
}

/// The declarations of one chapter, ordered by span: parents before the
/// children they contain.
fn symbols_of(connection: &Connection, file_id: i64) -> rusqlite::Result<Vec<StructureSymbol>> {
    let mut statement = connection.prepare(
        "SELECT id, parent_id, kind, name, qname, start_line, start_col, end_line, end_col
         FROM symbol WHERE file_id = ?1 ORDER BY start_byte, id",
    )?;
    let symbols = statement
        .query_map([file_id], |row| {
            Ok(StructureSymbol {
                id: row.get(0)?,
                parent: row.get(1)?,
                kind: row.get(2)?,
                name: row.get(3)?,
                qname: row.get(4)?,
                line: row.get::<_, i64>(5)? as u32,
                column: row.get::<_, i64>(6)? as u32,
                end_line: row.get::<_, i64>(7)? as u32,
                end_column: row.get::<_, i64>(8)? as u32,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(symbols)
}

#[cfg(test)]
mod tests {
    use super::{file_structure, FileStructure, StructureRequest, StructureSymbol};
    use crate::cut::CutReason;
    use rusqlite::Connection;

    /// S5: a path that the book does not have is `None`, never an empty
    /// chapter that reads as «the file has no declarations».
    #[test]
    fn an_unknown_path_is_not_an_empty_chapter() {
        let connection = Connection::open_in_memory().expect("in memory");
        connection
            .execute_batch(crate::schema::SCHEMA_SQL)
            .expect("the schema");
        assert_eq!(
            file_structure(&connection, "src/nope.rs").expect("the read"),
            None
        );
    }

    /// The chapter the answer tests read: two top-level items that declare
    /// children, two that do not.
    ///
    /// ```text
    /// 1  type Item;
    /// 2  impl Item {
    /// 3      method a;
    /// 4      method b;
    /// 5  }
    /// 6  fn top;
    /// 7  module inner {
    /// 8      fn c;
    /// 9  }
    /// ```
    fn nested() -> FileStructure {
        let row = |id: i64, parent: Option<i64>, kind: &str, line: u32| StructureSymbol {
            id,
            parent,
            kind: kind.to_string(),
            name: format!("n{id}"),
            qname: format!("chapter::n{id}"),
            line,
            column: 1,
            end_line: line,
            end_column: 1,
        };
        FileStructure {
            file: "src/nested.rs".to_string(),
            lang: "rust".to_string(),
            content_hash: "a".repeat(64),
            indexed_at: 0,
            symbols: vec![
                row(1, None, "type", 1),
                row(2, None, "impl", 2),
                row(3, Some(2), "method", 3),
                row(4, Some(2), "method", 4),
                row(5, None, "fn", 6),
                row(6, None, "module", 7),
                row(7, Some(6), "fn", 8),
            ],
        }
    }

    /// T5/S7: the default answer is the outline — the declarations whose
    /// parent is `NULL` — and it says how many children it hid and the exact
    /// cursor that reads them.
    #[test]
    fn the_default_answer_is_the_outline_with_the_collapsed_block() {
        let answer = nested().answer(&StructureRequest::default());
        let ids: Vec<i64> = answer.symbols.iter().map(|row| row.id).collect();
        assert_eq!(ids, vec![1, 2, 5, 6], "the outline keeps the top level");
        assert_eq!(answer.total, 7, "total counts the whole chapter");
        let collapsed = answer.collapsed.expect("three children are hidden");
        assert_eq!(collapsed.hidden_symbols, 3);
        assert_eq!(
            collapsed.declarations_with_children, 2,
            "the `impl` and the `module` of the outline"
        );
        assert_eq!(collapsed.after_line, 3, "the first hidden child's line");
        let truncated = answer.truncated.expect("the outline cut something");
        assert_eq!(truncated.reason, CutReason::Collapsed);
        assert_eq!(truncated.shown, 4);
        assert_eq!(truncated.total, Some(7));
    }

    /// T5/S7: `after_line`/`limit` are a range over the span-ordered list, and
    /// the pages concatenated are that list, in order and without repeats.
    #[test]
    fn after_line_and_limit_slice_the_span_ordered_list() {
        let mut seen: Vec<u32> = Vec::new();
        let mut after_line = Some(0);
        loop {
            let answer = nested().answer(&StructureRequest {
                after_line,
                limit: Some(3),
                kinds: Vec::new(),
                max_chars: None,
            });
            assert!(answer.collapsed.is_none(), "a range is not an outline");
            seen.extend(answer.symbols.iter().map(|row| row.line));
            let Some(truncated) = answer.truncated else {
                break;
            };
            assert_eq!(truncated.reason, CutReason::Nodes);
            assert_eq!(truncated.shown, 3);
            assert_eq!(truncated.total, Some(7));
            after_line = answer.symbols.last().map(|row| i64::from(row.line));
        }
        assert_eq!(seen, vec![1, 2, 3, 4, 6, 7, 8], "the whole list, once");
    }

    /// T5: `limit` has a hard ceiling, so a chapter with thousands of rows is
    /// still a summary, and the cut says why.
    #[test]
    fn the_limit_is_capped_and_the_cut_is_a_nodes_cut() {
        let answer = nested().answer(&StructureRequest {
            after_line: Some(0),
            limit: Some(usize::MAX),
            kinds: Vec::new(),
            max_chars: None,
        });
        assert_eq!(answer.symbols.len(), 7, "the cap still answers the chapter");
        assert!(answer.truncated.is_none(), "nothing was left out");

        let answer = nested().answer(&StructureRequest {
            after_line: Some(0),
            limit: Some(2),
            kinds: Vec::new(),
            max_chars: None,
        });
        assert_eq!(answer.symbols.len(), 2);
        assert_eq!(
            answer.truncated.map(|truncated| truncated.reason),
            Some(CutReason::Nodes)
        );
    }

    /// T5: `kinds` filters the rows and `total` keeps counting the chapter, so
    /// the agent never mistakes a filter for a smaller file.
    #[test]
    fn the_kinds_filter_narrows_the_rows_and_not_the_total() {
        let answer = nested().answer(&StructureRequest {
            after_line: Some(0),
            kinds: vec!["method".to_string()],
            ..StructureRequest::default()
        });
        let ids: Vec<i64> = answer.symbols.iter().map(|row| row.id).collect();
        assert_eq!(ids, vec![3, 4]);
        assert_eq!(answer.total, 7, "the chapter is still seven declarations");

        let answer = nested().answer(&StructureRequest {
            kinds: vec!["type".to_string()],
            ..StructureRequest::default()
        });
        let ids: Vec<i64> = answer.symbols.iter().map(|row| row.id).collect();
        assert_eq!(ids, vec![1], "a type with no children hides nothing");
        assert_eq!(answer.total, 7);
        assert!(answer.collapsed.is_none());
        assert!(answer.truncated.is_none());
    }

    /// T5: the outline is bounded too: a chapter with more top-level rows than
    /// the limit answers the first page and a cursors, never the whole file.
    #[test]
    fn the_outline_itself_is_sliced_by_the_limit() {
        let answer = nested().answer(&StructureRequest {
            after_line: None,
            limit: Some(2),
            kinds: Vec::new(),
            max_chars: None,
        });
        let ids: Vec<i64> = answer.symbols.iter().map(|row| row.id).collect();
        assert_eq!(ids, vec![1, 2]);
        let truncated = answer.truncated.expect("the limit cut the outline");
        assert_eq!(truncated.reason, CutReason::Nodes);
        assert_eq!(truncated.shown, 2);
        assert_eq!(truncated.total, Some(7));
    }

    /// T5: a kind that the book does not declare is a vocabulary answer, not
    /// an empty chapter that reads as «nothing here».
    #[test]
    fn a_kind_the_book_does_not_declare_is_named_in_the_vocabulary() {
        assert!(StructureRequest::kind_is_known("method"));
        assert!(StructureRequest::kind_is_known("class"));
        assert!(!StructureRequest::kind_is_known("whatever"));
        assert_eq!(StructureRequest::KINDS.len(), 9);
    }
}
