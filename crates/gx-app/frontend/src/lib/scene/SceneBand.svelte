<svelte:options namespace="svg" />

<script>
  // One band of the drawing (`f11-elementos.md`, T2; S3, S5).
  //
  // A band is one of the two things the IR already names: the container of a
  // graph (`cluster-<id>`, the dashed boundary of a component diagram) and the
  // lane of a timeline (`lane-<id>`, the row an event belongs to). Where its
  // name sits — the corner of a container, the gutter of a lane — is a rule of
  // the emitter and arrives decided in the paint plan.
  import SceneText from "./SceneText.svelte";

  let { paint } = $props();
</script>

{#if paint.isLane}
  <rect
    id={paint.id}
    class="lane-band"
    x={paint.box.x}
    y={paint.box.y}
    width={paint.box.w}
    height={paint.box.h}
  />
{:else}
  <rect
    id={paint.id}
    class="group-box"
    x={paint.box.x}
    y={paint.box.y}
    width={paint.box.w}
    height={paint.box.h}
    rx={paint.radius ?? undefined}
    ry={paint.radius ?? undefined}
  />
{/if}

{#if paint.label !== null}
  <SceneText paint={paint.label} />
{/if}
