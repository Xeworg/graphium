# QUICKSTART

Graphium dibuja diagramas: la IA manda el JSON por MCP y la ventana los
coloca sobre un plano con grilla.

F0 (spike técnico) y F1 (MVP) están cerradas, y el catálogo de F2 está
completo: **12 kinds**. Un diagrama es un documento JSON con su `kind`; el
agente lo manda por MCP y la ventana lo pone en el plano.

## 1. Compilar

```bash
cargo build --release --bin graphium
```

Únicas dependencias de sistema: `webkit2gtk-4.1` y `gtk3` (RNF2). En Fedora:

```bash
sudo dnf install webkit2gtk4.1-devel gtk3-devel
```

## 2. Configurar el cliente MCP

El binario imprime su propia configuración:

```bash
./target/release/graphium mcp-config
```

```json
{"command": "/ruta/absoluta/al/graphium", "args": ["--stdio"]}
```

Copiar ese JSON en la configuración MCP del cliente. En Pi, el bloque es:

```toml
[mcp_servers.graphium]
command = "/ruta/absoluta/al/graphium"
args = ["--stdio"]
```

## 3. Pedir un diagrama

Desde el cliente MCP:

```
render_diagram con el documento {"version": 1, "kind": "graph", "nodes": [{"id": "a", "label": "A"}], "edges": []}
```

La ventana abre con el diagrama sobre el plano. Con la rueda hacés zoom,
arrastrando hacés pan, y con el fondo del plano la grilla se mueve con vos.

## 4. Exportar el diagrama

El diagrama vive en la ventana; para sacarlo de ahí está `export_diagram`. Dos
caminos, y no son intercambiables.

**Texto, para pegar.** Cero acceso a disco: el SVG vuelve por MCP en `content`.

```
export_diagram con {"format": "svg", "opts": {"inline": true}}
```

**Archivo, dentro de la raíz de exportes** (`~/.graphium/exports/`, o
`GRAPHIUM_EXPORTS_ROOT` si la configuraste):

```
export_diagram con {"format": "svg", "opts": {"path": "arquitectura.svg"}}
export_diagram con {"format": "png", "opts": {"path": "arquitectura.png", "scale": 2}}
```

Queda en `~/.graphium/exports/arquitectura.png`. El `path` es **siempre
relativo** a esa raíz: los absolutos, el `..` y los symlinks vuelven con
`export_path_outside_root` y no escriben nada. Pisar un archivo existente pide
`"overwrite": true`; sin eso la respuesta es `export_exists` y los bytes viejos
quedan. `png` **necesita** `path` (son bytes, no viajan como texto), acepta
`scale` hasta 3 y un techo de píxeles; el SVG inline tiene tope de 1 MB y el
export entero, deadline de 5 s. Exportar **no toca el plano**: no abre, no
cierra ni vuelve a dibujar nada.

## 5. Modos del binario

| Comando | Qué hace |
|---|---|
| `graphium` | Abre la ventana en modo solo-ui (sin servidor MCP). |
| `graphium --stdio` | Modo MCP: sirve por stdio y es dueño de la ventana. |
| `graphium mcp-config` | Imprime el JSON de configuración MCP. |
| `graphium --version` | Imprime la versión. |
| `graphium --help` | Imprime la ayuda. |

## 6. Una sola ventana

El primer proceso es dueño de la ventana. Un segundo cliente MCP se conecta
por el socket local y **reenvía** sus pedidos: los dos comparten la misma
ventana y ninguno abre una segunda.

## Problemas frecuentes

- **"webkit2gtk no encontrado"**: falta `libwebkit2gtk-4.1-dev` (o
  `webkit2gtk4.1-devel` en Fedora). El mensaje lo dice al arrancar.
- **La ventana no aparece**: estás en una sesión sin display. El render
  responde `rendered: false` y el diagrama se puede exportar igual.
- **La grilla no se ve**: con zoom menor a 0.3 la grilla se oculta a propósito
  para no tapar los diagramas.

## Para agentes: cuándo usar Graphium y cuándo no

Graphium le manda al cliente MCP sus propias instrucciones: el agente no
necesita leer este archivo para saber qué hacer. El resumen es este.

**Usalo** cuando una imagen explique mejor que un texto: el flujo de una
función o módulo, dependencias entre paquetes, el schema de una base de datos
(tablas, columnas, claves, cardinalidad), o la arquitectura de unos servicios.

**No lo uses** para un dato suelto, una lista de archivos, un diff o un stack
trace: un diagrama ahí cuesta una ventana y no agrega nada. Nunca dibujes para
compensar no haber entendido el código: primero se lee.

### El ciclo que el agente sigue

| Paso | Tool | Qué pasa |
|---|---|---|
| 1 | `get_diagram_schema` (sin `kind`) | Lista los kinds disponibles con sus límites y un ejemplo. |
| 2 | `get_diagram_schema` con `kind` | El JSON Schema, las reglas cruzadas, los shapes válidos y los límites. |
| 3 | `validate_diagram` | Dry-run: la misma validación que el render, **sin abrir nada**. Se itera gratis. |
| 4 | `render_diagram` | Dibuja en el plano. `rendered: true` es que la ventana confirmó el pintado. |
| 5 | `update_diagram` | Cambia un diagrama existente con un merge patch RFC 7386, transaccional. |
| 6 | `export_diagram` | Lo saca de la ventana: `format: "svg"` + `opts.inline` para pegarlo, `opts.path` para escribirlo dentro de la raíz de exportes (el PNG exige `path`). |

Después de una compaction, el agente recupera el contexto con
`list_diagrams` y `get_diagram` en vez de adivinar un `diagram_id`.

### Los códigos de error que hay que mirar

| Código | Qué significa |
|---|---|
| `unknown_ref` | Una arista o relación apunta a un id que no existe. |
| `duplicate_id` | Dos nodos o tablas con el mismo id. |
| `unknown_field` | Una clave que el schema no admite; el `hint` trae el nombre parecido. |
| `unknown_kind` | El kind no existe; el `hint` lista los que sí. |
| `too_large` | El documento supera el límite de su kind. |
| `stale_revision` | Se mandó una revisión vieja a `update_diagram`. |
| `not_owner` | El diagrama pertenece a otra sesión MCP. |
| `export_path_outside_root` | El `path` del export es absoluto, sube con `..` o pasa por un symlink. |
| `export_exists` | El archivo ya está en la raíz de exportes: mandá `overwrite: true`. |
| `export_timeout` | El export no terminó en 5 s. |
| `disk_quota_exceeded` | La raíz de exportes llegó a su tope de 200 MB. |

### Configurar el cliente

```bash
graphium mcp-config --client pi      # TOML para Pi
graphium mcp-config --client claude  # JSON para Claude
graphium mcp-config --client cursor  # JSON para Cursor
graphium mcp-config                 # el JSON genérico del PRD
```
