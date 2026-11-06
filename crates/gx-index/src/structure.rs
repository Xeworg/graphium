//! `code_structure`: the containment tree of one chapter (S5, S7).
//!
//! The book keeps the declarations of a file with their spans and their
//! `parent_id`, and this module answers them ordered by span: the tool rebuilds
//! the tree from the parent links instead of storing a second shape of the same
//! file (ideas §10, «Cómo se sirven las tres tools»).

use rusqlite::{Connection, OptionalExtension};

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
    pub symbols: Vec<StructureSymbol>,
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
    use super::file_structure;
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
}
