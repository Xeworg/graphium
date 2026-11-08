<svelte:options namespace="svg" />

<script>
  // One node of the drawing (`f11-elementos.md`, T2; S3-S5).
  //
  // The shape is drawn from the numbers the paint plan carries (the measured
  // box, S4) and the frozen tokens (the rim of a cylinder, the fold of a note,
  // the diamond of a decision — S5). A node with rows is a table: a header, a
  // rule and one line per row with its key badge, the table
  // `write_node_table` writes.
  //
  // The classes are the classes of `renderer.rs` (`node-fill node-stroke
  // shape-*`), so `src/app.css` paints this node with the same rules and the
  // same CSS custom properties as the exported string: no colour is written
  // here (S3).
  import SceneText from "./SceneText.svelte";

  let { paint } = $props();
</script>

{#if paint.cylinder !== null}
  <path id={paint.id} class={paint.classes} d={paint.cylinder.body} />
  <path
    class="node-stroke shape-cylinder"
    fill="none"
    d={paint.cylinder.rimPath}
  />
{:else if paint.diamond !== null}
  <polygon id={paint.id} class={paint.classes} points={paint.diamond} />
{:else if paint.shape === "ellipse"}
  <ellipse
    id={paint.id}
    class={paint.classes}
    cx={paint.center.x}
    cy={paint.center.y}
    rx={paint.rect.w / 2}
    ry={paint.rect.h / 2}
  />
{:else if paint.note !== null}
  <path id={paint.id} class={paint.classes} d={paint.note.body} />
  <path class="node-stroke shape-note" fill="none" d={paint.note.foldPath} />
{:else}
  <rect
    id={paint.id}
    class={paint.classes}
    x={paint.rect.x}
    y={paint.rect.y}
    width={paint.rect.w}
    height={paint.rect.h}
    rx={paint.rect.radius ?? undefined}
    ry={paint.rect.radius ?? undefined}
  />
{/if}

{#if paint.isTable}
  <!-- The header, centered in the first line of the box. -->
  <SceneText paint={paint.header} />
  <!-- The rule that separates the header from the rows. -->
  <line
    class="node-stroke"
    x1={paint.rule.x1}
    y1={paint.rule.y1}
    x2={paint.rule.x2}
    y2={paint.rule.y2}
    stroke-width={paint.rule.width}
  />
  <!-- One line per row: the key role on the left, the name, the note right. -->
  {#each paint.rows as row, index (index)}
    <SceneText paint={row.badge} />
    <SceneText paint={row.name} />
    {#if row.note !== null}
      <SceneText paint={row.note} />
    {/if}
  {/each}
{:else if paint.label !== null}
  <SceneText paint={paint.label} />
{/if}
