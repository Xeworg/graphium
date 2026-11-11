# f23-contrato

Contract ergonomics: the document teaches itself — version truth, machine-usable
errors, examples that show what exists, and no secret conventions. Branch
`feature/f23-contrato` stacked on `staging` @ `111c5bb`. Contract-only: no UI,
no renderer, no breaking renames.

## Specs

S1 (user, verbatim): «explicame el proximo cambio» — the cut is f23 (contract
ergonomics), derived by audit (L2) and approved in full (L3).

S2 (version truth, audit frictions 2 + 10): `get_diagram_schema` publishes the
kind's contract version (`kind_version`) and, per field, `x-graphium-since` —
the mechanism PRD §10.1 (`docs/PRD.md:566`) already promises and the code never
implemented (`grep x-graphium crates/` → zero hits). `UnsupportedVersion`
(`gx-core/src/error.rs:40`) is constructed for the first time: an unsupported
document `version` reports `unsupported_version` at `path: "version"` (PRD
RF5, `docs/PRD.md:188`) instead of the generic `invalid_value` (`flow.rs:196`).
`validate_diagram` fills `supported_versions` (`contract.rs:97,123,143,157`
always returned empty). All current fields are `since: 1` — f22's opt-in
fields are valid in version-1 documents, so the truth table starts uniform and
future additions declare their own since.

S3 (machine-usable errors, audit friction 3): error paths spell one uniform
style — `nodes[0].label` — everywhere, and the two broken constructors die:
the schema-failure collapse that returns `path: ""` with a mixed message
(`gx-core/src/kind.rs:288-301`) reports the serde path; the unknown-field
string surgery that mangles nesting (`kind.rs:309`, `edges.0.label` →
`edges[0[label]`) is replaced by the same uniform style. An empty node/actor
id carries a hint in every kind (`flow.rs:127`, `graph.rs:101`,
`sequence.rs:103`, `c4.rs:183`, `er.rs:133`, `groups.rs:40` have `None`).

S4 (teaching truth, audit frictions 1, 7, 8, 9): the writer-facing surface
teaches what exists — (a) every kind's `min_example` shows its opt-in fields
(`activation`, `attributes`/`methods`, `connects`, `role: milestone`); (b)
`sequence`, `tree` and `mindmap` publish `rules()` like the other eleven; (c)
the `\n` line-break convention is written in the rules and the tool
description (the scene declares labels pre-broken: `gx-core/src/scene.rs:18`);
(d) the tool description exists once and stops asserting "every node needs a
unique `id`" — false for swimlane/timeline/gantt (`gx-mcp/src/harness.rs:1983`
vs `lanes.rs:88-95`).

S5 (cheap contract decisions, audit frictions 4 + 5): document `version`
becomes optional with default `1` (it has exactly one legal value; the kind is
still required — it is the dispatch discriminator); `title`, when present, is
validated to 1–120 visible characters (PRD §10.1, `docs/PRD.md:563` — today
nothing validates it); and the PRD's stale promises stop lying: `notes[]`,
document-level `description` and document-level `emphasis` move from §10.1's
common fields to the F3+ backlog (docs change, no code) until their rendered
semantics exist.

Out of scope: field renames across kinds (breaking, never asked);
`label_max_width` auto-wrap (touches renderer + twin, its own cut); window
work (Ctrl+C source copy, clear-canvas banner, onboarding — §19 rows 1, 4, 6).

## Tasks

- T0 (S2-S5): the audit handoff is the evidence; its friction table lands in
  the Log (L4).
- T1 (S2): kind_version + x-graphium-since in the schema; UnsupportedVersion;
  supported_versions in validate_diagram.
- T2 (S3): uniform error paths; schema-failure paths; empty-id hints.
- T3 (S4): examples, rules(), the \n convention, one honest tool description.
- T4 (S5): version default, title validation, PRD stale-fields correction.
- T5: gates, goldens (schema/examples may move pinned artifacts — review,
  never rubber-stamp), close.

## Risk

- Item 3: the error paths and the schema are contracts other tools already
  consume (`update_diagram` takes an RFC 7386 patch; the MCP schema is the
  agent's only map). The style stays compatible (`nodes[0].label` is what the
  PRD already documents) and every pin that moves is reviewed against the
  rule it pins.

## Log

- L1 (2026-11-10, user, verbatim): «explicame el proximo cambio».
- L2 (2026-11-10, owner decision, verbatim): «Auditar y derivar el alcance» —
  re-read PRD + contract + real documents, propose the spec list before any
  code. The audit ran read-only over staging `111c5bb` (subagent
  gentle-ai-explore) and returned ten frictions with path:line evidence.
- L3 (2026-11-10, owner decision, verbatim): «Aprobar S1-S5» — full scope as
  proposed in the friction table. S5's notes[]/description/emphasis resolves
  to the PRD correction (L2 of the spec text): contract-only, no UI.

- L4 (2026-11-10, execution, T1): version truth landed. (a) The trait gained
  `KIND_VERSION` beside its other consts (a kind that forgets it does not
  compile) — fourteen kinds plus the two test doubles; the registry publishes
  it as `SchemaResponse.kind_version`. (b) `annotate_since` walks the schema
  stamping `x-graphium-since: 1` on every field of every `properties` map —
  top level and `$defs` alike, which is where schemars parks the structs (the
  opt-in `Message.activation` lives in `$defs`). (c) `UnsupportedVersion` is
  constructed for the first time: `validate_header` reports it at
  `path: "version"` (PRD RF5) instead of `invalid_value`. (d) `validate_diagram`
  fills `supported_versions` in both kind-resolved branches. Gates: fmt/clippy
  0, 850 tests (two new contract tests). Trap: the worktree needed the
  `frontend/dist` stub or `gx-app` does not compile (tauri's
  `frontendDist`) — same as f22.

- L5 (2026-11-10, execution, T2): the error paths now name the place. Discovery
  that changed the design: serde's own messages carry NO path at all — verified
  with a scratch crate (`unknown field \`labl\`…`, `missing field \`id\``, no
  location), so the old `replace('.', "[")` surgery could never have recovered
  one. The fix is a schema-guided locator in `parse_input`: when serde rejects a
  document, `first_mismatch` walks it against the kind's JSON Schema (the
  subset schemars emits: `$ref` into `$defs`, `type` as name or list,
  `properties`, `required`, `items`, `additionalProperties: false`) and reports
  the exact path in the same style every kind rule already uses —
  `nodes[0].labl` — with the serde message as the reason. The six kinds whose
  empty-id error had no hint now say what to write. Gates: fmt/clippy 0, 853
  tests (three new: unknown field place, schema-failure place including a
  nested `messages[0].label`, empty-id hint). Test-authoring trap: `graph.nodes`
  carries `serde(default)`, so a document without it is legal — the missing-field
  case needed `version`, which is structurally required.
