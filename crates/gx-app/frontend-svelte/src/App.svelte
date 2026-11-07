<script>
  // The plan, as a Svelte component — T2 of `odd/tasks/svelte-ui.md` (S2).
  //
  // What this component owns now: the camera (wheel zoom anchored at the
  // cursor, drag pan, the animated pan, the camera intents of the renders),
  // the objects with their `×`, their box and the culling (≤ 3 live SVGs),
  // the wholesale reconciliation from `graphium://state`, and the clones of
  // the user plan as plane objects. What it does NOT own yet (T3): the
  // panels, the version history, the tools pill of the clones (`×`, `↻`,
  // history), the clone move by drag (C11 marks it T3), the settings panel
  // and the switcher UI. `CONTRACT.md` is the parity checklist T4 walks.
  import { onMount } from "svelte";

  import {
    CAMERA_THROTTLE_MS,
    CULL_ZOOM,
    EVENTS,
    MAX_LIVE_SVGS,
    OBJECT_PADDING,
    PAN_MS,
    USER_PLAN,
    ZOOM_STEP,
  } from "./lib/contract.js";
  import { emit, hasBridge, listen } from "./lib/tauri.js";
  import {
    gridPaint,
    palettePaint,
    settingsFromPayload,
    themeOf,
  } from "./lib/grid.js";
  import {
    cameraOn,
    clampZoom,
    distanceToView,
    objectCenter,
    rectsIntersect,
    visibleRect,
    zoomAtPoint,
  } from "./lib/plane.js";

  // --- page state (plan.js:270-290) ---------------------------------------

  // The camera lives in the page (PRD §7.2): Rust keeps only a copy.
  let camera = $state({ tx: 0, ty: 0, zoom: 1 });
  // The settings of the human. `null` until the first `graphium://settings`.
  let settings = $state(null);
  // The state Rust owns (PRD §8.6.1): the truth about what the plan shows.
  let state = $state({ diagrams: [], sessions: [], clones: [] });
  // The objects of the plane, in arrival order — the same order the vanilla
  // `objects` Map iterates (plan.js:275). Diagrams and clones share it; the
  // ids never collide (the store numbers `d-…`, the owner numbers `c-…`).
  let plane = $state([]);
  // State 1 (`index.html:67-74`): only a session plan without objects shows
  // it; the user plan has its own empty state, which is T3 (`paintEmpty`).
  let showEmpty = $state(true);
  // The class the surface gets while the hand drags it (`plan.dragging`).
  let draggingNow = $state(false);

  // --- machinery outside reactivity ----------------------------------------

  let planEl = $state(null);
  let diagramsEl = $state(null);
  // The size of the plan in CSS pixels, before the current resize
  // (plan.js:287-290). Refreshed before every culling decision that follows
  // an insertion or a paint.
  let viewport = { w: 0, h: 0 };
  let cameraTimer = null;
  // The pan in flight, if there is one (plan.js:605-623).
  let pan = null;
  // The drag of the camera the hand started (plan.js:416-430).
  let dragging = null;
  // The diagram the human last chose — only bookkeeping until the panel and
  // the clone move arrive (T3): nothing paints it yet (plan.js:839).
  let selectedId = null;
  // The plan on screen (A2 of `f8-planos.md`): one at a time, each with its
  // own camera (plan.js:1171-1176).
  let activePlan = null;
  const planCameras = new Map();
  // The version the human is viewing of a diagram / clone (T3 owns the
  // history UI; T2 keeps the maps so a replaced object leaves no stale mark).
  const viewedVersion = new Map();
  const viewedCloneVersion = new Map();
  // Why the plan on screen changed, when it had to (rendered by T3).
  let sessionMessage = "";

  const grid = $derived(gridPaint(settings, camera));
  const palette = $derived(palettePaint(settings));
  // The camera transform of the layer. The vanilla page applies the same
  // transform to every object (`plan.js:393-396`); one transform on the
  // camera layer is the same result for one write.
  const layerTransform = $derived(
    `translate(${camera.tx}px, ${camera.ty}px) scale(${camera.zoom})`,
  );
  // Scaffold diagnostic only (CONTRACT.md §9): how many drawings are live.
  const liveSvgs = $derived(plane.filter((rec) => !rec.culled).length);

  // The grid and the palette, published to the stylesheets exactly like
  // `applyGrid`/`applySettings` do (`plan.js:362-383`, `plan.js:729-742`).
  $effect(() => {
    if (!planEl) return;
    for (const [name, value] of Object.entries(grid)) {
      planEl.style.setProperty(name, value);
    }
  });
  $effect(() => {
    const root = document.documentElement;
    root.dataset.theme = themeOf(settings);
    for (const [name, value] of Object.entries(palette)) {
      root.style.setProperty(name, value);
    }
  });
  $effect(() => {
    if (!diagramsEl) return;
    diagramsEl.style.transform = layerTransform;
  });

  // --- records --------------------------------------------------------------

  function newRecord(id, kind, planId) {
    return {
      id,
      kind,
      // The plan the object belongs to (A2; B1): a clone carries the user
      // plan; a diagram, the session that drew it.
      planId,
      caption: "",
      // The rectangle of the object in plan units (PRD §7.2: the SVG plus
      // its 24 px padding). The culling tests it and the box paints it;
      // it never asks the DOM for a layout.
      rect: { x: 0, y: 0, w: 0, h: 0 },
      // Where the content sits. A diagram: the box of its drawing. A clone:
      // the content **is** the object (the owner's rectangle).
      content: { x: 0, y: 0, w: 0, h: 0 },
      // Whether the drawing is out of the DOM (RNF1-ter). Born culled.
      culled: true,
      hidden: false,
      svg: null,
      previewSvg: null,
    };
  }

  function findRecord(id) {
    return plane.find((rec) => rec.id === id) ?? null;
  }

  function removeRecord(id) {
    const index = plane.findIndex((rec) => rec.id === id);
    if (index !== -1) {
      plane.splice(index, 1);
    }
  }

  // --- camera (plan.js:333-637) ---------------------------------------------

  function refreshViewport() {
    viewport = {
      w: (planEl && planEl.clientWidth) || window.innerWidth,
      h: (planEl && planEl.clientHeight) || window.innerHeight,
    };
  }

  function worldCenter() {
    return {
      x: (viewport.w / 2 - camera.tx) / camera.zoom,
      y: (viewport.h / 2 - camera.ty) / camera.zoom,
    };
  }

  /**
   * The camera copy for Rust: throttled to one event per
   * `CAMERA_THROTTLE_MS` (plan.js:404-412, C5).
   */
  function notifyCamera() {
    if (cameraTimer !== null) {
      return;
    }
    cameraTimer = window.setTimeout(() => {
      cameraTimer = null;
      emit(EVENTS.cameraChanged, { ...camera });
    }, CAMERA_THROTTLE_MS);
  }

  /** Stops the pan in flight: the human took the plan with the hand. */
  function cancelPan() {
    if (pan === null) {
      return;
    }
    window.cancelAnimationFrame(pan.frame);
    pan = null;
  }

  /**
   * Pans until `target` over `PAN_MS` (250 ms), cubic ease-out, and never
   * touches the zoom (plan.js:600-623, C12).
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
      applyCulling();
      notifyCamera();
      pan = elapsed < 1 ? { frame: window.requestAnimationFrame(step) } : null;
    };
    pan = { frame: window.requestAnimationFrame(step) };
  }

  /**
   * A camera intent coming from Rust (PRD §7.2): it wins over a pan in
   * flight and never drifts (plan.js:564-578, C12).
   */
  function applyCameraIntent(intent) {
    if (!intent) {
      return;
    }
    cancelPan();
    camera.tx = Number(intent.tx || 0);
    camera.ty = Number(intent.ty || 0);
    camera.zoom = clampZoom(Number(intent.zoom || 1));
    applyCulling();
    notifyCamera();
  }

  /** One step of the wheel, anchored at the cursor (plan.js:389-396, C10). */
  function zoomAt(screenX, screenY, factor) {
    const next = zoomAtPoint(camera, screenX, screenY, factor);
    camera.tx = next.tx;
    camera.ty = next.ty;
    camera.zoom = next.zoom;
    applyCulling();
    notifyCamera();
  }

  // --- pointer handlers (plan.js:416-520; the clone move is T3) -------------

  function onWheel(event) {
    event.preventDefault();
    // The human taking the wheel is the new owner of the view: a pan in
    // flight stops where it is instead of fighting the zoom.
    cancelPan();
    zoomAt(event.clientX, event.clientY, event.deltaY < 0 ? ZOOM_STEP : 1 / ZOOM_STEP);
  }

  function onPointerDown(event) {
    cancelPan();
    // A button (the `×` of an object) does its own thing: it never starts a
    // drag of the camera (plan.js:432-435).
    if (event.target instanceof Element && event.target.closest("button")) {
      return;
    }
    // T3 puts the move of a clone here (`cloneUnder`/`moveCloneStart`,
    // plan.js:441-502); until then the background pans, as it always did.
    dragging = { x: event.clientX, y: event.clientY, tx: camera.tx, ty: camera.ty };
    draggingNow = true;
    planEl.setPointerCapture(event.pointerId);
  }

  function onPointerMove(event) {
    if (!dragging) {
      return;
    }
    camera.tx = dragging.tx + (event.clientX - dragging.x);
    camera.ty = dragging.ty + (event.clientY - dragging.y);
    applyCulling();
    notifyCamera();
  }

  function endDrag(event) {
    if (!dragging) {
      return;
    }
    dragging = null;
    draggingNow = false;
    if (planEl.hasPointerCapture(event.pointerId)) {
      planEl.releasePointerCapture(event.pointerId);
    }
  }

  /** A resize keeps the world point under the centre; it never refits. */
  function onResize() {
    const next = {
      w: (planEl && planEl.clientWidth) || window.innerWidth,
      h: (planEl && planEl.clientHeight) || window.innerHeight,
    };
    camera.tx += (next.w - viewport.w) / 2;
    camera.ty += (next.h - viewport.h) / 2;
    viewport = next;
    applyCulling();
    notifyCamera();
  }

  // --- culling (plan.js:2064-2195) -------------------------------------------

  /**
   * Decides, object by object, whether its SVG stays in the DOM (RNF1-ter,
   * C14): at most `MAX_LIVE_SVGS` drawings, the ones closest to the centre
   * of the view; everything out of the view, below `CULL_ZOOM`, or of a plan
   * that is not on screen, becomes a box. It runs on every camera write.
   */
  function applyCulling() {
    const view = visibleRect(camera, viewport);
    const zoomedOut = camera.zoom < CULL_ZOOM;
    const candidates = [];
    for (const rec of plane) {
      if (rec.hidden) {
        cullRecord(rec);
        continue;
      }
      if (zoomedOut || !rectsIntersect(rec.rect, view)) {
        cullRecord(rec);
        continue;
      }
      candidates.push({ rec, distance: distanceToView(rec.rect, view) });
    }
    candidates.sort((left, right) => left.distance - right.distance);
    for (let index = 0; index < candidates.length; index += 1) {
      if (index < MAX_LIVE_SVGS) {
        materializeRecord(candidates[index].rec);
      } else {
        cullRecord(candidates[index].rec);
      }
    }
  }

  function cullRecord(rec) {
    if (rec.culled) {
      return;
    }
    // The drawing leaves the page; the node stays reachable in `rec.svg`, so
    // coming back into the view re-mounts the same node (the action in the
    // template does the swap).
    rec.culled = true;
  }

  function materializeRecord(rec) {
    if (!rec.culled) {
      return;
    }
    // An object whose drawing has not arrived yet (a clone waiting for its
    // paint) keeps its box: there is nothing to mount yet.
    if (!rec.previewSvg && !rec.svg) {
      return;
    }
    rec.culled = false;
  }

  /** What the box of a culled object says (plan.js:2098-2108, C15). */
  function boxLabel(rec) {
    const title = rec.caption || rec.id;
    return `${title} — ${Math.round(rec.rect.w)} × ${Math.round(rec.rect.h)}`;
  }

  // --- plans (plan.js:1204-1400) ----------------------------------------------

  function planOf(sessionId) {
    const session = state.sessions.find((entry) => entry.id === sessionId);
    return { id: sessionId, label: session ? session.label : sessionId };
  }

  function isVisiblePlan(planId) {
    return activePlan !== null && planId === activePlan.id;
  }

  function sessionDiagramCount(sessionId) {
    return state.diagrams.filter((diagram) => diagram.session_id === sessionId).length;
  }

  /** The plan the state leaves on screen when the human has not chosen one. */
  function initialPlan() {
    const drawn = state.sessions.find(
      (session) => sessionDiagramCount(session.id) > 0,
    );
    return drawn || state.sessions[0] || null;
  }

  /**
   * Shows the objects of the plan on screen and hides the rest (A2; C16):
   * hiding is one flag that takes the object out of the paint — the place,
   * the rectangle and the camera are untouched, and nothing is closed.
   */
  function applyPlanVisibility() {
    for (const rec of plane) {
      rec.hidden = !isVisiblePlan(rec.planId || "");
    }
  }

  /** The center of the newest object of a plan, in plan units (A2; B1). */
  function newestCenter(planId) {
    if (planId === USER_PLAN.id) {
      const newest = state.clones[state.clones.length - 1];
      return newest === undefined ? null : objectCenter(newest);
    }
    const diagramsOfPlan = state.diagrams.filter(
      (diagram) => diagram.session_id === planId,
    );
    const newest = diagramsOfPlan[diagramsOfPlan.length - 1];
    return newest === undefined ? null : objectCenter(newest);
  }

  /** Brings back the camera of the plan on screen (A2; C16). */
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
        const target = cameraOn(camera, viewport, center);
        camera.tx = target.tx;
        camera.ty = target.ty;
      }
    }
    applyCulling();
    notifyCamera();
  }

  /**
   * Brings one plan on screen (A2): the diagrams of its session and nobody
   * else's. The plan being left keeps the camera it had.
   */
  function showPlan(plan) {
    sessionMessage = "";
    const same =
      plan === null
        ? activePlan === null
        : activePlan !== null && activePlan.id === plan.id;
    if (same) {
      return;
    }
    cancelPan();
    if (activePlan !== null) {
      planCameras.set(activePlan.id, { tx: camera.tx, ty: camera.ty, zoom: camera.zoom });
    }
    activePlan = plan;
    applyPlanVisibility();
    restorePlanCamera();
    paintEmpty();
  }

  /** The session of the plan on screen is gone: switch, never stay empty. */
  function reconcileActivePlan() {
    // The plan of the human does not end with a session.
    if (activePlan !== null && activePlan.id === USER_PLAN.id) {
      return;
    }
    if (
      activePlan !== null &&
      state.sessions.some((session) => session.id === activePlan.id)
    ) {
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
  }

  /** How many objects the plan on screen has (A2; B1). */
  function planDiagramCount() {
    if (activePlan === null) {
      return 0;
    }
    if (activePlan.id === USER_PLAN.id) {
      return state.clones.length;
    }
    return sessionDiagramCount(activePlan.id);
  }

  /**
   * The empty surface of the plan on screen. The user plan has its own empty
   * state (`#empty-user`), which lands with the panels (T3, C34).
   */
  function paintEmpty() {
    const hasObjects = planDiagramCount() > 0;
    const onUserPlan = activePlan !== null && activePlan.id === USER_PLAN.id;
    showEmpty = !hasObjects && !onUserPlan;
  }

  // --- diagrams (plan.js:1879-2062) -------------------------------------------

  /** Asks Rust to close a diagram (PRD §7 RF28): the page never closes alone. */
  function emitClose(diagramId) {
    emit(EVENTS.close, { diagram_id: diagramId });
  }

  /**
   * Whether this object is the first of **its** plan (A2): only the first
   * object of a plan is framed with the camera of the event; from the second
   * one of that plan on, the camera pans and the zoom of the human is never
   * touched (C16). Read before the new object lands.
   */
  function framesOnInsert(planId) {
    for (const rec of plane) {
      if ((rec.planId || "") === planId) {
        return false;
      }
    }
    return true;
  }

  /**
   * Mounts a render as an object of the plan (C19, C20; the wire behaviour
   * of `insertDiagram`, `plan.js:1889-2053`).
   */
  function insertDiagram(payload) {
    const p = payload || {};
    // A re-render of the same id replaces the object, and with it leaves the
    // preview the human was viewing (C19).
    const previous = findRecord(p.diagram_id);
    if (previous) {
      removeRecord(p.diagram_id);
      viewedVersion.delete(p.diagram_id);
    }
    const holder = document.createElement("div");
    holder.innerHTML = p.svg || "";
    const svg = holder.querySelector("svg");
    if (!svg) {
      emit(EVENTS.renderAck, { req_id: p.req_id, ok: false });
      return;
    }
    // The render names the plan it belongs to (A2): that plan comes on
    // screen, so the human always sees what the agent just drew.
    const sessionId = typeof p.session_id === "string" ? p.session_id : "";
    if (sessionId !== "") {
      showPlan(planOf(sessionId));
    }
    const width = Number.parseFloat(svg.getAttribute("width") || "0");
    const height = Number.parseFloat(svg.getAttribute("height") || "0");
    // `placement` is the corner of the **object**: the SVG sits one padding
    // inside it. Without it the object lands on the centre of the view.
    const placement = p.placement;
    const center = placement
      ? {
          x: placement.x + (width + 2 * OBJECT_PADDING) / 2,
          y: placement.y + (height + 2 * OBJECT_PADDING) / 2,
        }
      : worldCenter();

    const rec = newRecord(p.diagram_id, "diagram", sessionId);
    rec.content = {
      x: center.x - width / 2,
      y: center.y - height / 2,
      w: width,
      h: height,
    };
    rec.svg = svg;
    rec.rect = {
      x: center.x - width / 2 - OBJECT_PADDING,
      y: center.y - height / 2 - OBJECT_PADDING,
      w: width + 2 * OBJECT_PADDING,
      h: height + 2 * OBJECT_PADDING,
    };
    rec.culled = false;

    const first = framesOnInsert(sessionId);
    plane.push(rec);
    // The size of the plan is refreshed before the culling decides, so the
    // new object is tested against the window the plan really has.
    refreshViewport();
    applyCulling();
    paintEmpty();

    // The first object of its plan (or a `focus` intent) takes the camera of
    // the event; the rest only pan, never zoom (PRD §7.2, RF27).
    if (first || p.focus) {
      applyCameraIntent(p.camera);
    } else if (placement) {
      panTo(cameraOn(camera, viewport, center));
    }

    // The frontend answers after two animation frames: the SVG is painted
    // (PRD §8.6.1, step 4). Rust waits `RENDER_ACK_TIMEOUT_MS` for this.
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        emit(EVENTS.renderAck, { req_id: p.req_id, ok: true });
      });
    });
  }

  // --- state (plan.js:2233-2286) ------------------------------------------------

  /**
   * Reads from the state where an object sits and what it is called
   * (PRD §8.6.1, S8): the page invents neither.
   */
  function readObject(rec, diagram) {
    if (typeof diagram.title === "string" && diagram.title !== "") {
      rec.caption = diagram.title;
    }
    if (typeof diagram.session_id === "string" && diagram.session_id !== "") {
      rec.planId = diagram.session_id;
    }
    if (diagram.pos && diagram.bbox) {
      rec.rect = {
        x: Number(diagram.pos.x),
        y: Number(diagram.pos.y),
        w: Number(diagram.bbox.w),
        h: Number(diagram.bbox.h),
      };
    }
  }

  /**
   * Drops what the state no longer lists, refreshes the rest, and asks the
   * owner for the drawing of every clone the page never drew (C17, C18).
   */
  function syncObjects() {
    const open = new Set(state.diagrams.map((diagram) => diagram.diagram_id));
    for (const clone of state.clones) {
      open.add(clone.clone_id);
    }
    for (const rec of [...plane]) {
      if (!open.has(rec.id)) {
        removeRecord(rec.id);
      }
    }
    if (selectedId !== null && !open.has(selectedId)) {
      selectedId = null;
    }
    for (const diagram of state.diagrams) {
      const rec = findRecord(diagram.diagram_id);
      if (rec) {
        readObject(rec, diagram);
      }
    }
    for (const clone of state.clones) {
      const rec = upsertClone(clone);
      // The state carries no SVG: a clone the page never drew — a window
      // that came back to life — asks the owner for the version it keeps.
      // A clone whose original closed has nothing to ask (`updatable` is
      // false), so the object keeps its box.
      if (!rec.svg && clone.updatable) {
        emit(EVENTS.cloneView, {
          clone_id: clone.clone_id,
          version: clone.snapshot_version,
        });
      }
    }
    applyPlanVisibility();
    applyCulling();
  }

  // --- clones of the user plan (plan.js:1510-1877; tools pill is T3) ----------

  /**
   * One clone of the user plan (B1): an object of the surface that belongs to
   * the human. It reuses the machinery of a diagram — the rectangle the
   * culling tests, the box, the camera transform — and carries none of the
   * agent parts. The tools pill (`×`, `↻`, history) is T3 (C31).
   */
  function createCloneObject(clone) {
    const rec = newRecord(clone.clone_id, "clone", USER_PLAN.id);
    // Born without a drawing: its box names it until the paint arrives.
    rec.culled = true;
    rec.rect = { x: 0, y: 0, w: 0, h: 0 };
    return rec;
  }

  /**
   * Puts the object of a clone where its rectangle says. The content **is**
   * the object — the drawing centers inside it with the stylesheet — so
   * moving a clone is moving one rectangle.
   */
  function placeCloneContent(rec) {
    rec.content = {
      x: rec.rect.x,
      y: rec.rect.y,
      w: rec.rect.w,
      h: rec.rect.h,
    };
  }

  /** Reads from the state where a clone sits and what it is called (B3). */
  function readClone(rec, clone) {
    if (typeof clone.title === "string" && clone.title !== "") {
      rec.caption = clone.title;
    }
    if (clone.pos && clone.bbox) {
      rec.rect = {
        x: Number(clone.pos.x),
        y: Number(clone.pos.y),
        w: Number(clone.bbox.w),
        h: Number(clone.bbox.h),
      };
      placeCloneContent(rec);
    }
  }

  /** The object of a clone, created or kept in step with the state. */
  function upsertClone(clone) {
    let rec = findRecord(clone.clone_id);
    if (!rec) {
      rec = createCloneObject(clone);
      plane.push(rec);
    }
    readClone(rec, clone);
    return rec;
  }

  /** Mounts the drawing the owner rendered for a clone (B1, B2). */
  function mountCloneDrawing(rec, payloadSvg) {
    const holder = document.createElement("div");
    holder.innerHTML = payloadSvg;
    const svg = holder.querySelector("svg");
    if (!svg) {
      return;
    }
    // A **new** drawing, not the one that left the page: the action swaps
    // the nodes and the culling re-decides right after.
    rec.svg = svg;
    rec.previewSvg = null;
    rec.culled = false;
  }

  /**
   * Paints a clone the owner rendered (B1, B2): the object takes the place
   * the owner keeps, the drawing mounts where it was, and a clone that just
   * arrived brings the user plan on screen — cloning is asking to analyze it.
   */
  function applyClonePaint(paint) {
    const p = paint || {};
    const first = framesOnInsert(USER_PLAN.id);
    const rec = upsertClone(p);
    if (p.svg) {
      mountCloneDrawing(rec, p.svg);
      // The truth about what the clone shows is the owner's: the snapshot
      // ends the browsing, an older version is a preview of the history.
      const version = Number(p.version || p.snapshot_version);
      const snapshot = Number(p.snapshot_version || version);
      if (version >= snapshot) {
        viewedCloneVersion.delete(p.clone_id);
      } else {
        viewedCloneVersion.set(p.clone_id, version);
      }
    }
    refreshViewport();
    applyCulling();
    paintEmpty();
    if (!p.bring) {
      return;
    }
    showPlan(USER_PLAN);
    // The camera travels until the clone, keeping the zoom of the human; the
    // first object of the plan is framed by the camera the plan restored.
    const center = objectCenter(p);
    if (!first && center !== null) {
      panTo(cameraOn(camera, viewport, center));
    }
  }

  // --- wiring (plan.js:2332-2449) ----------------------------------------------

  /**
   * Mounts (and unmounts, and swaps) the drawing of an object inside its
   * `.content`, before the `×`. This is the DOM half of the culling: the
   * flags live on the record, the nodes move here.
   */
  function svgMount(node, params) {
    let mounted = null;
    const apply = (current) => {
      const target = current.culled ? null : current.preview || current.svg;
      if (mounted === target) {
        return;
      }
      if (mounted) {
        mounted.remove();
        mounted = null;
      }
      if (target) {
        const button = node.querySelector("button.diagram-close");
        if (button) {
          node.insertBefore(target, button);
        } else {
          node.appendChild(target);
        }
        mounted = target;
      }
    };
    apply(params);
    return { update: apply };
  }

  onMount(() => {
    refreshViewport();

    // The listeners of the surface, attached the way the vanilla page does:
    // the wheel is not passive, because it calls `preventDefault`
    // (plan.js:398-412).
    planEl.addEventListener("wheel", onWheel, { passive: false });
    planEl.addEventListener("pointerdown", onPointerDown);
    planEl.addEventListener("pointermove", onPointerMove);
    planEl.addEventListener("pointerup", endDrag);
    planEl.addEventListener("pointercancel", endDrag);
    window.addEventListener("resize", onResize);

    // Every listener is registered **before** `graphium://ready` leaves, or
    // the burst Rust answers with would reach nobody (C2).
    listen(EVENTS.settings, (event) => {
      settings = settingsFromPayload(event.payload);
    });
    listen(EVENTS.state, (event) => {
      // The model lives in Rust (PRD §8.6.1): the page only reads the state.
      const payload = event.payload || {};
      state.diagrams = Array.isArray(payload.diagrams) ? payload.diagrams : [];
      state.sessions = Array.isArray(payload.sessions) ? payload.sessions : [];
      state.clones = Array.isArray(payload.clones) ? payload.clones : [];
      // A session the owner dropped takes its plan away: switch, never an
      // empty surface without explanation (A2).
      reconcileActivePlan();
      syncObjects();
      paintEmpty();
    });
    listen(EVENTS.render, (event) => {
      insertDiagram(event.payload || {});
    });
    listen(EVENTS.clonePaint, (event) => {
      applyClonePaint(event.payload || {});
    });

    // The camera copy of Rust starts as the page has it: the page pushes it
    // once so the owner never places a diagram against a camera it did not
    // read (`app.rs:727`).
    emit(EVENTS.cameraChanged, { ...camera });
    // The handshake (`plan.js:2449`): without it the first render waits
    // `READY_TIMEOUT_MS` and ends as `ui_error` (A1).
    emit(EVENTS.ready, {});

    return () => {
      planEl.removeEventListener("wheel", onWheel);
      planEl.removeEventListener("pointerdown", onPointerDown);
      planEl.removeEventListener("pointermove", onPointerMove);
      planEl.removeEventListener("pointerup", endDrag);
      planEl.removeEventListener("pointercancel", endDrag);
      window.removeEventListener("resize", onResize);
    };
  });
</script>

<!-- The plan: an infinite surface with a grid. The camera lives here
     (PRD §7.1, `index.html:11-13`). -->
<div id="plan" class="plan" class:dragging={draggingNow} bind:this={planEl}>
  <div class="diagrams" bind:this={diagramsEl}>
    {#each plane as rec (rec.id)}
      <div
        class="diagram"
        class:clone={rec.kind === "clone"}
        class:plan-hidden={rec.hidden}
      >
        <!-- The box that stands in for the drawing while it is out of the DOM
             (RNF1-ter). Before the content, so the `×` keeps painting above
             it: a culled diagram is still closable. -->
        <div
          class="diagram-box"
          class:hidden={!rec.culled}
          style:left="{rec.rect.x}px"
          style:top="{rec.rect.y}px"
          style:width="{rec.rect.w}px"
          style:height="{rec.rect.h}px"
        >
          <span class="diagram-box-label">{boxLabel(rec)}</span>
        </div>
        <div
          class="content"
          style:left="{rec.content.x}px"
          style:top="{rec.content.y}px"
          style:width="{rec.content.w}px"
          style:height="{rec.content.h}px"
          use:svgMount={{ culled: rec.culled, svg: rec.svg, preview: rec.previewSvg }}
        >
          {#if rec.kind === "diagram"}
            <!-- The `×` of the object (PRD §7 RF28): it shows up when the
                 pointer walks over the diagram, in the padding ring. -->
            <button
              type="button"
              class="diagram-close"
              aria-label="cerrar el diagrama"
              title="cerrar el diagrama"
              onpointerdown={(event) => event.stopPropagation()}
              onclick={(event) => {
                event.stopPropagation();
                emitClose(rec.id);
              }}>×</button>
          {/if}
        </div>
      </div>
    {/each}
  </div>
</div>

<!-- State 1 of the vanilla page (`index.html:67-74`): the surface says what to
     do with it instead of waiting for an agent that draws somewhere else. -->
{#if showEmpty}
  <div class="empty">
    <p class="empty-title">esperando al agente…</p>
    <p class="empty-hint">
      conectá un cliente MCP y corré <code>graphium mcp-config</code>
    </p>
  </div>
{/if}

<!-- Scaffold diagnostic, not part of the parity contract (CONTRACT.md §9): it
     tells the two builds apart while the port is partial. T4 removes it. -->
<div class="scaffold" aria-live="polite">
  svelte · {settings ? themeOf(settings) : "sin ajustes"} · {plane.length}
  objeto{plane.length === 1 ? "" : "s"} · {liveSvgs} en página{hasBridge() ? "" : " · sin puente"}
</div>
