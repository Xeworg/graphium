# f20-reuso

The consolidation cut the three reuse audits called for: every hand-copied
block the audits found goes to its shared home, the contract stops deriving
nothing, and the rule gaps close. Branch `feature/f20-reuso` stacked on
`feature/f19-formas` @ f4dac6d (the files overlap), worktree
`~/Proyectos/graphium-f20`.

## Specs

S1 (user, verbatim): «haz una revision de los diagramas previos para encontrar
cuales diagramas tienen codigo que podrian reutilizarse en otros, pon 3
agentes explore»

S2 (user, verbatim): «arreglalos todos» + «aplica todos los cambios quise
decir» — every finding of the three audits (kinds internal, render layer,
contract surface) is applied, none deferred.

S3 (header truth): `flow::validate_header` is the one header validator; the
seven copies (graph, tree, timeline, swimlane, sequence, er, mindmap) go away.
The drift dies with them: every kind emits `invalid_value` on an unsupported
version (er's `unsupported_version` was the outlier) and every kind trims the
declared kind.

S4 (limits truth): the two LIMITS profiles become named consts
(`graph_limits` 300/600/3.0, `timeline_limits` 200/0/40/200) and the fourteen
kinds declare which profile they are.

S5 (ids truth): the id-uniqueness loops of graph, c4, er, sequence and the
gantt `depends_on` dedup go through the generalized `SeenIds`/`duplicate`
helpers; the er drift dies — column names get the same emptiness check table
names had.

S6 (graph skeleton): `flow::to_ir_graph(direction, hint, nodes, edges,
groups)` is the one `IrGraph` builder; the seven hand rebuilds (graph, class,
er, c4, tree, mindmap, sequence) and the four `groups =` epilogues go away.

S7 (lanes preamble): `lanes.rs` owns the lane-validation match (with the
kind's noun), the single-lane default and the per-event lane fallback;
timeline, gantt and swimlane consume them.

S8 (flowchart start): the rule gap closes — a flowchart declares exactly one
`start`, the same rule state already enforces for `initial`. This is an
intentional contract change: documents without a start are rejected from this
cut on, with a hint that names the rule.

S9 (examples live with the kind): `DiagramKind::min_example()` (default
`Value::Null`... no — default panics? no: the registry pin catches it) — the
examples move from the 200-line match in gx-mcp to a trait method; O7 holds
again: a new kind never touches gx-mcp, and the index shows its example.

S10 (contract truth round two): `admits_groups` derives from the kind's own
schema (`groups` in properties); `PHASE` is a trait const beside `KIND` (no
`_ => "F3"` fallback); `kind_names()` derives from the registry (a registered
but unlisted kind can no longer escape the per_kind suite); `LimitsPayload`
exposes `max_lanes` and `max_events`.

S11 (rules derive): `Role` moves to `gx-core/src/kind.rs`; `DiagramKind`
gains `roles()` and `rules()`. gx-mcp keeps only the generic base rules
(version, unknown fields) and the groups rule; the role→shape line derives
from `roles()`; the per-kind teaching (state's one initial, c4's no-nesting,
gantt's spans/cycles, timeline's order/end, class acyclicity, dataflow flow,
swimlane lanes) lives in each kind's `rules()`. c4's level→types sentence is
spelled once, generated from `types_of`.

S12 (render folds): `write_lines` folds the three copies of "one text, N
lines" (centered, beside, table header); the cluster/lane band boxes share the
`Band` writer with `geometry::band_label_at`; `SceneBand` gains an additive
`kind` field and the painters stop prefix-parsing ids; `write_event` uses the
shared box writer.

S13 (badge truth): a row without a key draws no badge — the empty
`row-badge` `<text>` elements disappear from c4-context, er-posts and every
other table that paid er's ornament. Both emitters change together; goldens
regenerate.

S14 (wontfix, recorded): the glyph twins stay (pinned by equivalence, 3–6
formulas each — a `glyph` payload would widen the wire for little); the shape
dispatch ladder stays (the Rust match is the compiler-forced checklist);
`SceneBand.members`/`gutter` stay (they document the drawing even though no
painter reads them yet); the crow's foot and `Table::of` twins stay (the
heaviest, but their divergence is exactly what equivalence pins).

## Tasks

- T1 (S3-S8, er width): gx-kinds mechanical consolidation + the flowchart
  start rule — flow.rs, lanes.rs, kind.rs consts, the seven header kinds, the
  id-loop kinds, to_ir_graph adopters, timeline/gantt/swimlane lanes,
  flowchart validation + tests, er dead code, test helper dedup.
- T2 (S9-S11): examples + phases + admits_groups + kind_names + LimitsPayload
  + Role/rules migration — gx-core kind.rs, gx-kinds (examples + rules move),
  gx-mcp contract.rs.
- T3 (S12-S13): render folds + badge truth — gx-render-svg renderer/scene,
  gx-core scene.rs (SceneBand.kind), glyph.js/scene.js/SceneNode.svelte,
  goldens regen.
- T4: orchestrator — honest gates (workspace + parity + equivalence),
  work-unit commits, doc close.

## Risk

- Item 3: two agent-visible contract changes land deliberately — flowchart
  rejects start-less documents (S8) and `rules[]` wording becomes derived
  (S11); pinned by updated tests.
- Item 5: ci.yml untouched this time; the O7 line-based gate keeps passing
  because gx-mcp's remaining kind matches (none) shrink to zero.

## Log

- L1 (2026-11-09, user, verbatim): S1.
- L2 (2026-11-09, user, verbatim): S2 — the three audits (kinds internal,
  render layer, contract surface) are the finding list; every finding applied.
- L3 (2026-11-09, scoping): the cut stacks on f19-formas (overlapping files);
  S8 is an intentional flowchart contract change aligned with state's
  initial rule; S14 records the four wontfix findings with reasons.
- L7 (2026-11-09, execution): the cut ran parallel — T2 by worker A and T3 by
  worker B in isolated worktrees (t2/t3) with disjoint file-level surfaces,
  merged sequentially into the base; the orchestrator applied T1 inline and
  the blocked half of S12 (SceneBand.kind) after both merges. Two of the
  workers' honest findings turned into fixes: worker A verified that T1's
  ir_graph TB default silently flipped er's LR contract (fixed at the call
  site, pinned by a unit test — the er-posts golden caught it), and the
  revived gates caught the stale-dist trap (parity/equivalence ran green
  against a bundle built before the merges; after rebuilding, C46 exposed the
  kind-less band fixture, which now speaks the contract). Final gates on the
  merged base: fmt OK, clippy 0, cargo test --workspace 42 suites 0 failed,
  parity 71/71, equivalence 19/19.
