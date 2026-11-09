# f18-gantt

The `gantt` kind (PRD row 20, F3): a project schedule on the lanes engine —
tasks as time spans, lanes as rows, dependency arrows between tasks. Branch
`feature/f18-gantt` from staging `81a6c73`, worktree `~/Proyectos/graphium-f18`.

## Specs

S1 (PRD §6 row 20, verbatim): `| 20 | `gantt` | Cronograma de proyecto, plan, dependencias | lanes | F3 |`

S2 (PRD limits, verbatim): ``gantt`/`timeline`: 200 eventos`` — same family
numbers as `timeline`: `max_nodes: 200, max_edges: 0, max_density: 0.0,
max_lanes: 40, max_events: 200`.

S3 (PRD kind_data, verbatim): ``gantt` → `{start, end, depends_on}``` — the
kind-specific keys of the family table §8.3.

S4 (PRD IR mapping, verbatim): `| `IrTimeline` (lanes) | `sequence`, `timeline`,
`swimlane`, `activity`, `journey`, `gantt` | 6 kinds |` — gantt maps to
`Ir::Timeline` with `LayoutHint::Lanes`. A task becomes an `IrEvent` whose
`span` is `Some(Span { start, end })` and whose dependencies travel in the
additive field `IrEvent.depends_on: Vec<String>` (amended by L3: `IrValue` has
no list variant, so `kind_data` cannot carry an id list; it stays empty). No IR
variants gained; no layout engine change.

S5 (contract): version 1, `kind: "gantt"`, optional `title`, optional `lanes`
(same `Lane` shape as timeline), `tasks[]` with `id` (optional, default
`task-<n>` like the timeline `event-` rule), `label`, `lane` (optional; the
timeline rule about lanes applies), `start` (i64, required), `end` (i64,
required; `start == end` is a milestone point), `depends_on` (optional array of
task ids). `deny_unknown_fields`. Family-level validations identical to
`timeline`: duplicate ids rejected, unknown lane rejected, single implicit lane
`cronograma` when the document declares none.

S6 (discipline of dependencies, no new error codes): a `depends_on` entry
naming an unknown task id → `unknown_ref` naming the missing id (the family
precedent of the c4 relations, amended by L4); a task depending on itself →
`invalid_value`; a duplicated id inside `depends_on` → `invalid_value`; `end <
start` → `invalid_value` naming the task. Cycles are drawn, not validated, in
v1: the lanes layout is columnar, not topological.

S7 (render): dependency arrows are scene edges of a timeline. The scene builder
(`crates/gx-render-svg/src/scene.rs`) derives, for every event whose
`kind_data` carries `depends_on`, one `SceneEdge` per declared dependency —
from the dependency's event box to the task's box, arrowhead on, no label, in
deterministic order (tasks in id order, dependencies in declared order), ids
via the existing `scene_edge_id` pattern. The route is a deterministic elbow
computed from the two boxes (same spirit as `event_bar`: frozen numbers, no
guessing). Both emitters draw them through the existing edges layer; the plan
comment "A timeline has no edges" is rewritten to state the new rule. No new
tokens, no `gx-layout` change; `gx-core` stays untouched unless the worker
proves otherwise (report back before widening the surface).

S8 (surface): `gantt` registers right after `c4` (next position in
`register_all` and `kind_names`); `get_diagram_schema` lists it from the
registry with the family ceilings of S2; `crates/gx-mcp/src/contract.rs` arms
`min_example_of` and `rules_of` for gantt — the rules document `start`, `end`
and `depends_on`; the schema pin test picks it up.

S9 (golden): case `gantt-plan` (4 tasks across 2 lanes, 2 dependencies, one
milestone with `start == end`) feeds the four golden tests. Adding the case
requires BOTH fixtures — `golden/gantt-plan.svg` and
`golden-scenes/gantt-plan.json` — or the run fails with "the scene fixture is
missing". Regenerate with the established `UPDATE_GOLDEN` / `UPDATE_SCENES`
flow and review both pins by hand before committing.

Out of scope for v1 (deliberately not built): visual nesting, a `parent`
field, cycle detection, percentages/progress, calendar awareness (dates as
numbers, not days).

## Tasks

- T1 (S1-S6): contract RED→GREEN — `crates/gx-kinds/src/gantt.rs`,
  registration in `crates/gx-kinds/src/lib.rs`, `crates/gx-kinds/tests/gantt.rs`.
- T2 (S4, S7, S9): `to_ir` with spans + dependency arrows in the scene + the
  `gantt-plan` golden case — `crates/gx-render-svg/src/scene.rs` (and its
  tests), `crates/gx-app/tests/golden_svg.rs` + both fixtures. RED first: a
  scene test asserting that a timeline whose events carry `depends_on` yields
  the edges.
- T3 (S8): MCP surface — `crates/gx-mcp/src/contract.rs`, `tools.rs`, schema
  pin.
- T4 (S1-S9): orchestrator — gates on the worktree, work-unit commits, doc
  close. `docs/PRD.md` needs no new row (row 20 exists); counts stay untouched
  (same policy as f17).

## Log

- L1 (2026-11-09, user, verbatim): «por donde quedamos» — session resume; the
  day-close record names this cut: «PRÓXIMA SESIÓN: f18 gantt con
  dependencias (extiende timeline con event_bar de f11), luego f19 git graph,
  f20 deployment, mejoras (ambigüedad, bm25, quirks SQL)».
- L2 (2026-11-09, user, verbatim): «merge a staggin y continua con lo
  siguiente» — wave executed (PR #55, binaries rebuilt, daemon restarted) and
  the next cut, f18-gantt, opened.
- L3 (2026-11-09, discovery): the writer gate sent the unit inline (the
  orchestrator implemented T1-T3 itself). Proof for widening the surface (S7
  allowed it): `IrValue` has no list variant, and encoding ids in one string
  collides with free-form ids, so `depends_on` travels in the new additive
  `IrEvent.depends_on: Vec<String>` (`#[serde(default)]`, 9 existing literals
  updated mechanically). `kind_data` stays empty for gantt; the PRD §8.3
  `{start, end, depends_on}` keys are taught in `rules[]` instead.
- L4 (2026-11-09, decision): the unknown-dependency error uses `unknown_ref`,
  not the `invalid_value` first written in S6 — the family precedent is the
  c4 relation errors, and the code already exists. S6 rewritten; no new error
  codes, spirit unchanged.
