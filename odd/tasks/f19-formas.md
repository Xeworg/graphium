# f19-formas

One vocabulary of shapes, made structural: a single source for the wire word,
derived (never hand-copied) class names and schema truth, a live Rust↔JS
parity gate, fail-loud JS, and the `Circle` dot with its beside label — so the
next shape costs one variant plus a glyph, nothing else. Branch
`feature/f19-formas` from staging `bcd822e`, worktree `~/Proyectos/graphium-f19`.
Designed from three independent analyses (architecture, painter parity,
contract ergonomics).

## Specs

S1 (user, verbatim): «debemos tener todas formas basicas, deberíamos
asegurarnos antes de que continuemos agregando diagramas que tengamos toda una
estructura de formas necesarias para reutilizar»

S2 (user, verbatim): «si pero analiza con tres agentes como deberia funcionar
el sistema de formas para que sea mas extensible»

S3 (single source): `Shape` gains `ALL: [Shape; N]` and `as_str()` (mirroring
`Direction::ALL`, `crates/gx-core/src/ir.rs:39`); the wire word is spelled once.
`shape_class` (crates/gx-render-svg/src/geometry.rs:232) derives
`"shape-" + as_str` — the second spelling dies.

S4 (schema truth): `shapes_of` (crates/gx-mcp/src/contract.rs:772-785) stops
being a hand match. The kind handler gains a roles/shapes accessor; kinds
declare their shapes once (role-table kinds derive from `ROLES`; kinds without
one declare what their drawing uses; default `[Rounded]`; c4 `[Box]`). The er
fiction dies: `shapes_of("er")` reports `["box"]`, not `["table","column"]`;
flowchart reports its real five; the 20+ kinds that today report `[]` report
truth. This is an agent-visible contract change, pinned by tests.

S5 (fail loud): the frontend stops degrading silently. `glyph.js:96` loses
`default: "shape-box"`; `scene.js:182` throws on an unknown shape instead of
normalizing to `box`. A shape without a painter branch is a red run, not a
wrong drawing.

S6 (live parity gate): the Rust≡JS shape vocabulary is bound by tests —
`Shape::ALL` == the scene payload's accepted shapes == `shapes_of("graph")`;
`parity.mjs` probes every accepted shape, not only `"box"`;
`equivalence.mjs:295` stops aborting on the stale 15 (it mounts
`CASES.len()`); `.github/workflows/ci.yml` runs `node tools/equivalence.mjs`
next to `parity.mjs`.

S7 (Note pin): the note glyph gets committed evidence — a golden case whose
document draws a note node (`golden/graph-note.svg` + scene), closing the only
shape with zero export coverage.

S8 (Circle): `Shape::Circle` enters the enum as a **dot**: glyph = circle of
the frozen diameter (new token, both languages), label = **beside** the dot.
Terminators stay `Ellipse` — flowchart start/end and state initial/final do
not move, and the prose at flowchart.rs:4 is corrected to say ellipse. The
placement rule lives in `geometry.rs` as a value
(`LabelPlacement::Inside | Beside { at, anchor }`) and travels in
`SceneNode.label_at` + `label_anchor` (the `SceneEdge.label_at` precedent);
both painters read it, the Svelte text painter already honours anchors.

S9 (two boxes): for a dot the measured envelope (spacing, dagre) and the drawn
glyph (anchoring) are two rects derived from one source in `geometry.rs` —
the `event_bar` precedent. `measure_node` keeps measuring the label; the dot
adds its diameter; edges anchor on the glyph rect; neighbours reserve the
envelope.

S10 (surface): `get_diagram_schema` shows `circle` for the kinds that declare
it; the shapes payload of the scene carries the accepted list; harness text
stays truthful («valid shapes» stops being an empty promise); PRD needs no new
row — the shapes vocabulary is §6.1/§11.1 structure, and the PRD's 2–3 day
price for a new shape is exactly this cut.

## Tasks

- T1 (S3-S6): single source + derived truth + fail loud + live gates.
  gx-core ir.rs; gx-render-svg geometry.rs; gx-mcp contract.rs (+handler
  accessor in gx-core/kind.rs); frontend scene.js/glyph.js; parity.mjs;
  equivalence.mjs; ci.yml; tests.
- T2 (S7, S9-S10): Circle + beside + two boxes + goldens. gx-core ir.rs,
  tokens.rs, scene.rs, engine.rs (measure), geometry.rs placement, renderer.rs
  write arm, scene.rs payload, glyph.js/SceneNode.svelte (payload-driven),
  golden `shape-gallery` (all 7 shapes in one document, note included) +
  `graph-note` if separate, scene_payload/grid test updates.
- T3: orchestrator — gates honest, work-unit commits, doc close.

## Risk

- Item 3: `shapes[]` changes agent-visible output (er, flowchart, 20+ kinds)
  and `SceneNode` gains additive fields — pinned by updated schema tests and
  scene_payload tests.
- Item 5: `ci.yml` gains the equivalence step — verified by running the probe
  locally before the commit.

## Log

- L1 (2026-11-09, user, verbatim): S1.
- L2 (2026-11-09, user, verbatim): S2 — three independent analyses ran
  (architecture: keep the enum, kill the copies; parity: the equivalence gate
  is dead and JS fails soft; contract: role vocabulary wins, shapes[] lies
  today). Their findings are the specs above.
- L3 (2026-11-09, decision): terminators stay Ellipse (no existing golden
  churn); `Circle` is a dot with beside label; er's shapes list corrected to
  the truth; JS fails loud on unknown shapes.
- L4 (2026-11-09, evidence): the revived equivalence gate caught a real,
  pre-existing drift on its first full run: the painter's `num()` rounded
  exact halves up (`91.125 → 91.13`) where the canonical formatter writes
  half-even (`91.12`), and four baselines of the c4 golden diverged. `num()`
  now implements the canonical rule of `gx-core/src/fmt.rs`; equivalence is
  18/18 with the fix. The stale 15 abort had kept this hidden since the
  seventeenth scene arrived.
- L5 (2026-11-09, T2): `Circle` shipped as a dot — envelope = dot + gap +
  label (`measure_node`), glyph rect at the left edge of the envelope
  (`geometry::glyph_rect`, so arriving arrows land on the dot), beside label
  start-anchored at `SceneNode.label_at` (additive field, `None` everywhere
  else; the `SceneEdge.label_at` precedent), frozen token
  `circle_diameter: 24.0` travelling in `SceneTokens`, painter branch reading
  the payload. The golden `shape-gallery` pins all seven shapes in one
  document; terminators stayed `Ellipse` per L3.
