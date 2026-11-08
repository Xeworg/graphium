<script>
  // The panel of the open diagrams (PRD §7 RF26, RF28, S1): the toggle in the
  // top-left corner and the list it opens. Ported from
  // `crates/gx-app/frontend/plan.js:836-1006` and `index.html:21-35`.
  //
  // It is built only from `graphium://state` (CONTRACT.md §8, C22/C23/C24):
  // the list is the `diagrams` prop, one entry per open diagram, with its
  // title, its kind, the moment of its last change, its `×`, its `⧉` and — when
  // the store keeps more than one version — the `◀ v3/7 ▶` of its history. The
  // page never invents an entry and never closes one by itself: the buttons ask
  // Rust and the state that comes back is what the panel shows.
  import { formatUpdated } from "./labels.js";
  import { versionAt, versionLabel } from "./history.js";

  let {
    diagrams = [],
    selectedId = null,
    viewed = {},
    onselect,
    onclose,
    onclone,
    onversion,
  } = $props();

  // The panel of the diagrams starts open, like the vanilla page
  // (`index.html:21-35`: `aria-expanded="true"`).
  let open = $state(true);
</script>

<button
  id="panel-toggle"
  class="panel-toggle"
  type="button"
  aria-label="mostrar u ocultar los diagramas abiertos"
  aria-expanded={open}
  aria-controls="panel"
  title="diagramas abiertos"
  onclick={() => (open = !open)}>☰</button
>

<aside id="panel" class="panel" class:hidden={!open} aria-label="diagramas abiertos">
  <h2 class="panel-title">diagramas abiertos</h2>
  <ul id="panel-list" class="panel-list">
    {#each diagrams as diagram (diagram.diagram_id)}
      {@const chosen = diagram.diagram_id === selectedId}
      {@const at = versionAt(diagram, viewed[diagram.diagram_id])}
      <li class="panel-item" class:selected={chosen}>
        <button
          type="button"
          class="panel-item-open"
          aria-current={chosen ? "true" : "false"}
          title={`ir a ${diagram.title}`}
          onclick={() => onselect?.(diagram)}
        >
          <span class="panel-item-title">{diagram.title}</span>
          <span class="panel-item-meta">
            <span class="panel-item-kind">{diagram.kind}</span>
            <span class="panel-item-updated">{formatUpdated(diagram.updated_at)}</span>
          </span>
        </button>

        <!-- Cloning onto the user plan (B1 of `f8-planos.md`, S2): the same
             grammar as the `×`, and it only asks — a clone never becomes a
             second `diagram_id` of the store (S4). -->
        <button
          type="button"
          class="panel-item-clone"
          aria-label={`clonar ${diagram.title} al plano del usuario`}
          title="clonar al plano del usuario"
          onclick={() => onclone?.(diagram.diagram_id)}>⧉</button
        >
        <button
          type="button"
          class="panel-item-close"
          aria-label={`cerrar ${diagram.title}`}
          title={`cerrar ${diagram.title}`}
          onclick={() => onclose?.(diagram.diagram_id)}>×</button
        >

        <!-- The history of the diagram (RF14, L5): «◀ v3/7 ▶» when the store
             keeps more than one version. The newest restores the live drawing
             the page already holds; walking back asks Rust for a read-only
             render. -->
        {#if diagram.version_count > 1}
          <div class="panel-item-versions">
            <button
              type="button"
              class="panel-item-version-step"
              aria-label={`versión anterior de ${diagram.title}`}
              disabled={at <= diagram.oldest_version}
              onclick={() => onversion?.(diagram, -1)}>◀</button
            >
            <span class="panel-item-version-label">{versionLabel(diagram, viewed[diagram.diagram_id])}</span>
            <button
              type="button"
              class="panel-item-version-step"
              aria-label={`versión siguiente de ${diagram.title}`}
              disabled={at >= diagram.current_version}
              onclick={() => onversion?.(diagram, 1)}>▶</button
            >
          </div>
        {/if}
      </li>
    {/each}
  </ul>
  <p id="panel-empty" class="panel-empty" class:hidden={diagrams.length > 0}>
    sin diagramas abiertos
  </p>
</aside>
