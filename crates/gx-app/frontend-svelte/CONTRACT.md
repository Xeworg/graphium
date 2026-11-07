# CONTRACT — the frontend ↔ Rust contract of the Graphium plan

The parity checklist of the Svelte port (`odd/tasks/svelte-ui.md`, T1) and the
map T4 walks item by item.

Every line reference points at this worktree (`feature/svelte-ui`, from
`main` @ 77bda03):

| File | What it is |
| --- | --- |
| `crates/gx-app/frontend/plan.js` | the vanilla page logic (2449 lines) |
| `crates/gx-app/frontend/index.html` | the vanilla DOM (258 lines) |
| `crates/gx-app/frontend/style.css` | the vanilla stylesheet (1352 lines) |
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

Tick only with the wire string, the payload keys and the numbers of this
document. `T1` marks what the scaffold already satisfies (verified by
`scripts/svelte-smoke.sh`, §10).

**Bridge and handshake**

- [ ] C1 `src/lib/tauri.js` uses `window.__TAURI__.event` first and `plugin:event|emit`
      / `plugin:event|listen` as the fallback, with the same console error text. — **T1 done**
- [ ] C2 The page registers every listener **before** emitting `graphium://ready`. — **T1 done**
- [ ] C3 `graphium://ready` is emitted once, with `{}`, after the surface exists. — **T1 done**
- [ ] C4 `render_ack ok:true` is emitted after two `requestAnimationFrame`, and
      `ok:false` only when the payload has no `<svg>`. — **T1 done** (placeholder mount)
- [ ] C5 `camera-changed` is throttled to 100 ms and carries `{tx,ty,zoom}`. — **T2 done**
- [ ] C6 `settings-changed` is throttled to 100 ms, deduplicated by `settingsId`,
      and never sent before the first `graphium://settings`. — **T3 done**
      (`notifySettings`/`onSettingsChange` in `App.svelte`; observed in the
      browser harness: nothing leaves the page before the first settings, a
      burst of two inputs is one write, a repeated value is no write)

**Grid, camera, culling**

- [ ] C7 The grid publishes `--grid-angle`, `--grid-sin`, `--grid-cos`,
      `--grid-tan`, `--grid-diagonal`, `--grid-color`, `--grid-step`,
      `--grid-opacity` with the vanilla formulas. — **T1 done**
- [ ] C8 The grid hides below `GRID_HIDE_ZOOM` and when `grid_visible` is false. — **T1 done**
- [ ] C9 The stylesheet keeps `--grid-angle: 45deg`, `--grid-diagonal: 59px`,
      `--grid-step: 41.72px`, the tile `calc(diagonal * tan * zoom) ×
      calc(diagonal * zoom)` and the two per-family phases. — **T1 done**
- [ ] C10 Zoom clamps to `[0.05, 4.0]` with step 1.1 and keeps the world point
      under the cursor fixed; a wheel cancels the pan in flight. — **T2 done**
- [ ] C11 Drag pans the camera; drag on a clone of the user plan moves the clone
      (screen px ÷ zoom) and emits `graphium://clone-move` only when it moved. — **T2 + T3 done** (`cloneUnder`/`moveCloneStart`/`endDrag` in `App.svelte`:
      the move is `screen px ÷ zoom`, a click writes nothing)
- [ ] C12 A camera intent (`render.camera`) sets the camera and cancels the pan;
      without an intent the camera pans (`PAN_MS`, cubic) and never changes the zoom. — **T2 done**
- [ ] C13 A window resize keeps the world point under the centre (no refit). — **T2 done**
- [ ] C14 Culling keeps at most 3 live SVGs, culls out-of-view objects and any
      zoom below 0.4, breaks ties by distance to the view centre, and every
      `applyCamera` re-decides. — **T2 done**
- [ ] C15 A culled object becomes a box reading `<title> — <w> × <h>` in plan
      units, with the title falling back to the `diagram_id`. — **T2 done**
- [ ] C16 Each plan keeps its own camera; the first object of a plan is framed by
      the event's camera, later ones pan; `plan-hidden` hides and never closes. — **T2 + T3 done** (cameras and `plan-hidden` in T2; the switch is the
      switcher rows of T3)

**Objects and state**

- [ ] C17 `graphium://state` replaces `diagrams`/`sessions`/`clones` wholesale;
      nothing on screen exists that the event does not carry. — **T2 done**
- [ ] C18 Objects are dropped when the state no longer lists them; `selectedId`
      clears; titles and rectangles are refreshed from the state. — **T2 done**
- [ ] C19 A render of a `diagram_id` already on the plan replaces the object (and
      drops its preview). — **T2 done**
- [ ] C20 `placement` is the object corner: the SVG sits `OBJECT_PADDING` inside. — **T2 done** (real object)
- [ ] C21 A render payload whose `svg` holds no `<svg>` gets `render_ack ok:false`
      and mounts nothing. — **T1 done**
- [ ] C22 Rust's `graphium://state` after the ack is the only source of the panel. — **T3 done** (`PlanPanel` reads the `diagrams` prop, which is exactly
      `state.diagrams`)

**Panels**

- [ ] C23 Panel of open diagrams: title, kind, `updated_at` in `es` short format,
      `×` (emit close), `⧉` (emit clone), selection marks and pans. — **T3 done**
      (`PlanPanel.svelte`, `formatUpdated`/`labels.js`, `selectDiagram`)
- [ ] C24 «sin diagramas abiertos» while the list is empty; the toggle flips
      `aria-expanded`. — **T3 done** (starts open, like `index.html:21-35`)
- [ ] C25 Version nav `◀ v3/7 ▶` only when `version_count > 1`, disabled at the
      ends, position `at - oldest_version + 1` over `version_count`. — **T3 done**
      (`history.js`; observed: `v7/7` at the live drawing, `◀` → `v6/7` and a
      `graphium://version-view {diagram_id, version: 10}`)
- [ ] C26 `graphium://version-view` for an older version, live drawing restored
      (no event) at the newest; the first version the history discarded falls
      back to the newest. — **T3 done** (observed: `▶` back to the newest emits
      nothing and returns to `v7/7`; an out-of-range view reads as the live one)
- [ ] C27 A preview replaces the drawing in place, keeps the object rectangle and
      is what the culling re-mounts. — **T3 done** (`paintPreview`/`restoreLive`;
      the `svgMount` action mounts `previewSvg` before `svg`)
- [ ] C28 Switcher: «Mi plano» first, then one row per session (family dot,
      label, count, family name, project last segment or «sin proyecto»,
      `activa`/`en gracia`), the chosen plan marked. — **T3 done**
      (`SessionsPanel.svelte`, `labels.js`)
- [ ] C29 `initialPlan` = first session with diagrams, else first session, else
      none; a session that vanished shows the notice and switches. — **T3 done**
      (observed notice: `la sesión «graphium-1» terminó; muestro el plano de «sesión dos»`)
- [ ] C30 `←`/`→` walk the history of the selected object (clone or diagram) and
      never steal arrows from an `input`/`textarea`/`select`. — **T3 done**
      (`onKeyDown` in `App.svelte`; a clone walks the history of its original)
- [ ] C31 User plan: clone object with its `×`, `↻`, `◀ n/m ▶` pill (no `v`
      prefix), refresh disabled unless `updatable`, history only with
      `version_count > 1`. — **T3 done** (`CloneTools.svelte`; observed `2/7`,
      the refresh disabled and its title `el original ya no está en el plan` once
      `updatable` is false, and the `×` emitting `graphium://clone-remove`).
      L8 note: the pill measures itself against the **object's** width
      (`.content.clientWidth`), so a wide clone keeps it at the corner and a
      narrow one hangs it centered; the vanilla measured the `.diagram`
      container, which has no width (all its children are absolute), so it always
      applied `.overhang`. The port follows the written rule (L8/C31: «overhang
      centered when the object is narrower»); T4 should know this is the one
      observed visual difference of the pill on wide objects.
- [ ] C32 `graphium://clone-view` without version for the refresh, with version
      for a step; a version equal to `snapshot_version` sends the refresh form. — **T3 done** (`refreshClone` emits `{clone_id}`; `viewCloneVersion` emits
      `{clone_id, version}` and, at the photo, resets the view — `refreshClone`'s
      form, as `plan.js:1830-1877` writes it)
- [ ] C33 A clone paint with a newer version ends the browsing; an older one is a
      preview; `bring` brings the user plan on screen and pans (the first object
      just takes the plan's camera). — **T3 done** (`applyClonePaint`)
- [ ] C34 Empty states: `#empty` only when the plan on screen has no objects and
      is not the user plan; `#empty-user` on the user plan. — **T3 done**
      (`showEmpty`/`showEmptyUser`, both derived from the plan on screen)
- [ ] C35 Banner shows `path`, `message`, `hint` and `+N errores` when `extra > 0`;
      the close button hides it; `graphium://error` also hides the ghost. — **T3 done**
- [ ] C36 Ghost: «calculando layout…» then `N nodos, M edges` after 2000 ms;
      hidden by a render or an error. (`graphium://pending` has no emitter: keep
      the handler, do not invent an event.) — **T3 done** (observed: the counts
      after the 2 s timer, hidden by `graphium://error` and by a render)
- [ ] C37 Settings panel: 6 pickers + 7 theme radios with swatches from
      `PLAN_PRESETS`/`THEME_PRESETS`, values next to the sliders, Escape closes,
      focus goes back to the toggle. — **T3 done** (`SettingsPanel.svelte`: the
      six grid/palette pickers, the six colour overrides of the same panel, and
      the seven chips with four dots each from `THEME_CHIPS`; `0.10`, `100px`
      and `45°` beside the sliders; Escape closes and the focus returns)
- [ ] C38 No control may emit `settings-changed` before the first
      `graphium://settings` arrives, and every input change repaints the grid
      and the palette immediately. — **T3 done** (observed: `data-theme` and
      `--bg` follow the new settings, `#plan`'s `--grid-color` repaints at once)
- [ ] C39 `data-theme` on the root element follows the settings theme (unknown →
      `dark`), and the palette variables are painted over it. — **T1 done** (root + palette)

**Numbers and text**

- [ ] C40 Every number of §6 appears once, with the vanilla value, discoverable
      from the Svelte source. — **T1 done** (`src/lib/contract.js`)
- [ ] C41 Every user-visible string is the vanilla one, verbatim (Spanish), e.g.
      `esperando al agente…`, `sin diagramas abiertos`, `en gracia`,
      `actualizar el clon`, `el original ya no está en el plan`,
      `la sesión «…» terminó; muestro el plano de «…»`. — **T3 done** (the
      built strings live in `src/lib/labels.js`; the markup keeps its own;
      observed character by character in the browser harness)

**Stylesheet**

- [ ] C42 The parallel build carries the whole of `style.css` (all three panels,
      the objects, the boxes, the banner, the ghost) with the same class names
      as today. — **T2 + T3 done** (`src/app.css` is the vanilla stylesheet byte
      for byte, plus the `#app { height: 100% }` rule the Svelte mount needs;
      the palette, the seven theme blocks and the `.graphium` drawing classes
      arrived with T3)
- [ ] C43 No inline `style="…"` attribute in markup: CSS variables go through
      CSSOM (`setProperty`), which the CSP (`tauri.conf.json:12`,
      `default-src 'self'`) does not police. — **T1 done**

## 9. What the tasks delivered, and what T4 inherits

`crates/gx-app/frontend-svelte/`:

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

**T4 inherits:** the checklist of §8 (every item is now marked), the smoke test
of §10 and the two documented findings: (1) the pill of a clone measures itself
against the object's width, so a wide clone keeps it at the corner while the
vanilla page always applied `.overhang` (see the C31 note); (2) `+N errores` in
the banner is unreachable because Rust never sends `extra` (§5.7). The scaffold
diagnostic chip and its `.scaffold` CSS are gone with T3.

## 10. Selecting the Svelte build, and the evidence T1 took

The default path is untouched: `crates/gx-app/tauri.conf.json` still says
`"frontendDist": "frontend"`, the vanilla files are byte-identical, and the
Rust tests that `include_str!` them keep passing.

The switch is the `TAURI_CONFIG` environment variable, which both
`tauri-build` (`tauri-build-2.7.4/src/lib.rs:655,681`) and the codegen of
`tauri::generate_context!()` (`tauri-codegen-2.7.4/src/lib.rs:56-91`) merge
into the parsed config with a JSON merge patch; `tauri-build` also emits
`cargo:rerun-if-env-changed=TAURI_CONFIG`, so changing it recompiles `gx-app`
and nothing else. Relative paths of the config are resolved against the
directory of `tauri.conf.json` (`crates/gx-app`), so:

```bash
# 1. the Svelte bundle
scripts/svelte-build.sh            # npm + vite build → crates/gx-app/frontend-svelte/dist

# 2. the binary that embeds it
TAURI_CONFIG='{"build":{"frontendDist":"frontend-svelte/dist"}}' cargo build -p gx-app

# the helper that does both (and runs it with --run)
scripts/svelte-tauri.sh
```

The vanilla build is the same command without the variable
(`scripts/svelte-tauri.sh --vanilla`).

Evidence (T1, observed on this worktree):

1. `cargo test -p gx-app` → green with the default (vanilla) config.
2. `scripts/svelte-build.sh` → `dist/` with `index.html`,
   `assets/index-*.js` (34.6 kB), `assets/index-*.css` (2.1 kB).
3. `TAURI_CONFIG='{"build":{"frontendDist":"frontend-svelte/dist"}}' cargo build -p gx-app`
   → the binary embeds the Svelte chunks; `strings target/debug/graphium` shows
   `assets/index-<hash>.js` and no `plan.js`.
4. `DISPLAY=:0 scripts/svelte-smoke.sh` → `rendered=true reason=none` for both
   the vanilla and the Svelte binary. `rendered:true` is only possible after
   `graphium://ready` (else `ui_error`) **and** `render_ack` (else
   `ui_timeout`), so it is the observable form of A1 + A2 with the switch
   active. The script isolates the socket in its own `XDG_RUNTIME_DIR` and the
   settings in its own `HOME`, both under `/tmp`, and quits the owner it started.
