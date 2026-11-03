# QUICKSTART

Graphium dibuja diagramas: la IA manda el JSON por MCP y la ventana los
coloca sobre un plano con grilla.

Esta es la guía de F0 (spike técnico). El MVP completo llega en F1.

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

## 4. Modos del binario

| Comando | Qué hace |
|---|---|
| `graphium` | Abre la ventana en modo solo-ui (sin servidor MCP). |
| `graphium --stdio` | Modo MCP: sirve por stdio y es dueño de la ventana. |
| `graphium mcp-config` | Imprime el JSON de configuración MCP. |
| `graphium --version` | Imprime la versión. |
| `graphium --help` | Imprime la ayuda. |

## 5. Una sola ventana

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
