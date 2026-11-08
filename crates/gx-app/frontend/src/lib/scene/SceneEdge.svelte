<svelte:options namespace="svg" />

<script>
  // One routed edge of the drawing (`f11-elementos.md`, T2; S3, S5).
  //
  // The route is the polyline the layout produced, drawn as a `points`
  // attribute (geometry in attributes, never in a style string). The arrowhead
  // is a `<marker>` of `DiagramScene` and the crow's foot is drawn here from
  // the tokens: the kick of a relation is a glyph of the painter, not a path
  // shipped in the payload (S5).
  import SceneText from "./SceneText.svelte";

  let { paint } = $props();
</script>

<polyline
  id={paint.id}
  class={paint.classes}
  points={paint.points}
  marker-end={paint.marker ?? undefined}
/>

{#each paint.feet as foot, index (index)}
  {#if foot.kind === "many"}
    <path class={foot.classes} d={foot.path} />
  {:else}
    <line
      class={foot.classes}
      x1={foot.from.x}
      y1={foot.from.y}
      x2={foot.to.x}
      y2={foot.to.y}
    />
  {/if}
  {#if foot.circle !== null}
    <circle
      class={foot.classes}
      cx={foot.circle.cx}
      cy={foot.circle.cy}
      r={foot.circle.r}
    />
  {/if}
{/each}

{#if paint.label !== null}
  <SceneText paint={paint.label} />
{/if}
