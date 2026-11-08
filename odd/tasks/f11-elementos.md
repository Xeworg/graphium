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

- L5 (2026-11-09, worker/T2-B): the scene is painted, and the plan mounts it.

  **What changed.** `src/lib/scene/scene.js` is the door (`sceneFromPayload(p)`
  → scene or `null`; `null` is what keeps the legacy `{@html}` branch alive
  until C). It is pure, it never throws, and it separates two kinds of odd: a
  **structural** one (no canvas, no tokens, no measured box, an id that is not
  an id) is `null` for the whole scene, while an unknown `shape`/`emphasis`/
  `style` falls back to the neutral word (`box`, `normal`, `solid`) instead of
  blanking the plan over a value the frontend does not know yet. An edge whose
  route has fewer than two points is dropped, exactly as the emitter draws no
  line for it. `src/lib/scene/glyph.js` is the arithmetic of
  `gx-render-svg/src/geometry.rs` in JS — label lines and baselines, the box of
  a node, the table of a node with rows, the crow's foot, the cylinder rim, the
  note fold, the diamond, the lifeline, the marker ids, the classes — and it is
  where the paint plan of a whole scene is built (`diagramPaint`).
  `DiagramScene.svelte` mounts a real `<svg class="graphium">` with the layer
  order of `renderer.rs` (groups, lanes, lifelines, edges, nodes-and-events) and
  the children `SceneNode`/`SceneEdge`/`SceneBand`/`SceneText`, each keyed by
  the id of the drawing and each declared
  `<svelte:options namespace="svg" />` (a component whose root is a `<rect>`
  is created in the HTML namespace without it — the probes caught the whole
  scene painting nothing). The classes are the classes of the emitter, so
  `src/app.css` paints the mounted scene with the same rules and the same CSS
  custom properties: **zero colour literals and zero style strings in the
  scene**, geometry always in attributes (`x/y/width/points/d`).

  `App.svelte` gained the switch and nothing else: the record holds `scene`
  beside `svg` (and `previewScene` beside `previewSvg`), `insertDiagram`,
  `applyVersionPaint` and `applyClonePaint` store the scene when the payload
  carries one, and `drawingInView(rec)` picks what the culling keeps mounted
  (preview scene first, then preview string, then live scene, then live
  string). The culling itself is untouched — same `MAX_LIVE_SVGS` funnel, same
  re-decide on every camera write — which is all S7 asks: the invariant is now
  "≤ 3 live **drawings**, scenes on the new path", and a scene-painted object
  and a string-painted one are indistinguishable to the box, the `×`, the tools
  and the pan.

  **Why one paint plan and not one `$derived` per component.** The first cut
  gave every component its own derivations (rect, classes, table, paths): ~900
  components × 5-8 signals, and the 300-node mount cost **30.0 ms**. With the
  plan computed once per scene and handed down as one prop, the same mount
  costs **25.2 ms** (same page, same fixture, 9 samples). The components stayed
  components and the math stayed in one pure module; the difference is only
  where the numbers are computed.

  **Evidence, RED then GREEN, in a throwaway harness** (`/tmp/f11-probe`,
  reusing `tools/lib/harness.mjs`; the payloads are the real scenes of
  `er`, `component`, `timeline`, `sequence` and `chain(300)`, dumped from the
  workspace crates):

  - the pure probes (`scene.test.mjs`: the door, the ids, the classes, the
    glyph numbers against the ones `geometry.rs` asserts, the static rules) went
    from **0/30** (the modules did not exist) to **29/29**;
  - the browser probes (`browser.probe.mjs`, the real built bundle in headless
    Chromium with the faked bridge) went from **14/33** to **35/35**. The red
    ones were the ones this task exists for: a payload with `elements` mounted
    the *string* (`styles=1`), and a scene payload without `svg` was answered
    `render_ack ok:false` with no object at all. The green ones now include the
    crow's foot (`d="M 259.84 55.75 L 281.84 55.75 M 259.84 55.75 L 281.84
    69.39 M 259.84 55.75 L 281.84 42.11"`, the circle at `249.94 r 6.29` and
    the single stroke at `165.04,28.30 → 165.04,65.70` — the very numbers the
    emitter writes for that document), the `PK`/`FK` badges and the row rule,
    the four lifelines of the sequence, the four lanes and four events of the
    timeline, the containers as their own layer, ≤ 3 live scenes with their
    boxes, and the version preview and the clone paint landing as scenes when
    the payload carries elements and as strings when it does not.

  `node tools/parity.mjs` → `57/57` on the new bundle, untouched; `npm run
  build` exit 0; `DISPLAY=:0 scripts/smoke.sh` → `rendered=true reason=none`
  (the 300-node diagram of the bench paints through the scene inside the real
  shell); `cargo fmt --check`, `cargo clippy --workspace` and `cargo test
  --workspace` all exit 0 (nothing on the Rust side changed: the relay was
  already in 4b95581).

  **The measurement** (`tools/measure-html.mjs`, new: `--mode=both|html|scene`,
  `--payload` for a real `{svg, elements}`, `--nodes` for the synthetic chain).
  One sample = `graphium://render` dispatched into the real bundle → the
  drawing is a child of `.content` → `#plan.getBoundingClientRect()` (style +
  layout), in headless Chromium, 2 warm-ups and 9 measured mounts per path,
  each one after emptying the plane so it is a mount and never a re-mount. The
  `render_ack` is excluded on purpose: it waits two frames (C4) and hides every
  difference inside the 4-32 ms pacing noise.

  | Path | chain(300) real payload, median | min … max | chain(300) synthetic | chain(12) synthetic |
  | --- | ---: | ---: | ---: | ---: |
  | string (`html`) | **8.9 ms** | 7.4 … 14.8 | 9.1 ms | 2.0 ms |
  | scene (`scene`) | **25.2 ms** | 20.6 … 28.1 | 27.9 ms | 5.8 ms |

  The payload measured is the real one: `svg` 126 561 B and `elements`
  114 625 B (serde; 114 585 B once the tool re-serialises the same object, the
  40 B being `0.0` → `0`), the two numbers of L4. **The scene mount loses to
  the string**: 2.8× on 300 nodes, 2.9× on 12. The 1.466 ms/mount baseline of
  this file is not reproduced by any instrument here — the raw floor of the
  same 900-element tree in the same page is 4.1-4.6 ms (`innerHTML` and
  `createElementNS` both, insert + layout included) and the string path through
  the real app is 8.9 ms — but the ranking does not depend on which of those is
  the baseline: building 900 elements through ~900 component instances costs
  more than parsing one string. That is the trade the owner accepted in S1 and
  the reason T3/C is not blocked by it: the scene pays once per render, and it
  buys the incremental update the string cannot do at all (T4/D morphs a
  version step in place; the string has to be re-parsed in full). If T3/C wants
  the cold mount back, the levers are measured and known: fewer component
  instances per element and one plan per scene are already in; the next one is
  skipping the `SceneText` instance for a node label.

  **Steady state memory: not established.** `tools/bench-live.sh` ran against
  the rebuilt binary with the 300-node diagram on screen and reports `pss
  354 433 kB` over **one** process: in this environment the owner's
  `WebKitWebProcess` is not a child of it, so the script's tree walk misses the
  web process (and other owners' webviews are running on the machine). The
  number is therefore not comparable with the ± 5 MB band of
  `svelte-ui-t5-bench.md` §4 and this task does not claim it; the check stays
  open for T3/C, where the string dies and the comparison becomes possible
  without the ambiguity.

  **Specs.** S3 (same schema, same ids `g-`/`m-`/`cluster-`/`lane-`, palette by
  CSS custom properties, no colour literal), S4 (no measuring anywhere in the
  painter: the boxes are `NodeBox`, the labels arrive broken), S5 (crow's foot,
  cylinder rim, note fold, diamond, markers and lifelines drawn from the tokens
  and the frozen constants — the payload carries numbers, not paths) and S7
  (the culling inherited unchanged; the invariant now counts scenes). S1 and S2
  are untouched by construction: the string still travels beside the scene and
  the export never changed. The equivalence gate S8 is T3/C, on all 15 golden
  documents, and the browser probes here are its small preview (the scene DOM
  reproduces the emitter's numbers for the er document, element by element).
