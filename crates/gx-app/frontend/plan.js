// Graphium plan: the camera lives here (PRD §7.2).
//
// Rust never writes the camera: it emits intents (place, focus, fit) and the
// frontend applies them. Rust keeps only a copy (CameraSnapshot) that the
// frontend pushes with the `camera-changed` event.

const GRID_DIAGONAL_PLAN = 59; // The diagonal of the rhombus.
const GRID_ANGLE_DEGREES = 45; // The deviation of the lines from the vertical.
const GRID_HIDE_ZOOM = 0.3; // Below this zoom the grid fades out.
const ZOOM_MIN = 0.05;
const ZOOM_MAX = 4.0;
const ZOOM_STEP = 1.1;
const CAMERA_THROTTLE_MS = 100;
const SETTINGS_THROTTLE_MS = 100;
// How long the camera travels until a diagram (PRD §7.2: an animated pan of
// 250 ms, never a jump).
const PAN_MS = 250;
// The padding of the object of a diagram around its SVG (PRD §7.2). Rust
// already applied it to `placement` and to the `bbox` of the state, so the
// page only uses it to put the SVG one padding inside the object.
const OBJECT_PADDING = 24;
// Culling (PRD RNF1-ter, §7.2, §7.3). The canvas keeps at most this many
// SVGs of diagrams in the DOM; an object out of the view, or seen at a zoom
// below `CULL_ZOOM`, is a box with its title and its size instead.
// RNF1-ter fixes the ceiling at 3 and the threshold at 0.4, and §7.2 already
// asks for the boxes below 0.3: the 0.4 of RNF1-ter is the one applied, so a
// drawing is never on the page at a zoom where the PRD wants a box.
const MAX_LIVE_SVGS = 3;
const CULL_ZOOM = 0.4;

// The keys of `Settings` in Rust (`crates/gx-app/src/settings.rs`), in the
// order the page sends them back. The panel reads and writes exactly these:
// the page never invents a setting and never decides a default.
const SETTINGS_KEYS = [
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

// The themes of the plan (`Theme` in `crates/gx-core/src/render.rs`), in the
// order the panel lists them. The stylesheet answers to the same name with a
// `[data-theme="…"]` block (PRD §8.5.2 RF22, `f6-themes.md` R5).
const THEMES = ["dark", "light", "brasa", "arcilla", "aurora", "neblina", "pitaya"];

// The presets of F1 and of the four themes of F6 (PRD §8.5.2). Rust owns them
// (`settings::presets`): this table only fills the controls, and the contract
// test in `crates/gx-app/tests/settings_panel.rs` keeps both sides equal.
const PLAN_PRESETS = {
  "plano": {
    "accent": "#6ea8fe",
    "background": "#1b1b1b",
    "edge_stroke": "#8a8a96",
    "edge_text": "#b9b9c2",
    "grid_angle_degrees": 45,
    "grid_color": "#ffffff",
    "grid_diagonal": 59,
    "grid_opacity": 0.14,
    "grid_visible": true,
    "node_fill": "#2a2a30",
    "node_stroke": "#4d4d57",
    "node_text": "#e9e9ec",
    "theme": "dark"
  },
  "claro": {
    "accent": "#2f6fd0",
    "background": "#f7f7f8",
    "edge_stroke": "#6f6f7a",
    "edge_text": "#4a4a54",
    "grid_angle_degrees": 45,
    "grid_color": "#000000",
    "grid_diagonal": 59,
    "grid_opacity": 0.1,
    "grid_visible": true,
    "node_fill": "#ffffff",
    "node_stroke": "#c7c7cf",
    "node_text": "#1b1b1f",
    "theme": "light"
  },
  "sin grilla": {
    "accent": "#6ea8fe",
    "background": "#1b1b1b",
    "edge_stroke": "#8a8a96",
    "edge_text": "#b9b9c2",
    "grid_angle_degrees": 45,
    "grid_color": "#ffffff",
    "grid_diagonal": 59,
    "grid_opacity": 0.14,
    "grid_visible": false,
    "node_fill": "#2a2a30",
    "node_stroke": "#4d4d57",
    "node_text": "#e9e9ec",
    "theme": "dark"
  },
  "brasa": {
    "accent": "#f5a623",
    "background": "#131316",
    "edge_stroke": "#8f867c",
    "edge_text": "#cbbcab",
    "grid_angle_degrees": 45,
    "grid_color": "#ffffff",
    "grid_diagonal": 59,
    "grid_opacity": 0.16,
    "grid_visible": true,
    "node_fill": "#1e1e24",
    "node_stroke": "#3d3d47",
    "node_text": "#f2e9e1",
    "theme": "brasa"
  },
  "arcilla": {
    "accent": "#c05a3a",
    "background": "#faf6f0",
    "edge_stroke": "#8a7a68",
    "edge_text": "#5c4d40",
    "grid_angle_degrees": 45,
    "grid_color": "#000000",
    "grid_diagonal": 59,
    "grid_opacity": 0.1,
    "grid_visible": true,
    "node_fill": "#fffdf9",
    "node_stroke": "#d9cbb7",
    "node_text": "#3a2f27",
    "theme": "arcilla"
  },
  "aurora": {
    "accent": "#9a8cf0",
    "background": "#0e1626",
    "edge_stroke": "#7288b0",
    "edge_text": "#a9bcd9",
    "grid_angle_degrees": 45,
    "grid_color": "#c8dcff",
    "grid_diagonal": 59,
    "grid_opacity": 0.13,
    "grid_visible": true,
    "node_fill": "#16213a",
    "node_stroke": "#2c3d5e",
    "node_text": "#e6ecf7",
    "theme": "aurora"
  },
  "neblina": {
    "accent": "#2f8f86",
    "background": "#eef1f2",
    "edge_stroke": "#6f8080",
    "edge_text": "#4c5c5d",
    "grid_angle_degrees": 45,
    "grid_color": "#000000",
    "grid_diagonal": 59,
    "grid_opacity": 0.1,
    "grid_visible": true,
    "node_fill": "#fafbfb",
    "node_stroke": "#cbd3d5",
    "node_text": "#22302f",
    "theme": "neblina"
  },
  "pitaya": {
    "accent": "#e06a8d",
    "background": "#100e13",
    "edge_stroke": "#94707f",
    "edge_text": "#c99fb0",
    "grid_angle_degrees": 45,
    "grid_color": "#ffffff",
    "grid_diagonal": 59,
    "grid_opacity": 0.13,
    "grid_visible": true,
    "node_fill": "#1b161c",
    "node_stroke": "#4d3a45",
    "node_text": "#f0dce4",
    "theme": "pitaya"
  }
};

// One name per look: the chip of a theme shows the palette of its preset.
// `dark` is the theme, `plano` is the look the panel calls it.
const THEME_PRESETS = {
  "dark": "plano",
  "light": "claro",
  "brasa": "brasa",
  "arcilla": "arcilla",
  "aurora": "aurora",
  "neblina": "neblina",
  "pitaya": "pitaya"
};

/**
 * Fills the palette of dots beside every theme name: four swatches — the
 * background, the nodes, their text and the accent — read from
 * `PLAN_PRESETS`, the same table the presets use, so the chips and the
 * presets cannot drift apart.
 */
function fillSwatches() {
  for (const label of document.querySelectorAll("label.settings-radio")) {
    const input = label.querySelector('input[name="settings-theme"]');
    const preset = input && PLAN_PRESETS[THEME_PRESETS[input.value]];
    const swatches = label.querySelector(".swatches");
    if (!preset || !swatches || swatches.childElementCount > 0) {
      continue;
    }
    for (const key of ["background", "node_fill", "node_text", "accent"]) {
      const dot = document.createElement("span");
      dot.className = "swatch";
      dot.style.background = preset[key];
      swatches.appendChild(dot);
    }
  }
}

const plan = document.getElementById("plan");
const diagrams = document.getElementById("diagrams");
const empty = document.getElementById("empty");
const banner = document.getElementById("banner");
const bannerPath = document.getElementById("banner-path");
const bannerMessage = document.getElementById("banner-message");
const bannerHint = document.getElementById("banner-hint");
const bannerExtra = document.getElementById("banner-extra");
const ghost = document.getElementById("ghost");
const ghostText = document.getElementById("ghost-text");

// The panel of the open diagrams (PRD §7 RF26, S1).
const panel = document.getElementById("panel");
const panelToggle = document.getElementById("panel-toggle");
const panelList = document.getElementById("panel-list");
const panelEmpty = document.getElementById("panel-empty");

// The panel of the sessions (S1, S3, D3, D5): its own button in the
// bottom-left corner and its own panel above it, separated from the list of
// open diagrams.
const sessionsPanel = document.getElementById("sessions-panel");
const sessionsToggle = document.getElementById("sessions-toggle");

// The selector of sessions (S1, S3, D3, D5): the rows of the panel of
// sessions and the notice that says when the watched session ended.
const sessionList = document.getElementById("session-list");
const sessionNotice = document.getElementById("session-notice");

// The settings panel (PRD §7.1 RF25, S19).
const settingsToggle = document.getElementById("settings-toggle");
const settingsPanel = document.getElementById("settings-panel");
const settingsClose = document.getElementById("settings-close");
const backgroundInput = document.getElementById("settings-background");
const gridColorInput = document.getElementById("settings-grid-color");
const gridOpacityInput = document.getElementById("settings-grid-opacity");
const gridDiagonalInput = document.getElementById("settings-grid-diagonal");
const gridAngleInput = document.getElementById("settings-grid-angle");
const gridVisibleInput = document.getElementById("settings-grid-visible");
// The colours of the drawing (`f6-themes.md`, R2): the theme is the base and
// each picker is the override of the human.
const nodeFillInput = document.getElementById("settings-node-fill");
const nodeStrokeInput = document.getElementById("settings-node-stroke");
const nodeTextInput = document.getElementById("settings-node-text");
const edgeStrokeInput = document.getElementById("settings-edge-stroke");
const edgeTextInput = document.getElementById("settings-edge-text");
const accentInput = document.getElementById("settings-accent");
const gridOpacityValue = document.getElementById("settings-grid-opacity-value");
const gridDiagonalValue = document.getElementById("settings-grid-diagonal-value");
const gridAngleValue = document.getElementById("settings-grid-angle-value");
const themeInputs = Array.from(
  document.querySelectorAll('input[name="settings-theme"]'),
);

const camera = { tx: 0, ty: 0, zoom: 1 };
const objects = new Map(); // diagram_id -> element
const state = { diagrams: [], sessions: [] }; // the state Rust owns (PRD §8.6.1)
let cameraTimer = null;
// The settings of the human: Rust sends them and the page paints them. Until
// the first `graphium://settings` arrives the plan shows the `plano` preset.
let settings = null;
let settingsTimer = null;
let settingsPending = null;
let settingsSent = null;
// The size of the plan in CSS pixels, before the current resize.
let viewport = {
  w: plan.clientWidth || window.innerWidth,
  h: plan.clientHeight || window.innerHeight,
};

// --- Tauri bridge -------------------------------------------------------

const tauri = window.__TAURI__;
const internals = window.__TAURI_INTERNALS__;

/**
 * Emits an event to Rust.
 *
 * The global bundle (`window.__TAURI__`) is the documented path. When it is
 * not there, the internal IPC is used directly: it is the same protocol the
 * bundle speaks (`plugin:event|emit`), so the plan works either way.
 */
function emit(name, payload) {
  if (tauri && tauri.event && typeof tauri.event.emit === "function") {
    tauri.event.emit(name, payload);
    return;
  }
  if (internals && typeof internals.invoke === "function") {
    internals.invoke("plugin:event|emit", { event: name, payload });
    return;
  }
  console.error(`graphium: no hay puente de Tauri para emitir ${name}`);
}

/**
 * Listens to an event coming from Rust, with the same fallback as `emit`.
 */
function listen(name, handler) {
  if (tauri && tauri.event && typeof tauri.event.listen === "function") {
    tauri.event.listen(name, handler);
    return;
  }
  if (internals && typeof internals.invoke === "function") {
    internals.invoke("plugin:event|listen", {
      event: name,
      target: { kind: "Any" },
      handler: internals.transformCallback((event) => handler(event)),
    });
  }
}

// --- Camera -------------------------------------------------------------

function clampZoom(zoom) {
  return Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, zoom));
}

// Before the first settings arrive the plan shows the `plano` preset: the
// same numbers the stylesheet declares.
const GRID_FALLBACK = {
  grid_color: "rgba(255, 255, 255, 0.14)",
  grid_opacity: 1,
  grid_diagonal: GRID_DIAGONAL_PLAN,
  grid_angle_degrees: GRID_ANGLE_DEGREES,
  grid_visible: true,
};

function gridSettings() {
  return settings === null ? GRID_FALLBACK : settings;
}

/**
 * Paints the grid from the settings of the human (PRD §7.1 RF24, RF25).
 *
 * The geometry is recomputed from the angle and the separation the human
 * chose: the perpendicular step is `separation * sin(angle) * zoom` and the
 * tile of the background is `separation * zoom` (the rule that keeps the
 * mesh a lattice at any angle and any zoom). The thickness stays one screen
 * pixel because it is a length of the gradient, not a scaled one.
 */
function applyGrid() {
  const grid = gridSettings();
  const radians = (grid.grid_angle_degrees * Math.PI) / 180;
  const sine = Math.sin(radians);

  plan.style.setProperty("--grid-angle", `${grid.grid_angle_degrees}deg`);
  plan.style.setProperty("--grid-sin", sine.toFixed(4));
  plan.style.setProperty("--grid-cos", Math.cos(radians).toFixed(4));
  // The tile of the mesh is `diagonal * tan(angle)` wide and `diagonal`
  // tall: with a square tile the lattice only closed at 45 degrees.
  plan.style.setProperty("--grid-tan", Math.tan(radians).toFixed(4));
  plan.style.setProperty("--grid-diagonal", `${grid.grid_diagonal}px`);
  plan.style.setProperty("--grid-color", grid.grid_color);
  plan.style.setProperty(
    "--grid-step",
    `${(grid.grid_diagonal * sine * camera.zoom).toFixed(2)}px`,
  );
  // A grid the human hid is painted with opacity 0, and so is a grid whose
  // lines would be noise at this zoom.
  const visible = grid.grid_visible && camera.zoom >= GRID_HIDE_ZOOM;
  plan.style.setProperty("--grid-opacity", visible ? String(grid.grid_opacity) : "0");
}

function applyCamera() {
  plan.style.setProperty("--tx", `${camera.tx}px`);
  plan.style.setProperty("--ty", `${camera.ty}px`);
  // The tile of the grid scales with the camera, so the tile keeps being one
  // cell of the mesh at any zoom.
  plan.style.setProperty("--zoom", String(camera.zoom));
  applyGrid();

  const transform = `translate(${camera.tx}px, ${camera.ty}px) scale(${camera.zoom})`;
  for (const element of objects.values()) {
    element.style.transform = transform;
  }
  // The camera is also what decides which drawings stay in the page
  // (RNF1-ter): every frame of a pan, every step of the wheel, every
  // insertion, focus and fit end up here, so this is the single place where
  // the culling is due.
  applyCulling();
}

function notifyCamera() {
  if (cameraTimer !== null) {
    return;
  }
  cameraTimer = window.setTimeout(() => {
    cameraTimer = null;
    emit("camera-changed", { ...camera });
  }, CAMERA_THROTTLE_MS);
}

function zoomAt(screenX, screenY, factor) {
  const next = clampZoom(camera.zoom * factor);
  const applied = next / camera.zoom;
  // Keep the world point under the cursor fixed.
  camera.tx = screenX - (screenX - camera.tx) * applied;
  camera.ty = screenY - (screenY - camera.ty) * applied;
  camera.zoom = next;
  applyCamera();
  notifyCamera();
}

plan.addEventListener(
  "wheel",
  (event) => {
    event.preventDefault();
    // The human taking the wheel is the new owner of the view: a pan in
    // flight stops where it is instead of fighting the zoom.
    cancelPan();
    zoomAt(event.clientX, event.clientY, event.deltaY < 0 ? ZOOM_STEP : 1 / ZOOM_STEP);
  },
  { passive: false },
);

let dragging = null;

plan.addEventListener("pointerdown", (event) => {
  // The same for the drag: the hand of the human wins over the animation.
  cancelPan();
  dragging = { x: event.clientX, y: event.clientY, tx: camera.tx, ty: camera.ty };
  plan.classList.add("dragging");
  plan.setPointerCapture(event.pointerId);
});

plan.addEventListener("pointermove", (event) => {
  if (!dragging) {
    return;
  }
  camera.tx = dragging.tx + (event.clientX - dragging.x);
  camera.ty = dragging.ty + (event.clientY - dragging.y);
  applyCamera();
  notifyCamera();
});

function endDrag(event) {
  if (!dragging) {
    return;
  }
  dragging = null;
  plan.classList.remove("dragging");
  if (plan.hasPointerCapture(event.pointerId)) {
    plan.releasePointerCapture(event.pointerId);
  }
}

plan.addEventListener("pointerup", endDrag);
plan.addEventListener("pointercancel", endDrag);

/**
 * Applies a camera intent coming from Rust (PRD §7.2): the camera lives
 * here, Rust only says where to look.
 */
function applyCameraIntent(intent) {
  if (!intent) {
    return;
  }
  // A pan in flight would keep writing the camera every frame and would
  // leave the human where the pan was going, not where the intent says: the
  // intent wins (PRD §7.2, RF27).
  cancelPan();
  camera.tx = Number(intent.tx || 0);
  camera.ty = Number(intent.ty || 0);
  camera.zoom = clampZoom(Number(intent.zoom || 1));
  applyCamera();
  notifyCamera();
}

/**
 * The camera that puts `center` (a point of the plan) in the middle of the
 * view, keeping the zoom the human chose (PRD §7.2, RF26).
 */
function cameraOn(center) {
  return {
    tx: viewport.w / 2 - center.x * camera.zoom,
    ty: viewport.h / 2 - center.y * camera.zoom,
  };
}

// The pan in flight, if there is one.
let pan = null;

/** Stops the pan in flight: the human took the plan with the hand. */
function cancelPan() {
  if (pan === null) {
    return;
  }
  window.cancelAnimationFrame(pan.frame);
  pan = null;
}

/**
 * Pans the camera until `target` over `PAN_MS` (PRD §7.2: an animated pan of
 * 250 ms, never a dry jump).
 *
 * The target carries the translation and nothing else: a pan that moved the
 * scale would throw away the view the human chose.
 */
function panTo(target) {
  cancelPan();
  const from = { tx: camera.tx, ty: camera.ty };
  const started = window.performance.now();
  const step = (now) => {
    const elapsed = Math.min(1, (now - started) / PAN_MS);
    // Fast at the start, soft at the end: the eye follows the object.
    const eased = 1 - Math.pow(1 - elapsed, 3);
    camera.tx = from.tx + (target.tx - from.tx) * eased;
    camera.ty = from.ty + (target.ty - from.ty) * eased;
    applyCamera();
    notifyCamera();
    pan = elapsed < 1 ? { frame: window.requestAnimationFrame(step) } : null;
  };
  pan = { frame: window.requestAnimationFrame(step) };
}

// A window resize keeps the world point that was under the center of the
// view: it never refits, because refitting would throw away the human's view
// (PRD §7.2, S23).
window.addEventListener("resize", () => {
  const next = {
    w: plan.clientWidth || window.innerWidth,
    h: plan.clientHeight || window.innerHeight,
  };
  // The world point under the center stays put when the translation moves by
  // half of the growth of the viewport.
  camera.tx += (next.w - viewport.w) / 2;
  camera.ty += (next.h - viewport.h) / 2;
  viewport = next;
  applyCamera();
  notifyCamera();
});

// --- Settings (PRD §7.1 RF25, §8.5.2 RF22, S19) --------------------------

/**
 * Reads the object Rust sends, key by key.
 *
 * The keys are the contract: they are exactly the fields of `Settings` in
 * `crates/gx-app/src/settings.rs`, and the panel never reads anything else.
 */
function settingsFromPayload(payload) {
  const p = payload || {};
  return {
    accent: String(p.accent),
    background: String(p.background),
    edge_stroke: String(p.edge_stroke),
    edge_text: String(p.edge_text),
    grid_angle_degrees: Number(p.grid_angle_degrees),
    grid_color: String(p.grid_color),
    grid_diagonal: Number(p.grid_diagonal),
    grid_opacity: Number(p.grid_opacity),
    grid_visible: Boolean(p.grid_visible),
    node_fill: String(p.node_fill),
    node_stroke: String(p.node_stroke),
    node_text: String(p.node_text),
    theme: String(p.theme),
  };
}

/** The settings the controls show right now. */
function readPanel() {
  const theme = themeInputs.find((input) => input.checked) || themeInputs[0];
  return {
    accent: accentInput.value,
    background: backgroundInput.value,
    edge_stroke: edgeStrokeInput.value,
    edge_text: edgeTextInput.value,
    grid_angle_degrees: Number(gridAngleInput.value),
    grid_color: gridColorInput.value,
    grid_diagonal: Number(gridDiagonalInput.value),
    grid_opacity: Number(gridOpacityInput.value),
    grid_visible: gridVisibleInput.checked,
    node_fill: nodeFillInput.value,
    node_stroke: nodeStrokeInput.value,
    node_text: nodeTextInput.value,
    theme: theme ? theme.value : "dark",
  };
}

/** The numbers next to the sliders, so the human sees what they chose. */
function showValues(next) {
  gridOpacityValue.textContent = Number.isFinite(next.grid_opacity)
    ? next.grid_opacity.toFixed(2)
    : "";
  gridDiagonalValue.textContent = Number.isFinite(next.grid_diagonal)
    ? `${Math.round(next.grid_diagonal)}px`
    : "";
  gridAngleValue.textContent = Number.isFinite(next.grid_angle_degrees)
    ? `${Math.round(next.grid_angle_degrees)}°`
    : "";
}

/** Shows what Rust says the file holds. */
function fillPanel(next) {
  backgroundInput.value = next.background;
  gridColorInput.value = next.grid_color;
  gridOpacityInput.value = String(next.grid_opacity);
  gridDiagonalInput.value = String(next.grid_diagonal);
  gridAngleInput.value = String(next.grid_angle_degrees);
  gridVisibleInput.checked = next.grid_visible;
  nodeFillInput.value = next.node_fill;
  nodeStrokeInput.value = next.node_stroke;
  nodeTextInput.value = next.node_text;
  edgeStrokeInput.value = next.edge_stroke;
  edgeTextInput.value = next.edge_text;
  accentInput.value = next.accent;
  for (const input of themeInputs) {
    input.checked = input.value === next.theme;
  }
  showValues(next);
}

/**
 * Paints the plan with the settings: every colour of the palette and the
 * theme.
 *
 * The theme is a real theme (RF22, S18), not a background: `data-theme` on
 * the root element switches the whole palette of the stylesheet (the chrome
 * of the panel included), and the colours the human changed are painted over
 * it as variables, so nodes, labels and arrows keep their contrast
 * (`f6-themes.md`, R1, R2).
 */
function applySettings(next) {
  settings = next;
  const root = document.documentElement;
  root.dataset.theme = THEMES.includes(next.theme) ? next.theme : "dark";
  root.style.setProperty("--bg", next.background);
  root.style.setProperty("--node-fill", next.node_fill);
  root.style.setProperty("--node-stroke", next.node_stroke);
  root.style.setProperty("--node-text", next.node_text);
  root.style.setProperty("--edge-stroke", next.edge_stroke);
  root.style.setProperty("--edge-text", next.edge_text);
  root.style.setProperty("--accent", next.accent);
  applyGrid();
}

/** A stable key for the values of a settings object. */
function settingsId(next) {
  return SETTINGS_KEYS.map((key) => `${key}=${next[key]}`).join("|");
}

/**
 * Sends the whole settings object to Rust, at most ten times per second: a
 * dragged slider fires sixty changes per second and each save is a file.
 *
 * The page never writes the file and never decides a default: Rust saves,
 * sanitises and answers with what the file holds.
 */
function notifySettings(next) {
  settingsPending = next;
  if (settingsTimer !== null) {
    return;
  }
  settingsTimer = window.setTimeout(() => {
    settingsTimer = null;
    const payload = settingsPending;
    settingsPending = null;
    const id = settingsId(payload);
    if (id === settingsSent) {
      // Nothing moved since the last write: no file, no round trip.
      return;
    }
    settingsSent = id;
    emit("settings-changed", payload);
  }, SETTINGS_THROTTLE_MS);
}

/** A control moved: the page paints it and Rust is told. */
function onPanelInput() {
  if (settings === null) {
    // Nothing has arrived from Rust yet: the empty controls are not a value
    // the human chose, and a half-filled object must never reach the file.
    return;
  }
  const next = readPanel();
  applySettings(next);
  showValues(next);
  notifySettings(next);
}

function setPanelOpen(open) {
  const wasInside = settingsPanel.contains(document.activeElement);
  settingsPanel.classList.toggle("hidden", !open);
  settingsToggle.setAttribute("aria-expanded", open ? "true" : "false");
  if (open) {
    backgroundInput.focus();
  } else if (wasInside) {
    // The keyboard goes back to where it came from.
    settingsToggle.focus();
  }
}

settingsToggle.addEventListener("click", () => {
  setPanelOpen(settingsPanel.classList.contains("hidden"));
});

settingsClose.addEventListener("click", () => {
  setPanelOpen(false);
});

settingsPanel.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    event.preventDefault();
    setPanelOpen(false);
  }
});

for (const input of [
  backgroundInput,
  gridColorInput,
  gridOpacityInput,
  gridDiagonalInput,
  gridAngleInput,
  gridVisibleInput,
  nodeFillInput,
  nodeStrokeInput,
  nodeTextInput,
  edgeStrokeInput,
  edgeTextInput,
  accentInput,
  ...themeInputs,
]) {
  input.addEventListener("input", onPanelInput);
  input.addEventListener("change", onPanelInput);
}

fillSwatches();

// --- Panel of open diagrams (PRD §7 RF26, RF28, S1, S3) -----------------

// The diagram the human last chose in the panel. It is the only mark the
// page keeps: whether a diagram is open is a fact of Rust, not of the panel.
let selectedId = null;

/**
 * Asks Rust to close a diagram (PRD §7 RF28, S3).
 *
 * The page never closes anything by itself: the model lives in Rust, the id
 * stays invalid and is never handed out again (RF28.1), and what comes back
 * in `graphium://state` is what the plan keeps showing.
 */
function emitClose(diagramId) {
  emit("graphium://close", { diagram_id: diagramId });
}

/** The moment of the last change, in the clock of the human. */
function formatUpdated(value) {
  const when = new Date(value);
  if (Number.isNaN(when.getTime())) {
    // A state without a readable date shows the raw value: the panel never
    // invents a time for a diagram.
    return String(value ?? "");
  }
  return when.toLocaleString("es", { dateStyle: "short", timeStyle: "short" });
}

/** One entry of the panel: title, kind, updated and its close button. */
function panelItem(diagram) {
  const item = document.createElement("li");
  item.className = "panel-item";
  item.classList.toggle("selected", diagram.diagram_id === selectedId);

  const open = document.createElement("button");
  open.type = "button";
  open.className = "panel-item-open";
  open.setAttribute("aria-current", diagram.diagram_id === selectedId ? "true" : "false");
  open.title = `ir a ${diagram.title}`;
  open.addEventListener("click", () => selectDiagram(diagram.diagram_id));

  const title = document.createElement("span");
  title.className = "panel-item-title";
  title.textContent = diagram.title;

  const meta = document.createElement("span");
  meta.className = "panel-item-meta";

  const kind = document.createElement("span");
  kind.className = "panel-item-kind";
  kind.textContent = diagram.kind;

  const updated = document.createElement("span");
  updated.className = "panel-item-updated";
  updated.textContent = formatUpdated(diagram.updated_at);

  meta.append(kind, updated);
  open.append(title, meta);

  const close = document.createElement("button");
  close.type = "button";
  close.className = "panel-item-close";
  close.textContent = "×";
  close.setAttribute("aria-label", `cerrar ${diagram.title}`);
  close.title = `cerrar ${diagram.title}`;
  close.addEventListener("click", () => emitClose(diagram.diagram_id));

  item.append(open, close);
  return item;
}

/**
 * Builds the list of open diagrams from the state Rust owns (PRD §8.6.1,
 * S8). Nothing else feeds it: an entry the event does not carry does not
 * exist, and the page never invents one.
 */
function renderPanel() {
  panelList.replaceChildren(...state.diagrams.map(panelItem));
  panelEmpty.classList.toggle("hidden", state.diagrams.length > 0);
}

/** Hides the panel or brings it back: one discreet button, no chrome lost. */
function setDiagramPanelOpen(open) {
  panel.classList.toggle("hidden", !open);
  panelToggle.setAttribute("aria-expanded", open ? "true" : "false");
}

panelToggle.addEventListener("click", () => {
  setDiagramPanelOpen(panel.classList.contains("hidden"));
});

/** Hides the panel of sessions or brings it back, like the panel of
 * diagrams: one discreet button, and the two panels are independent. */
function setSessionsPanelOpen(open) {
  sessionsPanel.classList.toggle("hidden", !open);
  sessionsToggle.setAttribute("aria-expanded", open ? "true" : "false");
}

sessionsToggle.addEventListener("click", () => {
  setSessionsPanelOpen(sessionsPanel.classList.contains("hidden"));
});

/** The center of the object of a diagram, in plan units, from the state. */
function objectCenter(diagram) {
  const pos = diagram.pos;
  const bbox = diagram.bbox;
  if (!pos || !bbox) {
    // A diagram without a place has no place to go yet: the page does not
    // invent coordinates for it.
    return null;
  }
  // `bbox` is already the object (the SVG plus its padding, PRD §7.2).
  return { x: pos.x + bbox.w / 2, y: pos.y + bbox.h / 2 };
}

/**
 * Moves the camera until a diagram, keeping the zoom (PRD §7 RF26): the view
 * travels, the scale stays where the human left it.
 */
function focusDiagram(diagram) {
  const center = objectCenter(diagram);
  if (center === null) {
    return;
  }
  panTo(cameraOn(center));
}

/** The human chose an entry: it is marked and the camera goes to it. */
function selectDiagram(diagramId) {
  const diagram = state.diagrams.find((entry) => entry.diagram_id === diagramId);
  if (!diagram) {
    return;
  }
  // The human asked for this diagram, so the plan shows its session if the
  // selector was hiding it: the camera never travels to an object nobody can
  // see (D5). Choosing from the list keeps working as it always did.
  if (!isVisibleSession(diagram.session_id)) {
    selectSessionById(diagram.session_id);
  }
  selectedId = diagramId;
  renderPanel();
  focusDiagram(diagram);
}

// --- Selector of sessions (S1, S3, D3, D5) ------------------------------

// The family of a session as the human reads it and the colour it paints
// with. The keys are the wire strings of `SessionFamily::as_str`
// (`crates/gx-mcp/src/session.rs`); the colours are tokens of `style.css`. A
// family the page does not know reads and paints as `other`: a mislabelled
// agent is worse than an unlabelled one (D1).
const SESSION_FAMILIES = {
  pi: "Pi",
  claude: "Claude",
  opencode: "OpenCode",
  ui: "Interfaz",
  other: "Otro",
};

// The session the human chose to watch, or `null` for «Todas» (S3). The
// filter only hides objects of the plan: the diagrams of the other sessions
// stay open in Rust, with the place Rust gave them (D5).
let sessionFilter = null;
// Why the plan went back to «Todas», when it had to. Empty means no notice.
let sessionMessage = "";

/** The colour class of a family: one of the five tokens, else `other`. */
function familyClass(family) {
  return Object.prototype.hasOwnProperty.call(SESSION_FAMILIES, family)
    ? `family-${family}`
    : "family-other";
}

/** The family of a session as the human reads it. */
function familyName(family) {
  return SESSION_FAMILIES[family] || SESSION_FAMILIES.other;
}

/** Whether the plan is showing every session («Todas» is selected). */
function showsEverySession() {
  return sessionFilter === null;
}

/** Whether a diagram of `sessionId` belongs to what the plan shows now. */
function isVisibleSession(sessionId) {
  return sessionFilter === null || sessionId === sessionFilter.id;
}

/** How many diagrams of a session are on the plan right now. */
function sessionDiagramCount(sessionId) {
  return state.diagrams.filter((diagram) => diagram.session_id === sessionId).length;
}

/** The last part of a project path: `/home/x/graphium` -> `graphium`. */
function projectName(project) {
  const text = typeof project === "string" ? project : "";
  const parts = text.split("/").filter((part) => part !== "");
  return parts.length > 0 ? parts[parts.length - 1] : "";
}

/** The text of the count: `1 diagrama`, `3 diagramas`. */
function diagramCountText(count) {
  return `${count} ${count === 1 ? "diagrama" : "diagramas"}`;
}

/** The dot of a family: the colour of whoever drew the diagram (D3). */
function sessionDot(family) {
  const dot = document.createElement("span");
  dot.className = `session-dot ${familyClass(family)}`;
  return dot;
}

/**
 * One row of the selector: the colour of the family, the label of the
 * session, its project, whether it is `activa` or `en gracia` and how many
 * diagrams it has on the plan. Every piece of it comes from the state Rust
 * emits; the page invents none.
 */
function sessionRow(session) {
  const row = document.createElement("button");
  row.type = "button";
  row.className = "session-row";
  const chosen = !showsEverySession() && sessionFilter.id === session.id;
  row.classList.toggle("selected", chosen);
  row.classList.toggle("in-grace", session.state === "in_grace");
  row.setAttribute("aria-pressed", chosen ? "true" : "false");
  row.title = session.label;
  row.addEventListener("click", () => selectSession(session));

  const head = document.createElement("span");
  head.className = "session-head";
  const label = document.createElement("span");
  label.className = "session-label";
  label.textContent = session.label;
  const diagramsOfSession = sessionDiagramCount(session.id);
  const count = document.createElement("span");
  count.className = "session-count";
  count.textContent = String(diagramsOfSession);
  count.title = diagramCountText(diagramsOfSession);
  head.append(label, count);

  const meta = document.createElement("span");
  meta.className = "session-meta";
  const family = document.createElement("span");
  family.className = "session-family";
  family.textContent = familyName(session.family);
  const project = document.createElement("span");
  project.className = "session-project";
  const name = projectName(session.project);
  project.textContent = name || "sin proyecto";
  if (name) {
    project.title = session.project;
  }
  const status = document.createElement("span");
  status.className = "session-state";
  status.textContent = session.state === "in_grace" ? "en gracia" : "activa";
  meta.append(family, project, status);

  const body = document.createElement("span");
  body.className = "session-body";
  body.append(head, meta);
  row.append(sessionDot(session.family), body);
  return row;
}

/**
 * The fixed first row: «Todas». It shows every session again, which is what
 * the plan shows before the human chooses anything.
 */
function allSessionsRow() {
  const row = document.createElement("button");
  row.type = "button";
  row.className = "session-row";
  row.classList.toggle("selected", showsEverySession());
  row.setAttribute("aria-pressed", showsEverySession() ? "true" : "false");
  row.title = "mostrar todas las sesiones";
  row.addEventListener("click", () => selectSession(null));

  const head = document.createElement("span");
  head.className = "session-head";
  const label = document.createElement("span");
  label.className = "session-label";
  label.textContent = "Todas";
  const count = document.createElement("span");
  count.className = "session-count";
  count.textContent = String(state.diagrams.length);
  count.title = diagramCountText(state.diagrams.length);
  head.append(label, count);

  const meta = document.createElement("span");
  meta.className = "session-meta";
  const all = document.createElement("span");
  all.className = "session-family";
  all.textContent = "todas las sesiones";
  meta.append(all);

  const body = document.createElement("span");
  body.className = "session-body";
  body.append(head, meta);

  const dot = document.createElement("span");
  dot.className = "session-dot family-all";
  row.append(dot, body);
  return row;
}

/**
 * Shows the objects of the chosen session and hides the others (D5).
 *
 * Hiding is one class that takes the object out of the paint (`display:
 * none` in the stylesheet): the camera, the rectangle of the object and the
 * place Rust gave it are untouched, so «Todas» brings every diagram back
 * exactly where it was. Nothing is closed here: the diagrams of the other
 * sessions keep living in Rust.
 */
function applySessionFilter() {
  for (const element of objects.values()) {
    element.classList.toggle("session-hidden", !isVisibleSession(element.sessionId || ""));
  }
  // The badge of a diagram says whose work it is; with one session chosen
  // every badge would say the same thing, so it only shows under «Todas»
  // (S1, D3).
  diagrams.classList.toggle("show-session-badges", showsEverySession());
}

/**
 * The human chose a row of the selector (S3): the plan shows that session and
 * hides the rest, or shows them all again with «Todas». The camera is not
 * moved: filtering hides, it never travels (D5).
 */
function selectSession(session) {
  sessionFilter = session === null ? null : { id: session.id, label: session.label };
  sessionMessage = "";
  applySessionFilter();
  // The drawings that come back are mounted again by the camera that is
  // already there: applying it changes nothing of the view, it only lets the
  // culling decide which objects keep their SVG.
  applyCamera();
  renderSessions();
}

/** Chooses a session by its id; an id the selector does not list means «Todas». */
function selectSessionById(sessionId) {
  selectSession(state.sessions.find((session) => session.id === sessionId) || null);
}

/**
 * The session the human was watching is gone: its grace ended and the owner
 * dropped it with its diagrams (D4, S2). The selector says so and the plan
 * goes back to «Todas», instead of leaving an empty plan with no explanation
 * (S3, D5).
 */
function reconcileSessionFilter() {
  if (sessionFilter === null) {
    return;
  }
  if (state.sessions.some((session) => session.id === sessionFilter.id)) {
    return;
  }
  sessionMessage = `la sesión «${sessionFilter.label}» terminó; vuelvo a «Todas»`;
  sessionFilter = null;
}

/**
 * Builds the selector from the state: the fixed row «Todas» and one row per
 * session, in the order Rust lists them (D3: the human sees every session).
 * The notice explains why the plan fell back to «Todas», when it had to.
 */
function renderSessions() {
  const rows = [allSessionsRow()];
  for (const session of state.sessions) {
    rows.push(sessionRow(session));
  }
  sessionList.replaceChildren(...rows);
  sessionNotice.textContent = sessionMessage;
  sessionNotice.classList.toggle("hidden", sessionMessage === "");
}

/**
 * The colour and the name of the session that drew a diagram (S1, D3): the
 * badge of the object. A diagram whose session the state does not list keeps
 * a badge with no colour instead of inventing one.
 */
function paintBadge(element) {
  const session = state.sessions.find((entry) => entry.id === element.sessionId);
  if (!session) {
    element.badge.className = "diagram-badge";
    element.badge.title = "";
    return;
  }
  element.badge.className = `diagram-badge ${familyClass(session.family)}`;
  element.badge.title = session.label;
}

// --- Diagrams -----------------------------------------------------------

function worldCenter() {
  const rect = plan.getBoundingClientRect();
  return {
    x: (rect.width / 2 - camera.tx) / camera.zoom,
    y: (rect.height / 2 - camera.ty) / camera.zoom,
  };
}

function insertDiagram(payload) {
  const container = document.createElement("div");
  container.className = "diagram";

  const content = document.createElement("div");
  content.className = "content";

  const holder = document.createElement("div");
  holder.innerHTML = payload.svg;

  const svg = holder.querySelector("svg");
  if (!svg) {
    emit("render_ack", { req_id: payload.req_id, ok: false });
    return;
  }

  const width = Number.parseFloat(svg.getAttribute("width") || "0");
  const height = Number.parseFloat(svg.getAttribute("height") || "0");
  // Rust says where the diagram goes in plan units (PRD §7.2, `place`);
  // without that intent it lands on the center of the view. `placement` is
  // the corner of the **object**, so the SVG sits one padding inside it.
  const placement = payload.placement;
  const center = placement
    ? {
        x: placement.x + (width + 2 * OBJECT_PADDING) / 2,
        y: placement.y + (height + 2 * OBJECT_PADDING) / 2,
      }
    : worldCenter();

  content.style.left = `${center.x - width / 2}px`;
  content.style.top = `${center.y - height / 2}px`;
  // The content keeps the size of its drawing even while the drawing is out
  // of the DOM (RNF1-ter): the `×` of the object hangs from its top-right
  // corner, and the object must not change shape because of the culling.
  content.style.width = `${width}px`;
  content.style.height = `${height}px`;
  content.appendChild(svg);
  container.appendChild(content);

  // The `×` of the object (PRD §7 RF28, S3): it shows up when the pointer
  // walks over the diagram. It sits in the padding ring of the object, so it
  // never covers the drawing nor the arrows of the plan.
  const close = document.createElement("button");
  close.type = "button";
  close.className = "diagram-close";
  close.textContent = "×";
  close.setAttribute("aria-label", "cerrar el diagrama");
  close.title = "cerrar el diagrama";
  close.addEventListener("pointerdown", (event) => {
    // The plan starts the drag of the camera on its own pointer down: the
    // button is not the background, so it does not move the view.
    event.stopPropagation();
  });
  close.addEventListener("click", (event) => {
    event.stopPropagation();
    emitClose(payload.diagram_id);
  });
  content.appendChild(close);

  // The badge of the session (S1, D3): a dot in the colour of whoever drew
  // this diagram. It is positioned inside the padding ring of the object and
  // is never in its flow, so the drawing keeps its geometry; the state paints
  // its colour, because the family arrives with the sessions.
  const badge = document.createElement("span");
  badge.className = "diagram-badge";
  content.appendChild(badge);

  // The rectangle of the object in plan units (PRD §7.2: the SVG plus its
  // 24 px padding). The state carries the same numbers; this is the copy the
  // culling tests every frame, and it never asks the DOM for a layout to get
  // it.
  container.rect = {
    x: center.x - width / 2 - OBJECT_PADDING,
    y: center.y - height / 2 - OBJECT_PADDING,
    w: width + 2 * OBJECT_PADDING,
    h: height + 2 * OBJECT_PADDING,
  };

  // The box that stands in for the drawing while its SVG is out of the DOM
  // (RNF1-ter). It is created with the object and only shown when the
  // culling asks for it, so the plan never changes shape because of it.
  const box = document.createElement("div");
  box.className = "diagram-box hidden";
  const boxLabel = document.createElement("span");
  boxLabel.className = "diagram-box-label";
  box.appendChild(boxLabel);
  // The box goes **before** the content, so the `×` of the object (a child of
  // `.content`, in the padding ring) keeps painting above it: a culled
  // diagram is still closable from its own object (PRD §7 RF28).
  container.insertBefore(box, content);

  // What the culling keeps of this object, hung from the object itself: the
  // drawing it may have to leave out of the page, the box that replaces it,
  // the rectangle to test and the title and the size to show. It is not a
  // second map on purpose: when the state drops the diagram the element
  // leaves the plan and its record leaves with it, so the two can never
  // drift.
  container.diagramId = payload.diagram_id;
  container.svg = svg;
  container.content = content;
  container.box = box;
  container.boxLabel = boxLabel;
  container.badge = badge;
  container.caption = "";
  container.culled = false;
  paintBox(container);

  // F2: a new diagram **sums** to the plan, it does not replace the one
  // that was there (PRD §11.1, S9). The plan is a canvas with room for
  // several objects, each one where Rust placed it.
  //
  // The first diagram of the plan frames the view; from the second on the
  // camera pans until the new object and the zoom of the human is never
  // touched (PRD §7.2, RF27). The answer is read before the new object
  // lands, or the first one would count as the second.
  const first = framesOnInsert();
  objects.set(payload.diagram_id, container);
  diagrams.appendChild(container);
  // The size of the plan is refreshed before the camera paints, so the
  // culling tests the new object against the window it really has.
  viewport = {
    w: plan.clientWidth || window.innerWidth,
    h: plan.clientHeight || window.innerHeight,
  };
  applyCamera();
  empty.classList.add("hidden");

  // One single place decides what the camera does with the intent of the
  // event: mostrar la cámara del render cuando el plano está vacío o cuando
  // el agente pidió `opts.focus`, y hacer pan sin tocar el zoom en el resto.
  if (first || payload.focus) {
    applyCameraIntent(payload.camera);
  } else if (placement) {
    panTo(cameraOn(center));
  }

  // The frontend answers after two animation frames: the SVG is painted
  // (PRD §8.6.1, step 4).
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      emit("render_ack", { req_id: payload.req_id, ok: true });
    });
  });
}

/**
 * Whether this render is the first diagram of the plan.
 *
 * Only the first one is framed with the camera of the event: from the second
 * on the plan pans until the new object and keeps the zoom (PRD §7.2, RF27).
 */
function framesOnInsert() {
  return objects.size === 0;
}

// --- Culling (PRD RNF1-ter, §7.2, §7.3) ---------------------------------

/**
 * The rectangle of the plan the camera shows, in plan units.
 *
 * `viewport` is the size of the plan in screen pixels and the camera scales
 * plan units by `zoom`, so the division gives the window the objects are
 * tested against. The size the page already keeps is used on purpose: the
 * culling never reads a layout in the middle of a frame.
 */
function visibleRect() {
  return {
    x: -camera.tx / camera.zoom,
    y: -camera.ty / camera.zoom,
    w: viewport.w / camera.zoom,
    h: viewport.h / camera.zoom,
  };
}

/** Whether two rectangles of the plan share at least one point. */
function rectsIntersect(a, b) {
  return a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
}

/** How far the centre of `rect` is from the centre of the view, squared. */
function distanceToView(rect, view) {
  const dx = rect.x + rect.w / 2 - (view.x + view.w / 2);
  const dy = rect.y + rect.h / 2 - (view.y + view.h / 2);
  return dx * dx + dy * dy;
}

/**
 * What the box of a culled diagram says (RNF1-ter): its title and the size
 * of its object in plan units, e.g. `schema — 320 × 180`. The size is the
 * rectangle the box occupies, padding included, so the number is the one the
 * human sees on the plan.
 */
function boxText(element) {
  // A diagram the state has not listed yet has no title to show: the page
  // falls back to the identifier Rust gave it instead of inventing one.
  const title = element.caption || element.diagramId;
  const width = Math.round(element.rect.w);
  const height = Math.round(element.rect.h);
  return `${title} — ${width} × ${height}`;
}

/**
 * Paints the box of an object: the very rectangle of the object in plan
 * units, plus the title and the size it reads.
 */
function paintBox(element) {
  element.box.style.left = `${element.rect.x}px`;
  element.box.style.top = `${element.rect.y}px`;
  element.box.style.width = `${element.rect.w}px`;
  element.box.style.height = `${element.rect.h}px`;
  element.boxLabel.textContent = boxText(element);
}

/**
 * Takes the drawing of an object out of the DOM (RNF1-ter).
 *
 * The node is **not** dropped: it stays reachable from the object
 * (`element.svg`), so coming back into the view re-appends the same node
 * instead of parsing the markup of the render again. The `div.diagram` and
 * its `div.content` stay where they are: the object keeps its place and its
 * size at every zoom (PRD §7.2), and only its content changes.
 */
function cullObject(element) {
  if (element.culled) {
    return;
  }
  element.culled = true;
  element.svg.remove();
  element.box.classList.remove("hidden");
}

/** Brings the drawing of an object back into the DOM (RNF1-ter). */
function materializeObject(element) {
  if (!element.culled) {
    return;
  }
  element.culled = false;
  // The `×` of the object is the last child of `.content`: the drawing goes
  // before it, the way `insertDiagram` left it.
  element.content.prepend(element.svg);
  element.box.classList.add("hidden");
}

/**
 * Decides, object by object, whether its SVG stays in the DOM (PRD
 * RNF1-ter; §7.2, `culling`).
 *
 * An object out of the view, or seen at a zoom below `CULL_ZOOM`, becomes a
 * box with its title and its size; the rest keep their drawing, up to
 * `MAX_LIVE_SVGS` of them. The PRD fixes the ceiling and not the tie-break,
 * so the objects closest to the centre of the view are the ones that stay
 * materialized: they are the ones the human is looking at. A tie keeps the
 * order of insertion.
 *
 * It runs on every `applyCamera()` (each frame of the pan, each step of the
 * wheel, each insertion, focus and fit) and after every state, and costs one
 * pass over the objects with the rectangles the state already carries: no
 * `getBBox()` and no layout read per object in the middle of a frame.
 */
function applyCulling() {
  const view = visibleRect();
  // Below this zoom a drawing is not worth its bytes, and the boxes read
  // better than a shrunken diagram (PRD §7.2, §7.3).
  const zoomedOut = camera.zoom < CULL_ZOOM;
  const candidates = [];
  for (const element of objects.values()) {
    // An object the selector is hiding keeps no drawing in the page: the
    // three live SVGs are for the diagrams the human is looking at (D5).
    if (element.classList.contains("session-hidden")) {
      cullObject(element);
      continue;
    }
    if (zoomedOut || !rectsIntersect(element.rect, view)) {
      cullObject(element);
      continue;
    }
    candidates.push({ element, distance: distanceToView(element.rect, view) });
  }
  candidates.sort((left, right) => left.distance - right.distance);
  for (let index = 0; index < candidates.length; index += 1) {
    if (index < MAX_LIVE_SVGS) {
      materializeObject(candidates[index].element);
    } else {
      cullObject(candidates[index].element);
    }
  }
}

/**
 * Reads from the state where an object sits and what it is called (PRD
 * §8.6.1, S8). The page invents neither: a state without `pos`/`bbox` keeps
 * the rectangle the render placed, and a title is only replaced by one.
 */
function readObject(element, diagram) {
  if (typeof diagram.title === "string" && diagram.title !== "") {
    element.caption = diagram.title;
  }
  // Which session drew this diagram: the key the selector filters by and the
  // colour of the badge (S1, D3). It is the same id Rust lists in `sessions`.
  if (typeof diagram.session_id === "string" && diagram.session_id !== "") {
    element.sessionId = diagram.session_id;
  }
  paintBadge(element);
  if (diagram.pos && diagram.bbox) {
    element.rect = {
      x: Number(diagram.pos.x),
      y: Number(diagram.pos.y),
      w: Number(diagram.bbox.w),
      h: Number(diagram.bbox.h),
    };
  }
  paintBox(element);
}

/**
 * Drops from the plan the objects Rust no longer lists (PRD §7 RF28): the
 * state is the truth, so a closed diagram leaves the plan when the state says
 * so and not when the page feels like it.
 */
function syncObjects() {
  const open = new Set(state.diagrams.map((diagram) => diagram.diagram_id));
  for (const [diagramId, element] of objects) {
    if (open.has(diagramId)) {
      continue;
    }
    element.remove();
    objects.delete(diagramId);
  }
  if (selectedId !== null && !open.has(selectedId)) {
    selectedId = null;
  }
  // The state is also where the title and the rectangle of an object come
  // from (PRD §8.6.1, S8): a box left on the plan reads them from here, and
  // the culling tests the rectangle Rust placed.
  for (const diagram of state.diagrams) {
    const element = objects.get(diagram.diagram_id);
    if (element) {
      readObject(element, diagram);
    }
  }
  // The selector hides the objects of the sessions the human is not watching
  // and badges the rest (D3, D5), before the culling decides which drawings
  // stay in the page.
  applySessionFilter();
  // A state may have moved an object or given it a title: whether it keeps
  // its SVG depends on both, so the culling is decided again.
  applyCulling();
}

function showError(payload) {
  bannerPath.textContent = payload.path || "";
  bannerMessage.textContent = payload.message || "";
  bannerHint.textContent = payload.hint || "";
  const extra = Number(payload.extra || 0);
  bannerExtra.textContent = extra > 0 ? `+${extra} errores` : "";
  banner.classList.remove("hidden");
}

document.getElementById("banner-close").addEventListener("click", () => {
  banner.classList.add("hidden");
});

function showPending(payload) {
  ghostText.textContent = "calculando layout…";
  ghost.classList.remove("hidden");
  window.clearTimeout(showPending.timer);
  showPending.timer = window.setTimeout(() => {
    const nodes = Number(payload.nodes || 0);
    const edges = Number(payload.edges || 0);
    ghostText.textContent = `${nodes} nodos, ${edges} edges`;
  }, 2000);
}

function hidePending() {
  ghost.classList.add("hidden");
  window.clearTimeout(showPending.timer);
}

// --- Wiring -------------------------------------------------------------

listen("graphium://ready", () => {});
listen("graphium://render", (event) => {
  hidePending();
  insertDiagram(event.payload);
});
listen("graphium://state", (event) => {
  // The model of the diagrams lives in Rust (PRD §8.6.1): the frontend only
  // reads the state it receives. It carries the diagrams of the plan and
  // **every** session the human can choose from (S1, S3, D3).
  const payload = event.payload || {};
  const diagrams = Array.isArray(payload.diagrams) ? payload.diagrams : [];
  state.diagrams = diagrams;
  state.sessions = Array.isArray(payload.sessions) ? payload.sessions : [];
  // A watched session the owner dropped takes the plan back to «Todas», with
  // a notice, never to an empty plan with no explanation (S3, D5).
  reconcileSessionFilter();
  syncObjects();
  renderPanel();
  renderSessions();
  empty.classList.toggle("hidden", diagrams.length > 0);
});
listen("graphium://pending", (event) => {
  showPending(event.payload || {});
});
listen("graphium://error", (event) => {
  hidePending();
  showError(event.payload || {});
});
listen("graphium://settings", (event) => {
  const next = settingsFromPayload(event.payload);
  // While the human is still moving a control the panel is the truth: the
  // confirmation of an older value must not pull a slider back under their
  // hand. The last change is always confirmed once the panel goes quiet.
  if (settingsTimer !== null || settingsPending !== null) {
    return;
  }
  settingsSent = settingsId(next);
  fillPanel(next);
  applySettings(next);
});

applyCamera();
emit("graphium://ready", {});
