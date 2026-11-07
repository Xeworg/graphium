<script>
  // The plan, as a Svelte component — scaffold of T1 (`odd/tasks/svelte-ui.md`).
  //
  // What this component already owns: the surface, the isometric grid with the
  // contract numbers of `tests/grid.rs`, the `graphium://ready` handshake and
  // the paint confirmation of a render. What it does NOT own yet (T2/T3): the
  // camera movement, the placement cascade, the objects with their boxes and
  // culling, the panels of diagrams/sessions/settings, the versions and the
  // clones. `CONTRACT.md` is the parity checklist T4 walks.
  import { onMount } from "svelte";

  import { EVENTS, OBJECT_PADDING } from "./lib/contract.js";
  import { emit, hasBridge, listen } from "./lib/tauri.js";
  import {
    gridPaint,
    palettePaint,
    settingsFromPayload,
    themeOf,
  } from "./lib/grid.js";

  // The camera lives in the page (PRD §7.2, `plan.js:274`): Rust keeps only a
  // copy. T1 leaves it at the identity; T2 owns the pan, the wheel and the pan
  // animation.
  let camera = $state({ tx: 0, ty: 0, zoom: 1 });
  // The settings of the human: Rust sends them and the page paints them
  // (`plan.js:282`). `null` until the first `graphium://settings` arrives.
  let settings = $state(null);
  // How many diagrams the state lists (scaffold: the panels are T3).
  let openDiagrams = $state(0);
  // How many drawings this page mounted (scaffold diagnostic only).
  let paintedSvg = $state(0);
  let planEl;
  let diagramsEl;

  const grid = $derived(gridPaint(settings, camera));
  const palette = $derived(palettePaint(settings));

  // The grid, published to the stylesheet exactly like `applyGrid` does
  // (`plan.js:362-383`).
  $effect(() => {
    if (!planEl) return;
    for (const [name, value] of Object.entries(grid)) {
      planEl.style.setProperty(name, value);
    }
  });

  // The theme and the palette of the human (`plan.js:729-742`): the root
  // element carries both, so the chrome follows the theme too.
  $effect(() => {
    const root = document.documentElement;
    root.dataset.theme = themeOf(settings);
    for (const [name, value] of Object.entries(palette)) {
      root.style.setProperty(name, value);
    }
  });

  // The camera transform of the layer (`plan.js:385-402`, `applyCamera`).
  $effect(() => {
    if (!diagramsEl) return;
    diagramsEl.style.transform = `translate(${camera.tx}px, ${camera.ty}px) scale(${camera.zoom})`;
  });

  /**
   * Mounts the drawing of a render. T1 placeholder: the object lands where
   * Rust placed it, without the box, the culling or the `×` — all of that is
   * T2 (`plan.js:1889-2053`, `insertDiagram`).
   */
  function mountRender(payload) {
    const holder = document.createElement("div");
    holder.innerHTML = payload.svg || "";
    const svg = holder.querySelector("svg");
    if (!svg) {
      emit(EVENTS.renderAck, { req_id: payload.req_id, ok: false });
      return;
    }
    const width = Number.parseFloat(svg.getAttribute("width") || "0");
    const height = Number.parseFloat(svg.getAttribute("height") || "0");
    // `placement` is the corner of the **object** — the SVG plus its padding
    // — so the drawing sits one padding inside it (`plan.js:1908-1920`).
    const placement = payload.placement;
    const center = placement
      ? {
          x: placement.x + (width + 2 * OBJECT_PADDING) / 2,
          y: placement.y + (height + 2 * OBJECT_PADDING) / 2,
        }
      : { x: 0, y: 0 };

    const object = document.createElement("div");
    object.className = "diagram-t1";
    object.style.left = `${center.x - width / 2}px`;
    object.style.top = `${center.y - height / 2}px`;
    object.appendChild(svg);
    diagramsEl.appendChild(object);
    paintedSvg += 1;

    // The frontend answers after two animation frames: the SVG is painted
    // (PRD §8.6.1 step 4, `plan.js:2039-2044`). Rust waits
    // `RENDER_ACK_TIMEOUT_MS` (2000 ms) for this.
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        emit(EVENTS.renderAck, { req_id: payload.req_id, ok: true });
      });
    });
  }

  onMount(() => {
    // Every listener is registered before `graphium://ready` leaves, or the
    // burst Rust answers with (`graphium://settings` and `graphium://state`,
    // `app.rs:803-810`) would reach nobody. Same order as `plan.js:2332-2449`.
    listen(EVENTS.settings, (event) => {
      settings = settingsFromPayload(event.payload);
    });
    listen(EVENTS.state, (event) => {
      const payload = event.payload || {};
      openDiagrams = Array.isArray(payload.diagrams) ? payload.diagrams.length : 0;
    });
    listen(EVENTS.render, (event) => {
      mountRender(event.payload || {});
    });

    // The camera copy of Rust starts at the identity: the page pushes it once
    // so the owner never places a diagram against a camera it did not read
    // (`app.rs:727`, `plan.js:404-412`).
    emit(EVENTS.cameraChanged, { ...camera });

    // The handshake (`plan.js:2449`): Rust marks the page alive and answers
    // with the settings and the state. Without it the first render waits
    // `READY_TIMEOUT_MS` and ends as `ui_error` (`app.rs:23`, `app.rs:1089`).
    emit(EVENTS.ready, {});
  });
</script>

<!-- The plan: an infinite surface with a grid. The camera lives here
     (PRD §7.1, `index.html:11-13`). -->
<div id="plan" class="plan" bind:this={planEl}>
  <div class="diagrams" bind:this={diagramsEl}></div>
</div>

<!-- State 1 of the vanilla page (`index.html:67-74`): the surface says what to
     do with it instead of waiting for an agent that draws somewhere else. -->
{#if openDiagrams === 0}
  <div class="empty">
    <p class="empty-title">esperando al agente…</p>
    <p class="empty-hint">
      conectá un cliente MCP y corré <code>graphium mcp-config</code>
    </p>
  </div>
{/if}

<!-- Scaffold diagnostic, not part of the parity contract: it only shows that
     the handshake completed (`state`), that the settings arrived and how many
     drawings this page mounted. T4 removes it when the panels land. -->
<div class="scaffold" aria-live="polite">
  svelte · {settings ? themeOf(settings) : "sin ajustes"} · {openDiagrams} diagrama{openDiagrams === 1 ? "" : "s"} ·
  {paintedSvg} pintado{paintedSvg === 1 ? "" : "s"}{hasBridge() ? "" : " · sin puente"}
</div>
