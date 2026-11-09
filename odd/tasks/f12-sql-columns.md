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
- L3 (2026-11-09, worker): T2 landed — the SQL detail now has a producer that
  turns it into the `er` document the door already renders. **Placement
  decision (the dependency direction).** `gx-index` is a leaf: `Cargo.toml`
  lists `ignore`, `notify`, `rusqlite`, `serde`, `serde_json`, `sha2`,
  `thiserror`, `tree-sitter` and the grammars, and **no crate of Graphium**,
  while `gx-kinds` depends on `gx-core` and is consumed today only by `gx-mcp`
  and `gx-app`. Making the book depend on the kinds would put the book above
  the diagram kinds and drag the whole registry into every book user, so the
  bridge does not build `gx_kinds::er::ErInput`: it writes **the JSON that
  `ErInput` deserializes** (`er.rs:40-90`, `deny_unknown_fields` included) with
  the `serde_json` the book already has, and the door's own kind parses,
  validates and translates it exactly as it does an author's document. The SQL
  chapter knows the DDL, `er.rs` knows rows and edges, and neither learns about
  the other (S6: no new wire message, no CONTRACT payload change, no
  `er.rs`-shaped type of our own to drift). The producer is
  `crates/gx-index/src/lang/er_bridge.rs` (`document(&[ParsedTable]) ->
  Option<Value>`, `document_json` for the text `render_json` takes); it is
  declared from `sql.rs` with `#[path = "er_bridge.rs"] pub mod er_bridge;`
  because `lang/mod.rs` was not among the delegated surfaces and the bridge is
  not a grammar — it maps **the detail this chapter reads** and nothing else.
  When `lang/mod.rs` is in somebody's surface, the declaration moves there one
  line and the `#[path]` goes. **The mapping (the er contract is law,
  `er.rs:245-260`).** A table with columns is a `tables[]` entry; its columns
  are its rows. `key` is `pk` for a column the DDL marks as a key (inline or
  table level), `fk` for a column the DDL declares as a foreign key (inline
  `REFERENCES` or a member of a table-level `FOREIGN KEY`), `pk_fk` for both —
  the vocabulary `key_of` maps to `RowKey` (`er.rs:100-110`). `nullable` is
  `!not_null && !primary_key`: SQL's own rule is that a `PRIMARY KEY` implies
  `NOT NULL`, so the detail keeps the declaration the DDL writes (S4) and the
  diagram shows the fact of the schema; `er.rs:296-305` turns it into
  `IrRow.nullable` and `er.rs:319-330` derives the optional end of the crow's
  foot from it. `type` is the DDL's type when it writes one, which becomes
  `IrRow.note` (`er.rs:296-305`). A relation is `from` the **referenced table's
  single primary-key column** to `to` the **referencing column** — where
  `er.rs` anchors the edge (`from_row`/`to_row`, `er.rs:311-318`) — and the
  inline form and the table-level `FOREIGN KEY … REFERENCES` produce the same
  relation; `cardinality` is left out on purpose, because `er.rs` derives it
  and writing it would repeat what the document says. **What it refuses to
  draw, and why** (the contract has no way to say it, and a document that says
  it is rejected): a target that is not a primary key (`REFERENCES store
  (name)`, the fixture's table-level key), a target table the detail does not
  carry, a **composite** key (the contract keys a relation on one column, so
  the key columns are `pk` rows and the relation that would point at them is
  not drawn), and a composite foreign key (the contract is column to column).
  The column keeps its `fk` row regardless, because that is what the DDL
  declares. A table with no columns (`CREATE TABLE … AS SELECT`) is not drawn
  as an empty compartment, and a detail with nothing to draw produces no
  document at all. A named `CONSTRAINT fk FOREIGN KEY …` never reaches the
  bridge — the pinned grammar makes it an error node (S3) and the detail
  carries nothing for it. **RED→GREEN.** RED (1), the tests written first:
  `cargo test -p gx-index --test sql_er_bridge` → exit 101, `error[E0432]:
  unresolved import gx_index::lang::sql::er_bridge`. RED (2), with the module,
  its `#[path]` declaration and a `document` that answers `None`: same command
  → exit 101, 5 failed / 0 passed (panics at «the bridge builds an `er`
  document», «the document of the chapter», «the tables with rows still
  draw»). One correction on the way: the first version of the contract-refusal
  fixture pointed at `country.code`, which **is** a primary key, so the
  relation was legal and the fixture — not the bridge — was wrong; it now
  carries the three real refusals (unknown table, non-key target, composite
  key) and asserts no relation is drawn. RED (3), the last guard, test first:
  same command → exit 101, «`shipment.ghost` is not a row of the table, so no
  edge can anchor there: [("shipment", "id", "shipment", "ghost")]» — a
  table-level key naming a column its own table does not have would have made
  the door reject the document (`unknown_ref`), so the bridge now skips it.
  GREEN: same command → exit 0, 6 passed. **Door evidence** (outside the crate, because `gx-index` may not
  depend on `gx-kinds`/`gx-core`): a scratch package in
  `/tmp/f12-t2-door-Hm1EUu` with path dependencies on `gx-core`, `gx-kinds`,
  `gx-index`, `gx-layout` and `gx-render-svg` — `serde_json::from_str::<
  ErInput>` accepted the document, `ErKind::validate` returned ok, `to_ir`
  produced `store{id pk BIGINT/*not nullable*/, name VARCHAR(255), region
  nullable}` and `shipment{id pk, store_id fk not nullable, code fk nullable,
  label pk}`, and one `IrEdge` `store → shipment` with
  `from_row=Some(0)`, `to_row=Some(1)`, `arrow=false`, cardinality
  required-one / required-many (NOT NULL foreign key). The same text through
  `gx_core::engine::render_json` rendered (`2 nodes, 1 edges, 4946 bytes`) and
  the SVG names every table and every row — S6 with the door that already
  exists. **Gates** (real exit codes): `cargo test -p gx-index` exit 0 (6 new
  bridge tests; the four `sql_detail.rs` tests and the two `languages.rs` SQL
  tests pass unchanged); `cargo test -p gx-kinds` exit 0 (97 passed);
  `cargo test --workspace` exit 0, 0 failed (`frontend/dist` is built in this
  worktree, so `gx-app` and its `golden_svg` ran — the `er-posts` golden is
  byte-identical and `er.rs`'s author path is untouched); `cargo fmt --check`
  exit 0; `cargo clippy --workspace` exit 0, no warning. Differential: the base
  commit 6361f58 extracted with `git archive` into `/tmp/f12-t2-base-sE173T`
  (no git state touched) and the change both run `cargo test -p gx-index --test
  sql_detail --test languages` → 47 + 4 passed, 0 failed, exit 0, identical.
  **Covered**: S5 (the bridge and the contract-safe skips), S6 (the document is
  the existing `er` version-1 wire shape and it renders through the existing
  door; no new message, no CONTRACT change), S7 (the two-table FK fixture test;
  `er-posts` byte-identical; the two `languages.rs` SQL tests green, so columns
  are still not declarations). **Deliberately not built**: no dependency from
  `gx-index` on `gx-kinds`/`gx-core` and no `Cargo.toml` change, no engine,
  model or registry change (the bridge is a producer over `ParsedFile::tables`,
  it does not touch the pipeline), no MCP tool, CLI seam or frontend change
  (T3's surface: where the owner meets the bridged document), no mapping of a
  composite key or of a named `CONSTRAINT … FOREIGN KEY`, no `er.rs` change, no
  schema or schema-version change, no serde derive.

- L4 (2026-11-09, worker): T3 landed — the closing task: the surface, the live
  proof and the honest limits. **The seam, and the touch it refused to make.**
  The owner's index→document path already had its two halves: the producer
  (`er_bridge::document_json`, L3) and the door that renders `er` version-1
  documents from JSON (`gx_core::engine::render_json`, and `render_diagram` over
  it). What was missing was the call that turns *a `.sql` file* into that
  document, and it is one function:
  `crates/gx-index/src/lang/er_bridge.rs::chapter_document(path, source)` — the
  chapter's own `lang::parse` plus `document_json`, the path and the text of the
  file in, the text of the document out. It refuses by answering `None` and it
  needs no language check to do it (a chapter carries `tables` only when its
  grammar reads a detail), so a Rust chapter, a `CREATE TABLE … AS SELECT` and a
  path no grammar of the book reads all answer no document — never an empty one.
  **No new wire message, no new MCP tool, no CLI flag and no `CONTRACT.md`
  line**: that file is the frontend↔Rust contract and knows nothing of this
  path, so it is untouched. What is deliberately *not* built is the last mile to
  the agent: a tool that answers the document (a new name, or a `diagram` key on
  the answer of `code_structure`) is a change of the tool payload the agents are
  written against, and the task's non-CONTRACT rule plus S6 ask for none. It is
  the one thing between this seam and the owner asking «the ER of
  `db/schema.sql`» in a plain sentence, and it is named here as the gap rather
  than smuggled in as a tool.
  **RED→GREEN.** RED, tests first:
  `cargo test -p gx-index --test sql_er_bridge` → exit 101, `error[E0425]:
  cannot find function chapter_document in module er_bridge` (three times).
  GREEN: the function → exit 0, 9 passed (the six of T2 unchanged). The three
  new tests: the file in / the document the door renders out (and it is the very
  text `document_json` builds, not a second mapping); a file with no schema
  answers `None` (another language, `CREATE TABLE … AS SELECT`, a path no
  grammar reads); the named-key limit, pinned.
  **A correction the pinned grammar made (S3).** The limit test written first
  asserted the task's sentence — a named `CONSTRAINT fk FOREIGN KEY …` yields no
  detail — and it failed: the detail **does** read it. The parse-tree probe on
  the pinned `tree-sitter-sequel 0.3.11` (`/tmp/f12-t3-live-BwHayH`, `to_sexp`)
  shows why: `column_definitions (constraints (ERROR (keyword_constraint)
  (identifier)) (constraint (keyword_key) (ordered_columns …)
  (keyword_references) (object_reference …) (identifier)))` — the name and the
  `FOREIGN` keyword land in an `ERROR` node and the `(…) REFERENCES …` tail that
  follows still parses as the *unnamed* key constraint, which `table_constraints`
  reads. So the measured truth is: **the name is lost and the key is read**; the
  column is a `fk` row and the ER relation is drawn exactly as for the unnamed
  form (verified two-table, one-line and with `ON DELETE CASCADE`; a composite
  target is still refused by the er contract's single-key rule). The doc now says
  that (it would have been false to say "not supported"), the test pins it
  (`a_named_foreign_key_loses_its_name_and_keeps_its_key`), and S3's sentence
  needs the parent's restatement: the form is unparseable *as a named
  constraint*, not *unreadable as a key*. L2's «yields no detail» was the error,
  and the older note in `sql.rs` about the missing `uses` edge was right all
  along (a table-level key publishes no `uses` edge — detail only).
  **The limits, where the owner reads them.** `sql.rs`'s chapter header now
  carries a plain-language block beside the two grammar gap notes: (1) named
  `CONSTRAINT <name> FOREIGN KEY` — the **name** is not read at this grammar
  version, the key it declares is (the diagram draws it), and the chapter
  publishes no `uses` edge for a table-level key; (2) composite keys are carried
  as one mark per column and the ER mapping is **single-key only** — no relation
  to or from a composite key, the columns keep their `pk`/`fk` rows; (3)
  `CREATE TABLE … AS SELECT` **yields no columns** and is not drawn.
  `er_bridge.rs`'s own "what it does not draw" list and its new surface
  paragraph say the same in the producer's words, and the stale «a named key
  never reaches the bridge» bullet was corrected.
  **Live proof, two ways, real exit codes.** (1) A scratch package outside the
  worktree (`/tmp/f12-t3-live-BwHayH`, path dependencies, never committed) read
  the `.sql` fixture from disk, called `chapter_document("db/schema.sql", …)` and
  rendered the result through `gx_core::engine::render_json` with the product's
  engines: the door accepted the seam's document — 4949 bytes, 2 nodes, 1 edge —
  and the SVG text names `store`, `shipment` and `store_id`; exit 0. (2) The
  real MCP door with `DISPLAY=:0`: the same document through
  `target/debug/graphium --stdio` (initialize + `tools/call render_diagram`, in
  the isolation shape of `scripts/smoke.sh` — own `XDG_RUNTIME_DIR`/`HOME` under
  `/tmp`, owner quit on exit): `rendered=true`, `rendered_kind=er`,
  `diagram_id=d-0001`, 2 nodes, 1 edge — the diagram on the plan, the window
  confirming the paint; exit 0. The binary is the worktree's dev build (13:00,
  before this task's edits); T3 touches no render path, so what it proves is the
  door T3 connects to, and the document is the one current sources produce.
  **A build fact the proof met (not built here).** The workspace declares
  `gx-index` with `default-features = false` and `gx-app` exposes only the five
  C2 languages as features, so the default `graphium` binary has no SQL grammar
  (`cargo build -p gx-app` uses the `gx-index` unit whose features are
  `lang-rust/typescript/python/go/java`). The render door needs no grammar, so
  the proof holds; a product path that *indexes* `.sql` needs the SQL feature at
  build time (`--features gx-mcp/lang-sql`), which is the language lever of
  T4/C3 and not a surface of this task.
  **Gates** (real exit codes): `cargo test -p gx-index` exit 0 (`sql_er_bridge`
  9 passed — the six of T2 unchanged —, `sql_detail` 4, `languages` 47, unit
  89); `cargo test --workspace` exit 0, 0 failed (`er-posts` byte-identical: the
  golden suite passed with no `UPDATE_GOLDEN`); `cargo fmt --check` exit 0;
  `cargo clippy --workspace` exit 0, no warning (plus `cargo clippy -p gx-index
  --all-targets` exit 0). No differential against a base copy was needed: the
  change is additive (one new function plus documentation; no existing function
  body moved) and the base commit is this branch's HEAD.
  **Covered**: S6 (the document of «this `.sql` file» is the `er` version-1 wire
  shape and it reaches the plan through the call that already serves `er`
  documents — proven with `rendered=true`; no new message, no CONTRACT payload
  change), S7 (the seam's tests plus the three `sql_detail` and the two
  `languages.rs` SQL tests green, `er-posts` byte-identical). S1-S5 stand as T1
  and T2 delivered them. **S3 is covered only in its measured form**: the third
  form is documented and pinned, but the claim «not supported» is not what the
  grammar does — see the correction above, and the parent's restatement of S3 is
  the one open item.
  **The feature's state: complete, with one deliberate gap.** «This `.sql`
  file → the ER diagram of it» works end to end in Rust and reaches the plan
  through the existing door; what does not exist — on purpose, because it is a
  CONTRACT touch — is the MCP tool answer that would hand the document to the
  agent without a Rust caller. The doc-only parts of this task (the limits) have
  no RED: they are prose pinned by tests where the claim is observable, and
  nothing in them changes behaviour.
  **Deliberately not built**: no MCP tool or tool-answer field, no CLI flag, no
  `CONTRACT.md` line, no frontend change, no `er.rs` change, no dependency or
  `Cargo.toml` change (no feature for SQL in `gx-app`), no schema or
  schema-version change, no mapping of a composite key or of a composite foreign
  key, and no commit (the orchestrator commits).
