#!/usr/bin/env node
// The equivalence gate of `f11-elementos.md` S8 (task T3/C).
//
// The promise of S8: the mounted scene, serialized, is the golden SVG of the
// same document modulo formatting, on **every committed golden document**. This tool
// is that promise as a committed gate. It mounts each document through the
// real built bundle (`graphium://render` with `elements` only — no `svg` key at
// all), serializes the `<svg class="graphium">` the painter produced and
// compares it, element by element, with `crates/gx-app/tests/golden/<name>.svg`
// (the export artifact `golden_svg.rs` pins byte by byte).
//
// The scenes are committed at `crates/gx-app/tests/golden-scenes/<name>.json`
// and `crates/gx-app/tests/golden_svg.rs` regenerates and re-checks them
// (`UPDATE_SCENES=1`), so the probe never renders on its own: the drawing
// under test is the one the product's single door produced.
//
// Usage (from `crates/gx-app/frontend`, after `npm run build`):
//
//   node tools/equivalence.mjs
//   node tools/equivalence.mjs --json /tmp/equivalence.json
//
// `--chromium <path>` (or `CHROMIUM`) picks the browser, like `parity.mjs`.
// Exit code: 0 when every document is equivalent.
//
// --- what "modulo formatting" means ---------------------------------------
//
// Formatting is all a string diff would see that is not the drawing: attribute
// order, whitespace between elements, `450.27` vs `450.27`, `0.00` vs `0`, the
// sorted-ness of class lists, and the two emitter-only artifacts (the `<style>`
// block — the painter takes its colours from `src/app.css` — and the `xmlns`).
// Both sides are serialized into the same canonical tree: tag, attributes
// (class lists sorted, numeric values rounded to the two decimals the drawing
// promises, path/points/viewBox numbers compared as numbers), trimmed text,
// and children in paint order. Anything outside those rules is a difference
// and fails the probe.
//
// Four differences are **decisions**, not formatting, and the probe names
// them so a reader knows why they are allowed (Judgment Day L7 item (f),
// B-006). Each one is recorded in the report as an axis trigger:
//
//   1. Empty layer containers. The emitter always writes the layers its kind
//      has (`drawing.draws_clusters()` is "the document is a graph", so a
//      graph without containers still gets `<g class="layer-groups"></g>`);
//      the painter only writes a layer that has elements in it
//      (`DiagramScene.svelte`). Both sides' empty `layer-*` groups are
//      dropped before the comparison. A layer that is empty on one side and
//      not on the other still fails: the rule drops empty groups, never
//      elements.
//   2. The source of the `<defs>` markers. The emitter derives the marker set
//      from the **unfiltered** edge list (`write_defs` walks `drawing.edges`)
//      and the painter from the **filtered** one (`arrowMarkers` walks the
//      scene's edges, which the scene builder and `scene.js` already dropped
//      when they had no route). The probe compares the markers each side
//      actually references (`marker-end="url(#…)"`), so a marker referenced
//      by a drawn line is always required on both sides, and a definition
//      nobody references is not evidence either way. The report says which
//      ids were dropped as unreferenced.
//   3. One-point edges. A route with one point is a degenerate line: the
//      emitter writes the `<polyline>` anyway (its `points` is not empty) and
//      the painter drops the edge (`scene.js`, the rule of the emitter when
//      no line can be drawn). A `<polyline class="edge-stroke">` with fewer
//      than two points is dropped from the comparison and counted. The 15
//      golden documents do not have one — the report shows `0` — and the rule
//      exists so the decision is written down instead of surprising the next
//      reader with a red probe.
//   4. The interaction of an element (`f11-elementos.md` T4/D). An element of
//      the mounted scene is one hit target — `role`, `tabindex`, `aria-label`,
//      `aria-pressed` and the `data-element` hook — and an exported file has
//      no pointer and no keyboard: the attributes are painter-only. Both
//      sides' interaction attributes are dropped before the comparison and
//      counted, so the report shows how many the mounted scene carries. The
//      drawing under them is untouched: same ids, same classes, same numbers.
//      A class (the highlight) is not an interaction attribute — a probe that
//      mounted a *selected* scene would still fail here, on purpose.
//
// Everything else is not negotiable: same ids (`g-`/`m-`/`cluster-`/`lane-`),
// same boxes, same classes, same glyph geometry.

import { readFile, readdir } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { evaluate, launch, openPage, serve } from "./lib/harness.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const APP = path.resolve(HERE, ".."); // crates/gx-app/frontend
const TESTS = path.resolve(APP, "..", "tests");
const DIST = path.resolve(APP, "dist");
const GOLDEN = path.resolve(TESTS, "golden");
const SCENES = path.resolve(TESTS, "golden-scenes");

const argv = process.argv.slice(2);
const option = (name, fallback = null) => {
  const hit = argv.find((argument) => argument.startsWith(`--${name}=`));
  return hit === undefined ? fallback : hit.slice(name.length + 3);
};
const JSON_OUT = option("json", null);
const CHROMIUM = option("chromium", process.env.CHROMIUM || "chromium-browser");

// --- the canonicalizer -----------------------------------------------------
//
// This source is evaluated **in the page** (it needs a DOM): it turns either
// an element of the mounted scene or the golden SVG parsed by `DOMParser` into
// the same canonical tree. It is a string because the browser cannot import a
// node module; `--print-canon` is not a feature, the code lives here once.

const CANON_SOURCE = `(() => {
  'use strict';
  // The attributes of the drawing that carry one number.
  const NUMERIC = new Set([
    'x', 'y', 'width', 'height', 'rx', 'ry', 'cx', 'cy', 'r',
    'x1', 'y1', 'x2', 'y2', 'stroke-width', 'font-size', 'font-weight',
    'refX', 'refY', 'markerWidth', 'markerHeight',
  ]);
  // The attributes of the drawing that carry a list of numbers (punctuation
  // is formatting: "1.00,2.00" and "1 2" are the same points).
  const NUMBER_LISTS = new Set(['points', 'viewBox', 'd', 'transform']);
  const round = (value) => {
    const rounded = Math.round(value * 100) / 100;
    return Object.is(rounded, -0) ? 0 : rounded;
  };
  const numberList = (text) => {
    const tokens = text.trim().split(/[\\s,]+/).filter((token) => token !== '');
    return tokens.map((token) => {
      const value = Number(token);
      return Number.isFinite(value) && token.trim() !== '' ? round(value) : token;
    });
  };
  const attribute = (name, value, side) => {
    if (name === 'xmlns' || name.startsWith('xmlns:')) return null;
    // Axis 4: the interaction of an element is painter-only (T4/D).
    if (name === 'role' || name === 'tabindex' || name === 'focusable' ||
        name.startsWith('aria-') || name.startsWith('data-')) {
      axes.interaction[side] += 1;
      return null;
    }
    if (name === 'class') {
      return value.trim().split(/\\s+/).filter(Boolean).sort().join(' ');
    }
    if (NUMERIC.has(name)) {
      const parsed = Number(value);
      return Number.isFinite(parsed) ? round(parsed) : value;
    }
    if (NUMBER_LISTS.has(name)) {
      return numberList(value);
    }
    if (name === 'marker-end' || name === 'marker-start' || name === 'marker-mid') {
      const match = /url\\(\\s*#([^\\s)]+)\\s*\\)/.exec(value);
      return match === null ? value.trim() : 'url(#' + match[1] + ')';
    }
    return value;
  };
  const referencedMarkers = (root) => {
    const ids = new Set();
    for (const node of root.querySelectorAll('[marker-end], [marker-start], [marker-mid]')) {
      for (const name of ['marker-end', 'marker-start', 'marker-mid']) {
        const value = node.getAttribute(name);
        if (value === null) continue;
        const match = /url\\(\\s*#([^\\s)]+)\\s*\\)/.exec(value);
        if (match !== null) ids.add(match[1]);
      }
    }
    return ids;
  };
  const layerOf = (node) => {
    if (node.localName !== 'g') return null;
    const classes = (node.getAttribute('class') || '').trim().split(/\\s+/);
    const layer = classes.find((name) => name.startsWith('layer-'));
    return layer === undefined ? null : layer;
  };
  const axes = { emptyLayers: { expected: 0, actual: 0 }, markers: { expected: [], actual: [] }, onePointEdges: { expected: 0, actual: 0 }, interaction: { expected: 0, actual: 0 } };
  // The canonical tree. 'side' is 'expected' (the golden SVG) or 'actual'
  // (the mounted scene); it only decides where an axis trigger is recorded.
  const canon = (node, side, referenced) => {
    const tag = node.localName;
    if (tag === 'style') return null;
    const attrs = {};
    for (const name of node.getAttributeNames()) {
      const value = attribute(name, node.getAttribute(name), side);
      if (value !== null) attrs[name] = value;
    }
    // Axis 3: a one-point route is a degenerate edge the painter drops.
    if (tag === 'polyline' && Array.isArray(attrs.points) && attrs.points.length < 4) {
      axes.onePointEdges[side] += 1;
      return null;
    }
    const children = [];
    let text = '';
    for (const child of node.childNodes) {
      if (child.nodeType === 3) {
        const content = child.nodeValue.replace(/\\s+/g, ' ').trim();
        if (content !== '') text += content;
        continue;
      }
      if (child.nodeType !== 1) continue;
      // Axis 1: an empty layer container; the painter does not write it.
      if (layerOf(child) !== null && child.children.length === 0) {
        axes.emptyLayers[side] += 1;
        continue;
      }
      const value = canon(child, side, referenced);
      if (value !== null) children.push(value);
    }
    // Axis 2: only the markers the document references are evidence; a def
    // nobody points at is a consequence of the filtered/unfiltered source.
    if (tag === 'marker') {
      if (!referenced.has(attrs.id)) {
        axes.markers[side].push(attrs.id);
        return null;
      }
    }
    if (tag === 'defs' && children.length === 0) {
      // Both sides write an empty <defs> when no edge carries an arrow; it is
      // the same drawing, so compare it as one node either way.
      return { tag, attrs, text: '', children: [] };
    }
    return { tag, attrs, text, children };
  };
  const canonicalize = (root, side) => {
    const referenced = referencedMarkers(root);
    return { tree: canon(root, side, referenced), axes };
  };
  return { canonicalize };
})()`;

// --- structural diff -------------------------------------------------------

function firstDifferences(expected, actual, path = "$", out = [], limit = 12) {
  if (out.length >= limit) return out;
  const where = (name) => `${path}.${name}`;
  if (expected === undefined || actual === undefined) {
    out.push(
      `${path}: ${expected === undefined ? "missing in the golden" : "missing in the scene"}`,
    );
    return out;
  }
  if (typeof expected !== typeof actual) {
    out.push(`${path}: ${JSON.stringify(expected)} (golden) vs ${JSON.stringify(actual)} (scene)`);
    return out;
  }
  if (typeof expected !== "object" || expected === null || actual === null) {
    if (JSON.stringify(expected) !== JSON.stringify(actual)) {
      out.push(`${path}: ${JSON.stringify(expected)} (golden) vs ${JSON.stringify(actual)} (scene)`);
    }
    return out;
  }
  if (Array.isArray(expected) || Array.isArray(actual)) {
    const left = Array.isArray(expected) ? expected : [];
    const right = Array.isArray(actual) ? actual : [];
    if (left.length !== right.length) {
      out.push(`${where("length")}: ${left.length} (golden) vs ${right.length} (scene)`);
    }
    for (let index = 0; index < Math.max(left.length, right.length); index += 1) {
      firstDifferences(left[index], right[index], `${path}[${index}]`, out, limit);
      if (out.length >= limit) break;
    }
    return out;
  }
  const keys = new Set([...Object.keys(expected), ...Object.keys(actual)]);
  for (const key of keys) {
    firstDifferences(expected[key], actual[key], where(key), out, limit);
    if (out.length >= limit) break;
  }
  return out;
}

// --- fixtures --------------------------------------------------------------

async function documents() {
  // The 15 documents of `golden_svg.rs` are the scenes committed beside the
  // goldens. The golden directory keeps one more drawing
  // (`export-clusters-emphasis.svg`) that is pinned but outside the case list,
  // so the scenes — not the goldens — are the inventory: each one must have
  // its golden, and a scene without one is a fixture out of sync.
  const scenes = (await readdir(SCENES)).filter((name) => name.endsWith(".json")).sort();
  const goldens = new Set(
    (await readdir(GOLDEN)).filter((name) => name.endsWith(".svg")).map((name) => name.replace(/\.svg$/, "")),
  );
  const missing = scenes
    .map((name) => name.replace(/\.json$/, ""))
    .filter((name) => !goldens.has(name));
  if (missing.length > 0) {
    throw new Error(`the fixtures do not pair up: no golden for ${missing.join(", ")}`);
  }
  const docs = await Promise.all(
    scenes.map(async (file) => {
      const name = file.replace(/\.json$/, "");
      return {
        name,
        golden: await readFile(path.join(GOLDEN, `${name}.svg`), "utf8"),
        scene: JSON.parse(await readFile(path.join(SCENES, file), "utf8")),
      };
    }),
  );
  if (docs.length === 0) {
    throw new Error("no committed scenes to mount");
  }
  return docs;
}

// --- one mounted document --------------------------------------------------

async function mountAndCompare(browser, url, document, index) {
  const page = await openPage(browser, url, "global");
  try {
    const expression = `(async () => {
      const { canonicalize } = ${CANON_SOURCE};
      __t.rust('graphium://settings', ${JSON.stringify({
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
      })});
      __t.rust('graphium://render', ${JSON.stringify({
        req_id: `eq-${index}`,
        diagram_id: `d-eq-${index}`,
        session_id: "s1",
        elements: document.scene,
        placement: { x: 40, y: 40 },
        camera: { tx: 0, ty: 0, zoom: 1 },
      })});
      await __t.wait(240);
      const acks = __t.emitted('render_ack').filter((ack) => ack.req_id === 'eq-${index}');
      if (acks.length !== 1 || acks[0].ok !== true) {
        return { mounted: false, error: 'render_ack ' + JSON.stringify(acks) };
      }
      const object = document.querySelector('#plan .diagram');
      if (object === null) return { mounted: false, error: 'no .diagram mounted' };
      const roots = object.querySelectorAll('.content svg.graphium');
      if (roots.length !== 1) return { mounted: false, error: 'svg.graphium roots: ' + roots.length };
      const root = roots[0];
      const nodes = root.querySelectorAll('[id]').length;
      // The string branch is gone after T3/C: nothing in the object carries
      // the markup of the export (its <style> block is the tell).
      const stringNodes = object.querySelectorAll('.content style').length;
      const actual = canonicalize(root, 'actual');
      const parsed = new DOMParser().parseFromString(${JSON.stringify(document.golden)}, 'image/svg+xml');
      if (parsed.querySelector('parsererror') !== null) return { mounted: false, error: 'the golden does not parse' };
      const expected = canonicalize(parsed.documentElement, 'expected');
      return { mounted: true, nodes, stringNodes, expected, actual };
    })()`;
    const value = await evaluate(browser.cdp, page.session.sessionId, expression);
    if (!value.mounted) return { name: document.name, ok: false, error: value.error };
    const diffs = firstDifferences(value.expected.tree, value.actual.tree);
    return {
      name: document.name,
      ok: diffs.length === 0 && value.stringNodes === 0,
      diffs,
      nodes: value.nodes,
      stringNodes: value.stringNodes,
      axes: {
        emptyLayers: value.actual.axes.emptyLayers,
        markers: value.actual.axes.markers,
        onePointEdges: value.actual.axes.onePointEdges,
        interaction: value.actual.axes.interaction,
      },
    };
  } finally {
    await page.close();
  }
}

// --- runner ----------------------------------------------------------------

async function main() {
  const docs = await documents();
  const browser = await launch(CHROMIUM);
  const server = await serve(DIST);
  const results = [];
  try {
    for (const [index, document] of docs.entries()) {
      results.push(await mountAndCompare(browser, server.url, document, index));
    }
  } finally {
    await server.close();
    await browser.close();
  }

  let failures = 0;
  for (const result of results) {
    const axes = result.axes
      ? ` [empty layers ${result.axes.emptyLayers.expected}/${result.axes.emptyLayers.actual}, ` +
        `unreferenced markers ${result.axes.markers.expected.length}/${result.axes.markers.actual.length}, ` +
        `one-point edges ${result.axes.onePointEdges.expected}/${result.axes.onePointEdges.actual}, ` +
        `interaction attributes ${result.axes.interaction.expected}/${result.axes.interaction.actual}]`
      : "";
    if (result.ok) {
      console.log(
        `ok   ${result.name}: the mounted scene ≡ ${result.name}.svg (${result.nodes} ids)${axes}`,
      );
    } else {
      failures += 1;
      console.log(`FAIL ${result.name}:${axes}`);
      if (result.error) {
        console.log(`       ${result.error}`);
      }
      for (const diff of result.diffs || []) {
        console.log(`       ${diff}`);
      }
    }
  }
  const passed = results.length - failures;
  console.log("");
  console.log(`equivalence: ${passed}/${results.length} golden documents`);
  console.log(
    "modulo formatting: attribute order, whitespace, number spelling, class order, the <style> " +
      "block of the export and xmlns; the four decisions of S8 are the empty layer containers, " +
      "the filtered <defs> marker set, the one-point edges and the painter-only interaction " +
      "attributes (see the header).",
  );

  if (JSON_OUT !== null) {
    const { writeFile } = await import("node:fs/promises");
    await writeFile(JSON_OUT, `${JSON.stringify({ passed, total: results.length, results }, null, 2)}\n`);
    console.log(`report: ${JSON_OUT}`);
  }
  process.exitCode = failures === 0 ? 0 : 1;
}

main().catch((error) => {
  console.error(`equivalence: ${error.message}`);
  process.exitCode = 1;
});
