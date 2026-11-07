# Graphium — Svelte frontend (experimental)

The plan of Graphium as a Svelte + Vite app. It is one of the two frontends of
this branch: the vanilla one (`../frontend/`) stays the **default**
`frontendDist` and the one the Rust tests pin with `include_str!`; this one is
selected with a config override, so both stay runnable from the same commit
(S4 of `odd/tasks/svelte-ui.md`).

Read [`CONTRACT.md`](./CONTRACT.md) first: it maps every `graphium://` intent,
every event, every ack and timeout and every panel of the vanilla page, and it
is the parity checklist T4 walks.

## Build

```bash
# From the repository root.
scripts/svelte-build.sh        # npm (if needed) + vite build → dist/
```

Or by hand:

```bash
cd crates/gx-app/frontend-svelte
npm ci
npm run build
```

## Run it inside the Tauri shell

The switch is the `TAURI_CONFIG` environment variable, which `tauri-build` and
`tauri::generate_context!()` merge into `crates/gx-app/tauri.conf.json` with a
JSON merge patch; the relative path of `frontendDist` is resolved against the
config directory, so `frontend-svelte/dist` is this bundle. Nothing in the
default configuration changes.

```bash
# From the repository root.
scripts/svelte-tauri.sh            # build target/debug/graphium with this bundle
scripts/svelte-tauri.sh --run      # ... and run it (solo-ui mode)
scripts/svelte-tauri.sh --vanilla  # the default build (vanilla page), for the A/B
```

By hand:

```bash
TAURI_CONFIG='{"build":{"frontendDist":"frontend-svelte/dist"}}' cargo build -p gx-app
```

Switching the variable back and forth only recompiles `gx-app` (the build
script emits `cargo:rerun-if-env-changed=TAURI_CONFIG`).

## Evidence that it works inside the window

```bash
scripts/svelte-smoke.sh            # paints a diagram through the MCP door
```

It prints `rendered=true` only if the page announced itself
(`graphium://ready`) **and** confirmed the paint (`render_ack`), because a
missing ready ends as `ui_error` and a missing ack as `ui_timeout`. It uses its
own `XDG_RUNTIME_DIR` and `HOME` under `/tmp`, so it never talks to the owner of
the human, and it quits the owner it started.

## What is ported so far

T1 of `odd/tasks/svelte-ui.md`: the bridge (`src/lib/tauri.js`) with the same
two doors as the vanilla page, the `graphium://ready` handshake and the
`render_ack` of a render, the isometric grid of PRD §7.1 RF24/RF25 with the
contract numbers of `crates/gx-app/tests/grid.rs` (`src/lib/grid.js`,
`src/app.css`), and `data-theme` plus the palette variables of
`graphium://settings`. The contract map and the parity checklist are in
[`CONTRACT.md`](./CONTRACT.md).

T2: the plane — the camera (wheel anchored at the cursor, the 250 ms cubic
pan, camera intents, the throttles, resize), the objects with their box and
their `×`, the culling (≤ 3 live SVGs, `plan-hidden`), and the clones of the
user plan as objects of the plane (`src/App.svelte`, `src/lib/plane.js`).

T3: every remaining piece of chrome — the panel of open diagrams with the
`◀ v3/7 ▶` history, the switcher of plans («Mi plano» first), the settings
panel (the 12 controls, the 7 theme chips with their palettes, Escape and the
focus back to the gear), the tools pill of a clone (history, «Actualizar el
clon», the `×` that removes the view instance), the clone move by drag, the
`←`/`→` of the selected object, the two empty states, the error banner and the
render ghost (`src/lib/*.svelte`, `src/lib/{presets,labels,history}.js`). The
whole vanilla `style.css` — the seven theme blocks and the `.graphium` drawing
classes included — is `src/app.css`.

The scaffold diagnostic chip is gone: the two builds are told apart by the
`TAURI_CONFIG` switch, not by a label in the window. `CONTRACT.md` §8 says, item
by item, what is done and how it was observed.

## Writing rules that come from the shell, not from taste

- **No inline styles in markup.** The CSP of the window is `default-src 'self'`
  (`crates/gx-app/tauri.conf.json`), so `style="…"` and inline `<style>` are
  off the table. CSS variables are published with CSSOM (`setProperty`), which
  is what the vanilla page does too.
- **`base: "./"`** in `vite.config.js`: the bundle is embedded in the binary and
  served by the `tauri://` asset protocol from the root of `dist`, so absolute
  `/assets/…` URLs would leave the embedded directory.
- **Spanish strings, verbatim.** The UI text is the user's language; the port
  copies the vanilla strings, it does not translate them.
