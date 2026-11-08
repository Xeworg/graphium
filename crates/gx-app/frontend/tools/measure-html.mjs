#!/usr/bin/env node
// The T2 measurement (odd/tasks/mejoras-frontend.md): the cost of re-parsing
// the drawing markup on every mount (`markup` + `{@html}`) against the mount
// the current action does (the same DOM node moved back into the page, no
// parse), over the 300-node fixture.
//
// The decision it feeds: `{@html}` is adopted when one re-parse of the
// fixture stays at or under the agreed ~3 ms threshold; otherwise the action
// stays and this script is the record of why.
//
// It runs the built page (`npm run build` first) in headless Chromium over
// HTTP with the faked Tauri bridge — the same engine and clock
// `bench.mjs`/`parity.mjs` measure under.
//
//   node tools/measure-html.mjs [--nodes=300] [--repeats=50]

import { launch, evaluate, openPage, serve } from "./lib/harness.mjs";

const argv = process.argv.slice(2);
const option = (name, fallback) => {
  const hit = argv.find((argument) => argument.startsWith(`--${name}=`));
  return hit === undefined ? fallback : hit.slice(name.length + 3);
};
const NODES = Number(option("nodes", "300"));
const REPEATS = Number(option("repeats", "50"));
const CHROMIUM = option("chromium", process.env.CHROMIUM || "chromium-browser");

const HERE = new URL(".", import.meta.url).pathname;

/** The same fixture shape `bench.mjs` measures: rect+text per node, one line
 * per edge. Inlined here so this script stays standalone. */
function drawingSvg(count, { width = 4000, height = 3000 } = {}) {
  const perRow = Math.ceil(Math.sqrt(count));
  const stepX = Math.floor((width - 400) / perRow);
  const stepY = Math.floor((height - 300) / perRow);
  const nodeWidth = 220;
  const nodeHeight = 80;
  const nodes = [];
  for (let index = 0; index < count; index += 1) {
    const column = index % perRow;
    const row = Math.floor(index / perRow);
    const x = 200 + column * stepX;
    const y = 150 + row * stepY;
    nodes.push(
      `<g class="node"><rect x="${x}" y="${y}" width="${nodeWidth}" height="${nodeHeight}" rx="10"/><text x="${x + 12}" y="${y + 46}">nodo ${index}</text></g>`,
    );
  }
  const edges = [];
  for (let index = 1; index < count; index += 1) {
    const to = nodes[index - 1].match(/x="(\d+)" y="(\d+)"/);
    const at = nodes[index].match(/x="(\d+)" y="(\d+)"/);
    edges.push(
      `<line x1="${Number(to[1]) + nodeWidth}" y1="${Number(to[2]) + nodeHeight / 2}" x2="${at[1]}" y2="${Number(at[2]) + nodeHeight / 2}"/>`,
    );
  }
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" class="graphium">${edges.join("")}${nodes.join("")}</svg>`;
}

const svg = drawingSvg(NODES);
const server = await serve(new URL("../dist", import.meta.url).pathname);
const browser = await launch(CHROMIUM);
try {
  const page = await openPage(browser, server.url, "global", { waitForReady: false });
  const result = await evaluate(
    browser.cdp,
    page.session.sessionId,
    `(async () => {
      const svgString = ${JSON.stringify(svg)};
      const content = document.createElement('div');
      content.className = 'content';
      document.body.appendChild(content);

      // Warm both paths once, out of the clock.
      const holder = document.createElement('div');
      holder.innerHTML = svgString;
      const node = holder.querySelector('svg');
      content.innerHTML = svgString;

      const time = (fn, repeats) => {
        // One sample = the mean of the repeats ops, taken 5 times; the
        // reported number is the median sample. First and last samples are
        // discarded noise guards only if fewer than 5 fit — with 5 all count.
        const samples = [];
        for (let sample = 0; sample < 5; sample += 1) {
          const started = performance.now();
          for (let index = 0; index < repeats; index += 1) fn();
          samples.push((performance.now() - started) / repeats);
        }
        samples.sort((a, b) => a - b);
        return samples[Math.floor(samples.length / 2)];
      };

      // Path A — what the action (svgMount) does on a re-mount: the same
      // node moves back into the page. No parse.
      const actionOp = time(() => {
        if (node.parentNode === content) content.removeChild(node);
        content.prepend(node);
      }, ${REPEATS});

      // Path B — what the {@html} interpolation does on every update: the
      // markup is parsed again, a new node tree every time.
      const htmlOp = time(() => {
        content.innerHTML = svgString;
      }, ${REPEATS});

      content.remove();
      return { nodes: ${NODES}, svgBytes: svgString.length, repeats: ${REPEATS}, actionMsPerMount: actionOp, htmlMsPerMount: htmlOp };
    })()`,
  );
  const ms = (value) => Number(value).toFixed(4);
  console.log(
    `measure-html nodes=${result.nodes} svgBytes=${result.svgBytes} repeats=${result.repeats}`,
  );
  console.log(`  action (node re-mount, no parse): ${ms(result.actionMsPerMount)} ms/mount`);
  console.log(`  {@html} (full re-parse per mount): ${ms(result.htmlMsPerMount)} ms/mount`);
  console.log(`  ratio: ${(result.htmlMsPerMount / Math.max(result.actionMsPerMount, 1e-9)).toFixed(1)}x`);
  console.log(
    result.htmlMsPerMount <= 3
      ? "  decision input: re-parse <= 3 ms threshold -> {@html} adoption is on the table"
      : "  decision input: re-parse > 3 ms threshold -> keep the action",
  );
  process.exitCode = 0;
} finally {
  await browser.close?.().catch?.(() => {});
  await server.close();
}
