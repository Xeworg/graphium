<svelte:options namespace="svg" />

<script>
  // One text element of the drawing (`f11-elementos.md`, T2; S4).
  //
  // The lines arrive pre-broken (the `\n` of the payload is the wrap) and the
  // baselines arrive computed (`glyph.js`, the arithmetic of `geometry.rs`), so
  // this component only spells them: one `<text>` for one line, one `<tspan>`
  // per line inside a single `<text>` for many — exactly what the emitter
  // writes. Nothing here measures (S4) and nothing here colours (S3).
  //
  // `hot` is the highlight of the element it names (`f11-elementos.md`, T4/D):
  // the class is the one of the drawing (`emphasis-highlight-text`), so a
  // selected or hovered element takes its colour from the same rule the
  // emphasis uses.
  let { paint, hot = false } = $props();

  const classes = $derived(
    hot && !paint.classes.includes("emphasis-highlight-text")
      ? `${paint.classes} emphasis-highlight-text`
      : paint.classes,
  );
</script>

{#if paint.lines.length === 1}
  <text
    class={classes}
    x={paint.x}
    y={paint.baselines[0]}
    font-size={paint.size}
    text-anchor={paint.anchor ?? undefined}
    dominant-baseline={paint.dominantBaseline}
    font-weight={paint.weight ?? undefined}>{paint.lines[0]}</text
  >
{:else if paint.lines.length > 1}
  <text
    class={classes}
    font-size={paint.size}
    text-anchor={paint.anchor ?? undefined}
    dominant-baseline={paint.dominantBaseline}
    font-weight={paint.weight ?? undefined}
  >
    {#each paint.lines as line, index (index)}
      <tspan x={paint.x} y={paint.baselines[index]}>{line}</tspan>
    {/each}
  </text>
{/if}
