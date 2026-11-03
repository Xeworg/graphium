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

// The keys of `Settings` in Rust (`crates/gx-app/src/settings.rs`), in the
// order the page sends them back. The panel reads and writes exactly these:
// the page never invents a setting and never decides a default.
const SETTINGS_KEYS = [
  "background",
  "grid_color",
  "grid_opacity",
  "grid_diagonal",
  "grid_angle_degrees",
  "grid_visible",
  "theme",
];

// The three presets of F1 (PRD §8.5.2). Rust owns them
// (`settings::presets`): this table only fills the controls, and the contract
// test in `crates/gx-app/tests/settings_panel.rs` keeps both sides equal.
const PLAN_PRESETS = {
  "plano": {
    "background": "#1b1b1b",
    "grid_color": "#ffffff",
    "grid_opacity": 0.14,
    "grid_diagonal": 59,
    "grid_angle_degrees": 45,
    "grid_visible": true,
    "theme": "dark"
  },
  "claro": {
    "background": "#f4f4f5",
    "grid_color": "#1b1b1b",
    "grid_opacity": 0.1,
    "grid_diagonal": 59,
    "grid_angle_degrees": 45,
    "grid_visible": true,
    "theme": "light"
  },
  "sin grilla": {
    "background": "#1b1b1b",
    "grid_color": "#ffffff",
    "grid_opacity": 0.14,
    "grid_diagonal": 59,
    "grid_angle_degrees": 45,
    "grid_visible": false,
    "theme": "dark"
  }
};

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
const gridOpacityValue = document.getElementById("settings-grid-opacity-value");
const gridDiagonalValue = document.getElementById("settings-grid-diagonal-value");
const gridAngleValue = document.getElementById("settings-grid-angle-value");
const themeInputs = Array.from(
  document.querySelectorAll('input[name="settings-theme"]'),
);
const presetButtons = Array.from(document.querySelectorAll("[data-preset]"));

const camera = { tx: 0, ty: 0, zoom: 1 };
const objects = new Map(); // diagram_id -> element
const state = { diagrams: [] }; // the state Rust owns (PRD §8.6.1)
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
    zoomAt(event.clientX, event.clientY, event.deltaY < 0 ? ZOOM_STEP : 1 / ZOOM_STEP);
  },
  { passive: false },
);

let dragging = null;

plan.addEventListener("pointerdown", (event) => {
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
  camera.tx = Number(intent.tx || 0);
  camera.ty = Number(intent.ty || 0);
  camera.zoom = clampZoom(Number(intent.zoom || 1));
  applyCamera();
  notifyCamera();
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
    background: String(p.background),
    grid_color: String(p.grid_color),
    grid_opacity: Number(p.grid_opacity),
    grid_diagonal: Number(p.grid_diagonal),
    grid_angle_degrees: Number(p.grid_angle_degrees),
    grid_visible: Boolean(p.grid_visible),
    theme: String(p.theme),
  };
}

/** The settings the controls show right now. */
function readPanel() {
  const theme = themeInputs.find((input) => input.checked) || themeInputs[0];
  return {
    background: backgroundInput.value,
    grid_color: gridColorInput.value,
    grid_opacity: Number(gridOpacityInput.value),
    grid_diagonal: Number(gridDiagonalInput.value),
    grid_angle_degrees: Number(gridAngleInput.value),
    grid_visible: gridVisibleInput.checked,
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
  for (const input of themeInputs) {
    input.checked = input.value === next.theme;
  }
  showValues(next);
}

/**
 * Paints the plan with the settings: the background, the grid and the theme.
 *
 * The theme is a real theme (RF22, S18), not a background: `data-theme` on
 * the root element switches the whole palette of the stylesheet, so nodes,
 * labels and arrows keep their contrast.
 */
function applySettings(next) {
  settings = next;
  const root = document.documentElement;
  root.dataset.theme = next.theme === "light" ? "light" : "dark";
  root.style.setProperty("--bg", next.background);
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
  ...themeInputs,
]) {
  input.addEventListener("input", onPanelInput);
  input.addEventListener("change", onPanelInput);
}

for (const button of presetButtons) {
  button.addEventListener("click", () => {
    const preset = PLAN_PRESETS[button.dataset.preset];
    if (!preset) {
      return;
    }
    // A preset fills the controls and the change travels to Rust like any
    // other one: the page never writes the file.
    const next = settingsFromPayload(preset);
    fillPanel(next);
    applySettings(next);
    notifySettings(next);
  });
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
  // without that intent it lands on the center of the view.
  const placement = payload.placement;
  const center = placement
    ? { x: placement.x + width / 2, y: placement.y + height / 2 }
    : worldCenter();

  content.style.left = `${center.x - width / 2}px`;
  content.style.top = `${center.y - height / 2}px`;
  content.appendChild(svg);
  container.appendChild(content);

  // Capacity 1 for now: a new diagram replaces the previous one
  // (PRD §11.1, F1 capacity 1).
  for (const [id, element] of objects) {
    if (id !== payload.diagram_id) {
      element.remove();
      objects.delete(id);
    }
  }

  objects.set(payload.diagram_id, container);
  diagrams.appendChild(container);
  applyCamera();
  // The view fits the new diagram (PRD §7.2, S17): a diagram that does not
  // fit is cut by the window, and a cut diagram cannot be read.
  applyCameraIntent(payload.camera);
  viewport = {
    w: plan.clientWidth || window.innerWidth,
    h: plan.clientHeight || window.innerHeight,
  };
  empty.classList.add("hidden");

  // The frontend answers after two animation frames: the SVG is painted
  // (PRD §8.6.1, step 4).
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      emit("render_ack", { req_id: payload.req_id, ok: true });
    });
  });
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
  // reads the state it receives.
  const diagrams = Array.isArray(event.payload) ? event.payload : [];
  state.diagrams = diagrams;
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
