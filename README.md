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

Únicas dependencias de sistema: `webkit2gtk-4.1` y `gtk3`. Para compilar desde
el repositorio hace falta además **Node 22 y npm**: el plano es una app Svelte
y su bundle se embebe en el binario al compilar (`scripts/build.sh` hace
`npm ci` + `npm run build` y después `cargo build`).

```bash
# Fedora
sudo dnf install webkit2gtk4.1-devel gtk3-devel
# Debian/Ubuntu
sudo apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev

cargo build --release --bin graphium
cargo test --workspace
```

`scripts/build.sh` hace las dos mitades en orden (`npm ci` + `npm run build` →
`cargo build`); si lo hacés a mano, compilá el bundle primero:

```bash
cd crates/gx-app/frontend && npm ci && npm run build && cd -
cargo build --release --bin graphium
```

## Instalar

Con el repositorio clonado, instala el binario en `$HOME/.local/bin` y la
extensión en tu agente de IA:

```bash
./install.sh          # compila, instala el binario y ofrece conectar tu agente
./install.sh agent    # elige con un checklist en qué agentes instalar la extensión
./install.sh update   # actualiza binario y extensión si hay un release más nuevo
./install.sh uninstall
```

Sin clonar nada, una vez publicado el primer release:

```bash
curl -fsSL https://github.com/Xeworg/graphium/releases/latest/download/install.sh | bash
```

Al final de `./install.sh`, y en `./install.sh agent`, aparece un checklist con
los hosts de agente que el instalador conoce (hoy `pi` y `gentle-shell`):

- Arranca con los hosts detectados en la máquina ya marcados y muestra cuáles
  ya tienen la extensión instalada (y su versión).
- ↑/↓ mueve, espacio marca o desmarca, `a` marca/desmarca todos, Enter
  confirma, y `q` (o Esc) sale sin instalar nada: así se saltea y se conecta el
  agente más tarde.
- Si la terminal no da para el checklist (TERM tonta, sin `tput`/terminfo)
  pregunta con números, como antes.
- Sin terminal (scripts, pipes) hay que pedir los agentes explícitos:
  `./install.sh agent --env pi [--env gentle-shell]` o `./install.sh agent
  --all`.

Agregar un agente nuevo es una línea en el registro `AGENT_REGISTRY` de
`install.sh` (nombre, variable de entorno y directorio por defecto): la ayuda,
la detección, el estado, el checklist y la instalación salen de ahí.

El binario abre una ventana (Tauri): necesita `webkit2gtk-4.1` en tiempo de
ejecución; el instalador lo avisa si falta. Los tests del instalador corren
contra un HOME de scratch: `bash scripts/test-installer.sh`.

## Usarlo desde un cliente MCP

El binario imprime su propia configuración:

```bash
./target/release/graphium mcp-config --client pi   # pi | claude | cursor | generic
```

El cliente habla con un **daemon en la bandeja**: la primera vez que un
cliente arranca, lanza `graphium --owner` (detached, sobrevive al cierre del
cliente); la ventana solo aparece con el primer diagrama o con el click en el
icono de la bandeja, cerrarla la oculta, y el proceso muere solo cuando no
quedan sesiones (`graphium quit` lo cierra cuando quieras). RF31 del PRD.

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
