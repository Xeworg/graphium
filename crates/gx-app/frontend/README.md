# Graphium — the plan (Svelte + Vite)

The plan of Graphium as a Svelte + Vite app. It is **the only frontend of the
tree** since the single-frontend cut
(`odd/tasks/frontend-consolidacion.md`): `crates/gx-app/tauri.conf.json` says
`frontendDist: "frontend/dist"`, and `tauri::generate_context!()` embeds that
bundle in the binary. The vanilla page and the `TAURI_CONFIG` A/B switch are
gone; `CONTRACT.md` §10 keeps the historical note and the coverage map.

Read [`CONTRACT.md`](./CONTRACT.md) first: it maps every `graphium://` intent,
every event, every ack and timeout and every panel, and its §8 is the parity
checklist.

## Build and run

The order is not a preference: the bundle has to exist when cargo compiles,
because the codegen of `tauri::generate_context!()` reads `frontendDist` at
compile time. On a clean checkout a bare `cargo build` stops with «the
`frontendDist` configuration is set to `frontend/dist` but this path doesn't
exist».

```bash
# From the repository root.
scripts/build.sh              # npm ci (when needed) + npm run build + cargo build -p gx-app
scripts/build.sh --run        # ... and open the window
scripts/build.sh --release    # the release binary instead

# The in-shell smoke test: paints a diagram through the MCP door and reports
# what Rust says came back. `rendered=true` is only possible after
# `graphium://ready` (else `ui_error`) AND `render_ack` (else `ui_timeout`).
DISPLAY=:0 scripts/smoke.sh
```

`scripts/build.sh` runs `npm ci` only when `node_modules` is missing (or
`--clean` is passed) and then `npm run build`; the bundle lands in
`crates/gx-app/frontend/dist/`, which stays out of git. Node 22 and npm are
build dependencies, next to `webkit2gtk-4.1` and `gtk3`; CI runs the same two
steps before `cargo build --workspace` (`.github/workflows/ci.yml`), and so do
`install.sh` and `release.yml`.

If you prefer the raw commands, they are exactly:

```bash
cd crates/gx-app/frontend
npm ci && npm run build
cd ../../..
cargo build -p gx-app
```

There is no env var or flag that selects a frontend: the binary always embeds
the Svelte bundle. The evidence is in the bytes:

```bash
strings -a target/debug/graphium | grep -o 'assets/index-[A-Za-z0-9_-]*\.js'  # one hit
strings -a target/debug/graphium | grep -c 'plan\.js'                         # 0
```

## What is ported

T1: the bridge (`src/lib/tauri.js`) with the same two doors as the vanilla page,
the `graphium://ready` handshake and the `render_ack` of a render, the isometric
grid of PRD §7.1 RF24/RF25 with the contract numbers of
`crates/gx-app/tests/grid.rs` (`src/lib/grid.js`, `src/app.css`), and
`data-theme` plus the palette variables of `graphium://settings`.

T2: the plane — the camera (wheel anchored at the cursor, the 250 ms cubic pan,
camera intents, the throttles, resize), the objects with their box and their
`×`, the culling (≤ 3 live SVGs, `plan-hidden`), and the clones of the user plan
as objects of the plane (`src/App.svelte`, `src/lib/plane.js`).

T3: every remaining piece of chrome — the panel of open diagrams with the
`◀ v3/7 ▶` history, the switcher of plans («Mi plano» first), the settings panel
(the 12 controls, the 7 theme chips with their palettes, Escape and the focus
back to the gear), the tools pill of a clone (history, «Actualizar el clon», the
`×` that removes the view instance), the clone move by drag, the `←`/`→` of the
selected object, the two empty states, the error banner and the render ghost
(`src/lib/*.svelte`, `src/lib/{presets,labels,history}.js`). The whole vanilla
`style.css` — the seven theme blocks and the `.graphium` drawing classes
included — is `src/app.css`.

T4: the parity certification. `CONTRACT.md` §8 carries one evidence line per
checklist item, produced by the harness in `tools/`. T4 was a comparison
against the vanilla page; with the page retired the harness asserts the same
rules on the one bundle (see `CONTRACT.md` §10 for what that loses and where
each rule lives now).

## The harnesses (committed, no new dependencies)

The suite runs the **real built bundle** in headless Chromium over HTTP, with a
faked Tauri bridge injected before the bundle evaluates (`__TAURI__` with the
`event` door, or `__TAURI_INTERNALS__` with the IPC door, or nothing). That is
how the DOM, the wire payloads, the throttles and the culling are observed
without instrumenting Rust. Node 22's global `WebSocket` drives Chromium over
the DevTools protocol; nothing was added to `package.json` (the devDependencies
are unchanged: `svelte`, `@sveltejs/vite-plugin-svelte`, `vite`).

```bash
# The §8 checklist on the built bundle (exit 0 = every probe passed).
node tools/parity.mjs [--chromium /path/to/chrome] [--json=/tmp/parity.json]

# Asset weight, LOC, and the headless half of the timings.
node tools/bench.mjs --assets
node tools/bench.mjs --repeats=5 --json=/tmp/bench.json

# The real window, the MCP door, and the memory of the process tree.
DISPLAY=:0 tools/bench-live.sh \
    --binary target/debug/graphium --label svelte --run 1 --json=/tmp/live-1.json
```

The A/B results of T5 are in `odd/tasks/svelte-ui-t5-bench.md`; the harnesses no
longer have a vanilla side to compare against.

## Writing rules that come from the shell, not from taste

- **No inline styles in markup.** The CSP of the window is `default-src 'self'`
  (`crates/gx-app/tauri.conf.json`), so `style="…"` and inline `<style>` are
  off the table. CSS variables are published with CSSOM (`setProperty`), which
  is the rule C43 checks.
- **`base: "./"`** in `vite.config.js`: the bundle is embedded in the binary and
  served by the `tauri://` asset protocol from the root of `dist`, so absolute
  `/assets/…` URLs would leave the embedded directory.
- **Spanish strings, verbatim.** The UI text is the user's language; the port
  copies the old strings, it does not translate them (C41 checks it).
