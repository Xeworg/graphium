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

T1 of `odd/tasks/svelte-ui.md`:

- the bridge (`src/lib/tauri.js`) with the same two doors as the vanilla page;
- the `graphium://ready` handshake and the `render_ack` of a render, with a
  placeholder object instead of the real plane;
- the isometric grid of PRD §7.1 RF24/RF25 with the contract numbers of
  `crates/gx-app/tests/grid.rs` (`src/lib/grid.js`, `src/app.css`);
- `data-theme` and the palette variables from `graphium://settings`.

T2 (plane: camera, objects, clones, culling) and T3 (panels, versions, themes)
are still the vanilla page's job. The `src/App.svelte` component says which
parts are placeholders; `CONTRACT.md` §8 says which task owns each item.

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
