//! The columns detail of a SQL chapter (f12, S3/S4): what a `CREATE TABLE`
//! declares inside its parentheses, carried on the chapter beside its symbols.
//!
//! A column is not a declaration of the book (the decision of `sql.rs`), so the
//! detail is not a `symbol` row and not an `edge` row: it is the shape the DDL
//! gives, on the chapter, for whoever builds a diagram out of it (T2). The
//! fixture is a two-table schema with the two forms the pinned grammar parses —
//! an inline `PRIMARY KEY`, an inline `REFERENCES other (id)`, a table-level
//! composite `PRIMARY KEY (…)` and a table-level `FOREIGN KEY … REFERENCES …`.
//!
//! Everything here reads the chapter the engine parses: no book, no database
//! and no diagram is built, and nothing of this file reaches `gx-kinds`.

use gx_index::lang::parse;

#[cfg(feature = "lang-sql")]
use gx_index::lang::model::{ParsedForeignKey, ParsedReference};

#[cfg(feature = "lang-sql")]
const DDL: &str = r#"
-- The schema of the fixture: two tables and the detail of each one.

CREATE TABLE store (
    id BIGINT PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    region TEXT
);

CREATE TABLE shipment (
    id BIGINT NOT NULL,
    store_id BIGINT NOT NULL REFERENCES store (id),
    code VARCHAR(32),
    label TEXT,
    PRIMARY KEY (id, label),
    FOREIGN KEY (code) REFERENCES store (name)
);
"#;

/// S4 (f12): a `CREATE TABLE` yields its columns — the name, the type as the
/// DDL spells it and the nullability the DDL writes — and the chapter keeps
/// them beside its symbols: a column is still not a declaration of the book.
#[cfg(feature = "lang-sql")]
#[test]
fn sql_reads_the_name_type_and_nullability_of_every_column() {
    let parsed = parse("db/schema.sql", DDL.as_bytes(), 0).expect("the SQL grammar");
    let tables = parsed
        .tables
        .as_deref()
        .expect("a SQL chapter carries the detail of its tables");
    assert_eq!(
        tables
            .iter()
            .map(|table| table.name.as_str())
            .collect::<Vec<_>>(),
        ["store", "shipment"],
        "the detail follows the DDL: one entry per table, in the order it declares them"
    );

    let columns: Vec<(&str, &str, bool)> = tables[0]
        .columns
        .iter()
        .map(|column| {
            (
                column.name.as_str(),
                column.type_name.as_str(),
                column.not_null,
            )
        })
        .collect();
    assert_eq!(
        columns,
        [
            ("id", "BIGINT", false),
            ("name", "VARCHAR(255)", true),
            ("region", "TEXT", false),
        ],
        "a column is its name, its type whole (`VARCHAR(255)`), and whether the DDL says NOT NULL"
    );

    // S2: the detail is carried beside the symbols --- a schema's `type`s are
    // the tables, and no column is one of them.
    let symbols: Vec<(&str, &str)> = parsed
        .symbols
        .iter()
        .map(|symbol| (symbol.kind, symbol.name.as_str()))
        .collect();
    assert_eq!(
        symbols,
        [("type", "store"), ("type", "shipment")],
        "a column is not a declaration of the book"
    );
}

/// S4 (f12): the key of a table, both forms the grammar parses — the inline
/// `PRIMARY KEY` of a column and the table-level `PRIMARY KEY (a, b)`, which is
/// carried whole as a key marker on every column it names.
#[cfg(feature = "lang-sql")]
#[test]
fn sql_marks_the_primary_key_of_a_column_inline_and_at_table_level() {
    let parsed = parse("db/schema.sql", DDL.as_bytes(), 0).expect("the SQL grammar");
    let tables = parsed
        .tables
        .as_deref()
        .expect("a SQL chapter carries the detail of its tables");

    let inline: Vec<(&str, bool)> = tables[0]
        .columns
        .iter()
        .map(|column| (column.name.as_str(), column.primary_key))
        .collect();
    assert_eq!(
        inline,
        [("id", true), ("name", false), ("region", false)],
        "`id BIGINT PRIMARY KEY` marks its own column and no other"
    );

    let composite: Vec<(&str, bool)> = tables[1]
        .columns
        .iter()
        .map(|column| (column.name.as_str(), column.primary_key))
        .collect();
    assert_eq!(
        composite,
        [
            ("id", true),
            ("store_id", false),
            ("code", false),
            ("label", true),
        ],
        "a table-level `PRIMARY KEY (id, label)` marks both of its columns and only them"
    );
}

/// S4 (f12): the foreign key of a table, both forms the grammar parses — the
/// inline `REFERENCES store (id)` of a column and the table-level
/// `FOREIGN KEY (code) REFERENCES store (name)` — with the columns each one
/// names, so whoever draws the relation does not have to guess them.
#[cfg(feature = "lang-sql")]
#[test]
fn sql_reads_the_target_of_a_foreign_key_inline_and_at_table_level() {
    let parsed = parse("db/schema.sql", DDL.as_bytes(), 0).expect("the SQL grammar");
    let tables = parsed
        .tables
        .as_deref()
        .expect("a SQL chapter carries the detail of its tables");

    let inline = tables[1]
        .columns
        .iter()
        .find(|column| column.name == "store_id")
        .expect("the column of the inline key");
    assert_eq!(
        inline.references,
        Some(ParsedReference {
            table: "store".to_string(),
            columns: vec!["id".to_string()],
        }),
        "an inline REFERENCES carries its table and the column it points to"
    );

    assert_eq!(
        tables[1].foreign_keys,
        [ParsedForeignKey {
            columns: vec!["code".to_string()],
            table: "store".to_string(),
            target_columns: vec!["name".to_string()],
        }],
        "a table-level FOREIGN KEY is the table's own key: its columns and the target's"
    );
    assert!(
        tables[0].foreign_keys.is_empty(),
        "`store` declares no key outside its columns"
    );
}

/// PRESERVE (S4, f12): the detail is SQL's alone. Every chapter of another
/// language of the book keeps the chapter it had, with no table detail at all —
/// the field is `None` and nothing of the pipeline changed for it.
#[cfg(feature = "lang-rust")]
#[test]
fn a_chapter_of_another_language_carries_no_table_detail() {
    let parsed = parse("src/demo.rs", b"pub struct Demo;\n", 0).expect("the Rust grammar");
    assert!(
        parsed.tables.is_none(),
        "the table detail is SQL's alone: {:?}",
        parsed.tables
    );
    assert_eq!(
        parsed
            .symbols
            .iter()
            .map(|symbol| (symbol.kind, symbol.name.as_str()))
            .collect::<Vec<_>>(),
        [("type", "Demo")]
    );
}
