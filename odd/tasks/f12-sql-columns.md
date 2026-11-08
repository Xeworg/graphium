# f12-sql-columns — el puente index→diagrama: columnas y claves reales desde SQL DDL

Branch `feature/f12-sql-columns` from `staging` @ b3c0522. Worktree:
`/home/xeworg/Proyectos/graphium-f12`. Sources: one read-only exploration
round (the index engine, the ER row flow, the pinned grammar, the goldens —
its handoff is quoted in the Specs below), the f11 deferral (S6/L2), and the
owner's opening of the feature on 2026-11-09.

## Specs

- S1 (owner, verbatim): «sigue con f12» — the owner opened the candidate that
  f11 deferred; the deferred requirement, quoted from f11's S6: "SQL column
  extraction stays out (S6) — ER columns stay author-authored this feature.
  Tree-sitter SQL column extraction (blocked by design at
  `gx-index/src/lang/sql.rs:1-10`) is OUT OF SCOPE — candidate f12."
- S2 (exploration, the real shape): f12 is a new index→diagram surface, not a
  change to `sql.rs`'s declaration rules. The chapter's design decision stands
  («una columna no es una declaración del libro», `sql.rs:4-6`): columns are
  DETAIL carried beside the symbols, never `symbol` rows.
- S3 (extraction, what the pinned grammar gives): `tree-sitter-sequel 0.3.11`
  parses `column_definition` (name + type + `_not_null`/`_primary_key` +
  inline `references`), table-level `_primary_key_constraint` and
  `_key_constraint` (`FOREIGN KEY … REFERENCES`). Named constraints
  (`CONSTRAINT <name> FOREIGN KEY …`) are unparseable at this version
  (`sql.rs:33-38`; grammar `_constraint_literal` admits only PRIMARY
  KEY/CHECK) — f12 reads the two forms that parse and documents the third as
  not supported.
- S4 (the columns detail): a `CREATE TABLE` yields its columns — name, type,
  nullability, PK (inline or table-level), inline FK target — carried on the
  table's detail structure (a new field beside `ParsedFile.symbols/edges`,
  `model.rs:60-78`), never through the `symbol` table (the CHECK at
  `schema.rs:449` stays untouched; no schema-version bump).
- S5 (the bridge): a producer turns the SQL detail into
  `gx_kinds::er::ErInput`-shaped data (`er.rs:69-88`) so `to_ir`
  (`er.rs:262`) builds `IrNode.rows` + `IrEdge` — the first index→diagram
  path (none exists today; rows are author-only at `er.rs:296-305`). The ER
  validation contract holds: a relation's `from` must be PK, `to` must be FK
  (`er.rs:245-260`); composite `PRIMARY KEY (a, b)` maps with the er
  contract's own rules or is documented as not mapped.
- S6 (the door): the diagram reaches the plan through the existing door —
  the renderer/renderer call that already serves `er` documents from JSON —
  with the document the bridge produced; no new wire message, no CONTRACT
  payload change.
- S7 (gates): `er-posts` stays byte-identical (the author path is
  untouched); new fixture tests: columns detail extracted from a DDL fixture
  (the RED of T1), a two-table FK DDL that lands as an ER diagram with rows
  and the FK edge (the RED of T2), and the two `languages.rs` SQL tests
  (`languages.rs:1715-1802`) stay green — columns are still not declarations.

## Tasks (delivery order)

- T1 (S3 S4 S7; route: worker; commit: work-unit): the SQL detail — read
  `column_definition` (name, type, nullability, PK, inline `references`)
  and table-level `_primary_key_constraint`/`_key_constraint` in
  `crates/gx-index/src/lang/sql.rs`, carried as a new detail field on
  `ParsedFile` (`model.rs:60-78`) beside symbols/edges, never as symbols.
  Test-first: RED = a fixture DDL asserts the extracted column list (names,
  types, PK, inline FK target) from the chapter; GREEN = the detail lands;
  the two existing `languages.rs` SQL tests stay green unchanged.
- T2 (S5 S6 S7; route: worker; commit: work-unit): the bridge — a producer
  from the SQL detail to `gx_kinds::er::ErInput` (tables with columns, FK
  relations from inline and table-level `REFERENCES`), so a `.sql` fixture
  becomes an ER document that renders rows and the FK edge with the er
  contract's validation intact. RED = the two-table DDL fixture asserts the
  built `IrNode.rows`/`IrEdge`; GREEN = it renders through the door;
  `er-posts` byte-identical.
- T3 (S6 S7; route: worker; commit: work-unit): the surface — where the
  owner meets it (the render path that takes the bridged document; a CLI or
  MCP-visible seam only if the existing door offers one naturally — no new
  wire message), plus the documentation of the limits (named FK constraints
  not supported; composite PK rule) in the chapter header where sql.rs
  already documents its grammar gap (`sql.rs:33-38`), and the feature doc's
  Log closing entry.

## Log

- L1 (2026-11-09, owner): «sigue con f12» — opening the candidate f11
  deferred (S6/L2). The exploration handoff (2026-11-09, gentle-ai-explore,
  quoted for the record): the blocked point is a design decision, not a
  missing grammar (`sql.rs:4-6`, `sql.rs:60-66`); the engine pattern is
  `Grammar` + declarative `Rules` (`engine.rs:40-77`, `154-170`) with the
  Rust worked example (`registry.rs:144-190`); results land as
  `ParsedSymbol`/`ParsedEdge` in `symbol`/`edge` (`model.rs:38-58`,
  `schema.rs:445-475`) — **not** IR; ER rows are author-only today
  (`gx-kinds/src/er.rs:296-305`) and the scene reads `SceneNode.rows`
  (`scene.rs:139`, geometry `geometry.rs:250-311`); ER edges are
  `IrEdge{from,to,from_row,to_row,cardinality}` keyed by table name with the
  `er` JSON the only author today (`er.rs:311-318`); the pinned
  `tree-sitter-sequel 0.3.11` parses `column_definition`,
  `_primary_key_constraint` and `_key_constraint … REFERENCES` (inline FK
  included) but NOT named `CONSTRAINT fk` (`grammar.js:2811-2823`);
  `er-posts` is the ER-with-rows golden (`golden_svg.rs:102-124`,
  `golden/er-posts.svg:39,42`). The proposed cut (T1 detail, T2 bridge, T3
  surface) and the honest blockers (no index→diagram path exists; the
  `symbol.kind` CHECK and `edge.kind` must not be widened; named FK
  constraints unparseable) are the source of S2-S7 above.
- L2 (2026-11-09, worker): T1 landed — the SQL chapter reads the columns
  detail and carries it beside its symbols, never as one. **The shape.**
  `ParsedFile` gained `tables: Option<Vec<ParsedTable>>` (`lang/model.rs`),
  `None` for every language but SQL and never written to `symbol`/`edge`;
  `ParsedTable{name, columns, foreign_keys}`,
  `ParsedColumn{name, type_name, not_null, primary_key, references}`,
  `ParsedReference{table, columns}` and
  `ParsedForeignKey{columns, table, target_columns}`. The seam is data, not a
  language check in the engine: `Grammar` gained `detail: Option<Detail>`
  (`engine.rs`), `extract` fills the field with
  `grammar.detail.map(|detail| detail(&root, bytes))`, and `registry.rs` binds
  `Some(sql::tables)` for SQL and `detail: None` for the other nineteen
  grammars — the engine still names no language. No serde derive was added
  (`ParsedFile`/`ParsedEdge` carry none and nothing serializes a chapter, so
  the `Option` is the defaulting), no schema change (`schema.rs:449`, `:474`
  untouched), no schema-version bump, no `relations.rs` change. **The grammar
  facts, verified on the pinned `tree-sitter-sequel 0.3.11`** (parse-tree
  probe, not speculation):
  - table-level constraints land as `column_definitions > constraints >
    constraint`, **not** as `_primary_key_constraint`/`_key_constraint`:
    those grammar rules are hidden (leading `_`) and inlined, so the node kind
    to match is `constraint` and the keywords tell its forms apart
    (`keyword_primary`; a `keyword_key` with `keyword_references` is a foreign
    key). This is the finding T2 builds on;
  - a composite `PRIMARY KEY (id, label)` arrives whole (two `column` children
    of `ordered_columns`) and is carried as `primary_key: true` on each of its
    columns; mapping a composite key onto the `er` document's single-key
    contract is **not** decided here (T2's S5);
  - an inline `REFERENCES store (id)` and a table-level `FOREIGN KEY (code)
    REFERENCES store (name)` both carry their target table and target columns;
  - a *named* `CONSTRAINT fk FOREIGN KEY …` parses as an `ERROR` node at this
    version and yields no detail (documented in `sql.rs` beside the older gap
    note); a named `CONSTRAINT pk PRIMARY KEY (…)` does parse and is read like
    the unnamed one;
  - `CREATE TABLE … AS SELECT` writes no column list, so it stays a `type` of
    the chapter and is not an entry of the detail;
  - the chapter's `uses` edges are unchanged (the three forms `visit` already
    read): a table-level FK is detail only, and whoever draws that relation
    reads it from `ParsedTable::foreign_keys`, not from an edge.
  **RED→GREEN.** RED (1), the fixture test written first:
  `cargo test -p gx-index --test sql_detail` → exit 101,
  `error[E0609]: no field 'tables' on type 'ParsedFile'` (four times). RED (2),
  with the field and the plumbing in place but no grammar bound: same command
  → exit 101, the three detail tests panicked at «a SQL chapter carries the
  detail of its tables» (the PRESERVE test passed). GREEN: `sql::tables` +
  `Some(sql::tables)` → exit 0, 4 passed. The fixture
  (`crates/gx-index/tests/sql_detail.rs`) is two tables: an inline
  `PRIMARY KEY`, a `NOT NULL`, a plain nullable column, an inline
  `REFERENCES store (id)`, a table-level `PRIMARY KEY (id, label)` and a
  table-level `FOREIGN KEY (code) REFERENCES store (name)`, plus the assertion
  that the chapter's symbols are still only the two `type`s (S2). **Gates**
  (real exit codes): `cargo test -p gx-index` 0 failed (4 new tests; the two
  `languages.rs` SQL tests passed unchanged); `cargo test --workspace` exit
  101 — `gx-app` cannot compile because `frontend/dist` is not built in this
  worktree (gitignored build artifact, pre-existing, untouched by T1);
  `cargo test --workspace --exclude gx-app` exit 0, 0 failed; `cargo fmt
  --check` exit 0; `cargo clippy --workspace` exit 101 for the same
  `frontend/dist` reason, `cargo clippy --workspace --exclude gx-app
  --all-targets` exit 0 and `cargo clippy -p gx-index --all-targets` exit 0.
  **Covered**: S3 (extraction), S4 (the detail structure), S7 (the first
  fixture test; the two `languages.rs` SQL tests stay green unchanged). S4's
  documented limits: a named `CONSTRAINT <name> FOREIGN KEY` is not readable at
  this grammar version, and the composite PK is carried whole as markers whose
  ER mapping is T2's. **Deliberately not built**: no bridge to
  `gx_kinds::er::ErInput`, no renderer or frontend change, no `gx-kinds`
  change, no new wire message, no schema or schema-version change, no new edge
  kind, no serde derive.
