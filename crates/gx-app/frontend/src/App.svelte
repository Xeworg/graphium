<script>
  // The plan, as a Svelte component — T2 and T3 of `odd/tasks/svelte-ui.md`
  // (S2). `CONTRACT.md` is the parity checklist T4 walks.
  //
  // What this component owns: the camera (wheel zoom anchored at the cursor,
  // drag pan, the animated pan, the camera intents of the renders), the
  // objects with their `×`, their box and the culling (≤ 3 live SVGs), the
  // wholesale reconciliation from `graphium://state`, the clones of the user
  // plan (their move by drag, their tools pill) and every remaining piece of
  // chrome of the vanilla page: the panel of open diagrams, the history of
  // versions, the switcher of plans, the settings panel, the error banner and
  // the render ghost.
  //
  // The panels themselves are components (`PlanPanel`, `SessionsPanel`,
  // `SettingsPanel`, `CloneTools`); this file keeps the state, the wire and
  // the plane. The pure arithmetic lives in `src/lib/` (`plane.js`, `grid.js`,
  // `history.js`, `labels.js`, `presets.js`).
  import { onMount } from "svelte";

  import {
    CAMERA_THROTTLE_MS,
    CULL_ZOOM,
    EVENTS,
    MAX_LIVE_SVGS,
    OBJECT_PADDING,
    PAN_MS,
    SETTINGS_THROTTLE_MS,
    USER_PLAN,
    USER_PLAN_ID,
    ZOOM_STEP,
  } from "./lib/contract.js";
  import { emit, listen } from "./lib/tauri.js";
  import {
    gridPaintCamera,
    gridPaintStatic,
    palettePaint,
    settingsFromPayload,
    settingsId,
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
  import { boxText, sessionEndedNotice } from "./lib/labels.js";
  import CloneTools from "./lib/CloneTools.svelte";
  import PlanPanel from "./lib/PlanPanel.svelte";
  import SessionsPanel from "./lib/SessionsPanel.svelte";
  import SettingsPanel from "./lib/SettingsPanel.svelte";

  // --- page state (plan.js:270-290) ---------------------------------------

  // The camera lives in the page (PRD §7.2): Rust keeps only a copy.
  let camera = $state({ tx: 0, ty: 0, zoom: 1 });
  // The settings of the human. `null` until the first `graphium://settings`.
  let settings = $state(null);
  // The state Rust owns (PRD §8.6.1): the truth about what the plan shows and
  // the only source of the panels (C22). It is `$state.raw` because it is
  // replaced wholesale on every `graphium://state` (C17) and no code writes one
  // of its three arrays in place: the deep proxy would only wrap every diagram
  // and every clone of the payload for nothing.
  let state = $state.raw({ diagrams: [], sessions: [], clones: [] });
  // The objects of the plane, in arrival order — the same order the vanilla
  // `objects` Map iterates (plan.js:275). Diagrams and clones share it; the
  // ids never collide (the store numbers `d-…`, the owner numbers `c-…`).
  let plane = $state([]);
  // State 1 (`index.html:67-82`): only a session plan without objects shows
  // `#empty`; the user plan has its own empty state (`#empty-user`).
  // The diagram the human last chose — the panel marks it and the arrows of
  // the keyboard walk its history (C23, C30).
  let selectedId = $state(null);
  // The class the surface gets while the hand drags it (`plan.dragging`).
  let draggingNow = $state(false);
  // The plan on screen (A2 of `f8-planos.md`): one at a time, each with its
  // own camera (plan.js:1171-1176).
  let activePlan = $state(null);
  // Why the plan on screen changed, when it had to (rendered by the switcher;
  // C29).
  let sessionMessage = $state("");
  // The version of the history the human is looking at, per diagram and per
  // clone: the absolute number while a preview is on the plan, nothing when
  // the live drawing (or the photo of the clone) is (C25-C27, C31-C33).
  let viewedVersion = $state({});
  let viewedCloneVersion = $state({});
  // The error banner (`#banner`) and the render ghost (`#ghost`): `null` while
  // they are hidden (C35, C36).
  let banner = $state(null);
  let ghost = $state(null);

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
  // The clone the pointer is moving (B3 of `f8-planos.md`): dragging an
  // **object** of the user plan moves it; dragging the background pans.
  let moving = null;
  const planCameras = new Map();
  // The 100 ms throttle and the `settingsId` dedupe of `settings-changed`
  // (C6; plan.js:744-773).
  let settingsTimer = null;
  let settingsPending = null;
  let settingsSent = null;
  // The timer of «calculando layout…» -> `N nodos, M edges` (C36).
  let ghostTimer = null;

  // The grid is published in the two halves it is written in (T1): the six
  // variables that only the settings decide — written once per settings change
  // — and the five the camera derives, written on every camera write. One
  // effect for all eleven rewrote the six on every pan frame: six CSSOM writes
  // per frame that could not change anything.
  const gridStatic = $derived(gridPaintStatic(settings));
  const gridCamera = $derived(gridPaintCamera(settings, camera));
  const palette = $derived(palettePaint(settings));
  // The camera transform of the layer. The vanilla page applies the same
  // transform to every object (`plan.js:393-396`); one transform on the
  // camera layer is the same result for one write.
  const layerTransform = $derived(
    `translate(${camera.tx}px, ${camera.ty}px) scale(${camera.zoom})`,
  );
  // How many objects the plan on screen has (A2; B1): the clones on the user
  // plan, the diagrams of the session otherwise.
  const planObjectCount = $derived(
    activePlan === null
      ? 0
      : activePlan.id === USER_PLAN.id
        ? state.clones.length
        : sessionDiagramCount(activePlan.id),
  );
  // State 1: a session plan with nothing on it, and the plan of the human with
  // nothing on it, are two different empty states (C34).
  const showEmpty = $derived(planObjectCount === 0 && activePlan?.id !== USER_PLAN_ID);
  const showEmptyUser = $derived(planObjectCount === 0 && activePlan?.id === USER_PLAN_ID);
  // The rows of the switcher: every session the state lists, in the order Rust
  // lists it, with how many of its diagrams are open (C28).
  const sessionRows = $derived(
    state.sessions.map((session) => ({
      session,
      count: sessionDiagramCount(session.id),
    })),
  );

  // The grid and the palette, published to the stylesheets exactly like
  // `applyGrid`/`applySettings` do (`plan.js:362-383`, `plan.js:729-742`). The
  // static half depends on `settings` alone, so a pan frame never enters it.
  $effect(() => {
    if (!planEl) return;
    for (const [name, value] of Object.entries(gridStatic)) {
      planEl.style.setProperty(name, value);
    }
  });
  $effect(() => {
    if (!planEl) return;
    for (const [name, value] of Object.entries(gridCamera)) {
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
      // The drawing of the version the human is browsing, while it lasts
      // (C27). The culling re-mounts this one, not the live drawing under it.
      previewSvg: null,
      // The entry of the clone this object is, as the owner states it (B1;
      // plan.js:1690): the tools pill reads its counts from here.
      cloneState: null,
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

  // --- pointer handlers (plan.js:416-551) -----------------------------------

  function onWheel(event) {
    event.preventDefault();
    // The human taking the wheel is the new owner of the view: a pan in
    // flight stops where it is instead of fighting the zoom.
    cancelPan();
    zoomAt(event.clientX, event.clientY, event.deltaY < 0 ? ZOOM_STEP : 1 / ZOOM_STEP);
  }

  /**
   * The clone under the pointer, when the plan on screen is the one of the
   * human (B3). Anywhere else there is nothing to grab: the background pans the
   * camera, which is what it has always done.
   */
  function cloneUnder(target) {
    if (activePlan === null || activePlan.id !== USER_PLAN_ID) {
      return null;
    }
    if (!(target instanceof Element)) {
      return null;
    }
    const node = target.closest(".diagram.clone");
    if (node === null) {
      return null;
    }
    const rec = findRecord(node.dataset.id);
    return rec !== null && rec.kind === "clone" ? rec : null;
  }

  /**
   * Starts moving the clone the hand grabbed (B3): the object becomes the
   * chosen one (the arrows of the keyboard walk its history), the pointer is
   * captured and the place it started from is kept, so a click writes nothing.
   */
  function moveCloneStart(event, rec) {
    selectedId = rec.id;
    moving = {
      rec,
      fromX: event.clientX,
      fromY: event.clientY,
      x: rec.rect.x,
      y: rec.rect.y,
    };
    draggingNow = true;
    planEl.setPointerCapture(event.pointerId);
    event.preventDefault();
  }

  /**
   * Puts a clone where the hand says, in plan units: the pointer travels in
   * screen pixels and the plan in plan units, and the zoom of the camera is
   * what joins them (B3). The object is one reactive rectangle: moving it
   * repaints the content and the box.
   */
  function setClonePlace(rec, x, y) {
    rec.rect = { ...rec.rect, x, y };
    placeCloneContent(rec);
  }

  function onPointerDown(event) {
    cancelPan();
    // A button (the `×` of an object, a step of the history) does its own
    // thing: it never starts a move of the object nor a drag of the camera
    // (plan.js:432-435).
    if (event.target instanceof Element && event.target.closest("button")) {
      return;
    }
    const clone = cloneUnder(event.target);
    if (clone !== null) {
      moveCloneStart(event, clone);
      return;
    }
    dragging = { x: event.clientX, y: event.clientY, tx: camera.tx, ty: camera.ty };
    draggingNow = true;
    planEl.setPointerCapture(event.pointerId);
  }

  function onPointerMove(event) {
    if (moving !== null) {
      const zoom = camera.zoom > 0 ? camera.zoom : 1;
      setClonePlace(
        moving.rec,
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
    applyCulling();
    notifyCamera();
  }

  function endDrag(event) {
    // The hand let go of a clone: the object stays where it is and the owner
    // keeps that place (B3).
    if (moving !== null) {
      const move = moving;
      moving = null;
      draggingNow = false;
      if (planEl.hasPointerCapture(event.pointerId)) {
        planEl.releasePointerCapture(event.pointerId);
      }
      const rect = move.rec.rect;
      if (rect.x !== move.x || rect.y !== move.y) {
        // Only the position of the view travels: no camera, no store, no
        // render (V2). A click without movement asks for nothing.
        emit(EVENTS.cloneMove, {
          clone_id: move.rec.id,
          pos: { x: rect.x, y: rect.y },
        });
      }
      return;
    }
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
  }

  /** The session of the plan on screen is gone: switch, never stay empty. */
  function reconcileActivePlan() {
    if (planOnScreenEnded()) {
      const ended = activePlan;
      const next = initialPlan();
      showPlan(next === null ? null : { id: next.id, label: next.label });
      if (ended !== null) {
        sessionMessage = sessionEndedNotice(ended, next);
      }
    }
    // A plan the state no longer lists has nobody to come back to: its camera
    // leaves with the session, or the map grows with one entry per session the
    // human ever visited (A2). After `showPlan`, which saves the camera of the
    // plan it leaves — including the one that just ended.
    prunePlanCameras();
  }

  /**
   * Whether the plan on screen has to leave (A2): its session is gone, or no
   * plan has come on screen yet — the first state has to show one.
   */
  function planOnScreenEnded() {
    // The plan of the human does not end with a session.
    if (activePlan === null) {
      return true;
    }
    if (activePlan.id === USER_PLAN.id) {
      return false;
    }
    return !state.sessions.some((session) => session.id === activePlan.id);
  }

  /** Forgets the camera of every plan the state does not list any more. */
  function prunePlanCameras() {
    const live = new Set(state.sessions.map((session) => session.id));
    live.add(USER_PLAN_ID);
    for (const planId of [...planCameras.keys()]) {
      if (!live.has(planId)) {
        planCameras.delete(planId);
      }
    }
  }

  // --- diagrams (plan.js:1879-2062) -------------------------------------------

  /** Asks Rust to close a diagram (PRD §7 RF28): the page never closes alone. */
  function emitClose(diagramId) {
    emit(EVENTS.close, { diagram_id: diagramId });
  }

  /** Asks Rust to put a clone of a diagram on the user plan (B1). */
  function emitClone(diagramId) {
    emit(EVENTS.clone, { diagram_id: diagramId });
  }

  /** Asks the owner to remove a clone from the user plan (L6). */
  function emitCloneRemove(cloneId) {
    emit(EVENTS.cloneRemove, { clone_id: cloneId });
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
      delete viewedVersion[p.diagram_id];
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

  /**
   * The human chose an entry of the panel: its plan comes on screen, the entry
   * is marked and the camera travels until the object (plan.js:984-1000).
   */
  function selectDiagram(diagram) {
    // The camera never travels to an object nobody can see (A2, D5).
    showPlan(planOf(diagram.session_id));
    selectedId = diagram.diagram_id;
    const center = objectCenter(diagram);
    if (center === null) {
      return;
    }
    panTo(cameraOn(camera, viewport, center));
  }

  // --- history of versions (plan.js:1008-1150) --------------------------------

  /**
   * Asks Rust to draw one version of the history on the plan (RF14, L5).
   * Read-only on purpose: the object never moves, and walking back to the
   * newest version restores the live drawing the page already holds — without
   * a render at all (C26).
   */
  function viewVersion(diagram, version) {
    const target = Math.min(
      diagram.current_version,
      Math.max(diagram.oldest_version, version),
    );
    if (target === diagram.current_version) {
      delete viewedVersion[diagram.diagram_id];
      restoreLive(findRecord(diagram.diagram_id));
    } else {
      viewedVersion[diagram.diagram_id] = target;
      emit(EVENTS.versionView, {
        diagram_id: diagram.diagram_id,
        version: target,
      });
    }
  }

  /** One step of the history, from the version in view (plan.js:1024-1028). */
  function versionStep(diagram, step) {
    const at = viewedVersion[diagram.diagram_id] ?? diagram.current_version;
    viewVersion(diagram, at + step);
  }

  /**
   * Paints the preview of a version where the live drawing was (L5, C27): the
   * object keeps its rectangle on the plan — the culling keeps testing the
   * rectangle Rust placed — and the preview centers on it, so the plan never
   * changes shape because of the history. The live drawing stays reachable on
   * the record for the restore.
   */
  function paintPreview(rec, svg) {
    const width = Number.parseFloat(svg.getAttribute("width") || "0");
    const height = Number.parseFloat(svg.getAttribute("height") || "0");
    const center = {
      x: rec.rect.x + rec.rect.w / 2,
      y: rec.rect.y + rec.rect.h / 2,
    };
    rec.content = {
      x: center.x - width / 2,
      y: center.y - height / 2,
      w: width,
      h: height,
    };
    rec.previewSvg = svg;
  }

  /** Brings the live drawing back: the preview leaves the plan (C26). */
  function restoreLive(rec) {
    if (!rec || !rec.previewSvg) {
      return;
    }
    rec.previewSvg = null;
    // The styles of the live drawing come from the rectangle Rust placed: the
    // SVG sits one padding inside the object, as at insert time.
    rec.content = {
      x: rec.rect.x + OBJECT_PADDING,
      y: rec.rect.y + OBJECT_PADDING,
      w: rec.rect.w - 2 * OBJECT_PADDING,
      h: rec.rect.h - 2 * OBJECT_PADDING,
    };
  }

  /** The preview of a version the human is still asking for (R4, C27). */
  function applyVersionPaint(payload) {
    const rec = findRecord(payload.diagram_id);
    if (!rec) {
      // The diagram closed while the render of the preview ran.
      return;
    }
    // Only the version the human still asked for paints: walking fast through
    // the history must not leave an older answer under a newer one.
    const viewed = viewedVersion[payload.diagram_id];
    if (viewed === undefined || viewed !== Number(payload.version)) {
      return;
    }
    const holder = document.createElement("div");
    holder.innerHTML = payload.svg || "";
    const svg = holder.querySelector("svg");
    if (!svg) {
      return;
    }
    paintPreview(rec, svg);
  }

  // --- clones of the user plan (plan.js:1510-1877) ----------------------------

  /**
   * One clone of the user plan (B1): an object of the surface that belongs to
   * the human. It reuses the machinery of a diagram — the rectangle the
   * culling tests, the box, the camera transform — and carries none of the
   * agent parts: no `×` of a diagram, no `diagram_id`, and its place is
   * written by the hand (B3).
   */
  function createCloneObject(clone) {
    const rec = newRecord(clone.clone_id, "clone", USER_PLAN.id);
    rec.cloneState = clone;
    // Born without a drawing: its box names it until the paint arrives.
    rec.culled = true;
    rec.rect = { x: 0, y: 0, w: 0, h: 0 };
    return rec;
  }

  /**
   * Puts the object of a clone where its rectangle says. The content **is**
   * the object — the drawing centers inside it with the stylesheet — so moving
   * a clone is moving one rectangle, and its tools travel with it.
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
    rec.cloneState = clone;
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
    // A **new** drawing, not the one that left the page: the action swaps the
    // nodes and the culling re-decides right after.
    rec.svg = svg;
    rec.previewSvg = null;
    rec.culled = false;
  }

  /**
   * Paints a clone the owner rendered (B1, B2): the object takes the place the
   * owner keeps, the drawing mounts where it was, and a clone that just
   * arrived brings the user plan on screen — cloning is asking to analyze it.
   */
  function applyClonePaint(paint) {
    const p = paint || {};
    const first = framesOnInsert(USER_PLAN.id);
    const rec = upsertClone(p);
    // A paint without a drawing is an answer with nothing new to draw: the
    // clone the human asked for already exists (L6) or its photo is the only
    // drawing left (S4), so the page keeps what it shows.
    if (p.svg) {
      mountCloneDrawing(rec, p.svg);
      // The truth about what the clone shows is the owner's: the snapshot ends
      // the browsing, an older version is a preview of the history (C33).
      const version = Number(p.version || p.snapshot_version);
      const snapshot = Number(p.snapshot_version || version);
      if (version >= snapshot) {
        delete viewedCloneVersion[p.clone_id];
      } else {
        viewedCloneVersion[p.clone_id] = version;
      }
    }
    refreshViewport();
    applyCulling();
    if (!p.bring) {
      // A version of the history never moves the camera; the answer to a clone
      // request always does: the human asked to see that clone.
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

  /**
   * «Actualizar el clon» (B2, S3): asks the owner for the current version of the
   * original. The page draws nothing by itself and moves nothing.
   */
  function refreshClone(clone) {
    if (!clone || !clone.updatable) {
      return;
    }
    delete viewedCloneVersion[clone.clone_id];
    emit(EVENTS.cloneView, { clone_id: clone.clone_id });
  }

  /** One step of the history of the original (B2, B3). */
  function cloneVersionStep(clone, step) {
    if (!clone || !clone.updatable) {
      return;
    }
    const at = viewedCloneVersion[clone.clone_id] ?? clone.snapshot_version;
    viewCloneVersion(clone, at + step);
  }

  /** Asks the owner for one version of the original inside the clone (B3). */
  function viewCloneVersion(clone, version) {
    const target = Math.min(
      clone.current_version,
      Math.max(clone.oldest_version, version),
    );
    if (target === clone.snapshot_version) {
      // The photo is the version in view: there is nothing to browse.
      delete viewedCloneVersion[clone.clone_id];
    } else {
      viewedCloneVersion[clone.clone_id] = target;
    }
    emit(EVENTS.cloneView, { clone_id: clone.clone_id, version: target });
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
        delete viewedCloneVersion[rec.id];
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

  // --- banner and ghost (plan.js:2303-2330; C35, C36) --------------------------

  /** State 3: what failed, where, and what the human can do about it. */
  function showError(payload) {
    const p = payload || {};
    const extra = Number(p.extra || 0);
    banner = {
      path: p.path || "",
      message: p.message || "",
      hint: p.hint || "",
      extra: extra > 0 ? `+${extra} errores` : "",
    };
  }

  /**
   * State 2: a render in flight. «calculando layout…» at once and the counts
   * after 2000 ms. `graphium://pending` has no emitter in this tree: the
   * handler is kept exactly as the vanilla page has it, and the event is never
   * invented (C36).
   */
  function showPending(payload) {
    ghost = { text: "calculando layout…" };
    window.clearTimeout(ghostTimer);
    ghostTimer = window.setTimeout(() => {
      const nodes = Number(payload.nodes || 0);
      const edges = Number(payload.edges || 0);
      ghost = { text: `${nodes} nodos, ${edges} edges` };
    }, 2000);
  }

  function hidePending() {
    ghost = null;
    window.clearTimeout(ghostTimer);
  }

  // --- settings (plan.js:639-833; C6, C37, C38) -------------------------------

  /**
   * Sends the whole settings object to Rust, at most ten times per second
   * (plan.js:755-773, C6): a dragged slider fires sixty changes per second and
   * each save is a file. Nothing moved since the last write asks for nothing.
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
      emit(EVENTS.settingsChanged, payload);
    }, SETTINGS_THROTTLE_MS);
  }

  /**
   * A control of the panel moved: the plan repaints at once (the grid and the
   * palette read `settings`) and Rust is told, throttled.
   *
   * Nothing leaves the page before the first `graphium://settings`: the empty
   * controls are not a value the human chose, and a half-filled object must
   * never reach the file (C6, plan.js:775-786).
   */
  function onSettingsChange(next) {
    if (settings === null) {
      return;
    }
    settings = next;
    notifySettings(next);
  }

  // --- keyboard (plan.js:2408-2448; C30) ---------------------------------------

  /**
   * ←/→ walk the history of the selected object (a clone first: it walks the
   * history of its original), like the arrows of its panel item. A field that
   * holds the keyboard keeps its arrows: the history never steals them from the
   * human.
   */
  function onKeyDown(event) {
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
    const clone = state.clones.find((entry) => entry.clone_id === selectedId);
    if (clone) {
      if (!(clone.version_count > 1) || !clone.updatable) {
        return;
      }
      const at = viewedCloneVersion[selectedId] ?? clone.snapshot_version;
      viewCloneVersion(clone, event.key === "ArrowLeft" ? at - 1 : at + 1);
      event.preventDefault();
      return;
    }
    const diagram = state.diagrams.find((entry) => entry.diagram_id === selectedId);
    if (!diagram || !(diagram.version_count > 1)) {
      return;
    }
    const at = viewedVersion[selectedId] ?? diagram.current_version;
    viewVersion(diagram, event.key === "ArrowLeft" ? at - 1 : at + 1);
    event.preventDefault();
  }

  // --- the culling's DOM half ---------------------------------------------------

  /**
   * Mounts (and unmounts, and swaps) the drawing of an object inside its
   * `.content`, prepended the way the vanilla page does it (`content.prepend`
   * in `insertDiagram`, `paintPreview` and `materializeObject`), so the `×` of a
   * diagram keeps painting above it and the tools of a clone keep their corner.
   * This is the DOM half of the culling: the flags live on the record, the
   * nodes move here. The preview of a version is what the culling re-mounts
   * while it lasts (C27).
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
        node.prepend(target);
        mounted = target;
      }
    };
    apply(params);
    return { update: apply };
  }

  // --- wiring (plan.js:2332-2449) ----------------------------------------------

  onMount(() => {
    refreshViewport();

    // The listeners of the surface, attached the way the vanilla page does:
    // the wheel is not passive, because it calls `preventDefault`
    // (plan.js:398-412). The resize and the keyboard are not here: they belong
    // to `<svelte:window>` below, which removes them when the page goes.
    planEl.addEventListener("wheel", onWheel, { passive: false });
    planEl.addEventListener("pointerdown", onPointerDown);
    planEl.addEventListener("pointermove", onPointerMove);
    planEl.addEventListener("pointerup", endDrag);
    planEl.addEventListener("pointercancel", endDrag);

    // Every listener is registered **before** `graphium://ready` leaves, or
    // the burst Rust answers with would reach nobody (C2).
    listen(EVENTS.settings, (event) => {
      const next = settingsFromPayload(event.payload);
      // While the human is still moving a control the panel is the truth: the
      // confirmation of an older value must not pull a slider back under their
      // hand. The last change is always confirmed once the panel goes quiet
      // (`plan.js:2439-2442`).
      if (settingsTimer !== null || settingsPending !== null) {
        return;
      }
      settingsSent = settingsId(next);
      settings = next;
    });
    listen(EVENTS.state, (event) => {
      // The model lives in Rust (PRD §8.6.1): the page only reads the state.
      const payload = event.payload || {};
      // Wholesale, never field by field: `state` is `$state.raw`, so the three
      // arrays are new on every event and the identity of the object is what
      // tells the page the state moved (C17).
      state = {
        diagrams: Array.isArray(payload.diagrams) ? payload.diagrams : [],
        sessions: Array.isArray(payload.sessions) ? payload.sessions : [],
        clones: Array.isArray(payload.clones) ? payload.clones : [],
      };
      // A session the owner dropped takes its plan away: switch, never an
      // empty surface without explanation (A2, C29).
      reconcileActivePlan();
      syncObjects();
    });
    listen(EVENTS.render, (event) => {
      // A render means the layout is over: the ghost leaves before the object
      // lands (C36).
      hidePending();
      insertDiagram(event.payload || {});
    });
    listen(EVENTS.versionPaint, (event) => {
      applyVersionPaint(event.payload || {});
    });
    listen(EVENTS.clonePaint, (event) => {
      applyClonePaint(event.payload || {});
    });
    listen(EVENTS.pending, (event) => {
      showPending(event.payload || {});
    });
    listen(EVENTS.error, (event) => {
      // An error also hides the ghost: the layout is not coming (C35).
      hidePending();
      showError(event.payload || {});
    });

    // The camera copy of Rust starts as the page has it: the page pushes it
    // once so the owner never places a diagram against a camera it did not
    // read (`app.rs:727`).
    emit(EVENTS.cameraChanged, { ...camera });
    // The handshake (`plan.js:2449`): without it the first render waits
    // `READY_TIMEOUT_MS` and ends as `ui_error` (A1).
    emit(EVENTS.ready, {});

    return () => {
      // The wheel and the pointer are still imperative (T1: whether they move
      // to the template is the empirical test of T4), so they are removed here.
      planEl.removeEventListener("wheel", onWheel);
      planEl.removeEventListener("pointerdown", onPointerDown);
      planEl.removeEventListener("pointermove", onPointerMove);
      planEl.removeEventListener("pointerup", endDrag);
      planEl.removeEventListener("pointercancel", endDrag);
      // A timer that fires after the component is gone writes to a page that
      // is no longer there: the two throttles and the ghost of a render in
      // flight leave with it.
      window.clearTimeout(cameraTimer);
      cameraTimer = null;
      window.clearTimeout(settingsTimer);
      settingsTimer = null;
      settingsPending = null;
      window.clearTimeout(ghostTimer);
      ghostTimer = null;
    };
  });
</script>

<!-- The resize and the keyboard of the page: the template owns both, so the
     component removes them when it goes (the teardown bug of T1). -->
<svelte:window onresize={onResize} onkeydown={onKeyDown} />

<!-- The plan: an infinite surface with a grid. The camera lives here
     (PRD §7.1, `index.html:11-13`). -->
<div id="plan" class="plan" class:dragging={draggingNow} bind:this={planEl}>
  <div class="diagrams" bind:this={diagramsEl}>
    {#each plane as rec (rec.id)}
      <div
        class="diagram"
        class:clone={rec.kind === "clone"}
        class:plan-hidden={rec.hidden}
        data-id={rec.id}
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
          <span class="diagram-box-label">{boxText(rec)}</span>
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
              }}>×</button
            >
          {:else}
            <!-- The tools of a clone (B2, B3): the history of the original,
                 «Actualizar el clon» and the `×` that removes the view
                 instance, in one compact pill (L8). -->
            <CloneTools
              clone={rec.cloneState}
              viewed={viewedCloneVersion[rec.id]}
              width={rec.rect.w}
              hidden={rec.hidden}
              onstep={(step) => cloneVersionStep(rec.cloneState, step)}
              onrefresh={() => refreshClone(rec.cloneState)}
              onremove={() => emitCloneRemove(rec.id)}
            />
          {/if}
        </div>
      </div>
    {/each}
  </div>
</div>

<!-- The panel of the open diagrams (PRD §7 RF26, RF28, S1). It is a sibling of
     the plan, never a child: the wheel and the drag of the camera listen on
     `#plan`, so the panel can never steal them. -->
<PlanPanel
  diagrams={state.diagrams}
  selectedId={selectedId}
  viewed={viewedVersion}
  onselect={selectDiagram}
  onclose={emitClose}
  onclone={emitClone}
  onversion={versionStep}
/>

<!-- The switcher of plans (A2 of `f8-planos.md`; S1, S3, D3, D5): «Mi plano»
     and one row per session, with the notice that says when the plan on screen
     had to change because its session ended. -->
<SessionsPanel
  rows={sessionRows}
  clones={state.clones}
  activePlanId={activePlan === null ? null : activePlan.id}
  notice={sessionMessage}
  onchoose={showPlan}
/>

<!-- State 1 of the vanilla page (`index.html:67-74`): the surface says what to
     do with it instead of waiting for an agent that draws somewhere else. -->
<div id="empty" class="empty" class:hidden={!showEmpty}>
  <p class="empty-title">esperando al agente…</p>
  <p class="empty-hint">
    conectá un cliente MCP y corré <code>graphium mcp-config</code>
  </p>
</div>

<!-- State 1 of the plan of the human (B1 of `f8-planos.md`, S2/S4): the
     surface is theirs and nobody draws on it for them. Only one of the two
     empty states paints at a time, and the page decides with the plan on
     screen (C34). -->
<div id="empty-user" class="empty" class:hidden={!showEmptyUser}>
  <p class="empty-title">tu plano está vacío</p>
  <p class="empty-hint">
    cloná un diagrama desde el panel de diagramas abiertos con <code>⧉</code> para
    analizarlo acá
  </p>
</div>

<!-- State 3: error banner. -->
<div id="banner" class="banner" class:hidden={banner === null} role="alert">
  <div class="banner-body">
    <span id="banner-path" class="banner-path">{banner?.path ?? ""}</span>
    <span id="banner-message" class="banner-message">{banner?.message ?? ""}</span>
    <span id="banner-hint" class="banner-hint">{banner?.hint ?? ""}</span>
    <span id="banner-extra" class="banner-extra">{banner?.extra ?? ""}</span>
  </div>
  <button
    id="banner-close"
    class="banner-close"
    type="button"
    aria-label="cerrar"
    onclick={() => (banner = null)}>×</button
  >
</div>

<!-- State 2: a render in flight. -->
<div id="ghost" class="ghost" class:hidden={ghost === null}>
  <span id="ghost-text">{ghost?.text ?? "calculando layout…"}</span>
</div>

<!-- The settings of the plan (PRD §7.1 RF25, §8.5.2 RF22, S19). The look
     belongs to the human: the agent never changes it. The gear and the panel
     sit here, outside `#plan`, so the wheel and the drag of the camera never
     see them. -->
<SettingsPanel settings={settings} onchange={onSettingsChange} />
