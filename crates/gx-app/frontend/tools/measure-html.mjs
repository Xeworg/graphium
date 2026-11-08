#!/usr/bin/env node
// The mount measurement of `f11-elementos.md` T2/B (S8 preview): what it costs
// the plan to put one drawing on the screen, **the string path against the
// element path**, on the same fixture, with the same clock, under the same
// engine.
//
// Why the browser and not the real window: `bench.mjs` (and the A/B bench of
// `odd/tasks/svelte-ui-t5-bench.md`) already measures the plan in headless
// Chromium over HTTP with the faked Tauri bridge, and the real window cannot be
// driven from outside (`parity.mjs`, first paragraphs). The bundle under test
// is the **real built one** (`npm run build` / `scripts/build.sh`), mounted by
// the real `App.svelte`: the scene goes through `sceneFromPayload`, the
// components and the culling exactly as it does in the shell.
//
// What is measured (one sample):
//
//   t0 = performance.now()
//   __t.rust('graphium://render', payload)     // the event Rust emits
//   … until the drawing is a child of `.content` (microtasks, no frames)
//   plan.getBoundingClientRect()               // style + layout of the new svg
//   t1 = performance.now()
//
// That is the **mount**: parse (string) or build (elements), insert, and lay
// out. The `render_ack` is not part of it on purpose — it waits two animation
// frames (C4), which is the bimodal 4–32 ms `bench.mjs` reports and hides every
// difference inside the noise. The clock here is the work, not the pacing.
//
// What it prints:
//
// - `modes.html` — the string path: `holder.innerHTML = svg` → insert → layout,
//   which is what `insertDiagram` did before T2/B and what the legacy branch
//   still does. Its median is the number to compare with the 1.466 ms/mount
//   baseline the feature doc names.
// - `modes.scene` — the element path: the payload's `elements` mounted by
//   `DiagramScene` (keyed components, one element per node/edge/band/text).
//
// Both numbers come from the same instrument, on the same fixture, in the same
// page, so the difference is the two paints and nothing else.
//
// Usage (from `crates/gx-app/frontend`, after `npm run build`):
//
//   node tools/measure-html.mjs                      # synthetic 300-node fixture
//   node tools/measure-html.mjs --mode=scene
//   node tools/measure-html.mjs --payload=/tmp/f11-300.json
//   node tools/measure-html.mjs --payload=/tmp/x.json --samples=11 --json=/tmp/m.json
//
// `--payload` takes `{ "svg": "…", "elements": { … } }` — the two halves of one
// real render, which is what `gx_core::RenderOutcome` carries and what
// `crates/gx-app/tests/scene_payload.rs` prints. Without it the tool builds a
// fixture of the same shape in JS (like `bench.mjs` builds its drawings), so
// the tool runs on a fresh checkout with no Rust step.
//
// Exit code 0 when every measurement completed.

import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { evaluate, launch, openPage, serve } from "./lib/harness.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const APP = path.resolve(HERE, ".."); // crates/gx-app/frontend
const DIST = path.resolve(APP, "dist");

const argv = process.argv.slice(2);
const option = (name, fallback = null) => {
  const hit = argv.find((argument) => argument.startsWith(`--${name}=`));
  return hit === undefined ? fallback : hit.slice(name.length + 3);
};
// `--scene` / `--html` are the shorthand of the two modes of the feature doc.
const MODE = argv.includes("--scene")
  ? "scene"
  : argv.includes("--html")
    ? "html"
    : option("mode", "both");
const PAYLOAD_FILE = option("payload", null);
const NODES = Number(option("nodes", "300"));
const SAMPLES = Number(option("samples", "9"));
const WARMUP = Number(option("warmup", "2"));
const REPEATS = Number(option("repeats", "1"));
const JSON_OUT = option("json", null);
const CHROMIUM = option("chromium", process.env.CHROMIUM || "chromium-browser");

if (!["both", "html", "scene"].includes(MODE)) {
  console.error(`measure-html: --mode must be both, html or scene (got ${MODE})`);
  process.exit(2);
}

// --- the fixtures ----------------------------------------------------------

const SETTINGS = {
  accent: "#6ea8fe",
  background: "#1b1b1b",
  edge_stroke: "#8a8a96",
  edge_text: "#b9b9c2",
  grid_angle_degrees: 45,
  grid_color: "rgba(255, 255, 255, 0.14)",
  grid_diagonal: 59,
  grid_opacity: 1,
  grid_visible: true,
  node_fill: "#2a2a30",
  node_stroke: "#4d4d57",
  node_text: "#e9e9ec",
  theme: "dark",
};
const SESSIONS = [
  { id: "measure", family: "pi", label: "measure", project: "/measure/graphium", state: "active" },
];

/**
 * A chain of `count` nodes and `count - 1` arrows, in both halves of the
 * payload: the SVG the string path mounts and the elements the element path
 * mounts. The numbers are the frozen tokens of `gx-core` and the ids are the
 * ids of the drawing (`g-<id>`, `m-<from>-<to>`), so the two fixtures are the
 * same drawing and not two shapes that happen to be the same size.
 */
function chain(count) {
  const tokens = {
    text_size: 14,
    node_padding: 12,
    corner_radius: 6,
    border_width: 1,
    line_height: 1.25,
    regular_weight: 400,
    semibold_weight: 600,
    arrow_length: 10,
    crow_foot: 22,
    crow_foot_spread: 0.85,
    badge_width: 26,
    muted_opacity: 0.55,
    lifeline_dash: [4, 4],
    dash_pattern: [6, 4],
    group_dash: [8, 5],
  };
  const width = 152.43;
  const height = 41.5;
  // The chain step of the 300-node payload of L4 (`chain(300)` of RNF10).
  const step = 232.43;
  const nodes = [];
  const edges = [];
  const svgNodes = [];
  const svgEdges = [];
  for (let index = 0; index < count; index += 1) {
    const center = { x: 76.22 + index * step, y: 20.75 };
    const id = `g-n${index}`;
    const label = `nodo ${index} del pipeline`;
    nodes.push({
      id,
      label,
      shape: "rounded",
      emphasis: "normal",
      style: "solid",
      center,
      size: { w: width, h: height },
    });
    svgNodes.push(
      `<rect id="${id}" class="node-fill node-stroke shape-rounded" x="${(center.x - width / 2).toFixed(2)}" y="${(center.y - height / 2).toFixed(2)}" width="${width.toFixed(2)}" height="${height.toFixed(2)}" rx="6" ry="6"/>\n` +
        `<text class="node-text text" x="${center.x.toFixed(2)}" y="${center.y.toFixed(2)}" font-size="14.00" text-anchor="middle" dominant-baseline="central">${label}</text>\n`,
    );
    if (index > 0) {
      const from = nodes[index - 1].center;
      const points = [
        { x: from.x + width / 2, y: from.y },
        { x: from.x + width / 2 + 40, y: from.y },
        { x: center.x - width / 2 - 40, y: center.y },
        { x: center.x - width / 2, y: center.y },
      ];
      edges.push({
        id: `m-n${index - 1}-n${index}`,
        from: `n${index - 1}`,
        to: `n${index}`,
        emphasis: "normal",
        style: "solid",
        arrow: true,
        points,
      });
      svgEdges.push(
        `<polyline id="m-n${index - 1}-n${index}" class="edge-stroke" points="${points
          .map((point) => `${point.x.toFixed(2)},${point.y.toFixed(2)}`)
          .join(" ")}" marker-end="url(#arrow-normal)"/>\n`,
      );
    }
  }
  const canvas = {
    x: 0,
    y: 0,
    w: 76.22 + (count - 1) * step + width / 2 + 76.22,
    h: height,
  };
  // The elements of the same drawing: one node per box and one edge per arrow,
  // with the measured size in `size` and the label as it arrives (S4).
  const elements = {
    canvas,
    tokens,
    direction: "LR",
    layout_hint: "layered",
    palette: {
        source: "css-vars",
        theme: "dark",
        bg: "var(--bg)",
        node_fill: "var(--node-fill)",
        node_stroke: "var(--node-stroke)",
        node_text: "var(--node-text)",
        edge_stroke: "var(--edge-stroke)",
        edge_text: "var(--edge-text)",
        accent: "var(--accent)",
      },
      nodes,
      edges,
      bands: [],
      events: [],
    };
  const style =
    "<style>\n.node-fill{fill:rgba(42, 42, 48, 1.00)}\n.node-stroke{stroke:rgba(77, 77, 87, 1.00);stroke-width:1.00}\n" +
    ".node-text{fill:rgba(233, 233, 236, 1.00)}\n.edge-stroke{stroke:rgba(138, 138, 150, 1.00);stroke-width:1.00;fill:none}\n" +
    ".text{font-family:Inter, sans-serif}\n.arrow-normal{fill:rgba(138, 138, 150, 1.00)}\n</style>\n";
  const marker =
    '<defs>\n<marker id="arrow-normal" viewBox="0.00 0.00 10.00 10.00" refX="10.00" refY="5.00" markerWidth="10.00" markerHeight="10.00" markerUnits="userSpaceOnUse" orient="auto">\n' +
    '<path class="arrow-normal" d="M 0.00 0.00 L 10.00 5.00 L 0.00 10.00 Z"/>\n</marker>\n</defs>\n';
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" class="graphium" viewBox="0.00 0.00 ${canvas.w.toFixed(2)} ${canvas.h.toFixed(2)}" width="${canvas.w.toFixed(2)}" height="${canvas.h.toFixed(2)}">\n` +
    `${style}${marker}<g class="layer-edges">\n${svgEdges.join("")}</g>\n<g class="layer-nodes">\n${svgNodes.join("")}</g>\n</svg>`;
  return { svg, elements };
}

async function fixture() {
  if (PAYLOAD_FILE !== null) {
    const payload = JSON.parse(await readFile(PAYLOAD_FILE, "utf8"));
    if (typeof payload.svg !== "string" || typeof payload.elements !== "object") {
      throw new Error(`${PAYLOAD_FILE} is not { svg, elements }`);
    }
    return { source: PAYLOAD_FILE, svg: payload.svg, elements: payload.elements };
  }
  const built = chain(NODES);
  return { source: `synthetic chain(${NODES})`, ...built };
}

// --- the measurement -------------------------------------------------------

/** One page load: settings + state in, then `SAMPLES` mounts of each mode. */
async function measure(browser, url, halves) {
  const page = await openPage(browser, url, "global", { waitForReady: false, timeoutMs: 15000 });
  const run = (expression) => evaluate(browser.cdp, page.session.sessionId, expression, true);
  try {
    // Wait for the handshake of the page before handing it a render (C2).
    await run(`(async () => {
      for (let index = 0; index < 600; index += 1) {
        if (__h.emitted.some((entry) => entry.name === 'graphium://ready')) return true;
        await new Promise((resolve) => setTimeout(resolve, 10));
      }
      throw new Error('ready never arrived');
    })()`);
    await run(`(() => {
      __t.rust('graphium://settings', ${JSON.stringify(SETTINGS)});
      __t.rust('graphium://state', ${JSON.stringify({ diagrams: [], sessions: SESSIONS, clones: [] })});
      return true;
    })()`);
    await new Promise((resolve) => setTimeout(resolve, 80));

    // The reset between two mounts: the plane drops the record (and with it the
    // drawing), so every sample pays one mount and never a re-mount, and the
    // culling never piles up more than one live drawing.
    const reset = async () => {
      await run(`(() => {
        __t.rust('graphium://state', ${JSON.stringify({ diagrams: [], sessions: SESSIONS, clones: [] })});
        return true;
      })()`);
      await new Promise((resolve) => setTimeout(resolve, 30));
    };

    const modes = MODE === "both" ? ["html", "scene"] : [MODE];
    const samples = {};
    for (const mode of modes) {
      samples[mode] = [];
      for (let index = 0; index < WARMUP + SAMPLES; index += 1) {
        const payload =
          mode === "scene"
            ? { elements: halves.elements }
            : { svg: halves.svg };
        const measured = await run(`(async () => {
          const payload = ${JSON.stringify(payload)};
          payload.diagram_id = 'd-measure-${mode}';
          payload.session_id = 'measure';
          payload.req_id = 'req-${mode}-${index}';
          payload.placement = { x: 100, y: 100 };
          payload.camera = { tx: 0, ty: 0, zoom: 1 };
          const sameId = document.querySelector('[data-id="' + payload.diagram_id + '"]') !== null;
          const before = document.querySelectorAll('.diagram .content > svg').length;
          const started = performance.now();
          __t.rust('graphium://render', payload);
          // The page flushes its effects in a microtask (Svelte schedules the
          // update there), so the drawing lands without waiting for a frame.
          let spins = 0;
          while (
            document.querySelectorAll('.diagram .content > svg').length === before &&
            spins < 64
          ) {
            await Promise.resolve();
            spins += 1;
          }
          document.getElementById('plan').getBoundingClientRect();
          const ms = performance.now() - started;
          // The ack comes two frames later (C4): it is not part of the mount,
          // it only says the page kept its contract. Waiting for it here keeps
          // the measurement honest without folding the frames into the number.
          await new Promise((resolve) => setTimeout(resolve, 45));
          const host = document.querySelector('[data-id="' + payload.diagram_id + '"]');
          const svg = host === null ? null : host.querySelector('.content > svg');
          const ack = __h.emitted.filter((entry) => entry.name === 'render_ack' && entry.payload.req_id === payload.req_id).pop();
          return {
            ms,
            spins,
            mounted: svg !== null,
            // A scene mounts no <style> block: that is how the two paths are
            // told apart from the outside.
            isScene: svg !== null && svg.querySelectorAll('style').length === 0,
            elements: svg === null ? 0 : svg.querySelectorAll('*').length,
            ok: ack === undefined ? null : ack.payload.ok,
            sameId,
          };
        })()`);
        await reset();
        if (index >= WARMUP) {
          samples[mode].push(measured);
        }
      }
    }
    return samples;
  } finally {
    await page.close();
  }
}

const median = (values) => {
  const sorted = [...values].sort((left, right) => left - right);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0 ? (sorted[middle - 1] + sorted[middle]) / 2 : sorted[middle];
};
const mean = (values) => values.reduce((sum, value) => sum + value, 0) / values.length;

function summarise(samples) {
  const times = samples.map((sample) => sample.ms);
  return {
    samples: samples.map((sample) => Number(sample.ms.toFixed(3))),
    medianMs: Number(median(times).toFixed(3)),
    meanMs: Number(mean(times).toFixed(3)),
    minMs: Number(Math.min(...times).toFixed(3)),
    maxMs: Number(Math.max(...times).toFixed(3)),
    mounted: samples.every((sample) => sample.mounted),
    isScene: samples.every((sample) => sample.isScene),
    elements: samples.length === 0 ? 0 : samples[0].elements,
    ok: samples.every((sample) => sample.ok === true),
    reMount: samples.some((sample) => sample.sameId),
  };
}

// --- main ------------------------------------------------------------------

async function main() {
  const chromium = execFileSync(CHROMIUM, ["--version"], { encoding: "utf8" }).trim();
  if (!existsSync(path.join(DIST, "index.html"))) {
    console.error(`measure-html: no bundle at ${DIST} (run npm run build first)`);
    process.exit(2);
  }

  const halves = await fixture();
  const server = await serve(DIST);
  const browser = await launch(CHROMIUM);
  const runs = [];
  try {
    for (let repeat = 0; repeat < REPEATS; repeat += 1) {
      runs.push(await measure(browser, server.url, halves));
    }
  } finally {
    await browser.close();
    await server.close();
  }

  const modes = {};
  for (const mode of Object.keys(runs[0])) {
    const samples = runs.flatMap((run) => run[mode]);
    modes[mode] = summarise(samples);
  }
  const report = {
    chromium,
    fixture: halves.source,
    nodes: halves.elements.nodes.length,
    edges: halves.elements.edges.length,
    bytes: { svg: halves.svg.length, elements: JSON.stringify(halves.elements).length },
    samples: SAMPLES,
    warmup: WARMUP,
    repeats: REPEATS,
    modes,
  };
  if (modes.html !== undefined && modes.scene !== undefined) {
    const faster = modes.scene.medianMs <= modes.html.medianMs ? "scene" : "html";
    report.verdict = {
      faster,
      sceneMs: modes.scene.medianMs,
      htmlMs: modes.html.medianMs,
      ratio: Number((modes.scene.medianMs / modes.html.medianMs).toFixed(2)),
    };
  }
  console.log(JSON.stringify(report, null, 2));
  if (JSON_OUT !== null) await writeFile(JSON_OUT, `${JSON.stringify(report, null, 2)}\n`);
}

await main();
