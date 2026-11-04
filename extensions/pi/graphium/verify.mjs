/**
 * Check of the Graphium extension without Pi: the factory is called with a
 * stub `ExtensionAPI`, and every exported helper is exercised.
 *
 * Node strips the TypeScript types on its own, and the only import of Pi is a
 * `import type`, so this runs with `node extensions/pi/graphium/verify.mjs`.
 */

import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import graphium, {
  exportsRoot,
  findBinary,
  formatBytes,
  recentExports,
  statusLines,
} from "./index.ts";

/** A stub of the parts of `ExtensionAPI` this extension touches. */
function stubPi() {
  const registered = new Map();
  const commands = new Map();
  const handlers = new Map();
  return {
    registered,
    commands,
    handlers,
    registerMcpServer(name, config) {
      registered.set(name, config);
    },
    registerCommand(name, options) {
      commands.set(name, options);
    },
    on(event, handler) {
      handlers.set(event, handler);
    },
    getMcpServers() {
      return [...registered].map(([name, config]) => ({ name, ...config }));
    },
  };
}

const checks = [];
function check(name, body) {
  body();
  checks.push(name);
}

check("findBinary honours GRAPHIUM_BIN", () => {
  const previous = process.env.GRAPHIUM_BIN;
  process.env.GRAPHIUM_BIN = process.execPath;
  try {
    assert.equal(findBinary("/nonexistent"), process.execPath);
  } finally {
    if (previous === undefined) delete process.env.GRAPHIUM_BIN;
    else process.env.GRAPHIUM_BIN = previous;
  }
});

check("findBinary returns null when there is nothing to find", () => {
  const previous = process.env.GRAPHIUM_BIN;
  const previousPath = process.env.PATH;
  delete process.env.GRAPHIUM_BIN;
  process.env.PATH = "";
  const empty = mkdtempSync(join(tmpdir(), "graphium-none-"));
  try {
    // Both the cwd and the place the extension lives, or the walk up from here
    // would find this very checkout and the test would pass for the wrong reason.
    assert.equal(findBinary(empty, empty), null);
  } finally {
    if (previous !== undefined) process.env.GRAPHIUM_BIN = previous;
    process.env.PATH = previousPath;
  }
});

check("findBinary finds the checkout from the extension itself, not the cwd", () => {
  const previous = process.env.GRAPHIUM_BIN;
  delete process.env.GRAPHIUM_BIN;
  try {
    const outside = mkdtempSync(join(tmpdir(), "graphium-otro-"));
    const found = findBinary(outside);
    assert.ok(found && found.includes("graphium"), `found: ${found}`);
    assert.ok(found.includes("target/"), `sale del checkout, no del cwd: ${found}`);
  } finally {
    if (previous !== undefined) process.env.GRAPHIUM_BIN = previous;
  }
});

check("the exports root is the one Graphium writes to", () => {
  const previous = process.env.GRAPHIUM_EXPORTS_ROOT;
  process.env.GRAPHIUM_EXPORTS_ROOT = "/tmp/graphium-exports-check";
  try {
    assert.equal(exportsRoot(), "/tmp/graphium-exports-check");
  } finally {
    if (previous === undefined) delete process.env.GRAPHIUM_EXPORTS_ROOT;
    else process.env.GRAPHIUM_EXPORTS_ROOT = previous;
  }
});

check("recentExports lists the newest files with their size, nested included", () => {
  const root = mkdtempSync(join(tmpdir(), "graphium-exports-"));
  mkdirSync(join(root, "demo"));
  writeFileSync(join(root, "demo", "a.svg"), "x".repeat(2048));
  writeFileSync(join(root, "viejo.png"), "y".repeat(10));
  const listed = recentExports(root, 2);
  assert.equal(listed.length, 2);
  assert.ok(listed.includes("demo/a.svg (2.0 KB)"), listed.join(" · "));
  assert.ok(listed.some((line) => line.startsWith("viejo.png (")), listed.join(" · "));
});

check("formatBytes reads like a human", () => {
  assert.equal(formatBytes(12), "12 B");
  assert.equal(formatBytes(2048), "2.0 KB");
  assert.equal(formatBytes(3 * 1024 * 1024), "3.0 MB");
});

check("the factory registers the MCP server and the /graphium command", () => {
  const pi = stubPi();
  graphium(pi, process.execPath);
  const server = pi.registered.get("graphium");
  assert.ok(server, "the server is registered");
  assert.equal(server.command, process.execPath);
  assert.deepEqual(server.args, ["--stdio"]);
  assert.ok(pi.commands.has("graphium"), "the command is registered");
  assert.equal(pi.commands.get("graphium").description.includes("open"), true);
});

check("the status names the binary, the MCP server and the exports root", () => {
  const pi = stubPi();
  pi.registerMcpServer("graphium", { command: process.execPath, args: ["--stdio"] });
  const lines = statusLines(pi, process.execPath).join("\n");
  assert.match(lines, /binario: .*node|binario: .*\/usr\//);
  assert.match(lines, /MCP: registrado/);
  assert.match(lines, /raíz de exportes: /);
  assert.match(lines, /\/graphium open/);
});

check("without a binary the command says what to do instead of throwing", async () => {
  const pi = stubPi();
  graphium(pi, null);
  assert.ok(pi.commands.has("graphium"), "the command still exists");
  assert.equal(pi.registered.size, 0, "no server is registered without a binary");
  assert.ok(pi.handlers.has("session_start"), "the human is told once, at start");

  const notices = [];
  const ctx = { ui: { notify: (text) => notices.push(text) } };
  await pi.commands.get("graphium").handler("open", ctx);
  assert.match(notices.join("\n"), /no encontré el binario/);

  const status = statusLines(pi, null).join("\n");
  assert.match(status, /no encontrado/);
  assert.match(status, /sin binario no hay servidor/);
});

check("a rejected registration does not throw: the command reports it", async () => {
  {
    const notices = [];
    const handlers = new Map();
    const pi = {
      registerMcpServer() {
        throw new Error("otra extensión es dueña del transporte MCP");
      },
      registerCommand() {},
      on(event, handler) {
        handlers.set(event, handler);
      },
      getMcpServers() {
        return [];
      },
    };
    graphium(pi, process.execPath);
    assert.ok(handlers.has("session_start"), "an error handler is installed");
    await handlers.get("session_start")({}, { ui: { notify: (text) => notices.push(text) } });
    assert.match(notices.join("\n"), /no se pudo registrar/);
  }
});

const failures = checks.length;
console.log(`ok — ${failures} checks`);
for (const name of checks) console.log(`  · ${name}`);
