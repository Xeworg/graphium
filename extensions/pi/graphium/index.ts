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
 * Rules this file follows (Pi extension docs):
 * - nothing long-lived starts in the factory: finding the binary is a `stat`,
 *   the version is read inside the command that needs it;
 * - every failure is a notify with the next step, never a throw: an extension
 *   that breaks must not take the session down.
 */

import { execFileSync, spawn } from "node:child_process";
import { existsSync, readdirSync, statSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import type { ExtensionAPI, ExtensionCommandContext } from "@earendil-works/pi-coding-agent";

/** The name of the MCP server, as `/mcp` shows it. */
const SERVER = "graphium";

/** Where the binary is looked up, in the order we trust it. */
const EXPLICIT = "GRAPHIUM_BIN";

/** How many exported files the status lists. */
const RECENT_EXPORTS = 3;

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
 * The extension's own location comes first, not the working directory: this
 * file is shipped inside the Graphium checkout, so walking up from here finds
 * the build even when Pi is started in another project.
 */
export function findBinary(
  cwd: string = process.cwd(),
  near: string = here(),
): string | null {
  const explicit = process.env[EXPLICIT];
  if (explicit && existsSync(explicit)) {
    return explicit;
  }

  const checkout = walkUpForBinary(near) ?? walkUpForBinary(cwd);
  if (checkout) {
    return checkout;
  }

  // An installed binary, on the PATH.
  const path = process.env.PATH ?? "";
  for (const directory of path.split(":")) {
    if (!directory) continue;
    const candidate = join(directory, "graphium");
    if (existsSync(candidate)) {
      return candidate;
    }
  }
  return null;
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

/** The lines `/graphium` prints. */
export function statusLines(pi: Pick<ExtensionAPI, "getMcpServers">, binary: string | null): string[] {
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
  if (binary) {
    // Registering here is what the docs call "while the extension loads": the
    // session connects it at start, and a `mcp.json` entry with the same name
    // still wins, so the human keeps the last word.
    //
    // A registration that throws (an invalid config, another extension owning
    // the MCP transport) must not take the session down: `/graphium` says
    // what happened, because a factory has no UI to say it with.
    try {
      pi.registerMcpServer(SERVER, {
        command: binary,
        args: ["--stdio"],
        description:
          "Graphium: dibuja diagramas en una ventana sobre un plano y los exporta a SVG o PNG",
      });
    } catch (error) {
      pi.on("session_start", async (_event, ctx) => {
        ctx.ui.notify(
          `Graphium: el servidor MCP no se pudo registrar (${(error as Error).message})`,
          "info",
        );
      });
    }
  } else {
    pi.on("session_start", async (_event, ctx) => {
      ctx.ui.notify(
        `Graphium: no encontré el binario. Definí ${EXPLICIT} o compilá con \`cargo build --release\`.`,
        "info",
      );
    });
  }

  pi.registerCommand("graphium", {
    description:
      "Estado de Graphium (binario, versión, raíz de exportes); `open` abre la ventana",
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
      for (const line of statusLines(pi, binary)) {
        ctx.ui.notify(line, "info");
      }
    },
  });
}
