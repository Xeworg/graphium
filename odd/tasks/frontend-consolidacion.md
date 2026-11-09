# frontend-consolidacion — one `frontend/` directory, the Svelte one

Branch `feature/frontend-consolidacion` from `staging` @ 622642c (the
svelte-ui merge). The user's ask, verbatim, and the reason:

## Specs

- S1 (user, verbatim): «he notado que la carpeta donde esta todo se llama
  frontend-svelte y otra que se llama frontend hay que arreglar eso y dejar
  una de frontend solamente confundira al usuario que quiera estudiar el
  proyecto». ONE directory named `frontend/` — the Svelte app; the vanilla
  page leaves the tree.
- S2 (user, verbatim): «hazle el merge a staggin y procede luego con ese
  arreglo» — this cut runs right after the svelte-ui merge; the TAURI_CONFIG
  A/B switch loses its reason to exist and is retired.

## Tasks

- T1 (S1 S2; route: worker; commit: —): consolidate — remove the vanilla
  `frontend/`, `git mv frontend-svelte → frontend`, point the Tauri build at
  the built assets (`frontendDist: "frontend/dist"` + the before-build step
  that runs the Vite build), retire the A/B switch scripts, repoint every
  Rust test that reads the vanilla files to the Svelte sources with the same
  pinned numbers, and update every doc that names the old paths.

## Notes for the worker (evidence taken on this tree)

- `tauri.conf.json` says `"frontendDist": "frontend"` (raw files, no build).
  The Svelte app needs its Vite build BEFORE the cargo build: wire the
  before-build step (tauri `build.beforeBuildCommand` or the script the CI
  uses — read ci.yml) so a plain `cargo build` after `npm run build` works
  and the DEFAULT build embeds the Svelte bundle. dist/ stays gitignored.
- Rust tests read the vanilla files by path (`tests/grid.rs` pins the
  contract numbers living in `plan.js`; CONTRACT.md §6 lists them). The
  numbers are identical in `frontend/src/lib/contract.js` (T2/T3 kept them
  byte-equal) — repoint the tests there; the pinned values must not change.
- Scripts: `scripts/svelte-tauri.sh` (the switch: --vanilla / TAURI_CONFIG)
  and `scripts/svelte-build.sh` lose the switch; keep ONE way to build+run
  (npm build → cargo build), simplify or fold them; `scripts/svelte-smoke.sh`
  keeps working with the default build (rename or keep, your call — document
  it). CONTRACT.md §10 (the switch) becomes a historical note marked retired;
  README.md updated to the single-frontend story.
- References to sweep: CONTRACT.md, README.md, odd/tasks/svelte-ui.md (leave
  history intact — add nothing but a path note if needed), docs/PRD.md,
  docs/QUICKSTART.md, ci.yml, package.json location (moves with the dir).
- npm install must work in the new location; `npx vite build` from
  `frontend/`; then `cargo build -p gx-app` with NO TAURI_CONFIG must embed
  the Svelte bundle (`strings target/debug/graphium | grep -c plan.js` = 0,
  `assets/index-*.js` present) and `DISPLAY=:0 scripts/svelte-smoke.sh` →
  `rendered=true reason=none`.

## Log

- L1 (2026-11-07, user): the two verbatims in S1/S2, after reading the bench
  («si ganamos mucho a poco precio») and deciding the merge to staging.
- L2 (2026-11-07, orchestrator): merge `622642c` integrated svelte-ui into
  staging (clean, no conflicts — the branch only added files; `frontend/`
  was byte-identical on both sides since F8). This cut opens on
  `feature/frontend-consolidacion` from that merge.
- L3 (2026-11-07, T1): **one frontend.** `crates/gx-app/frontend` is the
  Svelte app: the vanilla `index.html`/`plan.js`/`style.css` were removed and
  `frontend-svelte/` was moved onto the name (`git rm -r` + `git mv`, history
  kept). `tauri.conf.json` says `"frontendDist": "frontend/dist"`; there is no
  `TAURI_CONFIG` and no A/B. **Build path:** `scripts/build.sh`
  (`npm ci` when needed → `npm run build` → `cargo build -p gx-app`; `--clean`,
  `--run`, `--release`), and `scripts/smoke.sh` is the in-shell smoke test
  (renamed from `svelte-smoke.sh`; same body). `svelte-build.sh` and
  `svelte-tauri.sh` are gone. **Before-build wiring:** `cargo build` never runs
  the Tauri CLI hooks, so `build.beforeBuildCommand` would be dead code in this
  repository; the order lives where it runs: `scripts/build.sh`, `ci.yml`
  (Node 22 + `npm ci` + `npm run build` before `cargo build --workspace`, plus
  the parity step), `release.yml` (both jobs) and `install.sh`
  (`require_node_toolchain` + `build_frontend_bundle`, with an actionable error
  if node/npm are missing). Without the bundle `generate_context!()` fails with
  «the `frontendDist` configuration is set to `frontend/dist` but this path
  doesn't exist» — that is why every cargo entry point grew the frontend step.

  **Tests.** `grid.rs` repointed to `src/app.css` + `contract.js`/`grid.js`
  (assertions untouched, 7/7 green) and the one `camera_fit.rs` resize
  assertion now reads `App.svelte`'s `onResize` (7/7). New
  `tests/contract_numbers.rs` (8 tests) pins every contract number against
  `src/lib/contract.js`: 59, 45, `59·sin(45°) = 41.72`, the `--zoom` range
  0.05…4.0, the 100 ms throttles, the 250 ms pan, the 0.3 grid hide, the 24 px
  padding, the 3-live-SVG ceiling and the 0.4 cull zoom; plus the 13
  `SETTINGS_KEYS` against `Settings`, the keys `settingsFromPayload` reads,
  `THEMES` against `Theme::ALL`, the 18 event names against `app.rs` and the 8
  presets of `presets.js` against `settings::presets`. The six page suites
  (`culling`, `panel`, `sessions_panel`, `settings_panel`, `user_plan_page`,
  `versions`; 55 tests) were retired with the page: they asserted the vanilla
  implementation structure (function bodies, DOM ids), which no longer exists.
  `CONTRACT.md` §10 carries the coverage map — contract numbers →
  `contract_numbers.rs`/`grid.rs`; the §8 observable rules →
  `tools/parity.mjs` in CI; the wire (B1-B10/R1-R8) → `end_to_end.rs`,
  `user_plan.rs`, `settings.rs`, `round_trip.rs`, `placement.rs`,
  `camera_fit.rs`; the real webview → `scripts/smoke.sh`.

  **Parity harness.** `tools/parity.mjs` is no longer differential: it runs the
  one built bundle and asserts each probe (57/57 probes, exit 0 in CI's
  Chromium step). C42 (app.css vs the vanilla stylesheet) retired with the page;
  C9/C41/C43 now check the Svelte source and C40 keeps the numbers in
  `contract.js`. `tools/bench.mjs` lost its vanilla side; `bench-live.sh`
  points at the moved `tools/diagram.mjs`. The A/B numbers stay in
  `odd/tasks/svelte-ui-t5-bench.md`.

  **Reference sweep.** `crates/gx-app/tauri.conf.json`; the app
  `README.md`/`CONTRACT.md`, `index.html` comment, `.gitignore`,
  `package.json` + `package-lock.json` (`graphium-frontend-svelte` →
  `graphium-frontend`, both name fields so `npm ci` agrees with the lock) and
  `tools/*` headers/paths; `docs/PRD.md` §8 (tree and ownership rows) and
  `docs/QUICKSTART.md` §1; root `README.md` "Compilar";
  `extensions/pi/graphium/README.md`; `ci.yml`; `release.yml`; `install.sh`.
  `odd/tasks/svelte-ui.md` and `odd/tasks/svelte-ui-t5-bench.md` stay as
  written (history), and CONTRACT.md's delivery records quote the paths and
  commands of the branch they were taken on.

  **Evidence (this worktree).** `npx vite build` → 0 (bundle 72.88 kB js /
  16.71 kB css); `cargo build -p gx-app` with `TAURI_CONFIG` unset → 0
  (`strings target/debug/graphium | grep -c plan\.js` = 0;
  `assets/index-BCk072b7.js` embedded); `cargo test --workspace` → 0 (0
  failed); `cargo fmt --all -- --check` → 0; `cargo clippy --workspace
  --all-targets -- -D warnings` → 0; `node tools/parity.mjs` → 0 (57/57);
  `DISPLAY=:0 scripts/smoke.sh` → `rendered=true reason=none`;
  `find crates/gx-app -name plan.js` → empty. Step 5 extended the edit
  surfaces to `release.yml`, `install.sh`, root `README.md` and
  `extensions/pi/graphium/README.md`; each got only the npm step.
