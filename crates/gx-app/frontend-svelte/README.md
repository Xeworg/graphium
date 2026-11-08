# Graphium — Svelte frontend (experimental)

The plan of Graphium as a Svelte + Vite app. It is one of the two frontends of
this branch: the vanilla one (`../frontend/`) stays the **default**
`frontendDist` and the one the Rust tests pin with `include_str!`; this one is
selected with a config override, so both stay runnable from the same commit
(S4 of `odd/tasks/svelte-ui.md`).

Read [`CONTRACT.md`](./CONTRACT.md) first: it maps every `graphium://` intent,
every event, every ack and timeout and every panel of the vanilla page, and its
§8 is the parity checklist T4 certified item by item.

## A/B: the two frontends, end to end

Both builds come from the same commit and end in the same binary path,
`target/debug/graphium`; the only thing that decides which frontend is inside is
the `TAURI_CONFIG` build variable. Nothing in `crates/gx-app/tauri.conf.json`
changes, so the default build (and the Rust tests that read the vanilla files)
never see the Svelte bundle.

### 1. Build each side

```bash
# From the repository root.

# Svelte side: the Vite bundle, then the binary that embeds it.
scripts/svelte-build.sh
TAURI_CONFIG='{"build":{"frontendDist":"frontend-svelte/dist"}}' cargo build -p gx-app

# Vanilla side: the same binary, no override.
env -u TAURI_CONFIG cargo build -p gx-app
```

`scripts/svelte-build.sh` runs `npm ci` (only when `node_modules` is missing or
`--clean` is passed) and `npm run build`; the bundle lands in
`crates/gx-app/frontend-svelte/dist/`. `scripts/svelte-tauri.sh` does the build
half of both sides for you (`scripts/svelte-tauri.sh` = Svelte,
`scripts/svelte-tauri.sh --vanilla` = vanilla).

Because the two builds share `target/debug/graphium`, keep one of them aside if
you want to run both without rebuilding:

```bash
TAURI_CONFIG='{"build":{"frontendDist":"frontend-svelte/dist"}}' cargo build -p gx-app
cp target/debug/graphium /tmp/graphium-svelte
env -u TAURI_CONFIG cargo build -p gx-app
cp target/debug/graphium /tmp/graphium-vanilla
```

### 2. Run each side

```bash
# The window (the human's app).
scripts/svelte-tauri.sh --run          # build + run the Svelte frontend
scripts/svelte-tauri.sh --run --vanilla # build + run the vanilla one

# Or any of the two binaries, as the app, as the MCP server, or asking the
# running owner to leave.
/tmp/graphium-svelte
/tmp/graphium-svelte --stdio           # MCP over stdin/stdout (what agents use)
/tmp/graphium-svelte quit

# The in-shell smoke test: paints a diagram through the MCP door and reports
# what Rust says came back. `rendered=true` is only possible after
# `graphium://ready` (else `ui_error`) AND `render_ack` (else `ui_timeout`).
DISPLAY=:0 scripts/svelte-smoke.sh /tmp/graphium-svelte
DISPLAY=:0 scripts/svelte-smoke.sh /tmp/graphium-vanilla
```

Both binaries are the same application: the difference lives in the bytes of the
embedded bundle, not in a flag, an argument or a label in the window. There is
no diagnostic chip any more — the two UIs are meant to be indistinguishable
(T4 exists to check that).

### 3. How to tell them apart

```bash
# The Svelte binary embeds the Vite chunks; the vanilla one embeds plan.js.
strings -a /tmp/graphium-svelte  | grep -o 'assets/index-[A-Za-z0-9_-]*\.js'  # one hit
strings -a /tmp/graphium-svelte  | grep -c 'plan\.js'                        # 0
strings -a /tmp/graphium-vanilla | grep -c 'plan\.js'                        # 1
```

With the window open, the DOM tells the same story: the Svelte page mounts under
`#app` (`<div id="app"><div id="plan" …>`), and its `.diagram` objects carry the
id as a `data-id` attribute, while the vanilla objects keep it as a JS property.
None of that is visible to the human, which is the point of the parity run.

### 4. The exact `TAURI_CONFIG` mechanics

`TAURI_CONFIG` is a JSON **merge patch** applied over the parsed
`crates/gx-app/tauri.conf.json` in two places, at build time and at compile time:

1. `tauri-build-2.7.4/src/lib.rs:655,681` — the build script reads the variable,
   parses it as JSON and merges it into the config it hands to the codegen; it
   also emits `cargo:rerun-if-env-changed=TAURI_CONFIG`, so changing the
   variable recompiles `gx-app` (and nothing else).
2. `tauri-codegen-2.7.4/src/lib.rs:56-91` — `tauri::generate_context!()` reads
   the same variable and merges it the same way before resolving the config, so
   the `frontendDist` embedded in the binary is the one the variable selected.

Relative paths in the config are resolved against the directory of
`tauri.conf.json`, i.e. `crates/gx-app/`; that is why the value is
`frontend-svelte/dist` and not `crates/gx-app/frontend-svelte/dist`.

A merge patch replaces whole values, so the override is exactly:

```json
{"build":{"frontendDist":"frontend-svelte/dist"}}
```

Anything else in `tauri.conf.json` stays as written. Building with the variable
unset (`env -u TAURI_CONFIG cargo build -p gx-app`) is the vanilla build.

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
checklist item, produced by the harness in `tools/`; this README documents the
A/B switch from build to binary.

## The harnesses (committed, no new dependencies)

The suite runs the **real built bundle** in headless Chromium over HTTP, with a
faked Tauri bridge injected before the bundle evaluates (`__TAURI__` with the
`event` door, or `__TAURI_INTERNALS__` with the IPC door, or nothing). That is
how the DOM, the wire payloads, the throttles and the culling are observed
without instrumenting Rust — and why the same probe can run against the vanilla
page, which is what makes the parity claim a comparison instead of an opinion.
Node 22's global `WebSocket` drives Chromium over the DevTools protocol; nothing
was added to `package.json` (the devDependencies are unchanged: `svelte`,
`@sveltejs/vite-plugin-svelte`, `vite`).

```bash
# T4: the §8 checklist, both builds, 58 probes (exit 0 = certified).
npx vite build
node crates/gx-app/frontend-svelte/tools/parity.mjs [--build=svelte|vanilla] [--json=/tmp/parity.json]

# T5: asset weight, LOC, and the headless half of the timings.
node crates/gx-app/frontend-svelte/tools/bench.mjs --assets
node crates/gx-app/frontend-svelte/tools/bench.mjs --both --repeats=5 --json=/tmp/bench.json

# T5: the real window, the MCP door, and the memory of the process tree.
DISPLAY=:0 crates/gx-app/frontend-svelte/tools/bench-live.sh \
    --binary /tmp/graphium-svelte --label svelte --run 1 --json=/tmp/live-svelte-1.json
```

The results of T5 are in `odd/tasks/svelte-ui-t5-bench.md`.

## Writing rules that come from the shell, not from taste

- **No inline styles in markup.** The CSP of the window is `default-src 'self'`
  (`crates/gx-app/tauri.conf.json`), so `style="…"` and inline `<style>` are
  off the table. CSS variables are published with CSSOM (`setProperty`), which
  is what the vanilla page does too — C43 checks it.
- **`base: "./"`** in `vite.config.js`: the bundle is embedded in the binary and
  served by the `tauri://` asset protocol from the root of `dist`, so absolute
  `/assets/…` URLs would leave the embedded directory.
- **Spanish strings, verbatim.** The UI text is the user's language; the port
  copies the vanilla strings, it does not translate them (C41 checks it).
