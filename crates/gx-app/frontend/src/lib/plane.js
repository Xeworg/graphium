// The pure math of the plane: camera arithmetic and culling geometry, ported
// from the vanilla page (`crates/gx-app/frontend/plan.js`). Every function
// here takes plain values and returns plain values — no DOM, no state — so
// the component (`src/App.svelte`) keeps the orchestration and this file
// keeps the numbers the parity contract pins (CONTRACT.md §6).
//
//   camera:     `plan.js:333-637` (`clampZoom`, `zoomAt`, `cameraOn`)
//   culling:    `plan.js:2064-2145` (`visibleRect`, `rectsIntersect`,
//               `distanceToView`, `objectCenter`)

import { ZOOM_MAX, ZOOM_MIN, CULL_ZOOM, MAX_LIVE_SVGS } from "./contract.js";

/** The clamp of the wheel (plan.js:334-336): 0.05 … 4.0. */
export function clampZoom(zoom) {
  return Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, zoom));
}

/**
 * The camera that keeps the world point under the cursor fixed
 * (plan.js:389-396, `zoomAt`): the cursor is the anchor, the zoom multiplies
 * and the translation absorbs the difference.
 */
export function zoomAtPoint(camera, screenX, screenY, factor) {
  const next = clampZoom(camera.zoom * factor);
  const applied = next / camera.zoom;
  return {
    zoom: next,
    tx: screenX - (screenX - camera.tx) * applied,
    ty: screenY - (screenY - camera.ty) * applied,
  };
}

/**
 * The camera that puts `center` (a point of the plan) in the middle of the
 * view, keeping the zoom the human chose (plan.js:592-598, `cameraOn`).
 */
export function cameraOn(camera, viewport, center) {
  return {
    tx: viewport.w / 2 - center.x * camera.zoom,
    ty: viewport.h / 2 - center.y * camera.zoom,
  };
}

/**
 * The rectangle of the plan the camera shows, in plan units
 * (plan.js:2076-2088, `visibleRect`): the division by the zoom turns the
 * screen-sized window into plan units.
 */
export function visibleRect(camera, viewport) {
  return {
    x: -camera.tx / camera.zoom,
    y: -camera.ty / camera.zoom,
    w: viewport.w / camera.zoom,
    h: viewport.h / camera.zoom,
  };
}

/** Whether two rectangles of the plan share at least one point. */
export function rectsIntersect(a, b) {
  return a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
}

/** How far the centre of `rect` is from the centre of the view, squared. */
export function distanceToView(rect, view) {
  const dx = rect.x + rect.w / 2 - (view.x + view.w / 2);
  const dy = rect.y + rect.h / 2 - (view.y + view.h / 2);
  return dx * dx + dy * dy;
}

/**
 * The center of the object a state entry names, in plan units
 * (plan.js:969-978): `bbox` is already the object (the SVG plus its padding),
 * so the center is `pos + bbox / 2`. A diagram without a place has no place
 * to go yet — the page never invents coordinates for it.
 */
export function objectCenter(obj) {
  const pos = obj && obj.pos;
  const bbox = obj && obj.bbox;
  if (!pos || !bbox) {
    return null;
  }
  return { x: pos.x + bbox.w / 2, y: pos.y + bbox.h / 2 };
}

/**
 * The pure decision of the culling (plan.js:2064-2145, RNF1-ter, C14): which
 * object ids keep their drawing in the DOM. Hidden objects, everything out of
 * the view, below `CULL_ZOOM`, and — of the survivors, sorted by distance to
 * the centre of the view — everything past `MAX_LIVE_SVGS`, is out. It takes
 * plain values and returns plain values — no DOM, no state — so the funnel
 * that applies it (`applyCulling` in `App.svelte`) can run on every camera
 * write without entering reactivity.
 *
 * An object whose drawing has not arrived yet (a clone waiting for its paint)
 * still comes out alive here: the funnel keeps its box, because there is
 * nothing to mount yet.
 */
export function cullDecide(plane, camera, viewport) {
  const view = visibleRect(camera, viewport);
  const zoomedOut = camera.zoom < CULL_ZOOM;
  const candidates = [];
  for (const rec of plane) {
    if (rec.hidden) {
      continue;
    }
    if (zoomedOut || !rectsIntersect(rec.rect, view)) {
      continue;
    }
    candidates.push({ rec, distance: distanceToView(rec.rect, view) });
  }
  candidates.sort((left, right) => left.distance - right.distance);
  const alive = new Set();
  for (
    let index = 0;
    index < candidates.length && index < MAX_LIVE_SVGS;
    index += 1
  ) {
    alive.add(candidates[index].rec.id);
  }
  return alive;
}
