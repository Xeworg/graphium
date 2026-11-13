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
  bookLine,
  cwdOf,
  exportsRoot,
  findBinary,
  formatBytes,
  identityEnv,
  recentExports,
  sessionIdOf,
  statusLines,
} from "./index.ts";

/** A stub of the parts of `ExtensionAPI` this extension touches. */
function stubPi() {
  const registered = new Map();
  const commands = new Map();
  const handlers = new Map();
  /** Every registration call, in order, so a re-registration is visible. */
  const calls = [];
  return {
    registered,
    commands,
    handlers,
    calls,
    registerMcpServer(name, config) {
      calls.push(`register:${name}`);
      registered.set(name, config);
    },
    unregisterMcpServer(name) {
      calls.push(`unregister:${name}`);
      registered.delete(name);
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

/**
 * The budget the ODD task gives the server description (f28 T6a): the one
 * sentence tool search ranks and codemode returns, so it does not grow.
 */
const DESCRIPTION_BUDGET = 169;

/**
 * Every check, in declaration order, with its outcome. A failing check does not
 * stop the ones after it: one run reports all of them, which is what a RED is
 * worth reading.
 */
const checks = [];
const pending = [];
function check(name, body) {
  const outcome = { name, error: undefined };
  checks.push(outcome);
  const failed = (error) => {
    outcome.error = error ?? new Error("check failed");
  };
  try {
    const result = body();
    if (result instanceof Promise) {
      pending.push(result.then(() => {}, failed));
      return;
    }
  } catch (error) {
    failed(error);
  }
}

/** A stub of an `ExtensionContext`: the extension reads `cwd` and the id, no more. */
function stubContext(cwd, sessionId) {
  return { cwd, sessionManager: { getSessionId: () => sessionId } };
}

/**
 * A `before_agent_start` event with the shape Pi hands the handler: the
 * normalized options of the run, which a handler may fill in.
 */
function promptEvent(selectedTools) {
  return {
    type: "before_agent_start",
    messageType: "user",
    prompt: "¿quién usa parse_side?",
    systemPromptOptions: {
      selectedTools,
      toolGuidelines: {},
      promptGuidelines: [],
    },
  };
}

/**
 * The prefix Pi declares an MCP tool with: `mcp__<server>__<tool>` (docs/mcp.md,
 * "Configuration rules"). Spelled here instead of imported, so the check holds
 * the extension to the documented name and not to its own helper.
 */
const TOOL_PREFIX = "mcp__graphium__";

/**
 * Runs `body` with a known `PI_SESSION_ID` in the process, then puts the real
 * one back: the factory takes the id from the process when it has no context.
 */
async function withSessionId(value, body) {
  const previous = process.env.PI_SESSION_ID;
  if (value === undefined) delete process.env.PI_SESSION_ID;
  else process.env.PI_SESSION_ID = value;
  try {
    return await body();
  } finally {
    if (previous === undefined) delete process.env.PI_SESSION_ID;
    else process.env.PI_SESSION_ID = previous;
  }
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

check("the book is declared direct, so its tools reach the model without a search", () => {
  const pi = stubPi();
  graphium(pi, process.execPath);
  assert.equal(
    pi.registered.get("graphium").exposure,
    "direct",
    "without `exposure` the server falls back to codemode and the model never sees its tools",
  );
});

check("the description names the five book tools and promises nothing it cannot keep", () => {
  const pi = stubPi();
  graphium(pi, process.execPath);
  const description = pi.registered.get("graphium").description;

  for (const tool of ["code_search", "code_structure", "code_impact", "code_diagram", "code_er"]) {
    assert.ok(description.includes(`\`${tool}\``), `the description names ${tool}: ${description}`);
  }
  // A PNG needs `path` and the rasterizer the app provides: naming it without
  // naming `path` is the false claim the old text carried.
  assert.ok(
    !/PNG/i.test(description) || /path/.test(description),
    `a PNG is not promised without \`path\`: ${description}`,
  );
  // The book is not read unconditionally: without a project it answers
  // `invalid_value`, and `GRAPHIUM_BOOK=off` makes it `book_disabled`.
  assert.match(description, /requiere proyecto|sin proyecto|con proyecto/, description);
  assert.ok(
    description.length <= DESCRIPTION_BUDGET,
    `the description stays inside its budget (${description.length} > ${DESCRIPTION_BUDGET})`,
  );
});

check("the status names the binary, the MCP server and the exports root", () => {
  const pi = stubPi();
  pi.registerMcpServer("graphium", { command: process.execPath, args: ["--stdio"] });
  const lines = statusLines(pi, process.execPath).join("\n");
  assert.match(lines, /binario: .*node|binario: .*\/usr\//);
  assert.match(lines, /MCP: registrado/);
  assert.match(lines, /libro: activo: el agente lee el índice de /);
  assert.match(lines, /raíz de exportes: /);
  assert.match(lines, /\/graphium open/);
});

check("the book line says whether the book is on and which project it reads", () => {
  const identity = identityEnv({ cwd: "/tmp/graphium-proyecto" }, {});
  assert.equal(
    bookLine(identity, {}),
    "libro: activo: el agente lee el índice de /tmp/graphium-proyecto",
  );
  assert.match(bookLine(identity, { GRAPHIUM_BOOK: "off" }), /^libro: apagado \(GRAPHIUM_BOOK=off\)/);
  assert.match(bookLine(identity, { GRAPHIUM_BOOK: "on" }), /^libro: activo/);
  assert.match(bookLine({}, {}), /no declara GRAPHIUM_PROJECT: el libro responde invalid_value/);
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

check("the identity env always declares Pi and the project", () => {
  const env = identityEnv({ cwd: "/tmp/graphium-proyecto" }, {});
  assert.equal(env.GRAPHIUM_CLIENT, "pi");
  assert.equal(env.GRAPHIUM_PROJECT, "/tmp/graphium-proyecto");
  // Without a directory from the event the project is the one Pi runs in.
  assert.equal(identityEnv({}, {}).GRAPHIUM_PROJECT, process.cwd());
  assert.equal(identityEnv({ cwd: "   " }, {}).GRAPHIUM_PROJECT, process.cwd());
});

check("the identity env takes the session from the context first, then from PI_SESSION_ID", () => {
  const fromContext = identityEnv(
    { sessionId: sessionIdOf(stubContext("/tmp/uno", "9f1c2ab4")), cwd: "/tmp/uno" },
    { PI_SESSION_ID: "la-del-proceso" },
  );
  assert.equal(fromContext.GRAPHIUM_SESSION, "9f1c2ab4", "el contexto gana sobre el proceso");
  assert.equal(fromContext.GRAPHIUM_PROJECT, "/tmp/uno");
  const fromProcess = identityEnv({}, { PI_SESSION_ID: "b7d0" });
  assert.equal(fromProcess.GRAPHIUM_SESSION, "b7d0", "sin contexto vale el proceso");
});

check("the identity env leaves the session out instead of inventing it", () => {
  const env = identityEnv(
    { sessionId: sessionIdOf(stubContext("/tmp/uno", undefined)), cwd: "/tmp/uno" },
    {},
  );
  assert.equal("GRAPHIUM_SESSION" in env, false, "sin id no hay clave");
  assert.deepEqual(Object.keys(env).sort(), ["GRAPHIUM_CLIENT", "GRAPHIUM_PROJECT"]);
  assert.equal(
    "GRAPHIUM_SESSION" in identityEnv({ sessionId: "   " }, {}),
    false,
    "un id en blanco no es un id",
  );
});

check("reading the identity from a missing or stale context answers null instead of throwing", () => {
  assert.equal(sessionIdOf(undefined), null);
  assert.equal(sessionIdOf(null), null);
  assert.equal(sessionIdOf({ cwd: "/tmp/uno" }), null);
  assert.equal(cwdOf(undefined), null);
  assert.equal(cwdOf({ cwd: "" }), null);
  const stale = {
    get cwd() {
      throw new Error("ctx viejo después de newSession()");
    },
    get sessionManager() {
      throw new Error("ctx viejo después de newSession()");
    },
  };
  assert.equal(sessionIdOf(stale), null);
  assert.equal(cwdOf(stale), null);
});

check("the registration passes the identity env to the MCP server", async () => {
  await withSessionId(undefined, async () => {
    const pi = stubPi();
    graphium(pi, process.execPath);
    const server = pi.registered.get("graphium");
    assert.ok(server.env, "el servidor se registra con env");
    assert.equal(server.env.GRAPHIUM_CLIENT, "pi");
    assert.equal(server.env.GRAPHIUM_PROJECT, process.cwd());
    assert.equal(
      "GRAPHIUM_SESSION" in server.env,
      false,
      "sin sesión todavía, no se inventa un id",
    );
  });
});

check("a new session re-registers with its own id; the same session is left alone", async () => {
  await withSessionId(undefined, async () => {
    const notices = [];
    const pi = stubPi();
    graphium(pi, process.execPath);
    assert.deepEqual(pi.calls, ["register:graphium"], "la fábrica registra una vez");

    const handler = pi.handlers.get("session_start");
    const start = (sessionId, cwd) =>
      handler(
        { type: "session_start", reason: "startup" },
        { ...stubContext(cwd, sessionId), ui: { notify: (text) => notices.push(text) } },
      );

    await start("sesion-uno", "/tmp/proyecto-uno");
    assert.deepEqual(pi.calls, [
      "register:graphium",
      "unregister:graphium",
      "register:graphium",
    ]);
    assert.deepEqual(pi.registered.get("graphium").env, {
      GRAPHIUM_CLIENT: "pi",
      GRAPHIUM_PROJECT: "/tmp/proyecto-uno",
      GRAPHIUM_SESSION: "sesion-uno",
    });

    // The same session starting again (a reload) does not touch the connection.
    await start("sesion-uno", "/tmp/proyecto-uno");
    assert.equal(pi.calls.length, 3, "el mismo id no se vuelve a registrar");

    // A new session in the same Pi process gets its own name.
    await start("sesion-dos", "/tmp/proyecto-dos");
    assert.deepEqual(pi.calls.slice(3), ["unregister:graphium", "register:graphium"]);
    assert.deepEqual(pi.registered.get("graphium").env, {
      GRAPHIUM_CLIENT: "pi",
      GRAPHIUM_PROJECT: "/tmp/proyecto-dos",
      GRAPHIUM_SESSION: "sesion-dos",
    });
    assert.deepEqual(notices, [], "nadie molesta al humano cuando sale bien");
  });
});

check("a re-registration that fails tells the human and does not throw out of the handler", async () => {
  await withSessionId(undefined, async () => {
    const pi = stubPi();
    graphium(pi, process.execPath);
    const handler = pi.handlers.get("session_start");
    pi.registerMcpServer = () => {
      throw new Error("otra extensión es dueña del transporte MCP");
    };
    const notices = [];
    await handler(
      {},
      { ...stubContext("/tmp/uno", "sesion-uno"), ui: { notify: (text) => notices.push(text) } },
    );
    assert.match(notices.join("\n"), /no se pudo registrar/);
    assert.equal(pi.registered.size, 0, "no queda un servidor a medias");
  });
});

check("the guidelines are keyed by tool and each one names its tool", async () => {
  const pi = stubPi();
  graphium(pi, process.execPath);
  const handler = pi.handlers.get("before_agent_start");
  assert.equal(typeof handler, "function", "before_agent_start is wired");

  const event = promptEvent(["read", `${TOOL_PREFIX}code_impact`]);
  await handler(event, stubContext("/tmp/graphium-proyecto", "sesion-uno"));
  const written = event.systemPromptOptions.toolGuidelines;

  assert.ok(written, "the guidelines land in the run's toolGuidelines");
  for (const tool of ["code_search", "code_structure", "code_impact", "code_diagram", "code_er"]) {
    assert.ok(
      Object.prototype.hasOwnProperty.call(written, `${TOOL_PREFIX}${tool}`),
      `there is a guideline for ${tool}: ${Object.keys(written).join(" ")}`,
    );
  }
  for (const key of Object.keys(written)) {
    assert.ok(key.startsWith(TOOL_PREFIX), `the guideline is keyed by the declared name: ${key}`);
    assert.ok(written[key].length > 0, `${key} carries at least one bullet`);
    // Each bullet is read next to the tool it belongs to, so it says which
    // tool it is talking about instead of leaving the reader to guess.
    const tool = key.slice(TOOL_PREFIX.length);
    assert.ok(
      written[key].join(" ").includes(tool),
      `the guideline of ${key} names ${tool}: ${written[key].join(" | ")}`,
    );
  }
});

check("the hooks are wired and a guideline that fails cannot break the session", async () => {
  const pi = stubPi();
  graphium(pi, process.execPath);
  for (const event of ["session_start", "before_agent_start"]) {
    assert.equal(typeof pi.handlers.get(event), "function", `${event} is wired`);
  }
  const handler = pi.handlers.get("before_agent_start");
  const ctx = stubContext("/tmp/graphium-proyecto", "sesion-uno");

  // An event without options, and one whose options refuse the write: neither
  // may throw out of the handler, or the run is lost to a teaching line.
  await handler({}, ctx);
  const frozen = promptEvent(["read"]);
  Object.freeze(frozen.systemPromptOptions.toolGuidelines);
  await handler(frozen, ctx);

  // The next run starts with the options it declared, not with a half-written
  // object left behind by the failure.
  const event = promptEvent(["read", `${TOOL_PREFIX}code_impact`]);
  await handler(event, ctx);
  assert.deepEqual(
    event.systemPromptOptions.selectedTools,
    ["read", `${TOOL_PREFIX}code_impact`],
    "the run keeps the tools it declared",
  );
});

await Promise.all(pending);

const failed = checks.filter((outcome) => outcome.error !== undefined);
if (failed.length > 0) {
  for (const outcome of failed) {
    console.log(`✗ ${outcome.name}\n    ${outcome.error?.message ?? outcome.error}`);
  }
  console.log(`\n${failed.length} de ${checks.length} checks fallan`);
  process.exitCode = 1;
} else {
  console.log(`ok — ${checks.length} checks`);
  for (const outcome of checks) console.log(`  · ${outcome.name}`);
}
