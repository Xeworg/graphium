# f15-er-project — one schema, many `.sql` files

The `er` document stops being a one-file view: `code_er` gains an optional
`scope` — `"file"` (default, today's answer byte-for-byte) or `"project"`
(the detail of every `.sql` chapter of the book, merged, deduplicated by
table name, with cross-file foreign keys anchored at last). The audit's
largest coverage win, without touching the `er` contract or the schema.

Branch `feature/f15-er-project` from staging `d3c3386`, worktree
`~/Proyectos/graphium-f15`.

## Specs

- **S1** Owner: «dale a f15» — approving the audit's candidate and the
  synthesis wording: «f15: `er` multi-archivo — mergear el detalle de todos
  los `.sql` del libro antes de `document()`; el win de cobertura más
  grande sin tocar el contrato er». The finding quoted: «a FK to a table
  declared in another `.sql` silently vanishes» and «per-file migrations
  are the normal shape of a real schema» (A §4.1, er_bridge.rs:56-62).
- **S2** `code_er` grows one optional argument, `scope`: `"file"` (the
  default — the answer is exactly today's, one file in, the bridge's own
  document out) or `"project"` — every `.sql` chapter the book knows is
  parsed **live** from disk, the details are merged in the book's chapter
  order, duplicate table names keep the first occurrence (the existing
  `drawable()` dedupe, now doing cross-file work), and `document()` builds
  one document from the merged detail. `path` stays **required** for both
  scopes (the declined auto-discovery of f13 stays declined); under
  `"project"` the named file's tables are part of the merge like any
  other chapter's.
- **S3** The chapter list comes from the book (the same source that knows
  which files are chapters), never from a directory walk; a chapter that
  no longer reads from disk is skipped, and the answer counts what it
  merged: `files` (chapters parsed for this document) beside f14's
  `tables`/`relations`/`chars`. The `er_over_kind` warning applies to the
  merged document unchanged (f14).
- **S4** The merge lives in `gx-index` beside the bridge (a
  `project_document`-shaped function over the merged detail, or
  `document_json` fed the merged slice), so the door never grows a second
  mapping: what it answers is still the bridge's output, and the existing
  test comparing the answer with the bridge keeps passing for
  `scope: "file"`.
- **S5** Cross-file anchors work: a FK in one file referencing a table
  declared in another resolves under `"project"` (the `anchor`/
  `primary_key` pair already takes the merged slice). The contract
  unchanged: single-column PK anchors, composite keys still skip, named
  FKs still lose their name.
- **S6** Additive only: `scope` absent means today's answer byte-for-byte
  (a differential over the base tree proves it); `er-posts` golden
  untouched; the frontend CONTRACT.md untouched; the new argument is
  documented in `tool_schema`, the description (+ the duplicated const,
  byte-identical) and the PRD row/prose.
- **S7** Gates: RED→GREEN per task; `cargo test -p gx-mcp --lib`, `cargo
  test --workspace` 0 failed (goldens intact), `cargo fmt --check`,
  `cargo clippy --workspace`, and the base-vs-working differential for the
  default-scope invariance.

## Tasks

- **T1** (S4,S5) The merge in `gx-index`: the merged-detail document
  function beside `chapter_document`, with its own tests (cross-file FK,
  dedupe, skip-no-detail file). Route: `crates/gx-index/src/lang/er_bridge.rs`
  + `crates/gx-index/tests/sql_er_bridge.rs`. Commit: (pending)
- **T2** (S2,S3) The door: `scope` argument (dispatch + `tool_schema`),
  `code_er_query_with` project branch (chapter list from the book, live
  reads, `files` count, warning reuse), RED fixtures first (two files with
  a cross-file FK; deleted file; default-scope invariance). Route:
  `crates/gx-mcp/src/{lib.rs,dispatch.rs}`. Commit: (pending)
- **T3** (S6) Harness + PRD text. Route: `harness.rs`, `lib.rs` const,
  `docs/PRD.md`. Commit: (pending)
- **T4** (S7) Full gates + differential + Log. Route: this file. Commit:
  (pending)

## Log

- **L1** (2026-11-08, owner): «dale a f15» — after f14 merged (staging
  `d3c3386`, binary `26dff4f0`). The declined auto-discovery stays
  declined: `path` remains required; `scope` chooses the breadth, the
  file stays the anchor.
- **L2** (2026-11-08, orchestrator): T1-T4 landed as **one work unit**
  (665 authored lines, commit 6d53181). **T1 was a pin, not a RED**: the
  bridge already supported the merged slice (`document_json`'s
  `anchor`/`primary_key` resolve over the whole slice), so the
  multi-chapter semantics landed as gx-index tests (162 lines) with no
  production change there — the worker reported this honestly instead of
  manufacturing a failure. **The door**: one minimal read-only method,
  `Book::paths_of_language` (book.rs, 17 lines), answers which files are
  chapters of a language; `file_document` preserves f13's answer verbatim
  in its own branch (structural invariance, plus the differential), and
  `project_document` shares the f14 counts/warning path with the new
  `files` field. **The differential was run before and after the change**
  (separate `CARGO_TARGET_DIR` per tree, base = staging `d3c3386`):
  scope-less answers identical md5 on both sides — the S6 invariance
  claim, measured. **Gates** (orchestrator-verified on the finished
  tree): gx-index suite green, `cargo test -p gx-mcp --lib` **209
  passed**, workspace **40 suites, 0 failed** (goldens intact, no
  `UPDATE_GOLDEN`), `cargo fmt --check` 0, `cargo clippy --workspace` 0
  warnings. **Deliberately not built**: trimming the merged document to
  the ceilings (the `er_over_kind` warning of f14 is the honest signal;
  the render remains the authority), auto-discovery (declined, f13 L1),
  and detail persistence in the schema (the live read stays the rule).
  The cut closes on its branch; the merge to staging is the owner's
  call.
