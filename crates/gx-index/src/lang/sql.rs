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
//! (`store_id BIGINT REFERENCES store (id)`); a *named* constraint
//! (`CONSTRAINT fk FOREIGN KEY …`) is not in its rules and parses as an error
//! node, so the book keeps no relation for it and the fixture of the tests
//! writes the inline form.

use tree_sitter::Node;

use crate::lang::emit::{dotted_module, emit_named, text};
use crate::lang::engine::{Body, Nesting};
use crate::lang::model::ParsedSymbol;
use crate::relations::{push_edges, RawEdge, USES};

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

    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    for child in children {
        visit(&child, source, out);
    }
}

/// The tables an `object_reference` of a node names, at the top of the node.
fn referenced(node: &Node, source: &[u8]) -> Vec<String> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|child| child.kind() == "object_reference")
        .filter_map(|reference| reference_name(&reference, source))
        .collect()
}

/// The table a `REFERENCES` clause points to.
fn foreign_key(node: &Node, source: &[u8]) -> Vec<String> {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    let mut names = Vec::new();
    for (index, child) in children.iter().enumerate() {
        if child.kind() != "keyword_references" {
            continue;
        }
        if let Some(name) = children
            .get(index + 1)
            .filter(|target| target.kind() == "object_reference")
            .and_then(|target| reference_name(target, source))
        {
            names.push(name);
        }
    }
    names
}

/// The table name of an `object_reference`: `name` of the `schema.name` or
/// `database.schema.name` path the grammar writes.
fn reference_name(reference: &Node, source: &[u8]) -> Option<String> {
    reference
        .child_by_field_name("name")
        .and_then(|name| text(&name, source))
}
