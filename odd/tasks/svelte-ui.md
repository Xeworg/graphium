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
