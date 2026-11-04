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
  try {
    assert.equal(findBinary(mkdtempSync(join(tmpdir(), "graphium-none-"))), null);
  } finally {
    if (previous !== undefined) process.env.GRAPHIUM_BIN = previous;
    process.env.PATH = previousPath;
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
  const previous = process.env.GRAPHIUM_BIN;
  process.env.GRAPHIUM_BIN = process.execPath;
  try {
    const pi = stubPi();
    graphium(pi);
    const server = pi.registered.get("graphium");
    assert.ok(server, "the server is registered");
    assert.equal(server.command, process.execPath);
    assert.deepEqual(server.args, ["--stdio"]);
    assert.ok(pi.commands.has("graphium"), "the command is registered");
    assert.equal(pi.commands.get("graphium").description.includes("open"), true);
  } finally {
    if (previous === undefined) delete process.env.GRAPHIUM_BIN;
    else process.env.GRAPHIUM_BIN = previous;
  }
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
  const previous = process.env.GRAPHIUM_BIN;
  const previousPath = process.env.PATH;
  delete process.env.GRAPHIUM_BIN;
  process.env.PATH = "";
  try {
    const cwd = process.cwd();
    process.chdir(mkdtempSync(join(tmpdir(), "graphium-nobin-")));
    try {
      graphium(pi);
      assert.ok(pi.commands.has("graphium"), "the command still exists");
      assert.equal(pi.registered.size, 0, "no server is registered without a binary");
      const notices = [];
      const ctx = { ui: { notify: (text) => notices.push(text) } };
      await pi.commands.get("graphium").handler("open", ctx);
      assert.match(notices.join("\n"), /no encontré el binario/);
    } finally {
      process.chdir(cwd);
    }
  } finally {
    if (previous !== undefined) process.env.GRAPHIUM_BIN = previous;
    process.env.PATH = previousPath;
  }
});

check("a rejected registration does not throw: the command reports it", async () => {
  const previous = process.env.GRAPHIUM_BIN;
  process.env.GRAPHIUM_BIN = process.execPath;
  try {
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
    graphium(pi);
    assert.ok(handlers.has("session_start"), "an error handler is installed");
    await handlers.get("session_start")({}, { ui: { notify: (text) => notices.push(text) } });
    assert.match(notices.join("\n"), /no se pudo registrar/);
  } finally {
    if (previous === undefined) delete process.env.GRAPHIUM_BIN;
    else process.env.GRAPHIUM_BIN = previous;
  }
});

const failures = checks.length;
console.log(`ok — ${failures} checks`);
for (const name of checks) console.log(`  · ${name}`);
