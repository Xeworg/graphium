// The grid of the plan: the geometry of PRD §7.1 RF24/RF25 and decision S23,
// ported from `plan.js:341-383` (`GRID_FALLBACK`, `gridSettings`, `applyGrid`).
//
// It is the only piece of the plane this task (T1) owns; the camera, the
// objects, the culling and the panels are T2/T3 and stay in the vanilla page
// until then. The formulas are the vanilla ones, untouched:
//
//   perpendicular step = diagonal * sin(angle) * zoom
//   tile               = (diagonal * tan(angle) * zoom) × (diagonal * zoom)
//
// `tests/grid.rs` pins the numbers and the stylesheet declarations that carry
// them; `src/app.css` declares the same `--grid-*`/`--shift-*` tokens.

import {
  GRID_ANGLE_DEGREES,
  GRID_DIAGONAL_PLAN,
  GRID_HIDE_ZOOM,
  SETTINGS_KEYS,
  THEMES,
} from "./contract.js";

/**
 * The plan paints this before the first `graphium://settings` arrives
 * (plan.js:341-347): the same numbers the stylesheet declares.
 */
export const GRID_FALLBACK = {
  grid_color: "rgba(255, 255, 255, 0.14)",
  grid_opacity: 1,
  grid_diagonal: GRID_DIAGONAL_PLAN,
  grid_angle_degrees: GRID_ANGLE_DEGREES,
  grid_visible: true,
};

/** The settings the grid paints from (plan.js:349-351). */
export function gridSettings(settings) {
  return settings === null || settings === undefined ? GRID_FALLBACK : settings;
}

/**
 * Reads the object Rust sends, key by key (plan.js:647-665).
 *
 * The keys are the contract: they are exactly the fields of `Settings` in
 * `crates/gx-app/src/settings.rs`, and the page never reads anything else.
 */
export function settingsFromPayload(payload) {
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

/** The theme the stylesheet answers to; an unknown one reads as `dark`. */
export function themeOf(settings) {
  return settings && THEMES.includes(settings.theme) ? settings.theme : "dark";
}

/**
 * A stable key for the values of a settings object (plan.js:744-746): it is
 * what keeps `settings-changed` from writing the same file twice.
 */
export function settingsId(next) {
  return SETTINGS_KEYS.map((key) => `${key}=${next[key]}`).join("|");
}

/**
 * The CSS custom properties of the grid (plan.js:362-383).
 *
 * The thickness stays one screen pixel because it is a length of the
 * gradient, not a scaled one; the phase of each family is the pan projected
 * on its own normal, and a single offset would break the lattice.
 *
 * It is published in two halves (T1 of `odd/tasks/mejoras-frontend.md`), which
 * is what keeps a pan frame from rewriting what only the settings decide:
 * `gridPaintStatic` holds the six variables that are a function of the settings
 * alone and `gridPaintCamera` the five that move with the camera. The union of
 * the two is the object the single `gridPaint` returned, value for value.
 */
export function gridPaintStatic(settings) {
  const grid = gridSettings(settings);
  const radians = (grid.grid_angle_degrees * Math.PI) / 180;

  return {
    "--grid-angle": `${grid.grid_angle_degrees}deg`,
    "--grid-sin": Math.sin(radians).toFixed(4),
    "--grid-cos": Math.cos(radians).toFixed(4),
    "--grid-tan": Math.tan(radians).toFixed(4),
    "--grid-diagonal": `${grid.grid_diagonal}px`,
    "--grid-color": grid.grid_color,
  };
}

/**
 * The variables of the grid that move with the camera, plus the camera itself
 * (`plan.js:362-383`). The step and the opacity read the settings too — the
 * diagonal and the angle scale the step, `grid_visible` and `grid_opacity`
 * fade the grid — but they are written with the camera, because the zoom is
 * what changes them on a pan or on a wheel, frame after frame.
 */
export function gridPaintCamera(settings, camera) {
  const grid = gridSettings(settings);
  const radians = (grid.grid_angle_degrees * Math.PI) / 180;
  const visible = grid.grid_visible && camera.zoom >= GRID_HIDE_ZOOM;

  return {
    "--grid-step": `${(grid.grid_diagonal * Math.sin(radians) * camera.zoom).toFixed(2)}px`,
    "--grid-opacity": visible ? String(grid.grid_opacity) : "0",
    "--tx": `${camera.tx}px`,
    "--ty": `${camera.ty}px`,
    "--zoom": String(camera.zoom),
  };
}

/**
 * The custom properties of the palette (plan.js:729-742, `applySettings`).
 *
 * The theme is a real theme, not a background: `data-theme` on the root
 * switches the whole palette of the stylesheet, and the colours the human
 * changed are painted over it as variables.
 */
export function palettePaint(settings) {
  const next = settings;
  if (!next) {
    return {};
  }
  return {
    "--bg": next.background,
    "--node-fill": next.node_fill,
    "--node-stroke": next.node_stroke,
    "--node-text": next.node_text,
    "--edge-stroke": next.edge_stroke,
    "--edge-text": next.edge_text,
    "--accent": next.accent,
  };
}
