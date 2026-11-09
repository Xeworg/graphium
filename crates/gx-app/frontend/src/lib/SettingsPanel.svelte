<script>
  // The settings of the plan (PRD §7.1 RF25, §8.5.2 RF22, S19): the gear in the
  // top-right corner and the panel it opens. Ported from
  // `crates/gx-app/frontend/plan.js:639-833` and `index.html:105-248`.
  //
  // The look belongs to the human: the panel reads and writes exactly the 13
  // keys of `Settings` (CONTRACT.md §5.6) and never invents a value. Every
  // input change paints the plan at once (`onchange` repaints the grid and the
  // palette) and asks Rust to save it, throttled and deduplicated by the app;
  // until the first `graphium://settings` arrives the controls are inert
  // (C6, C38).
  //
  // The theme fieldset is the one selector of the look: seven chips, each with
  // its name and the four colours its preset paints (`PLAN_PRESETS`, via
  // `THEME_CHIPS`), the same table Rust keeps (`settings::presets`).
  import { THEME_CHIPS } from "./presets.js";

  let { settings = null, onchange } = $props();

  // The six grid/palette pickers plus the six colour overrides, in the order
  // and with the ids of the vanilla panel (`index.html:129-209`). The ids are
  // the vanilla ones on purpose: T4 diffs them.
  const FIELDS = [
    { key: "background", id: "settings-background", label: "fondo", type: "color" },
    { key: "grid_color", id: "settings-grid-color", label: "grilla", type: "color" },
    {
      key: "grid_opacity",
      id: "settings-grid-opacity",
      label: "opacidad",
      type: "range",
      min: 0,
      max: 1,
      step: 0.01,
    },
    {
      key: "grid_diagonal",
      id: "settings-grid-diagonal",
      label: "separación",
      type: "range",
      min: 20,
      max: 200,
      step: 1,
    },
    {
      key: "grid_angle_degrees",
      id: "settings-grid-angle",
      label: "ángulo",
      type: "range",
      min: 15,
      max: 75,
      step: 1,
    },
    { key: "grid_visible", id: "settings-grid-visible", label: "mostrar grilla", type: "checkbox" },
    { key: "node_fill", id: "settings-node-fill", label: "nodos", type: "color" },
    { key: "node_stroke", id: "settings-node-stroke", label: "borde", type: "color" },
    { key: "node_text", id: "settings-node-text", label: "texto de nodo", type: "color" },
    { key: "edge_stroke", id: "settings-edge-stroke", label: "aristas", type: "color" },
    { key: "edge_text", id: "settings-edge-text", label: "texto de arista", type: "color" },
    { key: "accent", id: "settings-accent", label: "acento", type: "color" },
  ];

  // The panel of the settings starts closed (`index.html:105-126`).
  let open = $state(false);

  // What the controls show. It starts with the `plano` preset so the controls
  // are usable before the first settings, but nothing is sent until Rust has
  // spoken (C6).
  function emptyDraft() {
    return {
      accent: "#6ea8fe",
      background: "#1b1b1b",
      edge_stroke: "#8a8a96",
      edge_text: "#b9b9c2",
      grid_angle_degrees: 45,
      grid_color: "#ffffff",
      grid_diagonal: 59,
      grid_opacity: 0.14,
      grid_visible: true,
      node_fill: "#2a2a30",
      node_stroke: "#4d4d57",
      node_text: "#e9e9ec",
      theme: "dark",
    };
  }

  let draft = $state(emptyDraft());
  let panelEl = $state(null);
  let toggleEl = $state(null);
  // The controls themselves, by settings key: opening the panel focuses the
  // first one, the way the vanilla page does (`plan.js:791-793`).
  const fieldEls = {};

  // Shows what Rust says the file holds (plan.js:700-718, `fillPanel`). The
  // app only forwards a `graphium://settings` that is not the confirmation of a
  // write in flight, so a slider is never pulled out of the hand that drags it
  // (R1, plan.js:2439-2442).
  $effect(() => {
    const next = settings;
    if (!next) {
      return;
    }
    draft = { ...next };
  });

  /** The settings the controls show right now (plan.js:669-686, `readPanel`). */
  function readDraft() {
    return {
      accent: draft.accent,
      background: draft.background,
      edge_stroke: draft.edge_stroke,
      edge_text: draft.edge_text,
      grid_angle_degrees: Number(draft.grid_angle_degrees),
      grid_color: draft.grid_color,
      grid_diagonal: Number(draft.grid_diagonal),
      grid_opacity: Number(draft.grid_opacity),
      grid_visible: Boolean(draft.grid_visible),
      node_fill: draft.node_fill,
      node_stroke: draft.node_stroke,
      node_text: draft.node_text,
      theme: draft.theme,
    };
  }

  /** A control moved: the plan repaints and Rust is told (plan.js:775-786). */
  function onInput() {
    if (settings === null) {
      // Nothing has arrived from Rust yet: the controls are not a value the
      // human chose, and a half-filled object must never reach the file.
      return;
    }
    onchange?.(readDraft());
  }

  function onField(event, field) {
    const input = event.currentTarget;
    if (field.type === "range") {
      draft[field.key] = Number(input.value);
    } else if (field.type === "checkbox") {
      draft[field.key] = Boolean(input.checked);
    } else {
      draft[field.key] = String(input.value);
    }
    onInput();
  }

  function onTheme(event) {
    draft.theme = String(event.currentTarget.value);
    onInput();
  }

  /** The number beside a slider, so the human sees what they chose. */
  function valueText(field) {
    const value = Number(draft[field.key]);
    if (!Number.isFinite(value)) {
      return "";
    }
    if (field.key === "grid_opacity") {
      return value.toFixed(2);
    }
    if (field.key === "grid_diagonal") {
      return `${Math.round(value)}px`;
    }
    if (field.key === "grid_angle_degrees") {
      return `${Math.round(value)}°`;
    }
    return "";
  }

  function close() {
    open = false;
  }

  // The keyboard goes back to where it came from (plan.js:790-800): whether the
  // focus was inside the panel is read *before* the panel hides.
  //
  // T3 of `odd/tasks/mejoras-frontend.md` — the decision the task asked for
  // (trap the dialog or drop the role): the panel is NOT a modal dialog and it
  // does not become one. It floats over a plan the human keeps using while it
  // is open — a slider of the panel repaints that plan live, and the diagram
  // panel and the switcher stay reachable — so trapping Tab inside it would be
  // a keyboard trap in a panel that blocks nothing. `role="dialog"` promised a
  // modality the panel never had; the panel is a labelled `group` and the
  // disclosure grammar lives on the gear (`aria-expanded` + `aria-controls`),
  // with Escape closing it and the focus coming back to the gear. What the
  // rejected half would have required (a focus trap) is written down in
  // `odd/tasks/mejoras-frontend.md` (L6).
  let wasInside = false;
  $effect.pre(() => {
    if (!open) {
      wasInside = panelEl !== null && panelEl.contains(document.activeElement);
    }
  });
  $effect(() => {
    if (open) {
      fieldEls.background?.focus();
    } else if (wasInside) {
      wasInside = false;
      toggleEl?.focus();
    }
  });
</script>

<button
  id="settings-toggle"
  class="settings-toggle"
  type="button"
  aria-label="ajustes del plano"
  aria-expanded={open}
  aria-controls="settings-panel"
  title="ajustes del plano"
  bind:this={toggleEl}
  onclick={() => (open = !open)}>⚙</button
>

<!-- The Escape of the panel listens here, on the panel as a whole, and a
     `group` is not an interactive element: the keyboard never focuses the panel
     itself, it only bubbles through it. `svelte-ignore` says so on purpose. -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  id="settings-panel"
  class="settings-panel"
  class:hidden={!open}
  role="group"
  aria-label="ajustes del plano"
  bind:this={panelEl}
  onkeydown={(event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      close();
    }
  }}
>
  <div class="settings-head">
    <h2 class="settings-title">ajustes del plano</h2>
    <button
      id="settings-close"
      class="settings-close"
      type="button"
      aria-label="cerrar los ajustes"
      onclick={close}>×</button
    >
  </div>

  {#each FIELDS as field (field.key)}
    <div class="settings-field">
      <label for={field.id}>{field.label}</label>
      {#if field.type === "range"}
        <input
          id={field.id}
          type="range"
          min={field.min}
          max={field.max}
          step={field.step}
          value={draft[field.key]}
          bind:this={fieldEls[field.key]}
          oninput={(event) => onField(event, field)}
          onchange={(event) => onField(event, field)}
        />
        <output id={`${field.id}-value`} class="settings-value" for={field.id}
          >{valueText(field)}</output
        >
      {:else if field.type === "checkbox"}
        <input
          id={field.id}
          type="checkbox"
          checked={Boolean(draft[field.key])}
          bind:this={fieldEls[field.key]}
          oninput={(event) => onField(event, field)}
          onchange={(event) => onField(event, field)}
        />
        <span class="settings-value"></span>
      {:else}
        <input
          id={field.id}
          type="color"
          value={draft[field.key]}
          bind:this={fieldEls[field.key]}
          oninput={(event) => onField(event, field)}
          onchange={(event) => onField(event, field)}
        />
        <span class="settings-value"></span>
      {/if}
    </div>
  {/each}

  <!-- The one selector of the plan (RF22, `f6-themes.md` R5): every theme is a
       chip with its name and a small palette beside it. `sin grilla` is not a
       look — it is the grid of `plano` turned off — so it stays a preset. -->
  <fieldset class="settings-theme">
    <legend>tema</legend>
    {#each THEME_CHIPS as chip (chip.value)}
      <label class="settings-radio">
        <input
          type="radio"
          name="settings-theme"
          value={chip.value}
          checked={draft.theme === chip.value}
          onchange={onTheme}
        />
        {chip.label}
        <span class="swatches" aria-hidden="true">
          {#each chip.swatches as color, index (index)}
            <span class="swatch" style:background={color}></span>
          {/each}
        </span>
      </label>
    {/each}
  </fieldset>
</div>
