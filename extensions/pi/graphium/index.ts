/**
 * Graphium for Pi — the first client integration.
 *
 * What it saves: configuring the MCP server by hand. Pi reads this extension,
 * finds the `graphium` binary and registers the server for the session, so a
 * `pi` session has `render_diagram` and `export_diagram` without a line of
 * `mcp.json`.
 *
 * What it adds on top: `/graphium` says where the binary is, which version it
 * is, where the exports land and what is already there; `/graphium open` opens
 * the plan without asking the agent for a diagram.
 *
 * What it teaches: the server is registered `exposure: "direct"`, so its tools
 * are declared to the model instead of waiting in a codemode namespace; one
 * sentence in `description` says what the server offers; and
 * `before_agent_start` attaches a short rule to each book tool, so the model
 * reaches for the book at the moment it is useful (f28 T6a).
 *
 * What it declares: the identity of the session it runs in, through the `env`
 * of the registration, so Graphium shows `Pi` as the agent family and keeps the
 * same session across a reconnect (f3 S4/D1). The identity is self-declared and
 * is not a security boundary: the fence is still `not_owner` in the owner.
 *
 * Rules this file follows (Pi extension docs):
 * - nothing long-lived starts in the factory: finding the binary is a `stat`,
 *   the version is read inside the command that needs it;
 * - every failure is a notify with the next step, never a throw: an extension
 *   that breaks must not take the session down.
 * - the teaching never replaces the prompt: it fills the per-tool guidelines
 *   slot, which is text Pi appends, while the server keeps the contract.
 */

import { execFileSync, spawn } from "node:child_process";
import { existsSync, readdirSync, statSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import type {
  ExtensionAPI,
  ExtensionCommandContext,
  ExtensionContext,
} from "@earendil-works/pi-coding-agent";

/** The name of the MCP server, as `/mcp` shows it. */
const SERVER = "graphium";

/** Where the binary is looked up, in the order we trust it. */
const EXPLICIT = "GRAPHIUM_BIN";

/** How many exported files the status lists. */
const RECENT_EXPORTS = 3;

/** What the server offers, in a sentence: tool search ranks the server's tools by
 * it, codemode's `describeNamespace()` returns it, and a model-facing tool
 * listing shows it once under the group heading (Pi docs, "Configure servers",
 * and `ToolNamespace`).
 *
 * With `exposure: "direct"` the server leaves the `mcp_servers` prompt section,
 * which only lists indirect servers; the sentence still answers anyone who asks
 * what the server is, which is how tool search and codemode find it.
 *
 * The budget is 169 characters and the text earns each one: the five tools of
 * the book by name (the old text named two), the export it can always keep
 * (`svg`; a `png` needs `path` and the rasterizer the app provides, so it is
 * left to `export_diagram`'s own description), and the condition it used to
 * hide — the book needs a project, and it can be off with `GRAPHIUM_BOOK=off`
 * or fail to open, answering `book_disabled` or `book_unavailable`. The
 * contract is the server's job, not this line's. */
const DESCRIPTION =
  "Dibuja diagramas y exporta SVG; responde desde el libro: `code_search`, " +
  "`code_structure`, `code_impact`, `code_diagram` y `code_er` (con proyecto y libro activo).";

/**
 * The rules this extension owns, keyed by the name the server gives the tool.
 *
 * They carry the moments no text carries today: a server's instructions are not
 * part of any tool description (Pi docs, "Control tool exposure"), and a tool
 * description teaches the contract, not when to reach for it. Each bullet names
 * its tool because it is read next to the declarations, where a rule with no
 * name is a rule for no tool. It is a handful on purpose: the server owns the
 * details, this file only teaches the moment.
 */
const BOOK_TOOL_GUIDELINES: Record<string, readonly string[]> = {
  code_search: [
    "Para ubicar un símbolo por su nombre, `code_search` responde del libro antes que cualquier grep.",
  ],
  code_structure: [
    "Antes de abrir un archivo para entenderlo, `code_structure` da su silueta de declaraciones.",
  ],
  code_impact: [
    "Si estás por grepear quién usa un símbolo, `code_impact` ya sabe quién apunta a él.",
    "Antes de cambiar una firma, `code_impact` recorre sus llamadores.",
  ],
  code_diagram: [
    "Para el dibujo sin escribir JSON a mano, `code_diagram` entrega el `graph` del vecindario ya armado.",
  ],
  code_er: [
    "Para el `er` de un `.sql`, `code_er` lo arma del archivo: no escribas el documento a mano.",
  ],
  code_index: [
    "Si un archivo que esperabas no está en el libro, `code_index` dice por qué lo saltó.",
  ],
};

/** The directory this extension lives in, wherever Pi resolved it from. */
function here(): string {
  try {
    return dirname(fileURLToPath(import.meta.url));
  } catch {
    return process.cwd();
  }
}

/**
 * Walks up from a directory looking for a checkout's build.
 *
 * Release before debug: the release build is what the human uses, the debug one
 * is what the tests build. The depth is bounded because walking to `/` from a
 * deep directory is not a search, it is a guess.
 *
 * `findBinary` calls it with two different meanings. From `here()` a hit is the
 * checkout the extension itself ships in; from the cwd, last, a hit is only the
 * human's working directory and never outranks the installed binary.
 */
function walkUpForBinary(start: string): string | null {
  let directory = resolve(start);
  for (let depth = 0; depth < 8; depth += 1) {
    for (const relative of ["target/release/graphium", "target/debug/graphium"]) {
      const candidate = join(directory, relative);
      if (existsSync(candidate)) {
        return candidate;
      }
    }
    const parent = dirname(directory);
    if (parent === directory) break;
    directory = parent;
  }
  return null;
}

/**
 * Finds the Graphium binary, or `null` when this machine has none.
 *
 * The order, and why it is this order:
 *
 * 1. `GRAPHIUM_BIN` wins over everything: a human who sets it has answered.
 * 2. The extension's own location: this file ships inside the Graphium checkout,
 *    so walking up from here finds that checkout's build even when Pi is started
 *    in another project. That is how a developer tests a change.
 * 3. The `PATH`: a `graphium` there is the installed product, and the extension
 *    is supposed to serve the product.
 * 4. Walking up from the cwd, last: the cwd is the human's project, not
 *    Graphium's checkout, so a build found there is a coincidence and must never
 *    shadow the installed binary. It stays as the last resort for the case where
 *    the extension is loaded from somewhere else and the human wants their
 *    working directory's build.
 */
export function findBinary(
  cwd: string = process.cwd(),
  near: string = here(),
): string | null {
  const explicit = process.env[EXPLICIT];
  if (explicit && existsSync(explicit)) {
    return explicit;
  }

  // The checkout this extension belongs to, not the one Pi happens to run in.
  const checkout = walkUpForBinary(near);
  if (checkout) {
    return checkout;
  }

  // The installed binary, on the PATH.
  const path = process.env.PATH ?? "";
  for (const directory of path.split(":")) {
    if (!directory) continue;
    const candidate = join(directory, "graphium");
    if (existsSync(candidate)) {
      return candidate;
    }
  }

  // Last: a build under whatever directory the human is working in.
  return walkUpForBinary(cwd);
}

/** The exports root Graphium writes to (PRD §7 RNF4). */
export function exportsRoot(): string {
  const configured = process.env.GRAPHIUM_EXPORTS_ROOT;
  return configured && configured.length > 0
    ? configured
    : join(homedir(), ".graphium", "exports");
}

/** The files already exported, newest first, with their size. */
export function recentExports(root: string, limit = RECENT_EXPORTS): string[] {
  const found: { name: string; size: number; time: number }[] = [];
  const walk = (directory: string, prefix: string) => {
    let entries: string[];
    try {
      entries = readdirSync(directory);
    } catch {
      return;
    }
    for (const entry of entries) {
      const full = join(directory, entry);
      let stats;
      try {
        stats = statSync(full);
      } catch {
        continue;
      }
      if (stats.isDirectory()) {
        walk(full, `${prefix}${entry}/`);
      } else {
        found.push({ name: `${prefix}${entry}`, size: stats.size, time: stats.mtimeMs });
      }
    }
  };
  walk(root, "");
  return found
    .sort((left, right) => right.time - left.time)
    .slice(0, limit)
    .map((file) => `${file.name} (${formatBytes(file.size)})`);
}

/** Bytes as the human reads them. */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** The version of the binary, or why it could not be read. */
export function versionOf(binary: string): string {
  try {
    return execFileSync(binary, ["--version"], { encoding: "utf8", timeout: 5000 }).trim();
  } catch (error) {
    return `no se pudo leer la versión: ${(error as Error).message}`;
  }
}

/** Opens the plan in its own process, without a session asking for a diagram. */
export function openPlan(binary: string): string {
  try {
    const child = spawn(binary, [], { detached: true, stdio: "ignore" });
    child.unref();
    return "ventana abierta";
  } catch (error) {
    return `no se pudo abrir la ventana: ${(error as Error).message}`;
  }
}

/** What the extension can read about the session it is running in. */
export interface IdentityInput {
  /** The session id a Pi context exposed, when there is one. */
  sessionId?: string | null;
  /** The project directory the event or the context provided. */
  cwd?: string | null;
}

/** The string, or `null`: a blank value is not an identity. */
function notEmpty(value: unknown): string | null {
  return typeof value === "string" && value.trim().length > 0 ? value : null;
}

/**
 * The `env` the MCP server is registered with: the identity Graphium shows in
 * its session selector (f3 S4/D1).
 *
 * `GRAPHIUM_CLIENT` and `GRAPHIUM_PROJECT` are always there; `GRAPHIUM_SESSION`
 * only when an id is known. A missing id is left out, never invented: a session
 * shown under the wrong name is worse for the human than one without a name.
 *
 * Pure on purpose, `env` included, so the three cases (id, no id, changed id)
 * are testable without Pi.
 */
export function identityEnv(
  identity: IdentityInput = {},
  env: Record<string, string | undefined> = process.env,
): Record<string, string> {
  const session = notEmpty(identity.sessionId) ?? notEmpty(env.PI_SESSION_ID);
  const built: Record<string, string> = {
    GRAPHIUM_CLIENT: "pi",
    GRAPHIUM_PROJECT: notEmpty(identity.cwd) ?? process.cwd(),
  };
  if (session) {
    built.GRAPHIUM_SESSION = session;
  }
  return built;
}

/**
 * The session id a Pi context exposes, or `null`.
 *
 * `ExtensionContext.sessionManager` is a `ReadonlySessionManager`, and
 * `getSessionId()` is the id `/resume` and the session file use. Every read is
 * guarded because a context goes stale when the session is replaced.
 */
export function sessionIdOf(ctx: ExtensionContext | null | undefined): string | null {
  try {
    return notEmpty(ctx?.sessionManager?.getSessionId());
  } catch {
    return null;
  }
}

/** The working directory a Pi context exposes, or `null`. */
export function cwdOf(ctx: ExtensionContext | null | undefined): string | null {
  try {
    return notEmpty(ctx?.cwd);
  } catch {
    return null;
  }
}

/** Whether two identities say the same thing, key by key. */
function sameIdentity(left: Record<string, string>, right: Record<string, string>): boolean {
  const keys = Object.keys(left);
  return keys.length === Object.keys(right).length && keys.every((key) => left[key] === right[key]);
}

/**
 * The name Pi declares a tool of this server with: `mcp__<server>__<tool>`
 * (Pi docs, "Configuration rules").
 *
 * The template is exact, not an approximation: the server name has no character
 * Pi would sanitize, and every book tool is lowercase with underscores.
 */
export function declaredToolName(tool: string): string {
  return `mcp__${SERVER}__${tool}`;
}

/**
 * The one line `/graphium` prints about the book the agent will see.
 *
 * It reads the two sources that decide it and invents none: the identity env
 * this extension registers (`GRAPHIUM_PROJECT`, through `identityEnv`) and the
 * `GRAPHIUM_BOOK` the MCP child inherits from this process. A session without a
 * project is the `invalid_value` at `session.project` the server answers.
 */
export function bookLine(
  identity: Record<string, string | undefined> = identityEnv(),
  env: Record<string, string | undefined> = process.env,
): string {
  const off = (env.GRAPHIUM_BOOK ?? "").trim() === "off";
  const project = notEmpty(identity.GRAPHIUM_PROJECT);
  if (off) {
    return "libro: apagado (GRAPHIUM_BOOK=off): el agente no tiene índice y escribe los documentos a mano";
  }
  return project
    ? `libro: activo: el agente lee el índice de ${project}`
    : "libro: activo, pero la sesión no declara GRAPHIUM_PROJECT: el libro responde invalid_value";
}

/** The lines `/graphium` prints. */
export function statusLines(
  pi: Pick<ExtensionAPI, "getMcpServers">,
  binary: string | null,
  identity: Record<string, string | undefined> = identityEnv(),
): string[] {
  const root = exportsRoot();
  const recent = recentExports(root);
  const registered = pi
    .getMcpServers()
    .some((server) => server.name === SERVER);
  return [
    binary
      ? `binario: ${binary} · ${versionOf(binary)}`
      : `binario: no encontrado (definí ${EXPLICIT} o compilá con \`cargo build --release\`)`,
    binary
      ? `MCP: ${registered ? "registrado para esta sesión" : "no registrado"}, como \`${SERVER}\` en /mcp`
      : "MCP: sin binario no hay servidor que registrar",
    bookLine(identity),
    `raíz de exportes: ${root}`,
    recent.length > 0
      ? `últimos archivos: ${recent.join(", ")}`
      : "últimos archivos: todavía no exportaste nada",
    "para el plano: `/graphium open` abre la ventana sin pedir un diagrama",
  ];
}

/**
 * The extension.
 *
 * `binary` is a parameter with a default instead of a local call so the no-binary
 * path can be exercised: a test cannot make this checkout disappear.
 */
export default function graphium(
  pi: ExtensionAPI,
  binary: string | null = findBinary(),
) {
  /** The `env` the current registration carries; `null` until one works. */
  let registered: Record<string, string> | null = null;
  /** Why the first registration failed, so the human hears it once, at start. */
  let failure: Error | null = null;

  const register = (identity: Record<string, string>): void => {
    if (!binary) return;
    pi.registerMcpServer(SERVER, {
      command: binary,
      args: ["--stdio"],
      env: identity,
      description: DESCRIPTION,
      // `direct` is "declared to the model while active" (Pi docs, "Tool
      // exposure"). Without it a registered server falls back to `codemode`,
      // which leaves the tools callable only from a script and declared to
      // nobody: that is the defect this flag fixes, and the reason an agent
      // reached for `grep` while `code_impact` was connected.
      exposure: "direct",
    });
    registered = identity;
  };

  if (binary) {
    // Registering here is what the docs call "while the extension loads": the
    // session connects it at start, and a `mcp.json` entry with the same name
    // still wins, so the human keeps the last word.
    //
    // The identity is as complete as the process knows it and no more: Pi has
    // no session while extensions load, so `session_start` finishes the job.
    //
    // A registration that throws (an invalid config, another extension owning
    // the MCP transport) must not take the session down: `/graphium` says
    // what happened, because a factory has no UI to say it with.
    try {
      register(identityEnv());
    } catch (error) {
      registered = null;
      failure = error as Error;
    }
  }

  pi.on("session_start", async (_event, ctx) => {
    if (!binary) {
      ctx.ui.notify(
        `Graphium: no encontré el binario. Definí ${EXPLICIT} o compilá con \`cargo build --release\`.`,
        "info",
      );
      return;
    }
    if (failure) {
      ctx.ui.notify(
        `Graphium: el servidor MCP no se pudo registrar (${failure.message})`,
        "info",
      );
      return;
    }

    // A new session in the same Pi process must reach Graphium under its own
    // name, or it would inherit the diagrams of the one before it. The check is
    // against what was registered, so an unchanged session is left alone.
    const wanted = identityEnv({ sessionId: sessionIdOf(ctx), cwd: cwdOf(ctx) });
    if (registered && sameIdentity(registered, wanted)) {
      return;
    }
    try {
      if (registered) {
        pi.unregisterMcpServer(SERVER);
        registered = null;
      }
      register(wanted);
    } catch (error) {
      // Never throw out of the handler: a new identity is worth reporting, but
      // not worth taking the session down for.
      try {
        ctx.ui.notify(
          `Graphium: el servidor MCP no se pudo registrar (${(error as Error).message})`,
          "info",
        );
      } catch {
        // No UI left to say it with.
      }
    }
  });

  // The guide's per-tool slot is `toolGuidelines` in the options of
  // `before_agent_start`: "Guideline bullets contributed by each tool, keyed by
  // tool name", rendered while the tool is active — the same mechanism as the
  // `promptGuidelines` of a tool this extension could register itself. Pi
  // registers the MCP tools, so this hook is the only place the extension can
  // attach a rule to them. The keys are written even while the tools are not
  // declared, because codemode carries a tool's guidelines into its
  // declarations when a human overrides the exposure.
  //
  // Pi reports a throwing handler and goes on (Pi docs, "Errors and cleanup"),
  // but this file's rule is stricter: a failure of the teaching never leaves
  // this extension. The next run gets the text again.
  pi.on("before_agent_start", (event) => {
    try {
      const options = event.systemPromptOptions;
      if (!options) return;
      for (const [tool, bullets] of Object.entries(BOOK_TOOL_GUIDELINES)) {
        options.toolGuidelines[declaredToolName(tool)] = [...bullets];
      }
    } catch {
      // Lose the line, never the run.
    }
  });

  pi.registerCommand("graphium", {
    description:
      "Estado de Graphium (binario, versión, libro, raíz de exportes); `open` abre la ventana",
    handler: async (args: string, ctx: ExtensionCommandContext) => {
      const verb = args.trim();
      if (verb === "open") {
        if (!binary) {
          ctx.ui.notify(`Graphium: no encontré el binario para abrir el plano`, "info");
          return;
        }
        ctx.ui.notify(`Graphium: ${openPlan(binary)}`, "info");
        return;
      }
      for (const line of statusLines(
        pi,
        binary,
        identityEnv({ sessionId: sessionIdOf(ctx), cwd: cwdOf(ctx) }),
      )) {
        ctx.ui.notify(line, "info");
      }
    },
  });
}
