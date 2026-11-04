# Graphium

Diagramas que la IA puede dibujar y el humano puede leer.

Graphium es un servidor **MCP** y una **ventana-plano** en el mismo binario: el
agente manda el diagrama como JSON por MCP, Graphium lo valida, lo compone con
su propio motor (layout `dagre` + renderer SVG propio con métricas de fuente
reales) y lo pinta sobre un plano infinito con grilla, en lugar de devolver un
texto que nadie mira.

```
agente ──JSON──▶ validación ──▶ IR ──▶ layout ──▶ SVG ──▶ plano
                    │
                    └── errores accionables (`code`, `path`, `hint`)
```

## Estado

| Fase | Alcance | Estado |
|---|---|---|
| **F0** | Spike técnico: workspace, Tauri 2, `dagre`, renderer propio, ventana con grilla, socket single-instance, round-trip de render | cerrada |
| **F1** | MVP: kinds `graph` y `er`, `validate_diagram`, `get_diagram_schema`, ciclo de vida de diagramas, harness del agente, degradación y presupuesto, `opts.extract`, cámara con fit-to-view, panel de ajustes con tema claro, golden SVG en CI | cerrada |
| **F2 (en curso)** | Formato completo. Hecho: **los 10 kinds del catálogo** (`flowchart`, `sequence`, `class`, `state`, `component`, `dataflow`, `tree`, `mindmap`, `timeline`, `swimlane`) con los motores `tree` y `lanes`, **varios diagramas en el plano**, panel lateral y culling, **export `svg`/`png`** (`export_diagram`: inline sin tocar disco o a archivo dentro de la raíz de exportes, sin Mermaid). Falta: clusters, `emphasis`, historial, `opts.replace_id` y `opts.interactive` | en curso |
| F3 | Interactividad, click→evento, watch | pendiente |

Decisiones y criterios de aceptación: [`docs/PRD.md`](docs/PRD.md).
Trabajo por tarea, con specs y evidencia: [`odd/tasks/`](odd/tasks/).
Integraciones propias de los clientes (Pi primero): [`extensions/`](extensions/README.md).

## Compilar

Únicas dependencias de sistema: `webkit2gtk-4.1` y `gtk3`.

```bash
# Fedora
sudo dnf install webkit2gtk4.1-devel gtk3-devel
# Debian/Ubuntu
sudo apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev

cargo build --release --bin graphium
cargo test --workspace
```

## Instalar

Con el repositorio clonado, instala el binario en `$HOME/.local/bin` y la
extensión en tu agente de IA:

```bash
./install.sh          # compila e instala $HOME/.local/bin/graphium
./install.sh agent    # instala la extensión en el agente (pi; pregunta en una tty)
./install.sh update   # actualiza binario y extensión si hay un release más nuevo
./install.sh uninstall
```

Sin clonar nada, una vez publicado el primer release:

```bash
curl -fsSL https://github.com/Xeworg/graphium/releases/latest/download/install.sh | bash
```

El binario abre una ventana (Tauri): necesita `webkit2gtk-4.1` en tiempo de
ejecución; el instalador lo avisa si falta. Los tests del instalador corren
contra un HOME de scratch: `bash scripts/test-installer.sh`.

## Usarlo desde un cliente MCP

El binario imprime su propia configuración:

```bash
./target/release/graphium mcp-config --client pi   # pi | claude | cursor | generic
```

Y el quickstart para agentes está en [`docs/QUICKSTART.md`](docs/QUICKSTART.md).

## Garantías que el repositorio sostiene con tests

- **Determinismo:** el mismo JSON produce el mismo SVG byte a byte, con
  golden files comparados en CI.
- **Presupuestos:** bytes del SVG (`900 B × nodo + 350 B × arista + 40 KB`),
  matriz de layout medida en release y techo de 15 MB del binario, todos
  medidos en CI.
- **Degradación antes que rechazo:** pasado el aviso de tamaño el diagrama
  pierde primero los labels de las aristas, después aire, después el kerning
  fino, y solo entonces se corta con un `hint` que dice cómo seguir.
- **El plano es del humano:** colores, grilla y tema viven en
  `~/.graphium/settings.json` y el agente no los toca.

## Licencia

MIT. La fuente Inter embebida (`assets/fonts/`) se distribuye con su propia
licencia SIL Open Font License en `assets/fonts/LICENSE-Inter.txt`.
