//! The bridge from the SQL detail to an `er` document (S5, S6 of f12).
//!
//! The book reads a schema as **detail** — a table with its columns, carried on
//! the chapter beside its symbols ([`crate::lang::model::ParsedTable`]) — and
//! stops there: a column is not a declaration of the book. This module is the
//! one place that turns that detail into a **diagram**: the `er` document of
//! `gx-kinds` (`version: 1, kind: "er"`), which is the single wire shape the
//! engine door already accepts (`gx_core::engine::render_json`) — the bridge
//! adds no message and no contract field (S6).
//!
//! **Why the bridge lives here and not in `gx-kinds` or in a caller.** The
//! dependency direction of the workspace is `gx-core` ← `gx-kinds` ←
//! `gx-mcp`/`gx-app`, and `gx-index` is a leaf that depends on no crate of
//! Graphium at all: the book is not allowed to know about diagrams, so it
//! cannot name `gx_kinds::er::ErInput`. In the other direction, a bridge inside
//! `gx-kinds` would have to know the shape of the book's detail, which is
//! `gx-index`'s. So the bridge writes **the JSON** that `ErInput` deserializes
//! — the shape of `er.rs:40-90`, `deny_unknown_fields` included — with the
//! `serde_json` the book already depends on, and the door's own kind parses,
//! validates and translates it exactly as it does an author's document. The SQL
//! chapter knows the DDL, `er.rs` knows rows and edges, and neither learns
//! about the other.
//!
//! **The mapping (the er contract is law, `er.rs:245-260`).**
//!
//! - a table with columns is a `tables[]` entry whose `columns` are its rows, in
//!   the DDL's order;
//! - `key` is `pk` for a column the DDL marks as a key (inline or at table
//!   level), `fk` for a column the DDL declares as a foreign key — an inline
//!   `REFERENCES` or a member of a table-level `FOREIGN KEY` — and `pk_fk` for
//!   a column that is both; that is the vocabulary `key_of` maps to
//!   `RowKey::Pk`/`RowKey::Fk`/`RowKey::PkFk` (`er.rs:100-110`);
//! - `nullable` is the negation of `NOT NULL`, and a key column is never
//!   nullable because SQL says so: the detail keeps the declaration the DDL
//!   writes (S4) and the diagram shows the fact of the schema;
//! - `type` is the DDL's type when it writes one; the author path renders it as
//!   the row's note (`er.rs:296-305`);
//! - a relation is `from` the **referenced table's primary-key column** to `to`
//!   the **referencing column**, which is where the er edge anchors
//!   (`edge.from_row`/`edge.to_row`, `er.rs:311-318`) and what its validation
//!   demands: `from` must be a key of its table and `to` must be a foreign key
//!   of its own. `cardinality` is left out on purpose: `er.rs` derives the
//!   crow's foot from the foreign key's nullability (`er.rs:319-330`), so
//!   writing it would repeat what the document already says.
//!
//! **The surface, and the last mile it does not build (S6).**
//!
//! [`chapter_document`] is the whole of the surface: the path of a `.sql` file
//! and its text in, the document the door already renders out. It is the
//! chapter's own parse plus [`document_json`], so a door that already renders
//! `er` documents (`gx_core::engine::render_json`, and `render_diagram` over
//! it) needs nothing new: the document *is* the file. No new wire message, no
//! new MCP tool, no CLI flag and no `CONTRACT.md` line — the reader that hands
//! the document to the door is the one every `er` diagram already uses.
//!
//! What is deliberately **not** here is the agent-facing name of that call: a
//! tool that answers the document (a new name, or a new key on the answer of
//! `code_structure`) would be a change of the tool contract the agents are
//! written against, and this feature's S6 asks for none.
//!
//! **What it does not draw, and why.**
//!
//! - a foreign key whose `from` would not be a primary key — a target that is a
//!   unique column instead (`REFERENCES store (name)`), or a table the detail
//!   does not carry — is skipped: the er contract has no way to say it and a
//!   document that says it is rejected. The column keeps its `fk` row, which is
//!   what the DDL declares;
//! - a **composite** key is not a relation of the `er` contract, which keys a
//!   relation on one column: a table whose primary key is more than one column
//!   offers no single column to anchor `from`, so a foreign key that points at
//!   it is skipped. Its columns are still `pk` rows — the DDL marks them as the
//!   key — and the loss is documented here and in `sql.rs`;
//! - a composite foreign key (`FOREIGN KEY (a, b) REFERENCES …`) is skipped for
//!   the same reason: the contract is column to column;
//! - a *named* `CONSTRAINT fk FOREIGN KEY (…) REFERENCES …` keeps its key: the
//!   pinned grammar drops the name and the `FOREIGN` keyword into an error node
//!   and still parses the tail as the unnamed key, which the detail reads — the
//!   pair `(store_id) REFERENCES store (id)` below is drawn whether the DDL
//!   named the constraint or not. What the diagram cannot keep is the **name**,
//!   because the grammar never gives it (documented in `sql.rs`);
//! - a table with no columns (`CREATE TABLE … AS SELECT` writes none) is
//!   skipped instead of drawn as an empty compartment, and a detail with no
//!   table with rows produces no document at all.

use serde_json::{Map, Value};

use crate::lang::model::{ParsedColumn, ParsedTable};

/// The version of the `er` contract the document is written in.
pub const VERSION: u32 = 1;

/// The kind of the document: `er`.
pub const KIND: &str = "er";

/// The `er` document of a SQL detail, when the detail has a table with rows to
/// draw.
///
/// `None` is the answer when nothing can be drawn: the detail is empty or every
/// table it carries writes no column list (`CREATE TABLE … AS SELECT`), which
/// is not a diagram of an empty schema but the absence of one.
pub fn document(tables: &[ParsedTable]) -> Option<Value> {
    let tables = drawable(tables);
    if tables.is_empty() {
        return None;
    }

    let nodes: Vec<Value> = tables.iter().map(|table| node(table)).collect();
    let mut relations: Vec<Value> = Vec::new();
    let mut drawn: Vec<(String, String, String, String)> = Vec::new();
    for table in &tables {
        // The inline `REFERENCES` of a column and the table-level `FOREIGN
        // KEY (…) REFERENCES …` are the same relation of the schema; the er
        // contract draws both as one edge.
        for column in &table.columns {
            let Some(reference) = &column.references else {
                continue;
            };
            let Some(from) = anchor(&tables, &reference.table, &reference.columns) else {
                continue;
            };
            push_relation(
                &mut relations,
                &mut drawn,
                (&reference.table, from),
                (&table.name, &column.name),
            );
        }
        for key in &table.foreign_keys {
            // The contract is column to column: a composite foreign key has no
            // way to be said and is skipped whole.
            let [column] = key.columns.as_slice() else {
                continue;
            };
            // `to` must be a row of its table for the edge to anchor to it.
            if !table
                .columns
                .iter()
                .any(|candidate| candidate.name == *column)
            {
                continue;
            }
            let Some(from) = anchor(&tables, &key.table, &key.target_columns) else {
                continue;
            };
            push_relation(
                &mut relations,
                &mut drawn,
                (&key.table, from),
                (&table.name, column),
            );
        }
    }

    let mut document = Map::new();
    document.insert("version".to_string(), Value::from(VERSION));
    document.insert("kind".to_string(), Value::String(KIND.to_string()));
    document.insert("tables".to_string(), Value::Array(nodes));
    document.insert("relations".to_string(), Value::Array(relations));
    Some(Value::Object(document))
}

/// The document as the text the engine door reads (`render_json`), when the
/// detail has a table with rows to draw.
pub fn document_json(tables: &[ParsedTable]) -> Option<String> {
    document(tables).map(|document| document.to_string())
}

/// The `er` document of one `.sql` file: «this `.sql` file» in, the document
/// the engine door renders out (S6 of f12).
///
/// The call is the chapter's own parse ([`crate::lang::parse`]) plus
/// [`document_json`] and nothing else, so the document it answers is the one a
/// caller would build by hand, never a second mapping that could drift from
/// it. `path` is the chapter's path relative to the root of the worktree, the
/// same path the book stores; `source` is the text of the file as it is now
/// (the detail is read live, because the schema keeps no row for a column).
///
/// It refuses by answering `None`, and it needs no language check to do it: a
/// chapter carries the table detail only when its grammar reads one
/// ([`crate::lang::engine::Detail`]), so a file of another language, a DDL with
/// no column list and a path no grammar of the book reads all produce no
/// document — never an empty one.
pub fn chapter_document(path: &str, source: &str) -> Option<String> {
    // The modification time is the book's business (it stamps the `file` row),
    // never the document's: the caller has the path and the text, and that is
    // all this mapping reads.
    let parsed = crate::lang::parse(path, source.as_bytes(), 0).ok()?;
    document_json(parsed.tables.as_deref()?)
}

/// The tables of the detail the document draws: the ones with rows, once each.
///
/// A table with no columns is not a compartment with no rows but a table the
/// detail cannot draw, and the er contract's table names are unique, so a name
/// the document already has is not drawn twice (the door would reject it).
fn drawable(tables: &[ParsedTable]) -> Vec<&ParsedTable> {
    let mut kept: Vec<&ParsedTable> = Vec::new();
    for table in tables {
        if table.columns.is_empty() {
            continue;
        }
        if kept.iter().any(|candidate| candidate.name == table.name) {
            continue;
        }
        kept.push(table);
    }
    kept
}

/// One table of the document: its name and its rows.
fn node(table: &ParsedTable) -> Value {
    let columns: Vec<Value> = table
        .columns
        .iter()
        .map(|column| row(column, is_foreign_key(column, table)))
        .collect();
    let mut node = Map::new();
    node.insert("name".to_string(), Value::String(table.name.clone()));
    node.insert("columns".to_string(), Value::Array(columns));
    Value::Object(node)
}

/// One row of a table: the column as the er contract writes it.
fn row(column: &ParsedColumn, foreign_key: bool) -> Value {
    let mut row = Map::new();
    row.insert("name".to_string(), Value::String(column.name.clone()));
    // The type is the row's note when the door translates it
    // (`er.rs:296-305`): a DDL that writes no type adds nothing.
    if !column.type_name.is_empty() {
        row.insert("type".to_string(), Value::String(column.type_name.clone()));
    }
    if let Some(key) = key_of(column, foreign_key) {
        row.insert("key".to_string(), Value::String(key.to_string()));
    }
    // SQL's own rule: a `PRIMARY KEY` implies `NOT NULL`, so a key column is
    // never drawn nullable even when the DDL only writes the key.
    row.insert(
        "nullable".to_string(),
        Value::Bool(!column.not_null && !column.primary_key),
    );
    Value::Object(row)
}

/// The role of the column in the keys, in the vocabulary `er.rs`'s `key_of`
/// reads (`pk`, `fk`, `pk_fk`).
fn key_of(column: &ParsedColumn, foreign_key: bool) -> Option<&'static str> {
    match (column.primary_key, foreign_key) {
        (true, true) => Some("pk_fk"),
        (true, false) => Some("pk"),
        (false, true) => Some("fk"),
        (false, false) => None,
    }
}

/// Whether the DDL declares the column as a foreign key: an inline
/// `REFERENCES`, or a member of one of the table's `FOREIGN KEY (…)`s.
fn is_foreign_key(column: &ParsedColumn, table: &ParsedTable) -> bool {
    column.references.is_some()
        || table
            .foreign_keys
            .iter()
            .any(|key| key.columns.contains(&column.name))
}

/// The column a relation anchors `from`: the single primary key of the
/// referenced table, whether the DDL names it or leaves it out.
///
/// The er contract demands a key on the `from` side (`er.rs:245-260`) and keys
/// the relation on one column, so a table with a **composite** key — or with no
/// key at all — anchors nothing and the relation is not drawn.
fn anchor<'a>(tables: &[&'a ParsedTable], table: &str, target: &[String]) -> Option<&'a str> {
    let key = primary_key(tables, table)?;
    match target {
        [] => Some(key),
        [column] if column == key => Some(key),
        _ => None,
    }
}

/// The primary-key column of a table of the document, when it has exactly one.
fn primary_key<'a>(tables: &[&'a ParsedTable], name: &str) -> Option<&'a str> {
    let table = tables.iter().find(|candidate| candidate.name == name)?;
    let mut keys = table.columns.iter().filter(|column| column.primary_key);
    let key = keys.next()?;
    if keys.next().is_some() {
        return None;
    }
    Some(key.name.as_str())
}

/// Records one relation of the document, once.
fn push_relation(
    relations: &mut Vec<Value>,
    drawn: &mut Vec<(String, String, String, String)>,
    from: (&str, &str),
    to: (&str, &str),
) {
    let relation = (
        from.0.to_string(),
        from.1.to_string(),
        to.0.to_string(),
        to.1.to_string(),
    );
    if drawn.contains(&relation) {
        return;
    }
    drawn.push(relation);

    let mut sides = Map::new();
    sides.insert("from".to_string(), side(from.0, from.1));
    sides.insert("to".to_string(), side(to.0, to.1));
    relations.push(Value::Object(sides));
}

/// One side of a relation: the table and the column.
fn side(table: &str, column: &str) -> Value {
    let mut side = Map::new();
    side.insert("table".to_string(), Value::String(table.to_string()));
    side.insert("column".to_string(), Value::String(column.to_string()));
    Value::Object(side)
}
