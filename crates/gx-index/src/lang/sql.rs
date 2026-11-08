//! The SQL chapter of the book (T3 Batch 2; S2, S4).
//!
//! The chapter keeps the objects a schema declares: the tables, the views (a
//! materialized view is a view) and the types, as `type`; and the routines — a
//! function and a trigger — as `fn`. A column is not a declaration of the book
//! (a field of a class is not one either), and an index is not one either: it
//! has no body, no name the language refers to, and what matters to the book is
//! the table it reads.
//!
//! **What are the relations of SQL (a decision, not a default).** SQL has no
//! calls and no imports in the sense the other grammars read them: the tree of
//! a statement names objects, it does not name a call target or a module. The
//! one relation the tree spells cleanly is a **reference to another object of
//! the schema**, and the book reads it as `uses`:
//!
//! - the table a query reads: `FROM`/`JOIN` of a `SELECT` (so a `CREATE VIEW`
//!   uses the tables of its query);
//! - the table an index is built on: `CREATE INDEX … ON <table>`;
//! - the table a foreign key points to: `… REFERENCES <table>`.
//!
//! There is no `calls`, no `imports` and no `inherits`: a `CALL` of a
//! procedure, a `\i` of psql and a `CREATE TABLE … INHERITS` of PostgreSQL are
//! not part of the SQL this grammar reads, and inventing one would publish a
//! number D3 cannot stand behind. The doc entry of T3 Batch 2 states the same
//! decision.
//!
//! What is SQL's alone is here; the declarations the book keeps are the tables
//! of [`crate::lang::registry::SQL`] and the pipeline is
//! [`crate::lang::engine`] (S3, T2). Every declaration of SQL is a hook: a
//! `CREATE` statement has no `name` field, so the name is the first
//! `object_reference` of the statement and the hook reads it there.
//!
//! The grammar understands a foreign key written inline
//! (`store_id BIGINT REFERENCES store (id)`) and, at table level, as the
//! `constraint` node the next paragraph describes. A *named* constraint
//! (`CONSTRAINT fk FOREIGN KEY …`) is not in the grammar's rules: the name and
//! the `FOREIGN` keyword land in an error node and the `(…) REFERENCES …` tail
//! still parses as the unnamed key, so the book keeps no `uses` relation for
//! the constraint (the detail of f12 does read the key it declares — see the
//! limits below).
//!
//! **The detail of a table (S3, S4 of f12).** SQL is the one language of the
//! book whose schema hands the diagram a shape, and that shape is **not** a
//! declaration: a `CREATE TABLE` declares a table, and its columns are detail
//! carried beside the symbols — on [`crate::lang::model::ParsedFile::tables`] —
//! never in the `symbol` or `edge` tables of the schema (the CHECK at
//! `schema.rs:449` stays what it is, and no schema version moves). This module
//! reads that detail with [`tables`]; [`crate::lang::engine::Grammar`] carries
//! it without knowing which language asked for it.
//!
//! What the pinned grammar (`tree-sitter-sequel 0.3.11`) gives, and what it does
//! not:
//!
//! - a column is a `column_definition`: `name` and `type` are fields of the
//!   node and the type is whole (`VARCHAR(255)`); the constraints are children
//!   — `keyword_not` + `keyword_null` (a bare `keyword_null` is the explicit
//!   `NULL`), the inline `keyword_primary` + `keyword_key`, and an inline
//!   `keyword_references` + `object_reference` + the target's columns;
//! - the table-level constraints land as `constraint` children of
//!   `column_definitions`, **not** as the `_primary_key_constraint` and
//!   `_key_constraint` rules the grammar spells: a rule whose name starts with
//!   `_` is hidden and inlined, so the node kind to match is `constraint` and
//!   the keywords it carries are what tells its forms apart (`keyword_primary`
//!   is a primary key; a `keyword_key` with `keyword_references` is a foreign
//!   key). T2 builds on exactly this;
//! - a table-level `PRIMARY KEY (a, b)` arrives whole, so every column it names
//!   is marked as a key and a composite key is carried as what it is. Mapping a
//!   composite key onto a single-key document is a decision of the side that
//!   builds the diagram (T2), not of this chapter: [`er_bridge`] draws both of
//!   its columns as key rows and draws no relation that would point at the key
//!   alone, because the `er` contract keys a relation on one column;
//! - a *named* foreign key (`CONSTRAINT fk FOREIGN KEY …`) is not in the
//!   grammar's rules at this version (its `_constraint_literal` admits only
//!   `PRIMARY KEY` and `CHECK`): the name and the `FOREIGN` keyword land in an
//!   `ERROR` node, and the `(…) REFERENCES …` tail that follows still parses as
//!   the unnamed key constraint — so the name is lost and **the key is read**
//!   (measured on the pinned grammar; the fixtures write the inline, the
//!   unnamed table-level and the named forms). A named primary key
//!   (`CONSTRAINT pk PRIMARY KEY (…)`) parses whole, name field included, and
//!   the keyword check reads it like the unnamed one;
//! - the relations of the chapter do not change with this detail: its `uses`
//!   edges stay the three forms [`visit`] reads, and a table-level foreign key
//!   lives in [`crate::lang::model::ParsedTable::foreign_keys`] only. Its
//!   relation is drawn by whoever builds the diagram out of the detail;
//! - a `CREATE TABLE … AS SELECT` writes no column list, so it stays a `type`
//!   of the chapter and is not an entry of the detail: the detail is what the
//!   DDL writes between the parentheses.
//!
//! **The limits, in plain language (f12; what the owner should know).**
//!
//! - a named `CONSTRAINT <name> FOREIGN KEY (…) REFERENCES …` is not fully
//!   supported at this grammar version: the constraint's **name is not read**
//!   (the name and the `FOREIGN` keyword are an error node). The key it
//!   declares is another matter — the tail parses and the diagram draws it, so
//!   a named key and an unnamed one draw the same relation — and the chapter
//!   publishes no `uses` edge for either, because a table-level key is detail
//!   only;
//! - a **composite key** is carried as what it is, one mark per column
//!   (`PRIMARY KEY (a, b)` marks `a` and `b` as keys), and the mapping to the
//!   `er` contract is **single-key only**: a relation that would point at a
//!   composite key, or a foreign key of more than one column, is not drawn. The
//!   columns keep their `pk`/`fk` rows, so the diagram still says what the
//!   schema declares;
//! - a `CREATE TABLE … AS SELECT` **yields no columns**: it writes no column
//!   list, so the detail has nothing for it and it is not drawn in the diagram.
//!
//! **The diagram side (S5, S6 of f12).** [`er_bridge`] turns this detail into
//! the `er` document the engine door already renders — the first index→diagram
//! path — with the inline and the table-level foreign keys as its relations.
//! It is a producer over the detail, not a change of this chapter: the symbols
//! and the relations of a SQL file are the same ones with the bridge or without
//! it. [`er_bridge::chapter_document`] is the whole of the surface: the path of
//! a `.sql` file and its text in, the document the door already renders out, so
//! «this `.sql` file» reaches the plan through the render call that already
//! serves `er` documents (no new message, no new tool, no flag). That module
//! documents the mapping and what it deliberately leaves undrawn (a target that
//! is not a primary key, a composite key).

use tree_sitter::Node;

use crate::lang::emit::{dotted_module, emit_named, text};
use crate::lang::engine::{Body, Nesting};
use crate::lang::model::{
    ParsedColumn, ParsedForeignKey, ParsedReference, ParsedSymbol, ParsedTable,
};
use crate::relations::{push_edges, RawEdge, USES};

/// The bridge from this chapter's detail to an `er` document (S5, S6 of f12).
///
/// The file lives beside this chapter (`er_bridge.rs`) because it maps **the
/// detail this module reads** and nothing else: the bridge is not a grammar,
/// so it is not an entry of the list of [`crate::lang::registry`] — it is
/// declared here, by the chapter that owns the shape it reads.
#[path = "er_bridge.rs"]
pub mod er_bridge;

/// The language of the chapter, as the table `file` spells it.
pub const LANGUAGE: &str = "sql";

/// The tree-sitter grammar of SQL: one grammar answers every `.sql` path.
pub fn language(_path: &str) -> tree_sitter::Language {
    tree_sitter::Language::new(tree_sitter_sequel::LANGUAGE)
}

/// The module path of a SQL file: its directories and its name without the
/// extension, dotted (`db.schema`). SQL declares no namespace around a file, so
/// the path is the only source of the name.
pub fn module_path(relative: &str) -> String {
    dotted_module(relative, &[])
}

/// The object a `CREATE` statement declares: the tables, the views and the
/// types as `type`, the routines as `fn`.
pub(crate) fn declaration(
    node: &Node,
    nesting: &Nesting,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) -> Option<Body> {
    let kind = match node.kind() {
        "create_table" | "create_view" | "create_materialized_view" | "create_type" => "type",
        "create_function" | "create_trigger" => "fn",
        _ => return None,
    };
    let name = declared(node, source)?;
    emit_named(node, kind, &name, nesting.prefix, nesting.parent, out);
    None
}

/// The name a `CREATE` statement declares: the first `object_reference` of the
/// statement, read by its `name` field (`schema.table` keeps the table name).
fn declared(node: &Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    let reference = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == "object_reference")?;
    reference
        .child_by_field_name("name")
        .and_then(|name| text(&name, source))
}

/// The relations of one SQL chapter, as the grammar sees them.
pub(crate) fn relations(root: &Node, source: &[u8]) -> Vec<RawEdge> {
    let mut out = Vec::new();
    visit(root, source, &mut out);
    out
}

/// Walks the tree once and records every relation it recognises.
fn visit(node: &Node, source: &[u8], out: &mut Vec<RawEdge>) {
    let byte = node.start_byte();
    match node.kind() {
        // `FROM`/`JOIN` name the tables a query reads.
        "relation" => {
            push_edges(out, byte, USES, referenced(node, source));
        }
        // `CREATE INDEX … ON <table>`: the index reads the table.
        "create_index" => {
            push_edges(out, byte, USES, referenced(node, source));
        }
        // `… REFERENCES <table>`: the foreign key reads the table.
        "column_definition" => {
            push_edges(out, byte, USES, foreign_key(node, source));
        }
        _ => {}
    }

    for child in children(node) {
        visit(&child, source, out);
    }
}

/// The tables an `object_reference` of a node names, at the top of the node.
fn referenced(node: &Node, source: &[u8]) -> Vec<String> {
    children(node)
        .into_iter()
        .filter(|child| child.kind() == "object_reference")
        .filter_map(|reference| reference_name(&reference, source))
        .collect()
}

/// The table a `REFERENCES` clause points to, as the relations read it.
fn foreign_key(node: &Node, source: &[u8]) -> Vec<String> {
    reference(node, source)
        .map(|target| vec![target.table])
        .unwrap_or_default()
}

/// The columns detail of a chapter (S3, S4 of f12): every `CREATE TABLE` whose
/// DDL declares columns, with its columns and its keys.
pub(crate) fn tables(root: &Node, source: &[u8]) -> Vec<ParsedTable> {
    let mut out = Vec::new();
    collect_tables(root, source, &mut out);
    out
}

/// Walks the tree once and records the detail of every table it finds.
fn collect_tables(node: &Node, source: &[u8], out: &mut Vec<ParsedTable>) {
    if node.kind() == "create_table" {
        if let Some(table) = table(node, source) {
            out.push(table);
        }
    }
    for child in children(node) {
        collect_tables(&child, source, out);
    }
}

/// One `CREATE TABLE`: its columns and the keys it declares at table level.
///
/// The table whose DDL writes no column list (`CREATE TABLE … AS SELECT`) has
/// no detail: there is nothing between the parentheses to read.
fn table(node: &Node, source: &[u8]) -> Option<ParsedTable> {
    let name = declared(node, source)?;
    let definitions = children(node)
        .into_iter()
        .find(|child| child.kind() == "column_definitions")?;
    let mut table = ParsedTable {
        name,
        columns: Vec::new(),
        foreign_keys: Vec::new(),
    };
    for child in children(&definitions) {
        match child.kind() {
            "column_definition" => {
                if let Some(column) = column(&child, source) {
                    table.columns.push(column);
                }
            }
            "constraints" => table_constraints(&child, source, &mut table),
            _ => {}
        }
    }
    Some(table)
}

/// One `column_definition`: the name, the type as the DDL spells it, the
/// nullability and the key the DDL gives it, and an inline `REFERENCES`.
fn column(node: &Node, source: &[u8]) -> Option<ParsedColumn> {
    let name = node
        .child_by_field_name("name")
        .and_then(|name| text(&name, source))?;
    let parts = children(node);
    Some(ParsedColumn {
        name,
        type_name: node
            .child_by_field_name("type")
            .and_then(|kind| text(&kind, source))
            .unwrap_or_default(),
        // `NOT NULL` is a sequence of two keywords, so a `NOT` of some other
        // clause (inside an expression) is never taken for it.
        not_null: parts
            .windows(2)
            .any(|pair| pair[0].kind() == "keyword_not" && pair[1].kind() == "keyword_null"),
        primary_key: parts.iter().any(|part| part.kind() == "keyword_primary"),
        references: reference(node, source),
    })
}

/// The constraints a table declares outside its columns: a `PRIMARY KEY (…)`
/// marks every column it names, and a `FOREIGN KEY (…) REFERENCES …` is one of
/// the table's keys. A `UNIQUE`, an `INDEX` or a `CHECK` is not detail the
/// diagram reads, so it is not carried.
fn table_constraints(node: &Node, source: &[u8], table: &mut ParsedTable) {
    for constraint in children(node) {
        if constraint.kind() != "constraint" {
            continue;
        }
        let parts = children(&constraint);
        let columns = parts
            .iter()
            .find(|part| part.kind() == "ordered_columns")
            .map(|ordered| ordered_columns(ordered, source))
            .unwrap_or_default();
        if parts.iter().any(|part| part.kind() == "keyword_primary") {
            for column in &mut table.columns {
                if columns.contains(&column.name) {
                    column.primary_key = true;
                }
            }
        } else if let Some(target) = reference(&constraint, source) {
            table.foreign_keys.push(ParsedForeignKey {
                columns,
                table: target.table,
                target_columns: target.columns,
            });
        }
    }
}

/// The columns an `ordered_columns` names: `(id, code)` → `id`, `code`.
fn ordered_columns(node: &Node, source: &[u8]) -> Vec<String> {
    children(node)
        .iter()
        .filter(|child| child.kind() == "column")
        .filter_map(|child| child.child_by_field_name("name"))
        .filter_map(|name| text(&name, source))
        .collect()
}

/// The `REFERENCES` clause of a column or a constraint: the table it names and
/// the columns of that table, when the DDL spells them.
fn reference(node: &Node, source: &[u8]) -> Option<ParsedReference> {
    let parts = children(node);
    let at = parts
        .iter()
        .position(|part| part.kind() == "keyword_references")?;
    let table = parts
        .get(at + 1)
        .filter(|target| target.kind() == "object_reference")
        .and_then(|target| reference_name(target, source))?;
    Some(ParsedReference {
        table,
        columns: parts[at + 2..]
            .iter()
            .take_while(|part| part.kind() == "identifier")
            .filter_map(|part| text(part, source))
            .collect(),
    })
}

/// The named children of a node, collected so a walk may recurse while it holds
/// them.
fn children<'tree>(node: &Node<'tree>) -> Vec<Node<'tree>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

/// The table name of an `object_reference`: `name` of the `schema.name` or
/// `database.schema.name` path the grammar writes.
fn reference_name(reference: &Node, source: &[u8]) -> Option<String> {
    reference
        .child_by_field_name("name")
        .and_then(|name| text(&name, source))
}
