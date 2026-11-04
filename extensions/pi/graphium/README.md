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
- **`/graphium`** muestra dónde está el binario, qué versión es, si el MCP quedó
  registrado, cuál es la raíz de exportes y los últimos archivos exportados.
- **`/graphium open`** abre la ventana del plano sin pedirle un diagrama al
  agente.
- Si no encuentra el binario, **no rompe la sesión**: avisa qué hacer
  (`GRAPHIUM_BIN` o `cargo build --release`).

Dónde busca el binario, en orden:

1. `GRAPHIUM_BIN`, si está definida y existe.
2. `target/release/graphium` y después `target/debug/graphium` en el directorio
   actual (un checkout de Graphium).
3. `graphium` en el `PATH`.

## Instalar

Con el repo clonado, para todas las sesiones de Pi:

```bash
ln -s "$PWD/extensions/pi/graphium" ~/.pi/agent/extensions/graphium
```

o, si preferís no tocar tu carpeta de extensiones, agregá la ruta a
`~/.pi/agent/settings.json`:

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
del servidor MCP y que el comando sin binario avise en vez de tirar.

## Un detalle que esta extensión no arregla

`graphium mcp-config --client pi` imprime TOML, pero Pi lee
`~/.pi/agent/mcp.json` en JSON. Mientras eso siga así, la extensión es el camino
correcto para Pi: no pasa por ese archivo.
