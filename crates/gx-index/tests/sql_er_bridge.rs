//! The bridge from the SQL detail to an `er` document (f12, S5/S6/S7).
//!
//! The book reads a schema as detail — a table with its columns, carried on the
//! chapter beside its symbols — and stops there: a column is not a declaration
//! (`sql.rs`, S2). This bridge is the first index→diagram path: it turns that
//! detail into the `er` document the engine door already accepts
//! (`version: 1, kind: "er"`), with the tables as nodes whose rows are the
//! columns and the foreign keys as the relations of the er contract.
//!
//! The JSON is what this file asserts, because the JSON is what the bridge
//! produces: the bridge does not depend on `gx-kinds` (the book is a leaf that
//! knows no diagram, and `gx-kinds` sits above it), so the step from the
//! document to `IrNode.rows`/`IrEdge` is `er.rs`'s own, tested there with the
//! author path. The document is written in exactly the contract's vocabulary —
//! `key: pk|fk|pk_fk`, `nullable`, `type`, `relations[].from/.to` — so the door
//! translates it with the rules it already has (`er.rs:296-305` for the row,
//! `er.rs:311-318` for the edge's anchors).
//!
//! Everything here goes through the chapter the engine parses: the fixture is
//! DDL, not a hand-built detail, except where the test names the shape the
//! bridge must refuse.

#[cfg(feature = "lang-sql")]
use serde_json::Value;

#[cfg(feature = "lang-sql")]
use gx_index::lang::model::{ParsedColumn, ParsedForeignKey, ParsedTable};
#[cfg(feature = "lang-sql")]
use gx_index::lang::parse;
#[cfg(feature = "lang-sql")]
use gx_index::lang::sql::er_bridge;

/// The two-table schema of the chapter's own fixture (`tests/sql_detail.rs`):
/// an inline `PRIMARY KEY`, an inline `REFERENCES store (id)`, a table-level
/// composite `PRIMARY KEY (id, label)` and a table-level
/// `FOREIGN KEY (code) REFERENCES store (name)`.
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

/// The same two tables, but the key is declared at table level and it points at
/// the primary key of the other table: the form the er contract can draw.
#[cfg(feature = "lang-sql")]
const DDL_TABLE_LEVEL_KEY: &str = r#"
CREATE TABLE store (
    id BIGINT PRIMARY KEY,
    name VARCHAR(255) NOT NULL
);

CREATE TABLE shipment (
    id BIGINT PRIMARY KEY,
    store_id BIGINT NOT NULL,
    FOREIGN KEY (store_id) REFERENCES store (id)
);
"#;

/// The document of one DDL fixture, through the chapter the engine parses.
#[cfg(feature = "lang-sql")]
fn document(ddl: &str) -> Value {
    let parsed = parse("db/schema.sql", ddl.as_bytes(), 0).expect("the SQL grammar");
    let tables = parsed
        .tables
        .as_deref()
        .expect("a SQL chapter carries the detail of its tables");
    er_bridge::document(tables).expect("the bridge builds an `er` document")
}

/// One row of a table of the document, as the `er` contract writes it: the
/// name, the type (the row's note, `er.rs:296-305`), the key and nullability.
#[cfg(feature = "lang-sql")]
fn row(name: &str, kind: &str, key: &str, nullable: bool) -> (String, String, String, bool) {
    (
        name.to_string(),
        kind.to_string(),
        key.to_string(),
        nullable,
    )
}

/// The rows of one table of the document, in the order it draws them.
#[cfg(feature = "lang-sql")]
fn rows(document: &Value, name: &str) -> Vec<(String, String, String, bool)> {
    document["tables"]
        .as_array()
        .expect("tables")
        .iter()
        .find(|table| table["name"] == name)
        .unwrap_or_else(|| panic!("the document has a table `{name}`: {document}"))["columns"]
        .as_array()
        .expect("columns")
        .iter()
        .map(|column| {
            (
                column["name"]
                    .as_str()
                    .expect("the name of a row")
                    .to_string(),
                column["type"].as_str().unwrap_or_default().to_string(),
                column["key"].as_str().unwrap_or_default().to_string(),
                column["nullable"]
                    .as_bool()
                    .expect("every row says whether it is nullable"),
            )
        })
        .collect()
}

/// One relation of the document: `from`, then `to`.
#[cfg(feature = "lang-sql")]
fn relation(
    table: &str,
    column: &str,
    target: &str,
    target_column: &str,
) -> (String, String, String, String) {
    (
        table.to_string(),
        column.to_string(),
        target.to_string(),
        target_column.to_string(),
    )
}

/// The relations of the document, as `from` → `to`, in the order it draws them.
#[cfg(feature = "lang-sql")]
fn relations(document: &Value) -> Vec<(String, String, String, String)> {
    document["relations"]
        .as_array()
        .expect("relations")
        .iter()
        .map(|relation| {
            (
                relation["from"]["table"]
                    .as_str()
                    .expect("from.table")
                    .to_string(),
                relation["from"]["column"]
                    .as_str()
                    .expect("from.column")
                    .to_string(),
                relation["to"]["table"]
                    .as_str()
                    .expect("to.table")
                    .to_string(),
                relation["to"]["column"]
                    .as_str()
                    .expect("to.column")
                    .to_string(),
            )
        })
        .collect()
}

/// S5 (f12): the detail of two tables becomes an `er` document — the rows the
/// DDL declares (name, type, key, nullability) and the foreign key as the
/// relation of the er contract, anchored from the referenced key to the
/// referencing column.
#[cfg(feature = "lang-sql")]
#[test]
fn the_detail_of_two_tables_becomes_an_er_document_with_its_rows_and_its_key() {
    let document = document(DDL);

    // S6: the wire shape the door already accepts --- no new message, no new
    // field: `er` version 1 and the four keys of the contract (`ErInput`).
    assert_eq!(
        document["version"], 1,
        "the document is the version 1 contract"
    );
    assert_eq!(document["kind"], "er", "the document is an `er` document");
    assert_eq!(
        document
            .as_object()
            .expect("a document is an object")
            .keys()
            .collect::<Vec<_>>(),
        ["kind", "relations", "tables", "version"],
        "the bridge adds no field of its own to the contract"
    );
    // The text the door reads (`render_json(src)`) is the same document.
    let parsed = parse("db/schema.sql", DDL.as_bytes(), 0).expect("the SQL grammar");
    let text = er_bridge::document_json(parsed.tables.as_deref().expect("the detail"))
        .expect("the text of the document");
    assert_eq!(
        serde_json::from_str::<Value>(&text).expect("the document is JSON"),
        document,
        "`document_json` is the very document, as the text the door reads"
    );

    assert_eq!(
        rows(&document, "store"),
        [
            row("id", "BIGINT", "pk", false),
            row("name", "VARCHAR(255)", "", false),
            row("region", "TEXT", "", true),
        ],
        "a table is a node whose rows are its columns: the inline key is `pk`, a \
         `NOT NULL` row is not nullable, and a plain column is"
    );
    assert_eq!(
        rows(&document, "shipment"),
        [
            row("id", "BIGINT", "pk", false),
            row("store_id", "BIGINT", "fk", false),
            row("code", "VARCHAR(32)", "fk", true),
            row("label", "TEXT", "pk", false),
        ],
        "the inline `REFERENCES` marks its column `fk` and a table-level key marks \
         the columns it names; a key column is not nullable, because SQL says so"
    );

    assert_eq!(
        relations(&document),
        [relation("store", "id", "shipment", "store_id")],
        "the relation goes from the referenced primary key to the referencing \
         column --- where `er.rs` anchors its edge --- and the fixture's \
         `FOREIGN KEY (code) REFERENCES store (name)` is not drawn: the er \
         contract's `from` must be a primary key and `store.name` is not one"
    );
}

/// S5 (f12): a table-level `FOREIGN KEY (…) REFERENCES …` pointing at the
/// primary key is drawn like an inline one — the two forms the pinned grammar
/// parses reach the same relation.
#[cfg(feature = "lang-sql")]
#[test]
fn a_table_level_foreign_key_to_the_primary_key_is_drawn_like_an_inline_one() {
    let document = document(DDL_TABLE_LEVEL_KEY);

    assert_eq!(
        rows(&document, "shipment"),
        [
            row("id", "BIGINT", "pk", false),
            row("store_id", "BIGINT", "fk", false),
        ],
        "the column a table-level `FOREIGN KEY` names is a foreign key row"
    );
    assert_eq!(
        relations(&document),
        [relation("store", "id", "shipment", "store_id")],
        "a table-level foreign key is the same relation as the inline one"
    );
}

/// S5 (f12): a foreign key the er contract cannot anchor is skipped, and the
/// document stays valid — the er contract's law is that `from` is a key and
/// `to` is a foreign key (`er.rs:245-260`), so a target that is not a primary
/// key, a table the detail does not carry, and a composite key anchor nothing.
#[cfg(feature = "lang-sql")]
#[test]
fn a_relation_the_er_contract_cannot_anchor_is_skipped() {
    const DDL: &str = r#"
CREATE TABLE country (
    code TEXT PRIMARY KEY,
    label TEXT NOT NULL
);

CREATE TABLE pair (
    left_id BIGINT,
    right_id BIGINT,
    PRIMARY KEY (left_id, right_id)
);

CREATE TABLE city (
    id BIGINT PRIMARY KEY,
    country_code TEXT REFERENCES missing (code),
    name TEXT,
    pair_id BIGINT,
    FOREIGN KEY (name) REFERENCES country (label),
    FOREIGN KEY (pair_id) REFERENCES pair (left_id)
);
"#;
    let document = document(DDL);

    assert_eq!(
        rows(&document, "pair"),
        [
            row("left_id", "BIGINT", "pk", false),
            row("right_id", "BIGINT", "pk", false)
        ],
        "a composite key is carried whole, as one `pk` row per column it names — \
         what is not mapped is the relation that would point at it"
    );
    assert_eq!(
        rows(&document, "city"),
        [
            row("id", "BIGINT", "pk", false),
            row("country_code", "TEXT", "fk", true),
            row("name", "TEXT", "fk", true),
            row("pair_id", "BIGINT", "fk", true),
        ],
        "the DDL declares all three keys, so all three columns are `fk` rows, \
         even when their relation cannot be drawn"
    );
    assert!(
        relations(&document).is_empty(),
        "`missing` is not in the detail, `country.label` is not a key, and \
         `pair`'s key is composite (the er contract keys a relation on one \
         column): nothing is drawn — and nothing invalid is drawn either: {:?}",
        relations(&document)
    );
}

/// S5 (f12): the bridge never writes a relation the er contract would reject —
/// a foreign key whose referencing column is not a column of its table cannot
/// anchor `to` (`er.rs:245-260`), so it is skipped like the other ones.
#[cfg(feature = "lang-sql")]
#[test]
fn a_relation_whose_column_is_not_a_row_of_its_table_is_skipped() {
    let detail = [ParsedTable {
        name: "shipment".to_string(),
        columns: vec![ParsedColumn {
            name: "id".to_string(),
            type_name: "BIGINT".to_string(),
            not_null: true,
            primary_key: true,
            references: None,
        }],
        foreign_keys: vec![ParsedForeignKey {
            columns: vec!["ghost".to_string()],
            table: "shipment".to_string(),
            target_columns: vec!["id".to_string()],
        }],
    }];
    let document = er_bridge::document(&detail).expect("the table has a row to draw");
    assert!(
        relations(&document).is_empty(),
        "`shipment.ghost` is not a row of the table, so no edge can anchor there: {:?}",
        relations(&document)
    );
}

/// S5 (f12): a table with no columns is skipped instead of drawn as an empty
/// compartment — a `CREATE TABLE … AS SELECT` writes no column list, so there
/// is no row to draw.
#[cfg(feature = "lang-sql")]
#[test]
fn a_table_with_no_columns_is_skipped_instead_of_an_empty_compartment() {
    // The shape a `CREATE TABLE … AS SELECT` leaves for the bridge: a table
    // with no rows. A detail with no row at all is no diagram at all.
    let snapshot = ParsedTable {
        name: "snapshot".to_string(),
        columns: Vec::new(),
        foreign_keys: Vec::new(),
    };
    assert!(
        er_bridge::document(std::slice::from_ref(&snapshot)).is_none(),
        "a detail with no column to draw produces no document"
    );

    // Beside a schema that has rows, the empty table is not an empty node: the
    // document is made of the tables with rows only.
    let mut detail: Vec<ParsedTable> = parse("db/schema.sql", DDL.as_bytes(), 0)
        .expect("the SQL grammar")
        .tables
        .expect("the detail of the chapter");
    detail.push(snapshot);
    let document = er_bridge::document(&detail).expect("the tables with rows still draw");
    assert_eq!(
        document["tables"]
            .as_array()
            .expect("tables")
            .iter()
            .map(|table| table["name"].as_str().expect("a name"))
            .collect::<Vec<_>>(),
        ["store", "shipment"],
        "an empty table is not drawn as a compartment with no rows"
    );

    // And the DDL the shape comes from: a `CREATE TABLE … AS SELECT` writes no
    // column list, so a chapter of that DDL alone has nothing to draw.
    for ddl in [
        "CREATE TABLE snapshot AS SELECT id, name FROM store;\n",
        "CREATE TABLE empty ();\n",
    ] {
        let parsed = parse("db/snapshot.sql", ddl.as_bytes(), 0).expect("the SQL grammar");
        assert!(
            er_bridge::document(parsed.tables.as_deref().unwrap_or_default()).is_none(),
            "no column list, no document: {ddl}"
        );
    }
}

/// PRESERVE (S2, S7 of f12): the bridge reads the chapter, it does not change
/// it. The symbols are still the declarations of the schema — one `type` per
/// table and no column among them — and the relations of the chapter are still
/// its own `uses` edges.
#[cfg(feature = "lang-sql")]
#[test]
fn the_bridge_reads_the_chapter_without_changing_it() {
    let parsed = parse("db/schema.sql", DDL.as_bytes(), 0).expect("the SQL grammar");
    let symbols: Vec<(&str, String)> = parsed
        .symbols
        .iter()
        .map(|symbol| (symbol.kind, symbol.name.clone()))
        .collect();
    let edges: Vec<(&str, String)> = parsed
        .edges
        .iter()
        .map(|edge| (edge.kind, edge.dst_name.clone()))
        .collect();

    let document = er_bridge::document(parsed.tables.as_deref().expect("the detail"))
        .expect("the document of the chapter");
    assert!(!document["tables"].as_array().expect("tables").is_empty());

    assert_eq!(
        parsed
            .symbols
            .iter()
            .map(|symbol| (symbol.kind, symbol.name.clone()))
            .collect::<Vec<_>>(),
        symbols,
        "drawing the detail does not declare anything: a column is still not a \
         declaration of the book"
    );
    assert_eq!(
        parsed
            .edges
            .iter()
            .map(|edge| (edge.kind, edge.dst_name.clone()))
            .collect::<Vec<_>>(),
        edges,
        "drawing the detail does not add a relation to the chapter"
    );
}

/// S6 (f12): the surface is one call — the path of a `.sql` file and its text
/// in, the document the render door takes out.
///
/// The call composes the chapter's own parse with the bridge, so «this .sql
/// file» reaches the plan through the doors that already exist: the document it
/// answers is the one `render_json` (the engine door) and `render_diagram` (the
/// MCP door) already render. No new message, no new tool, no flag.
#[cfg(feature = "lang-sql")]
#[test]
fn a_sql_file_answers_the_document_the_door_renders() {
    let text = er_bridge::chapter_document("db/schema.sql", DDL)
        .expect("the schema of the fixture draws a document");

    // The very text the two-step path builds: the chapter's parse plus
    // `document_json`, never a second mapping that could drift from it.
    let parsed = parse("db/schema.sql", DDL.as_bytes(), 0).expect("the SQL grammar");
    assert_eq!(
        Some(text.as_str()),
        er_bridge::document_json(parsed.tables.as_deref().expect("the detail of the chapter"))
            .as_deref(),
        "the file in, the document the door renders out: one mapping, one document"
    );

    let document: Value = serde_json::from_str(&text).expect("the document is JSON");
    assert_eq!(
        document["tables"]
            .as_array()
            .expect("tables")
            .iter()
            .map(|table| table["name"].as_str().expect("the name of a table"))
            .collect::<Vec<_>>(),
        ["store", "shipment"],
        "the door gets the tables of the file, not a summary of them"
    );
    assert_eq!(
        relations(&document),
        [relation("store", "id", "shipment", "store_id")],
        "and the foreign key of the file travels with them as the er relation"
    );
}

/// S6 (f12): the call answers no document when the chapter has nothing to
/// draw. It never invents a diagram, and it needs no language check to know:
/// the detail is the mention of the tables.
#[cfg(feature = "lang-sql")]
#[test]
fn a_file_without_a_schema_answers_no_document() {
    let cases = [
        // A chapter of another language: it parses, and it carries no table
        // detail, so there is nothing to draw.
        ("src/main.rs", "pub fn one() {}\n"),
        // A `CREATE TABLE … AS SELECT` writes no column list.
        (
            "db/snapshot.sql",
            "CREATE TABLE snapshot AS SELECT id, name FROM store;\n",
        ),
        // A path no grammar of the book reads.
        ("notes.txt", "CREATE TABLE store (id BIGINT PRIMARY KEY);\n"),
    ];
    for (path, source) in cases {
        assert_eq!(
            er_bridge::chapter_document(path, source),
            None,
            "there is no schema to draw on `{path}`"
        );
    }
}

/// S3, S7 (f12): the limit the chapter documents, pinned — the *name* of a
/// named `CONSTRAINT <name> FOREIGN KEY …` is not readable at this grammar
/// version, and what the name leaves behind still parses: the `(…) REFERENCES
/// …` tail is read as the key it declares, so the column is a `fk` row and the
/// relation is drawn.
#[cfg(feature = "lang-sql")]
#[test]
fn a_named_foreign_key_loses_its_name_and_keeps_its_key() {
    const DDL: &str = r#"
CREATE TABLE store (
    id BIGINT PRIMARY KEY,
    name VARCHAR(255) NOT NULL
);

CREATE TABLE shipment (
    id BIGINT PRIMARY KEY,
    store_id BIGINT NOT NULL,
    CONSTRAINT fk_store FOREIGN KEY (store_id) REFERENCES store (id)
);
"#;
    let text = er_bridge::chapter_document("db/named.sql", DDL)
        .expect("the schema of the fixture draws a document");
    let document: Value = serde_json::from_str(&text).expect("the document is JSON");

    // The pinned grammar (`tree-sitter-sequel 0.3.11`) does not parse the name:
    // `CONSTRAINT fk_store FOREIGN` lands in an `ERROR` node and the
    // `(store_id) REFERENCES store (id)` tail parses as the unnamed key, which
    // the chapter reads. The limit is the name the book cannot keep, not the key
    // it can: the diagram is the same one the unnamed form draws.
    assert_eq!(
        rows(&document, "shipment"),
        [
            row("id", "BIGINT", "pk", false),
            row("store_id", "BIGINT", "fk", false),
        ],
        "the key the named constraint declares is read: `store_id` is a `fk` row"
    );
    assert_eq!(
        relations(&document),
        [relation("store", "id", "shipment", "store_id")],
        "and its relation is drawn, because the er contract can anchor it"
    );
}

/// The detail of one chapter of a merged project, as the door concatenates it:
/// the tables of the chapter in the order the book lists its chapters, or
/// nothing when a chapter carries no table (S2/S3 of f15).
#[cfg(feature = "lang-sql")]
fn details_of(chapters: &[(&str, &str)]) -> Vec<ParsedTable> {
    let mut merged: Vec<ParsedTable> = Vec::new();
    for (path, source) in chapters {
        let parsed = parse(path, source.as_bytes(), 0).expect("the grammar of the chapter");
        if let Some(tables) = parsed.tables {
            merged.extend(tables);
        }
    }
    merged
}

/// S4/S5 (f15): the merge is the bridge fed the merged slice — a foreign key in
/// one chapter that references a table declared in **another** chapter anchors,
/// because `document_json` resolves `anchor`/`primary_key` over the whole slice
/// and never over one chapter at a time.
#[cfg(feature = "lang-sql")]
#[test]
fn a_foreign_key_across_two_chapters_anchors_on_the_merged_slice() {
    const USERS: &str = "\
CREATE TABLE users (\n  id INTEGER PRIMARY KEY,\n  name TEXT NOT NULL\n);\n";
    const POSTS: &str = "\
CREATE TABLE posts (\n  id INTEGER PRIMARY KEY,\n  user_id INTEGER NOT NULL REFERENCES users(id),\n  title TEXT\n);\n";

    // The per-chapter answer still loses the cross-file key: the target table
    // is not in the slice, so the bridge draws no relation for it. That is the
    // finding S1 closes («a FK to a table declared in another `.sql` silently
    // vanishes»), pinned as the reason the merge exists.
    let posts_alone = er_bridge::document(
        parse("db/posts.sql", POSTS.as_bytes(), 0)
            .expect("the SQL grammar")
            .tables
            .as_deref()
            .expect("the detail of the chapter"),
    )
    .expect("the table with rows draws");
    assert!(
        relations(&posts_alone).is_empty(),
        "one chapter alone cannot anchor a key to a table it does not carry: {:?}",
        relations(&posts_alone)
    );

    let merged = details_of(&[("db/users.sql", USERS), ("db/posts.sql", POSTS)]);
    let text = er_bridge::document_json(&merged).expect("the merged detail draws");
    let document: Value = serde_json::from_str(&text).expect("the document is JSON");

    assert_eq!(
        document["tables"]
            .as_array()
            .expect("tables")
            .iter()
            .map(|table| table["name"].as_str().expect("a table has a name"))
            .collect::<Vec<_>>(),
        ["users", "posts"],
        "the merged slice draws the tables of both chapters, in the order the \
         book lists them"
    );
    assert_eq!(
        relations(&document),
        [relation("users", "id", "posts", "user_id")],
        "the cross-file foreign key anchors at last: `users.id` is a key of the \
         merged slice, so the relation `posts.user_id` declares is drawn"
    );
    assert_eq!(
        rows(&document, "posts"),
        [
            row("id", "INTEGER", "pk", false),
            row("user_id", "INTEGER", "fk", false),
            row("title", "TEXT", "", true),
        ],
        "the referencing column keeps its `fk` row, as it did per chapter"
    );
}

/// S2/S4 (f15): a table name the merged slice carries twice is drawn **once**,
/// and the first occurrence wins — `drawable()` dedupes by name, so the second
/// chapter's copy contributes rows only when the first one had none to draw.
#[cfg(feature = "lang-sql")]
#[test]
fn a_table_name_declared_twice_is_drawn_once_with_the_first_occurrence() {
    const FIRST: &str = "\
CREATE TABLE users (\n  id INTEGER PRIMARY KEY,\n  name TEXT NOT NULL\n);\n";
    // The same table, migrated: another shape under the same name.
    const SECOND: &str = "\
CREATE TABLE users (\n  id INTEGER PRIMARY KEY,\n  email TEXT NOT NULL\n);\n\
CREATE TABLE posts (\n  id INTEGER PRIMARY KEY,\n  user_id INTEGER NOT NULL REFERENCES users(id)\n);\n";

    let merged = details_of(&[("db/001-users.sql", FIRST), ("db/002-users.sql", SECOND)]);
    let document: Value =
        serde_json::from_str(&er_bridge::document_json(&merged).expect("the merged detail draws"))
            .expect("the document is JSON");

    assert_eq!(
        document["tables"]
            .as_array()
            .expect("tables")
            .iter()
            .filter(|table| table["name"] == "users")
            .count(),
        1,
        "the er contract keys a table by name, so `users` is one compartment: {document}"
    );
    assert_eq!(
        rows(&document, "users"),
        [
            row("id", "INTEGER", "pk", false),
            row("name", "TEXT", "", false),
        ],
        "the first occurrence wins: the second chapter's `users` adds no row to \
         the document"
    );
    assert_eq!(
        relations(&document),
        [relation("users", "id", "posts", "user_id")],
        "the relation of the second chapter still anchors on the drawn `users`"
    );
}

/// S3 (f15): a chapter with no table contributes nothing to the merge — it is
/// not an empty node and it does not change the document of the chapters that
/// do carry detail.
#[cfg(feature = "lang-sql")]
#[test]
fn a_chapter_with_no_tables_contributes_nothing_to_the_merge() {
    const USERS: &str = "\
CREATE TABLE users (\n  id INTEGER PRIMARY KEY,\n  name TEXT NOT NULL\n);\n";
    // Parses, carries no table: another language and a SQL chapter of notes.
    const NOTES_SQL: &str = "-- the schema is in db/users.sql\nSELECT 1;\n";
    const RUST: &str = "pub fn main() {}\n";

    let alone = details_of(&[("db/users.sql", USERS)]);
    let with_empty = details_of(&[
        ("db/notes.sql", NOTES_SQL),
        ("db/users.sql", USERS),
        ("src/main.rs", RUST),
    ]);

    assert_eq!(
        er_bridge::document_json(&with_empty),
        er_bridge::document_json(&alone),
        "a chapter with no tables is not a table of the merged document and \
         changes nothing about the ones that are"
    );
    let document: Value = serde_json::from_str(
        &er_bridge::document_json(&with_empty).expect("the merged detail draws"),
    )
    .expect("the document is JSON");
    assert_eq!(
        document["tables"]
            .as_array()
            .expect("tables")
            .iter()
            .map(|table| table["name"].as_str().expect("a table has a name"))
            .collect::<Vec<_>>(),
        ["users"],
        "only the chapter that declares a table appears"
    );
}
