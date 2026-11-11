// The browser half of the T4/T5 harnesses: a static server, a very small
// Chrome DevTools Protocol client, the headless Chromium launcher and the
// script that fakes the Tauri bridge before the bundle evaluates.
//
// No dependency is added to `package.json`: Node 22 carries `node:http`,
// `node:fs` and a global `WebSocket`, which is all a CDP client needs.

import { spawn, spawnSync } from "node:child_process";
import { createServer } from "node:http";
import { existsSync } from "node:fs";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";

// --- static server ---------------------------------------------------------

const MIME = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".svg": "image/svg+xml",
  ".json": "application/json",
  ".png": "image/png",
  ".ico": "image/x-icon",
  ".woff2": "font/woff2",
};

export async function serve(root) {
  const server = createServer(async (request, response) => {
    const url = new URL(request.url, "http://127.0.0.1");
    const relative = decodeURIComponent(url.pathname === "/" ? "/index.html" : url.pathname);
    const file = path.join(root, relative);
    if (!file.startsWith(root)) {
      response.writeHead(403).end();
      return;
    }
    try {
      const body = await readFile(file);
      response.writeHead(200, {
        "content-type": MIME[path.extname(file)] || "application/octet-stream",
        "cache-control": "no-store",
      });
      response.end(body);
    } catch {
      response.writeHead(404, { "content-type": "text/plain" }).end("not found");
    }
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address();
  return { url: `http://127.0.0.1:${port}`, close: () => new Promise((r) => server.close(r)) };
}

// --- a very small CDP client over the DevTools WebSocket -------------------

export class Cdp {
  #socket;
  #nextId = 1;
  #pending = new Map();
  #listeners = new Set();

  static async connect(url) {
    const client = new Cdp();
    client.#socket = new WebSocket(url);
    await new Promise((resolve, reject) => {
      client.#socket.addEventListener("open", resolve, { once: true });
      client.#socket.addEventListener("error", reject, { once: true });
    });
    client.#socket.addEventListener("message", (message) => {
      const frame = JSON.parse(message.data);
      if (frame.id !== undefined) {
        const entry = client.#pending.get(frame.id);
        if (entry === undefined) return;
        client.#pending.delete(frame.id);
        if (frame.error) entry.reject(new Error(`${frame.error.code}: ${frame.error.message}`));
        else entry.resolve(frame.result);
        return;
      }
      for (const listener of client.#listeners) listener(frame);
    });
    return client;
  }

  send(method, params = {}, sessionId = undefined) {
    const id = this.#nextId++;
    const message = { id, method, params };
    if (sessionId !== undefined) message.sessionId = sessionId;
    this.#socket.send(JSON.stringify(message));
    return new Promise((resolve, reject) => this.#pending.set(id, { resolve, reject }));
  }

  on(listener) {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  }

  close() {
    this.#socket.close();
  }
}

// --- the browser -----------------------------------------------------------

/** The version of the browser, so a failed launch says which one it was. */
function chromiumVersion(chromium) {
  const probe = spawnSync(chromium, ["--version"], { encoding: "utf8" });
  const text = `${probe.stdout || ""}${probe.stderr || ""}`.trim();
  return text === "" ? `${chromium}: unknown version` : text;
}

export async function launch(chromium = process.env.CHROMIUM || "chromium-browser") {
  const profile = await mkdtemp(path.join(tmpdir(), "gx-parity-chromium-"));
  const child = spawn(
    chromium,
    [
      "--headless=new",
      "--no-sandbox",
      "--no-first-run",
      "--no-default-browser-check",
      "--disable-dev-shm-usage",
      "--disable-gpu",
      "--hide-scrollbars",
      // A background headless page may fall to one rAF per second; the
      // handshake is measured in frames (C4), so keep the compositor awake.
      "--disable-background-timer-throttling",
      "--disable-renderer-backgrounding",
      "--disable-backgrounding-occluded-windows",
      "--window-size=1280,800",
      `--user-data-dir=${profile}`,
      "--remote-debugging-port=0",
      "--remote-allow-origins=*",
      "about:blank",
    ],
    { stdio: ["ignore", "ignore", "pipe"] },
  );
  let stderr = "";
  child.stderr.on("data", (chunk) => {
    stderr += chunk.toString();
  });
  const portFile = path.join(profile, "DevToolsActivePort");
  // A cold CI runner takes its time to start the first browser, and a process
  // that already exited is never going to open a port: the wait watches both,
  // says which browser it was and kills what it started (f26, the CI flake).
  const timeout = Number(process.env.CHROMIUM_PORT_TIMEOUT_MS || 60000);
  const deadline = Date.now() + timeout;
  let exited = null;
  child.on("exit", (code, signal) => {
    exited = { code, signal };
  });
  child.on("error", (error) => {
    exited = { code: null, signal: null, error };
  });
  const giveUp = async (error) => {
    child.kill("SIGKILL");
    await rm(profile, { recursive: true, force: true });
    throw error;
  };
  while (!existsSync(portFile)) {
    if (exited !== null) {
      const how = exited.error
        ? `${exited.error.message}`
        : `exit ${exited.code}${exited.signal ? ` (${exited.signal})` : ""}`;
      await giveUp(
        new Error(`chromium exited before opening a devtools port: ${how}\n${chromiumVersion(chromium)}\n${stderr}`),
      );
    }
    if (Date.now() > deadline) {
      await giveUp(
        new Error(
          `chromium did not open a devtools port in ${timeout} ms\n${chromiumVersion(chromium)}\n${stderr}`,
        ),
      );
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  // The file may be written in two steps: wait until it holds both lines.
  let port = 0;
  let browserPath = "";
  for (;;) {
    const text = await readFile(portFile, "utf8").catch(() => "");
    const [first, second] = text.split("\n");
    if (first && second) {
      port = Number(first);
      browserPath = second.trim();
      break;
    }
    if (Date.now() > deadline) {
      await giveUp(new Error(`devtools port file never completed\n${chromiumVersion(chromium)}`));
    }
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  const cdp = await Cdp.connect(`ws://127.0.0.1:${port}${browserPath}`);
  return {
    cdp,
    async close() {
      try {
        await cdp.send("Browser.close");
      } catch {
        /* the socket dies with the process */
      }
      cdp.close();
      child.kill("SIGTERM");
      await new Promise((resolve) => setTimeout(resolve, 300));
      child.kill("SIGKILL");
      await rm(profile, { recursive: true, force: true });
    },
  };
}

export function helperSource(mode) {
  return `(() => {
  'use strict';
  const mode = ${JSON.stringify(mode)};
  const h = {
    emitted: [],
    listeners: new Map(),
    heard: [],
    readySeen: false,
    raf: 0,
    invokes: [],
    errors: [],
    door: null,
  };
  window.__h = h;
  window.__t = {};
  const raf0 = window.requestAnimationFrame.bind(window);
  window.requestAnimationFrame = (callback) => {
    h.raf += 1;
    return raf0(callback);
  };
  const clone = (value) => {
    if (value === undefined) return null;
    try {
      return JSON.parse(JSON.stringify(value));
    } catch {
      return String(value);
    }
  };
  const record = (name, payload, door) => {
    h.door = h.door || door;
    h.emitted.push({
      name,
      payload: clone(payload),
      raf: h.raf,
      plan: !!document.getElementById('plan'),
      t: performance.now(),
    });
    if (name === 'graphium://ready') h.readySeen = true;
  };
  const listen = (name, handler) => {
    if (!h.readySeen && !h.heard.includes(name)) h.heard.push(name);
    if (!h.listeners.has(name)) h.listeners.set(name, []);
    h.listeners.get(name).push(handler);
    return Promise.resolve(() => {});
  };
  if (mode === 'global') {
    window.__TAURI__ = {
      event: {
        emit: (name, payload) => record(name, payload, 'global'),
        listen,
        unlisten: () => Promise.resolve(),
      },
    };
  } else if (mode === 'internals') {
    window.__TAURI_INTERNALS__ = {
      invoke: (command, args) => {
        h.invokes.push({ command, args: clone(args) });
        if (command === 'plugin:event|emit') record(args.event, args.payload, 'internals');
        return Promise.resolve();
      },
      transformCallback: (callback) => {
        const id = Math.floor(Math.random() * 1e9);
        window['_' + id] = callback;
        return id;
      },
    };
  }

  const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const q = (selector) => document.querySelector(selector);
  const all = (selector) => Array.from(document.querySelectorAll(selector));
  // The id of an object is a property on the vanilla element (diagramId,
  // plan.js:1991) and a data-id attribute in the Svelte markup; ask both.
  const byId = (id) => all('.diagram').find((element) => (element.dataset.id || element.diagramId) === id) || null;
  const text = (selector) => {
    const element = q(selector);
    return element === null ? null : element.textContent.trim();
  };
  const attr = (selector, name) => {
    const element = q(selector);
    return element === null ? null : element.getAttribute(name);
  };
  const shown = (selector) => {
    const element = q(selector);
    return element === null ? null : !element.classList.contains('hidden');
  };
  // The label of the single node of a scene fixture: how a probe names one
  // mounted drawing since f11-elementos.md T3/C. The data-marker of the string
  // era died with the flip: the drawing is elements now.
  const markerOf = (element) => {
    if (element === null || element === undefined) return null;
    const svg = element.localName === 'svg' ? element : element.querySelector('svg.graphium');
    if (svg === null) return null;
    const text = svg.querySelector('text');
    return text === null ? null : text.textContent.trim();
  };
  const emitted = (name) => h.emitted.filter((entry) => entry.name === name).map((entry) => entry.payload);
  const count = (name) => h.emitted.filter((entry) => entry.name === name).length;
  const rust = (name, payload) => {
    const handlers = h.listeners.get(name) || [];
    for (const handler of handlers) handler({ event: name, payload });
    return handlers.length;
  };
  const expect = (condition, message) => {
    if (!condition) throw new Error('expect: ' + message);
  };
  const cam = () => {
    const element = document.getElementById('plan');
    const read = (name, fallback) => {
      const value = element.style.getPropertyValue(name);
      return value === '' ? fallback : parseFloat(value);
    };
    return { tx: read('--tx', 0), ty: read('--ty', 0), zoom: read('--zoom', 1) };
  };
  const near = (a, b, epsilon = 0.01) => Math.abs(a - b) <= epsilon;
  const click = (target) => {
    const element = typeof target === 'string' ? q(target) : target;
    expect(element !== null, 'click: no element ' + target);
    element.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, view: window }));
    return true;
  };
  const key = (name, target) => {
    const element = target || document.body;
    const event = new KeyboardEvent('keydown', { key: name, bubbles: true, cancelable: true });
    element.dispatchEvent(event);
    return event.defaultPrevented;
  };
  const input = (id, value) => {
    const element = document.getElementById(id);
    expect(element !== null, 'input: no #' + id);
    if (element.type === 'checkbox') element.checked = Boolean(value);
    else element.value = String(value);
    element.dispatchEvent(new Event('input', { bubbles: true }));
    element.dispatchEvent(new Event('change', { bubbles: true }));
    return element.value;
  };
  const timeout = q('#settings-panel');
  Object.assign(window.__t, {
    wait, q, all, byId, text, attr, shown, markerOf, emitted, count, rust, expect, cam, near, click, key, input,
    ready: () => h.readySeen,
    settle: () => wait(40),
  });
})();`;
}


/** Evaluates an expression in a page session and returns its value. */
export async function evaluate(cdp, sessionId, expression, awaitPromise = true) {
  const result = await cdp.send(
    "Runtime.evaluate",
    { expression, awaitPromise, returnByValue: true, userGesture: true },
    sessionId,
  );
  if (result.exceptionDetails) {
    const details = result.exceptionDetails;
    const description =
      details.exception?.description || details.exception?.value || details.text;
    const error = new Error(String(description).split("\n")[0]);
    error.details = description;
    throw error;
  }
  return result.result.value;
}

/**
 * Opens a page with the faked bridge injected before any page script runs,
 * and waits for the bundle to have spoken (ready) or for the timeout.
 */
export async function openPage(browser, url, bridge, { waitForReady = true, timeoutMs = 10000 } = {}) {
  const { targetId } = await browser.cdp.send("Target.createTarget", { url: "about:blank" });
  const { sessionId } = await browser.cdp.send("Target.attachToTarget", { targetId, flatten: true });
  const session = { sessionId, console: [], exceptions: [], targetId };
  await browser.cdp.send("Page.enable", {}, sessionId);
  await browser.cdp.send("Runtime.enable", {}, sessionId);
  await browser.cdp.send(
    "Page.addScriptToEvaluateOnNewDocument",
    { source: helperSource(bridge) },
    sessionId,
  );
  const off = browser.cdp.on((frame) => {
    if (frame.sessionId !== sessionId) return;
    if (frame.method === "Runtime.consoleAPICalled") {
      const text = frame.params.args
        .map((argument) => (argument.value !== undefined ? String(argument.value) : argument.description || ""))
        .join(" ");
      session.console.push({ type: frame.params.type, text });
    }
    if (frame.method === "Runtime.exceptionThrown") {
      session.exceptions.push(
        frame.params.exceptionDetails.exception?.description ||
          frame.params.exceptionDetails.text,
      );
    }
  });
  await browser.cdp.send("Page.navigate", { url }, sessionId);
  if (waitForReady) {
    const deadline = Date.now() + timeoutMs;
    for (;;) {
      const probe = await evaluate(
        browser.cdp,
        sessionId,
        `({ ready: !!(window.__h && window.__h.readySeen), emitted: window.__h ? window.__h.emitted.length : -1 })`,
        false,
      ).catch(() => null);
      if (probe && probe.ready) break;
      if (Date.now() > deadline) break;
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
  }
  return {
    session,
    off,
    async close() {
      off();
      await browser.cdp.send("Target.closeTarget", { targetId }).catch(() => {});
    },
  };
}
