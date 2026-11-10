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
