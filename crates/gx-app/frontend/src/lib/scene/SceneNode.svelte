<svelte:options namespace="svg" />

<script>
  // One node of the drawing (`f11-elementos.md`, T2; S3-S5).
  //
  // The shape is drawn from the numbers the paint plan carries (the measured
  // box, S4) and the frozen tokens (the rim of a cylinder, the fold of a note,
  // the diamond of a decision — S5). A node with rows is a table: a header, a
  // rule and one line per row with its key badge, the table
  // `write_node_table` writes. An event that lasts is a bar under its box
  // (`f11-elementos.md` T4/D): the rectangle travels in the plan.
  //
  // The classes are the classes of `renderer.rs` (`node-fill node-stroke
  // shape-*`), so `src/app.css` paints this node with the same rules and the
  // same CSS custom properties as the exported string: no colour is written
  // here (S3).
  //
  // Interaction (T4/D) is the element itself and callback props: the shape is
  // the hit target and the one tab stop of the element, and the hover, the
  // focus and the click travel up to `App.svelte`, which holds the state. The
  // labels are not hit targets (`src/app.css` sets `pointer-events: none` on
  // them), so the pointer never leaves the shape while it walks over the text.
  import SceneText from "./SceneText.svelte";

  /**
   * @type {{
   *   paint: any,
   *   owner?: string,
   *   hot?: boolean,
   *   pressed?: boolean,
   *   onhover?: (owner: string, id: string | null) => void,
   *   onfocus?: (owner: string, id: string | null) => void,
   *   onselect?: (owner: string, id: string) => void,
   * }}
   */
  let {
    paint,
    owner = "",
    hot = false,
    pressed = false,
    onhover = () => {},
    onfocus = () => {},
    onselect = () => {},
  } = $props();

  /** The element itself is the hit target: it is one node, not a group. */
  const hit = $derived({
    "data-element": paint.id,
    role: "button",
    tabindex: "0",
    "aria-label": paint.name,
    "aria-pressed": pressed ? "true" : "false",
    onpointerenter: () => onhover(owner, paint.id),
    onpointerleave: () => onhover(owner, null),
    onfocus: () => onfocus(owner, paint.id),
    onblur: () => onfocus(owner, null),
    onclick: () => onselect(owner, paint.id),
    onkeydown: (event) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        onselect(owner, paint.id);
      }
    },
  });

  /**
   * The highlight: the classes of the drawing, plus the emphasis it means. An
   * element the kind already highlighted is left as it is — the class is the
   * same one, and half the IR is already accent.
   */
  const shapeClasses = $derived(
    hot && !paint.classes.includes("emphasis-highlight")
      ? `${paint.classes} emphasis-highlight`
      : paint.classes,
  );
</script>

{#if paint.cylinder !== null}
  <path id={paint.id} class={shapeClasses} d={paint.cylinder.body} {...hit} />
  <path
    class="node-stroke shape-cylinder"
    fill="none"
    d={paint.cylinder.rimPath}
  />
{:else if paint.diamond !== null}
  <polygon id={paint.id} class={shapeClasses} points={paint.diamond} {...hit} />
{:else if paint.shape === "ellipse"}
  <ellipse
    id={paint.id}
    class={shapeClasses}
    cx={paint.center.x}
    cy={paint.center.y}
    rx={paint.rect.w / 2}
    ry={paint.rect.h / 2}
    {...hit}
  />
{:else if paint.note !== null}
  <path id={paint.id} class={shapeClasses} d={paint.note.body} {...hit} />
  <path class="node-stroke shape-note" fill="none" d={paint.note.foldPath} />
{:else}
  <rect
    id={paint.id}
    class={shapeClasses}
    x={paint.rect.x}
    y={paint.rect.y}
    width={paint.rect.w}
    height={paint.rect.h}
    rx={paint.rect.radius ?? undefined}
    ry={paint.rect.radius ?? undefined}
    {...hit}
  />
{/if}

{#if paint.bar !== null}
  <!-- The bar of an event that lasts: the rectangle the scene carries (T4/D). -->
  <rect
    class="accent"
    x={paint.bar.x}
    y={paint.bar.y}
    width={paint.bar.w}
    height={paint.bar.h}
  />
{/if}

{#if paint.isTable}
  <!-- The header, centered in the first line of the box. -->
  <SceneText paint={paint.header} hot={hot} />
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
    <SceneText paint={row.badge} hot={hot} />
    <SceneText paint={row.name} hot={hot} />
    {#if row.note !== null}
      <SceneText paint={row.note} hot={hot} />
    {/if}
  {/each}
{:else if paint.label !== null}
  <SceneText paint={paint.label} hot={hot} />
{/if}
