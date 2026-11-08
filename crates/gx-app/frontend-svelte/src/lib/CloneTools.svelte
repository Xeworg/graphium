<script>
  // The clone of the user plan and its tools (B1, B2, B3 of `f8-planos.md`):
  // the `◀ 2/2 ▶` history of the original, «Actualizar el clon» and the `×`
  // that removes the view instance. Ported from
  // `crates/gx-app/frontend/plan.js:1510-1877` (`createCloneTools`,
  // `updateCloneTools`, `fitCloneTools`, the `×` of the clone).
  //
  // One compact row that never wraps (L8/L9): the pill grows inward from the
  // corner of its object and, when the object is narrower than the pill, it
  // hangs centered over the top edge (`.overhang`) — the minimal symmetric
  // overflow that keeps every control reachable. The `×` lives INSIDE the pill
  // (L8) and joins the row (L10).
  import { cloneVersionAt, cloneVersionLabel } from "./history.js";

  let { clone = null, viewed = undefined, width = 0, hidden = false, onstep, onrefresh, onremove } =
    $props();

  const count = $derived(Number(clone?.version_count || 0));
  const oldest = $derived(Number(clone?.oldest_version || 0));
  const current = $derived(Number(clone?.current_version || oldest));
  const updatable = $derived(Boolean(clone && clone.updatable));
  const at = $derived(clone === null ? current : cloneVersionAt(clone, viewed));

  let pillEl = $state(null);
  let overhang = $state(false);

  // The pill is measured against the rectangle of its object (`.content` is the
  // object, in plan units): it grows inward while it fits and centers over the
  // top edge when the object is narrower than the pill itself.
  $effect(() => {
    // Dependencies: the object's width, whether it is on screen at all, and
    // everything that changes the width of the pill.
    void width;
    void hidden;
    void count;
    void oldest;
    void current;
    void updatable;
    if (pillEl === null) {
      return;
    }
    const objectWidth = pillEl.parentElement?.clientWidth ?? 0;
    if (objectWidth === 0) {
      // A hidden object has no layout to measure: keep the last decision.
      return;
    }
    overhang = pillEl.offsetWidth > objectWidth;
  });
</script>

<div class="diagram-clone-tools" class:overhang bind:this={pillEl}>
  <!-- A version the history discarded cannot be browsed: the indicator falls
       back to the newest one the owner can draw (RF14, L5). -->
  <div class="panel-item-versions clone-versions" class:hidden={!(count > 1)}>
    <button
      type="button"
      class="panel-item-version-step"
      aria-label="versión anterior del original"
      disabled={!updatable || at <= oldest}
      onpointerdown={(event) => event.stopPropagation()}
      onclick={() => onstep?.(-1)}>◀</button
    >
    <span class="panel-item-version-label">{cloneVersionLabel(clone, viewed)}</span>
    <button
      type="button"
      class="panel-item-version-step"
      aria-label="versión siguiente del original"
      disabled={!updatable || at >= current}
      onpointerdown={(event) => event.stopPropagation()}
      onclick={() => onstep?.(1)}>▶</button
    >
  </div>

  <button
    type="button"
    class="clone-refresh"
    aria-label="actualizar el clon"
    title={updatable ? "actualizar el clon" : "el original ya no está en el plan"}
    disabled={!updatable}
    onpointerdown={(event) => event.stopPropagation()}
    onclick={() => onrefresh?.()}>↻</button
  >

  <button
    type="button"
    class="diagram-close clone-close"
    aria-label="cerrar el clon"
    title="cerrar el clon"
    onpointerdown={(event) => event.stopPropagation()}
    onclick={() => onremove?.()}>×</button
  >
</div>
