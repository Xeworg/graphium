#!/usr/bin/env node
// The parity harness: it runs the **real built bundle** of the plan against a
// faked Tauri bridge, in headless Chromium over HTTP, and checks the
// `CONTRACT.md` §8 checklist item by item.
//
// It used to be differential: every probe ran against the Svelte build and
// against the vanilla page, and an `equal` probe failed when the two differed.
// The vanilla page retired with the switch (CONTRACT.md §10), so each probe
// now asserts its own expectation on the one bundle; `equal`/`report` survive
// as checklist labels, but there is no second build to compare against.
//
// Why a browser and not the real window (the discovery T3 left in memory):
// WebKitGTK cannot be screenshot from the outside (`import -window` fails on
// it) and it cannot be driven programmatically without instrumenting the Rust
// side, which this branch may not touch. The bundle in Chromium speaks the
// same DOM, the same CSS and the same bridge protocol, so every rule of §8
// that is observable in the DOM is checked here; the in-shell half (the real
// `graphium://ready` + `render_ack` round trip through WebKitGTK) is covered by
// `scripts/smoke.sh` and recorded in the same checklist.
//
// How the bridge is faked: the script below is injected with
// `Page.addScriptToEvaluateOnNewDocument`, so it exists **before** the bundle
// (a module) evaluates — `src/lib/tauri.js` and `plan.js` read
// `window.__TAURI__` once at load time. The fake records every event the page
// emits, keeps the listeners Rust would call, and stamps each emission with the
// `requestAnimationFrame` counter (which is how C4's "two frames" is observed).
//
// The harness used to be differential on purpose: each probe ran against the
// Svelte build and against the vanilla page, and an `equal` probe failed when
// the two observed values differed. That is what "parity" meant in the A/B
// branch; with the vanilla page retired each probe is checked on its own.
//
// Usage (run from `crates/gx-app/frontend`, after `npm run build` or
// `scripts/build.sh`):
//
//   node tools/parity.mjs
//   node tools/parity.mjs --json /tmp/parity.json
//
// `--chromium <path>` (or `CHROMIUM` in the environment) picks the browser;
// the default is `chromium-browser`.
//
// Exit code: 0 when every probe passed.

import { readFile, readdir } from "node:fs/promises";
import { existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { evaluate, launch, openPage, serve } from "./lib/harness.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const APP = path.resolve(HERE, ".."); // crates/gx-app/frontend
const SVELTE_DIR = path.resolve(APP, "dist");

const argv = process.argv.slice(2);
const option = (name, fallback = null) => {
  const hit = argv.find((argument) => argument.startsWith(`--${name}=`));
  return hit === undefined ? fallback : hit.slice(name.length + 3);
};
const JSON_OUT = option("json", null);
const CHROMIUM = option("chromium", process.env.CHROMIUM || "chromium-browser");

// --- fixtures --------------------------------------------------------------

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
  {
    id: "s1",
    family: "pi",
    label: "graphium-1",
    project: "/home/xeworg/proyectos/graphium",
    state: "active",
  },
  { id: "s2", family: "claude", label: "sesión dos", state: "in_grace" },
];

function diagram(id, extra = {}) {
  return {
    diagram_id: id,
    title: `schema ${id}`,
    kind: "graph",
    updated_at: "2026-11-07T12:00:00Z",
    session_id: "s1",
    pos: { x: 100, y: 100 },
    bbox: { w: 368, h: 228 },
    current_version: 7,
    oldest_version: 1,
    version_count: 7,
    ...extra,
  };
}

function cloneEntry(extra = {}) {
  return {
    clone_id: "c-1",
    origin_diagram_id: "d-1",
    origin_session_id: "s1",
    title: "schema d-1",
    snapshot_version: 5,
    current_version: 7,
    oldest_version: 1,
    version_count: 7,
    updatable: true,
    pos: { x: 600, y: 400 },
    bbox: { w: 368, h: 228 },
    ...extra,
  };
}

// A scene as the wire carries it since `f11-elementos.md` T3/C: the harness
// speaks the elements contract, never the SVG string of the export (S1/S2).
// One node, named by `marker` — the label is how a probe tells one mounted
// drawing from another (the `data-marker` of the string era died with the
// flip). The tokens are the frozen ones of `gx-core/src/tokens.rs`.
function sceneFixture(marker, width = 320, height = 180) {
  const box = { w: Math.min(200, width * 0.6), h: 48 };
  return {
    canvas: { x: 0, y: 0, w: width, h: height },
    tokens: {
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
    },
    direction: "TB",
    layout_hint: "graph",
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
    nodes: [
      {
        id: `g-${marker}`,
        label: marker,
        shape: "box",
        emphasis: "normal",
        style: "solid",
        center: { x: width / 2, y: height / 2 },
        size: box,
        rows: [],
        meta: {},
      },
    ],
    edges: [],
    bands: [],
    events: [],
  };
}

const RENDER = {
  req_id: "r1",
  diagram_id: "d-1",
  session_id: "s1",
  elements: sceneFixture("live"),
  placement: { x: 100, y: 100 },
  camera: { tx: 0, ty: 0, zoom: 1 },
};

// --- scenarios -------------------------------------------------------------
//
// A step is `{ id, name, cdp?, js, compare? }`:
//   id       the CONTRACT.md §8 items the probe is evidence for
//   name     short evidence label used in the checklist and the report
//   cdp      browser commands to run before the probe (input, metrics)
//   js       async body evaluated in the page; its return value is reported
//   compare  'equal' (default) fails when the builds differ; 'report' only
//            records both values (a documented difference).

const scenarios = [
  {
    name: "handshake",
    bridge: "global",
    steps: [
      {
        id: ["C2", "C3"],
        name: "listeners before ready, one ready with {}",
        js: `{
          const ready = __h.emitted.find((entry) => entry.name === 'graphium://ready');
          __t.expect(ready !== undefined, 'ready was never emitted');
          const firstOther = __h.emitted.slice(0, __h.emitted.indexOf(ready)).map((entry) => entry.name);
          // The port pushes the camera copy once at mount (T2): it is the only
          // event allowed before the handshake, and it is a no-answer intent.
          __t.expect(firstOther.every((name) => name === 'camera-changed'),
            'these events left before ready: ' + firstOther.join(','));
          __t.expect(__t.count('graphium://ready') === 1, 'ready emitted more than once');
          __t.expect(JSON.stringify(ready.payload) === '{}', 'ready payload is not {}');
          __t.expect(ready.plan === true, 'ready left before #plan existed');
          const functional = ['graphium://settings', 'graphium://state', 'graphium://render',
            'graphium://version-paint', 'graphium://clone-paint', 'graphium://pending',
            'graphium://error'];
          const missing = functional.filter((name) => !__h.heard.includes(name));
          __t.expect(missing.length === 0, 'listeners registered after ready: ' + missing.join(','));
          return { heard: __h.heard.slice().sort(), beforeReady: firstOther, readyCount: 1, door: __h.door };
        }`,
        compare: "report",
      },
      {
        id: ["C4"],
        name: "render_ack ok:true after two rAF",
        js: `(async () => {
          const before = __h.raf;
          __t.rust('graphium://render', ${JSON.stringify(RENDER)});
          await __t.wait(150);
          const acks = __t.emitted('render_ack');
          __t.expect(acks.length === 1, 'expected one ack, got ' + acks.length);
          __t.expect(acks[0].req_id === 'r1' && acks[0].ok === true, 'ack is not r1/ok:true');
          const frames = __h.emitted.find((entry) => entry.name === 'render_ack').raf - before;
          __t.expect(frames >= 2, 'ack left after ' + frames + ' frames');
          __t.expect(__t.all('.diagram').length === 1, 'the object did not mount');
          return { acks: acks.length, frames };
        })()`,
      },
      {
        id: ["C21"],
        name: "no scene in payload -> ack ok:false, nothing mounts",
        js: `(async () => {
          // A payload that only carries the SVG string of the export is the
          // payload of the old contract: the window has nothing to mount.
          __t.rust('graphium://render', {
            req_id: 'r2', diagram_id: 'd-broken', session_id: 's1',
            svg: '<svg xmlns="http://www.w3.org/2000/svg" data-marker="string"><text>string</text></svg>',
            placement: { x: 0, y: 0 },
          });
          await __t.wait(150);
          const acks = __t.emitted('render_ack');
          __t.expect(acks.length === 2, 'expected a second ack');
          const last = acks[acks.length - 1];
          __t.expect(last.req_id === 'r2' && last.ok === false, 'the broken payload was accepted');
          __t.expect(__t.all('.diagram').length === 1, 'a broken payload mounted an object');
          return { acks: acks.map((entry) => entry.ok) };
        })()`,
      },
      {
        id: ["C21"],
        name: "a payload with both keys mounts the scene, never the string",
        js: `(async () => {
          // The flip of f11-elementos.md T3/C: the html branch is dead. A
          // payload that still carries a string beside the scene paints the
          // scene.
          __t.rust('graphium://render', {
            req_id: 'r-string', diagram_id: 'd-string', session_id: 's1',
            svg: '<svg xmlns="http://www.w3.org/2000/svg" data-marker="string"><text>string</text></svg>',
            elements: ${JSON.stringify(sceneFixture("scene", 200, 120))},
            placement: { x: 0, y: 0 },
          });
          await __t.wait(150);
          const object = __t.byId('d-string');
          __t.expect(object !== null, 'the object did not mount');
          __t.expect(object.querySelector('.content [data-marker]') === null,
            'the string branch is still painting {@html}');
          const marker = __t.markerOf(object.querySelector('.content'));
          __t.expect(marker === 'scene', 'the scene did not mount: ' + marker);
          return { marker };
        })()`,
      },
    ],
  },
  {
    name: "grid",
    bridge: "global",
    setup: `(async () => {
      __t.rust('graphium://settings', ${JSON.stringify(SETTINGS)});
      await __t.settle();
    })()`,
    steps: [
      {
        id: ["C7", "C9"],
        name: "the eight grid variables from the contract formulas",
        js: `{
          const plan = document.getElementById('plan');
          const names = ['--grid-angle', '--grid-sin', '--grid-cos', '--grid-tan',
            '--grid-diagonal', '--grid-color', '--grid-step', '--grid-opacity'];
          const vars = Object.fromEntries(names.map((name) => [name, plan.style.getPropertyValue(name)]));
          const radians = (45 * Math.PI) / 180;
          __t.expect(vars['--grid-angle'] === '45deg', 'angle ' + vars['--grid-angle']);
          __t.expect(vars['--grid-sin'] === Math.sin(radians).toFixed(4), 'sin ' + vars['--grid-sin']);
          __t.expect(vars['--grid-cos'] === Math.cos(radians).toFixed(4), 'cos ' + vars['--grid-cos']);
          __t.expect(vars['--grid-tan'] === Math.tan(radians).toFixed(4), 'tan ' + vars['--grid-tan']);
          __t.expect(vars['--grid-diagonal'] === '59px', 'diagonal ' + vars['--grid-diagonal']);
          __t.expect(vars['--grid-step'] === (59 * Math.sin(radians)).toFixed(2) + 'px', 'step ' + vars['--grid-step']);
          __t.expect(vars['--grid-opacity'] === '1', 'opacity ' + vars['--grid-opacity']);
          __t.expect(vars['--grid-color'] === 'rgba(255, 255, 255, 0.14)', 'colour ' + vars['--grid-color']);
          return vars;
        }`,
      },
      {
        id: ["C38", "C39"],
        name: "settings repaint data-theme, palette and the grid at once",
        js: `(async () => {
          const light = Object.assign({}, ${JSON.stringify(SETTINGS)}, {
            theme: 'light', background: '#f7f4ec', grid_color: 'rgba(0, 0, 0, 0.2)', accent: '#123456',
          });
          __t.rust('graphium://settings', light);
          await __t.settle();
          const root = document.documentElement;
          const plan = document.getElementById('plan');
          __t.expect(root.dataset.theme === 'light', 'data-theme ' + root.dataset.theme);
          __t.expect(root.style.getPropertyValue('--bg') === '#f7f4ec', '--bg did not follow');
          __t.expect(root.style.getPropertyValue('--accent') === '#123456', '--accent did not follow');
          __t.expect(plan.style.getPropertyValue('--grid-color') === 'rgba(0, 0, 0, 0.2)', 'grid colour did not follow');
          const painted = { theme: root.dataset.theme, bg: root.style.getPropertyValue('--bg') };
          __t.rust('graphium://settings', Object.assign({}, light, { theme: 'neblina-no-existe' }));
          await __t.settle();
          __t.expect(root.dataset.theme === 'dark', 'unknown theme did not fall back to dark');
          return { painted, fallback: root.dataset.theme };
        })()`,
      },
      {
        id: ["C8"],
        name: "grid hides when grid_visible is false and below zoom 0.3",
        js: `(async () => {
          const plan = document.getElementById('plan');
          __t.rust('graphium://settings', Object.assign({}, ${JSON.stringify(SETTINGS)}, { grid_visible: false }));
          await __t.settle();
          const hiddenBySettings = plan.style.getPropertyValue('--grid-opacity');
          __t.expect(hiddenBySettings === '0', 'grid_visible false did not hide the grid');
          __t.rust('graphium://settings', ${JSON.stringify(SETTINGS)});
          await __t.settle();
          return { hiddenBySettings, back: plan.style.getPropertyValue('--grid-opacity') };
        })()`,
      },
      {
        id: ["C10"],
        name: "zoom clamps to [0.05, 4.0], step 1.1, anchor under the cursor",
        cdp: [
          { type: "wait", ms: 60 },
          { type: "wheel", x: 300, y: 250, deltaY: -100 },
          { type: "wait", ms: 60 },
        ],
        js: `(async () => {
          await __t.wait(80);
          const after = __t.cam();
          const world = { x: (300 - after.tx) / after.zoom, y: (250 - after.ty) / after.zoom };
          __t.expect(__t.near(after.zoom, 1.1, 0.001), 'one step is not 1.1: ' + after.zoom);
          __t.expect(__t.near(world.x, 300, 0.05) && __t.near(world.y, 250, 0.05),
            'the world point under the cursor moved: ' + JSON.stringify(world));
          return { zoom: after.zoom, world };
        })()`,
      },
      {
        id: ["C10"],
        name: "the clamps hold at both ends",
        cdp: [
          ...Array.from({ length: 22 }, () => ({ type: "wheel", x: 640, y: 500, deltaY: -100 })),
          { type: "wait", ms: 120 },
        ],
        js: `(async () => {
          await __t.wait(80);
          const high = __t.cam();
          __t.expect(__t.near(high.zoom, 4.0, 1e-6), 'zoom did not clamp at 4.0: ' + high.zoom);
          return { high: high.zoom };
        })()`,
      },
      {
        id: ["C10"],
        name: "the low clamp holds at 0.05",
        cdp: [
          ...Array.from({ length: 60 }, () => ({ type: "wheel", x: 640, y: 500, deltaY: 100 })),
          { type: "wait", ms: 150 },
        ],
        js: `(async () => {
          await __t.wait(120);
          const low = __t.cam();
          __t.expect(__t.near(low.zoom, 0.05, 1e-6), 'zoom did not clamp at 0.05: ' + low.zoom);
          return { low: low.zoom };
        })()`,
      },
      {
        id: ["C8"],
        name: "the grid is hidden below zoom 0.3",
        js: `{
          const opacity = document.getElementById('plan').style.getPropertyValue('--grid-opacity');
          __t.expect(opacity === '0', 'the grid still paints at zoom ' + __t.cam().zoom);
          return { opacity, zoom: __t.cam().zoom };
        }`,
      },
    ],
  },
  {
    name: "camera",
    bridge: "global",
    setup: `(async () => {
      __t.rust('graphium://settings', ${JSON.stringify(SETTINGS)});
      __t.rust('graphium://state', ${JSON.stringify({
        diagrams: [diagram("d-1")],
        sessions: SESSIONS,
        clones: [],
      })});
      await __t.settle();
      // Back to zoom 1 after the clamp probes: the intent is the reset door.
      __t.rust('graphium://render', ${JSON.stringify({
        ...RENDER,
        req_id: "r0",
        camera: { tx: 0, ty: 0, zoom: 1 },
      })});
      await __t.wait(150);
    })()`,
    steps: [
      {
        id: ["C5"],
        quiet: true,
        name: "baseline of the camera events",
        js: `{ window.__cameraBaseline = __t.count('camera-changed'); return { baseline: window.__cameraBaseline }; }`,
      },
      {
        id: ["C5"],
        name: "camera-changed is one 100 ms trailing event with {tx,ty,zoom}",
        cdp: [
          { type: "wheel", x: 400, y: 400, deltaY: -100 },
          { type: "wait", ms: 10 },
          { type: "wheel", x: 401, y: 401, deltaY: -100 },
          { type: "wait", ms: 10 },
          { type: "wheel", x: 402, y: 402, deltaY: -100 },
        ],
        js: `(async () => {
          await __t.wait(250);
          const events = __t.emitted('camera-changed').slice(window.__cameraBaseline || 0);
          __t.expect(events.length === 1, 'a burst of three wheels produced ' + events.length + ' events');
          const keys = Object.keys(events[0]).sort();
          __t.expect(JSON.stringify(keys) === JSON.stringify(['tx', 'ty', 'zoom']), 'payload keys: ' + keys.join(','));
          const camera = __t.cam();
          __t.expect(__t.near(events[0].zoom, camera.zoom, 1e-9), 'the copy is not the camera');
          return { events: events.length, keys };
        })()`,
      },
      {
        id: ["C12"],
        name: "a camera intent sets the camera and cancels the pan",
        js: `(async () => {
          __t.rust('graphium://render', {
            req_id: 'r-intent', diagram_id: 'd-intent', session_id: 's2',
            elements: ${JSON.stringify(sceneFixture("intent", 200, 120))},
            placement: { x: 900, y: 500 },
            camera: { tx: 50, ty: 60, zoom: 2 },
          });
          await __t.wait(40);
          const immediately = __t.cam();
          await __t.wait(400);
          const later = __t.cam();
          __t.expect(__t.near(immediately.tx, 50, 0.01) && __t.near(immediately.ty, 60, 0.01),
            'intent not applied at once: ' + JSON.stringify(immediately));
          __t.expect(__t.near(later.tx, 50, 0.01) && __t.near(later.zoom, 2, 0.001),
            'the camera moved after the intent: ' + JSON.stringify(later));
          return { immediately, later };
        })()`,
      },
      {
        id: ["C12"],
        name: "without an intent the second object pans and never zooms",
        js: `(async () => {
          const from = __t.cam();
          __t.rust('graphium://render', {
            req_id: 'r-pan', diagram_id: 'd-pan', session_id: 's2',
            elements: ${JSON.stringify(sceneFixture("pan", 240, 140))},
            placement: { x: 1200, y: 900 },
          });
          await __t.wait(60);
          const middle = __t.cam();
          await __t.wait(500);
          const end = __t.cam();
          __t.expect(__t.near(end.zoom, from.zoom, 1e-9), 'the pan changed the zoom');
          __t.expect(!__t.near(middle.tx, end.tx, 0.5), 'the camera jumped instead of panning');
          const plan = document.getElementById('plan');
          const expected = {
            tx: plan.clientWidth / 2 - (1200 + (240 + 48) / 2) * from.zoom,
            ty: plan.clientHeight / 2 - (900 + (140 + 48) / 2) * from.zoom,
          };
          __t.expect(__t.near(end.tx, expected.tx, 0.6) && __t.near(end.ty, expected.ty, 0.6),
            'the pan did not land on the object: ' + JSON.stringify({ end, expected }));
          // The middle sample is one frame of the animation: its exact value is timing,
          // not contract. What is contract is that it was not the end.
          return { from, panned: true, end, expected };
        })()`,
      },
      {
        id: ["C11"],
        name: "a drag on the background pans by the pointer delta",
        cdp: [
          { type: "mouse", sub: "mousePressed", x: 900, y: 600, button: "left", buttons: 1 },
          { type: "mouse", sub: "mouseMoved", x: 950, y: 630, button: "left", buttons: 1 },
          { type: "mouse", sub: "mouseReleased", x: 950, y: 630, button: "left", buttons: 0 },
        ],
        js: `(async () => {
          await __t.wait(120);
          const camera = __t.cam();
          const events = __t.emitted('camera-changed');
          __t.expect(events.length >= 1, 'the drag emitted no camera-changed');
          return { camera, emitted: true };
        })()`,
      },
      {
        id: ["C13"],
        name: "a resize keeps the world point under the centre",
        js: `(async () => {
          const before = __t.cam();
          const view = {
            w: document.getElementById('plan').clientWidth,
            h: document.getElementById('plan').clientHeight,
          };
          window.__resizeBefore = { before, view };
          return { before, view };
        })()`,
      },
      {
        id: ["C13"],
        name: "after the viewport grew, the same point stays centered",
        cdp: [{ type: "metrics", width: 1100, height: 720 }],
        js: `(async () => {
          await __t.wait(250);
          const before = window.__resizeBefore;
          const after = __t.cam();
          const view = {
            w: document.getElementById('plan').clientWidth,
            h: document.getElementById('plan').clientHeight,
          };
          const center = {
            x: (before.view.w / 2 - before.before.tx) / before.before.zoom,
            y: (before.view.h / 2 - before.before.ty) / before.before.zoom,
          };
          const now = {
            x: (view.w / 2 - after.tx) / after.zoom,
            y: (view.h / 2 - after.ty) / after.zoom,
          };
          __t.expect(__t.near(center.x, now.x, 0.5) && __t.near(center.y, now.y, 0.5),
            'the centre moved: ' + JSON.stringify({ center, now, view }));
          __t.expect(__t.near(after.zoom, before.before.zoom, 1e-9), 'the resize changed the zoom');
          return { center, now, view };
        })()`,
      },
    ],
  },
  {
    name: "objects",
    bridge: "global",
    setup: `(async () => {
      __t.rust('graphium://settings', ${JSON.stringify(SETTINGS)});
      __t.rust('graphium://state', ${JSON.stringify({
        diagrams: [],
        sessions: SESSIONS,
        clones: [],
      })});
      await __t.settle();
      __t.rust('graphium://render', ${JSON.stringify(RENDER)});
      await __t.wait(200);
    })()`,
    steps: [
      {
        id: ["C20"],
        name: "placement is the object corner and the SVG sits 24 px inside",
        js: `{
          const content = __t.byId("d-1").querySelector('.content');
          const box = __t.byId("d-1").querySelector('.diagram-box');
          const style = (element) => ({
            left: parseFloat(element.style.left), top: parseFloat(element.style.top),
            width: parseFloat(element.style.width), height: parseFloat(element.style.height),
          });
          const c = style(content);
          const b = style(box);
          __t.expect(c.left === 124 && c.top === 124, 'content is not one padding inside: ' + JSON.stringify(c));
          __t.expect(c.width === 320 && c.height === 180, 'content size ' + JSON.stringify(c));
          __t.expect(b.left === 100 && b.top === 100, 'box is not the placement: ' + JSON.stringify(b));
          __t.expect(b.width === 368 && b.height === 228, 'box does not carry the padding: ' + JSON.stringify(b));
          return { content: c, box: b };
        }`,
      },
      {
        id: ["C22", "C23"],
        name: "the state is the only source of the panel rows",
        js: `(async () => {
          __t.rust('graphium://state', ${JSON.stringify({
            diagrams: [diagram("d-1"), diagram("d-2", {
              title: "otro plano", kind: "mindmap", session_id: "s2",
              pos: { x: 900, y: 700 }, bbox: { w: 200, h: 120 },
              updated_at: "2026-01-02T03:04:00Z",
            })],
            sessions: SESSIONS,
            clones: [],
          })});
          await __t.settle();
          const rows = __t.all('#panel-list .panel-item');
          __t.expect(rows.length === 2, 'panel has ' + rows.length + ' rows');
          const first = rows[0];
          const title = first.querySelector('.panel-item-title').textContent.trim();
          const kind = first.querySelector('.panel-item-kind').textContent.trim();
          const updated = first.querySelector('.panel-item-updated').textContent.trim();
          __t.expect(title === 'schema d-1', 'title ' + title);
          __t.expect(kind === 'graph', 'kind ' + kind);
          __t.expect(updated !== '', 'no updated text');
          __t.expect(__t.q('#panel-empty').classList.contains('hidden'), 'the empty message is still painted');
          return { rows: rows.length, title, kind, updated };
        })()`,
      },
      {
        id: ["C24"],
        name: "the panel starts open and the toggle flips aria-expanded",
        js: `(async () => {
          __t.expect(__t.attr('#panel-toggle', 'aria-expanded') === 'true', 'the panel did not start open');
          __t.expect(__t.attr('#panel-toggle', 'aria-controls') === 'panel', 'aria-controls is not panel');
          __t.click('#panel-toggle');
          await __t.settle();
          const closed = { expanded: __t.attr('#panel-toggle', 'aria-expanded'), hidden: !__t.shown('#panel') };
          __t.expect(closed.expanded === 'false' && closed.hidden === true, 'the toggle did not close the panel');
          __t.click('#panel-toggle');
          await __t.settle();
          __t.expect(__t.attr('#panel-toggle', 'aria-expanded') === 'true', 'the toggle did not reopen the panel');
          return { closed, reopened: __t.shown('#panel') };
        })()`,
      },
      {
        id: ["C23"],
        name: "selecting a row marks it, moves the camera to it and pans",
        js: `(async () => {
          const rows = __t.all('#panel-list .panel-item');
          const target = rows.find((row) => row.querySelector('.panel-item-title').textContent.trim() === 'otro plano');
          __t.click(target.querySelector('.panel-item-open'));
          await __t.wait(450);
          const selected = __t.all('#panel-list .panel-item.selected');
          __t.expect(selected.length === 1, 'selected rows: ' + selected.length);
          __t.expect(selected[0].querySelector('.panel-item-title').textContent.trim() === 'otro plano', 'the wrong row is marked');
          __t.expect(__t.attr('#panel-list .panel-item.selected .panel-item-open', 'aria-current') === 'true', 'aria-current not set');
          const camera = __t.cam();
          const plan = document.getElementById('plan');
          const expected = {
            tx: plan.clientWidth / 2 - (900 + 100) * camera.zoom,
            ty: plan.clientHeight / 2 - (700 + 60) * camera.zoom,
          };
          __t.expect(__t.near(camera.tx, expected.tx, 0.6) && __t.near(camera.ty, expected.ty, 0.6),
            'the camera is not on the chosen object: ' + JSON.stringify({ camera, expected }));
          return { selected: 1, camera, expected };
        })()`,
      },
      {
        id: ["C23"],
        name: "x emits close and the clone button emits clone",
        js: `(async () => {
          const row = __t.all('#panel-list .panel-item')[1];
          __t.click(row.querySelector('.panel-item-close'));
          __t.click(row.querySelector('.panel-item-clone'));
          await __t.settle();
          const closes = __t.emitted('graphium://close');
          const clones = __t.emitted('graphium://clone');
          __t.expect(closes.length === 1 && closes[0].diagram_id === 'd-2', 'close payload ' + JSON.stringify(closes));
          __t.expect(clones.length === 1 && clones[0].diagram_id === 'd-2', 'clone payload ' + JSON.stringify(clones));
          return { closes, clones };
        })()`,
      },
      {
        id: ["C25"],
        name: "the version indicator reads v7/7 with the ends disabled",
        js: `{
          const row = __t.all('#panel-list .panel-item')[0];
          const label = row.querySelector('.panel-item-version-label').textContent.trim();
          const steps = Array.from(row.querySelectorAll('.panel-item-version-step'));
          __t.expect(label === 'v7/7', 'label ' + label);
          __t.expect(steps.length === 2, 'expected two step buttons');
          __t.expect(steps[0].disabled === false, 'the oldest step is disabled at the live drawing');
          __t.expect(steps[1].disabled === true, 'the newest step is enabled at the live drawing');
          __t.expect(steps[0].textContent.trim() === '◀' && steps[1].textContent.trim() === '▶', 'the steps are not the arrows');
          return { label, prev: steps[0].disabled, next: steps[1].disabled };
        }`,
      },
      {
        id: ["C26", "C27"],
        name: "◀ asks for the older version and the preview replaces the drawing in place",
        js: `(async () => {
          // The version history of a diagram is walked on the plan it belongs
          // to: the human chooses the row first, as the panel is meant to be
          // used.
          const row = __t.all('#panel-list .panel-item')
            .find((item) => item.querySelector('.panel-item-title').textContent.trim() === 'schema d-1');
          __t.click(row.querySelector('.panel-item-open'));
          await __t.wait(450);
          const before = __t.byId("d-1").querySelector('.content');
          const widthBefore = before.style.width;
          __t.click(row.querySelectorAll('.panel-item-version-step')[0]);
          await __t.settle();
          const events = __t.emitted('graphium://version-view');
          __t.expect(events.length === 1, 'version-view events: ' + events.length);
          __t.expect(events[0].diagram_id === 'd-1' && events[0].version === 6, 'payload ' + JSON.stringify(events[0]));
          const label = __t.all('#panel-list .panel-item')[0].querySelector('.panel-item-version-label').textContent.trim();
          __t.expect(label === 'v6/7', 'the indicator did not follow the view: ' + label);
          __t.rust('graphium://version-paint', { diagram_id: 'd-1', version: 6, elements: ${JSON.stringify(sceneFixture("preview", 200, 120))} });
          await __t.settle();
          const preview = __t.markerOf(__t.byId("d-1").querySelector('.content'));
          __t.expect(preview === 'preview', 'the preview did not mount: ' + preview);
          __t.expect(preview !== 'live', 'the live drawing is still mounted');
          const box = __t.byId("d-1").querySelector('.diagram-box');
          __t.expect(parseFloat(box.style.width) === 368 && parseFloat(box.style.left) === 100, 'the object rectangle moved');
          return { events, label, widthBefore };
        })()`,
      },
      {
        id: ["C26", "C19"],
        name: "▶ back to the newest restores the live drawing with no event",
        js: `(async () => {
          const row = __t.all('#panel-list .panel-item')
            .find((item) => item.querySelector('.panel-item-title').textContent.trim() === 'schema d-1');
          __t.click(row.querySelectorAll('.panel-item-version-step')[1]);
          await __t.settle();
          const events = __t.emitted('graphium://version-view');
          __t.expect(events.length === 1, 'walking forward emitted an event');
          const label = __t.all('#panel-list .panel-item')[0].querySelector('.panel-item-version-label').textContent.trim();
          __t.expect(label === 'v7/7', 'the indicator did not return: ' + label);
          const live = __t.markerOf(__t.byId("d-1").querySelector('.content'));
          __t.expect(live === 'live', 'the live drawing did not come back: ' + live);
          // A re-render of the same id replaces the object and drops a preview.
          __t.rust('graphium://render', ${JSON.stringify({ ...RENDER, req_id: "r-again", elements: sceneFixture("second") })});
          await __t.wait(200);
          const objects = __t.all('.diagram').filter((element) => (element.dataset.id || element.diagramId) === 'd-1');
          __t.expect(objects.length === 1, 'a re-render left ' + objects.length + ' objects');
          __t.expect(__t.markerOf(__t.byId("d-1").querySelector('.content')) === 'second',
            'the second drawing did not replace the first');
          return { events: events.length, objects: objects.length };
        })()`,
      },
      {
        id: ["C18", "C17"],
        name: "the state drops objects, clears the selection and refreshes titles",
        js: `(async () => {
          __t.rust('graphium://state', ${JSON.stringify({
            diagrams: [diagram("d-1", { title: "renombrado" }), diagram("d-2", { title: "otro plano", session_id: "s2" })],
            sessions: SESSIONS,
            clones: [],
          })});
          await __t.settle();
          const titles = __t.all('#panel-list .panel-item-title').map((element) => element.textContent.trim());
          __t.expect(titles.includes('renombrado'), 'the new title did not arrive: ' + titles.join('|'));
          // The human chooses the diagram that is about to disappear: its
          // selection has to clear with it.
          const row = __t.all('#panel-list .panel-item')
            .find((item) => item.querySelector('.panel-item-title').textContent.trim() === 'renombrado');
          __t.click(row.querySelector('.panel-item-open'));
          await __t.wait(450);
          __t.expect(__t.all('#panel-list .panel-item.selected').length === 1, 'the row was not selected');
          __t.rust('graphium://state', ${JSON.stringify({
            diagrams: [diagram("d-2", { title: "otro plano", session_id: "s2" })],
            sessions: SESSIONS,
            clones: [],
          })});
          await __t.settle();
          __t.expect(__t.byId("d-1") === null, 'the dropped object is still on the plan');
          __t.expect(__t.all('#panel-list .panel-item').length === 1, 'the panel still lists the dropped diagram');
          __t.expect(__t.all('#panel-list .panel-item.selected').length === 0, 'the selection survived the drop');
          return { titles, remaining: __t.all('.diagram').length };
        })()`,
      },
    ],
  },
  {
    name: "culling",
    bridge: "global",
    setup: `(async () => {
      __t.rust('graphium://settings', ${JSON.stringify(SETTINGS)});
      __t.rust('graphium://state', ${JSON.stringify({
        diagrams: [],
        sessions: [{ id: "s1", family: "pi", label: "graphium-1", state: "active" }],
        clones: [],
      })});
      await __t.settle();
      // The fixture of one culling render: the same scene with the node the
      // probe calls by name.
      const scene = (marker) => {
        const value = ${JSON.stringify(sceneFixture("a", 200, 120))};
        value.nodes[0].id = 'g-' + marker;
        value.nodes[0].label = marker;
        return value;
      };
      const placements = [
        ['d-1', 300, 300, 'a'], ['d-2', 340, 320, 'b'], ['d-3', 380, 340, 'c'],
        ['d-4', 900, 600, 'd'], ['d-5', 2000, 1500, 'e'],
      ];
      for (const [id, x, y, marker] of placements) {
        __t.rust('graphium://render', {
          req_id: 'r-' + id, diagram_id: id, session_id: 's1',
          elements: scene(marker),
          placement: { x, y },
          camera: id === 'd-1' ? { tx: 0, ty: 0, zoom: 1 } : undefined,
        });
        await __t.wait(30);
      }
      await __t.wait(400);
    })()`,
    steps: [
      {
        id: ["C14"],
        name: "at most three live scenes, the closest to the view centre",
        js: `(async () => {
          // The renders pan the camera; the focus intent puts it back on the
          // origin so the culling is judged against a known window.
          __t.rust('graphium://render', {
            req_id: 'r-reset', diagram_id: 'd-1', session_id: 's1',
            elements: ${JSON.stringify(sceneFixture("a", 200, 120))}, placement: { x: 300, y: 300 },
            camera: { tx: 0, ty: 0, zoom: 1 }, focus: { x: 0, y: 0, w: 1, h: 1 },
          });
          await __t.wait(200);
          const live = __t.all('.diagram:not(.plan-hidden) .content svg.graphium');
          __t.expect(live.length === 3, 'live scenes: ' + live.length);
          const boxes = __t.all('.diagram:not(.plan-hidden) .diagram-box:not(.hidden)');
          __t.expect(boxes.length === 2, 'painted boxes: ' + boxes.length);
          const markers = __t.all('.diagram:not(.plan-hidden) .content svg.graphium').map(__t.markerOf).sort();
          __t.expect(JSON.stringify(markers) === JSON.stringify(['a', 'b', 'c']),
            'the tie-break kept the wrong objects: ' + markers.join(','));
          return { live: live.length, boxes: boxes.length, markers };
        })()`,
      },
      {
        id: ["C15"],
        name: "a culled object is a box with title and size in plan units",
        js: `{
          const boxes = __t.all('.diagram:not(.plan-hidden) .diagram-box:not(.hidden)')
            .map((box) => box.querySelector('.diagram-box-label').textContent.trim()).sort();
          __t.expect(boxes.length === 2, 'boxes: ' + boxes.length);
          __t.expect(boxes.every((label) => / — \\d+ × \\d+$/.test(label)), 'box text: ' + boxes.join(' | '));
          __t.expect(boxes.includes('d-4 — 248 × 168'), 'no fallback to the diagram id: ' + boxes.join(' | '));
          return { boxes };
        }`,
      },
      {
        id: ["C14"],
        name: "below zoom 0.4 every drawing is a box",
        cdp: [
          ...Array.from({ length: 14 }, () => ({ type: "wheel", x: 640, y: 400, deltaY: 100 })),
          { type: "wait", ms: 150 },
        ],
        js: `(async () => {
          await __t.wait(150);
          __t.expect(__t.cam().zoom < 0.4, 'zoom ' + __t.cam().zoom);
          const live = __t.all('.diagram:not(.plan-hidden) .content svg.graphium');
          __t.expect(live.length === 0, 'live scenes at zoom ' + __t.cam().zoom + ': ' + live.length);
          return { zoom: __t.cam().zoom, live: live.length };
        })()`,
      },
      {
        id: ["C16"],
        name: "the first object of a plan frames; plan-hidden hides without closing",
        js: `(async () => {
          // d-9 arrives before the state that lists it: objects enter the plan
          // through a render, the state only reconciles them.
          __t.rust('graphium://render', {
            req_id: 'r-reset', diagram_id: 'd-1', session_id: 's1',
            elements: ${JSON.stringify(sceneFixture("a", 200, 120))}, placement: { x: 300, y: 300 },
            camera: { tx: 0, ty: 0, zoom: 1 }, focus: { x: 0, y: 0, w: 1, h: 1 },
          });
          await __t.wait(120);
          __t.rust('graphium://render', {
            req_id: 'r-nine', diagram_id: 'd-9', session_id: 's2',
            elements: ${JSON.stringify(sceneFixture("nine", 200, 120))}, placement: { x: 700, y: 700 },
            camera: { tx: 100, ty: 100, zoom: 1 },
          });
          await __t.wait(200);
          __t.rust('graphium://state', {
            diagrams: [
              { diagram_id: "d-1", title: "uno", kind: "graph", updated_at: "2026-11-07T12:00:00Z", session_id: "s1", pos: { x: 300, y: 300 }, bbox: { w: 248, h: 168 }, current_version: 1, oldest_version: 1, version_count: 1 },
              { diagram_id: "d-9", title: "nueve", kind: "graph", updated_at: "2026-11-07T12:00:00Z", session_id: "s2", pos: { x: 700, y: 700 }, bbox: { w: 248, h: 168 }, current_version: 1, oldest_version: 1, version_count: 1 },
            ],
            sessions: ${JSON.stringify(SESSIONS)},
            clones: [],
          });
          await __t.wait(100);
          const nine = __t.byId("d-9");
          __t.expect(nine !== null, 'the second plan object does not exist');
          // The render of d-9 brought its plan on screen; the human goes back
          // to the first one from the switcher.
          const rows = __t.all('#session-list .session-row');
          __t.click(rows.find((row) => row.textContent.includes('graphium-1')));
          await __t.wait(150);
          __t.expect(__t.byId("d-9").classList.contains('plan-hidden') === true,
            'the object of the other plan is not hidden');
          __t.expect(__t.byId("d-1").classList.contains('plan-hidden') === false,
            'the object of the plan on screen is hidden');
          return { hiddenOther: true };
        })()`,
      },
      {
        id: ["C16"],
        name: "each plan keeps its own camera",
        js: `(async () => {
          const before = __t.cam();
          const rows = __t.all('#session-list .session-row');
          const other = rows.find((row) => row.textContent.includes('sesión dos'));
          __t.click(other);
          await __t.wait(120);
          const otherCam = __t.cam();
          __t.expect(__t.byId("d-1").classList.contains('plan-hidden'), 'the first plan did not hide');
          __t.expect(!__t.byId("d-9").classList.contains('plan-hidden'), 'the second plan did not show');
          const back = rows.find((row) => row.textContent.includes('graphium-1'));
          __t.click(back);
          await __t.wait(150);
          const returned = __t.cam();
          __t.expect(__t.near(returned.tx, before.tx, 1e-6) && __t.near(returned.ty, before.ty, 1e-6),
            'the camera of the first plan was not restored: ' + JSON.stringify({ before, returned }));
          return { before, otherCam, returned };
        })()`,
      },
    ],
  },
  {
    name: "switcher",
    bridge: "global",
    setup: `(async () => {
      __t.rust('graphium://settings', ${JSON.stringify(SETTINGS)});
      __t.rust('graphium://state', ${JSON.stringify({
        diagrams: [diagram("d-1", { session_id: "s1" })],
        sessions: SESSIONS,
        clones: [],
      })});
      await __t.settle();
    })()`,
    steps: [
      {
        id: ["C28"],
        name: "Mi plano first, then one row per session with its family and state",
        js: `(async () => {
          __t.click('#sessions-toggle');
          await __t.settle();
          const rows = __t.all('#session-list .session-row');
          __t.expect(rows.length === 3, 'rows: ' + rows.length);
          __t.expect(rows[0].classList.contains('user-plan'), 'Mi plano is not first');
          __t.expect(rows[0].querySelector('.session-label').textContent.trim() === 'Mi plano', 'first label');
          const first = rows[1];
          __t.expect(first.querySelector('.session-label').textContent.trim() === 'graphium-1', 'session label');
          __t.expect(first.querySelector('.session-family').textContent.trim() === 'Pi', 'family name');
          __t.expect(first.querySelector('.session-project').textContent.trim() === 'graphium', 'project last segment');
          __t.expect(first.querySelector('.session-state').textContent.trim() === 'activa', 'state text');
          __t.expect(first.querySelector('.session-count').textContent.trim() === '1', 'count text');
          __t.expect(first.querySelector('.session-count').title === '1 diagrama', 'count title');
          const second = rows[2];
          __t.expect(second.querySelector('.session-project').textContent.trim() === 'sin proyecto', 'missing project fallback');
          __t.expect(second.querySelector('.session-state').textContent.trim() === 'en gracia', 'grace text');
          __t.expect(second.classList.contains('in-grace'), 'the grace class is missing');
          __t.expect(second.querySelector('.session-dot').classList.contains('family-claude'), 'family dot');
          __t.expect(rows.filter((row) => row.classList.contains('selected')).length === 1, 'the active plan is not marked once');
          return {
            rows: rows.map((row) => row.querySelector('.session-label').textContent.trim()),
            family: first.querySelector('.session-family').textContent.trim(),
            project: first.querySelector('.session-project').textContent.trim(),
          };
        })()`,
      },
      {
        id: ["C29"],
        name: "a session that vanished shows the notice and switches the plan",
        js: `(async () => {
          __t.rust('graphium://state', ${JSON.stringify({
            diagrams: [{ ...diagram("d-2"), session_id: "s2", title: "dos" }],
            sessions: [{ id: "s2", family: "claude", label: "sesión dos", state: "in_grace" }],
            clones: [],
          })});
          await __t.settle();
          const notice = __t.text('#session-notice');
          __t.expect(notice === 'la sesión «graphium-1» terminó; muestro el plano de «sesión dos»',
            'notice: ' + notice);
          __t.expect(__t.shown('#session-notice') === true, 'the notice is hidden');
          const selected = __t.all('#session-list .session-row.selected');
          __t.expect(selected.length === 1, 'selected rows: ' + selected.length);
          __t.expect(selected[0].querySelector('.session-label').textContent.trim() === 'sesión dos', 'the plan did not switch');
          return { notice };
        })()`,
      },
      {
        id: ["C34"],
        name: "the two empty states belong to the two kinds of plan",
        js: `(async () => {
          __t.rust('graphium://state', { diagrams: [], sessions: [{ id: "s2", family: "claude", label: "sesión dos", state: "in_grace" }], clones: [] });
          await __t.settle();
          const session = { empty: __t.shown('#empty'), user: __t.shown('#empty-user') };
          __t.expect(session.empty === true && session.user === false, 'session plan empty states: ' + JSON.stringify(session));
          const rows = __t.all('#session-list .session-row');
          __t.click(rows[0]);
          await __t.settle();
          const user = { empty: __t.shown('#empty'), user: __t.shown('#empty-user') };
          __t.expect(user.empty === false && user.user === true, 'user plan empty states: ' + JSON.stringify(user));
          return { session, user, texts: [__t.text('#empty .empty-title'), __t.text('#empty-user .empty-title')] };
        })()`,
      },
    ],
  },
  {
    name: "clone",
    bridge: "global",
    setup: `(async () => {
      __t.rust('graphium://settings', ${JSON.stringify(SETTINGS)});
      __t.rust('graphium://state', ${JSON.stringify({
        diagrams: [diagram("d-1")],
        sessions: SESSIONS,
        clones: [],
      })});
      await __t.settle();
      __t.rust('graphium://render', ${JSON.stringify(RENDER)});
      await __t.wait(200);
      __t.rust('graphium://clone-paint', ${JSON.stringify({
        ...cloneEntry(),
        version: 7,
        elements: sceneFixture("clone"),
        bring: true,
      })});
      await __t.wait(500);
      __t.rust('graphium://state', ${JSON.stringify({
        diagrams: [diagram("d-1")],
        sessions: SESSIONS,
        clones: [cloneEntry()],
      })});
      await __t.settle();
    })()`,
    steps: [
      {
        id: ["C11"],
        quiet: true,
        name: "the screen point of the clone",
        js: `{
          const content = __t.q('.diagram.clone .content');
          const rect = content.getBoundingClientRect();
          window.__point = { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
          return { point: window.__point, rect: { x: rect.x, y: rect.y, w: rect.width, h: rect.height } };
        }`,
      },
      {
        id: ["C33"],
        name: "a clone paint with bring brings the user plan on screen",
        js: `{
          const clone = __t.q('.diagram.clone');
          __t.expect(clone !== null, 'the clone object did not mount');
          __t.expect(clone.classList.contains('plan-hidden') === false, 'the user plan did not come on screen');
          __t.expect(__t.byId("d-1").classList.contains('plan-hidden'), 'the session object is still painted');
          __t.expect(__t.shown('#empty-user') === false, 'the user empty state still paints with a clone');
          __t.expect(__t.markerOf(__t.q('.diagram.clone .content')) === 'clone', 'the clone drawing did not mount');
          return { clone: true, drawing: true, point: window.__point };
        }`,
      },
      {
        id: ["C31"],
        name: "the pill: ◀ n/m ▶, refresh state, the × inside it",
        js: `{
          const pill = __t.q('.diagram.clone .diagram-clone-tools');
          __t.expect(pill !== null, 'the pill did not render');
          const label = pill.querySelector('.panel-item-version-label').textContent.trim();
          // The indicator names the version in view, which for a fresh clone
          // is its photo (the snapshot version), without the v prefix.
          __t.expect(/^\\d+\\/7$/.test(label), 'the clone indicator: ' + label);
          const refresh = pill.querySelector('.clone-refresh');
          __t.expect(refresh.disabled === false, 'refresh is disabled while updatable');
          __t.expect(refresh.title === 'actualizar el clon', 'refresh title ' + refresh.title);
          const steps = Array.from(pill.querySelectorAll('.panel-item-version-step'));
          __t.expect(steps[0].disabled === false, 'the oldest clone step is disabled while the photo is browsable');
          __t.expect(steps[1].disabled === false, 'the newest clone step is disabled at a photo older than the original');
          const close = pill.querySelector('.clone-close');
          __t.expect(close !== null, 'the × is not inside the pill');
          return { label, refresh: refresh.disabled, title: refresh.title };
        }`,
      },
      {
        id: ["C31"],
        name: "a clone whose original closed disables refresh with its title",
        js: `(async () => {
          const label = __t.q('.diagram.clone .panel-item-version-label').textContent.trim();
          __t.rust('graphium://state', ${JSON.stringify({
            diagrams: [diagram("d-1")],
            sessions: SESSIONS,
            clones: [{ ...cloneEntry(), updatable: false }],
          })});
          await __t.settle();
          const pill = __t.q('.diagram.clone .diagram-clone-tools');
          const refresh = pill.querySelector('.clone-refresh');
          const steps = Array.from(pill.querySelectorAll('.panel-item-version-step'));
          __t.expect(refresh.disabled === true, 'refresh stayed enabled');
          __t.expect(refresh.title === 'el original ya no está en el plan', 'title ' + refresh.title);
          __t.expect(steps.every((step) => step.disabled === true), 'the history is still browsable');
          __t.expect(__t.q('.diagram.clone .panel-item-version-label').textContent.trim() === label, 'the indicator moved');
          return { disabled: refresh.disabled, title: refresh.title, label };
        })()`,
      },
      {
        id: ["C31"],
        name: "the × inside the pill emits clone-remove",
        js: `(async () => {
          __t.click('.diagram.clone .clone-close');
          await __t.settle();
          const events = __t.emitted('graphium://clone-remove');
          __t.expect(events.length === 1 && events[0].clone_id === 'c-1', 'payload ' + JSON.stringify(events));
          return { events };
        })()`,
      },
      {
        id: ["C32"],
        name: "refresh asks for {clone_id} alone; a step asks with its version",
        js: `(async () => {
          __t.rust('graphium://state', ${JSON.stringify({
            diagrams: [diagram("d-1")],
            sessions: SESSIONS,
            clones: [cloneEntry()],
          })});
          await __t.settle();
          __t.click('.diagram.clone .clone-refresh');
          await __t.settle();
          __t.click('.diagram.clone .clone-close');
          const views = __t.emitted('graphium://clone-view');
          __t.expect(views.length >= 1, 'refresh emitted nothing');
          const refresh = views[views.length - 1];
          __t.expect(refresh.clone_id === 'c-1' && !('version' in refresh), 'refresh payload ' + JSON.stringify(refresh));
          return { views };
        })()`,
      },
      {
        id: ["C30", "C32"],
        name: "the arrows walk the clone history and never steal a field's arrows",
        cdp: [
          { type: "mouse", sub: "mousePressed", point: true, button: "left", buttons: 1 },
          { type: "mouse", sub: "mouseReleased", point: true, button: "left", buttons: 0 },
          { type: "wait", ms: 120 },
        ],
        js: `(async () => {
          await __t.wait(80);
          __t.rust('graphium://state', ${JSON.stringify({
            diagrams: [diagram("d-1")],
            sessions: SESSIONS,
            clones: [cloneEntry()],
          })});
          await __t.settle();
          const before = __t.emitted('graphium://clone-view').length;
          const prevented = __t.key('ArrowLeft');
          await __t.settle();
          const views = __t.emitted('graphium://clone-view');
          const step = views[views.length - 1];
          __t.expect(views.length === before + 1, 'the arrow emitted ' + (views.length - before));
          __t.expect(step.clone_id === 'c-1' && step.version === 4, 'step payload ' + JSON.stringify(step));
          __t.expect(prevented === true, 'the arrow of the history was not consumed');
          const field = __t.q('#settings-grid-opacity');
          if (field !== null) {
            field.focus();
            const guarded = { key: __t.key('ArrowLeft', field), views: __t.emitted('graphium://clone-view').length };
            __t.expect(guarded.key === false && guarded.views === views.length, 'the field lost its arrow: ' + JSON.stringify(guarded));
          }
          return { step, guarded: true };
        })()`,
      },
      {
        id: ["C33", "C31"],
        name: "a clone paint of the photo ends browsing; an older one is a preview",
        js: `(async () => {
          __t.rust('graphium://clone-paint', ${JSON.stringify({
            ...cloneEntry(),
            version: 3,
            elements: sceneFixture("old"),
            bring: false,
          })});
          await __t.wait(80);
          const browsing = __t.q('.diagram.clone .panel-item-version-label').textContent.trim();
          __t.expect(browsing === '3/7', 'the older version did not land on the pill: ' + browsing);
          __t.rust('graphium://clone-paint', ${JSON.stringify({
            ...cloneEntry(),
            version: 5,
            elements: sceneFixture("photo"),
            bring: false,
          })});
          await __t.wait(80);
          const ended = __t.q('.diagram.clone .panel-item-version-label').textContent.trim();
          __t.expect(ended === '5/7', 'the photo did not end the browsing: ' + ended);
          __t.expect(__t.markerOf(__t.q('.diagram.clone .content')) === 'photo', 'the photo did not mount');
          return { browsing, ended };
        })()`,
      },
      {
        id: ["C11"],
        name: "a click on the clone without movement emits nothing",
        cdp: [
          { type: "mouse", sub: "mousePressed", point: true, button: "left", buttons: 1 },
          { type: "mouse", sub: "mouseReleased", point: true, button: "left", buttons: 0 },
          { type: "wait", ms: 80 },
        ],
        js: `(async () => {
          await __t.wait(60);
          const moves = __t.emitted('graphium://clone-move');
          __t.expect(moves.length === 0, 'a click wrote ' + moves.length + ' moves');
          return { moves: moves.length };
        })()`,
      },
      {
        id: ["C11"],
        name: "a drag of 40 screen px at zoom 1 emits one clone-move of 40 plan units",
        cdp: [
          { type: "mouse", sub: "mousePressed", point: true, button: "left", buttons: 1 },
          { type: "mouse", sub: "mouseMoved", point: true, dx: 40, dy: 20, button: "left", buttons: 1 },
          { type: "mouse", sub: "mouseReleased", point: true, dx: 40, dy: 20, button: "left", buttons: 0 },
          { type: "wait", ms: 150 },
        ],
        js: `(async () => {
          await __t.wait(80);
          const moves = __t.emitted('graphium://clone-move');
          __t.expect(moves.length === 1, 'moves: ' + moves.length);
          const zoom = __t.cam().zoom;
          __t.expect(moves[0].clone_id === 'c-1', 'clone id ' + moves[0].clone_id);
          __t.expect(__t.near(moves[0].pos.x - 600, 40 / zoom, 0.01) && __t.near(moves[0].pos.y - 400, 20 / zoom, 0.01),
            'the delta is not screen px / zoom: ' + JSON.stringify({ move: moves[0], zoom }));
          const content = __t.q('.diagram.clone .content');
          __t.expect(__t.near(parseFloat(content.style.left), 600 + 40 / zoom, 0.5), 'the object did not move on the plan');
          return { moves, zoom };
        })()`,
      },
      {
        id: ["C31"],
        name: "the overhang rule (documented difference on wide clones)",
        compare: "report",
        js: `{
          const pill = __t.q('.diagram.clone .diagram-clone-tools');
          return { overhang: pill.classList.contains('overhang'), width: pill.offsetWidth, objectWidth: pill.parentElement.clientWidth };
        }`,
      },
    ],
  },
  {
    name: "chrome",
    bridge: "global",
    setup: `(async () => {
      __t.rust('graphium://settings', ${JSON.stringify(SETTINGS)});
      __t.rust('graphium://state', ${JSON.stringify({ diagrams: [], sessions: [{ id: "s1", family: "pi", label: "graphium-1", state: "active" }], clones: [] })});
      await __t.settle();
    })()`,
    steps: [
      {
        id: ["C35"],
        name: "the banner shows path, message, hint, +N and closes",
        js: `(async () => {
          __t.rust('graphium://error', { path: "plan.md", code: "E_PARSE", message: "no se pudo leer", hint: "revisá el archivo", extra: 2 });
          await __t.settle();
          __t.expect(__t.shown('#banner') === true, 'the banner did not show');
          const parts = {
            path: __t.text('#banner-path'), message: __t.text('#banner-message'),
            hint: __t.text('#banner-hint'), extra: __t.text('#banner-extra'),
          };
          __t.expect(parts.path === 'plan.md' && parts.message === 'no se pudo leer', 'banner body ' + JSON.stringify(parts));
          __t.expect(parts.hint === 'revisá el archivo', 'banner hint ' + parts.hint);
          __t.expect(parts.extra === '+2 errores', 'banner extra ' + parts.extra);
          __t.click('#banner-close');
          await __t.settle();
          __t.expect(__t.shown('#banner') === false, 'the close button did not hide the banner');
          return parts;
        })()`,
      },
      {
        id: ["C35", "C36"],
        name: "an error hides the ghost",
        js: `(async () => {
          __t.rust('graphium://pending', { nodes: 4, edges: 3 });
          await __t.settle();
          __t.expect(__t.shown('#ghost') === true, 'the ghost did not show');
          __t.rust('graphium://error', { path: 'x', message: 'boom' });
          await __t.settle();
          __t.expect(__t.shown('#ghost') === false, 'the error did not hide the ghost');
          return { ghostAfterError: __t.shown('#ghost') };
        })()`,
      },
      {
        id: ["C36"],
        name: "the ghost says calculando layout… then the counts after 2 s",
        js: `(async () => {
          __t.rust('graphium://pending', { nodes: 4, edges: 3 });
          await __t.settle();
          const first = __t.text('#ghost-text');
          __t.expect(first === 'calculando layout…', 'first text: ' + first);
          await __t.wait(2200);
          const later = __t.text('#ghost-text');
          __t.expect(later === '4 nodos, 3 edges', 'counts: ' + later);
          return { first, later };
        })()`,
      },
      {
        id: ["C36"],
        name: "a render hides the ghost",
        js: `(async () => {
          __t.rust('graphium://pending', { nodes: 1, edges: 0 });
          await __t.settle();
          __t.rust('graphium://render', ${JSON.stringify(RENDER)});
          await __t.wait(120);
          __t.expect(__t.shown('#ghost') === false, 'the render did not hide the ghost');
          return { ghost: __t.shown('#ghost') };
        })()`,
      },
    ],
  },
  {
    name: "settings",
    bridge: "global",
    setup: null,
    steps: [
      {
        id: ["C6"],
        name: "nothing is sent before the first settings",
        js: `(async () => {
          __t.click('#settings-toggle');
          await __t.settle();
          __t.input('settings-grid-opacity', 0.5);
          await __t.wait(180);
          __t.expect(__t.count('settings-changed') === 0, 'the page wrote before Rust spoke');
          return { sent: __t.count('settings-changed') };
        })()`,
      },
      {
        id: ["C6", "C37"],
        name: "the panel has the 12 controls, the values and the 7 theme chips",
        js: `(async () => {
          __t.rust('graphium://settings', ${JSON.stringify(SETTINGS)});
          await __t.settle();
          const ids = ['settings-background', 'settings-grid-color', 'settings-grid-opacity',
            'settings-grid-diagonal', 'settings-grid-angle', 'settings-grid-visible',
            'settings-node-fill', 'settings-node-stroke', 'settings-node-text',
            'settings-edge-stroke', 'settings-edge-text', 'settings-accent'];
          const missing = ids.filter((id) => document.getElementById(id) === null);
          __t.expect(missing.length === 0, 'missing controls: ' + missing.join(','));
          const radios = __t.all('input[name="settings-theme"]');
          __t.expect(radios.length === 7, 'theme radios: ' + radios.length);
          const swatches = __t.all('.settings-radio .swatch');
          __t.expect(swatches.length === 28, 'swatches: ' + swatches.length);
          const values = {
            opacity: __t.text('#settings-grid-opacity-value'),
            diagonal: __t.text('#settings-grid-diagonal-value'),
            angle: __t.text('#settings-grid-angle-value'),
          };
          __t.expect(values.opacity === '1.00', 'opacity value: ' + values.opacity);
          __t.expect(values.diagonal === '59px', 'diagonal value: ' + values.diagonal);
          __t.expect(values.angle === '45°', 'angle value: ' + values.angle);
          return { ids: ids.length, radios: radios.length, swatches: swatches.length, values };
        })()`,
      },
      {
        id: ["C6", "C38"],
        name: "a burst of two changes is one settings-changed, a repeat is none",
        js: `(async () => {
          __t.input('settings-grid-opacity', 0.6);
          __t.input('settings-grid-opacity', 0.7);
          await __t.wait(220);
          const events = __t.emitted('settings-changed');
          __t.expect(events.length === 1, 'a burst produced ' + events.length + ' writes');
          const payload = events[0];
          const keys = Object.keys(payload).sort();
          __t.expect(keys.length === 13, 'payload keys: ' + keys.join(','));
          __t.expect(payload.grid_opacity === 0.7, 'the payload is not the last value: ' + payload.grid_opacity);
          __t.input('settings-grid-opacity', 0.7);
          await __t.wait(220);
          __t.expect(__t.emitted('settings-changed').length === 1, 'a repeated value wrote again');
          const painted = document.getElementById('plan').style.getPropertyValue('--tx');
          return { events: 1, keys, painted };
        })()`,
      },
      {
        id: ["C37"],
        name: "Escape closes the panel and gives the focus back to the gear",
        js: `(async () => {
          __t.expect(__t.shown('#settings-panel') === true, 'the panel is not open');
          const focusInside = document.activeElement !== null && __t.q('#settings-panel').contains(document.activeElement);
          __t.expect(focusInside === true, 'the focus did not move into the panel when it opened');
          const event = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true });
          document.activeElement.dispatchEvent(event);
          await __t.settle();
          __t.expect(__t.shown('#settings-panel') === false, 'Escape did not close the panel');
          __t.expect(document.activeElement === __t.q('#settings-toggle'), 'the focus did not return to the gear');
          return { closed: true, focus: document.activeElement.id };
        })()`,
      },
    ],
  },
  {
    name: "bridge-fallback",
    bridge: "internals",
    steps: [
      {
        id: ["C1"],
        name: "the internal IPC door speaks plugin:event|emit and |listen",
        js: `{
          const emitted = __h.invokes.filter((entry) => entry.command === 'plugin:event|emit');
          const listens = __h.invokes.filter((entry) => entry.command === 'plugin:event|listen');
          __t.expect(emitted.length >= 1, 'emits through the internals: ' + emitted.length);
          const ready = emitted.find((entry) => entry.args.event === 'graphium://ready');
          __t.expect(ready !== undefined, 'ready did not travel through the internals');
          __t.expect(JSON.stringify(ready.args.payload) === '{}', 'payload ' + JSON.stringify(ready.args.payload));
          __t.expect(listens.length >= 7, 'listeners through the internals: ' + listens.length);
          return { door: __h.door, readyThroughInternals: true, listensAtLeastSeven: true };
        }`,
      },
    ],
  },
  {
    name: "bridge-missing",
    bridge: "absent",
    steps: [
      {
        id: ["C1"],
        name: "no bridge at all: the same console error, nothing emitted",
        js: `(async () => {
          await __t.wait(120);
          return { emitted: __h.emitted.length, errors: __h.errors };
        })()`,
        console: true,
      },
    ],
  },
];

// --- runner ----------------------------------------------------------------

async function runStep(browser, page, step) {
  for (const action of step.cdp || []) {
    // A `point` action is resolved in the page (`window.__point`), so a probe
    // can press the exact middle of an object it measured in a previous step.
    let x = action.x;
    let y = action.y;
    if (action.point) {
      const point = await evaluate(browser.cdp, page.session.sessionId, "window.__point", false);
      x = point.x + (action.dx || 0);
      y = point.y + (action.dy || 0);
    }
    if (action.type === "wheel") {
      await browser.cdp.send(
        "Input.dispatchMouseEvent",
        { type: "mouseWheel", x, y, deltaX: 0, deltaY: action.deltaY, pointerType: "mouse" },
        page.session.sessionId,
      );
    } else if (action.type === "mouse") {
      await browser.cdp.send(
        "Input.dispatchMouseEvent",
        {
          type: action.sub,
          x,
          y,
          button: action.button || "none",
          buttons: action.buttons ?? 0,
          clickCount: action.sub === "mousePressed" || action.sub === "mouseReleased" ? 1 : 0,
          pointerType: "mouse",
        },
        page.session.sessionId,
      );
    } else if (action.type === "metrics") {
      await browser.cdp.send(
        "Emulation.setDeviceMetricsOverride",
        { width: action.width, height: action.height, deviceScaleFactor: 1, mobile: false },
        page.session.sessionId,
      );
    } else if (action.type === "wait") {
      await new Promise((resolve) => setTimeout(resolve, action.ms));
    }
  }
  const body = String(step.js).trim();
  // A probe is either an expression (usually an async IIFE) or a block of
  // statements with its own `return`; both are wrapped so `await` works.
  const expression = body.startsWith("{")
    ? `(async () => { ${body} })()`
    : `(async () => (${body}))()`;
  const value = await evaluate(browser.cdp, page.session.sessionId, expression);
  if (step.console) {
    return { value, console: page.session.console.slice(), exceptions: page.session.exceptions.slice() };
  }
  return { value, console: null, exceptions: page.session.exceptions.slice() };
}

async function runBuild(browser, name, root, port) {
  const server = await serve(root);
  const results = [];
  for (const scenario of scenarios) {
    let page;
    try {
      page = await openPage(browser, server.url, scenario.bridge);
      if (scenario.setup) {
        const setup = String(scenario.setup).trim();
        const wrapped = setup.startsWith("{")
          ? `(async () => { ${setup} })()`
          : `(async () => (${setup}))()`;
        await evaluate(browser.cdp, page.session.sessionId, wrapped, true);
        await new Promise((resolve) => setTimeout(resolve, 120));
      }
      for (const step of scenario.steps) {
        if (step.console) {
          await evaluate(
            browser.cdp,
            page.session.sessionId,
            `(async () => {
              const errors = [];
              const original = console.error;
              console.error = (...args) => { errors.push(args.map(String).join(' ')); original(...args); };
              window.__h.errors = errors;
              return true;
            })()`,
          );
        }
        let outcome;
        try {
          outcome = await runStep(browser, page, step);
          if (step.quiet) continue;
          const consoleErrors = (outcome.console || page.session.console)
            .filter((entry) => entry.type === "error")
            .map((entry) => entry.text);
          if (step.console) {
            // The console error is the observable: capture it from the page
            // (the bundle may install its own console shim).
            const errors = await evaluate(
              browser.cdp,
              page.session.sessionId,
              `window.__h.errors`,
              false,
            );
            outcome.value = { emitted: outcome.value.emitted, errors };
            outcome.value.errors = errors;
          }
          results.push({
            scenario: scenario.name,
            build: name,
            id: step.id,
            name: step.name,
            compare: step.compare || "equal",
            ok: true,
            value: outcome.value,
            consoleErrors,
            exceptions: outcome.exceptions,
          });
        } catch (error) {
          if (step.quiet) throw error;
          let debug = null;
          try {
            debug = await evaluate(
              browser.cdp,
              page.session.sessionId,
              `(() => ({
                cam: window.__t ? __t.cam() : null,
                objects: window.__t ? __t.all('.diagram').map((element) => ({
                  id: element.dataset.id,
                  className: element.className,
                  rect: [Math.round(element.getBoundingClientRect().x), Math.round(element.getBoundingClientRect().y), Math.round(element.getBoundingClientRect().width), Math.round(element.getBoundingClientRect().height)],
                  content: element.querySelector('.content') ? [element.querySelector('.content').style.left, element.querySelector('.content').style.top, element.querySelector('.content').style.width, element.querySelector('.content').style.height] : null,
                })) : [],
                markers: window.__t ? __t.all('.content').map((node) => __t.markerOf(node)) : [],
                pill: window.__t && __t.q('.diagram-clone-tools') ? __t.q('.diagram-clone-tools').textContent.replace(/\\s+/g, ' ').trim() : null,
                emitted: window.__h ? __h.emitted.map((entry) => entry.name) : [],
                at700: document.elementFromPoint(700, 500) ? document.elementFromPoint(700, 500).className : null,
                at640: document.elementFromPoint(640, 420) ? document.elementFromPoint(640, 420).className : null,
              }))()`,
            );
          } catch {
            debug = null;
          }
          results.push({
            scenario: scenario.name,
            build: name,
            id: step.id,
            name: step.name,
            compare: step.compare || "equal",
            ok: false,
            error: error.message,
            debug,
            consoleErrors: (page.session.console || []).filter((entry) => entry.type === "error"),
            exceptions: page.session.exceptions.slice(),
          });
        }
      }
    } finally {
      if (page) await page.close();
    }
  }
  await server.close();
  return results;
}

function pairResults(all) {
  const key = (entry) => `${entry.scenario}::${entry.name}`;
  const byKey = new Map();
  for (const entry of all) {
    if (!byKey.has(key(entry))) byKey.set(key(entry), {});
    byKey.get(key(entry))[entry.build] = entry;
  }
  const checks = [];
  for (const [name, pair] of byKey) {
    const builds = Object.keys(pair);
    const first = pair[builds[0]];
    const equal =
      builds.length < 2
        ? null
        : JSON.stringify(pair[builds[0]].value) === JSON.stringify(pair[builds[1]].value);
    const ok =
      first.ok &&
      builds.every((build) => pair[build].ok) &&
      (first.compare !== "equal" || equal !== false);
    checks.push({
      scenario: first.scenario,
      name,
      id: first.id,
      compare: first.compare,
      ok,
      equal,
      values: Object.fromEntries(builds.map((build) => [build, pair[build].value ?? { error: pair[build].error }])),
      failures: builds
        .filter((build) => !pair[build].ok)
        .map((build) => `${build}: ${pair[build].error}${pair[build].debug ? ` | ${JSON.stringify(pair[build].debug)}` : ""}`),
    });
  }
  return checks;
}

// --- static checks (Node side: files, not the page) ------------------------

async function staticChecks() {
  const results = [];
  const push = (id, name, ok, value, error) =>
    results.push({ scenario: "static", build: "both", id, name, compare: "report", ok, value, error });

  // C40: every number of CONTRACT.md §6 lives in the Svelte source.
  const contractJs = await readFile(path.join(APP, "src/lib/contract.js"), "utf8");
  const numbers = {
    diagonal: /GRID_DIAGONAL_PLAN\s*=\s*59/.test(contractJs),
    angle: /GRID_ANGLE_DEGREES\s*=\s*45/.test(contractJs),
    hideZoom: /GRID_HIDE_ZOOM\s*=\s*0\.3/.test(contractJs),
    zoomRange: /ZOOM_MIN\s*=\s*0\.05/.test(contractJs) && /ZOOM_MAX\s*=\s*4/.test(contractJs),
    zoomStep: /ZOOM_STEP\s*=\s*1\.1/.test(contractJs),
    cameraThrottle: /CAMERA_THROTTLE_MS\s*=\s*100/.test(contractJs),
    settingsThrottle: /SETTINGS_THROTTLE_MS\s*=\s*100/.test(contractJs),
    pan: /PAN_MS\s*=\s*250/.test(contractJs),
    padding: /OBJECT_PADDING\s*=\s*24/.test(contractJs),
    culling: /MAX_LIVE_SVGS\s*=\s*3/.test(contractJs) && /CULL_ZOOM\s*=\s*0\.4/.test(contractJs),
  };
  const perp = (59 * Math.sin((45 * Math.PI) / 180)).toFixed(2);
  push(
    ["C40"],
    "every §6 number once, with the contract value, in src/lib/contract.js",
    Object.values(numbers).every(Boolean),
    { ...numbers, perpendicularStep: perp },
  );

  // C42 (the stylesheet carrying every rule of the vanilla style.css) retired
  // with the vanilla page: there is no second stylesheet to compare against.
  // CONTRACT.md §10 keeps the coverage map of the retired checks.
  const svelteCss = await readFile(path.join(APP, "src/app.css"), "utf8");

  // C9: the stylesheet declares the grid tokens the tests pin, and the tile.
  const declarations = {
    angle: /--grid-angle:\s*45deg/.test(svelteCss),
    diagonal: /--grid-diagonal:\s*59px/.test(svelteCss),
    step: /--grid-step:\s*41\.72px/.test(svelteCss),
    tileWidth: /calc\(var\(--grid-diagonal\)\s*\*\s*var\(--grid-tan\)\s*\*\s*var\(--zoom\)\)/.test(svelteCss),
    tileHeight: /calc\(var\(--grid-diagonal\)\s*\*\s*var\(--zoom\)\)/.test(svelteCss),
    phaseA: /--shift-a/.test(svelteCss),
    phaseB: /--shift-b/.test(svelteCss),
  };
  push(["C9"], "the grid token declarations and the tile formula", Object.values(declarations).every(Boolean), declarations);

  // C43: no inline style attribute in the markup of the app's source, and none
  // in the HTML the browser is served.
  const svelteFiles = (await readdir(path.join(APP, "src"), { recursive: true })).filter((file) =>
    file.endsWith(".svelte"),
  );
  const offenders = [];
  for (const file of svelteFiles) {
    const text = await readFile(path.join(APP, "src", file), "utf8");
    const markup = text.split("</script>")[1] ?? "";
    if (/\sstyle\s*=\s*"/.test(markup)) offenders.push(file);
    if (/<style/.test(text)) offenders.push(`${file} (style block)`);
  }
  const svelteHtml = await readFile(path.join(APP, "index.html"), "utf8");
  const htmlWithInlineStyle = /\sstyle\s*=/.test(svelteHtml);
  push(
    ["C43"],
    "no inline style attribute in the markup or in the served HTML",
    offenders.length === 0 && !htmlWithInlineStyle,
    { svelteFiles: svelteFiles.length, offenders, htmlWithInlineStyle },
  );

  // C41: the built strings the list of §8 C41 names are in the source,
  // verbatim (they were the vanilla ones; the port keeps them).
  const labels = await readFile(path.join(APP, "src/lib/labels.js"), "utf8");
  const appSource = await readFile(path.join(APP, "src/App.svelte"), "utf8");
  const components = (
    await Promise.all(
      (await readdir(path.join(APP, "src/lib")))
        .filter((file) => file.endsWith(".svelte") || file.endsWith(".js"))
        .map((file) => readFile(path.join(APP, "src/lib", file), "utf8")),
    )
  ).join("\n");
  const svelteSources = `${labels}\n${components}\n${appSource}`;
  const strings = [
    "esperando al agente…",
    "sin diagramas abiertos",
    "en gracia",
    "actualizar el clon",
    "el original ya no está en el plan",
    "tu plano está vacío",
    "calculando layout…",
  ];
  const missing = strings.filter((text) => !svelteSources.includes(text));
  push(
    ["C41"],
    "the built strings of the plan are in the source verbatim",
    missing.length === 0,
    { strings: strings.length, missing },
    missing.length === 0 ? null : `missing: ${missing.join(" | ")}`,
  );

  return results;
}

// --- main ------------------------------------------------------------------

async function main() {
  if (!existsSync(path.join(SVELTE_DIR, "index.html"))) {
    console.error("parity: no bundle at dist/ — run scripts/build.sh (or `npm run build`) first");
    process.exit(2);
  }
  const builds = [["svelte", SVELTE_DIR]];

  const browser = await launch(CHROMIUM);
  const all = [];
  try {
    for (const [name, root] of builds) {
      const results = await runBuild(browser, name, root, 0);
      all.push(...results);
    }
  } finally {
    await browser.close();
  }
  all.push(...(await staticChecks()));

  const checks = pairResults(all);
  const failures = checks.filter((check) => !check.ok);

  const report = {
    chromium: CHROMIUM,
    builds: builds.map(([name]) => name),
    checks: checks.map((check) => ({
      id: check.id,
      name: check.name,
      scenario: check.scenario,
      ok: check.ok,
      equal: check.equal,
      values: check.values,
      failures: check.failures,
    })),
    failures: failures.length,
  };

  for (const check of checks) {
    const status = check.ok ? (check.equal === false ? "diff" : "ok  ") : "FAIL";
    const ids = check.id.join(",");
    console.log(`${status} ${ids.padEnd(10)} ${check.name}`);
    if (!check.ok) {
      for (const failure of check.failures) console.log(`         ${failure}`);
      if (check.equal === false) console.log(`         values differ: ${JSON.stringify(check.values)}`);
    }
  }
  console.log(`\n${checks.length - failures.length}/${checks.length} probes passed`);
  if (JSON_OUT !== null) {
    const { writeFile } = await import("node:fs/promises");
    await writeFile(JSON_OUT, `${JSON.stringify(report, null, 2)}\n`);
    console.log(`parity: report written to ${JSON_OUT}`);
  }
  if (failures.length > 0) process.exit(1);
}

await main();
