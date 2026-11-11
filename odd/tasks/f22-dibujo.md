# f22-dibujo

What the drawing does not say: the eight items the critical review deferred
with their design decisions now taken by the owner. Branch
`feature/f22-dibujo` stacked on `staging` @ 4ac2730, worktree
`~/Proyectos/graphium-f22`.

## Specs

S1 (user, verbatim): «empecemos con f22» — the cut scope is the eight items
f21-roto declared out of scope: "the time axis, role glyphs (initial vs
final, class stereotypes), milestone rendering, class members, swimlane
connectors (a contract change), diamond/cylinder label geometry, flowchart
stubs, sequence activations."

S2 (eje de tiempo, owner decision verbatim: «Eje con marcas y valores»):
`timeline` and `gantt` draw a time axis — a line under the last lane with a
tick per column and the column's value written at the tick (`order` for
timeline events, `start..end` for gantt tasks). No axis today: events and
bars position by column but the drawing never says what the columns mean.

S3 (UML de estado, owner decision verbatim: «UML de estado»): the `initial`
role draws a solid filled dot and the `final` role draws a dot inside a ring
(both are the same empty ellipse today). Drawing-only change: the state
contract does not change.

S4 (estereotipos de clase, owner decision verbatim: «Estereotipos de
clase»): the box of an `abstract` or `interface` role carries its word above
the node name («interface», «abstract»), as UML marks stereotypes. Drawing
only; `class` carries nothing, `enum` keeps its own shape.

S5 (activaciones de secuencia, owner decision verbatim: «Activaciones de
secuencia»): the `sequence` kind gains the activation the PRD reserves in
`kind_data` (`sequence → {role, reply_to, activation}`, PRD §8.3): an opt-in
field on the message contract that draws a thin rectangle over the lifeline
while the actor holds the control. Contract addition, additive and opt-in.

S6 (hito, owner decision verbatim: «Rol milestone»): a `timeline` event may
declare `"role": "milestone"` — the same mechanism the other kinds use — and
an event with the role draws as a diamond in its column. Timeline gains its
ROLES list (it has none today).

S7 (class members, owner decision verbatim: «Class members»): a class node
may declare attributes and methods; the box draws with compartments. The
renderer already has the rows machinery (`node.rows` → `write_node_table`,
used by er): prefer reusing it over a new table writer. Contract addition,
additive and opt-in.

S8 (conectores swimlane, owner decision verbatim: «Conectores swimlane»):
swimlane steps may connect with arrows between steps, across lanes. Contract
addition (steps carry no connections today); the lanes engine routes them.

S9 (label geometry): the label fits the drawn shape, not just the rect: a
cylinder's label insets by the rim, a diamond's label is measured against the
diamond's usable width at its height. Evidence coordinate: `write_cylinder`
(renderer.rs:603) and `write_diamond` (renderer.rs:646) draw inside the rect
while the text centers on the full rect.

S10 (flowchart stubs): the review's exact finding is not quoted anywhere
surviving — recover it (four-lens review synthesis of 2026-11-09, visual
reader findings) before implementing this item, and write the finding into
the Log. If it cannot be recovered, re-derive it with a visual read of the
flowchart goldens and record that derivation.

Out of scope: f23 (contract ergonomics), push/PR to main (owner's decision,
ola first).

## Tasks

- T0 (S10): recover the flowchart-stubs finding (and the diamond/cylinder
  coordinates) from the review synthesis; evidence into Log.
- T1 (S3, S4): role glyphs without contract change — state initial/final
  UML dots, class stereotypes above the name — gx-render-svg, goldens.
- T2 (S2): time axis under the last lane for timeline and gantt — gx-layout
  lanes engine + renderer, goldens.
- T3 (S6): timeline ROLES + milestone diamond — gx-kinds timeline, renderer.
- T4 (S5): sequence activations — gx-kinds sequence contract + renderer.
- T5 (S7): class members via the existing rows machinery — gx-kinds class
  contract + renderer compartments.
- T6 (S8): swimlane connectors — gx-kinds swimlane contract + gx-layout
  routing + renderer.
- T7 (S9): diamond/cylinder label geometry — geometry.rs + renderer.
- T8: orchestrator — gates, goldens regen + hand review, commits, close.

## Risk

- Item 6: the lanes renderer and goldens are shared by T2/T3/T6 and every
  golden SVG may move; pins are regenerated and reviewed against the spec
  rule, never rubber-stamped. S5/S7/S8 add optional fields only (additive
  contracts, no consumed behavior changes).

## Log

- L1 (2026-11-10, user, verbatim): «empecemos con f22».
- L2 (2026-11-10, owner decisions verbatim): eje — «Eje con marcas y
  valores»; glifos — «UML de estado, Estereotipos de clase, Activaciones de
  secuencia»; hito — «Rol milestone»; contratos — «Class members,
  Conectores swimlane».
- L3 (2026-11-10, derived evidence, S10): the review synthesis did not
  survive, so the flowchart finding was re-derived with a visual read of
  `golden/flowchart-approval.svg` (rendered over #1b1b1b at 144 dpi): (a)
  the decision diamond's label «¿supera el límite?» is wider than the
  diamond at its height — the text overflows past the slanted edges (the
  diamond half of S9); (b) the branch edges leave from the diamond's side
  vertices, producing short diagonal stub segments before the vertical drop
  — flowchart convention branches from the bottom vertex or routes
  orthogonally from the tip. Both coordinates: the diamond dispatch at
  `write_diamond` (renderer.rs:646) + the layered edge routing that anchors
  at side vertices.
- L4 (2026-11-10, design note for T1, decided by the orchestrator within the
  S3/S4 owner decisions): (a) the UML glyphs enter as two new shapes of the
  f19 vocabulary — `Shape::Dot` (filled, wire word `dot`) for `initial` and
  `Shape::RingDot` (dot inside a ring, wire word `ring_dot`) for `final` —
  because shapes are the rendering vocabulary the kinds already map roles
  into (f20: the contract of a kind lives in the kind), and a meta/role
  string interpreted by the renderer would be the prefix anti-pattern f20
  killed; both reuse the Circle envelope (frozen diameter, label beside,
  dot at the left edge so arrows land on it), the ring's inner dot radius is
  half the outer radius. (b) The stereotype travels as `stereotype:
  Option<String>` on `IrNode` and `SceneNode` (additive, `skip_serializing_if
  none`), set by the class kind for `abstract`/`interface`; `measure_node`
  adds its line (0.85× text size) so the box fits it, and the renderer draws
  it above the name with guillemets («interface»), the UML notation. (c)
  Every shape/glyph change lands in both renderers: the Rust emitter AND the
  painter twin `glyph.js` (shapeClass fails loud on unknown shapes), with
  CSS classes in app.css and the export's write_style.
- L5 (2026-11-10, design note for T2, the S2 owner decision): the axis is a
  new layer of the drawing, drawn between the lane bands and the edges —
  `layer-axis`, after `layer-lanes`. The layout computes it (`LayoutAxis`:
  line y below the last lane, x span, one tick per known column at its time
  position, left + half step — never the event box center), the scene
  carries it (`SceneAxis`), the renderer spells `layer-axis` between lanes
  and edges, the painter twin draws the same layer from the same numbers
  (`axisOf` in scene.js fails loud on a malformed one). Columns the document
  never mentions do not tick. The canvas grows by the axis band. Sequence
  draws no axis: its columns are actors, the lifelines are its axis already.

- L6 (2026-11-10, execution, T1-T3): the equivalence gate caught the shape
  class spelling drift (the class CSS is the wire word of `as_str()` —
  `shape-ring_dot`, underscore); the app.css grid gate REDs when the window
  CSS misses a rule the drawing paints; parity C46 caught an undefined
  `shapeOf` killing the whole timeline mount (glyph.js is a pure module:
  helpers it needs must be exported from scene.js and imported). Goldens
  regenerated: state-order draws the frontier glyphs, class-shop the
  stereotypes, timeline-roadmap/gantt-plan the axis, and the new
  timeline-milestone golden joined the suite (20 cases). The milestone
  golden shows the diamond label overflowing its slanted edges — the S9
  finding, for T7.

- L7 (2026-11-10, design note for T4, the S5 owner decision): the activation
  travels the axis's path (L5) — the layout computes, the scene carries
  numbers, both painters spell them. (a) Contract: `Message.activation: bool`
  (serde default false, additive, opt-in): the message that carries it hands
  control to its `to` actor, who draws a thin rectangle over the lifeline.
  (b) IR: the flag rides the edge's `kind_data` (`"activation" = Bool(true)`),
  the scalar channel the IR already reserves for kind-specific data — no new
  `IrEdge` field for one kind. (c) Layout (`LanesEngine::layout`): a bar per
  activating message, on the `to` actor, spanning from its own row to the next
  message the actor **sends** (`from == actor`), so control is held until the
  answer leaves; without one the bar covers only its own row. Rows bracket
  their arrows: y from `row_y - 0.35*step` to `end_row_y + 0.35*step`. Bars of
  the same actor merge when they touch (a second activation inside a held
  control extends the bar, never doubles it). Output: `LayoutActivation { x,
  y1, y2 }` — the lifeline x is the actor's center, the same number
  `lifeline_x` uses — on the new `LayoutGraph.activations` (empty for every
  other engine, `Vec::is_empty` skipped in the payload, additive like the
  axis). (d) Painters: the export draws `<rect class="activation">` inside
  `layer-lifelines`, after the lines and under the arrows; width is a drawing
  constant (`ACTIVATION_WIDTH = 10.0`), the CSS rule joins `write_style` and
  `app.css`. (e) The painter twin parses `activationsOf` fail-loud (default
  `[]` when the key is absent: old payloads stay valid) and paints the same
  rects. (f) Evidence: golden `sequence-activation` joins the suite (21
  cases), parity adds C47 (the bar mounts with the scene's numbers), and the
  contract test pins the additive field plus the span rule.

- L8 (2026-11-10, execution, T4): the activation landed on every layer of
  the axis's path. `IrEdge` grew the `kind_data` channel `IrNode` and
  `IrEvent` already had (the scalar channel, `skip_serializing_if` empty —
  the asymmetry was the discovery: the flag had nowhere to ride). The span
  rule is the one L7c names: bar from the activating row to the actor's next
  outgoing message; two activations inside a held control merge. The width
  travels as a frozen token (`Tokens`/`SceneTokens.activation_width` = 10.0,
  all-or-nothing in the twin's `tokensOf`, mirrored in `gx-mcp`'s stub and
  parity's fixtures). Evidence: gates fmt/clippy 0, 832 tests (new: contract
  in `sequence.rs`, four bar-geometry tests in `lanes.rs`, the painter test
  in `renderer/tests.rs`), parity 72/72 with the new C47, equivalence 21/21
  with the new `sequence-activation` golden (its two bars: API rows 1-2,
  Base rows 2-3). The export golden of the nested clusters moved — the
  stylesheet is the drawing's, so the new `.activation` rule regenerates it;
  every scene json moved with the token. Trap avoided (L6): `shapeOf` was
  the last pure-module lesson; this time `activationsOf` went into
  `scene.js` first and `DiagramScene.svelte` reads the paint plan, not the
  module.

- L9 (2026-11-10, design + execution, T5): the members reuse the rows
  machinery whole — no new table writer. (a) The kind owns its contract:
  `ClassNode` carries the flow fields plus `attributes`/`methods` (opt-in,
  `deny_unknown_fields` keeps guarding; the shared `FlowNode` is untouched —
  `#[serde(flatten)]` cannot carry it). A blank member is rejected the way a
  blank label is. (b) The IR row gained `divider: bool` (serde default
  false): the compartment rule is the line above the first method, and the
  header rule is the first compartment's on its own — methods without
  attributes divide nothing. (c) Zero measurement change: the rule sits on
  the row's top edge, no extra line. (d) Both painters spell it: the emitter
  in `write_node_table`, the twin in the paint plan (`rowsOf` passes the
  flag, `SceneNode.svelte` draws the line) — same `node-stroke` class, no
  new CSS. Evidence: visual read of the regenerated `class-shop` golden
  (Order: attributes over methods under two rules; Payable: methods only,
  no divider; no overflow), gates fmt/clippy 0, 836 tests, parity 73/73
  (C48), equivalence 21/21. Test-authoring lesson (twice in one task): the
  second node of a renderer test must match the layout's ids — a node the
  layout does not name draws nothing, silently.

- L10 (2026-11-10, design + execution, T6): the connectors ride the timeline
  and the lanes engine routes them. (a) Contract: `Step.connects: Vec<String>`
  (opt-in) — the targets may be declared ids or resolved defaults (`step-N`),
  and the hint of an unknown target names the resolved ids; a step does not
  connect to itself. (b) IR: `IrTimeline.connections: Vec<IrConnection { from,
  to }>` (serde default, skip empty — additive; the gantt arrows stay the
  events' `depends_on`). (c) The lanes engine routes (`connector_route` in
  gx-core): forward flows leave the source's right edge and enter the
  target's left edge with one bend between the boxes; backward flows mirror
  it so the line never crosses a box; same column goes straight through the
  vertical; same row is a straight line. A connection with an end the layout
  did not place draws nothing — the layout is the authority. (d) The
  renderer combines: timeline edges = dependency arrows (gantt) + connector
  routes read from `layout.edges` with the same placement filter, so the
  pairing stays one to one. No payload change: connectors are scene edges —
  the twin draws them with the edge machinery it already has. (e) Evidence:
  the `swimlane-soporte` golden gained the four connectors of the process
  (forward, cross-lane, upward, and the backward loop to the customer —
  visually reviewed, nothing crosses a box), gates fmt/clippy 0, 845 tests
  (four route tests, three contract tests, the scene pairing test),
  parity 73/73, equivalence 21/21. No new parity case: there is no new
  payload field to pin — equivalence on the golden is the twin's witness.

- L11 (2026-11-10, design + execution, T7): the label fits the drawn shape,
  and both fixes are measurement fixes — the painters already center the
  text on the rect, so the box is what had to give. (a) Diamond: it
  inscribes the rect, so at the label's extreme rows its usable width is
  `w * (1 - label_h / h)`; the measured width is the one that makes the
  widest line fit there (`w = label_w * h / (h - label_h)`), replacing the
  padded rect of always for `Diamond` — in `measure_node` for the flowchart
  decisions and in `LanesEngine::event_size` for the milestone events, whose
  own column widens with the box (S5's widening covers it). (b) Cylinder:
  the box grows one rim at each end (`height += 2 * corner_radius`), so the
  label never rides on the rim bulge; the drawn rim caps at a quarter of the
  box, a cap this height always clears. No painter change, no payload
  change: both sides size from the same measured box. Evidence: visual reads
  of the regenerated goldens — `timeline-milestone` («código congelado» and
  «v1.0» fit their diamonds), `flowchart-approval` (the decision's label
  fits the slanted edges: the L3a finding, closed), `dataflow-etl` and
  `shape-gallery` (cylinders with clear air under the rim); gates fmt/clippy
  0, 847 tests (two `measure_node` tests — its first — and the milestone
  route test), parity 73/73, equivalence 21/21.
