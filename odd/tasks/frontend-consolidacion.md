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
