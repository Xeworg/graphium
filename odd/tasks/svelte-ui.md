# svelte-ui — migrate the whole Tauri frontend to Svelte, A/B measurable

Experimental branch: NO merge until the human decides with the measurements
in hand. Everything stays in this worktree and the `feature/svelte-ui` branch.

## Specs

- S1 (branch discipline): user verbatim — «hazlo en una rama propia no le
  haremos merge hasta probarlo adecuadamente». All work lives in
  `feature/svelte-ui` of this worktree; no merge to staging/main until the
  human approves it explicitly after reviewing the measurements.
- S2 (scope): user verbatim — «migre la intefaz completa a svelte». Full
  functional parity of the current frontend (`crates/gx-app/frontend/`:
  `index.html`, `plan.js`, `style.css`) as a Svelte app inside the same
  Tauri shell, keeping the `graphium://` intent contract with the Rust side
  intact (ready handshake, camera-changed, render acks, panels, versions,
  sessions, clones, themes).
- S3 (deliveries): user verbatim — «puede por secciones o entregas pero
  siempre en el worktree y la rama». Work arrives as verifiable sections
  (T1..T5); each one leaves the branch green (frontend builds, Rust tests
  pass, vanilla path untouched) and is committed as a work unit by the
  orchestrator.
- S4 (measurement): user verbatim — «quiero poder medir nuestra version vs
  la version con svelte». Both versions runnable from the same branch (the
  vanilla path stays the default; the Svelte build is selected by a config
  override, e.g. a second tauri conf), plus a bench harness that measures
  both under the same scenarios: frontend asset/bundle size, time to
  `graphium://ready`, render of a large diagram, memory while rendering,
  and LOC of the frontend for maintainability. Results land in a doc under
  `odd/tasks/` evidence.

## Tasks

- T1 (S1 S2 S3; route: worker; commit: —): map the full frontend↔Rust
  contract (every `graphium://` intent, event, timeout, panel) from
  `plan.js`/`index.html`/`app.rs`, then scaffold the Svelte (Vite) app that
  compiles and loads inside the Tauri shell with the ready handshake
  working, selected without touching the default `frontendDist`.
- T2 (S2; route: worker; commit: —): port the plane — isometric grid,
  camera, objects, clones, culling; contract numbers identical (grid.rs
  pins live in `plan.js`; Svelte keeps the same values discoverable).
- T3 (S2; route: worker; commit: worker commits the green unit (L7)): port the panels — per-kind forms,
  ×, version preview ◀v3/7▶, sessions panel, themes.
- T4 (S2 S3; route: worker; commit: —): parity checklist against the T1
  map, item by item; the A/B switch documented end to end.
- T5 (S4; route: worker; commit: —): the bench harness and the
  comparative results doc (vanilla vs Svelte, measured numbers).

## Log

- L1 (2026-11-07, user): «quiero que mientras haces esto pon en un worktree
  un agente a que migre la intefaz completa a svelte, ya que a largo plazo
  si nos podria dar un beneficio esto hazlo en una rama propia no le
  haremos merge hasta probarlo adecuadamente, sin embargo para la
  escalablidad del proyecto hay sufuciente posibilidad de mejora, quiero
  poder medir nuestra version vs la version con svelte»
- L2 (2026-11-07, user): «hay suficente recurso en la maquina para
  hacerlo, puede por secciones o entregas pero siempre en el worktree y la
  rama»
- L3 (2026-11-07, orchestrator): worktree `graphium-svelte` created on
  branch `feature/svelte-ui` from `main` @ 77bda03. Context found: the app
  is Tauri (`crates/gx-app`, `frontendDist: "frontend"`); Rust tests
  `include_str!` the vanilla frontend and pin contract numbers (grid
  angle/sin/cos in `plan.js`) — the vanilla path must stay the default so
  those tests never see the Svelte build. Prior decision on record (mnexis,
  2026-11-05): «análisis Svelte — no adoptar ahora»; this experiment is
  the data-gathering path the human chose instead.
- L4 (2026-11-07, worker, T1: S1 S2 S3): contract map + Svelte scaffold. No
  tracked file outside the allowed surfaces changed (`git diff --stat` empty)
  and `crates/gx-app/tauri.conf.json` still says `frontendDist: "frontend"`.
  - Contract map: `crates/gx-app/frontend-svelte/CONTRACT.md` — 10 JS→Rust
    intents (`plan.js:2449/410/1910,2042/770/849/1090/861/1838,1872,2289/536/1592`
    → `app.rs:803/727/732/921/747/772/821/850/876/905`), 8 Rust→page events
    (`app.rs:805/945, 762,809,840,863,896,916,1172, 1158, 783, 839,864, 796,
    758`, and `graphium://pending` which has no emitter), the ack/timeout table
    (ready `app.rs:23` 2000 ms, paint `app.rs:17` 2000 ms, camera + settings
    throttles 100 ms, pan 250 ms), every payload shape, the contract numbers
    (59/45°/41.72/24/3/0.4/zoom 0.05-4.0), the `plan.js`/`index.html`/
    `style.css` section inventory with line ranges, and the C1-C43 parity
    checklist T4 walks.
  - Switch: `TAURI_CONFIG` works with this Tauri — both
    `tauri-build-2.7.4` (`lib.rs:655,681`, `cargo:rerun-if-env-changed`) and
    the codegen of `tauri::generate_context!()`
    (`tauri-codegen-2.7.4/lib.rs:56-91`) merge it as a JSON patch, and
    `frontendDist` resolves against `crates/gx-app`. So:
    `TAURI_CONFIG='{"build":{"frontendDist":"frontend-svelte/dist"}}'
    cargo build -p gx-app` (~11 s recompile of `gx-app` only). No second conf
    file was needed.
  - Evidence: `cargo test -p gx-app` green with the default config (22 suites,
    all `test result: ok`, 0 failed). `scripts/svelte-build.sh` →
    `dist/` 44K, 3 files (js 34.6 kB, css 2.1 kB, html 0.58 kB; js 13.4 kB
    gzip). The Svelte binary embeds `assets/index-DS7FkpqO.js` +
    `index-BBA6K-Tu.css` and no `plan.js`; the vanilla binary embeds the
    opposite. `DISPLAY=:0 scripts/svelte-smoke.sh` (own
    `XDG_RUNTIME_DIR`/`HOME` under `/tmp`, owner quit afterwards) reports
    `rendered=true reason=none` for both binaries — `rendered:true` only
    happens after `graphium://ready` (else `ui_error`) and `render_ack` (else
    `ui_timeout`). A screenshot of the Svelte window shows the isometric grid
    and the chip `svelte · dark · 0 diagramas · 0 pintados`, i.e. the settings
    and the state arrived.
  - Inherited by T2: `src/lib/contract.js` (the numbers), `gridPaint` (grid
    finished), `.plan`/`.diagrams`/`.plan::before` in `src/app.css`, and the
    `.diagram-t1` placeholder to replace with the real object. By T3:
    `settingsFromPayload`/`settingsId`/`palettePaint` and the
    `graphium://settings`/`graphium://state` handlers to extend into the panels.
  - Raw numbers for T5 to re-measure (not the benchmark yet): vanilla frontend
    133,290 B / 4059 lines (`plan.js` 89,635 B, `style.css` 33,304 B,
    `index.html` 10,351 B) vs Svelte `dist/` 37,290 B / 3 files. T1 LOC:
    `src/` 6 files, 597 lines (`App.svelte` 156, `app.css` 168, `grid.js` 126,
    `contract.js` 86, `tauri.js` 51, `main.js` 10); the rest of the directory is
    the lockfile and the generated `dist/`.
- L5 (2026-11-07, orchestrator, T2: S2): the plane ported inline (the writer
  runtime routed it back: single unit of behaviour). Camera, objects, clones
  and culling with the contract numbers of `contract.js`; the vanilla
  frontend byte-identical, the switch the only selector.
  - Files: `src/App.svelte` (the plane: camera, records, wiring), NEW
    `src/lib/plane.js` (pure camera/culling math: `clampZoom`,
    `zoomAtPoint`, `cameraOn`, `visibleRect`, `rectsIntersect`,
    `distanceToView`, `objectCenter`), `src/lib/contract.js` (+`USER_PLAN_ID`
    /`USER_PLAN`), `src/app.css` (object blocks of `style.css` 396-519 + the
    clone cursor bits; `.diagram-t1` placeholder removed; `--surface`,
    `--surface-border`, `--hairline` tokens added).
  - Delivered (CONTRACT.md marks updated in-place): C5, C10-C16 (C11 pan
    half; clone move T3; C16 switch T3), C17-C20, C42 objects half. Clone
    tools pill, history, keyboard arrows, panels: T3.
  - Design note: the camera transform is one write on the `.diagrams` layer
    (`plan.js` writes it per object; same result for one write), and the
    culling DOM swap lives in one Svelte action (`svgMount`) driven by the
    `culled` flag, so the ≤3-live-SVGs rule runs on the same flags the
    vanilla page keeps.
  - Evidence (real exit codes): `npx vite build` → 0 (bundle 34.6 → 49.3 kB
    JS, CSS 3.46 kB); `cargo test -p gx-app` → 0 (22 suites, 0 failed,
    vanilla untouched: `git diff --stat main -- crates/gx-app/frontend/`
    empty); `TAURI_CONFIG='{"build":{"frontendDist":"frontend-svelte/dist"}}'
    cargo build -p gx-app` → 0, binary embeds `assets/index-*.js` and zero
    `plan.js`; `git status` shows changes only under `frontend-svelte/`;
    `DISPLAY=:0 scripts/svelte-smoke.sh` → `rendered=true reason=none`
    through the real T2 UI (ready + two-rAF ack + object mount).
  - Raw numbers for T5: Svelte `src/` now 7 files (adds `plane.js`, 83
    lines); `App.svelte` 636 lines. T3 inherits the state handlers it must
    extend into panels and the clone records that already carry
    `cloneState`/`caption` for the pill.
- L6 (2026-11-07, orchestrator): slice boundary recorded — 4 commits /
  ~2600 authored lines on `feature/svelte-ui` since `origin/main`
  (T1 scaffold + fix, T2 plane + docs). Per S1 this branch does not merge
  until the human decides with the measurements in hand (T5); the chained
  review slices, if any, are decided at that delivery, not now.
- L7 (2026-11-07, user, verbatim): «ese review a veces me falla ya lo
  deshabilite, los commits de la rama de svelte los puedes hacer sin
  problema asi tendre tambien visibilidad de los cambios, y puedes dejar
  que el agente haga los commits al finalizar el cambio solo que no puede
  hacer pr ni merge, ya sabes que esta rama sera solo para tener claro si
  existira un beneficio en esta migracion temprana, luego segui con el
  cambio del motor este es importante ya que lleva bastante escritura».
  Consecuencias: (1) el review nativo quedó deshabilitado por el humano —
  no se invoca más en este corte; (2) los commits sobre
  `feature/svelte-ui` están autorizados, y los workers pueden commitear su
  unidad al cerrarla; (3) PR y merge siguen PROHIBIDOS en esta rama (S1
  queda en pie); (4) el propósito de la rama es medir el beneficio de la
  migración temprana (S4); (5) el corte continúa después del motor (f10).
- L8 (2026-11-07, worker, T3: S2): the chrome ported — panels, versions,
  themes and the clone tools. The Svelte build reaches functional parity with
  the vanilla page: every item of CONTRACT.md §8 is now marked (C6,
  C11 clone-move half, C16 switch, C22-C38, C41-C42), with the wire strings,
  the payload keys and the numbers of the map; the C38/C39 pair was already
  done in T1.
  - Files (all inside `crates/gx-app/frontend-svelte/`): `src/App.svelte`
    (1398 lines: state, wire, camera, objects, clones, clone move, keyboard,
    banner, ghost, the settings throttle), `src/app.css` (the whole vanilla
    `style.css`, 1368 lines, byte for byte plus the `#app { height: 100% }`
    rule the Svelte mount needs — the seven `[data-theme]` blocks and the
    `.graphium` drawing classes were missing while the port was partial), NEW
    `src/lib/PlanPanel.svelte` (the list, `×`, `⧉`, the `◀ v3/7 ▶` history),
    NEW `src/lib/SessionsPanel.svelte` («Mi plano» + one row per session + the
    notice), NEW `src/lib/SettingsPanel.svelte` (12 controls, 7 theme chips
    with their four dots, Escape/focus), NEW `src/lib/CloneTools.svelte` (the
    pill: `◀ n/m ▶`, `↻`, the `×` inside it, the overhang), NEW
    `src/lib/presets.js` (PLAN_PRESETS/THEME_PRESETS/THEME_CHIPS), NEW
    `src/lib/labels.js` (the built Spanish strings, `es` short date, the
    notice), NEW `src/lib/history.js` (where an indicator sits, what a step
    asks for), `README.md` and `CONTRACT.md` (§7 tables + §8 marks + §9).
    The scaffold diagnostic chip is gone, as §9 of the contract said.
  - Gates (real exit codes, captured without pipes):
    `npx vite build` → 0 (JS 72.88 kB / 26.29 kB gzip, CSS 16.71 kB / 3.64 kB
    gzip, HTML 0.58 kB; `dist/` 90,193 B, 3 files);
    `cargo test -p gx-app` → 0 (22 suites, `test result: ok`, 0 failed;
    `git diff --stat main -- crates/gx-app/frontend/` empty — the vanilla path
    is byte-identical and the tests that `include_str!` it never see Svelte);
    `TAURI_CONFIG='{"build":{"frontendDist":"frontend-svelte/dist"}}' cargo
    build -p gx-app` → 0 (binary embeds `assets/index-BCk072b7.js` +
    `assets/index-BqaTbq4Y.css` and zero `plan.js`);
    `DISPLAY=:0 scripts/svelte-smoke.sh` → `rendered=true reason=none`
    (ready + paint ack through the real T3 UI).
  - Verification beyond the gates (no JS test runner in this package and
    adding one would mutate dependencies, so these are evidence of the task,
    not committed tests; the harness lives only in `/tmp` and was removed with
    the session): a browser harness runs the **real built bundle** with a
    faked Tauri bridge and checks 92 DOM-observable rules of §8 — nothing
    leaves the page before the first `graphium://settings`, a burst of two
    slider inputs is exactly one `settings-changed`, a repeated value sends
    nothing, the palette/`data-theme`/grid repaint at once, the panel item and
    the `v3/7` indicator, `◀` → `graphium://version-view{version:10}` and `▶`
    back to the live drawing with no event, the preview painted on the object,
    the switcher rows and the verbatim notice, both empty states, the pill
    (`2/7`, refresh disabled once `updatable` is false with
    `el original ya no está en el plan`, `graphium://clone-view` with/without
    version, `graphium://clone-remove`), the keyboard and its input guard, and
    the banner (`+2 errores`) and the ghost (`calculando layout…` → `4 nodos, 3
    edges` → hidden by error and by render). Two pixel-for-pixel comparisons
    against the vanilla page: the default state and a rich state (light theme,
    two diagrams, one clone, the settings panel open, a version preview) render
    **byte-identical screenshots** (1280×800, same PNG hash).
  - Typeface/LOC for T5 (raw numbers, not the benchmark yet): Svelte `src/`
    is now 14 files / 4,057 lines (App.svelte 1398, app.css 1368, the six
    lib modules 499, the four components 605, main.js 10) vs the vanilla
    133,290 B / 4,059 lines. `dist/` 90,193 B vs 44K at T1 and 52K at T2.
  - Findings left for T4 (recorded in CONTRACT.md §8/§9): (1) the clone pill
    measures itself against the **object** width, so a wide clone keeps it at
    the corner; the vanilla measured the `.diagram` container, which has no
    width (every child is absolute), so it always applied `.overhang` — the
    port follows the written L8/C31 rule, and this is the one observed visual
    difference on wide clones (verified by reading the class of both pages in
    the same state). (2) `+N errores` is unreachable: Rust never sends
    `extra` (§5.7). (3) `graphium://pending` still has no emitter: the handler
    is kept exactly as the vanilla page has it and the event is not invented
    (C36), so the ghost is unreachable from Rust today.
  - Commits on `feature/svelte-ui` (L7; no push, no PR, no merge):
    `11d8a97` feat(ui): the chrome — panels, versions, themes and the clone
    tools (code + CONTRACT.md + README.md), and the docs commit of this
    entry. What stays open for T4: the item-by-item walk of §8 with the
    checklist in hand, the A/B switch documented end to end, and the two
    findings above; T5 keeps the bench harness and the comparative results.
- L9 (2026-11-07, user decision via orchestrator): the Svelte diagram
  LIBRARY is out of this cut's scope — «primero medir». The human confirmed
  the factual review (the user sees an SVG picture: Rust's renderer bakes
  theme colors as literal RGB, the page mounts it opaquely with no
  per-node attributes) and chose to keep this branch a FAIR measurement:
  T4 walks the parity checklist and T5 benches vanilla vs Svelte as both
  render SVG today. The native diagram library (real elements per
  node/edge, interactive, the F3 of the PRD) opens as its own cut (f11)
  with the T5 numbers in hand; its first spike is an overlay (Rust emits
  node positions it already computes, Svelte mounts hit-areas) before any
  native re-render. Scope guard: nothing in this branch may drift into
  native rendering or interactive overlays.
