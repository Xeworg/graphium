# f14-answer-sizes — the book answers carry their size and their honesty

Two answer-contract additions, both additive, both closing findings of the
three-agent audit: `code_er` reports its size (counts + `chars` + an
up-front warning when the document exceeds the `er` kind's ceilings, so the
agent learns it before the wasted render round trip) and `code_diagram`
publishes `unresolved` like `code_impact` already does — "no relations" and
"nothing resolved" stop looking identical.

Branch `feature/f14-answer-sizes` from staging `b700559`, worktree
`~/Proyectos/graphium-f14`.

## Specs

- **S1** Owner: «continua con f14» — «contrato de tamaño para `code_er` +
  `unresolved` en `code_diagram`», the approved synthesis: «conteos +
  envelope o `too_large` temprano en la puerta» (B#1) and «`unresolved` en
  `code_diagram` — un campo, espejo de `code_impact`» (A#2/B#7).
- **S2** `code_er`'s answer grows three additive fields beside the unchanged
  `diagram`: `tables` (count), `relations` (count) and `chars` (the
  serialized size of the document, exactly what `code_structure`'s `chars`
  means). The document itself is never trimmed — it stays the bridge's own
  output, byte-for-byte `chapter_document`'s (f13's contract, L2).
- **S3** When the document exceeds the `er` kind's ceilings — the same
  predicate the render's `too_large` applies (gx-kinds `er.rs:118-124`, via
  whatever API `gx-mcp` can reach; if the exact validator is not reachable,
  the kind's published limits are applied and the code comment says so) —
  the answer gains `warnings[]` with code `er_over_kind`, `ok` stays `true`
  and the document still travels. The warning's hint tells the agent the
  render will refuse it with `too_large` and to split the schema.
- **S4** `code_diagram`'s answer grows `unresolved` (an integer, same name
  and same meaning as `code_impact`'s at lib.rs:654): the count of names the
  walk could not pin down. No change to the document itself.
- **S5** The harness keeps pace: `description("code_er")` and its duplicated
  const `CODE_ER_DESCRIPTION` (byte-identical) mention the counts, the
  `chars` field and the `er_over_kind` warning; `code_diagram`'s
  description mentions `unresolved`. INSTRUCTIONS' book paragraph only
  where a clause is needed. PRD: the `code_er` and `code_diagram` tool
  table rows and the F9 prose gain the fields.
- **S6** Additive only: existing answers keep every field they had (the
  parity of the other tools untouched), `er-posts` golden byte-identical,
  the frontend CONTRACT.md untouched, no schema.rs change.
- **S7** Gates: RED→GREEN per task; `cargo test -p gx-mcp --lib` (plus the
  new tests), `cargo test --workspace` 0 failed, `cargo fmt --check`,
  `cargo clippy --workspace` clean.

## Tasks

- **T1** (S2,S3) `code_er_query_with`: counts, `chars`, the over-kind
  warning — RED fixtures first (one small schema; one generated schema over
  the ceilings). Route: `crates/gx-mcp/src/lib.rs` + tests. Commit: (pending)
- **T2** (S4) `code_diagram_query_with`: publish `unresolved` — RED first
  (fixture with an unresolvable call). Route: `lib.rs` + tests. Commit:
  (pending)
- **T3** (S5) Harness + PRD text. Route: `harness.rs`, `lib.rs` const,
  `docs/PRD.md`. Commit: (pending)
- **T4** (S6,S7) Full gates + Log. Route: this file. Commit: (pending)

## Log

- **L1** (2026-11-08, owner): «revisa ahora luego haz el push a staggin y
  continua con f14» — push executed (`origin/staging f874530..b700559`),
  f14 opened as approved. The findings this cut closes are quoted from the
  audit: «code_er can hand an unrenderable document» (B §3.1: no envelope,
  no counts, `too_large` only at render — one guaranteed-wasted round trip
  and no remedy offered) and «`unresolved` only on `code_impact` …
  `code_diagram` runs the same walk and reports none, so the picture looks
  surer than the query that produced it» (B §3.7; A §3 «the document path
  is the one place it currently stays silent»).
- **L2** (2026-11-08, orchestrator): T1-T3 landed as **one work unit**
  (229 authored lines, commit 1a347c0). The worker found the render's
  `too_large` predicate private (`gx_core::engine::check_limits`), so the
  door re-applies the registry kind limits to the document counts with a
  comment saying so — the fallback S3 allowed; the render stays the
  authority that refuses. It also ran the base-vs-working differential
  over a separate `CARGO_TARGET_DIR`, which proved the change
  additive-only (the S6 claim, measured). The orchestrator verified the
  gates again on the finished tree: **40 workspace suites ok, 0 failed**
  (goldens intact, no `UPDATE_GOLDEN`), `cargo fmt --check` 0,
  `cargo clippy --workspace` 0 warnings. Verified wording: `unresolved`
  on `code_diagram` is pinned equal to `code_impact`'s for the same walk
  (lib.rs test ~4007); the `er_over_kind` test builds a schema over the
  ceilings, asserts `ok == true`, the warning, the hint naming
  `too_large`, and all tables still travelling. **A commit that had to be
  redone**: the first attempt's message broke under shell escaping and the
  docs commit absorbed the code files; `reset --mixed` + `commit -F`
  redid it as two honest commits (1a347c0 feat, this file docs). The cut
  closes on its branch; the merge to staging is the owner's call.
