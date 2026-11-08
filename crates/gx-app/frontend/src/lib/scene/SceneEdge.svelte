<svelte:options namespace="svg" />

<script>
  // One routed edge of the drawing (`f11-elementos.md`, T2; S3, S5).
  //
  // The route is the polyline the layout produced, drawn as a `points`
  // attribute (geometry in attributes, never in a style string). The arrowhead
  // is a `<marker>` of `DiagramScene` and the crow's foot is drawn here from
  // the tokens: the kick of a relation is a glyph of the painter, not a path
  // shipped in the payload (S5).
  //
  // Interaction (T4/D): the line is the element — its hit target is the stroke
  // the emitter draws, so a longer route is easier to hover and a `1 px` line
  // asks for a steady hand. The crow's foot and the label are decoration: the
  // classes and the numbers, never a handler of their own.
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

  /** The highlight, like a node: the class of the drawing, never twice. */
  const classes = $derived(
    hot && !paint.classes.includes("emphasis-highlight")
      ? `${paint.classes} emphasis-highlight`
      : paint.classes,
  );
</script>

<polyline
  id={paint.id}
  class={classes}
  points={paint.points}
  marker-end={paint.marker ?? undefined}
  {...hit}
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
  <SceneText paint={paint.label} hot={hot} />
{/if}
