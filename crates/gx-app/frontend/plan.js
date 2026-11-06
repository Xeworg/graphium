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
// State 1 of the user plan (B1 of `f8-planos.md`, S4): the human's own surface
// says what to do with it, instead of waiting for an agent that draws
// somewhere else.
const emptyUser = document.getElementById("empty-user");
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

// The switcher of plans (A2 of `f8-planos.md`; S1, S3, D3, D5): the rows of
// the panel of sessions and the notice that says when the plan on screen had
// to change because its session ended.
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
// The state Rust owns (PRD §8.6.1): the diagrams of the plan, every session
// the switcher lists and the clones of the user plan (`f8-planos.md`, B1).
const state = { diagrams: [], sessions: [], clones: [] };
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
// The clone the pointer is moving (B3 of `f8-planos.md`, S4): dragging an
// **object** of the user plan moves it, dragging the background keeps panning
// the camera. A session plan never moves an object — what an agent drew stays
// where the owner placed it.
let moving = null;

/**
 * The clone under the pointer, when the plan on screen is the one of the human
 * (B3). Anywhere else there is nothing to grab: the background pans the camera,
 * which is what it has always done.
 */
function cloneUnder(target) {
  if (activePlan === null || activePlan.id !== USER_PLAN_ID) {
    return null;
  }
  const container = target instanceof Element ? target.closest(".diagram.clone") : null;
  return container && container.cloneId ? container : null;
}

/**
 * Starts moving the clone the hand grabbed (B3): the object becomes the chosen
 * one (the arrows of the keyboard walk its history), the pointer is captured
 * and the place it started from is kept, so a click writes nothing.
 */
function moveCloneStart(event, element) {
  selectedId = element.cloneId;
  moving = {
    element,
    fromX: event.clientX,
    fromY: event.clientY,
    x: element.rect.x,
    y: element.rect.y,
  };
  plan.classList.add("dragging");
  plan.setPointerCapture(event.pointerId);
  event.preventDefault();
}

/** Puts a clone where the hand says, in plan units, and paints the object. */
function setClonePlace(element, x, y) {
  element.rect = { ...element.rect, x, y };
  placeCloneContent(element);
  paintBox(element);
}

plan.addEventListener("pointerdown", (event) => {
  // The same for the drag: the hand of the human wins over the animation.
  cancelPan();
  // A button of a clone (a step of its history, its refresh) does its own
  // thing: it never starts a move of the object nor a drag of the camera.
  if (event.target instanceof Element && event.target.closest("button")) {
    return;
  }
  const clone = cloneUnder(event.target);
  if (clone !== null) {
    moveCloneStart(event, clone);
    return;
  }
  dragging = { x: event.clientX, y: event.clientY, tx: camera.tx, ty: camera.ty };
  plan.classList.add("dragging");
  plan.setPointerCapture(event.pointerId);
});

plan.addEventListener("pointermove", (event) => {
  if (moving !== null) {
    // The pointer travels in screen pixels and the plan in plan units: the zoom
    // of the camera is what joins them (B3).
    const zoom = camera.zoom > 0 ? camera.zoom : 1;
    setClonePlace(
      moving.element,
      moving.x + (event.clientX - moving.fromX) / zoom,
      moving.y + (event.clientY - moving.fromY) / zoom,
    );
    return;
  }
  if (!dragging) {
    return;
  }
  camera.tx = dragging.tx + (event.clientX - dragging.x);
  camera.ty = dragging.ty + (event.clientY - dragging.y);
  applyCamera();
  notifyCamera();
});

function endDrag(event) {
  // The hand let go of a clone: the object stays where it is and the owner
  // keeps that place (B3).
  if (moving !== null) {
    const move = moving;
    moving = null;
    plan.classList.remove("dragging");
    if (plan.hasPointerCapture(event.pointerId)) {
      plan.releasePointerCapture(event.pointerId);
    }
    const element = move.element;
    if (element.rect.x !== move.x || element.rect.y !== move.y) {
      // Only the position of the view travels: no camera, no store, no render
      // (V2). A click without movement asks for nothing.
      emit("graphium://clone-move", {
        clone_id: element.cloneId,
        pos: { x: element.rect.x, y: element.rect.y },
      });
    }
    return;
  }
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

/**
 * Asks Rust to put a clone of a diagram on the user plan (B1; S2, S4).
 *
 * The clone is a view instance of the owner: the page does not copy the
 * drawing, does not pick a place and does not invent an id — Rust renders the
 * current document read-only, places the object on the user plan and answers
 * with `graphium://clone-paint` and a new state.
 */
function emitClone(diagramId) {
  emit("graphium://clone", { diagram_id: diagramId });
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

/** One entry of the panel: title, kind, updated, its clone and its close. */
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

  // Cloning onto the user plan (B1 of `f8-planos.md`, S2): the human copies the
  // diagram to a surface where no agent draws, to analyze it with calm. Same
  // visual grammar as the `×`, and it only asks: the clone never becomes a
  // second `diagram_id` of the store (S4).
  const clone = document.createElement("button");
  clone.type = "button";
  clone.className = "panel-item-clone";
  clone.textContent = "⧉";
  clone.setAttribute("aria-label", `clonar ${diagram.title} al plano del usuario`);
  clone.title = "clonar al plano del usuario";
  clone.addEventListener("click", () => emitClone(diagram.diagram_id));

  item.append(open, clone, close);

  // The history of the diagram (RF14, L5): «◀ v3/7 ▶» when the store keeps
  // more than one version. Navigating draws the chosen version on the plan,
  // read-only; the newest brings the live drawing back.
  if (diagram.version_count > 1) {
    item.append(versionNav(diagram));
  }
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
  // The human asked for this diagram, so its plan comes on screen: the camera
  // never travels to an object nobody can see (A2, D5). Choosing from the list
  // keeps working as it always did.
  showPlan(planOf(diagram.session_id));
  selectedId = diagramId;
  renderPanel();
  focusDiagram(diagram);
}

// --- History of versions (RF14; f2-cierre.md, T5) ------------------------

// The version of the history the human is looking at, per diagram: the
// absolute number of the version while a preview is on the plan, and
// nothing when the live drawing is. The page never writes the store: Rust
// renders the version read-only and answers with `graphium://version-paint`.
const viewedVersion = new Map(); // diagram_id -> version number

/**
 * One step of the history: ◀ walks back, ▶ walks forward, from the version
 * in view or from the live drawing when the human was not browsing.
 */
function versionStep(diagram, step) {
  const at = viewedVersion.get(diagram.diagram_id) ?? diagram.current_version;
  viewVersion(diagram, at + step);
}

/**
 * The history of the diagram the panel item shows (RF14, L5): «◀ v3/7 ▶».
 *
 * The number is the position of the version inside the ones the store
 * keeps (`oldest_version..=current_version`, absolute and consecutive), so
 * a history at its cap reads «v20/20». Without a version in view the
 * indicator names the live drawing, which is the last position. A version
 * the history discarded cannot be shown: the view falls back to the live
 * drawing instead of pointing at a number that no longer exists.
 */
function versionNav(diagram) {
  const nav = document.createElement("div");
  nav.className = "panel-item-versions";

  const viewed = viewedVersion.get(diagram.diagram_id) ?? null;
  const at =
    viewed !== null &&
    viewed >= diagram.oldest_version &&
    viewed <= diagram.current_version
      ? viewed
      : diagram.current_version;

  const prev = document.createElement("button");
  prev.type = "button";
  prev.className = "panel-item-version-step";
  prev.textContent = "◀";
  prev.setAttribute("aria-label", `versión anterior de ${diagram.title}`);
  prev.disabled = at <= diagram.oldest_version;
  prev.addEventListener("click", () => versionStep(diagram, -1));

  const label = document.createElement("span");
  label.className = "panel-item-version-label";
  label.textContent = `v${at - diagram.oldest_version + 1}/${diagram.version_count}`;

  const next = document.createElement("button");
  next.type = "button";
  next.className = "panel-item-version-step";
  next.textContent = "▶";
  next.setAttribute("aria-label", `versión siguiente de ${diagram.title}`);
  next.disabled = at >= diagram.current_version;
  next.addEventListener("click", () => versionStep(diagram, 1));

  nav.append(prev, label, next);
  return nav;
}

/**
 * Asks Rust to draw one version of the history on the plan (RF14, L5).
 *
 * Read-only on purpose: Rust renders the version without the hook, the
 * store keeps every fact it had, and the object never moves — the preview
 * takes the place the live drawing had. Walking back to the newest version
 * restores the live drawing the page already holds, so the indicator reads
 * «v7/7» without a render at all.
 */
function viewVersion(diagram, version) {
  const target = Math.min(
    diagram.current_version,
    Math.max(diagram.oldest_version, version),
  );
  if (target === diagram.current_version) {
    viewedVersion.delete(diagram.diagram_id);
    restoreLive(objects.get(diagram.diagram_id));
  } else {
    viewedVersion.set(diagram.diagram_id, target);
    emit("graphium://version-view", {
      diagram_id: diagram.diagram_id,
      version: target,
    });
  }
  // The indicator of the item follows the view at once: a click is answered
  // even when the render of the preview takes a moment.
  renderPanel();
}

/**
 * Paints the preview of a version where the live drawing was (L5): the
 * object keeps its rectangle on the plan — the culling keeps testing the
 * rectangle Rust placed — and the preview centers on it, so the plan never
 * changes shape because of the history. The live drawing stays reachable
 * on the object for the restore.
 */
function paintPreview(element, svg) {
  element.svg.remove();
  if (element.previewSvg) {
    element.previewSvg.remove();
  }
  const width = Number.parseFloat(svg.getAttribute("width") || "0");
  const height = Number.parseFloat(svg.getAttribute("height") || "0");
  const center = {
    x: element.rect.x + element.rect.w / 2,
    y: element.rect.y + element.rect.h / 2,
  };
  element.content.style.left = `${center.x - width / 2}px`;
  element.content.style.top = `${center.y - height / 2}px`;
  element.content.style.width = `${width}px`;
  element.content.style.height = `${height}px`;
  // The `×` is the last child of `.content`: the drawing goes first, the way
  // insertDiagram leaves it.
  element.content.prepend(svg);
  element.previewSvg = svg;
}

/** Brings the live drawing back: the preview leaves the plan. */
function restoreLive(element) {
  if (!element || !element.previewSvg) {
    return;
  }
  element.previewSvg.remove();
  element.previewSvg = null;
  // The styles of the live drawing come from the rectangle Rust placed:
  // the SVG sits one padding inside the object, as at insert time.
  const width = element.rect.w - 2 * OBJECT_PADDING;
  const height = element.rect.h - 2 * OBJECT_PADDING;
  const center = {
    x: element.rect.x + element.rect.w / 2,
    y: element.rect.y + element.rect.h / 2,
  };
  element.content.style.left = `${center.x - width / 2}px`;
  element.content.style.top = `${center.y - height / 2}px`;
  element.content.style.width = `${width}px`;
  element.content.style.height = `${height}px`;
  if (!element.svg.isConnected) {
    element.content.prepend(element.svg);
  }
}

// --- Switcher of plans (S1, S3, D3, D5; f8-planos.md, A2) ----------------

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

// The plan on screen (A2 of `f8-planos.md`, S1): the session whose diagrams
// the human is looking at, one at a time. The other diagrams stay open in
// Rust, with the place Rust gave them (D5); the plan just does not paint them.
// `null` until the state names a session.
let activePlan = null; // { id, label }
// The camera of each plan, keyed by the id of its session (A2). The page keeps
// the view of every plan, so coming back to one restores the camera the human
// left in it. Rust follows only the camera of the plan on screen, because that
// is the plan the next render is placed against.
const planCameras = new Map();
// Why the plan on screen changed without the human asking, when it had to.
// Empty means no notice.
let sessionMessage = "";

// The plan of the human (B1 of `f8-planos.md`; S2, S4): the row at the top of
// the switcher, always there and independent of the sessions. It holds the
// clones (`state.clones`), which live in the owner as **view instances**: they
// carry no `diagram_id`, they do not take a slot of the store and no agent can
// see, refresh or close them. The camera of the user plan is one entry of
// `planCameras`, like any other plan.
const USER_PLAN_ID = "user";
const USER_PLAN = { id: USER_PLAN_ID, label: "Mi plano" };
// The version of the original a clone is showing, per clone: the number of the
// version while the human browses an older one, and nothing while the clone
// shows the newest it pulled. The page never writes the store: Rust renders the
// version read-only and answers `graphium://clone-paint` (B2, B3).
const viewedCloneVersion = new Map(); // clone_id -> version number

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

/** Whether the plan on screen is the one of `planId` (A2; S4, B1). */
function isVisiblePlan(planId) {
  return activePlan !== null && planId === activePlan.id;
}

/** How many diagrams of a session are on the plan right now. */
function sessionDiagramCount(sessionId) {
  return state.diagrams.filter((diagram) => diagram.session_id === sessionId).length;
}

/** How many objects the plan on screen has (A2; B1). */
function planDiagramCount() {
  if (activePlan === null) {
    return 0;
  }
  if (activePlan.id === USER_PLAN_ID) {
    return state.clones.length;
  }
  return sessionDiagramCount(activePlan.id);
}

/**
 * Paints the empty surface of the plan on screen: the plan of the human has
 * its own (S4, B1) and neither of them shows while the plan has objects.
 */
function paintEmpty() {
  const hasObjects = planDiagramCount() > 0;
  const onUserPlan = activePlan !== null && activePlan.id === USER_PLAN_ID;
  empty.classList.toggle("hidden", hasObjects || onUserPlan);
  emptyUser.classList.toggle("hidden", hasObjects || !onUserPlan);
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

/** The text of the count of a clone: `1 clon`, `3 clones`. */
function cloneCountText(count) {
  return `${count} ${count === 1 ? "clon" : "clones"}`;
}

/** The dot of a family: the colour of whoever drew the diagram (D3). */
function sessionDot(family) {
  const dot = document.createElement("span");
  dot.className = `session-dot ${familyClass(family)}`;
  return dot;
}

/**
 * One row of the switcher: the colour of the family, the label of the
 * session, its project, whether it is `activa` or `en gracia` and how many
 * diagrams it has on the plan. Every piece of it comes from the state Rust
 * emits; the page invents none.
 */
function sessionRow(session) {
  const row = document.createElement("button");
  row.type = "button";
  row.className = "session-row";
  const chosen = isVisiblePlan(session.id);
  row.classList.toggle("selected", chosen);
  row.classList.toggle("in-grace", session.state === "in_grace");
  row.setAttribute("aria-pressed", chosen ? "true" : "false");
  row.title = session.label;
  row.addEventListener("click", () => showPlan({ id: session.id, label: session.label }));

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
 * Shows the objects of the plan on screen and hides the rest (A2; B1).
 *
 * Hiding is one class that takes the object out of the paint (`display: none`
 * in the stylesheet): the camera, the rectangle of the object and the place
 * Rust gave it are untouched, so coming back to a plan shows it exactly where
 * it was. Nothing is closed here: the diagrams of the other plans keep living
 * in Rust (D5) and the clones of the user plan keep their place (S4).
 */
function applyPlanVisibility() {
  for (const element of objects.values()) {
    element.classList.toggle("plan-hidden", !isVisiblePlan(element.planId || ""));
  }
}

/**
 * The plan of a session, as the switcher names it (A2). A session the state
 * does not list yet is still its own plan: a render can arrive before the
 * state that names it, and the page never invents another id.
 */
function planOf(sessionId) {
  const session = state.sessions.find((entry) => entry.id === sessionId);
  return { id: sessionId, label: session ? session.label : sessionId };
}

/**
 * The plan the state leaves on screen when the human has not chosen one (A2):
 * the first session that has diagrams, else the first session of the registry
 * order (a `BTreeMap`, never a `HashMap`), else no plan. A plan that already
 * has something on it beats an empty surface.
 */
function initialPlan() {
  const drawn = state.sessions.find((session) => sessionDiagramCount(session.id) > 0);
  return drawn || state.sessions[0] || null;
}

/**
 * Brings one plan on screen (A2, S1): the diagrams of its session and nobody
 * else's. The plan being left keeps the camera it had and the plan being
 * entered brings its own back, so the human never loses the view of a plan.
 */
function showPlan(plan) {
  sessionMessage = "";
  const same =
    plan === null ? activePlan === null : activePlan !== null && activePlan.id === plan.id;
  if (same) {
    return;
  }
  // A pan in flight belongs to the plan being left: it would keep writing the
  // camera it was travelling to and would land on the plan being entered, so
  // the switch stops it, the way a camera intent does (PRD §7.2, RF27).
  cancelPan();
  // The view of the plan being left stays in the page (A2): the camera of the
  // plan on screen is the only one Rust follows.
  if (activePlan !== null) {
    planCameras.set(activePlan.id, { tx: camera.tx, ty: camera.ty, zoom: camera.zoom });
  }
  activePlan = plan;
  applyPlanVisibility();
  restorePlanCamera();
  renderSessions();
  paintEmpty();
}

/**
 * The center of the newest object of a plan, in plan units (A2; B1). The store
 * lists the diagrams by `diagram_id` and the ids are monotonic and never
 * reused (PRD §11.1, RF28.1), so the last one of a plan is the one that
 * arrived last. The clones of the user plan arrive in the same order, so the
 * last clone is the newest object of that plan.
 */
function newestCenter(planId) {
  if (planId === USER_PLAN_ID) {
    const newest = state.clones[state.clones.length - 1];
    return newest === undefined ? null : objectCenter(newest);
  }
  const diagramsOfPlan = state.diagrams.filter((diagram) => diagram.session_id === planId);
  const newest = diagramsOfPlan[diagramsOfPlan.length - 1];
  return newest === undefined ? null : objectCenter(newest);
}

/**
 * Brings back the camera of the plan on screen (A2). Each plan keeps its own
 * view in the page; a plan nobody has visited yet has none, so the camera goes
 * to its newest object instead of landing on an empty stretch of the surface.
 */
function restorePlanCamera() {
  if (activePlan === null) {
    return;
  }
  const saved = planCameras.get(activePlan.id);
  if (saved) {
    camera.tx = saved.tx;
    camera.ty = saved.ty;
    camera.zoom = clampZoom(saved.zoom);
  } else {
    const center = newestCenter(activePlan.id);
    if (center !== null) {
      const target = cameraOn(center);
      camera.tx = target.tx;
      camera.ty = target.ty;
    }
  }
  // The drawings that left the page come back with the camera that is already
  // there: applying it also decides the culling (RNF1-ter).
  applyCamera();
  notifyCamera();
}

/**
 * The session of the plan on screen is gone: its grace ended and the owner
 * dropped it with its diagrams (D4, S2). The switcher says so and shows the
 * next plan, instead of leaving the surface empty with no explanation.
 */
function reconcileActivePlan() {
  // The plan of the human does not end with a session: its row is always in
  // the switcher, so nothing replaces it (S4).
  if (activePlan !== null && activePlan.id === USER_PLAN_ID) {
    return;
  }
  if (activePlan !== null && state.sessions.some((session) => session.id === activePlan.id)) {
    return;
  }
  const ended = activePlan;
  const next = initialPlan();
  showPlan(next === null ? null : { id: next.id, label: next.label });
  if (ended === null) {
    return;
  }
  sessionMessage =
    next === null
      ? `la sesión «${ended.label}» terminó; no queda ningún plano`
      : `la sesión «${ended.label}» terminó; muestro el plano de «${next.label}»`;
  renderSessions();
}

/**
 * The row of the plan of the human (B1; S4): «Mi plano», always first, with the
 * dot of the plan (nobody else draws there, so it is not a family), the number
 * of clones it holds and the plan on screen marked in it. Everything it says
 * comes from the state: the count is `state.clones` and the row exists even
 * with no session at all.
 */
function userPlanRow() {
  const row = document.createElement("button");
  row.type = "button";
  row.className = "session-row user-plan";
  const chosen = isVisiblePlan(USER_PLAN_ID);
  row.classList.toggle("selected", chosen);
  row.setAttribute("aria-pressed", chosen ? "true" : "false");
  row.title = "tu plano: los diagramas que clonaste para analizar";
  row.addEventListener("click", () => showPlan(USER_PLAN));

  const head = document.createElement("span");
  head.className = "session-head";
  const label = document.createElement("span");
  label.className = "session-label";
  label.textContent = USER_PLAN.label;
  const count = document.createElement("span");
  count.className = "session-count";
  count.textContent = String(state.clones.length);
  count.title = cloneCountText(state.clones.length);
  head.append(label, count);

  const meta = document.createElement("span");
  meta.className = "session-meta";
  const owner = document.createElement("span");
  owner.className = "session-family";
  owner.textContent = "plano del usuario";
  meta.append(owner);

  const dot = document.createElement("span");
  dot.className = "session-dot user-plan-dot";

  const body = document.createElement("span");
  body.className = "session-body";
  body.append(head, meta);
  row.append(dot, body);
  return row;
}

/**
 * Builds the switcher from the state: the plan of the human first and then one
 * row per session, in the order Rust lists them, with the plan on screen marked
 * as such (A2, D3; B1). There is no «Todas» row any more: the plan shows one
 * plan at a time. The notice explains why the plan on screen changed, when it
 * had to.
 */
function renderSessions() {
  sessionList.replaceChildren(userPlanRow(), ...state.sessions.map(sessionRow));
  sessionNotice.textContent = sessionMessage;
  sessionNotice.classList.toggle("hidden", sessionMessage === "");
}

// --- Plan of the human: the clones (`f8-planos.md`, B1/B2/B3) -------------

/**
 * One clone of the user plan (B1; S2, S4): an object of the surface that
 * belongs to the human.
 *
 * It reuses the machinery of a diagram object — the rectangle the culling
 * tests, the box that stands in for the drawing, the camera transform, the
 * selection — and it has none of the parts that belong to the agent: no `×`,
 * no `diagram_id`, and its place is written by the hand of the human (B3). The
 * drawing arrives from the owner with `graphium://clone-paint`.
 */
function createCloneObject(clone) {
  const container = document.createElement("div");
  container.className = "diagram clone";

  const content = document.createElement("div");
  content.className = "content";

  const box = document.createElement("div");
  box.className = "diagram-box hidden";
  const boxLabel = document.createElement("span");
  boxLabel.className = "diagram-box-label";
  box.appendChild(boxLabel);

  container.cloneId = clone.clone_id;
  // The plan the object belongs to, the key the switcher and the culling use
  // (A2; B1): a clone is an object of the user plan and of no other.
  container.planId = USER_PLAN_ID;
  container.content = content;
  container.box = box;
  container.boxLabel = boxLabel;
  container.caption = "";
  // The object is born without a drawing: its box names it until the paint of
  // the owner arrives (RNF1-ter).
  container.culled = true;
  container.rect = { x: 0, y: 0, w: 0, h: 0 };
  container.cloneState = clone;
  // The `×` lives INSIDE the toolbar pill (L8): one control anchored to the
  // corner of the object — the pill grows inward and wraps when the object is
  // narrow, so nothing sticks out of the box the human sees.
  container.cloneTools = createCloneTools(container);
  container.cloneTools.append(createCloneClose(container));
  content.append(container.cloneTools);
  container.append(box, content);
  return container;
}

/**
 * The `×` of a clone (L6 of `f8-planos.md`): the human removes the view
 * instance from their own plan. It is the same button of the plan — same
 * corner, same size, same tokens — and it never closes the diagram of the
 * session: closing that one belongs to the panel, and a clone is a view, not a
 * diagram of the store (S4).
 */
function createCloneClose(container) {
  const close = document.createElement("button");
  close.type = "button";
  close.className = "diagram-close clone-close";
  close.textContent = "×";
  close.setAttribute("aria-label", `cerrar el clon`);
  close.title = `cerrar el clon`;
  close.addEventListener("pointerdown", (event) => {
    // The plan starts the move of the clone on its own pointer down: the button
    // is not the object, so it does not drag it (B3).
    event.stopPropagation();
  });
  close.addEventListener("click", (event) => {
    event.stopPropagation();
    emitCloneRemove(container.cloneId);
  });
  return close;
}

/**
 * Asks the owner to remove a clone from the user plan (L6, defecto 2).
 *
 * The page does not delete the object by itself: the model lives in Rust and
 * the state that comes back is what takes the object off the plan, like every
 * other change of the plan (S8, S22).
 */
function emitCloneRemove(cloneId) {
  emit("graphium://clone-remove", { clone_id: cloneId });
}

/**
 * The tools of a clone (B2, B3): «Actualizar el clon» and the history of the
 * original in the «◀ v3/7 ▶» grammar of the panel item. They hang from the
 * corner of the object, like the `×` of a diagram, so the plan stays clean and
 * the human finds them on the object they belong to.
 */
function createCloneTools(container) {
  const tools = document.createElement("div");
  tools.className = "diagram-clone-tools";

  const versions = document.createElement("div");
  versions.className = "panel-item-versions clone-versions";

  const prev = document.createElement("button");
  prev.type = "button";
  prev.className = "panel-item-version-step";
  prev.textContent = "◀";
  prev.setAttribute("aria-label", "versión anterior del original");
  prev.addEventListener("click", () => cloneVersionStep(container.cloneState, -1));

  const label = document.createElement("span");
  label.className = "panel-item-version-label";

  const next = document.createElement("button");
  next.type = "button";
  next.className = "panel-item-version-step";
  next.textContent = "▶";
  next.setAttribute("aria-label", "versión siguiente del original");
  next.addEventListener("click", () => cloneVersionStep(container.cloneState, 1));

  versions.append(prev, label, next);

  const refresh = document.createElement("button");
  refresh.type = "button";
  refresh.className = "clone-refresh";
  refresh.textContent = "↻";
  refresh.setAttribute("aria-label", "actualizar el clon");
  refresh.title = "actualizar el clon";
  refresh.addEventListener("click", () => refreshClone(container.cloneState));

  // A button is not the object: pressing one never starts the drag of the
  // camera nor the move of the clone (B3).
  for (const button of [prev, next, refresh]) {
    button.addEventListener("pointerdown", (event) => event.stopPropagation());
  }

  container.cloneVersions = versions;
  container.clonePrev = prev;
  container.cloneLabel = label;
  container.cloneNext = next;
  container.cloneRefresh = refresh;

  tools.append(versions, refresh);
  return tools;
}

/**
 * Paints the tools of a clone from the state: the refresh is enabled only
 * while the original is on a plan (B2), the history walks the versions the
 * store keeps (`oldest_version..=current_version`, absolute and consecutive)
 * and reads «v3/7» like the panel item, and a clone whose original closed
 * keeps its photo and loses both.
 */
function updateCloneTools(container) {
  const clone = container.cloneState;
  if (!clone) {
    return;
  }
  const updatable = Boolean(clone.updatable);
  const oldest = Number(clone.oldest_version || 0);
  const current = Number(clone.current_version || oldest);
  const count = Number(clone.version_count || 0);
  const viewed = viewedCloneVersion.get(clone.clone_id);
  // Without a version in view the clone shows the photo: the newest version
  // it pulled, not the newest of the original.
  const shown = viewed === undefined ? Number(clone.snapshot_version || current) : viewed;
  // A version the history discarded cannot be browsed, because the store no
  // longer has it: the indicator falls back to the newest one the owner can
  // draw, like the panel item does (RF14, L5).
  const at = shown >= oldest && shown <= current ? shown : current;

  container.clonePrev.disabled = !updatable || at <= oldest;
  container.cloneNext.disabled = !updatable || at >= current;
  container.cloneLabel.textContent = count > 0 ? `v${at - oldest + 1}/${count}` : "";
  container.cloneVersions.classList.toggle("hidden", !(count > 1));
  container.cloneRefresh.disabled = !updatable;
  container.cloneRefresh.title = updatable
    ? "actualizar el clon"
    : "el original ya no está en el plan";
}

/**
 * Reads from the state where a clone sits and what it is called. Like a
 * diagram, the page invents neither: the place and the size are the ones the
 * owner keeps (B3) and the title is the one the clone froze.
 */
function readClone(element, clone) {
  element.cloneState = clone;
  if (typeof clone.title === "string" && clone.title !== "") {
    element.caption = clone.title;
  }
  if (clone.pos && clone.bbox) {
    element.rect = {
      x: Number(clone.pos.x),
      y: Number(clone.pos.y),
      w: Number(clone.bbox.w),
      h: Number(clone.bbox.h),
    };
    placeCloneContent(element);
  }
  updateCloneTools(element);
  paintBox(element);
}

/**
 * Puts the object of a clone where its rectangle says. The content **is** the
 * object — the drawing centers inside it with the stylesheet — so moving a
 * clone is moving one rectangle, and the drawing and its tools travel with it.
 */
function placeCloneContent(element) {
  const rect = element.rect;
  element.content.style.left = `${rect.x}px`;
  element.content.style.top = `${rect.y}px`;
  element.content.style.width = `${rect.w}px`;
  element.content.style.height = `${rect.h}px`;
}

/**
 * The object of a clone, created or kept in step with the state. Nothing else
 * creates one: the state of the owner and its paint are the only sources.
 */
function upsertClone(clone) {
  let element = objects.get(clone.clone_id);
  if (!element) {
    element = createCloneObject(clone);
    objects.set(clone.clone_id, element);
    diagrams.appendChild(element);
  }
  readClone(element, clone);
  return element;
}

/**
 * Mounts the drawing the owner rendered for a clone (B1, B2).
 *
 * It is a **new** drawing, not the one that left the page, so it is mounted
 * here and the culling decides afterwards whether it stays: the box leaves,
 * the object takes the node and every `applyCamera()` re-decides the
 * materialization like it does for a diagram (RNF1-ter).
 */
function mountCloneDrawing(element, payloadSvg) {
  const holder = document.createElement("div");
  holder.innerHTML = payloadSvg;
  const svg = holder.querySelector("svg");
  if (!svg) {
    return;
  }
  if (element.svg) {
    element.svg.remove();
  }
  element.svg = svg;
  element.content.prepend(svg);
  element.culled = false;
  element.box.classList.add("hidden");
}

/**
 * Paints a clone the owner rendered (B1, B2): the object takes the place the
 * owner keeps, the drawing mounts where it was, and a clone that just arrived
 * brings the user plan on screen — cloning is asking to analyze it (S2).
 */
function applyClonePaint(paint) {
  // Whether this is the first object of the user plan on the page is read
  // before it lands, the way a render decides its camera (A2).
  const first = framesOnInsert(USER_PLAN_ID);
  const element = upsertClone(paint);
  // A paint without a drawing is an answer with nothing new to draw: the clone
  // the human asked for already exists (L6) or its photo is the only drawing
  // left (S4), so the page keeps what it shows instead of blanking the object.
  if (paint.svg) {
    mountCloneDrawing(element, paint.svg);
    // The truth about what the clone shows is the owner's: showing its photo
    // ends the browsing, an older version is a preview of the history (B2, B3).
    const version = Number(paint.version || paint.snapshot_version);
    const snapshot = Number(paint.snapshot_version || version);
    if (version >= snapshot) {
      viewedCloneVersion.delete(paint.clone_id);
    } else {
      viewedCloneVersion.set(paint.clone_id, version);
    }
  }
  updateCloneTools(element);
  // The size of the plan is refreshed before the camera paints, so the culling
  // tests the new object against the window it really has.
  viewport = {
    w: plan.clientWidth || window.innerWidth,
    h: plan.clientHeight || window.innerHeight,
  };
  applyCamera();
  paintEmpty();
  // A version of the history never moves the camera; the answer to a clone
  // request always does: the human asked to see that clone (L6).
  if (!paint.bring) {
    return;
  }
  showPlan(USER_PLAN);
  // The camera travels until the clone the human asked for, keeping the zoom
  // they chose (PRD §7.2, RF27). The first object of the plan is framed by the
  // camera the plan already restored.
  const center = objectCenter(paint);
  if (!first && center !== null) {
    panTo(cameraOn(center));
  }
}

/**
 * «Actualizar el clon» (B2, S3): asks the owner for the current version of the
 * original. The page draws nothing by itself and moves nothing: the answer
 * brings the drawing and, when the photo grew, the new place on the plan.
 */
function refreshClone(clone) {
  if (!clone || !clone.updatable) {
    return;
  }
  viewedCloneVersion.delete(clone.clone_id);
  emit("graphium://clone-view", { clone_id: clone.clone_id });
}

/**
 * One step of the history of the original, from the version in view or from the
 * photo when the human was not browsing (B2, B3).
 */
function cloneVersionStep(clone, step) {
  if (!clone || !clone.updatable) {
    return;
  }
  const at = viewedCloneVersion.get(clone.clone_id) ?? clone.snapshot_version;
  viewCloneVersion(clone, at + step);
}

/**
 * Asks the owner for one version of the original inside the clone (B3, S4).
 *
 * Read-only, like the history of a panel item: Rust renders the version without
 * the hook, stores nothing and writes no version, and it recalculates the place
 * of the clone only when the version is newer than the photo — with the calm of
 * the user plan, where the sessions are not mixed (S1).
 */
function viewCloneVersion(clone, version) {
  const target = Math.min(
    clone.current_version,
    Math.max(clone.oldest_version, version),
  );
  if (target === clone.snapshot_version) {
    // The photo is the version in view: there is nothing to browse.
    viewedCloneVersion.delete(clone.clone_id);
  } else {
    viewedCloneVersion.set(clone.clone_id, target);
  }
  emit("graphium://clone-view", { clone_id: clone.clone_id, version: target });
  const element = objects.get(clone.clone_id);
  if (element) {
    updateCloneTools(element);
  }
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
  // A re-render of the same id (`opts.replace_id`, `update_diagram`)
  // replaces the object: the old container leaves the plan, or two
  // drawings would overlap under one id. With it leaves the preview the
  // human was viewing — the newest version is newer now.
  const previous = objects.get(payload.diagram_id);
  if (previous) {
    previous.remove();
    viewedVersion.delete(payload.diagram_id);
  }
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

  // The render names the plan it belongs to (A2 of `f8-planos.md`): the
  // session comes from Rust, and that plan comes on screen, so the human
  // always sees what the agent just drew. The other plans keep their place
  // and their camera; nothing of them is closed.
  const sessionId = typeof payload.session_id === "string" ? payload.session_id : "";
  if (sessionId !== "") {
    showPlan(planOf(sessionId));
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
  // The plan the object belongs to (A2; B1): the key the switcher and the
  // culling use, the same one the state carries for every diagram. A clone
  // carries the user plan here; a diagram, the session that drew it.
  container.planId = sessionId;
  container.svg = svg;
  container.content = content;
  container.box = box;
  container.boxLabel = boxLabel;
  container.caption = "";
  container.culled = false;
  paintBox(container);

  // F2: a new diagram **sums** to the plan, it does not replace the one
  // that was there (PRD §11.1, S9). The plan is a canvas with room for
  // several objects, each one where Rust placed it.
  //
  // The first diagram of **its** plan frames the view; from the second one of
  // that plan on the camera pans until the new object and the zoom of the
  // human is never touched (PRD §7.2, RF27; A2). The answer is read before
  // the new object lands, or the first one would count as the second.
  const first = framesOnInsert(sessionId);
  objects.set(payload.diagram_id, container);
  diagrams.appendChild(container);
  // The size of the plan is refreshed before the camera paints, so the
  // culling tests the new object against the window it really has.
  viewport = {
    w: plan.clientWidth || window.innerWidth,
    h: plan.clientHeight || window.innerHeight,
  };
  applyCamera();
  paintEmpty();

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
 * Whether this object is the first of **its** plan (A2: one plan per session,
 * every plan has its own camera, and the user plan has its clones; B1).
 *
 * Only the first object of a plan is framed with the camera of the event; from
 * the second one of that plan on, the camera pans until the new object and
 * keeps the zoom (PRD §7.2, RF27).
 */
function framesOnInsert(planId) {
  for (const element of objects.values()) {
    if ((element.planId || "") === planId) {
      return false;
    }
  }
  return true;
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
  // A clone whose drawing has not arrived yet has no node to take out: the
  // box already stands for it.
  if (element.svg) {
    element.svg.remove();
  }
  element.box.classList.remove("hidden");
}

/** Brings the drawing of an object back into the DOM (RNF1-ter). */
function materializeObject(element) {
  if (!element.culled) {
    return;
  }
  // An object whose drawing has not arrived yet (a clone the page asked the
  // owner for) keeps its box: there is nothing to mount yet (RNF1-ter).
  if (!element.previewSvg && !element.svg) {
    return;
  }
  element.culled = false;
  // The version the human is viewing is the drawing of the object while it
  // lasts (L5): coming back into the view mounts the preview, not the live
  // drawing under it. The `×` of the object is the last child of
  // `.content`: the drawing goes before it, the way `insertDiagram` left it.
  if (element.previewSvg) {
    element.content.prepend(element.previewSvg);
  } else if (element.svg) {
    element.content.prepend(element.svg);
  }
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
    // An object of a plan the human is not looking at keeps no drawing in the
    // page: the three live SVGs are for the plan on screen (A2, RNF1-ter).
    if (element.classList.contains("plan-hidden")) {
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
  // Which plan this object belongs to: the key the switcher and the culling
  // use (S1, D3; A2, B1). For a diagram it is the session that drew it, the
  // same id Rust lists in `sessions`.
  if (typeof diagram.session_id === "string" && diagram.session_id !== "") {
    element.planId = diagram.session_id;
  }
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
 * The ids of every object the state lists: the diagrams of the sessions and the
 * clones of the user plan (B1). The two kinds never collide — the store numbers
 * `d-…`, the owner numbers `c-…` — and both live in the same map, so the
 * camera, the culling and the selection work for the two of them.
 */
function objectIds() {
  const open = new Set(state.diagrams.map((diagram) => diagram.diagram_id));
  for (const clone of state.clones) {
    open.add(clone.clone_id);
  }
  return open;
}

/**
 * Drops from the plan the objects Rust no longer lists (PRD §7 RF28; B1): the
 * state is the truth, so a closed diagram leaves the plan when the state says
 * so and not when the page feels like it. The clones of the user plan follow
 * the same rule: the owner is the one that knows which ones exist.
 */
function syncObjects() {
  const open = objectIds();
  for (const [objectId, element] of objects) {
    if (open.has(objectId)) {
      continue;
    }
    element.remove();
    objects.delete(objectId);
    viewedCloneVersion.delete(objectId);
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
  // The clones of the user plan (B1): the same reconciliation, from the same
  // state. The SVG does not travel in it, so a clone the page never drew — a
  // window that came back to life has the state and no drawings — asks the
  // owner for the version the clone keeps. A photo whose original closed has
  // nothing to ask: the owner no longer holds its versions, so the object keeps
  // its box instead of asking for a drawing nobody can render (S4).
  for (const clone of state.clones) {
    const element = upsertClone(clone);
    if (!element.svg && clone.updatable) {
      emit("graphium://clone-view", {
        clone_id: clone.clone_id,
        version: clone.snapshot_version,
      });
    }
  }
  // The plan on screen paints its own objects and hides the rest (A2),
  // before the culling decides which drawings stay in the page.
  applyPlanVisibility();
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

listen("graphium://version-paint", (event) => {
  const payload = event.payload || {};
  const element = objects.get(payload.diagram_id);
  if (!element) {
    // The diagram closed while the render of the preview ran: there is
    // nothing to paint on.
    return;
  }
  // Only the version the human still asked for paints: walking fast
  // through the history must not leave an older answer under a newer one.
  const viewed = viewedVersion.get(payload.diagram_id) ?? null;
  if (viewed === null || viewed !== Number(payload.version)) {
    return;
  }
  const holder = document.createElement("div");
  holder.innerHTML = payload.svg;
  const svg = holder.querySelector("svg");
  if (!svg) {
    return;
  }
  paintPreview(element, svg);
});

/**
 * The drawing of a clone (B1, B2): the owner renders it read-only and the page
 * mounts it where the object is. Nothing of the store is written here — the
 * clone is a view instance and the answer carries the version it shows.
 */
listen("graphium://clone-paint", (event) => {
  applyClonePaint(event.payload || {});
});

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
  // The clones of the user plan (B1): view instances of the owner, never
  // diagrams of the store.
  state.clones = Array.isArray(payload.clones) ? payload.clones : [];
  // A session the owner dropped takes its plan away: the switcher says so and
  // shows another plan, never an empty surface with no explanation (S2, A2).
  reconcileActivePlan();
  syncObjects();
  renderPanel();
  renderSessions();
  paintEmpty();
});
listen("graphium://pending", (event) => {
  showPending(event.payload || {});
});
listen("graphium://error", (event) => {
  hidePending();
  showError(event.payload || {});
});
/**
 * ←/→ walk the history of the selected diagram, like the arrows of its
 * panel item. A field that holds the keyboard (the settings panel) keeps
 * its arrows: the history never steals them from the human.
 */
window.addEventListener("keydown", (event) => {
  if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") {
    return;
  }
  const target = event.target;
  if (target instanceof HTMLElement && target.matches("input, textarea, select")) {
    return;
  }
  if (selectedId === null) {
    return;
  }
  // A clone walks the history of its original with the same arrows (B3): the
  // preview is read-only and the owner recalculates the place when the version
  // is newer than the photo.
  const clone = state.clones.find((entry) => entry.clone_id === selectedId);
  if (clone) {
    if (!(clone.version_count > 1) || !clone.updatable) {
      return;
    }
    const at = viewedCloneVersion.get(selectedId) ?? clone.snapshot_version;
    viewCloneVersion(clone, event.key === "ArrowLeft" ? at - 1 : at + 1);
    event.preventDefault();
    return;
  }
  const diagram = state.diagrams.find((entry) => entry.diagram_id === selectedId);
  if (!diagram || !(diagram.version_count > 1)) {
    return;
  }
  const at = viewedVersion.get(selectedId) ?? diagram.current_version;
  viewVersion(diagram, event.key === "ArrowLeft" ? at - 1 : at + 1);
  event.preventDefault();
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
