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
- T3 (S2; route: worker; commit: —): port the panels — per-kind forms,
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
