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

const camera = { tx: 0, ty: 0, zoom: 1 };
const objects = new Map(); // diagram_id -> element
const state = { diagrams: [] }; // the state Rust owns (PRD §8.6.1)
let cameraTimer = null;
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

function applyCamera() {
  plan.style.setProperty("--tx", `${camera.tx}px`);
  plan.style.setProperty("--ty", `${camera.ty}px`);

  // The grid is parameterized by the deviation of its lines from the
  // vertical (RF24, RF25): sin and cos travel to the CSS so the angle can
  // change without touching the stylesheet.
  const radians = (GRID_ANGLE_DEGREES * Math.PI) / 180;
  const sine = Math.sin(radians);
  plan.style.setProperty("--grid-sin", sine.toFixed(4));
  plan.style.setProperty("--grid-cos", Math.cos(radians).toFixed(4));
  // The perpendicular step follows the diagonal and the angle.
  plan.style.setProperty(
    "--grid-step",
    `${(GRID_DIAGONAL_PLAN * sine * camera.zoom).toFixed(2)}px`,
  );
  // The tile of the mesh scales with the camera, so it keeps being a
  // period of the pattern at any zoom.
  plan.style.setProperty("--zoom", String(camera.zoom));

  plan.style.setProperty("--grid-opacity", camera.zoom < GRID_HIDE_ZOOM ? "0" : "1");

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

applyCamera();
emit("graphium://ready", {});
