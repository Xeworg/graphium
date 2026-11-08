<script>
  // The scene of one drawing (`f11-elementos.md`, T2; S3-S7).
  //
  // This is the element library the plan mounts instead of the SVG string: a
  // real `<svg class="graphium">` whose children are the elements of the scene,
  // each keyed by the id of the drawing so an update moves elements instead of
  // rebuilding a string. The layer order is the layer order of `renderer.rs` —
  // the containers of a graph, the lanes of a timeline, the lifelines of a
  // sequence, the edges and then the nodes and the events — so the export and
  // the window stack the drawing the same way.
  //
  // What the scene does not do: it measures nothing (the boxes arrive measured,
  // S4), it writes no colour (the classes of the drawing and the CSS custom
  // properties of `src/app.css` paint it, S3) and it ships no glyph as a path
  // (the crow's foot, the rim, the fold, the diamond and the markers are drawn
  // by the components from the tokens, S5).
  //
  // The paint plan is computed once per scene (`glyph.js`, `diagramPaint`) and
  // handed to the components as plain data: one reactive read for ~900
  // components, instead of one signal set per component (see L5 of the feature
  // doc for the measurement that decided this).
  //
  // Interaction (T4/D) is three ids and three callback props — no dispatcher,
  // no slots: the elements of the drawing surface their hover, their focus and
  // their click, and `App.svelte` holds the state. `selected` is the element
  // the human chose, `hovered`/`focused` are the transient ones, and the
  // highlight is the `emphasis-highlight` class the drawing already carries.
  // The callbacks take the id of the drawing (`owner`) first and the id of the
  // element second, and they are handed down unchanged: a stable function is a
  // prop that never changes, so a pointer moving over a drawing never
  // re-creates the props of its elements.
  import SceneBand from "./SceneBand.svelte";
  import SceneEdge from "./SceneEdge.svelte";
  import SceneNode from "./SceneNode.svelte";
  import { diagramPaint } from "./glyph.js";

  let {
    scene,
    owner = "",
    selected = null,
    hovered = null,
    focused = null,
    onhover = () => {},
    onfocus = () => {},
    onselect = () => {},
  } = $props();

  const paint = $derived(diagramPaint(scene));

  /** Whether an element of this drawing wears the highlight. */
  const isHot = (id) => id === selected || id === hovered || id === focused;
</script>

<svg
  class="graphium"
  viewBox="{paint.canvas.x} {paint.canvas.y} {paint.canvas.w} {paint.canvas.h}"
  width={paint.canvas.w}
  height={paint.canvas.h}
>
  <!-- The arrowheads, only the ones an edge references and sorted by id, like
       the `<defs>` of the emitter. -->
  <defs>
    {#each paint.markers as marker (marker.id)}
      <marker
        id={marker.id}
        viewBox="0 0 {marker.length} {marker.length}"
        refX={marker.length}
        refY={marker.half}
        markerWidth={marker.length}
        markerHeight={marker.length}
        markerUnits="userSpaceOnUse"
        orient="auto"
      >
        <path
          class={marker.id}
          d="M 0 0 L {marker.length} {marker.half} L 0 {marker.length} Z"
        />
      </marker>
    {/each}
  </defs>

  {#if paint.clusters.length > 0}
    <g class="layer-groups">
      {#each paint.clusters as band (band.id)}
        <SceneBand paint={band} />
      {/each}
    </g>
  {/if}

  {#if paint.lanes.length > 0}
    <g class="layer-lanes">
      {#each paint.lanes as band (band.id)}
        <SceneBand paint={band} />
      {/each}
    </g>
  {/if}

  {#if paint.lifelines.length > 0}
    <g class="layer-lifelines">
      {#each paint.lifelines as line, index (index)}
        <line class="lifeline" x1={line.x} y1={line.y1} x2={line.x} y2={line.y2} />
      {/each}
    </g>
  {/if}

  <g class="layer-edges">
    {#each paint.edges as edge (edge.id)}
      <SceneEdge
        paint={edge}
        {owner}
        hot={isHot(edge.id)}
        pressed={edge.id === selected}
        {onhover}
        {onfocus}
        {onselect}
      />
    {/each}
  </g>

  <g class="layer-nodes">
    {#each paint.nodes as node (node.id)}
      <SceneNode
        paint={node}
        {owner}
        hot={isHot(node.id)}
        pressed={node.id === selected}
        {onhover}
        {onfocus}
        {onselect}
      />
    {/each}
  </g>
</svg>
