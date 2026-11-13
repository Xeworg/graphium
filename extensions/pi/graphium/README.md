# Graphium para Pi

Extensión de Pi que conecta Graphium sin configurar nada a mano: busca el
binario, registra el servidor MCP para la sesión y agrega el comando
`/graphium`. Es la primera integración de [`extensions/`](../../README.md).

## Qué hace

- **Registra el MCP solo.** Al cargar, busca el binario y llama
  `pi.registerMcpServer("graphium", …)` con `exposure: "direct"`, la exposición
  que Pi documenta como «declarada al modelo mientras la sesión vive». Las tools
  —`render_diagram`, `export_diagram`, `code_impact` y el resto— se declaran al
  modelo; sin ese `exposure` el servidor cae en `codemode` y el agente solo las
  encuentra si se le ocurre buscarlas. Un `mcp.json` con una entrada `graphium`
  sigue ganando: el humano tiene la última palabra.
- **Le cuenta al agente lo que registra.** La descripción del servidor es una
  sola frase que nombra las cinco tools del libro —`code_search`,
  `code_structure`, `code_impact`, `code_diagram` y `code_er`— y no promete de
  más: `svg` siempre se exporta, un `png` necesita `path` y el rasterizador de la
  app (y por eso lo cuenta la descripción de `export_diagram`, no esta línea), y
  el libro necesita un proyecto y estar activo: `GRAPHIUM_BOOK=off` lo apaga y
  sin proyecto contesta `invalid_value`. La frase es la misma que rankea la
  búsqueda de tools y la que devuelve `describeNamespace()` en codemode.
- **Enseña el momento, no solo la tool.** En `before_agent_start` la extensión
  llena el slot de guías por tool (`toolGuidelines`) con un puñado de reglas
  cortas, cada una con el nombre de su tool: que `code_impact` ya sabe quién usa
  un símbolo antes de que grepeés, que antes de cambiar una firma recorre sus
  llamadores, que `code_search` ubica un símbolo por su nombre, o que si un
  archivo que esperabas no está en el libro, `code_index` dice por qué lo saltó.
  Es el mismo mecanismo con el que Pi suma las reglas de sus propias tools, así
  que el texto se lee junto a las declaraciones y no aparece si las tools no
  están. El contrato de cada tool sigue siendo del servidor: acá solo va cuándo
  conviene llamarlas.
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

1. `GRAPHIUM_BIN`, si está definida y existe: quien la define ya respondió.
2. Subiendo desde el directorio de la extensión: `target/release/graphium` y
   después `target/debug/graphium` del checkout al que la extensión pertenece,
   que es como se prueba un cambio en desarrollo.
3. `graphium` en el `PATH`: es el producto instalado, y la extensión sirve al
   producto.
4. Al final, subiendo desde el directorio de trabajo: un build ahí es una
   coincidencia del proyecto en el que estás, así que nunca le gana al binario
   instalado. Queda como último recurso para cuando la extensión se carga desde
   otro lado y querés el build de tu directorio.

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
del servidor MCP con `exposure: "direct"`, la descripción que nombra las cinco
tools del libro sin prometer lo que no cumple, el slot de guías por tool (cada
bullet nombra su tool), que los hooks estén cableados y que una guía que falla
no rompa la sesión, la línea del libro y que el comando sin binario avise en vez
de tirar. Un check que falla no corta el resto: la corrida lista todos. Para la
comprobación que exige un build en el checkout (`target/release/graphium` o
`target/debug/graphium`), compilá antes con `scripts/build.sh --release`.

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

Si escribís la entrada a mano, agregale `"exposure": "direct"` y el `env` si
querés la identidad: una entrada de `mcp.json` sin `exposure` cae en `codemode`,
y ahí el modelo no ve las tools hasta que se le ocurre buscarlas — el mismo
problema que la extensión arregla, reintroducido por el archivo. `graphium
mcp-config --client pi` imprime la entrada mínima del binario; la exposición es
decisión del cliente.
