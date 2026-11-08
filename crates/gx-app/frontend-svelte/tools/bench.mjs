#!/usr/bin/env node
// The T5 bench, frontend half: asset weight, lines of code, and — in headless
// Chromium over HTTP, with the faked Tauri bridge — the time from page load to
// `graphium://ready` and from `graphium://render` to `render_ack ok:true` for a
// small and a large drawing.
//
// Why the browser and not the real window: the two builds must be measured
// under the same engine and the same clock, and WebKitGTK cannot be driven from
// outside without instrumenting Rust (see `parity.mjs`). The in-shell half of
// the bench (startup, render through the MCP door, memory) lives in
// `bench-live.sh`, which reports the same scenarios against the real binary.
//
// Usage:
//   node tools/bench.mjs --build=svelte --run=1 --json=/tmp/bench-svelte-1.json
//   node tools/bench.mjs --build=vanilla --run=2
//   node tools/bench.mjs --assets          # only the asset/LOC table
//
// Exit code 0 when every measurement completed.

import { execFileSync } from "node:child_process";
import { readFile, readdir, stat, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import zlib from "node:zlib";

import { evaluate, launch, openPage, serve } from "./lib/harness.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const APP = path.resolve(HERE, ".."); // crates/gx-app/frontend-svelte
const VANILLA_DIR = path.resolve(APP, "..", "frontend");
const SVELTE_DIR = path.resolve(APP, "dist");

const argv = process.argv.slice(2);
const option = (name, fallback = null) => {
  const hit = argv.find((argument) => argument.startsWith(`--${name}=`));
  return hit === undefined ? fallback : hit.slice(name.length + 3);
};
const BUILD = option("build", "svelte");
const RUN = option("run", "1");
const JSON_OUT = option("json", null);
const ASSETS_ONLY = argv.includes("--assets");
// `--both` is the fair A/B: both bundles are served from the same browser, and
// the pages are opened alternately (svelte, vanilla, svelte, …) so a machine
// that gets slower halfway through does not land on one side only.
const BOTH = argv.includes("--both");
const REPEATS = Number(option("repeats", BOTH ? "5" : "1"));

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

const SESSIONS = [{ id: "bench", family: "pi", label: "bench", project: "/bench/graphium", state: "active" }];

/**
 * An SVG of `count` nodes and `count - 1` edges, built programmatically: it is
 * the shape Rust's renderer produces (a `<rect>`, a `<text>` and one `<line>`
 * per edge), not a hand-written string.
 */
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

function renderPayload(id, count, reqId) {
  return {
    req_id: reqId,
    diagram_id: id,
    session_id: "bench",
    svg: drawingSvg(count),
    placement: { x: 100, y: 100 },
    camera: { tx: 0, ty: 0, zoom: count > 50 ? 0.2 : 1 },
  };
}

// --- assets and lines of code ---------------------------------------------

async function gzipSize(file) {
  const body = await readFile(file);
  return { bytes: body.length, gzip: zlib.gzipSync(body, { level: 9 }).length };
}

async function assets() {
  const vanillaFiles = ["index.html", "plan.js", "style.css"];
  const vanilla = {};
  for (const name of vanillaFiles) {
    vanilla[name] = await gzipSize(path.join(VANILLA_DIR, name));
  }
  const distFiles = [];
  const walk = async (directory) => {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const file = path.join(directory, entry.name);
      if (entry.isDirectory()) await walk(file);
      else distFiles.push(file);
    }
  };
  await walk(SVELTE_DIR);
  const svelte = { files: {} };
  for (const file of distFiles.sort()) {
    const relative = path.relative(SVELTE_DIR, file);
    svelte.files[relative] = await gzipSize(file);
  }
  const total = (entries) =>
    Object.values(entries).reduce((sum, entry) => ({ bytes: sum.bytes + entry.bytes, gzip: sum.gzip + entry.gzip }), { bytes: 0, gzip: 0 });
  const byExtension = (entries, extension) =>
    total(Object.fromEntries(Object.entries(entries).filter(([name]) => name.endsWith(extension))));
  const vanillaTotal = total(vanilla);
  const svelteTotal = total(svelte.files);
  return {
    vanilla: {
      files: vanilla,
      total: vanillaTotal,
      js: vanilla["plan.js"],
      css: vanilla["style.css"],
      html: vanilla["index.html"],
    },
    svelte: {
      files: svelte.files,
      total: svelteTotal,
      js: byExtension(svelte.files, ".js"),
      css: byExtension(svelte.files, ".css"),
      html: byExtension(svelte.files, ".html"),
    },
    deltaBytes: svelteTotal.bytes - vanillaTotal.bytes,
    deltaGzip: svelteTotal.gzip - vanillaTotal.gzip,
  };
}

function lineCount(text) {
  if (text.length === 0) return 0;
  const lines = text.split("\n");
  return lines[lines.length - 1] === "" ? lines.length - 1 : lines.length;
}

async function linesOfCode() {
  const vanillaNames = ["index.html", "plan.js", "style.css"];
  const vanilla = {};
  for (const name of vanillaNames) {
    vanilla[name] = lineCount(await readFile(path.join(VANILLA_DIR, name), "utf8"));
  }
  // The Svelte frontend is everything the port authored: `src/` only (`dist/`
  // is generated, `tools/` is the harness, not the UI).
  const svelteFiles = {};
  const walk = async (directory) => {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const file = path.join(directory, entry.name);
      if (entry.isDirectory()) await walk(file);
      else if (/\.(svelte|js|css)$/.test(file)) {
        svelteFiles[path.relative(APP, file)] = lineCount(await readFile(file, "utf8"));
      }
    }
  };
  await walk(path.join(APP, "src"));
  const sum = (entries) => Object.values(entries).reduce((total, value) => total + value, 0);
  return {
    vanilla: { files: vanilla, total: sum(vanilla) },
    svelte: { files: svelteFiles, total: sum(svelteFiles) },
    delta: sum(svelteFiles) - sum(vanilla),
  };
}

// --- the headless measurement ---------------------------------------------

async function measure(browser, url) {
  const page = await openPage(browser, url, "global", { waitForReady: false, timeoutMs: 15000 });
  try {
    const ready = await evaluate(
      browser.cdp,
      page.session.sessionId,
      `(async () => {
        for (let index = 0; index < 600; index += 1) {
          const hit = window.__h.emitted.find((entry) => entry.name === 'graphium://ready');
          if (hit) {
            return {
              readyMs: hit.t,
              observedAt: performance.now(),
              // What the human sees: the first paint of the chrome, which is the
              // other half of "how long until the window is usable".
              paint: Object.fromEntries(performance.getEntriesByType('paint').map((entry) => [entry.name, entry.startTime])),
              entries: performance.getEntriesByType('resource').map((entry) => ({ name: entry.name.split('/').pop(), ms: entry.duration })),
            };
          }
          await new Promise((resolve) => setTimeout(resolve, 10));
        }
        throw new Error('ready never arrived');
      })()`,
    );
    await evaluate(
      browser.cdp,
      page.session.sessionId,
      `(() => {
        __t.rust('graphium://settings', ${JSON.stringify(SETTINGS)});
        __t.rust('graphium://state', ${JSON.stringify({ diagrams: [], sessions: SESSIONS, clones: [] })});
        return true;
      })()`,
      false,
    );
    await new Promise((resolve) => setTimeout(resolve, 150));

    const runs = [];
    const cases = [
      ["small", 12],
      ["large", 300],
    ];
    for (const [label, count] of cases) {
      for (let sample = 1; sample <= 2; sample += 1) {
        const payload = renderPayload(`d-bench-${label}-${sample}`, count, `req-${label}-${sample}`);
        const result = await evaluate(
          browser.cdp,
          page.session.sessionId,
          `(async () => {
            const payload = ${JSON.stringify(payload)};
            const before = __h.raf;
            const started = performance.now();
            __t.rust('graphium://render', payload);
            for (let index = 0; index < 2000; index += 1) {
              const ack = __h.emitted.find((entry) => entry.name === 'render_ack' && entry.payload.req_id === payload.req_id);
              if (ack) {
                return {
                  ms: ack.t - started,
                  frames: ack.raf - before,
                  ok: ack.payload.ok,
                  mounted: document.querySelectorAll('.diagram').length,
                  svgBytes: payload.svg.length,
                };
              }
              await new Promise((resolve) => setTimeout(resolve, 2));
            }
            throw new Error('no ack for ' + payload.req_id);
          })()`,
        );
        runs.push({ case: label, nodes: count, sample, ...result });
      }
    }
    return { ready, runs };
  } finally {
    await page.close();
  }
}

// --- main ------------------------------------------------------------------

async function main() {
  const assetTable = await assets();
  const loc = await linesOfCode();

  if (ASSETS_ONLY) {
    console.log(JSON.stringify({ assets: assetTable, loc }, null, 2));
    return;
  }

  const chromium = execFileSync("chromium-browser", ["--version"], { encoding: "utf8" }).trim();
  const builds = BOTH
    ? [
        ["svelte", SVELTE_DIR],
        ["vanilla", VANILLA_DIR],
      ]
    : [[BUILD, BUILD === "svelte" ? SVELTE_DIR : VANILLA_DIR]];
  for (const [name, root] of builds) {
    if (!existsSync(path.join(root, "index.html"))) {
      console.error(`bench: no bundle at ${root} (build it first)`);
      process.exit(2);
    }
  }

  const servers = {};
  for (const [name, root] of builds) servers[name] = await serve(root);
  const browser = await launch();
  const samples = Object.fromEntries(builds.map(([name]) => [name, { ready: [], runs: [] }]));
  try {
    for (let repeat = 0; repeat < REPEATS; repeat += 1) {
      for (const [name] of builds) {
        const timings = await measure(browser, servers[name].url);
        samples[name].ready.push(timings.ready);
        samples[name].runs.push(...timings.runs);
      }
    }
  } finally {
    await browser.close();
    for (const name of Object.keys(servers)) await servers[name].close();
  }

  const report = {
    mode: BOTH ? "both" : "single",
    build: BOTH ? null : BUILD,
    run: RUN,
    repeats: REPEATS,
    chromium,
    assets: assetTable,
    loc,
    builds: samples,
  };
  console.log(JSON.stringify(report, null, 2));
  if (JSON_OUT !== null) await writeFile(JSON_OUT, `${JSON.stringify(report, null, 2)}\n`);
}

await main();
