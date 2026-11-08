// The contract with Rust, in one place: event names, contract numbers and the
// keys of `Settings`. Every value here is a copy of the vanilla page
// (`crates/gx-app/frontend/plan.js`) and of the Rust side
// (`crates/gx-app/src/app.rs`); CONTRACT.md maps each one to its line.
//
// Nothing in this file decides anything: it only makes the numbers the
// vanilla page keeps in module scope discoverable to the Svelte port, which
// is what S2 asks for ("contract numbers identical … discoverable").

// --- Grid and camera (plan.js:7-21) -------------------------------------

/** The diagonal of the rhombus (plan.js:7, `tests/grid.rs`). */
export const GRID_DIAGONAL_PLAN = 59;
/** The deviation of the lines from the vertical (plan.js:8, `tests/grid.rs`). */
export const GRID_ANGLE_DEGREES = 45;
/** Below this zoom the grid fades out (plan.js:9). */
export const GRID_HIDE_ZOOM = 0.3;
export const ZOOM_MIN = 0.05;
export const ZOOM_MAX = 4.0;
export const ZOOM_STEP = 1.1;
/** The throttle of `camera-changed` (plan.js:13, Rust `app.rs:727`). */
export const CAMERA_THROTTLE_MS = 100;
/** The throttle of `settings-changed` (plan.js:14). */
export const SETTINGS_THROTTLE_MS = 100;
/** The animated pan of PRD §7.2 (plan.js:17). */
export const PAN_MS = 250;
/** The padding of the object of a diagram (plan.js:21, `camera.rs:18`). */
export const OBJECT_PADDING = 24;

// --- Culling (plan.js:28-29, PRD RNF1-ter) ------------------------------

export const MAX_LIVE_SVGS = 3;
export const CULL_ZOOM = 0.4;

// --- Settings (plan.js:34-53, `crates/gx-app/src/settings.rs`) -----------

/** The keys of `Settings` in Rust, in the order the page sends them back. */
export const SETTINGS_KEYS = [
  "accent",
  "background",
  "edge_stroke",
  "edge_text",
  "grid_angle_degrees",
  "grid_color",
  "grid_diagonal",
  "grid_opacity",
  "grid_visible",
  "node_fill",
  "node_stroke",
  "node_text",
  "theme",
];

/** The themes the panel lists (`Theme` in `gx-core/src/render.rs`). */
export const THEMES = ["dark", "light", "brasa", "arcilla", "aurora", "neblina", "pitaya"];

// --- Plans (plan.js:1185-1188, `f8-planos.md` B1) -----------------------

/** The sentinel id of the plan of the human: never a session id. */
export const USER_PLAN_ID = "user";
/** The plan of the human, as the switcher names it (its row is T3). */
export const USER_PLAN = { id: USER_PLAN_ID, label: "Mi plano" };

// --- Events -------------------------------------------------------------

// The names are the wire contract: a typo is a silent timeout in Rust
// (`READY_TIMEOUT_MS`, `RENDER_ACK_TIMEOUT_MS`) or a page that never paints.
export const EVENTS = {
  // Rust → frontend (listened by the page).
  settings: "graphium://settings",
  state: "graphium://state",
  render: "graphium://render",
  versionPaint: "graphium://version-paint",
  clonePaint: "graphium://clone-paint",
  error: "graphium://error",
  pending: "graphium://pending",
  closed: "graphium://closed",

  // frontend → Rust (listened by `app.rs`).
  ready: "graphium://ready",
  close: "graphium://close",
  versionView: "graphium://version-view",
  clone: "graphium://clone",
  cloneView: "graphium://clone-view",
  cloneMove: "graphium://clone-move",
  cloneRemove: "graphium://clone-remove",

  // The two the vanilla page sends without the `graphium://` prefix: Rust
  // listens to these exact names (`app.rs:727`, `app.rs:732`, `app.rs:921`).
  cameraChanged: "camera-changed",
  renderAck: "render_ack",
  settingsChanged: "settings-changed",
};
