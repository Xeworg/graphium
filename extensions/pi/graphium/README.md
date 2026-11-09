# Graphium para Pi

Extensión de Pi que conecta Graphium sin configurar nada a mano: busca el
binario, registra el servidor MCP para la sesión y agrega el comando
`/graphium`. Es la primera integración de [`extensions/`](../../README.md).

## Qué hace

- **Registra el MCP solo.** Al cargar, busca el binario y llama
  `pi.registerMcpServer("graphium", …)`. La sesión arranca con `render_diagram`,
  `export_diagram` y el resto de las tools disponibles, sin `mcp.json`. Un
  `mcp.json` con una entrada `graphium` sigue ganando: el humano tiene la última
  palabra.
- **Le cuenta al agente lo que registra.** La descripción del servidor nombra el
  libro del proyecto y sus dos rutas de dibujo: `code_diagram` para el código y
  `code_er` para un `.sql`. `/mcp` y el system prompt la leen, así que el agente
  sabe que hay un índice antes de escribir un documento a mano.
- **Declara quién es la sesión.** En el `env` del registro pasa
  `GRAPHIUM_CLIENT=pi`, `GRAPHIUM_PROJECT` (el directorio de trabajo) y
  `GRAPHIUM_SESSION` (el id de la sesión de Pi, cuando se puede leer). Gracias a
  eso la ventana muestra la sesión como «Pi», la distingue de otro agente, y
  sigue siendo la misma después de reconectar. Si el id cambia —otra sesión en
  el mismo Pi— la extensión vuelve a registrar el servidor con la identidad
  nueva; si no se puede leer ningún id, el `env` va sin `GRAPHIUM_SESSION` y no
  se inventa ninguno.
- **`/graphium`** muestra dónde está el binario, qué versión es, si el MCP quedó
  registrado, si el libro está activo y de qué proyecto lee el índice, cuál es la
  raíz de exportes y los últimos archivos exportados. La línea del libro sale de
  lo que la extensión ya sabe: `GRAPHIUM_BOOK` (apagado solo con `off`) y el
  `GRAPHIUM_PROJECT` que declara el `env` de la sesión.
- **`/graphium open`** abre la ventana del plano sin pedirle un diagrama al
  agente.
- Si no encuentra el binario, **no rompe la sesión**: avisa qué hacer
  (`GRAPHIUM_BIN` o `scripts/build.sh --release` en un checkout del repositorio).

Dónde busca el binario, en orden:

1. `GRAPHIUM_BIN`, si está definida y existe.
2. `target/release/graphium` y después `target/debug/graphium` en el directorio
   actual (un checkout de Graphium).
3. `graphium` en el `PATH`.

## Instalar

Con el instalador del repositorio (recomendado: instala el binario en
`$HOME/.local/bin`, así la extensión lo encuentra por el PATH):

```bash
./install.sh agent    # pregunta en qué agente instalar; sin tty: --env pi
```

A mano, con el repo clonado, para todas las sesiones de cada host:

```bash
# pi: PI_CODING_AGENT_DIR o ~/.pi/agent
# gentle-shell: GENTLE_SHELL_AGENT_DIR o ~/.gentle-shell/agent
ln -s "$PWD/extensions/pi/graphium" "${PI_CODING_AGENT_DIR:-$HOME/.pi/agent}/extensions/graphium"
ln -s "$PWD/extensions/pi/graphium" "${GENTLE_SHELL_AGENT_DIR:-$HOME/.gentle-shell/agent}/extensions/graphium"
```

En esta máquina `PI_CODING_AGENT_DIR` es `/home/xeworg/.gentle-shell/agent`;
con el instalador no importa: `--all` instala la copia en cada host presente
y tenerla en ambos no da conflicto (cada host registra su propio MCP y sus
sesiones son independientes).

o, si preferís no tocar tu carpeta de extensiones, agregá la ruta al
`settings.json` de ese mismo directorio:

```json
{ "extensions": ["/ruta/a/Graphium/extensions/pi/graphium"] }
```

Para probarla una sola vez, sin instalar nada:

```bash
pi -e ./extensions/pi/graphium/index.ts
```

Después del primer arranque, `/mcp` muestra `graphium` conectado y `/graphium`
te dice cómo quedó todo.

## Verificar sin Pi

El chequeo corre sobre un `ExtensionAPI` de mentira, así que no necesita Pi ni un
modelo:

```bash
node extensions/pi/graphium/verify.mjs
```

Cubre la búsqueda del binario (incluida la ausencia), la raíz de exportes, el
listado de archivos exportados, el orden de los bytes que muestra, el registro
del servidor MCP, la descripción que vende el libro, la línea del libro y que el
comando sin binario avise en vez de tirar. Para la comprobación que exige un
build en el checkout (`target/release/graphium` o `target/debug/graphium`),
compilá antes con `scripts/build.sh --release`.

## La configuración que Pi lee

Pi lee `~/.pi/agent/mcp.json` en JSON, con el mismo objeto `mcpServers` que los
otros clientes MCP. `graphium mcp-config --client pi` imprime exactamente esa
entrada, con la ruta absoluta del binario, para pegarla a mano:

```json
{
  "mcpServers": {
    "graphium": {
      "command": "/ruta/a/graphium",
      "args": ["--stdio"]
    }
  }
}
```

La extensión no pasa por ese archivo: registra el servidor desde `index.ts` y le
agrega el `env` de identidad. Un `mcp.json` con una entrada `graphium` sigue
ganando, así que el humano tiene la última palabra.
