# f17-c4-kind — C4: the architecture diagram agents actually ask for

A new kind, `c4`: the Context/Containers/Components model with a declared
level disciplining which element types are legal at each depth. The first
new kind since the original twelve, and the one an agent reaches for when
the human asks "explícame la arquitectura".

Branch `feature/f17-c4-kind` from staging `f18203c`, worktree
`~/Proyectos/graphium-f17`.

## Specs

- **S1** Owner (2026-11-08): selected «C4 / arquitectura» as the next new
  kind from the product menu — «Contexto → contenedores → componentes: el
  estándar moderno para que un agente explique la arquitectura de un
  sistema».
- **S2** The contract (`crates/gx-kinds/src/c4.rs`, registered like `er`):
  a document is `version: 1`, `kind: "c4"`, `title`, `level` (one of
  `"context" | "containers" | "components"`), `elements[]` and
  `relations[]`. Each element: `id`, `type` (`"person" | "system" |
  "container" | "component"`), `label`, optional `description` and
  `technology`. Each relation: `from`, `to`, optional `label` (e.g.
  "uses"). Unknown fields rejected; ids unique; every relation references
  elements present in the document.
- **S3** The level disciplines the types — context: `person` + `system`;
  containers: `system` + `container`; components: `container` +
  `component`. A type the level does not admit is a validation error
  (`invalid_value` at the element, naming the level and its legal types).
  No parent/nesting field in v1: the reading comes from the types, the
  level and the relations.
- **S4** The mapping to Ir follows the house pattern (`er.rs:419` is the
  register template): every element becomes a box node whose label carries
  the type and the label (the er-compartment precedent decides the exact
  row/label shape), every relation becomes a labelled edge. The layout and
  the scene pipeline stay generic — no new layout code.
- **S5** The render door draws it: golden fixture for a small c4 document
  (the double-binary golden flow, `UPDATE_GOLDEN`), and
  `get_diagram_schema` lists `c4` with its limits (the kind declares its
  ceilings like every kind — pick from the family: nodes ≤ 300 / edges ≤
  600 unless the contract says less).
- **S6** The surface teaches it: the harness (`get_diagram_schema` lists
  kinds from the registry — verify it picks c4 up automatically), PRD row
  for the kind, and the error codes it introduces (`wrong_level_type` or
  the equivalent the validator emits) land in the ERRORS teaching only if
  they are new codes.
- **S7** Gates: RED→GREEN per task; `cargo test -p gx-kinds`, `-p
  gx-render-svg`, workspace 0 failed, goldens via `UPDATE_GOLDEN` then a
  clean run, fmt/clippy clean.

## Tasks

- **T1** (S2,S3) The contract: `c4.rs` with the document shape, the level
  discipline, validation, `to_ir`, `register` — RED tests first (a valid
  context document validates; a `container` in `context` fails naming the
  level; a relation to an absent id fails; ids duplicate fails). Route:
  `crates/gx-kinds/src/c4.rs`, `lib.rs` (register_all + kind_names).
  Commit: (pending)
- **T2** (S4,S5) The render: the door draws c4 (golden fixture + the
  schema listing). Route: `crates/gx-render-svg/` (whatever the emitters
  need), golden fixtures, schema. Commit: (pending)
- **T3** (S6) Text: PRD + harness verification. Route: `docs/PRD.md`,
  harness if needed. Commit: (pending)
- **T4** (S7) Full gates + Log. Route: this file. Commit: (pending)

## Log

- **L1** (2026-11-08, owner): «dale» to f17 C4 — the first of the four
  approved new kinds (C4, gantt-with-dependencies, git graph, deployment).
  The design fixed by the orchestrator before opening: declared level with
  type discipline, flat elements, no visual nesting in v1 — the standard
  C4 reading from types + relations + level.
- **L2** (2026-11-09, orchestrator): T1-T3 landed as **one work unit**
  (1,260 authored lines, single feat commit — the kind is one behaviour).
  The contract: 11 c4 tests green (level discipline, absent ids, dupes,
  cross-level rejects), kind 25 registered, `get_diagram_schema` lists it
  with limits and levels (pinned by `the_c4_kind_is_listed_with_its
  _limits_and_its_levels`), render pinned by `the_c4_kind_renders_a
  _context` + the golden case **`c4-context`** (3 nodes, 2 edges, 4,354
  bytes, fnv1a64-pinned; one `CASES` entry feeds four tests: SVG byte
  pin, byte budget, determinism, scenes — the scenes fixture landed
  outside the worker's surfaces, flagged by the worker, accepted by the
  orchestrator). **No new emitter code**: the generic pipeline carried
  the box, header, detail rows and labelled edges (the worker said so
  and the clean golden run proved it). No new error codes (`invalid_enum`
  / `invalid_value`, the family's own). **Gates** (orchestrator-verified
  on the finished tree): workspace **41 suites, 0 failed** (exit 0),
  `cargo fmt --check` 0, `cargo clippy --workspace` 0 warnings, goldens
  clean after `UPDATE_GOLDEN` (dirty→clean per the worker, clean
  re-verified here). The cut closes on its branch; the merge rides with
  the next integration wave (PR from staging after
  `git merge origin/main` back into staging — the post-squash cycle).
  **Deliberately not built**: visual nesting of containers/components
  (v1 reads flat), a `parent` field, persons as special shapes, and any
  C4 model numbers beyond the label/type/technology/description the spec
  fixed.
