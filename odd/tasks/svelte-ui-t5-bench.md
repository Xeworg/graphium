# svelte-ui — T5, the comparative bench (vanilla vs Svelte)

Measured on `feature/svelte-ui` @ `d7eab44` (the T3 tree), from the two binaries
built by the same commit, under the same scenarios. The harness that produced
the numbers is committed with this document:

| Tool | What it measures |
| --- | --- |
| `crates/gx-app/frontend-svelte/tools/bench.mjs` | asset weight, lines of code, and — headless Chromium, real bundle, faked Tauri bridge — page load → `graphium://ready` and `graphium://render` → `render_ack ok:true` |
| `crates/gx-app/frontend-svelte/tools/bench-live.sh` | the real binary on `DISPLAY=:0`, through the MCP door: cold start, render round trip, steady-state memory of the process tree |
| `crates/gx-app/frontend-svelte/tools/diagram.mjs` | the fixture builder (a `render_diagram` document with N nodes) |
| `crates/gx-app/frontend-svelte/tools/parity.mjs` | the T4 checklist walker (the same bridge trick, one probe per §8 item) |

Environment: Fedora 44, kernel 7.2.15, 20 cores, 31.7 GiB RAM, Chromium
156.0.8078.79 (headless), WebKitGTK 2.54.2 (`webkit2gtk4.1`), Node 22.22.2,
`DISPLAY=:0` for the live half. The machine was not idle (other worktrees of
this repo had owners running, see the memory section); every scenario was run
at least twice and both runs are reported.

## 1. Frontend asset weight

`node tools/bench.mjs --assets` — bytes on disk and gzip -9 of the exact files
each binary embeds.

| Files | Raw B | Gzip B |
| --- | ---: | ---: |
| **vanilla total** (3 files) | **133,290** | **36,333** |
| `plan.js` (the logic) | 89,635 | 24,700 |
| `style.css` | 33,304 | 8,993 |
| `index.html` (all the markup) | 10,351 | 2,640 |
| **Svelte total** (`dist/`, 3 files) | **90,193** | **30,078** |
| `assets/index-BCk072b7.js` (markup + logic + runtime) | 72,887 | 26,076 |
| `assets/index-BqaTbq4Y.css` | 16,718 | 3,614 |
| `index.html` (the mount point) | 588 | 388 |
| **Delta** | **−43,097 (−32.3 %)** | **−6,255 (−17.2 %)** |

Split by kind: JS raw −18.7 % (72,887 vs 89,635) but **JS gzip +5.6 %**
(26,076 vs 24,700); CSS raw −49.8 % (16,718 vs 33,304), CSS gzip −59.8 %
(3,614 vs 8,993).

Reading: the Svelte bundle is a third smaller on disk, but the advantage almost
disappears over the wire because the hand-written `plan.js` is repetitive and
compresses better than a minified Vite bundle with a Svelte runtime inside. The
CSS win is real (Vite minifies; `style.css` is written by hand). And the split
matters for caching: the vanilla page serves markup (10.3 kB) and logic (89.6 kB)
as separate, cacheable files; the Svelte app is one hash-named JS blob that
includes the markup, so any change invalidates all of it.

## 2. Page load → `graphium://ready`

`node tools/bench.mjs --both --repeats=5`: both bundles served over HTTP from
the same Chromium, pages opened alternately (svelte, vanilla, svelte, …) so a
machine that drifts does not land on one side only. `performance.now()` at the
moment the faked bridge records `graphium://ready`, in ms.

| Run | Svelte (5 loads) | Vanilla (5 loads) |
| --- | --- | --- |
| 1 | 48.2, 30.1, 25.1, 29.0, 26.1 — median 29.0 | 16.1, 15.0, 20.3, 17.3, 17.8 — median 17.3 |
| 2 | 51.5, 27.9, 24.5, 27.0, 27.1 — median 27.1 | 17.7, 19.0, 15.6, 17.8, 17.6 — median 17.7 |

Svelte announces itself ~10 ms later, warm and cold, in both runs. That is a
difference in *what the event means* as much as in speed: the vanilla page emits
`graphium://ready` at the end of `plan.js` (before the browser has painted
anything), while the Svelte page emits it from `onMount`, after the component
tree exists and the `$effect`s that publish the grid and the palette have run.
10 ms is below one frame at 60 Hz; nothing in the handshake changes.

## 3. `graphium://render` → `render_ack ok:true`

Same harness, same page: the render is dispatched with the fixture, the clock
runs to the ack the page emits. Two fixtures, built programmatically
(`drawingSvg`): **small** = 12 nodes / 11 edges (1.9 kB), **large** = 300 nodes /
299 edges (~48 kB) — 300 is the MCP ceiling (`too_large` above it, found the
hard way). Two samples per case per load, five loads per build and run.

Headless, ms (10 samples per build per run):

| Run | Build | small (median) | large (median) |
| --- | --- | ---: | ---: |
| 1 | Svelte | 3.6 … 32.3 (29.7) | 23.7 … 29.5 (28.1) |
| 1 | Vanilla | 4.2 … 31.0 (29.0) | 24.9 … 30.4 (28.0) |
| 2 | Svelte | 6.3 … 30.4 (23.9) | 23.0 … 29.8 (27.5) |
| 2 | Vanilla | 8.3 … 30.6 (27.8) | 25.4 … 28.5 (27.9) |

The numbers are bimodal because the ack waits **two** `requestAnimationFrame`s
(C4): with headless Chromium's unpaced compositor that is ~4–17 ms, paced it is
~25–32 ms. Both builds land in the same two buckets, with the same spread. The
metric cannot resolve a difference: both mount the same SVG string, and the
mount is the parse of that string.

The real shell, `tools/bench-live.sh`, through the MCP door — the answer of
`render_diagram` includes Rust's layout, the wire, the paint and the ack, so it
is the end-to-end number the agent waits for (ms, two samples per cell):

| Scenario | Svelte run 1 | Svelte run 2 | Vanilla run 1 | Vanilla run 2 |
| --- | ---: | ---: | ---: | ---: |
| small (12 nodes) | 38 / 45 | 39 / 42 | 45 / 46 | 46 / 47 |
| large (300 nodes) | 487 / 490 | 484 / 470 | 495 / 481 | 498 / 473 |

`rendered=true reason=none` in all eight measurements. The ~480 ms of the large
case is Rust's layout, which both binaries pay identically; the frontend halves
of that number are inside the run-to-run noise (±10 ms).

## 4. Steady-state memory with the large diagram on screen

`tools/bench-live.sh`, after the 300-node diagram is painted and 3 s of idle:
the summed **Pss** (`smaps_rollup`) of the owner and every process of its tree
(the `--owner` process plus `WebKitWebProcess` and `WebKitNetworkProcess`), read
twice a second apart. Pss (not RSS) because WebKitGTK splits the window into
several processes that share most of their mapped libraries; RSS would count
each shared page once per process. RSS is reported too, for reference.

| Scenario | Svelte run 1 | Svelte run 2 | Vanilla run 1 | Vanilla run 2 |
| --- | ---: | ---: | ---: | ---: |
| Pss, kB (2 reads) | 249,299 / 249,307 | 254,142 / 254,150 | 248,198 / 248,202 | 248,506 / 248,514 |
| RSS, kB (2 reads) | 530,032 / 530,104 | 529,784 / 529,856 | 520,944 / 521,012 | 522,008 / 522,080 |

Svelte is +0.4 % in run 1 and +2.3 % in run 2 of Pss; RSS +1.7 % / +1.5 %. The
spread **between runs of the same build** (Svelte 249.3 → 254.1 MB) is larger
than the gap between builds in a run, so the honest reading is: same footprint,
leaning very slightly heavier on the Svelte side (the Svelte runtime and its
compiled component code live in the same web process as the page).

Method blind spots, stated: (a) Pss divides a shared page between its users, so
a library also mapped by a process *outside* the tree (another browser, the
shell) is counted as if only this tree used it; (b) a helper reparented out of
the tree would be missed (`pgrep -P` walks children); (c) the GPU/compositor
allocation of WebKitGTK is not fully visible in `smaps_rollup`; (d) the three
other Graphium owners running on this machine were excluded by filtering on the
owner's `XDG_RUNTIME_DIR`, but they still compete for memory and page cache.
`/usr/bin/time -v` was not used: it sees only the process it launches, and the
web process is a child of the owner, not of the launcher.

## 5. Lines of code (the maintainability number)

`node tools/bench.mjs --assets` counts `src/` of the Svelte app (not `dist/`,
not `tools/`) and the three vanilla files.

| Vanilla | Lines | Svelte `src/` | Lines |
| --- | ---: | --- | ---: |
| `plan.js` | 2,449 | `src/App.svelte` | 1,398 |
| `style.css` | 1,352 | `src/app.css` | 1,368 |
| `index.html` | 258 | `src/lib/*.svelte` (4) | 605 |
| | | `src/lib/*.js` (6) | 676 |
| | | `src/main.js` | 10 |
| **total** | **4,059** | **total** | **4,057** |

The port is **two lines shorter**, which is the least interesting true thing in
this document. The stylesheet is unchanged (1,352 → 1,368 lines: the mount rule
and the header comment) and dominates both totals; `src/app.css` is 34 % of the
Svelte total. What changed is the shape: 2,449 lines of one script that keeps
every piece of state in module scope became 1,398 lines of one component plus
eleven modules under `src/lib/` (four components, seven pure modules) and a
ten-line mount, with a written contract (`CONTRACT.md`), and the two halves of
the page — what the DOM says and what the state means — are no longer the same
function. That is a maintainability claim about **separation**, not about
volume, and the bench can only prove the volume.

## 6. The honest reading

**What Svelte wins.** Bytes on disk: −32 % raw, −43 kB, and the wire delta is
real too (−17 % gzip). CSS specifically: half the bytes (Vite minifies what was
hand-written). Structure: the same behaviour in thirteen documented files instead of
one 2,449-line script, with the pure arithmetic (`plane.js`, `grid.js`,
`history.js`) testable in isolation. And the time to `ready` is *not* dominated
by the bundle: 72.9 kB of minified JS with a runtime inside is within ~10 ms of
89.6 kB of hand-written JS.

**Where vanilla wins.** The JS gzip size (+5.6 %, 26 vs 24.7 kB): hand-written
code repeats itself and compresses; a minified bundle does not. The markup is
cacheable separately from the logic, and — the honest one — the vanilla page is
already written, already tested by the Rust suites that `include_str!` it, and
costs nothing to keep. Nothing in these numbers pays for the rewrite on its own.

**What is noise.** The small-diagram render (4–32 ms both sides, bimodal by
frame parity); the ±10 ms of the live round trip; the 1–5 MB of Pss between
runs. A difference below ~20 ms or ~5 MB cannot be claimed with this method.

**What this bench cannot see.** Chromium is not WebKitGTK: §2 and §3's headless
half measures the two frontends under one engine, and the in-shell numbers of
§3/§4 come from the real window but include Rust's layout in the same figure.
Neither half measures a *native* diagram library (per-node elements, hit areas,
interaction): both builds render the same SVG today, by design of this cut (L9).
The interactive/native comparison is the next cut's bench.

## 7. Reproduce it

```bash
# both binaries, from the same commit
scripts/svelte-build.sh
TAURI_CONFIG='{"build":{"frontendDist":"frontend-svelte/dist"}}' cargo build -p gx-app
cp target/debug/graphium /tmp/graphium-svelte
env -u TAURI_CONFIG cargo build -p gx-app
cp target/debug/graphium /tmp/graphium-vanilla

# §1, §5 (+ the headless half of §2 and §3)
node crates/gx-app/frontend-svelte/tools/bench.mjs --both --repeats=5 --json=/tmp/bench.json

# §3/§4, live, twice per binary
crates/gx-app/frontend-svelte/tools/bench-live.sh --binary /tmp/graphium-svelte  --label svelte  --run 1
crates/gx-app/frontend-svelte/tools/bench-live.sh --binary /tmp/graphium-vanilla --label vanilla --run 1
```

The JSON of every invocation used here is reproducible with those commands; the
tables above are those JSON files, transcribed.
