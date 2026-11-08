# CONTRACT — the frontend ↔ Rust contract of the Graphium plan

The parity checklist of the Svelte port (`odd/tasks/svelte-ui.md`, T1) and the
map T4 walks item by item.

> **Single frontend (2026-11-07).** `odd/tasks/frontend-consolidacion.md`
> removed the vanilla page and moved this app to `crates/gx-app/frontend/`.
> Citations of the form `plan.js:NNN` and `index.html:NNN` below are
> provenance for the vanilla snapshot the port was written against; those
> files no longer exist in the tree. The delivery records in §9 and the T4
> notes quote the paths, scripts and commands of the branch they were taken
> on (`frontend-svelte/`, `TAURI_CONFIG`, `scripts/svelte-*`): they are
> history, not instructions. §10 keeps the retired switch and the coverage
> map of the retired tests.

Every line reference points at this worktree (`feature/frontend-consolidacion`):

| File | What it is |
| --- | --- |
| `crates/gx-app/frontend/src/lib/contract.js` | the numbers, keys, themes and events of the contract |
| `crates/gx-app/frontend/src/app.css` | the stylesheet of the plan (the vanilla rules, kept) |
| `crates/gx-app/frontend/src/App.svelte` | the surface: camera, objects, clones, culling, wire |
| `crates/gx-app/src/app.rs` | the Tauri application: every listener and emit |
| `crates/gx-app/src/camera.rs` | the placement/fit numbers the page must match |
| `crates/gx-app/src/settings.rs` | `Settings`, the presets and their ranges |
| `crates/gx-app/capabilities/default.json` | the only events the window may use |

How to use it: the checklist in §8 is the work list. A section is only ported
when every one of its items is ticked, and the tick has to be backed by the
same wire strings, payload keys and contract numbers this document records —
not by "it looks the same".

T1 (this task) covers the scaffold: the bridge, the `graphium://ready`
handshake, the paint confirmation and the isometric grid. The item tables
therefore carry a **T1** column with `done`, `stub` or `T2`/`T3` (the task that
owns it).

---

## 1. The bridge

The app declares `withGlobalTauri: true` (`crates/gx-app/tauri.conf.json:10`)
and its window may only use three event permissions
(`crates/gx-app/capabilities/default.json:5,7-9`):
`core:event:allow-emit`, `core:event:allow-listen`, `core:event:allow-unlisten`.

`plan.js` uses two doors, in this order (`plan.js:292-331`):

1. `window.__TAURI__.event.emit/listen` — the global bundle;
2. `window.__TAURI_INTERNALS__.invoke("plugin:event|emit" | "plugin:event|listen")`
   — the same protocol, for a page where the bundle is not injected.

If neither exists the page logs `graphium: no hay puente de Tauri para emitir
<name>` and drops the event (`plan.js:315-316`). There is no other channel: no
`invoke` command of our own, no HTTP, no files.

**Svelte port:** `src/lib/tauri.js` (same two doors, same fallback, same
message).

## 2. JS → Rust: the intents

Every intent is emitted by the page; Rust listens with
`handle.listen(name, …)` on the app handle (the window event bridge). All
payloads are JSON objects.

| # | Event | Payload (keys → type) | Page (emit) | Rust (listen) | Rust answers |
| --- | --- | --- | --- | --- | --- |
| B1 | `graphium://ready` | `{}` | `plan.js:2449` | `app.rs:803` | `graphium://settings` + `graphium://state` |
| B2 | `camera-changed` | `tx:f64, ty:f64, zoom:f64` | `plan.js:410` (`notifyCamera`, `plan.js:404-412`) | `app.rs:727` (`CameraPayload`, `app.rs:614-619`) | nothing (Rust keeps the copy) |
| B3 | `render_ack` | `req_id:String, ok:bool` | `plan.js:1910` (`ok:false`), `plan.js:2042` (`ok:true`) | `app.rs:732` (`AckPayload`, `app.rs:621-625`) | nothing (resolves the wait of `app.rs:1162`) |
| B4 | `settings-changed` | the 13 `Settings` keys (§5.6) | `plan.js:770` (`notifySettings`, `plan.js:755-773`) | `app.rs:921` | `graphium://settings` |
| B5 | `graphium://close` | `diagram_id:String` | `plan.js:849` | `app.rs:747` (`ClosePayload`, `app.rs:749-751`) | `graphium://closed` + `graphium://state` |
| B6 | `graphium://version-view` | `diagram_id:String, version:u64` | `plan.js:1090` (`viewVersion`, `plan.js:1080-1105`) | `app.rs:772` (`VersionViewPayload`, `app.rs:774-777`) | `graphium://version-paint` or `graphium://error` |
| B7 | `graphium://clone` | `diagram_id:String` | `plan.js:861` | `app.rs:821` (`ClonePayload`, `app.rs:823-825`) | `graphium://clone-paint` + `graphium://state`, or `graphium://error` |
| B8 | `graphium://clone-view` | `clone_id:String, version:u64?` (`None` = «Actualizar el clon») | `plan.js:1838` (refresh), `plan.js:1872` (history step), `plan.js:2289` (hydrate a clone without drawing) | `app.rs:850` (`CloneViewPayload`, `app.rs:852-855`) | `graphium://state` then `graphium://clone-paint`, or `graphium://error` |
| B9 | `graphium://clone-move` | `clone_id:String, pos:{x:f64,y:f64}` | `plan.js:536` (`endDrag`, `plan.js:522-551`) | `app.rs:876` (`CloneMovePayload`, `app.rs:878-886`) | `graphium://state` |
| B10 | `graphium://clone-remove` | `clone_id:String` | `plan.js:1592` | `app.rs:905` (`CloneRemovePayload`, `app.rs:907-909`) | `graphium://state` |

Notes that the port must keep:

- `camera-changed`, `render_ack` and `settings-changed` carry **no**
  `graphium://` prefix (`app.rs:727`, `app.rs:732`, `app.rs:921`): a renamed
  event is a silent timeout, not an error.
- `render_ack` is emitted after **two** animation frames
  (`plan.js:2039-2044`): "the SVG is painted" (PRD §8.6.1 step 4).
- `render_ack ok:false` is only for a payload whose `svg` does not contain an
  `<svg>` element (`plan.js:1904-1912`).
- `camera-changed` is throttled to one event per `CAMERA_THROTTLE_MS`
  (100 ms, `plan.js:13`): the first call schedules, later calls inside the
  window return (`plan.js:404-412`).
- `settings-changed` is throttled the same way (`plan.js:14`, `plan.js:755-773`)
  and deduplicated with `settingsId` (`plan.js:744-746`): nothing moved, no
  file, no round trip.
- `graphium://clone-move` is only sent when the object really moved
  (`plan.js:530-540`): a click without movement asks for nothing.
- `graphium://version-view` at the newest version is not sent at all: the page
  restores the live drawing it already holds (`plan.js:1085-1105`).
- `graphium://clone-view` without `version` is the refresh
  (`plan.js:1830-1841`); with `version` it is a history step
  (`plan.js:1861-1877`).

## 3. Rust → page: the events

| # | Event | Payload (see §5) | Rust (emit) | Page (listen) |
| --- | --- | --- | --- | --- |
| R1 | `graphium://settings` | `Settings` | `app.rs:805` (on ready), `app.rs:945` (after a save) | `plan.js:2435-2444` |
| R2 | `graphium://state` | `PlanPayload` | `app.rs:809` (on ready), `app.rs:762` (close), `app.rs:840` (clone), `app.rs:863` (clone-view), `app.rs:896` (clone-move), `app.rs:916` (clone-remove), `app.rs:1172` (after a painted render) | `plan.js:2371-2388` |
| R3 | `graphium://render` | `RenderEvent` | `app.rs:1158` (the render hook) | `plan.js:2367-2370` |
| R4 | `graphium://version-paint` | `{diagram_id, version, svg}` | `app.rs:783-790` | `plan.js:2332-2358` |
| R5 | `graphium://clone-paint` | `ClonePaint` | `app.rs:839` (clone), `app.rs:864` (clone-view) | `plan.js:2360-2362` |
| R6 | `graphium://error` | `ErrorPayload` | `app.rs:796` (version-view), `app.rs:843` (clone), `app.rs:867` (clone-view) | `plan.js:2393-2396` |
| R7 | `graphium://closed` | `{diagram_id}` | `app.rs:758-761` | **not listened**: the page rebuilds from `graphium://state` |
| R8 | `graphium://pending` | `{nodes, edges}` (assumed) | **never emitted** in this tree | `plan.js:2390-2392` (dead: `showPending`/`hidePending`, `plan.js:2316-2330`) |

Notes:

- R2 is the only source of the panels (PRD §8.6.1, S8): `renderPanel`,
  `renderSessions` and `syncObjects` read nothing else.
- R1 carries the **sanitised** value the file holds: Rust saves, sanitises and
  re-reads before answering (`app.rs:927-945`), so the panel never shows a
  value that was not stored.
- R1 is ignored while a `settings-changed` is in flight
  (`plan.js:2439-2442`): the confirmation of an older value must not pull a
  slider out of the hand that is dragging it.
- R3's `session_id` names the plan the drawing belongs to; the page brings
  that plan on screen before placing the object (`plan.js:1914-1924`).
- R3's `focus` (plan units) wins over `camera`: the agent asked to show one
  node (PRD §7, RF27; `plan.js:2031-2036`).

## 4. Acks, timeouts and throttles

| # | Handshake | Where | Timeout / period | What happens when it fails |
| --- | --- | --- | --- | --- |
| A1 | ready handshake | page emits `graphium://ready` (`plan.js:2449`) → Rust `set_ready` (`app.rs:76`), answers `settings` + `state` (`app.rs:803-810`) | `READY_TIMEOUT_MS = 2000` (`app.rs:23`), waited in `wait_ready` (`app.rs:84-93`), used in the render hook (`app.rs:1089-1091`) | `RenderStatus::Error` → `rendered:false, reason:"ui_error"` |
| A2 | paint confirmation | Rust emits `graphium://render`, registers the ack (`app.rs:97-105`), page answers `render_ack` after 2 rAF (`plan.js:2039-2044`) → `resolve_ack` (`app.rs:107-112`) | `RENDER_ACK_TIMEOUT_MS = 2000` (`app.rs:17`), `recv_timeout` (`app.rs:1162-1169`) | `RenderStatus::Timeout` → `rendered:false, reason:"ui_timeout"`; the pending entry is dropped (`app.rs:1166`) |
| A3 | camera copy | page → `camera-changed` | `CAMERA_THROTTLE_MS = 100` (`plan.js:13`) | no error: Rust keeps the last copy and places against it |
| A4 | settings write | page → `settings-changed` | `SETTINGS_THROTTLE_MS = 100` (`plan.js:14`) + `settingsId` dedupe (`plan.js:744`) | no error: the file keeps its last value |
| A5 | pan animation | `panTo` (`plan.js:605-623`), `PAN_MS = 250` (`plan.js:17`) | one rAF per frame | cancelled by a camera intent (`plan.js:566`), the wheel (`plan.js:426`), a pointer down (`plan.js:485`) and a plan switch (`plan.js:1365`) |
| A6 | no display | Rust checks `DISPLAY`/`WAYLAND_DISPLAY` before painting (`app.rs:1072-1076`) | — | `RenderStatus::Headless` → `rendered:false, reason:"headless"` |

The four stable reasons of a non-painted render are `headless`, `no_window`,
`ui_timeout` and `ui_error` (`crates/gx-mcp/src/tools.rs:206-221`): the agent
branches on them, so the port cannot change which one a failure produces.

## 5. Payload shapes

### 5.1 `RenderEvent` (`app.rs:536-560`, `RenderEvent` + `FocusBox` `app.rs:562-569` + `Placement` `app.rs:571-575`)

| Field | Type | Notes |
| --- | --- | --- |
| `req_id` | string | echoed in `render_ack` |
| `diagram_id` | string | `d-…` |
| `session_id` | string | the plan the object belongs to |
| `svg` | string | the root `<svg>` with `width`/`height` |
| `placement` | `{x,y}` optional | top-left of the **object** (SVG + 24 px padding); absent when the window cannot tell its size |
| `camera` | `{tx,ty,zoom}` optional | the fit intent (PRD §7.2) |
| `focus` | `{x,y,w,h}` optional | box in plan units from `opts.focus` |

### 5.2 `StateEntry` (`app.rs:588-612`)

`diagram_id`, `title`, `kind`, `updated_at` (RFC3339 string), `session_id`,
`pos?{x,y}`, `bbox?{w,h}` (padding included), `current_version`, `oldest_version`,
`version_count`. `pos`/`bbox` are **absent** for a diagram without a place —
they are never sent as zero (`app.rs:630-662`, unit test `app.rs:1215-1237`).

### 5.3 `PlanPayload` (`app.rs:664-674`)

`diagrams: StateEntry[]`, `sessions: SessionView[]` (registry order, a
`BTreeMap`), `clones: CloneEntry[]` (arrival order).

### 5.4 `SessionView` (`crates/gx-mcp/src/session.rs:174-181`)

`id`, `family` (`pi`|`claude`|`opencode`|`ui`|`other`, `session.rs:53-61`),
`label`, `project` (optional path), `state` (`active`|`in_grace`,
`session.rs:110-115`).

### 5.5 `CloneEntry` (`app.rs:183-207`) and `ClonePaint` (`app.rs:209-227`)

`clone_id` (`c-…`), `origin_diagram_id`, `origin_session_id`, `title`,
`snapshot_version`, `current_version`, `oldest_version`, `version_count`,
`updatable` (false once the original closed), `pos{x,y}`, `bbox{w,h}`
(padding included). `ClonePaint` flattens the entry and adds `version`,
`svg?` (absent = nothing new to draw) and `bring` (whether the page travels to
the clone). A clone carries **no** `diagram_id` (`app.rs:1515-1519` asserts it).

### 5.6 `Settings` (`crates/gx-app/src/settings.rs:51-82`)

`background`, `grid_color`, `grid_opacity`, `grid_diagonal`,
`grid_angle_degrees`, `grid_visible`, `theme`, `node_fill`, `node_stroke`,
`node_text`, `edge_stroke`, `edge_text`, `accent`. The page reads and sends
exactly the 13 keys of `SETTINGS_KEYS` (`plan.js:34-48`, in its own order: the
wire is a JSON object, the order only matters to `settingsId`). Ranges Rust
enforces: diagonal `20..=200`, angle `15..=75` (`settings.rs:25`, `settings.rs:33`).

### 5.7 `ErrorPayload` (`crates/gx-mcp/src/contract.rs:19-25`)

`path`, `code`, `message`, `hint?`. The banner also reads `payload.extra`
(`plan.js:2308-2310`) to say `+N errores`: Rust never sends that key, so the
`+N` is unreachable today — the port keeps the field (harmless) and T4 records
the finding.

### 5.8 Other small payloads

`graphium://closed`: `{diagram_id}` (`app.rs:758-761`) — unlistened.
`graphium://version-paint`: `{diagram_id, version, svg}` (`app.rs:783-790`).
`CameraPayload`: `{tx,ty,zoom}` (`app.rs:614-619`). `AckPayload`:
`{req_id, ok}` (`app.rs:621-625`).

## 6. Contract numbers

Two sources, both pinned by tests: the page's constants (`plan.js:7-29`) and
Rust's (`camera.rs:11-22`). `tests/grid.rs:58-140` pins the stylesheet
declarations and the JS constants; `tests/camera_fit.rs` and `tests/culling.rs`
pin the rest.

| Number | Value | Page | Rust |
| --- | --- | --- | --- |
| grid diagonal | 59 | `plan.js:7` | `settings.rs:103` (`presets::of_theme`) |
| grid angle | 45° | `plan.js:8` | `settings.rs:33-35` |
| grid hides below | zoom 0.3 | `plan.js:9` | — |
| zoom range | 0.05 … 4.0 | `plan.js:10-11` | `FIT_MIN_ZOOM 0.4`, `FIT_MAX_ZOOM 1.0` (`camera.rs:14-16`, the **fit**, not the clamp) |
| zoom step | 1.1 | `plan.js:12` | — |
| camera throttle | 100 ms | `plan.js:13` | — |
| settings throttle | 100 ms | `plan.js:14` | — |
| pan | 250 ms, cubic ease-out | `plan.js:17`, `plan.js:605-623` | — |
| object padding | 24 | `plan.js:21` | `OBJECT_PADDING` (`camera.rs:18`) |
| culling ceiling | 3 live SVGs | `plan.js:28` | — |
| culling zoom | 0.4 | `plan.js:29` | — |
| fit margin | — | `plan.js` (none) | `FIT_MARGIN 32.0` (`camera.rs:11`) |
| cascade | 8 steps of 48 | `plan.js` (none: placement comes in the event) | `camera.rs:20-22` |
| perpendicular step | `diagonal * sin(angle)` = 41.72 px at 45°/59 | `plan.js:372-374` | `tests/grid.rs:14` |

The grid formulas the Svelte build must keep (`plan.js:362-383`): the
perpendicular step `diagonal * sin(angle) * zoom`, the tile
`(diagonal * tan(angle) * zoom) × (diagonal * zoom)`, `--grid-sin`,
`--grid-cos`, `--grid-tan` published with four decimals, and one phase per
family (`--shift-a`, `--shift-b`).

## 7. Panel / section inventory of the vanilla page

### 7.1 `plan.js` sections (in file order)

| Section | Lines | What it is | Contract items | State |
| --- | --- | --- | --- | --- |
| contract numbers | `1-31` | grid, zoom, throttles, pan, padding, culling | §6 | done (same values in `src/lib/contract.js`) |
| `SETTINGS_KEYS` / `THEMES` | `34-53` | the 13 keys and the 7 themes | §5.6 | done (`src/lib/contract.js`) |
| `PLAN_PRESETS` / `THEME_PRESETS` | `58-197` | the 8 presets and the theme→preset chips | R1, T3 | **T3 done** (`src/lib/presets.js`) |
| `fillSwatches` | `199-214` | the colour dots of the theme chips | R1, T3 | **T3 done** (`SettingsPanel.svelte`, four dots from `THEME_CHIPS`) |
| DOM handles + state | `216-290` | every element by id and the page state | — | done (ids kept, state in `App.svelte`) |
| Tauri bridge | `292-331` | `emit`/`listen` with the two doors | §1 | done (`src/lib/tauri.js`) |
| camera | `333-637` | grid paint, camera transform, wheel zoom, pan drag, clone move, camera intents, pan animation, resize | B2, A1, A3, A5 | **T2 + T3 done** (the clone move landed in T3) |
| settings panel | `639-833` | read/fill/apply the panel, throttled save, open/close, Escape | B4, R1, A4, T3 | **T3 done** (`SettingsPanel.svelte`, C6/C37/C38) |
| panel of open diagrams | `835-1006` | the list, `×`, `⧉`, selection, the camera focus | B5, B7, T3 | **T3 done** (`PlanPanel.svelte`) |
| history of versions | `1008-1150` | `◀ v3/7 ▶`, `version-view`, the preview in place | B6, R4, T3 | **T3 done** (`history.js`, `App.svelte`) |
| switcher of plans | `1152-1508` | families, `activePlan`, one camera per plan, the notice, «Mi plano» | R2, T3 | **T2 + T3 done** (`SessionsPanel.svelte`, `labels.js`) |
| plan of the human (clones) | `1510-1877` | the clone object, its `×`, `↻`, its history, drag | B8, B9, B10, R5, T3 | **T2 + T3 done** (`CloneTools.svelte`, the move in `App.svelte`) |
| diagrams | `1879-2062` | `insertDiagram`: the object, its box, `×`, placement, the camera intent, the ack | B5, B3, R3 | **T2 done** (object, box, `×`, placement, camera intent, ack) |
| culling | `2064-2301` | `visibleRect`, `applyCulling`, `readObject`, `syncObjects` | R2, T2 | **T2 done** |
| banner + ghost | `2303-2330` | `showError`, `showPending`, `hidePending` | R6, R8 | **T3 done** (`App.svelte`) |
| listeners | `2332-2449` | every `listen(...)` and the `ready` emit | §2, §3 | done (all eight, in `App.svelte`) |

### 7.2 `index.html` sections (by id)

| Element(s) | Lines | Role | State |
| --- | --- | --- | --- |
| `#plan` / `#diagrams` | `11-13` | the surface and the camera layer | done |
| `#panel-toggle` / `#panel` / `#panel-list` / `#panel-empty` | `21-35` | open diagrams panel | **T3 done** (`PlanPanel.svelte`) |
| `#sessions-toggle` / `#sessions-panel` / `#session-list` / `#session-notice` | `49-63` | switcher of plans | **T3 done** (`SessionsPanel.svelte`) |
| `#empty` | `67-74` | state 1: no agent yet | done |
| `#empty-user` | `76-82` | state 1 of the user plan | **T3 done** (C34) |
| `#banner` … `#banner-close` | `85-93` | state 3: error banner | **T3 done** (C35) |
| `#ghost` / `#ghost-text` | `96-98` | state 2: a render in flight | **T3 done** (C36) |
| `#settings-toggle` / `#settings-panel` / `#settings-close` | `105-126` | the human's settings | **T3 done** (`SettingsPanel.svelte`) |
| `#settings-*` inputs | `129-209` | 6 grid/palette pickers + the 6 colour overrides, same ids | **T3 done** (same ids, generated from the field table) |
| the theme fieldset (7 radios + swatches) | `211-248` | the one selector of the look (RF22) | **T3 done** (7 chips, 4 dots each) |

### 7.3 `style.css` sections

| Block | Lines | Role | State |
| --- | --- | --- | --- |
| `:root` tokens (grid, camera, palette, families, chrome) | `17-77` | the defaults of the `plano` preset | done (whole block, `src/app.css`) |
| theme blocks `[data-theme=…]` | `78-195` | the 7 themes of RF22 | **T3 done** (all seven, byte for byte) |
| `.graphium …` token rules | `197-341` | the classes the drawings paint with | **T3 done** (the whole block was missing while the port was partial) |
| `.plan`, `.plan.dragging`, `.plan::before` | `345-395` | surface, grid mesh, camera layer | done |
| `.diagrams`, `.diagram*`, `.content`, `.diagram-close`, `.diagram-box*` | `396-519` | the camera layer, the objects, the box and the `×` | **T2 done** |
| `.empty*` | `532-560` | the empty states | done (both states) |
| `.banner*`, `.ghost*` | `561-635` | the error banner and the render ghost | **T3 done** |
| `.panel*`, `.sessions-*`, `.session-*` | `636-1146` | the three panels and the switcher rows | **T3 done** |
| `.settings-*`, `.swatch*`, clone tools (`892-984`) | `1147-1352` | the settings dialog, the clone pill, the focus rings | **T3 done** |

## 8. Parity checklist for T4

How the checklist is enforced now: `tools/parity.mjs` runs the **real built
bundle** against a faked Tauri bridge, in headless Chromium over HTTP, and
asserts each probe's own expectation. It was differential while the A/B switch
existed — each probe ran on the vanilla page too and an `equal` probe failed
when the two differed, which is the T4 evidence of `odd/tasks/svelte-ui.md`;
with the page retired the probes assert on the one bundle and the
`equal`/`report` labels stay as checklist markers. The bridge is injected with
`Page.addScriptToEvaluateOnNewDocument`, so it exists before the bundle reads
`window.__TAURI__`; it records every emission with the
`requestAnimationFrame` counter (which is how "two frames" is observed), feeds
`graphium://` events to the listeners, and can be replaced by the
`__TAURI_INTERNALS__` door or by nothing (C1). The four `static:` probes read
files instead of a page (C9, C40, C41, C43); C42 retired with the vanilla
stylesheet (§10).

```bash
npm run build                                  # the bundle under test (`dist/`)
node tools/parity.mjs                          # the §8 probes, exit 0 = certified
```

Two items are marked `report` in the checklist: they record a documented fact
instead of an equality, and both stay visible on purpose.

1. **C2/C3** — the Svelte port pushes one `camera-changed` **before**
   `graphium://ready` (`App.svelte`, the end of `onMount`), so Rust's camera copy
   is never stale. The vanilla page does not. It is safe by B2 (Rust answers
   nothing) and harmless at any time (`app.rs:727` only stores the copy); the
   harness reports it so it stays visible.
2. **C31** — the overhang of the clone pill (see the item).

**Blind spot, stated:** the harness is Chromium, not WebKitGTK. Everything that
is DOM, CSS, wire or arithmetic is certified here; the WebKit half of the
handshake is `DISPLAY=:0 scripts/smoke.sh` (`rendered=true reason=none`, which
is only possible after A1 and A2), and the visual half is the two
byte-identical screenshots T3 took (recorded in L8). **No item is NOT COVERED.**

Tick only with the wire string, the payload keys and the numbers of this
document.

**Bridge and handshake**

- [x] C1 `src/lib/tauri.js` uses `window.__TAURI__.event` first and `plugin:event|emit`
      / `plugin:event|listen` as the fallback, with the same console error text. — **T4 ✓**
      `parity: bridge-fallback` (ready and the seven listeners travel through
      `__TAURI_INTERNALS__.invoke` with `{}`) and `parity: bridge-missing` (no
      bridge: no emission at all and `graphium: no hay puente de Tauri para
      emitir graphium://ready`, identical in both builds).
- [x] C2 The page registers every listener **before** emitting `graphium://ready`. — **T4 ✓**
      `parity: listeners before ready`: the seven functional listeners
      (`settings`, `state`, `render`, `version-paint`, `clone-paint`, `pending`,
      `error`) are all registered when ready leaves; the only pre-handshake
      emission is the T2 camera copy (see the note above).
- [x] C3 `graphium://ready` is emitted once, with `{}`, after the surface exists. — **T4 ✓**
      same probe: exactly one ready, payload `{}`, `#plan` in the DOM at emit
      time; the portable half of the gate is the smoke run (`ready` else
      `ui_error`).
- [x] C4 `render_ack ok:true` is emitted after two `requestAnimationFrame`, and
      `ok:false` only when the payload has no `<svg>`. — **T4 ✓** `parity:
      render_ack`: one ack for the render (r1, ok:true) emitted 2 frames after
      the dispatch; C21's broken payload gets ok:false and mounts nothing.
- [x] C5 `camera-changed` is throttled to 100 ms and carries `{tx,ty,zoom}`. — **T4 ✓**
      `parity: camera-changed`: a burst of three wheels produced exactly one
      event, keys `tx,ty,zoom`, value equal to the live camera.
- [x] C6 `settings-changed` is throttled to 100 ms, deduplicated by `settingsId`,
      and never sent before the first `graphium://settings`. — **T4 ✓**
      `parity: settings gate` + `parity: settings burst`: 0 events before Rust
      speaks, 2 inputs in 20 ms → 1 write with the 13 keys and the last value, a
      repeated value → 0 writes.

**Grid, camera, culling**

- [x] C7 The grid publishes `--grid-angle`, `--grid-sin`, `--grid-cos`,
      `--grid-tan`, `--grid-diagonal`, `--grid-color`, `--grid-step`,
      `--grid-opacity` with the vanilla formulas. — **T4 ✓** `parity: grid
      vars`: `45deg`, `0.7071`, `0.7071`, `1.0000`, `59px`, the settings
      colour, `41.72px`, `1` — byte-identical on both builds.
- [x] C8 The grid hides below `GRID_HIDE_ZOOM` and when `grid_visible` is false. — **T4 ✓**
      `parity: grid hides` (`--grid-opacity: 0` with `grid_visible:false`, and
      again at zoom 0.263).
- [x] C9 The stylesheet keeps `--grid-angle: 45deg`, `--grid-diagonal: 59px`,
      `--grid-step: 41.72px`, the tile `calc(diagonal * tan * zoom) ×
      calc(diagonal * zoom)` and the two per-family phases. — **T4 ✓**
      `static: grid tokens` reads the declarations, the two `calc()` tiles and
      `--shift-a`/`--shift-b` out of both stylesheets.
- [x] C10 Zoom clamps to `[0.05, 4.0]` with step 1.1 and keeps the world point
      under the cursor fixed; a wheel cancels the pan in flight. — **T4 ✓**
      `parity: zoom clamps` (4.0 and 0.05 exact after 22 and 60 wheels),
      `parity: zoom anchor` (world point moves < 0.05 px), `parity: pan` (a
      wheel during a pan stops it: the camera lands where the wheel left it).
- [x] C11 Drag pans the camera; drag on a clone of the user plan moves the clone
      (screen px ÷ zoom) and emits `graphium://clone-move` only when it moved. — **T4 ✓**
      `parity: background drag` (the hand moves `tx/ty` by the pointer delta),
      `parity: clone click` (no movement → no event), `parity: clone drag` (one
      event, `pos` delta 40/20 px ÷ zoom, and the object moved on the plan).
- [x] C12 A camera intent (`render.camera`) sets the camera and cancels the pan;
      without an intent the camera pans (`PAN_MS`, cubic) and never changes the
      zoom. — **T4 ✓** `parity: camera intent` (50/60/2 exactly, still there
      400 ms later), `parity: second object pans` (zoom untouched, not a jump,
      lands on `cameraOn(centre)` within 0.6 px).
- [x] C13 A window resize keeps the world point under the centre (no refit). — **T4 ✓**
      `parity: resize` (viewport 1280×800 → 1100×720: the centre world point
      moves < 0.5 plan units, zoom unchanged).
- [x] C14 Culling keeps at most 3 live SVGs, culls out-of-view objects and any
      zoom below 0.4, breaks ties by distance to the view centre, and every
      `applyCamera` re-decides. — **T4 ✓** `parity: culling` (5 objects → 3 live
      SVGs `a,b,c` = the closest, 2 boxes), `parity: culling zoom` (0 live at
      zoom 0.263), and the same probe after a focus intent (re-decided).
- [x] C15 A culled object becomes a box reading `<title> — <w> × <h>` in plan
      units, with the title falling back to the `diagram_id`. — **T4 ✓**
      `parity: box text`: `d-4 — 248 × 168` (no state title yet → the id is the
      caption), every box matching `/ — \d+ × \d+$/`.
- [x] C16 Each plan keeps its own camera; the first object of a plan is framed by
      the event's camera, later ones pan; `plan-hidden` hides and never closes. — **T4 ✓**
      `parity: plan-hidden` (the other plan's object carries the class and is
      still in the DOM), `parity: plan cameras` (leave plan 1 at 0/0, enter plan
      2 at 100/100, return to 0/0 exactly).

**Objects and state**

- [x] C17 `graphium://state` replaces `diagrams`/`sessions`/`clones` wholesale;
      nothing on screen exists that the event does not carry. — **T4 ✓**
      `parity: state drops`: the panel and the plan list exactly the state's
      entries after a replacement.
- [x] C18 Objects are dropped when the state no longer lists them; `selectedId`
      clears; titles and rectangles are refreshed from the state. — **T4 ✓**
      same probe: the dropped object leaves the plan, the selected row with it,
      and `renombrado` arrives in the panel.
- [x] C19 A render of a `diagram_id` already on the plan replaces the object (and
      drops its preview). — **T4 ✓** `parity: re-render`: one object for `d-1`
      after two renders, the second drawing (`data-marker="second"`) mounted.
- [x] C20 `placement` is the object corner: the SVG sits `OBJECT_PADDING` inside. — **T4 ✓**
      `parity: placement`: content at 124/124 (= placement 100 + 24), box at
      100/100 with 368×228.
- [x] C21 A render payload whose `svg` holds no `<svg>` gets `render_ack ok:false`
      and mounts nothing. — **T4 ✓** `parity: no <svg>`: ack `r2/ok:false`, the
      object count unchanged.
- [x] C22 Rust's `graphium://state` after the ack is the only source of the panel. — **T4 ✓**
      `parity: panel rows`: two rows appear only after the state, with the
      fields the state carries.

**Panels**

- [x] C23 Panel of open diagrams: title, kind, `updated_at` in `es` short format,
      `×` (emit close), `⧉` (emit clone), selection marks and pans. — **T4 ✓**
      `parity: panel rows` (title/kind/non-empty `es` date), `parity: select row`
      (`.selected`, `aria-current`, the camera on the object's centre),
      `parity: close/clone` (`{diagram_id}` payloads).
- [x] C24 «sin diagramas abiertos» while the list is empty; the toggle flips
      `aria-expanded`. — **T4 ✓** `parity: panel toggle`: starts open
      (`aria-expanded="true"`, as `index.html:21-35`), closes and reopens, and
      the empty text is the vanilla one.
- [x] C25 Version nav `◀ v3/7 ▶` only when `version_count > 1`, disabled at the
      ends, position `at - oldest_version + 1` over `version_count`. — **T4 ✓**
      `parity: version indicator`: `v7/7`, ◀ enabled, ▶ disabled, no nav at
      `version_count = 1` (the culling fixture).
- [x] C26 `graphium://version-view` for an older version, live drawing restored
      (no event) at the newest; the first version the history discarded falls
      back to the newest. — **T4 ✓** `parity: version back` (◀ → `{version:6}`,
      `v6/7`), `parity: version forward` (▶ at the newest: no event, `v7/7`, the
      live drawing back).
- [x] C27 A preview replaces the drawing in place, keeps the object rectangle and
      is what the culling re-mounts. — **T4 ✓** `parity: preview`: the preview
      marker is the only drawing in `.content`, the box rect stays 100/100/368×
      228.
- [x] C28 Switcher: «Mi plano» first, then one row per session (family dot,
      label, count, family name, project last segment or «sin proyecto»,
      `activa`/`en gracia`), the chosen plan marked. — **T4 ✓**
      `parity: switcher rows`: 3 rows, `Mi plano` first, label/family/project
      (`graphium` from `/home/…/graphium`)/`activa`/count/title, and
      `en gracia` + `family-claude` on the second session.
- [x] C29 `initialPlan` = first session with diagrams, else first session, else
      none; a session that vanished shows the notice and switches. — **T4 ✓**
      `parity: notice`: after the state drops `s1`, the plan is `sesión dos` and
      the text is exactly `la sesión «graphium-1» terminó; muestro el plano de
      «sesión dos»` (equal in both builds).
- [x] C30 `←`/`→` walk the history of the selected object (clone or diagram) and
      never steal arrows from an `input`/`textarea`/`select`. — **T4 ✓**
      `parity: arrows`: `←` on the selected clone emits
      `graphium://clone-view {clone_id, version:4}` (one step back from the
      photo, 5) and is consumed; a focused `#settings-grid-opacity` keeps its
      arrow (no event, not prevented).
- [x] C31 User plan: clone object with its `×`, `↻`, `◀ n/m ▶` pill (no `v`
      prefix), refresh disabled unless `updatable`, history only with
      `version_count > 1`. — **T4 ✓** `parity: pill` (`◀ 5/7 ▶`, refresh
      enabled with `actualizar el clon`, the `×` inside the pill),
      `parity: clone closed` (refresh disabled with `el original ya no está en
      el plan`, both steps disabled), `parity: pill remove`
      (`graphium://clone-remove {clone_id}`).
      L8 note, confirmed by the harness (`report`): the pill measures itself
      against the **object's** width (`.content.clientWidth`), so a wide clone
      keeps it at the corner and a narrow one hangs it centered; the vanilla
      measured the `.diagram` container, which has no width (all its children are
      absolute), so it always applied `.overhang`. Observed on a 368-px clone:
      Svelte `overhang:false`, vanilla `overhang:true`. The port follows the
      written rule; this is the one observed visual difference of the pill.
- [x] C32 `graphium://clone-view` without version for the refresh, with version
      for a step; a version equal to `snapshot_version` sends the refresh form. — **T4 ✓**
      `parity: refresh` (the `↻` payload is `{clone_id}` with no `version` key)
      and `parity: arrows` (the step carries `version`).
- [x] C33 A clone paint with a newer version ends the browsing; an older one is a
      preview; `bring` brings the user plan on screen and pans (the first object
      just takes the plan's camera). — **T4 ✓** `parity: clone bring` (user
      plan on screen, session object hidden, the clone drawing mounted),
      `parity: photo browsing` (version 3 → `3/7`, snapshot 5 → `5/7` and the
      photo mounted).
- [x] C34 Empty states: `#empty` only when the plan on screen has no objects and
      is not the user plan; `#empty-user` on the user plan. — **T4 ✓**
      `parity: empty states`: on a session plan `#empty` shows and `#empty-user`
      hides; on `Mi plano` the opposite, with the vanilla texts
      (`esperando al agente…`, `tu plano está vacío`).
- [x] C35 Banner shows `path`, `message`, `hint` and `+N errores` when `extra > 0`;
      the close button hides it; `graphium://error` also hides the ghost. — **T4 ✓**
      `parity: banner`: the four fields with `+2 errores`, the close button, and
      the ghost hidden by the same error.
- [x] C36 Ghost: «calculando layout…» then `N nodos, M edges` after 2000 ms;
      hidden by a render or an error. (`graphium://pending` has no emitter: keep
      the handler, do not invent an event.) — **T4 ✓** `parity: ghost`: the
      first text, the counts after 2.2 s, hidden by a render and by an error.
      The event still has no emitter in this tree (§5.7/R8).
- [x] C37 Settings panel: 6 pickers + 7 theme radios with swatches from
      `PLAN_PRESETS`/`THEME_PRESETS`, values next to the sliders, Escape closes,
      focus goes back to the toggle. — **T4 ✓** `parity: settings panel`: the
      12 control ids, 7 radios, 28 swatches, `1.00`/`59px`/`45°` next to their
      sliders; `parity: Escape` closes the dialog and returns the focus to
      `#settings-toggle`.
- [x] C38 No control may emit `settings-changed` before the first
      `graphium://settings` arrives, and every input change repaints the grid
      and the palette immediately. — **T4 ✓** `parity: settings gate` (0 writes
      before the first settings) and `parity: settings repaint` (`data-theme`,
      `--bg`, `--accent`, `--grid-color` follow the new value at once).
- [x] C39 `data-theme` on the root element follows the settings theme (unknown →
      `dark`), and the palette variables are painted over it. — **T4 ✓**
      `parity: settings repaint`: `data-theme="light"` and `--bg`/`--accent`
      applied; an unknown theme reads as `dark`.

**Numbers and text**

- [x] C40 Every number of §6 appears once, with the vanilla value, discoverable
      from the Svelte source. — **T4 ✓** `static: contract numbers`: the ten
      values (59, 45°, 0.3, 0.05/4.0, 1.1, 100/100 ms, 250 ms, 24, 3/0.4) are
      in `src/lib/contract.js`; the perpendicular step 41.72 follows from the
      formulas and C9 checks it in the stylesheet.
- [x] C41 Every user-visible string is the vanilla one, verbatim (Spanish), e.g.
      `esperando al agente…`, `sin diagramas abiertos`, `en gracia`,
      `actualizar el clon`, `el original ya no está en el plan`,
      `la sesión «…» terminó; muestro el plano de «…»`. — **T4 ✓**
      `static: strings` (7 verbatim literals in the Svelte sources) plus the
      runtime probes that compare the **rendered** texts with the vanilla page
      (`parity: switcher rows`, `notice`, `empty states`, `banner`, `ghost`,
      `panel toggle`), all equal.
- [x] C42 The parallel build carries the whole of `style.css` (all three panels,
      the objects, the boxes, the banner, the ghost) with the same class names
      as today. — **T4 ✓** `static: stylesheet`: every rule and declaration of
      `style.css` is in `src/app.css` (comments stripped, the `#app { height:
      100% }` mount rule removed: identical token stream, same rule count). T3's
      «byte for byte» was slightly optimistic — the header comment differs, the
      rules do not; corrected here.
- [x] C43 No inline `style="…"` attribute in markup: CSS variables go through
      CSSOM (`setProperty`), which the CSP (`tauri.conf.json:12`,
      `default-src 'self'`) does not police. — **T4 ✓** `static: inline styles`:
      no `style=` attribute and no `<style>` block in any `.svelte` file or in
      either served HTML; the grid/palette variables are observed in the page as
      `style.setProperty` values (C7/C39).

## 9. What the tasks delivered

`crates/gx-app/frontend/`:

| File | Content |
| --- | --- |
| `CONTRACT.md` | this document |
| `README.md` | how to build, how the switch works, how the evidence was taken |
| `package.json`, `package-lock.json`, `vite.config.js`, `index.html` | the Vite app |
| `src/main.js` | `mount(App)`, imports `app.css` |
| `src/App.svelte` | the surface, the camera, the objects, the clones, the culling, the state, the wire, the banner and the ghost |
| `src/app.css` | the whole vanilla `style.css`, byte for byte, plus `#app { height: 100% }` |
| `src/lib/contract.js` | event names, contract numbers, `SETTINGS_KEYS`, `THEMES`, `USER_PLAN` |
| `src/lib/tauri.js` | the bridge with the vanilla fallback |
| `src/lib/grid.js` | `GRID_FALLBACK`, `settingsFromPayload`, `settingsId`, `gridPaint`, `palettePaint` |
| `src/lib/plane.js` | the pure camera and culling math |
| `src/lib/presets.js` | `PLAN_PRESETS`, `THEME_PRESETS`, `THEME_CHIPS` (the swatches of the chips) |
| `src/lib/labels.js` | the built strings and the `es` date, verbatim from the vanilla page |
| `src/lib/history.js` | where the indicator of a diagram/clone sits and what a step asks for |
| `src/lib/PlanPanel.svelte` | the panel of open diagrams (list, `×`, `⧉`, history) |
| `src/lib/SessionsPanel.svelte` | the switcher of plans («Mi plano» + one row per session + notice) |
| `src/lib/SettingsPanel.svelte` | the settings dialog: the 12 controls, the 7 theme chips, Escape/focus |
| `src/lib/CloneTools.svelte` | the pill of a clone: history, `↻` and the `×` that removes the view |
| `.gitignore` | `dist/`, `node_modules/` |

**What T1 delivered:** the bridge, the `graphium://ready` handshake, the paint
confirmation and the isometric grid, with the contract numbers in
`src/lib/contract.js`. The camera itself was the identity and did not move yet.

**What T2 delivered (2026-11-07):** the plane, in `src/App.svelte` +
`src/lib/plane.js` (pure camera/culling math) + the object blocks of
`src/app.css`. The camera: wheel anchored at the cursor (step 1.1, clamp
0.05-4.0), drag pan with pointer capture, the 250 ms cubic pan, camera
intents, the 100 ms trailing throttle, resize keeping the centre point. The
objects: real `.diagram` objects with box, `×` and placement (the
`.diagram-t1` placeholder is gone), replaced on re-render, reconciled
wholesale from `graphium://state`. The culling: ≤ 3 live SVGs, zoom 0.4,
distance tie-break, per-plan visibility (`plan-hidden`). The clones: objects
of the user plan born culled, painted by `graphium://clone-paint`, `bring`
brings the plan on screen. The camera layer carries one transform instead of
one per object (same result, one write). Evidence: `npx vite build` green
(bundle 34.6 → 49.3 kB), `cargo test -p gx-app` green (22 suites, 0 failed),
the `TAURI_CONFIG` build embeds the Svelte chunks and no `plan.js`, `git
status` shows changes only under `frontend-svelte/`, and `DISPLAY=:0
scripts/svelte-smoke.sh` → `rendered=true reason=none`.

**What T3 delivered (2026-11-07):** every remaining piece of chrome, in the
components of the table above and the pure helpers `presets.js`, `labels.js`
and `history.js`. The settings panel (the 12 controls with their values, the 7
theme chips with the four dots of their preset, `data-theme` + the palette
repainted at once, the throttled and deduplicated `settings-changed` gated on
the first `graphium://settings`, Escape and the focus back to the gear); the
panel of open diagrams (title, kind, `es` date, `×`, `⧉`, selection marks, the
`◀ v3/7 ▶` history); the switcher of plans («Mi plano» first with its clone
count, one row per session, the notice, the chosen plan marked); the tools pill
of a clone (the `◀ n/m ▶` without the `v`, «Actualizar el clon», the `×` inside
the pill, one compact row with the overhang when the object is narrower); the
clone move by drag and the `graphium://clone-move` that only fires when it moved;
the `←`/`→` of the selected object; the two empty states; the error banner and
the render ghost; and the whole vanilla stylesheet — the seven theme blocks and
the `.graphium` drawing classes included. The scaffold diagnostic chip is gone
(§9 of this document said it would go when the panels landed).

How T3 was verified (the record is in `odd/tasks/svelte-ui.md`, L8):
`npx vite build` exit 0; `cargo test -p gx-app` exit 0 (22 suites, 0 failed,
vanilla path untouched: `git diff --stat main -- crates/gx-app/frontend/`
empty); `TAURI_CONFIG='{"build":{"frontendDist":"frontend-svelte/dist"}}'
cargo build -p gx-app` exit 0 and the binary embeds the Svelte chunks and zero
`plan.js`; `DISPLAY=:0 scripts/svelte-smoke.sh` → `rendered=true reason=none`;
plus a browser harness that runs the real bundle with a faked Tauri bridge and
checks 92 DOM-observable rules of §8 (C6, C23-C38, C41, L8), and two
pixel-for-pixel comparisons against the vanilla page: the default state and a
rich state (light theme, two diagrams, one clone, the settings panel open, a
version preview) render **byte-identical screenshots**. There is no JS test
runner in this package and adding one would mutate dependencies, so those
checks are evidence of this task, not committed tests.

**What T4 delivered (2026-11-07):** the certification of §8, item by item, with
the harness that produced it (`tools/parity.mjs`, committed with this document):
58 probes against the real bundle of each frontend — the Svelte one and the
vanilla page, same browser, same fake bridge — of which the `equal` ones must
observe the same value on both sides. Every item of §8 carries its evidence
line. The two differences the harness reports instead of requiring equality are
the T2 pre-ready `camera-changed` copy (C2) and the overhang of the clone pill
(C31); the blind spot is stated in §8: Chromium is not WebKitGTK, whose half of
the handshake is `scripts/svelte-smoke.sh` and whose visuals are T3's two
byte-identical screenshots.

The five findings T4 confirms and hands over: (1) the pill of a clone measures
itself against the object's width, so a wide clone keeps it at the corner while
the vanilla page always applied `.overhang` (C31); (2) `+N errores` in the
banner is unreachable because Rust never sends `extra` (§5.7); (3) `graphium://
pending` has no emitter, so the ghost is unreachable from Rust today (C36);
(4) the Svelte page emits one `camera-changed` before the handshake, deliberately
(C2); (5) T3's «byte for byte» for `src/app.css` meant *the rules*, not the prose
— the header comment differs (C42). The scaffold diagnostic chip and its
`.scaffold` CSS are gone with T3.

## 10. The A/B switch, RETIRED — and where the retired guards live

This section is a historical note, not a step to run. Until
`odd/tasks/frontend-consolidacion.md` the tree carried two frontends: the
vanilla page (`crates/gx-app/frontend/`, removed) and this Svelte app
(`crates/gx-app/frontend-svelte/`, now `crates/gx-app/frontend/`). The
vanilla page was the default `frontendDist`; the Svelte bundle was selected
with a `TAURI_CONFIG` JSON merge patch
(`{"build":{"frontendDist":"frontend-svelte/dist"}}`) that both
`tauri-build` and the codegen of `tauri::generate_context!()` applied.

S2 retired the switch. `tauri.conf.json` now says
`frontendDist: "frontend/dist"`, no environment variable selects a
frontend, and one path builds both halves:

```bash
scripts/build.sh              # npm ci (when needed) + npm run build + cargo build -p gx-app
scripts/build.sh --run        # ... and open the window
DISPLAY=:0 scripts/smoke.sh  # rendered=true reason=none in the real webview
```

Why there is no `build.beforeBuildCommand` in `tauri.conf.json`: this
repository builds with cargo, never with the `tauri` CLI, and the Tauri CLI
hooks do not run for `cargo build`. The order (Vite first, cargo second) is
wired where it does run: `scripts/build.sh`, `.github/workflows/ci.yml`,
`.github/workflows/release.yml` and `install.sh`.

### Coverage map of the retired tests

The six page suites (`tests/culling.rs`, `tests/panel.rs`,
`tests/sessions_panel.rs`, `tests/settings_panel.rs`,
`tests/user_plan_page.rs`, `tests/versions.rs`) asserted the vanilla page's
implementation structure — function bodies, DOM building, element ids — and
stopped existing with the page. What each one protected, and where that
enforcement lives now:

| What the retired suite protected | Where it lives now |
| --- | --- |
| grid geometry 59 / 45° / 41.72, the `--grid-*` tokens, the tile and the `.graphium` class rules | `tests/grid.rs`, unchanged assertions over `src/app.css` + `src/lib/contract.js`/`grid.js` |
| every contract number (zoom 0.05-4.0, padding 24, ceiling 3, cull 0.4, the throttles, the pan, the hidden grid), the keys of `Settings`, the themes, the event names and the presets table | `tests/contract_numbers.rs` over `src/lib/contract.js` and `src/lib/presets.js`; the seven themes against `Theme::ALL` |
| the panel of open diagrams with its `×`, the `◀ v3/7 ▶` history, the plan switcher, the settings panel and its ranges, the clone tools, the version preview, both empty states, the banner and the ghost — the observable rules of §8 | `tools/parity.mjs`: the §8 probes on the real built bundle, run by CI |
| the wire (B1-B10, R1-R8), the acks and their timeouts, the camera copy, the settings file, the store, the plan of the human | the Rust suites: `tests/end_to_end.rs`, `tests/user_plan.rs`, `tests/settings.rs`, `tests/round_trip.rs`, `tests/placement.rs`, `tests/camera_fit.rs` |
| the resize keeping the world point under the centre | `tests/camera_fit.rs`, now against `src/App.svelte`'s `onResize` |
| the real webview half of the handshake (WebKitGTK, not Chromium) | `scripts/smoke.sh`, `rendered=true reason=none` |
| C42 (`app.css` carried every rule of the vanilla `style.css`) and the vanilla side of C41 | retired with the page: there is no second stylesheet or source to diff. The surviving halves are C9 over `src/app.css` and C41 over the source (parity harness); the byte-level evidence stays in `odd/tasks/svelte-ui.md` (T4) and `odd/tasks/svelte-ui-t5-bench.md` |

The A/B evidence itself is kept verbatim in `odd/tasks/svelte-ui.md` and
`odd/tasks/svelte-ui-t5-bench.md`.
