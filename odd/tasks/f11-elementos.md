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
- L6 (2026-11-09, owner + orchestrator): the base changes — the branch rebases
  onto `feature/mejoras-frontend`. Owner: «pensé que ya esas mejoras estaban
  implementadas cuando se inició con f11, eran necesarias en f11 ya que eran
  correcciones de errores también», then «mejor hazlo tú, lleva las mejoras a
  f11». L3's "deliberately does NOT depend" is superseded: the painter now
  stands on the improved frontend, which is also the S7 reading — the culling
  the painter inherits is the pure `cullDecide` behind the one imperative
  funnel of mejoras T2 (not the `$effect` version of the old base), with
  `svelte:window`, the camera as a pure CSS transform and `{@html}` string
  records already in.

  **What the rebase did.** `git rebase feature/mejoras-frontend` over
  e47db9e/4b95581/06548a9 → 6fc05c3/51a61e2/2f8da86 on 5fbbfb1. Two
  conflicts. `tools/measure-html.mjs` (add/add): this branch's 410-line
  version wins — it measures both paths (`--mode=html|scene`), a superset of
  the 126-line mejoras tool whose T2 decision lives in that feature's Log.
  `App.svelte` (content): resolved by hand, re-expressing the painter
  integration on the improved file, where `rec.svg` is the markup **string**
  (mejoras T2 adopted `{@html}` and deleted `svgMount`). Concretely: the
  record gains `scene` beside the markup-string `svg` field and
  `previewScene` beside `previewSvg`; `drawingInView(rec)` returns `{scene}`
  or `{markup}` in the order of L5 (preview scene, preview string, live
  scene, live string); the template mounts
  `{#if drawing.scene}<DiagramScene … />{:else if drawing.markup}{@html}`;
  the scene wins in `insertDiagram`/`applyVersionPaint`/`applyClonePaint`
  with no parse on the scene path (the RED probe of L5 — elements without
  `svg` answered `ack ok:false` — stays green: ack true, no parse, object
  mounted); `paintPreview` splits into `centerContent` + `paintPreviewScene`;
  `restoreLive` and the culling mount guard cover the two shapes. One fix
  beyond the port: the clone-reconcile guard is now `!rec.svg && !rec.scene`
  — on the old base a scene-carrying clone kept `rec.svg` empty and
  re-emitted `cloneView` on every `graphium://state`.

  **Gates on the rebased branch, all fresh.** `npm run build` exit 0; `cargo
  build -p gx-app` exit 0; `DISPLAY=:0 scripts/smoke.sh` → `rendered=true
  reason=none` exit 0 (the scene paints inside the real shell); `node
  tools/parity.mjs` → `57/57 probes passed` exit 0; `cargo fmt --check`
  exit 0; `cargo clippy --workspace` exit 0; `cargo test --workspace` 0
  failed — the byte-exact golden gate of the emitter included, so the export
  artifact is untouched. S1–S8 keep their meaning: the string still travels
  beside the scene until T3/C, and the flip now happens on the improved
  base.
- L7 (2026-11-09, owner + Judgment Day): the owner requested a dual judgment
  over the changes of both stacked branches and their logic (`f15c1c2..HEAD`,
  35 files, +5498/−530). Two blind judges (A/B) swept the tree once; the
  controller canonicalized and froze 14 rows. One severe finding survived and
  was fixed in the one authorized round (owner-approved):

  - **JD-A-001** (canonicalized CRITICAL; controller-verified before the
    batch): SceneEdge ids were `edge_id(from, to)` with no disambiguation and
    no upstream uniqueness validation, so two IR edges between the same node
    pair yielded duplicate keys in `DiagramScene`'s keyed each
    (`DiagramScene.svelte:85`); Svelte throws `each_key_duplicate` in dev and
    prod, and a schema-valid document with two edges between the same nodes
    refused to mount — a regression against the string branch, which drew
    both. Fix (round 1 of 2, `jd-fix-agent`, surfaces `scene.rs` +
    `scene/tests.rs`): `scene_edge_id` — the first edge of a pair keeps
    exactly `m-<from>-<to>`, later twins get `-2`, `-3`, … in paint order;
    `edge_id`, shared with the export emitter, untouched. Two focused tests
    pin the no-twin and the twins cases. Re-judged `verified` by judge B;
    final verification by the controller: `cargo test -p gx-render-svg` 0
    failed (12 scene tests), `cargo test -p gx-app --test golden_svg` green
    (byte-exact, un-regenerated), `cargo fmt --check` and `cargo clippy`
    clean. **JUDGMENT: APPROVED.** Fix commit: `18b276d`.

  **Informational rows (14-row ledger; no fix scheduled), disposition:**
  (a) the painter ships zero committed coverage and parity only exercises the
  string branch (A-002 + B-002) — remediation is T3/C's committed equivalence
  gates (S8); (b) version previews and clone paints still carry only `svg`, so
  the scene branches of `applyVersionPaint`/`mountCloneDrawing` are unreachable
  until T3/C relays elements there (A-003 + B-003); (c) the IPC ships both
  payloads and the scene mount is 2.8× the string on 300 nodes — the trade L5
  documents and the owner accepted, payoff T4/D (A-004 + B-004); (d) the scene
  lives proxied inside the deep-`$state` `plane`, cost unmeasured — feeds the
  steady-state check already open for T3/C (B-005); (e) the switcher notice is
  visually-hidden **by the approved spec** of mejoras T3, but the parity
  assertion guarding it is vacuous (`shown()` checks only the `hidden` class)
  (A-005 + B-001); (f) S8 equivalence needs explicit modulo-formatting
  decisions on three axes — empty layer containers, the source of the `defs`
  markers (filtered vs unfiltered edges), one-point edges (B-006); (g)
  CONTRACT.md §9 inventory and the measure tool's deleted action baseline
  (A-007 + A-008). Items a, b, d, f, g are T3/C scope; e's probe belongs to the
  parity harness; c needs no action.
- L8 (2026-11-09, worker/T3-C): the flip. The scene is the only shape of the
  runtime drawing; the SVG string is now only the export artifact.

  **The probe came first, and it bit.** `crates/gx-app/frontend/tools/equivalence.mjs`
  (new, committed) mounts each of the 15 golden documents through the real built
  bundle — `graphium://render` with `elements` and **no `svg` key at all** — reads
  the `<svg class="graphium">` the painter produced, and compares it element by
  element with `crates/gx-app/tests/golden/<name>.svg` (the export artifact
  `golden_svg.rs` pins byte by byte). Both sides go through one canonicalizer:
  tag, attributes (class lists sorted; numbers rounded to the two decimals the
  drawing promises; `points`/`viewBox`/`d` compared as number lists), trimmed
  text, children in paint order. `modulo formatting` also covers the two
  emitter-only artifacts: the export's `<style>` block (the painter takes its
  colours from `src/app.css`) and `xmlns`. The first RED run was **0/15**, and it
  named real painter defects, not formatting: rounded boxes wrote `rx` without
  `ry` (the emitter writes both), and table rows wrote `text-anchor="start"`
  where the emitter omits the SVG default. Fixed in `SceneNode.svelte` and in
  the paint plan (`textPaint.anchor: null` means "the default, omit it"), the
  suite went **15/15** and the remaining differences were exactly the three the
  Judgment Day flagged.

  **The three decisions, written in the probe header (L7 item (f), B-006).**
  (1) *Empty layer containers*: the emitter writes the layers its kind has —
  `drawing.draws_clusters()` is "the document is a graph", so a graph with no
  containers still gets `<g class="layer-groups"></g>` (11 of the 15 documents) —
  and the painter only writes a layer with elements in it. Both sides' empty
  `layer-*` groups are dropped; a layer empty on one side and not the other
  still fails. (2) *The `<defs>` markers*: the emitter takes the set from the
  **unfiltered** edge list (`write_defs`), the painter from the **filtered**
  scene (`arrowMarkers`). The probe compares the markers each side actually
  references (`marker-end="url(#…)"`), so a referenced marker is required on
  both sides and an unreferenced definition is not evidence; the report counts
  the ids it set aside. (3) *One-point edges*: the emitter writes the
  `<polyline>` of a degenerate one-point route, the painter drops the edge
  (`scene.js`, the emitter's own rule); the probe drops one-point polylines from
  both sides and counts them. On the 15 documents axes 2 and 3 trigger **0**
  times (every run reports `unreferenced markers 0/0, one-point edges 0/0`) and
  axis 1 triggers on 11 graphs (1/0: the emitter's empty layer, which the
  painter does not write) and on the two timelines (1/1: both sides write an
  empty lane layer for the kind).

  **Two runs, both green, both reported.** Run 1 and run 2 of
  `node tools/equivalence.mjs` → `equivalence: 15/15 golden documents`, exit 0,
  with the axis counters above on every document. The scenes are committed at
  `crates/gx-app/tests/golden-scenes/<name>.json` (15 files, 92 KB, the
  `elements` of each render) and `golden_svg.rs` gained
  `the_committed_scenes_match_the_drawing`, the freshness gate: it re-renders
  each document and compares the committed scene byte by byte
  (`UPDATE_SCENES=1` regenerates). The probe reads the committed pair, so the
  gate needs no cargo run and the drawing under test is still the one the
  product's single door produced.

  **The flip.** `svg` left `RenderHookPayload` (`elements: Scene`, required), the
  window's `RenderEvent`, `ClonePaint` (`elements?`, absent = nothing new to
  draw) and the `graphium://version-paint` payload; all three runtime paths the
  Judgment Day named relay the scene instead (item (b), A-003 + B-003):
  `graphium://render`, the version preview (`render_version_view` now returns
  `Result<gx_core::Scene, ErrorPayload>`, the caller emits it) and the clone
  paint (`create_clone`/`view_clone` use the scene). The `svg_size` string
  archaeology of `app.rs` died with it: `drawing_size(&scene)` reads
  `scene.canvas`, the bounding box the layout measured. A renderer that builds
  no scene no longer paints (`ui_error`, documented in `gx-mcp`), and the
  `gx-mcp` test double gained a scene so the paint path keeps its tests — that
  is the one file touched outside the task's listed surfaces
  (`crates/gx-mcp/src/test_support.rs`, test-only; see Risks below).

  **The frontend lost the string, not the paint.** `App.svelte` dropped the
  `svg`/`previewSvg` record fields, `drawingInView` returns the scene or
  `null`, `insertDiagram` answers `render_ack ok:false` when the payload carries
  no mountable scene (no parse, no `innerHTML`), `applyVersionPaint` and
  `mountCloneDrawing` paint the scene only, `applyClonePaint` keys on
  `p.elements`, and the template has one branch:
  `{#if drawing !== null}<DiagramScene scene={drawing} />{/if}`. `scene.js` no
  longer documents the `null` as "the legacy branch stays alive": the scene is
  the wire.

  **Parity speaks the same contract** (L7 item (a), A-002 + B-002: the painter
  shipped zero committed coverage). `tools/parity.mjs` re-pointed every payload
  from `svg` strings to a `sceneFixture(...)` scene (the frozen tokens; one node
  named by the probe), reads mounted drawings by the scene's label instead of
  the dead `data-marker`, and added a probe for the flip itself: a payload
  carrying a string beside a scene paints the scene and no `data-marker` ever
  mounts. `node tools/parity.mjs` → **58/58 probes passed** (57 + the new one),
  exit 0, every probe through the scene path. `tools/lib/harness.mjs` gained
  `__t.markerOf` for that, nothing else.

  **Export untouched (S2).** `crates/gx-app/tests/golden/` is byte-identical and
  un-regenerated (`git status` shows no change under it), `gx-render-svg` was
  not touched, and `golden_svg.rs`'s byte-for-byte test passes inside the
  workspace suite. The export emitter keeps its own path and its own scene-free
  outcome (`the_export_does_not_build_a_scene` still asserts `elements == None`).

  **Gates, all fresh, real exit codes.** `npm run build` (frontend) exit 0
  (95.45 → 93.82 kB JS: the string branches left); `node tools/equivalence.mjs`
  15/15 exit 0 (twice before the flip and once after); `node tools/parity.mjs`
  58/58 exit 0; `touch crates/gx-app/build.rs && cargo build -p gx-app` exit 0;
  `DISPLAY=:0 scripts/smoke.sh` → `diagram=d-0001 rendered=true reason=none`
  exit 0 (the real WebKitGTK window mounts the scene of the flipped payload);
  `cargo test --workspace` exit 0 — 38 suites, 729 passed, 0 failed, the
  byte-exact goldens included; `cargo fmt --check` exit 0; `cargo clippy
  --workspace` exit 0, no warning. `tools/bench-live.sh` on the flipped binary
  (item (d)): `coldStartMs 496`, `render_diagram` 300 nodes `547/532 ms`,
  `rendered true reason none`, `pssKb [282 248, 282 263]`, **`processes: 1`** —
  the WebKitWebProcess is still not a child of the owner in this environment, so
  the ±5 MB band of `svelte-ui-t5-bench.md` §4 remains unmeasurable and the
  steady-state check stays open, as L5 recorded. (The bench's own teardown hung
  after writing its JSON and the command was cut; its report is
  `/tmp/bench-live-f11-t3c.json` and two scratch dirs under `/tmp/gx-bench-*`
  were left for the parent to remove — `rm` is blocked in the worker.)

  **CONTRACT.md** §5.1 (`RenderEvent.elements` replaces `svg`), §5.5
  (`ClonePaint.elements?`), §5.8 (`version-paint`), §6 (the culling ceiling is
  now "3 live scenes"), §8 (C4/C14/C19/C21 wording and probes) and **§9** were
  updated: the inventory lists `scene.js`, `glyph.js` and the four components,
  and a "what T3/C delivered" paragraph maps the flip and the S8 gate (item (g),
  A-007). The other half of item (g) — the measure tool's 1.466 ms/mount
  baseline, which cites the deleted vanilla action — is **not** touched: this
  task's surfaces forbid editing `tools/measure-html.mjs`'s measurement claims.
  It stays open for the parent, together with `README.md`'s harness list (no
  equivalence line; `README.md` is outside the surfaces too).

  **Specs.** Covers S1 (the string leaves the runtime path), S2 (export stays
  bytes on disk; goldens untouched), S7 (the invariant counts scenes; the
  culling decision and funnel are the ones inherited) and S8 (the equivalence
  gate, 15/15, committed and fresh via the golden suite). S3/S4/S5 keep their
  meaning from L4/L5 and the probes now prove them on all 15 documents: same ids
  and classes, boxes from `NodeBox`, glyphs drawn by the painter from the
  tokens. S6 (SQL columns out of scope) is untouched. Open after T3/C: T4/D
  (per-element interaction, the version-step morph without a Rust round trip,
  gantt bars), the steady-state memory check of item (d), and the two
  documentation/measure leftovers of item (g) named above.
- L9 (2026-11-09, orchestrator): T3/C verified independently and committed.

  **Verification.** The controller re-ran every gate on the delivered tree:
  `npm run build` exit 0; `node tools/equivalence.mjs` → `15/15 golden
  documents` exit 0, **twice**; `node tools/parity.mjs` → `58/58 probes
  passed` exit 0; `cargo build -p gx-app` exit 0; `DISPLAY=:0 scripts/smoke.sh`
  → `rendered=true reason=none` exit 0; `cargo fmt --check` exit 0; `cargo
  clippy --workspace` exit 0; `cargo test --workspace` 38 suites, 0 failed.
  The flip was read, not trusted: `RenderHookPayload` carries `elements` with
  no `svg` key, `render_version_view` returns `Result<gx_core::Scene, _>`, the
  template mounts one branch, and `svg_size` is gone behind a tombstone
  comment. Commit: `6617751`.

  **One surface deviation, accepted.** `crates/gx-mcp/src/test_support.rs` was
  outside the dispatch's allowed surfaces; the worker declared it, the
  controller reviewed it (test-only stub `scene()` on the renderer double,
  forced by the payload flip, since `gx-mcp` does not depend on the emitter
  per PRD §9.1) and accepts it into this delivery.

  **Leftovers closed by the parent** (the worker's surfaces forbade both): the
  frontend README's harness list now names `tools/equivalence.mjs`, and
  `tools/measure-html.mjs`'s header no longer presents the deleted legacy
  branch as live — the 1.466 ms/mount baseline is marked historical (it
  measured the removed `svgMount` action and was never reproduced, L5).

  **State.** S1 and S8 are done; the runtime drawing is the scene alone and
  the string survives only in the export path (S2, goldens byte-exact).
  Remaining on this feature: **T4/D** (per-element interaction: hover/focus/
  selection, the version-step morph without a Rust round trip, gantt bars),
  the steady-state memory check (item (d) — the `WebKitWebProcess` is still
  not a child of the owner, so the ±5 MB band stays unmeasurable), and nothing
  else. The merge of `feature/f11-elementos` into `staging` remains the
  owner's decision.
- L10 (2026-11-09, worker/T4-D): the drawing is a set of elements — you can
  touch them, step through their history and read a duration. The closing task
  of the feature, in the three pieces the task line names.

  **1. Interaction per element, callback props, no dispatcher.** Every node,
  event and edge of the mounted scene is its own element of the page: `role`
  `button`, one tab stop, `aria-label` from the drawing's own label (an edge
  says `<from> → <to>`), `aria-pressed` for the selection, and the pointer
  hover, the focus and the click/tap surface through **three callback props**
  (`onhover`/`onfocus`/`onselect`, `(owner, id)`) that `App.svelte` hands down
  unchanged. The page holds the state: `hoveredElement` and `focusedElement`
  (one for the whole plan — a pointer is one and a focus is one, `{recId,id}`)
  and `rec.selectedElement` (per drawing, so a drawing that leaves the plan
  takes its selection with it). The visual answer is the **existing** class of
  the drawing: `emphasis-highlight` on the shape and `emphasis-highlight-text`
  on its labels, never a colour and never a second class — `src/app.css` gains
  only `cursor`, the `:focus-visible` ring in `var(--accent)`, and
  `pointer-events: none` for the labels and the two decoration paths (rim and
  fold), which is what makes the whole box one hit target instead of a ring
  around its own text. The owners come first in the callbacks because a clone
  and its original draw the same element ids. The three functions are declared
  once and passed unchanged: a stable function is a prop that never changes, so
  a pointer crossing a 900-element drawing updates two class attributes and
  nothing else (the first cut passed per-record arrow functions — a new
  identity per hover change, which re-created the props of every element).
  Bands are deliberately **not** interactive: a container is the background its
  nodes are read on, and selecting one is a different feature. An edge is hit
  on its stroke, so a 1 px line asks for a steady hand — the honest limit of
  "the line is the element" (see the proposals).

  **2. The version step morphs.** `←`/`→` (and the `◀`/`▶` of the panel) paint
  a version the page already drew **from a cache on the object** (`rec.versions`,
  a plain `Map` outside reactivity) instead of asking Rust: no
  `graphium://version-view`, no render. The cache keeps the last
  `VERSION_CACHE_LIMIT = 8` versions of the object (re-inserting on use, so the
  oldest visit is the one that leaves), it is **emptied when the live drawing
  comes back** (`viewVersion` at `current_version`, the only place the restore
  happens) and it **dies with the object**: a re-render replaces the record
  (C19) and a state drop removes it (C18), so a version of a closed diagram is
  not kept anywhere. The morph itself is the shape the painter already had:
  `DiagramScene` and its children are keyed by the id of the drawing, so a new
  version payload moves the elements that changed and leaves the rest mounted —
  the probe holds the `<svg class="graphium">` **and** the DOM node of a node no
  version touches across a step and finds them identical. `applyVersionPaint`
  now caches **before** deciding whether the answer is still the one the human
  asked for: a version stepped through fast is exactly the one he may step back
  to.

  **3. Gantt bars, through the shared plan.** `IrEvent.span` reached the paint
  as nothing: the lanes engine sized the event box from the span (`gx-layout`,
  `event_size`, `span.end > span.start` summed into columns) and the renderer
  drew that box exactly like a point in time. The geometry is now decided
  **once** in `gx-render-svg/src/geometry.rs` — `event_bar(rect, span)` is a
  band along the bottom of the measured box, inset by the border,
  `EVENT_BAR_HEIGHT = 5.0` high, and `None` for a point in time under the very
  predicate the layout uses — the SVG emitter writes it, the scene builder
  carries the four numbers in the new `SceneEvent.bar`, and the painter draws
  the rectangle it was given (`glyph.js` only rounds it; S5: numbers, not
  paths). The class is `accent`, which the export's `<style>` block and
  `src/app.css` already resolve, so **no style rule was added and no golden of
  a document without a span changed**.

  **The goldens were regenerated on purpose — the only time in this feature.**
  `UPDATE_GOLDEN=1` (and `UPDATE_SCENES=1` for the committed scene) touched
  exactly one of the 15 documents: `timeline-roadmap` **2931 → 3078 bytes**
  (+147), two `<rect class="accent" x="301.00" y="73.75" width="116.00"
  height="5.00"/>` and the same pair at 601/183.75, one under each event that
  declares an `end`, plus the `bar` numbers of those two events in
  `crates/gx-app/tests/golden-scenes/timeline-roadmap.json`. `git status` shows
  those two files and nothing else under `crates/gx-app/tests/`. The export
  path itself (S2) did not change: same emitter, same plan, one more rectangle
  for a kind that has a duration.

  **RED, then GREEN, observed.**

  - *Bars, Rust.* Three behaviour tests written first: the emitter writes a bar
    for the spanned event and none for the point, the scene carries the same
    four numbers as the JSON of the event, and `event_bar` is the frozen
    constant and the layout's own predicate. `cargo test -p gx-render-svg`:
    **52 passed, 3 failed** — "one bar: the event with a span / left: 0, right:
    1", "la barra de un evento con tramo viaja como números (S5)" and "la barra
    del SVG". After the implementation: **0 failed**, and the golden gate then
    named the intended change by itself ("timeline-roadmap: the drawing changed
    (3078 bytes now, 2931 in the golden)", hash `2acb59d6…` vs `d34626e0…`)
    before the regeneration.
  - *Interaction and morph, in the browser harness.* Six probes written before
    the painter half and run against **the base bundle** (a copy of
    `1a9bdb2`'s frontend built in `/tmp`: same Chromium, same fake bridge, the
    **new** `tools/parity.mjs`): **59/64, exit 1**, with the five red ones the
    task exists for — "a version already visited asked Rust again: 3",
    "walking to the live drawing emitted: 3", "the node is not an element of
    its own", "click: no element null", "bars mounted: 0". (The sixth, "a
    re-render replaces the object and its cache with it", holds on the base by
    construction — with no cache every step asks — and is the PRESERVE guard of
    the eviction rule, not a RED case.) Against the delivered bundle:
    **64/64, exit 0**, twice (before and after the stable-callback refactor).
  - *Bars, painter side, same differential.* The new `tools/equivalence.mjs`
    (with the fourth decision of its header) and the regenerated golden pair
    against the **base** painter: **14/15, exit 1**, the only failure
    `timeline-roadmap` with `children.length: 10 (golden) vs 8 (scene)` — the
    two bars, and nothing else, which is also the proof that the interaction
    attributes the probe now drops do not hide any other difference. Against
    the delivered bundle: **15/15, exit 0**.

  **The S8 gate grew a fourth decision, written in its header.** An exported
  file has no pointer and no keyboard: `role`, `tabindex`, `aria-*` and the
  `data-element` hook are painter-only. The canonicalizer drops them on both
  sides and counts them (the report now reads `interaction attributes 0/15` for
  `er-posts`, `0/20` for `timeline-roadmap`), exactly like the `<style>` block
  of the export. A **class** is not one of them: a probe that mounted a
  selected scene would still fail, on purpose. CONTRACT.md §8 keeps the parity
  checklist and gained the three rows of this task (C44 the morph, C45 the
  interaction, C46 the bar), §9's T3/C paragraph now says *four* decisions, and
  a "what T4/D delivered" paragraph closes the section. No wire message, no
  payload field outside the scene, no new panel.

  **Gates, all fresh, real exit codes.** `npm run build` **0** (102.25 kB JS,
  +8.4 kB: ~3.3 kB of it is Svelte's generic attribute-spread runtime, measured
  by building the same components with the eleven attributes repeated on each
  of the five shape branches — 98.98 kB — and kept the single definition on
  purpose: one place to audit the a11y contract, no wrapper `<g>` the export
  does not have); `node tools/equivalence.mjs` → `15/15 golden documents`,
  **0**; `node tools/parity.mjs` → `64/64 probes passed`, **0** (the 58 of
  L8/9 untouched and green, six new); `touch crates/gx-app/build.rs && cargo
  build -p gx-app` **0**; `DISPLAY=:0 scripts/smoke.sh` →
  `diagram=d-0001 rendered=true reason=none`, **0**; `cargo test --workspace`
  → 38 suites, **733 passed, 0 failed**, **0** (the byte-exact goldens and the
  `the_committed_scenes_match_the_drawing` freshness gate among them);
  `cargo fmt --check` **0**; `cargo clippy --workspace` **0**, no warning.

  **Specs.** Covers S5 (the bar is numbers in the payload — the painter draws
  the rectangle it is given, and the geometry lives once, in `geometry.rs`), S3
  (same ids, same classes; the interaction adds attributes and no vocabulary)
  and S7 (the culling is untouched: still ≤3 live scenes, same funnel). S1/S2
  keep their meaning: the runtime is the scene alone, the export is bytes on
  disk, and the only byte change is the bar of a kind that has a duration. S4
  is untouched — the bar comes from the box the layout measured, the JS neither
  measures nor recomputes it. S8 is the gate that says all of it: 15/15 on the
  regenerated pair.

  **One surface deviation, for the same reason as L9.**
  `crates/gx-mcp/src/test_support.rs` (outside this task's list) builds a
  `SceneEvent` by hand, so the new `bar` field broke it; the stub now mirrors
  `event_bar` the way it already mirrors `scene_tokens`, with the same comment:
  `gx-mcp` cannot depend on `gx-render-svg` (PRD §9.1) and the double is
  test-only. Four lines, declared for the controller.

  **Proposed, not built.**
  (a) *Ship the version scenes upfront*: the page could hold the whole history
  of a diagram (or ask Rust for a batch of `k` scenes) and never ask again.
  Cost: a scene is the size of the live one — 114 625 B for the `chain(300)`
  fixture of L4 — so a 7-version history is ~800 kB of JSON per diagram on
  every render/state event, and even `timeline-roadmap` would carry ~21 kB.
  It needs a new wire field (CONTRACT §5) and a "which versions" policy in
  Rust; ~1 line in `app.rs` per version, ~40 in the painter, and the cache of
  this task becomes the fallback. Not worth it for the revisit case, which is
  what `←`/`→` does in practice.
  (b) *A composite keyboard widget*: today every element is a tab stop, so a
  300-node drawing is 900 of them. The standard shape is a roving `tabindex`
  (one stop per drawing) plus a navigation key inside it — but `←`/`→` are the
  history and `↑`/`↓` would have to be claimed, so it is a UX decision, not a
  refactor: ~1 constant + ~30 lines in `DiagramScene`, and it changes what the
  page's keyboard does.
  (c) *A wider edge hit area*: `pointer-events: bounding-box` on the polyline
  (one CSS line) or an invisible `stroke-width: 8` twin; the first makes a long
  diagonal hover over empty space, the second adds a node the export does not
  draw. Not obviously better than "the line is the element".
  (d) *The same cache for the history inside a clone*: the mechanism is the
  record's, so it is small (~15 lines), but `snapshot_version` moves the
  clone's own rectangle and the rule of what a photo means has to be re-read
  (`view_clone`), so it is a task, not a patch.
  (e) *Interactive bands*: containers and lanes as elements of their own (the
  members are already in the scene and the id is already `cluster-`/`lane-`).
  (f) *A discovered defect, not touched*: the export's `<style>` block has no
  `.cardinality` rule (`write_style` lists 20 selectors and the crow's foot is
  not one of them), so a standalone ER export paints its crow's feet with the
  browser default — black — while the window paints them with `.cardinality`
  from `src/app.css`. One line in `write_style` plus the `er-posts` golden
  regenerated; it is a pre-existing export bug and belongs to its own task, not
  to the bars.

  **Risks.** The tab order of a big drawing is long (proposal b); the hover of
  a 900-element scene was not measured after the stable-callback fix (the
  probe proves the behaviour, not the cost); and the base bundle and logs of
  the RED runs live in `/tmp` (`f11-t4d-base-JjWd`, `f11-t4d-base-path.txt`,
  `parity-red.txt`, `equiv-red.txt`, `g*.txt`), left for the parent to remove —
  `rm` is blocked in the worker. Nothing of that is inside the worktree:
  `git status --short` lists the source files of this delivery and this log
  entry, and no cache or scratch path of any tool.
- L11 (2026-11-09, orchestrator): T4/D verified independently and committed —
  the feature's tasks (T1-T4) are complete.

  **Verification.** The controller re-ran every gate on the delivered tree:
  `npm run build` exit 0; `node tools/equivalence.mjs` → `15/15 golden
  documents` exit 0 (four S8 axes in the header, the fourth being the new
  painter-only interaction attributes); `node tools/parity.mjs` → `64/64
  probes passed` exit 0; `cargo build -p gx-app` exit 0; `DISPLAY=:0
  scripts/smoke.sh` → `rendered=true reason=none` exit 0; `cargo fmt --check`
  exit 0; `cargo clippy --workspace` exit 0; `cargo test --workspace` 38
  suites, 0 failed. The hunks were read: `SceneEvent.bar` is numbers-only
  (`Option<Rect>`), emitter and scene read the same `geometry::event_bar`,
  callbacks are stable module-scope `(owner, id)` props, the cache is a
  per-record bounded `Map`, and the golden regeneration touched exactly the
  one document the bars changed (timeline-roadmap svg + scene pair).
  Commit: `639cc17`. Accepted surface deviation, same pattern as L9:
  `crates/gx-mcp/src/test_support.rs` (test-only stub bar). The worker's
  `/tmp` scratch was removed by the parent.

  **Feature state.** S1-S8 done (S6 out by owner decision, L2). The runtime
  drawing is the scene alone, it morphs in place under the history, its
  elements answer hover/focus/selection, and events that last draw their
  bars; the SVG string survives only as the export artifact, byte-pinned.
  Remaining, none of it in-scope code: the steady-state memory check of L7
  item (d) stays unmeasurable in this environment (the `WebKitWebProcess` is
  not a child of the owner); the pre-existing export bug the worker found
  (no `.cardinality` rule in `write_style`: standalone ER exports paint black
  crow's feet) is its own future task, untouched here; and the merge of
  `feature/f11-elementos` into `staging` — together with
  `feature/mejoras-frontend` — is the owner's decision.
