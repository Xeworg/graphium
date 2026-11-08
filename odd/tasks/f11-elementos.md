# f11-elementos — la biblioteca de elementos Svelte: fin del string SVG en runtime

Branch `feature/f11-elementos` from `staging` @ f15c1c2. Worktree:
`/home/xeworg/Proyectos/graphium-f11`. Sources: four read-only exploration
rounds (render path, Svelte element model, library decision, tree-sitter
semantics — persisted in project memory, topic `f11-biblioteca-elementos`)
plus the owner directive of 2026-11-08.

## Specs

- S1 (owner, verbatim): «por rendimiento estamos sacrificando escalabilidad,
  no quiero mas render de svg tenemos todo ya para eliminarlo e implementar
  nuestra propia bilioteca de diagramas» — the SVG string leaves the runtime
  path; a structured element payload takes its place.
- S2 (owner, verbatim): «claramente mantendremos la exportacion en png y svg,
  pero esta no sera parte del funcionamiento de la logica» — export stays in
  Rust (gx-render-svg emitter + golden_svg.rs gate) as an output artifact,
  never part of the paint path. `export.rs` keeps re-rendering from the
  stored document; the frontend never produces exported bytes.
- S3 (schema principle, from analysis): the element schema MIRRORS
  `Ir` ∪ `LayoutGraph` — it invents no third vocabulary. Ids stay
  `g-<id>`/`m-<from>-<to>`/`cluster-<id>`. `IrNode.meta` is the free channel
  for per-kind extras.
- S4 (text, from analysis): JS never measures. Labels arrive pre-broken
  with `\n`; sizes come from `NodeBox` (measured in Rust, `engine.rs:117`,
  lanes re-measure `gx-layout:1099-1127`) — the scene takes the authoritative
  size from NodeBox, not IrNode.size. No wrap feature is ported (none exists);
  wrapping would be a new feature on both sides, out of scope.
- S5 (glyph geometry, honest split): crow's foot, cylinder rim, note fold,
  diamond, arrow markers, lifelines are DRAWN by Svelte from shipped tokens
  (`gx-core/src/tokens.rs:36-45`, `CROW_FOOT`, `BADGE_WIDTH`) — not shipped
  as precomputed path strings (that would be "SVG again").
- S6 (owner decision 2026-11-08): ER columns stay author-authored this
  feature. Tree-sitter SQL column extraction (blocked by design at
  `gx-index/src/lang/sql.rs:1-10`) is OUT OF SCOPE — candidate f12.
- S7 (invariant redefinition): «≤3 live SVGs» becomes «≤3 live scenes» — the
  culling decision (`cullDecide`) is inherited unchanged; what changes is
  what mounts. The re-decide-on-every-camera-write rule holds.
- S8 (acceptance gate): the flip (task C) happens only when equivalence
  probes are green: the mounted scene serialized ≡ golden SVG, modulo
  formatting, on all 15 golden documents. The golden-hash gate remains ONLY
  on the Rust export.

## Tasks (delivery order)

- T1/A (S3 S5; route: worker; commit: work-unit): scene builder in Rust —
  refactor `gx-render-svg/src/renderer.rs` into a scene builder + two
  emitters sharing shape/arrow/text math: the SVG emitter (export + goldens
  only) and the element serializer. Add `LayoutGroup.members` in `gx-layout`
  (dropped today, `layout.rs:36-45` vs `ir.rs:281`). Serialize the scene
  into `RenderOutcome`/`RenderHookPayload` as the additive `elements` field
  beside `svg` (strictly optional; nothing rejects unknown keys). Elements:
  Scene{canvas,tokens,direction,palette-by-css-vars,nodes,edges,bands,events},
  per the analysis draft table. Zero frontend change in this task; parity
  must stay 57/57 untouched.
- T2/B (S3 S4 S5 S7; route: worker; commit: work-unit): the Svelte painter —
  `src/lib/scene/scene.js` (pure, no DOM, `sceneFromPayload(p)` returns
  Scene|null; null keeps the legacy `{@html}` branch alive until C) +
  `DiagramScene/SceneNode/SceneEdge/SceneBand/SceneText` keyed components
  inside a real `<svg class="graphium">`, same classes as `renderer.rs`
  (theming via existing CSSOM vars, zero color literals, geometry in
  attributes not style). Template picks scene when present, `{@html}`
  otherwise. MEASURE before adopting: 300-node fixture mount time vs the
  1.466 ms/mount string baseline (`tools/measure-html.mjs` method), steady
  state Pss inside ±5 MB of `svelte-ui-t5-bench.md` §4; numbers go in the
  feature doc L-entry.
- T3/C (S8; route: worker; commit: work-unit): the flip — equivalence probes
  in the harness (serialize mounted scene vs payload.svg modulo formatting,
  15 golden docs) as report items first, then gates; when green twice, `svg`
  LEAVES `RenderHookPayload` (runtime), CONTRACT.md §5.5 updated, the
  `{@html}` branch dies, svg_size string archaeology (app.rs:470) dies with
  it. Export path untouched.
- T4/D (route: worker; commit: work-unit): per-element interaction —
  node-level hover/focus/selection (callback props, no dispatcher/slots),
  version-step morph (no Rust round trip per ←/→), gantt bars
  (`IrEvent.span` already honored by layout, `gx-layout:1097-1112`; the
  renderer ignored it, `renderer.rs:922`). Semantic upgrades that are pure
  paint (table rows/badges, class compartments once `gx-kinds` grows the
  members field) land here or in follow-ups, not in T1-T3.

## Log

- L1 (2026-11-08, owner): «por rendimiento estamos sacrificando
  escalabilidad, no quiero mas render de svg tenemos todo ya para eliminarlo
  e implementar nuestra propia bilioteca de diagramas vuelve a analizarlo,
  claramente mantendremos la exportacion en png y svg, pero esta no sera
  parte del funcionamiento de la logica» — after the four analysis rounds
  and the owner's reading that strings trade scalability for paint speed.
- L2 (2026-11-08, owner): SQL column extraction stays out (S6) — «la
  extracción de sql aparte», confirmed not a prerequisite; candidate f12.
- L3 (2026-11-08, orchestrator): branch opened from staging @ f15c1c2 over
  the closed mejoras-frontend base (T1-T4 landed there, not yet merged to
  staging — this feature deliberately does NOT depend on it; both merge
  independently). Four analysis angles persisted in memory topic
  `f11-biblioteca-elementos`.
- L4 (2026-11-09, worker/T1-A): the scene exists in Rust and the SVG emitter
  survives as an emitter, not as the model.

  **What changed.** `gx-core/src/scene.rs` owns the schema (`Scene`,
  `SceneNode`, `SceneEdge`, `SceneBand`, `SceneEvent`, `SceneTokens`,
  `ScenePalette`) and re-exports it; `RenderOutcome.elements: Option<Scene>`
  and a defaulted `SvgRenderer::scene` carry it out of the single door
  (`engine.rs` builds it only when `opts.export` is off, so the export stays
  bytes on disk, S2). `gx-render-svg/src/renderer.rs` is now one of two
  emitters of one shared plan: `geometry.rs` holds every decision that is not
  "how do I spell this in SVG" (box of a node, order of the layers, ids
  `g-<id>`/`m-<from>-<to>`/`cluster-<id>`/`lane-<id>`, `\n` line rule and
  baselines, table geometry, crow's-foot points, the frozen constants) and
  `scene.rs` walks the same `Drawing` to produce the `Scene` instead of a
  string. `LayoutGroup` gained `members` (filled from `IrGroup.members` in
  `gx-layout`, dropped until today) and `gutter` (the left lane gutter the
  lanes engine already computed) — both `serde(default)`, so a stored layout
  from before this commit still parses. `RenderHookPayload.elements` makes the
  scene additive beside `svg` (`tools.rs`); nothing rejects an unknown key.

  Two fields go past the draft table of the analysis, both because the
  painter cannot invent them: `Scene.layout_hint` (without it the JS cannot
  know a lanes graph draws a lifeline under every actor, S5) and
  `SceneEvent.center`/`size` (S4: the lanes engine measures an event box, the
  JS never does). `ScenePalette` ships the CSS custom properties
  (`var(--node-fill)`, …) and the theme name, never a colour literal.

  **Evidence.** `cargo fmt --check` exit 0; `cargo clippy --workspace` exit 0
  with no warning; `cargo test --workspace` all green — the 15 documents of
  `crates/gx-app/tests/golden_svg.rs` compare byte by byte against the
  **unchanged, un-regenerated** goldens, which is the proof the refactor kept
  the emitter intact; `node tools/parity.mjs` → `57/57 probes passed`;
  `DISPLAY=:0 scripts/smoke.sh` → `diagram=d-0001 rendered=true reason=none`.
  New witnesses: 16 tests in `gx-render-svg` (10 of the scene builder, 6 of
  the shared geometry and its constants), 2 layout tests for
  `members`/`gutter`, 1 payload test in `gx-mcp` (a renderer with no scene
  leaves the payload as it was) and 7 in `gx-app/tests/scene_payload.rs`,
  including "the export builds no scene" and "the scene carries no path".
  They bite: emptying the node list of the builder drops the scene suite to
  7/10 and the payload suite to 3/7, which is how the two suites were checked
  before landing.

  **Size on the 300-node fixture** (`chain(300)` of RNF10, measured by
  `the_scene_of_three_hundred_nodes_weighs_less_than_the_svg`): SVG string
  **126 561 B** vs elements JSON **114 625 B** — the scene is 90.6% of the
  string it replaces (11 936 B lighter), with all 300 nodes and 299 edges in
  it. The first cut weighed 143 947 B (113.7% of the SVG) with every optional
  field written as `null`; empty `rows`/`meta` and `None` fields are now
  absent, which is the difference.

  **Specs.** Covers S2 (export builds no scene), S3 (schema mirrors
  `Ir` ∪ `LayoutGraph`, ids of the drawing), S4 (pre-broken labels, size from
  `NodeBox`) and S5 (tokens and glyph constants, no path). S1 is only half
  done by construction: the scene exists but `svg` still travels beside it
  until T3/C.

  **Open piece of A.** The relay into the window's `graphium://render` event
  (`RenderEvent` in `crates/gx-app/src/app.rs`) is not in this commit: the
  edit surface of T1/A stops at `crates/gx-app/tests/**`, and the frontend
  still reads `svg` only. The two lines that close it are `elements:
  Option<gx_core::Scene>` on `RenderEvent` (`skip_serializing_if` none) and
  `elements: payload.elements.clone()` in its construction; they land when
  that surface is authorised, or with T2/B, which is the task that consumes
  it.
