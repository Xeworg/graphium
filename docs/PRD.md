# PRD — Graphium

**Visualizador de diagramas con motor propio, controlado por agentes de IA vía MCP**

- **Estado:** v1.7 — catálogo de **24 kinds** (5 motores de layout) + ventana como **plano (canvas) con grilla** + **escalabilidad documentada con números** (§7.3) + backlog P1/P2 de la revisión por pares (§19)
- **Fecha:** 2026-11-03
- **Owner:** Usuario + agente (Pi / el Gentleman)
- **Repo:** `/home/xeworg/Proyectos/Graphium`

---

## 1. Visión

Graphium es una aplicación de escritorio con **motor de diagramas propio** (render SVG nuestro + layout open source aislado) cuya ventana es un **plano (canvas) infinito con grilla**, donde los diagramas renderizados se colocan como objetos, se recorren con zoom/pan y se cierran uno por uno, cuando un agente de IA lo pide. El agente (Pi, Claude, u otro) no dibuja: **describe el diagrama en JSON** a través de un servidor **MCP** embebido; Graphium valida, calcula el layout y renderiza.

> Caso de uso del usuario (textual): *"que yo te pida un diagrama y me lo abras desde aca u otro agente meabras una ventana y me de el diagrama que pedi, ejemplo como funciona x funcion o diagrama la base de datos"*.

Un binario. Sin Mermaid como motor, sin librerías JS de diagramas: **nuestro propio sistema**. El canal primario es MCP + JSON versionado; el acceso a disco queda acotado a la **raíz de exportes autorizada** (§11).

## 2. Problema

Los agentes de código entienden un repositorio completo, pero le entregan al humano texto plano. Cuando el humano necesita **ver** la arquitectura, el ER de la base de datos o el flujo de una función, hoy el agente:

1. Genera Mermaid embebido en markdown (frágil, sin ventana dedicada, sin control visual), o
2. Escribe un archivo `.html`/`.mmd` y le pide al humano que lo abra (manual, sin validación).

No existe un canal **estándar** (MCP), **validado** (JSON schema) y **visual** (ventana propia) entre agente y humano. Y las soluciones basadas en Mermaid heredan sus límites: layout poco controlable, DSL frágil, render dependiente del ciclo de vida de una librería JS. Graphium resuelve el canal **y** el motor.

## 3. Objetivos y no-objetivos

### Objetivos

- **O1.** Ventana de escritorio dedicada que renderiza diagramas generados por agentes IA.
- **O2.** Contrato de intercambio **JSON versionado** con schema estricto: la IA nunca genera DSL de dibujo, solo estructura.
- **O3.** Servidor **MCP** embebido (stdio) que cualquier cliente MCP puede configurar; las herramientas informan a la IA cómo usar el formato (`get_diagram_schema`).
- **O4.** Bucle de autocorrección: JSON inválido → error con `code`, `path`, `message` y `hint` → la IA corrige en el turno siguiente, incluso sin renderizar (`validate_diagram`).
- **O5.** Binario único (Linux primero; macOS/Windows después), sin runtime externo.
- **O6.** **Motor propio sobre componentes open source**: render SVG, estilo y contrato son nuestros; el layout layered es una librería open source **aislada detrás de traits propios**. Cero Mermaid como motor, cero librerías JS de diagramas.
- **O7.** **Extensibilidad por diseño**: agregar un tipo de diagrama nuevo no toca el servidor MCP ni la app de ventana — solo su módulo de kind.
- **O8.** **Determinismo total**: mismo JSON → mismo SVG byte a byte en cualquier plataforma (fuente embebida + shaping Rust + reglas mecánicas §8.5).
- **O9.** **Recuperabilidad del agente**: el estado del agente (compaction, reinicio) nunca es excusa — `list_diagrams`/`get_diagram` devuelven lo que existe.
- **O10.** **Onboarding sin fricción**: de binario descargado a primer diagrama sin leer más de una página.

### No-objetivos (v1)

- Editor visual manual (drag & drop, mover o redimensionar objetos del plano) — Graphium es *viewer*, no editor.
- Colaboración multiusuario / servidor remoto.
- Render del lado de la IA (la IA no ve píxeles; recibe confirmaciones y errores accionables).
- Importar formatos legacy (Visio, drawio).
- Diagramas 3D y **gráficos estadísticos** (pie, barras, líneas) — no son estructuras, son datos; para eso el export a texto. `gantt` y `gitgraph` sí entran, en F3.
- Superar la calidad de layout de motores con décadas de madurez en F1 — objetivo: claro y legible dentro de los límites (§7 RF20).

## 4. Usuarios

| Actor | Rol |
|---|---|
| **Humano** | Pide el diagrama en lenguaje natural; mira la ventana; cierra el diagrama que quiere (panel lateral) o la ventana entera, sin romper la sesión del agente; controla los colores de la grilla. |
| **Agente IA (cliente MCP)** | Genera el JSON del diagrama y llama las tools; valida en seco antes de renderizar; recupera estado con list/get. |
| **Graphium** | Valida, calcula layout, renderiza SVG, reporta errores accionables al agente. |

## 5. Flujo de uso (historias)

**H1 — Diagrama de función (graph):**
El humano escribe *"mostrame cómo funciona la función `parse_config`"*. El agente analiza el código, valida en seco (`validate_diagram`), llama `render_diagram` con un `kind: "graph"`. Graphium abre la ventana —o la enfoca si ya estaba abierta— y coloca el diagrama en el plano, en menos de un segundo.

**H2 — Diagrama de base de datos (er):**
El humano pide *"diagrama la base de datos"*. El agente inspecciona el schema/migraciones y llama `render_diagram` con `kind: "er"` (tablas, columnas, PK/FK, cardinalidad). En el plano aparece el ER con las tablas como cajas de columnas.

**H3 — Corrección automática:**
El agente manda un edge hacia un `id` inexistente. La tool responde `ok: false` con `errors: [{path: "edges[3].to", code: "unknown_ref", message: "unknown node id 'db2'", hint: "did you mean 'db'?"}]`. El agente corrige y reenvía (idealmente validado antes con `validate_diagram`, sin abrir nada).

**H4 — Exportación inline (F2):**
El humano pide pegar el diagrama en el README del repo. El agente llama `export_diagram(format: "markdown", inline: true)` y recibe el texto (bloque mermaid embebido en markdown) por el canal MCP — sin escribir un archivo. Para archivo: `export_diagram(format: "svg", path: "arquitectura.svg")` escribe solo dentro de la raíz de exportes autorizada.

**H5 — Recuperación tras compaction (F2):**
El agente perdió el `diagram_id` de una sesión larga. Llama `list_diagrams()`, ve los diagramas abiertos en el plano con títulos y kinds, y sigue con `update_diagram` sobre el id correcto.

**H6 — Onboarding (F1):**
El humano descarga el binario, corre `graphium mcp-config`, pega el JSON impreso en su cliente MCP y pide su primer diagrama. Sin leer nada más.

## 6. Catálogo de tipos de diagrama y fases

Priorizados por frecuencia de uso real en desarrollo de software. Cada kind declara su motor de layout (§8).

| # | Kind | Uso típico | Motor | Fase |
|---|---|---|---|---|
| 1 | `graph` | Flujo de lógica, arquitectura, dependencias, llamadas de función | layered | **F1** |
| 2 | `er` | Schema de BD: tablas, columnas, PK/FK, cardinalidad | layered | **F1** |
| 3 | `flowchart` | Proceso con decisiones (if/else, aprobaciones, loops) | layered | F2 |
| 4 | `sequence` | Interacciones temporales: cliente↔API↔servicios↔BD | lanes | F2 |
| 5 | `class` | OOP: clases, interfaces, herencia, composición | layered | F2 |
| 6 | `state` | Máquinas de estado: order, session, pipeline de CI | layered | F2 |
| 7 | `component` | Arquitectura de sistemas (estilo C4): containers, boundaries | layered | F2 |
| 8 | `dataflow` | Pipeline de datos, ETL, DFD de procesos | layered | F2 |
| 9 | `tree` | Árbol de archivos, jerarquía de módulos y directorios | tree | F2 |
| 10 | `mindmap` | Jerarquías, descomposición de features | tree | F2 |
| 11 | `timeline` | Roadmap, cronología de eventos | lanes | F2 |
| 12 | `swimlane` | Proceso con carriles (soporte, negocio, equipos) | lanes | F2 |
| 13 | `block` | Diagrama de bloques: cadena, hardware, tubería | layered | F3 |
| 14 | `api` | Mapa de endpoints, verbos, contratos y dependencias | layered | F3 |
| 15 | `trust` | Trust boundaries y threat model (STRIDE-lite) | layered | F3 |
| 16 | `activity` | Activity/BPMN-lite: acciones, gateways, eventos | lanes | F3 |
| 17 | `journey` | User journey: pasos, puntos de fricción, emociones | lanes | F3 |
| 18 | `deployment` | Topología de infra: servicios, redes, LBs, K8s | layered | F3 |
| 19 | `network` | Topología de red L3: hosts, subnets, firewalls | layered | F3 |
| 20 | `gantt` | Cronograma de proyecto, plan, dependencias | lanes | F3 |
| 21 | `gitgraph` | Historial de branches, merges, cherry-picks | git | F3 |
| 22 | `sankey` | Flujo con peso: tráfico, dinero, conversión | sankey | F3 |
| 23 | `orgchart` | Organigrama: equipos, responsables, reportes | tree | F3 |
| 24 | `neural` | Arquitectura de modelo: capas, tensores, loops | layered | F3 |

**Lectura:** de los **24 kinds**, **13 usan `layered`**, **6 usan `lanes`**, **3 usan `tree`** y **2 son especializados** (`sankey`, `gitgraph`). El motor implementa **5 motores de layout** — layered (Sugiyama vía `dagre` open source), lanes, tree, sankey y git — y **la IR no crece**: 13 kinds comparten `IrGraph`, 6 usan `IrTimeline` y los 2 especializados se distinguen por `layout_hint`. Agregar un kind es *schema + traducción a la IR*, no motor nuevo (O7).

**Fases:** F1 = 2 kinds (`graph`, `er`) · F2 = 10 kinds · F3 = 12 kinds. Ampliar el catálogo después **no mueve fases ya cerradas**: un kind nuevo entra en la fase que esté abierta, sin tocar las anteriores.

### 6.1 Con cuáles iniciamos

El MVP arranca con **dos kinds de los 24 del catálogo**, no con veinticuatro:

| Kind | Qué ve el humano | Tamaño del ejemplo mínimo |
|---|---|---|
| `graph` | Cajas con flechas: flujo de una función, dependencias entre módulos, arquitectura de un servicio. | 3 nodos + 2 flechas |
| `er` | Tablas con columnas, PK/FK y cardinalidad: el esquema de una base de datos. | 2 tablas + 1 relación `1:N` |

Ejemplo mínimo de `graph` (lo que la IA envía por MCP):

```json
{
  "version": 1,
  "kind": "graph",
  "title": "parse_config() — flujo",
  "direction": "TB",
  "nodes": [
    { "id": "read", "label": "read file" },
    { "id": "ok",   "label": "Config" }
  ],
  "edges": [
    { "from": "read", "to": "ok", "label": "utf-8" }
  ]
}
```

Ejemplo mínimo de `er`:

```json
{
  "version": 1,
  "kind": "er",
  "title": "Schema — posts",
  "tables": [
    { "name": "users", "columns": [ { "name": "id", "type": "uuid", "key": "pk" } ] },
    { "name": "posts", "columns": [ { "name": "id", "type": "uuid", "key": "pk" }, { "name": "user_id", "type": "uuid", "key": "fk" } ] }
  ],
  "relations": [
    {
      "from": { "table": "users", "column": "id" },
      "to":   { "table": "posts", "column": "user_id" },
      "cardinality": "1:N"
    }
  ]
}
```

Los otros **22** kinds del catálogo (§6) entran por fases: **10 en F2** y **12 en F3**. Cada uno es *schema + traducción al IR*: **no tocan el servidor MCP ni la app de ventana** (O7).

### 6.2 Anatomía de un kind (receta)

Un kind nuevo sale barato **si no pide shapes nuevas**. La receta es siempre la misma:

1. **Schema** en `gx-kinds`: struct serde con `deny_unknown_fields` + `schemars`.
2. **Semántica**: `validate()` propio (refs, unicidad, enums, límites de tamaño) → `ValidationError { path, code, message, hint }`.
3. **Traducción**: `to_ir()` → una de las dos IR (`IrGraph` con `layout_hint`, o `IrTimeline`).
4. **Shapes**: solo si el kind necesita formas que no existen (cylinder, diamond, hexagon, actor…). Si reusa las existentes, `gx-render-svg` no se toca.
5. **Un golden-SVG** (hash en CI) + un test de validación con su caso de error.
6. **Una línea** en el registro de kinds.

Ni `gx-mcp` ni `gx-app` cambian (O7). Esfuerzo típico: **1 jornada** si reusa shapes; **2–3 jornadas** si necesita shapes nuevas.

## 7. Requisitos funcionales

### MVP (F1)

- **RF1.** Tool MCP `render_diagram(diagram, opts)`: valida, calcula layout y **coloca el diagrama en el plano**; abre la ventana si está cerrada (sin matar sesiones — §8.6) y desplaza la cámara hasta el diagrama nuevo. Responde cuando el SVG está listo.
- **RF2.** Tool MCP `clear_canvas()`: elimina **todos los diagramas** del plano (no cierra la ventana ni mata la sesión MCP).
- **RF3.** Tool MCP `get_diagram_schema(kind?)`: devuelve el JSON Schema (obligatorio `kind`, o índice de kinds disponibles con tamaño; nunca los 24 schemas inline).
- **RF4.** Kinds `graph` y `er` operativos.
- **RF5.** Validación estricta con errores accionables: `ValidationError { path, code, message, hint? }`; `code` es un enum estable (`unknown_ref`, `duplicate_id`, `unknown_field`, `unsupported_version`, `too_large`, `invalid_enum`, `missing_field`, …); máx. 20 errores + `errors_truncated: true`. Campos desconocidos: **rechazados** (`unknown_field` con did-you-mean del campo más cercano). `version` no soportada → `unsupported_version` en `path: "version"` + `supported_versions` en la respuesta.
- **RF6.** La ventana es un **plano (canvas)**: fondo con grilla (§7.1), cámara con **zoom** (rueda) y **pan** (arrastrando el fondo), **fit-to-view** al enfocar un diagrama y estado vacío explícito ("esperando al agente…"). En F1 el plano abre un solo diagrama; los varios simultáneos llegan con RF26.
- **RF7.** Servidor MCP por **stdio** (configurable como `command` en cualquier cliente MCP).
- **RF19.** `graphium mcp-config [--client pi|claude|cursor|generic]` imprime el JSON de configuración listo para pegar; `QUICKSTART.md` de una página en el repo.
- **RF20.** Política de tamaño: límite duro por kind (`graph`: 300 nodos / 600 edges) → error `too_large` con conteo recibido y máximo; warning no bloqueante > 150 nodos (`warnings[]` en la respuesta de la tool).

### 7.1 El plano (canvas): fondo, grilla y colocación

La ventana no es "un diagrama": es un **plano** por el que el humano se desplaza, con los diagramas colocados encima como objetos. El fondo es una **grilla** tipo plano de dibujo.

- **RF24 — Fondo con grilla (F1).** Grilla de dos familias de líneas diagonales (±32°) que forman rombos, 1 px, sobre un color base. Defaults tomados de la referencia visual: base `#1b1b1b`, línea `#ffffff` a 14% de opacidad, separación 59 px, ángulo 32°. Presets: `plano` (oscuro, el de la referencia), `claro`, `sin grilla`.
- **RF25 — El usuario controla los colores (F1).** Panel de ajustes en la ventana (engranaje) con: color de fondo, color de grilla, opacidad, separación, ángulo y mostrar/ocultar grilla. Se persiste en `~/.graphium/settings.json`; **el agente no cambia estos ajustes** (el color es decisión del humano). El tema claro/oscuro (RF22) se apoya en el mismo archivo.
- **RF26 — Varios diagramas en el plano (F2).** Cada diagrama abierto es un objeto con posición. Panel lateral izquierdo con la lista de abiertos (título, kind, actualizado) y botón de cerrar por ítem; al elegir un ítem, la **cámara se desplaza hasta ese diagrama**.
- **RF27 — Colocación y foco (F2).** Un diagrama nuevo aparece **en el centro de la vista actual**; si ya hay otro diagrama ahí, se separa en cascada hasta encontrar hueco — **nunca se superponen**. Al crearse con el programa abierto, la cámara hace **pan hasta el diagrama nuevo**. `opts.focus: {id, depth?}` centra un nodo y opcionalmente su vecindad (depth N, default 1).
- **RF28 — Cerrar es cerrar un diagrama (F2).** Cerrar un diagrama (panel lateral o `×` sobre el objeto) lo saca del plano y **libera su slot**, no su identificador: el `diagram_id` queda **invalidado y nunca se reutiliza** (RF28.1, §11.1); cerrar **no** cierra la ventana ni mata la sesión MCP. Cerrar la ventana entera tampoco (§8.6).
- **Límite de alcance.** El humano **no** arrastra ni redimensiona diagramas (se mantiene el no-objetivo de editor visual, §3): solo pan, zoom, enfocar y cerrar.
- **La grilla es chrome del frontend**, no parte del SVG: los exports (RF10) salen limpios y el determinismo O8 no se ve afectado.
- **Presets de color.** F1 ofrece `plano` (oscuro, el de la referencia), `claro` y `sin grilla`. El preset `claro` **no** es solo un fondo blanco: el render parametriza la paleta (§8.5.2), de modo que nodos, labels y flechas mantienen contraste correcto en ambos temas.

### 7.2 Cámara, geometría y estados del plano

**Propiedad de la cámara.** La cámara **vive en el frontend**: `view = { tx, ty, zoom }` en píxeles de plano. Rust **nunca** escribe la cámara: emite intenciones (`place`, `focus`, `fit`) y el frontend las aplica. Rust guarda solo una **copia** (`CameraSnapshot`) que el frontend le empuja con el evento `camera-changed` (throttle 100 ms), para poder calcular "el centro de la vista actual". El mutex de §8.6 protege el modelo de diagramas en Rust, **no** la cámara.

**Unidades y transformación.**
- 1 unidad de plano = 1 px CSS con `zoom = 1`.
- Cada diagrama es un contenedor `div.diagram` con `transform: translate(tx,ty) scale(zoom)`; el `<svg>` interno lleva `width`/`height`/`viewBox` en unidades de plano y **nunca** se reescala por su cuenta (el zoom se aplica solo al contenedor) — así no hay doble escala.
- `zoom ∈ [0.05, 4.0]`; la rueda hace pasos ×1.1 anclados al cursor; por debajo de `zoom 0.3` la grilla se oculta (muaré) y los diagramas se ven como recuadros con título (culling, §7.3).
- `devicePixelRatio` afecta solo el **grosor** de la grilla (1 px de pantalla); el resto del plano escala en unidades CSS.
- Al redimensionar la ventana se conserva el punto de mundo que estaba bajo el centro del viewport; **no** se refitea.

**Geometría del objeto y cascada (complementa RF27).**
- El objeto de un diagrama es su SVG más **24 px de padding**, en coordenadas del plano.
- Se coloca con su **centro en el centro de la vista actual**, conservando el zoom del humano (nunca > 100% al insertar).
- Si su rectángulo se solapa con otro (±24 px), **cascada** con offset `(48, 48)` px hasta 8 pasos; al agotarse, columna nueva a la derecha del bounding box global + 48 px.
- Si el destino queda fuera del viewport → **pan animado de 250 ms** hasta él.
- `fit-to-view` usa 32 px de margen y **clampa en zoom 0.4**: por debajo no se achica más (la degradación y `extract` resuelven el resto).
- La separación de grilla (59 px) es del **espacio del plano** y **escala con el zoom**; el grosor de línea se fija en 1 px de pantalla.

**Estados visibles de la ventana.**
1. **Vacío** — «esperando al agente…» + la línea `graphium mcp-config` para el primer uso. Solo cuando no hay renders en curso ni diagramas abiertos.
2. **En curso** — con un `render_diagram` en vuelo, el punto de colocación muestra una **tarjeta fantasma** (320×180, borde punteado) con «calculando layout…»; pasados 2 s, «N nodos, M edges».
3. **Error** — banner fijo bajo la barra superior con `path` + `message` + `hint` del primer error y «+N errores» si hubo más; cada error nuevo **reemplaza** al anterior y el banner se cierra con ×. El último diagrama bueno **no** se toca.
4. **Headless** — si la respuesta es `rendered: false`, no se abre ventana y no hay banner.
5. **Sin diagramas** (tras cerrar el último) → vuelve al estado (1).

### 7.3 Escalabilidad: presupuestos, límites y escalera de degradación

El producto escala con reglas, no con buena voluntad. Esta sección fija los presupuestos que gobiernan RF20 y el costo de cada render.

**RF20-bis — Presupuesto de render y escalera de degradación (F1).**
- Todo render **mide y declara** su costo: `render_diagram` responde `metrics: { nodes, edges, layout_ms, render_ms, svg_bytes }`; `validate_diagram` responde `projected_svg_bytes` para que el agente sepa el costo **antes** de abrir nada.
- Ningún kind puede declarar un límite cuyo render p95 exceda **1 s** en la máquina de referencia del CI; el límite se recalcula si cambia la matriz RNF1-bis.
- **Entre el warning (> 150 nodos) y el límite duro, Graphium NO rechaza: degrada**, en este orden y sin preguntar:
  1. omite los labels de las edges;
  2. reduce el espaciado entre nodos al 70% del default;
  3. emite el texto sin kerning fino;
  4. si el SVG resultante excede **2 MB**, corta y responde `ok: false, code: "too_large", svg_bytes` con `hint: "usá opts.extract: {around, depth}"`.
- Lo que se degradó se reporta en `warnings[]` con `code: "degraded"` y el paso aplicado, para que el agente pueda decidir partir el diagrama.
- **`opts.extract: {around, depth?}` entra en F1** (default `depth: 1`, tope 300 nodos en el subgrafo): el subgrafo conserva el `diagram_id` del padre y se marca `extracted_from: "<id>@<depth>"`. Sin `extract`, un grafo de 400 nodos no tiene respuesta útil en el MVP (R7).
- Todo layout tiene deadline `opts.layout_timeout_ms` (default 2000 ms, techo 10 000 ms); al vencer responde `ok: false, code: "layout_timeout", metrics: { elapsed_ms }` y **el plano queda intacto**.

**Límites por kind (F1).** `graph`/`flowchart`/`component`/`deployment`/`network`: 300 nodos / 600 edges · `tree`/`mindmap`/`orgchart`: 500 nodos / 499 edges · `sequence`: 40 carriles × 200 eventos · `sankey`: 150 nodos / 300 flujos · `gantt`/`timeline`: 200 eventos. Razón máxima edges/nodo = **3.0**; por encima → `too_large` con `metric: "density"`.

**RNF1-bis — Matriz de layout (F0 la produce, F1 la gatea).** Se mide con 4 casos fijos por motor, no con un número suelto: `100n/150e` sin grupos · `100n/400e` sin grupos · `100n/150e` + 8 clusters · `300n/600e` sin grupos. Objetivo p95 en la máquina de referencia del CI (Linux x86_64, 4 vCPU, 8 GB, SSD): **< 500 ms** para los tres primeros y **< 1.5 s** para el de 300 nodos, con `DagreEngine` en frío (no hay caché de layout, por diseño determinista).

**RNF1-ter — Presupuesto de bytes del SVG.** Un SVG de `graph` pesa como máximo `900 B × nodos + 350 B × edges + 40 KB` de cabecera (300n/600e ≤ 520 KB). El canvas mantiene **como máximo 3 SVGs en el DOM**: los diagramas fuera del viewport con `zoom < 0.4` se sustituyen por un recuadro con título y tamaño, y se re-materializan al entrar en vista.

**RNF8 — Cola y reporte de concurrencia (F1).** Los renders se encolan en una cola FIFO de profundidad **8** por proceso; con la cola llena la tool responde `ok: false, code: "busy", queue_depth, retry_after_ms`. El layout corre como máximo **2 en paralelo** (`spawn_blocking`). El pan de cámara solo lo dispara el render que gana el foco: si otro render tomó la cámara después, la respuesta incluye `focus_lost: true, focused_by_session: "<sid>"` — el diagrama **está** en el plano pero **no** en foco.

**RNF9 — Vida del owner y cuotas (F0).** Al arrancar, si el socket existe y el `connect` falla, el proceso **roba** el socket (unlink + bind) y loguea `owner_takeover`. Si el owner muere, las sesiones MCP conectadas reciben en su siguiente tool call `ok: false, code: "owner_unavailable"` con `hint: "graphium se reinició; usá list_diagrams"`: los diagramas **no** sobreviven al owner (sin persistencia — decisión explícita). Máximo **8 sesiones MCP** por owner; la novena recibe `too_many_sessions`.

**RNF10 — Cota de memoria (F2).** El proceso mantiene como máximo **12 IRs + 12 LayoutGraphs + 3 SVG strings** vivos; al cerrar un diagrama se liberan los tres. El historial (RF14) es **undo/redo de un nivel** (2 IRs por diagrama). `--dump-last` cuenta como un diagrama más. Presupuesto total: **RSS < 250 MB** con 12 diagramas de 300 nodos abiertos, medido en CI (`/proc/self/status:VmRSS`); el build falla si se excede el 150%.

**RF10-bis — Límites de export (F2).** `inline` devuelve como máximo **1 MB** de texto; si el contenido excede → `ok: false, code: "too_large", size_bytes` con `hint: "usá path para escribir a archivo o opts.extract"`. `png` acepta `opts.scale` (default 1.0, techo 3.0) y `opts.max_px` (default 16 Mpx, techo 64 Mpx): si `ancho × alto × scale²` excede `max_px` → `too_large` con las dimensiones calculadas. Todo export tiene deadline de **5 s** → `export_timeout`. La raíz de exportes tiene tope de **200 MB** → `disk_quota_exceeded`.

**RF29 — Ciclo de vida del catálogo (F2).** El catálogo tiene **techo declarado: 32 kinds**; pasar de 32 exige reemplazar uno. Un kind nuevo entra solo con el paquete completo: schema, `validate` con su caso de error, `to_ir`, **2 goldens** (el mínimo y uno en el límite de RF20) y **1 test de degradación** en el límite; CI corre la suite por kind registrado y falla el build si un kind no tiene sus 4 artefactos. Un kind sin uso declarado durante **2 versiones menores** pasa a `deprecated: true`: sigue validando y renderizando, pero `get_diagram_schema` lo marca y `render_diagram` devuelve `warnings: [{code: "deprecated_kind"}]`.

### v1 (F2)

- **RF8.** Kinds `sequence`, `class`, `state`, `component`.
- **RF9.** Tool `update_diagram(diagram_id, patch)`: **RFC 7386 (JSON Merge Patch)** sobre el documento fuente; arrays (`nodes`, `edges`, `groups`, tablas) se reemplazan completos — merge por id no existe; reglas documentadas en el schema.
- **RF10.** Tool `export_diagram(diagram_id?, format, opts)`:
  - `inline: true` → devuelve texto por MCP: `markdown` (bloque mermaid dentro de markdown), `mermaid` (DSL puro), `svg`. **Cero acceso a disco.** Camino primario para pegar en READMEs.
  - `path: <relativo>` → escribe SOLO dentro de la raíz de exportes (`~/.graphium/exports/` o configurada): rechaza `..`, absolutos y symlinks; `png` rasteriza con `resvg`; sobrescribir exige `opts.overwrite: true`.
  - Nota de identidad: Mermaid aparece **solo como formato de export de texto** para interoperar con GitHub — nunca como motor (O6 se mantiene).
- **RF11.** Tool `list_diagrams()`: diagramas abiertos en el plano con `{diagram_id, title, kind, updated_at}`.
- **RF12.** Tool `get_diagram(diagram_id)`: devuelve el JSON fuente — recuperación post-compaction (O9).
- **RF13.** *(sustituido por RF26/RF27 — modelo plano)* Varios diagramas simultáneos en el plano: tope configurable (default 12) con error claro `too_many_diagrams`; **sin barra de pestañas**.
- **RF14.** Historial de la sesión (flechas en la ventana) — por diagrama.
- **RF15.** Grupos anidados (clusters) en el motor de grafo.
- **RF16.** **`emphasis: "highlight" | "muted"`** en nodos y edges (además de `style: dashed`): la IA puede destacar una ruta crítica o atenuar el ruido; el render lo lleva a clases CSS.

### v2 (F3)

- **RF17.** Kinds `mindmap`, `deployment`, `timeline`.
- **RF18.** Interactividad: click en nodo → evento a la tool (opt-in `opts.interactive`); hover highlight.
- **RF21.** Modo `watch`: re-render al cambiar un `.graphium.json`.
- **RF22.** Temas claro/oscuro (clases CSS sobre el mismo SVG). **Promovido a F1** (§8.5.2): el preset `claro` es un tema real desde el MVP.
- **RF23.** Extracción de subgrafo para grafos enormes: `opts.extract: {around: <id>, depth: n}` renderiza solo la vecindad (complemento de RF20). **Promovido a F1** (§7.3).

### No funcionales

- **RNF1.** Binario único < 15 MB; startup < 300 ms; render de 50 nodos < 1 s; layout de 100 nodos < 500 ms.
- **RNF1-quater — Presupuesto de binario (F0).** Presupuesto desagregado del binario en release Linux: app + Tauri **6 MB**, motores de layout **1.5 MB**, `gx-render-svg` **1.5 MB**, fuentes embebidas **3 MB**, `resvg`/PNG **1.5 MB**, margen **1.5 MB** = **15 MB**. La fuente embebida se **subsetea** al repertorio declarado (Latin-1 + Latin Ext-A); CJK queda fuera de v1 y se cubre con fallback de fuente del sistema. Medición: `cargo build --release` + `strip` + `du -b target/release/graphium` en CI, con fallo del build si supera **15 700 000 bytes**. `startup_ms` se mide como *time to first painted frame* del plano vacío, cold start, SSD, mediana de 3 corridas, < **300 ms**.
- **RNF2.** Solo dependencia de sistema: webkit2gtk (mensaje claro y accionable si falta).
- **RNF3.** Contrato versionado (`version` entero); rechazo explícito de versiones no soportadas con `supported_versions` (RF5).
- **RNF4.** Seguridad: sin ejecución de código del JSON, sin network externa; **escritura de Graphium limitada a `~/.graphium/`** (`settings.json` + `exports/`); **la IA solo escribe dentro de `~/.graphium/exports/`** (RF10); MCP stdio solamente.
- **RNF5.** Determinismo §8.5 (verificable por hash).
- **RNF6.** Toda la lógica de validación/layout/render testeable sin GUI (tests puros, snapshots).
- **RNF7.** Observabilidad: logs a stderr (stdout es JSON-RPC puro) con niveles (`--log-level`); `--dump-last` vuelca el IR + LayoutGraph del último render para reproducir bugs de layout.

## 8. Motor: arquitectura en capas

```
      IA (JSON)                                HUMANO (ventana)
          │                                          ▲
 ┌────────▼──────────┐                     ┌─────────┴──────────┐
 │ Capa 1: Contrato  │  schema serde+schemars por kind           │
 │ Capa 2: Semántica │  validate() por kind (refs, únicos, enums)│
 │ Capa 3: IR        │  IrGraph | IrTimeline (comunes a todos)   │
 │ Capa 4: LAYOUT    │  ⭐ gx-layout — open source aislado (dagre)│
 │ Capa 5: RENDER    │  ⭐ gx-render-svg — SVG generado en Rust   │
 └───────────────────┘                     └────────────────────┘
 ── Capa 6 (transversal): MCP (rmcp) + ventana (Tauri) ──
```

### 8.1-8.3 Contrato, semántica e IR

```rust
pub trait DiagramKind: Send + Sync {
    const KIND: &'static str;
    const LAYOUT_HINT: LayoutHint;
    const LIMITS: KindLimits;                      // nodos, edges, carriles, eventos, densidad
    type Input: JsonSchema + for<'de> Deserialize<'de>;   // deny_unknown_fields
    fn validate(&self, input: &Self::Input) -> Result<(), Vec<ValidationError>>;
    fn to_ir(&self, input: &Self::Input) -> Ir;
    fn schema() -> schemars::schema::RootSchema;
}

enum Ir { Graph(IrGraph), Timeline(IrTimeline) }
struct IrGraph  { direction: Direction, layout_hint: LayoutHint, nodes: Vec<IrNode>, edges: Vec<IrEdge>, groups: Vec<IrGroup> }
struct IrNode   { id, label, shape, emphasis, style, meta, size: Size }   // size lo calcula gx-render-svg
struct IrEdge   { from, to, label, emphasis, style, arrow }
struct IrTimeline { lanes: Vec<IrLane>, events: Vec<IrEvent> }
enum LayoutHint { Layered, Tree, Sankey, Git }   // lanes => IrTimeline
struct IrEvent  { id, lane, label, order, span: Option<Span>, kind_data: BTreeMap<&'static str, IrValue> }
```

- `get_diagram_schema` se autogenera con `schemars` desde `Self::Input` (con `deny_unknown_fields`): lo que la IA lee y lo que validamos son la misma definición.
- `gx-layout` no conoce kinds; `gx-kinds` no conoce layout.
- **La IR no crece por kind:** 13 kinds comparten `IrGraph` (con `layout_hint`), 6 usan `IrTimeline`, y los 2 especializados (`sankey`, `gitgraph`) son un `layout_hint` más. Ampliar el catálogo no agranda la superficie del motor.

### 8.3.1 Mapeo kind → IR (normativo)

| IR | Kinds | Notas |
|---|---|---|
| `IrGraph` + `LayoutHint::Layered` | `graph`, `er`, `flowchart`, `class`, `state`, `component`, `dataflow`, `block`, `api`, `trust`, `deployment`, `network`, `neural` | 13 kinds |
| `IrGraph` + `LayoutHint::Tree` | `tree`, `mindmap`, `orgchart` | 3 kinds |
| `IrGraph` + `LayoutHint::Sankey` / `::Git` | `sankey`, `gitgraph` | 2 kinds |
| `IrTimeline` (lanes) | `sequence`, `timeline`, `swimlane`, `activity`, `journey`, `gantt` | 6 kinds |

- **La IR no gana variantes**: sigue `Ir::Graph | Ir::Timeline`. Lo específico de cada kind viaja en `kind_data: BTreeMap<&'static str, IrValue>` (ordenado; nunca se renderiza directo): `sequence` → `{role, reply_to, activation}` · `gantt` → `{start, end, depends_on}` · `journey` → `{score}` · `activity` → `{gateway}`.
- Cada kind **documenta sus claves de `kind_data`** en `rules[]` de `get_diagram_schema` (§11.1).
- La resolución IR → motor es **una sola función**: `gx_core::layout_for(&Ir) -> Layout`. Ni `gx-mcp` ni `gx-app` hacen `match` sobre kinds.

### 8.4 Layout — open source aislado detrás de traits nuestros

- **Grafo dirigido:** crate `dagre` (port completo de dagre.js a Rust, Apache-2.0): cycle removal, network simplex, Brandes-Kopf, crossing minimization, **clusters**, 4 direcciones.
- **Aislamiento:** `trait LayoutEngine { fn layout(&self, g: &IrGraph) -> LayoutGraph }` con `DagreEngine` (F1). Reemplazos (vendor/fork, `rust-sugiyama`, propio) no tocan nada más.
- **Motores de layout (5) y qué kinds los usan:** `DagreEngine`/layered — 13 kinds (F1); `LanesEngine` — 6 kinds (F2); `TreeEngine` — 3 kinds (F2); `SankeyEngine` y `GitEngine` — 1 kind cada uno (F3). Solo los dos últimos obligan a motor nuevo; el resto reusa layered/lanes/tree.
- **Plan operativo dagre (riesgo de juventud):** pin de versión exacta en `Cargo.toml`; F0 incluye una **matriz de capacidades** verificada por test (clusters, TB/BT/LR/RL, labels en edges, self-loops, nodos desconectados); trigger de vendoring: bloqueo F1 sin fix upstream en 2 semanas → vendor al workspace con atribución NOTICE.
- **Timeline (F2):** motor propio de lanes (trivial frente al layered).

### 8.4.1 Métricas de texto y dimensionado

El layout necesita el **tamaño** de cada nodo, pero solo `gx-render-svg` puede medir texto (rustybuzz + fuente embebida). La costura:

```rust
pub trait TextMetrics: Send + Sync {
    fn measure(&self, text: &str, style: &TextStyle) -> Size;
}
```

- `gx-render-svg` implementa `TextMetrics` y es la **única** fuente de métricas.
- `gx-layout` **recibe** el medidor como parámetro (`DagreEngine::new(metrics)`); no depende de `gx-render-svg` y **no mide texto**.
- `to_ir()` calcula `IrNode.size` con ese medidor antes de llamar al layout; el render **no recalcula** tamaños, solo dibuja el `LayoutGraph` recibido.
- Test de contrato: ningún `<text>` excede su caja (property test sobre 200 labels, incluidos los de §6.1).

Esto cierra R6: la métrica Rust es la misma que declara el SVG porque hay **una sola** implementación de medición.

### 8.5 Render propio + reglas de determinismo (O8)

- **Texto y shaping:** fuente OFL embebida (`include_bytes!`); medición y shaping con **rustybuzz** (kerning, ligaduras, clusters, bidi) — la métrica Rust es la misma que declara el SVG, con soporte no-Latín desde el diseño. F3: opción de emitir texto como paths en SVG standalone exportado (portabilidad absoluta, sin fuentes del sistema).
- **Reglas mecánicas de determinismo:** floats serializados con precisión fija (2 decimales, redondeo half-even); **sin `HashMap` en el pipeline** (BTreeMap/índices ordenados); orden estable por `id` en toda iteración; nada de reloj, random ni hilos con orden observable.
- **Salida:** string SVG con clases CSS (temas/hover sin regenerar), shapes (box, rounded, cylinder, diamond, ellipse, note), flechas, labels.
- **Export:** `svg` = el string; `png` = `resvg`; `mermaid`/`markdown` = traducción IR→DSL de texto (interoperabilidad GitHub, no motor).
- **Fuente y tokens visuales (congelados en F0):** **Inter 400/600** (OFL, embebida, subset Latin-1 + Latin Ext-A, ~1.2 MB) · texto **14 px** · padding de nodo **12 px** · radio de esquina **6 px** · grosor de borde **1 px** · interlineado **1.25**. Estos valores quedan congelados porque los golden-SVG los comparan byte a byte: cambiarlos es un commit que actualiza todos los hashes.

### 8.5.1 Forma canónica y superficie no determinista

- **Un único formateador** `fmt_num(f64) -> String` (2 decimales, half-even, sin `-0`) usado en **todo** atributo numérico.
- Los ids del SVG se derivan del id de la IR (`g-<id>`, `m-<from>-<to>`), **nunca** de contadores.
- `<defs>` y los markers se emiten en **orden de id**; sin espacios finales, sin declaración XML, atributos en orden fijo.
- **Fuera del pipeline determinista** (nunca comparados por hash): cámara (pan/zoom), grilla y sus colores, panel lateral, banner de error, geometría de ventana, `--dump-last` y **PNG** (depende de la versión de `resvg`).
- O8 se verifica en **Linux x86_64** hasta que existan builds de macOS/Windows.

### 8.5.2 Paleta parametrizada (temas)

El render recibe una `Palette` activa; **no** lee el filesystem, el entorno ni la hora.

```rust
pub struct Palette { theme: Theme, bg: Color, grid: Color, node_fill: Color, node_stroke: Color,
                     node_text: Color, edge_stroke: Color, edge_text: Color, accent: Color }
pub enum Theme { Dark, Light }
```

- El SVG lleva **clases CSS por token** (`class="node-fill"`, `class="edge-stroke"`, …) y un bloque `<style>` con la **paleta resuelta**, para que el archivo exportado se vea igual fuera de la app; en el canvas el tema se aplica por variable CSS sobre esas clases.
- Presets F1: `plano` (dark, `#1b1b1b` + grilla blanca 14%) · `claro` (light) · `sin grilla` (dark sin grilla).
- El export `svg` incluye la paleta resuelta; el export `png` rasteriza con esa paleta.
- El preset `claro` es un **tema real**, no un fondo blanco: nodos, labels y flechas mantienen contraste (decision del usuario, 2026-11-03).

### 8.6 Proceso, concurrencia y ciclo de vida

- **Single instance por usuario:** socket local (`$XDG_RUNTIME_DIR/graphium.sock`). El primer proceso es dueño de la ventana. Un segundo servidor stdio (otro cliente MCP) se conecta al dueño y **reenvía** comandos; los diagramas se etiquetan por sesión — dos agentes pueden convivir sin dos ventanas.
- **Cerrar la ventana NO mata el proceso** mientras haya sesión(es) MCP viva(s): pasa a estado headless-ready y la próxima tool reabre la ventana. `stdin` EOF del último cliente → shutdown limpio.
- **Sin display (SSH/CI):** `render_diagram` responde `ok: true, rendered: false, reason: "headless"` — validación y export inline siguen funcionando. `--headless` explícito disponible.
- **Concurrencia de renders:** las tool calls se ejecutan async; el layout corre fuera del event loop (`spawn_blocking`); el canvas tiene single-writer (mutex) con política **last-writer-wins** por llegada — el PRD no promete ordering entre sesiones distintas.

### 8.6.1 Runtime, render round-trip y modos de arranque

**Runtime.** Un `tokio` multi-hilo (`rt-multi-thread`) es el **único** runtime; `tauri::async_runtime` lo reutiliza. El transporte stdio de `rmcp` corre en un **thread dedicado** con runtime `current-thread` propio (la lectura bloqueante de `stdin` vive fuera del loop GTK). Al arrancar, el proceso toma `stdout` en exclusiva y redirige el fd 1 de librerías a `/dev/null`: **stdout es JSON-RPC puro**, toda salida de diagnóstico va a stderr (RNF7). Test de CI: el stdout de una corrida completa parsea como líneas JSON-RPC sucesivas.

**Render round-trip (F0).**
1. Validar + IR + layout + SVG en `spawn_blocking`.
2. Si el webview no existe, crearlo y esperar `graphium://ready`.
3. Emitir `graphium://render { req_id, diagram_id, svg, place }`.
4. El frontend responde `render_ack { req_id, ok }` **después de dos `requestAnimationFrame`** (el SVG ya está pintado).
5. Timeout de **2000 ms** → `ok: true, rendered: false, reason: "ui_timeout"`.

Razones de `rendered: false`: `headless` · `no_window` · `ui_timeout` · `ui_error`.

**Modos de arranque.**
- `graphium --stdio` → modo MCP: abre el servidor stdio; EOF del último cliente → shutdown limpio (§8.6).
- `graphium` (sin argumentos, doble clic) → modo **solo-ui**: abre la ventana y el plano, **no** abre servidor MCP y **no** muere por EOF. Sirve para configurar colores antes de tener un cliente MCP.
- `graphium mcp-config [--client ...]` imprime `{"command": "<ruta absoluta del binario>", "args": ["--stdio"]}`.

**Estado en el frontend.** El **modelo de diagramas vive en Rust** (`DiagramStore`); el frontend no es fuente de verdad. Al crear o recrear el webview, Rust emite `graphium://state` con la lista completa `{diagram_id, title, kind, updated_at, pos, bbox, session_id}` y el panel lateral se construye **solo** desde ese evento. `list_diagrams` lee el mismo `DiagramStore`.

## 9. Estructura del programa (workspace)

```
graphium/
├── Cargo.toml                  # workspace raíz
├── crates/
│   ├── gx-core/                # IR, ValidationError (path+code+hint), trait DiagramKind, registry.
│   ├── gx-kinds/               # Un módulo por kind: schema serde (deny_unknown) + validate + to_ir.
│   ├── gx-layout/              # ⭐ LayoutEngine trait + DagreEngine (pin + capability tests) + TimelineEngine.
│   ├── gx-render-svg/          # ⭐ SVG propio: rustybuzz + fuente embebida; feature "png" (resvg); export mermaid/markdown.
│   ├── gx-mcp/                 # rmcp: tools, errores RFC-7386, validación, list/get. Testeable in-process.
│   └── gx-app/                 # Tauri: ventana, single-instance socket, eventos. Cáscara delgada.
│       └── frontend/           # JS vanilla: canvas+grilla + zoom/pan + panel lateral + insertar SVG + estados vacío-error. Cero librerías.
├── assets/fonts/               # TTF OFL embebidos (determinismo O8)
├── .github/workflows/          # CI: webkit2gtk image, tests, golden-SVG hash, cargo tree gate, builds deb/AppImage
└── docs/{PRD.md, QUICKSTART.md}
```

| Cambio | Crates tocados |
|---|---|
| **Kind nuevo** | `gx-kinds` (módulo + 1 línea de registro) |
| Mejora/cambio de layout | `gx-layout` (detrás del trait) |
| Estilo/shapes/temas/export | `gx-render-svg` |
| Tool MCP nueva | `gx-mcp` |
| UI (canvas/grilla, panel lateral, estados, gestos) | `gx-app/frontend` |

**`gx-mcp` y `gx-app` jamás cambian al agregar kinds** (O7).

### 9.1 Matriz de dependencias y puerta única del motor

| Crate | Puede depender de | Gate de CI |
|---|---|---|
| `gx-core` | — | `cargo tree -p gx-core` no lista `gx-kinds` (no hay ciclo registro ↔ IR) |
| `gx-kinds` | `gx-core` | `cargo tree -p gx-kinds` no lista `gx-layout` ni `gx-render-svg` (§8.1) |
| `gx-layout` | `gx-core` **y nada más** (recibe el `TextMetrics` inyectado, §8.4.1) | `cargo tree -p gx-layout` no lista `gx-kinds`, `gx-render-svg`, `gx-mcp`, `gx-app` |
| `gx-render-svg` | `gx-core` | `cargo tree -p gx-render-svg` no lista `gx-layout`; la única `impl TextMetrics` está acá (§8.4.1) |
| `gx-mcp` | `gx-core` + `gx-kinds` **solo vía `registry::for_kind(&str)`** | ningún `match` sobre strings de kind (`grep` de `"graph" \| "er" \| …` en CI) |
| `gx-app` | todo | `cargo tree -p gx-app` es el único árbol que puede listar `tauri` y `webkit2gtk` (RNF2) |

- El trait `DiagramKind` declara `LAYOUT_HINT`, `LIMITS` y `schema()`; ningún crate fuera de `gx-kinds` repite esos datos. El límite de `graph` (300 nodos / 600 edges) y su `layout_hint` existen una sola vez, en su módulo.
- **Puerta única del motor (RNF6).** `gx_core::engine::render_json(src: &str, opts: &RenderOpts) -> Result<RenderOutcome, Vec<ValidationError>>` con `RenderOutcome { svg, layout, bbox, metrics }`; `gx-mcp` y los tests usan **exactamente** esta función: ningún test de RNF6 necesita Tauri, webkit ni webview. La colocación se testea con `place(&mut Model, id, &CameraSnapshot) -> Rect`, función pura.
- **Texto del contrato, siempre escapado.** Todo texto que viene del JSON se emite **escapado** (`&` → `&amp;`, `<` → `&lt;`, `>` → `&gt;`, `"` → `&quot;`, `'` → `&#39;`) dentro de `<text>`; el SVG se inserta con `innerHTML` bajo CSP de Tauri `default-src 'self'` sin `unsafe-inline` para scripts. Test: un `label` con `</text><script>…` produce SVG válido (parsea sin error) y ningún nodo `<script>` en el DOM.

## 10. Contrato de intercambio (formato `graphium.json` v1)

Común a todos los `kind`: `version`, `kind`, `title`, `emphasis?` (heredable), `notes[]`. **Campos desconocidos rechazados** con did-you-mean.

### `kind: "graph"`

```json
{
  "version": 1,
  "kind": "graph",
  "title": "parse_config() — flujo",
  "direction": "TB",
  "nodes": [
    { "id": "cli",  "label": "CLI arg",   "shape": "box" },
    { "id": "read", "label": "read file", "shape": "box" },
    { "id": "ok",   "label": "Config",    "shape": "box", "emphasis": "highlight" },
    { "id": "err",  "label": "error",     "shape": "box", "emphasis": "muted" }
  ],
  "edges": [
    { "from": "cli", "to": "read" },
    { "from": "read", "to": "ok",  "label": "utf-8" },
    { "from": "read", "to": "err", "label": "IO fail", "style": "dashed" }
  ],
  "groups": [
    { "id": "io", "label": "I/O", "members": ["read"] }
  ]
}
```

### `kind: "er"`

```json
{
  "version": 1,
  "kind": "er",
  "title": "Schema — usuarios y posts",
  "tables": [
    {
      "name": "users",
      "columns": [
        { "name": "id",    "type": "uuid", "key": "pk" },
        { "name": "email", "type": "text" }
      ]
    },
    {
      "name": "posts",
      "columns": [
        { "name": "id",      "type": "uuid", "key": "pk" },
        { "name": "user_id", "type": "uuid", "key": "fk" }
      ]
    }
  ],
  "relations": [
    {
      "from": { "table": "users", "column": "id" },
      "to":   { "table": "posts", "column": "user_id" },
      "cardinality": "1:N"
    }
  ]
}
```

- **Reglas de integridad del `er`.** Todo `relations[].from.table` y `relations[].to.table` debe existir en `tables[]` (`unknown_ref`, con did-you-mean de tabla); toda `relations[].column` debe existir en esa tabla; toda tabla necesita al menos una columna con `key: "pk"` (`missing_field`, `path: "tables[i]"`).

(Schemas de `sequence`/`class`/`state`/`component`: mismo patrón, se congelan al iniciar F2.)

**Validación semántica (todas con `path` + `code`):** refs existentes (`unknown_ref`), ids únicos (`duplicate_id`), `cardinality ∈ {1:1, 1:N, N:M}` (`invalid_enum`), límites de tamaño (RF20, `too_large`).

### 10.1 Campos comunes y evolución del contrato

- **Campos comunes a todos los kinds:** `version` (**requerido**, entero) · `kind` (**requerido**) · `title` (**requerido**, 1–120 caracteres, visible en el panel lateral) · `description?` (string) · `notes?: [{ text: string, anchor?: string }]` (máx. 20; `anchor` = id de nodo o tabla; el render textual de las notas es F3+) · `emphasis?: "highlight" | "muted"` (heredable) · `direction?: "TB"|"BT"|"LR"|"RL"` (**solo** kinds con `IrGraph`; default `"TB"`; en `er` es `unknown_field`).
- **`rendered_kind`** = el `kind` efectivamente usado tras resolver el registry (hoy siempre igual al enviado; reservado para inferencia futura). `render_diagram` lo devuelve para que el agente sepa qué schema se aplicó.
- **Evolución del contrato (RF5-bis).** `version` es la versión del **documento**, no del kind. Agregar un campo opcional con default es **aditivo y NO sube `version`** (sigue en `1`). Un campo obligatorio nuevo, un cambio de tipo o el borrado de un campo **sí** suben `version`. Cada campo declara `"x-graphium-since": "<n>"`; `get_diagram_schema` devuelve `supported_versions` y, si el kind tiene versión propia, `fields_ignored_in_this_version[]`. Un campo opcional desconocido de un kind conocido se rechaza igual (`unknown_field`), pero el `hint` dice «campo agregado en la versión N; actualizá Graphium o quitá el campo» y la respuesta incluye `min_version_required: N`. Un `kind` que esta versión no implementa → `ok: false, code: "unknown_kind"` + `available_kinds[]`.

## 11. Herramientas MCP

| Tool | Input | Output |
|---|---|---|
| `render_diagram` | `diagram`, `opts: { focus?: {id, depth?}, extract?: {around, depth?}, replace_id?: string, interactive?: bool }` | `{ ok, diagram_id, rendered_kind, rendered: bool, replaced?, metrics: {nodes, edges, layout_ms, render_ms, svg_bytes}, errors?: [...], warnings?: [...] }` |
| `validate_diagram` | `diagram` | `{ ok, errors?, warnings?, projected_svg_bytes? }` — **dry-run**: nada se renderiza ni abre |
| `update_diagram` | `diagram_id`, `patch` (RFC 7386), `revision?` | `{ ok, errors?, applied: bool, revision }` |
| `clear_canvas` | `{ scope?: "sesion" \| "plano" }` | `{ ok, removed: n }` |
| `list_diagrams` | — | `{ ok, diagrams: [{diagram_id, title, kind, updated_at, session_id}] }` |
| `get_diagram` | `diagram_id` | `{ ok, diagram, revision }` |
| `export_diagram` | `diagram_id?`, `format: "markdown"\|"mermaid"\|"svg"\|"png"`, `opts: { inline?: bool, path?: string, overwrite?: bool, scale?: number, max_px?: number }` | inline → `{ ok, content }`; archivo → `{ ok, path }` (solo raíz autorizada) |
| `get_diagram_schema` | `kind?` (opcional) | sin `kind` → índice de kinds (§11.1); con `kind` → schema + `rules[]`/`shapes[]`/`limits`/`examples[]` + `supported_versions` |
| `get_events` | `{ since?, limit? }` | `{ ok, events: [...], next_cursor }` — F3 |

Cada tool declara su descripción en el MCP ("que la ia por medio de un mcp entienda como se usa"). Resultados: sin schemas gigantes inline (RF3), errores con tope (RF5).

### 11.1 Catálogo de kinds, códigos de error y alcance de sesión

- **`get_diagram_schema(kind?)`** — el parámetro es **opcional**. Sin `kind` → índice `{ ok, kinds: [{ kind, phase, engine, mermaid_export, limits, min_example, disponible }], supported_versions }`; el índice **nunca** incluye los schemas. Con `kind` → `{ ok, kind, schema, rules[], shapes[], required[], limits, examples[], supported_versions }`, donde `shapes[]` es el **subconjunto válido para ese kind** (no el enum global) y `rules[]` son las reglas cruzadas (refs, unicidad) escritas en texto. Kind desconocido → `ok: false, code: "unknown_kind"`, `available_kinds[]`, `hint` con la lista.
- **Regla de oro.** Toda restricción expresable en JSON Schema (enums cerrados, `required`, mínimos/máximos, patrones) vive en el **schema**, no en `validate()`. `validate()` queda para reglas **cruzadas** (`unknown_ref`, `duplicate_id`, límites de tamaño). `validate_diagram` ejecuta **exactamente** el mismo pipeline que `render_diagram` (registry → schema → `validate()`), sin layout ni render.
- **Códigos de error estables** (se agregan al enum de RF5): `unknown_kind`, `unknown_diagram`, `diagram_closed`, `too_many_diagrams`, `too_many_sessions`, `not_owner`, `ambiguous_diagram`, `busy`, `owner_unavailable`, `layout_timeout`, `export_timeout`, `export_unsupported_kind`, `export_path_outside_root`, `export_exists`, `disk_quota_exceeded`, `invalid_patch`, `kind_change_not_allowed`, `stale_revision`, `deprecated_kind`.
- **Ciclo de vida de `diagram_id` (RF28.1).** El id es opaco, **monótono y nunca se reutiliza** (`d-0001`, `d-0002`, …): cerrar un diagrama lo **invalida**, no lo devuelve al pool (lo que se libera es el slot del plano, no el identificador). Toda tool que reciba un id inválido, cerrado o inexistente responde `ok: false, code: "unknown_diagram"` con `known_ids[]` y `hint: "el diagrama no está en el plano (quizá lo cerró el humano); usá list_diagrams()"`. Cerrar **no borra** el documento fuente: queda en memoria hasta que se cierre la ventana o se llame `clear_canvas`; `list_diagrams` devuelve solo los abiertos. `export_diagram` sin `diagram_id`: si hay exactamente 1 abierto en la sesión → ese; si hay 0 → `unknown_diagram`; si hay ≥ 2 → `ambiguous_diagram` con `candidates[]`.
- **Alcance de sesión.** Cada diagrama guarda `session_id` (socket peer). Por defecto `list_diagrams` y `clear_canvas` operan **solo sobre la sesión emisora**; `clear_canvas({ scope: "plano" })` borra todo el plano. `get_diagram`/`update_diagram`/`export_diagram` sobre un diagrama de otra sesión → `not_owner` con `hint: "ese diagrama pertenece a otra sesión MCP"`. El tope de diagramas abiertos (12) es **por ventana**.
- **F1 (capacidad 1).** Un segundo `render_diagram` **crea un diagrama nuevo, cierra el anterior** y responde `{ ok, diagram_id, replaced: "<id anterior>" | null }`. Para actualizar en vez de acumular: `update_diagram` (F2) o `opts.replace_id` (si no existe → `unknown_diagram`).
- **`update_diagram` es transaccional (RF9.1).** El merge RFC 7386 se aplica sobre una **copia**, el resultado se valida con el mismo pipeline de `render_diagram` y **solo entonces** reemplaza el documento; si falla, el documento y el render previos quedan intactos y la respuesta es `{ ok: false, errors: [...], applied: false }`. Casos: `patch.kind` distinto del actual → `kind_change_not_allowed` · `null` sobre `version`, `kind`, `title` o sobre el array principal del kind (`nodes`/`tables`/…) → `missing_field` (RFC 7386 borra la clave) · patch que no es un objeto JSON → `invalid_patch`. Cada diagrama lleva una `revision` monótona: `render_diagram`, `get_diagram` y `update_diagram` la devuelven, y un patch con `revision` vieja → `stale_revision` con la `revision` actual.
- **`opts` de `render_diagram`:** `{ focus?: {id, depth?}, extract?: {around, depth?}, replace_id?: string, interactive?: bool }` — `extract` en F1 (RF20-bis), `interactive` en F3.
- **Entrega de eventos (F3, RF18).** Con `interactive: true`, los clicks y hovers se acumulan en una cola **por sesión** y se leen con `get_events({ since?, limit? })` → `{ ok, events: [{cursor, diagram_id, node_id, type, at}], next_cursor }` (lectura por *pull*, máx. 100 eventos por llamada). Sin `interactive`, la cola no crece.
- **Matriz de export por kind (RF10.1).** Cada kind declara `mermaid_export: "nativa" | "con_pérdida" | "no_soportada"`. **Nativa:** `graph`/`flowchart` → `flowchart`, `er` → `erDiagram`, `sequence` → `sequenceDiagram`, `class` → `classDiagram`, `state` → `stateDiagram`, `gantt` → `gantt`, `gitgraph` → `gitGraph`, `journey` → `journey`, `sankey` → `sankey-beta`, `timeline` → `timeline`, `mindmap` → `mindmap`. **Con pérdida:** se exporta y la respuesta trae `warnings: [{code: "export_lossy"}]`. **No soportada** (`neural`, `trust`, `network`, `api`, `deployment`, `dataflow`, `tree`, `orgchart`, `component`, `block`): `export_unsupported_kind` con `hint: "exportá en svg o png"`. `svg` y `png` valen para los 24 kinds; `png` **solo** con `path:`.

## 12. Roadmap

| Fase | Alcance | Criterio de salida |
|---|---|---|
| **F0 — Spike técnico** | Workspace + Tauri 2 + rmcp + mini motor end-to-end (layout trivial → SVG nuestro → ventana-plano con grilla) + **matriz de capacidades de `dagre` por test** + single-instance socket básico + **runtime tokio y render round-trip** (§8.6.1) + **frozen de fuente y tokens** (§8.5). | Desde Pi: pido un diagrama → la ventana abre con el SVG de nuestro código sobre el plano; dagre: go/no-go documentado; **matriz RNF1-bis medida**. |
| **F1 — MVP** | RF1–RF7, RF19–RF20, **RF20-bis** (presupuesto + escalera de degradación), RF22 (tema `claro` real), RF23 (`opts.extract`), RF24–RF25, **§7.2** (cámara, geometría, estados), §7.3 y §11.1: `graph`+`er`, validación con codes/hints/deny, `validate_diagram`, `mcp-config` + QUICKSTART, **ventana = plano con grilla + ajustes de color**, fit-to-view, estados vacío/error, **RNF1-bis**, **RNF1-ter**, **RNF8** (cola), **RNF9** (owner), **RNF1-quater** (presupuesto de binario), **`revision` monótona**. | H1, H2, H3, H6; snapshots deterministas; golden-SVG hash en CI; presupuestos de §7.3 y RNF1-quater medidos en CI. |
| **F2 — Formato completo** | RF8–RF16, **RF9.1** (update transaccional), **RF10.1** (matriz de export), **RF10-bis** (límites de export), RF26–RF28, **RNF10** (cota de memoria), **RF29** (ciclo de vida del catálogo: techo 32 kinds), **§11.1** (alcance de sesión): **10 kinds** (flowchart, sequence, class, state, component, dataflow, tree, mindmap, timeline, swimlane), engines `lanes` + `tree`, clusters, RFC 7386 transaccional, export completo (inline + sandbox + matriz), **varios diagramas en el plano + panel lateral + cierre por ítem**, list/get, emphasis, historial. | H4, H5; suite completa de validación/layout/render; RSS < 250 MB con 12 diagramas de 300 nodos. |
| **F3 — Interactividad y alcance** | RF17–RF18, RF21: **12 kinds** (block, api, trust, activity, journey, deployment, network, gantt, gitgraph, sankey, orgchart, neural), engines `sankey` + `git`, **`get_events`** + click→evento (RF18), watch (RF21), texto-a-path opcional (§8.5). | Spike click→tool; cola de eventos por sesión sin pérdida en 100 eventos. |

### 12.1 F0 — decisiones y orden de trabajo

F0 no construye el producto: **cierra decisiones**. Se resuelven en este orden porque cada una condiciona a las siguientes; las de **[producto]** las decide el usuario y las de **[ingeniería]** el spike.

1. **[ingeniería]** Trait `DiagramKind` real + contrato JSON real, no structs hardcodeados: el schema sale de `schemars` y `graph` es el primer kind que lo atraviesa entero.
2. **[ingeniería]** Runtime async: `tokio` multi-hilo como único runtime y qué thread posee el loop stdio (§8.6.1).
3. **[producto]** Superficie de CLI: qué corre `graphium` pelado (modo solo-ui, §8.6.1) y qué imprime exactamente `mcp-config`.
4. **[ingeniería]** «Layout trivial» = posiciones fijas o `dagre` real: **`dagre` real**, si no la matriz de capacidades no prueba nada.
5. **[ingeniería]** Crate `dagre` exacto y versión pineada en el workspace (§8.4).
6. **[ingeniería]** Contrato del SVG raíz: `viewBox`/`width`/`height` en unidades de plano, sin `rect` de fondo, sin declaración XML (§8.5.1).
7. **[producto]** Fuente embebida: familia, peso y ejes — **Inter 400/600** (OFL, `include_bytes!`, subset Latin-1 + Latin Ext-A).
8. **[producto]** Tokens visuales: **14 px**, padding **12**, radio **6**, borde **1 px**, interlineado **1.25** (§8.5).
9. **[ingeniería]** Origen del `LayoutGraph`: centros de nodo o top-left, normalizado a `(0,0)` antes de entrar al render.
10. **[producto]** Grilla: CSS gradient o canvas, qué mide «separación 59 px» (paso entre líneas o diagonal) y cómo escala con el zoom (§7.2).
11. **[producto]** Propiedad de la cámara: confirmar que el frontend es dueño de `view` y Rust solo guarda `CameraSnapshot` (§7.2).
12. **[ingeniería]** Protocolo del socket single-instance: framing, permisos `0600`, socket huérfano (takeover) y auth entre procesos (§8.6, RNF9).
13. **[ingeniería]** Razones de `rendered: false` disponibles en F0: `headless` · `no_window` · `ui_timeout` · `ui_error` (§8.6.1).
14. **[ingeniería]** `.github/workflows` en F0 o en F1 (el golden hash es F1): CI mínimo en F0 para la matriz de `dagre`, gate de `cargo tree` y tamaño del binario.
15. **[ingeniería]** Si F0 emite `diagram_id` y guarda store, o es render-and-forget: **store**, si no F1 lo reescribe.

## 13. Riesgos y mitigaciones

| # | Riesgo | Prob. | Impacto | Mitigación |
|---|---|---|---|---|
| R1 | La IA genera JSON inválido | Alta | Medio | Codes estables + hints + `validate_diagram` (dry-run) — el bucle converge sin ventanas de prueba (O4). |
| R2 | Calidad del layout en grafos densos | Baja | Alto | `dagre` maduro (network simplex + Brandes-Kopf + clusters); plan operativo 8.4 (pin, matriz F0, trigger de vendoring). |
| R3 | webkit2gtk ausente | Media | Medio | Documentado + mensaje accionable + imagen CI dedicada. |
| R4 | Fricción Tauri+rmcp en un proceso | Media | Alto | Spike F0 exacto; headless-ready y single-instance ya especificados (8.6). |
| R5 | Scope creep de kinds | Alta | Medio | Catálogo cerrado con fases (§6); kind nuevo solo con spec + ejemplos + tests. |
| R6 | Desync medición↔render de texto | Baja | Alto | rustybuzz (shaping completo, no-Latín incluido) + fuente embebida + property tests adversariales + smoke visual CI. |
| R7 | Grafos enormes degradan la UX | Media | Medio | RF20 (límites + warnings + error claro), RF23 extract (F3), fit-to-view. |
| R8 | Complejidad del single-instance/reenvío | Media | Medio | 8.6 especifica el modelo completo; F0 incluye el socket básico; fallback documentado: una ventana por sesión si el reenvío no llega a F1. |
| R9 | El plano se convierte en editor (arrastrar, redimensionar, selección múltiple) | Media | Medio | Alcance acotado explícito (§7.1): pan, zoom, enfocar y cerrar; sin arrastre de objetos. Si el usuario lo pide, es una feature aparte, no un efecto colateral. |
| R10 | El catálogo de kinds crece sin techo (hoy 24) | Media | Medio | Techo declarado de **32 kinds** (RF29); un kind nuevo exige el paquete completo de artefactos y CI corre la suite por kind registrado; deprecación explícita a las 2 versiones menores sin uso. |
| R11 | La fuente embebida desborda el presupuesto de 15 MB (CJK por ejemplo) | Media | Alto | Fuente **Inter 400/600** subseteada a Latin-1 + Latin Ext-A (~1.2 MB) y gate de tamaño en CI (RNF1-quater); CJK fuera de v1 con fallback declarado. |
| R12 | El foco de cámara salta entre sesiones y el agente cree que su diagrama no se ve | Media | Medio | `focus_lost` + `focused_by_session` en la respuesta (RNF8) y cola FIFO con profundidad 8; el diagrama queda en el plano aunque no esté en foco. |

## 14. Criterios de aceptación (MVP)

1. Cliente MCP (Pi) con Graphium configurado: pedir *"diagrama la base de datos"* termina en ventana abierta con ER legible.
2. JSON inválido → `ok: false` + `errors[].path/code/hint`; nunca crash ni ventana en blanco (banner en ventana con el último error; el último render bueno se conserva).
3. **Determinismo (O8):** mismo JSON → SVG idéntico byte a byte (workflow golden-SVG en CI).
4. Binario único; `--version`, `--help` y `mcp-config` correctos.
5. Render de 50 nodos < 1 s en hardware normal.
6. **(O7)** Kind stub de prueba tocando solo `gx-kinds` — compila y renderiza sin cambios en `gx-mcp`/`gx-app`.
7. Sin Mermaid-motor ni librerías JS de diagramas (`cargo tree` gate en CI; Mermaid solo como export de texto F2+).
8. **(O10)** Un humano con cliente MCP limpio llega a H1 solo con QUICKSTART.md y `mcp-config`.
9. Cerrar la ventana con sesión MCP viva no rompe la siguiente tool call (reabre).
10. Grafos > 300 nodos → `too_large` con conteo claro; nunca ventana congelada.
11. La ventana abre con grilla de rombos sobre fondo `#1b1b1b` (o el color que el usuario elija); la rueda hace zoom y el arrastre hace pan sobre el plano.
12. Cambiar el color de fondo o de grilla desde los ajustes se persiste entre ejecuciones y **no** afecta al SVG exportado (el export sale sin grilla).
13. **(F2)** Con dos diagramas abiertos, ninguno se superpone; al pedir un tercero la cámara se desplaza hasta él, elegirlo en el panel lateral centra la cámara en él, y cerrarlo desde el panel lo saca del plano sin romper la sesión MCP.
14. **(F1)** El plano muestra la grilla de rombos sobre el fondo elegido; rueda = zoom, arrastre = pan; el SVG exportado **no** lleva grilla.
15. **(F1)** Un diagrama de 250 nodos se renderiza **degradado** si supera el umbral, con `warnings[].code = "degraded"`; uno que supera 2 MB de SVG responde `too_large` con `svg_bytes` y `hint` de `opts.extract`.
16. **(F1)** `opts.extract: {around, depth}` renderiza solo la vecindad, conserva el `diagram_id` del padre y responde `extracted_from`.
17. **(F1)** Con el preset `claro`, nodos, labels y flechas mantienen contraste legible (tema real, no solo fondo).
18. **(F1)** `render_diagram` responde `metrics` con `layout_ms`, `render_ms` y `svg_bytes`; `validate_diagram` responde `projected_svg_bytes`.
19. **(F1)** Cerrar un diagrama desde la UI lo invalida: un `update_diagram` posterior sobre ese id responde `unknown_diagram` con `known_ids`, y **no** se reusa ese id para otro diagrama.
20. **(F1)** Con dos sesiones MCP, `clear_canvas` de una no borra los diagramas de la otra; una tool sobre un diagrama ajeno responde `not_owner`.

## 15. Decisiones cerradas (2026-11-03, usuario)

| Decisión | Resultado |
|---|---|
| Stack | **Rust + Tauri 2 + rmcp** (Plan B documentado: Go + Wails, mismo contrato) |
| Motor | **Propio sobre open source**: render SVG nuestro (rustybuzz + fuente embebida) + layout `dagre` aislado por trait. Sin Mermaid-motor, sin JS de diagramas |
| Estructura | Workspace de crates; kinds nuevos solo tocan `gx-kinds` |
| Catálogo | **24 kinds** priorizados (§6); MVP `graph` + `er`; ampliación posterior no mueve fases cerradas |
| Export a archivo | **F2**, sandboxeado a raíz autorizada; inline (texto por MCP) también F2 |
| SO inicial | **Linux** |
| Modelo de ventana | **Plano (canvas) infinito con grilla de rombos** + **panel lateral** con la lista de diagramas abiertos (reemplaza las pestañas de v1.4) |
| Colocación | Diagrama nuevo **al centro de la vista actual**, en cascada si el sitio está ocupado; la cámara hace pan hasta él |
| Edición | El humano **no** arrastra ni redimensiona diagramas: solo pan, zoom, enfocar y cerrar |
| Colores | Los controla el **humano** desde los ajustes de la ventana (`~/.graphium/settings.json`); el agente no los cambia |
| Fuente y tokens | **Inter 400/600** embebida y subseteada (Latin-1 + Latin Ext-A); 14 px, padding 12, radio 6, borde 1 px, interlineado 1.25 — congelados en F0 porque los golden-SVG los comparan byte a byte |
| Escalabilidad | Ante un diagrama grande, **degradar en escalones** (labels → espaciado → kerning → corte) y solo después rechazar con `too_large` |
| `extract` | `opts.extract` **entra en F1** (no F3): sin él no hay respuesta útil para un grafo de 400 nodos |
| Tema claro | El preset `claro` es un **tema real en F1** (el render parametriza la paleta), no un fondo blanco |
| Presupuestos | Escalabilidad documentada con números: límites por kind, matriz de layout, presupuesto de bytes del SVG, cola, cuotas de sesión, cota de memoria, límites de export (§7.3) |

## 16. Registro de revisión por pares (v1.4)

Tres revisores IA (producto/UX, arquitectura, contrato MCP) auditaron v1.3. Incorporado en v1.4: export inline + sandbox (P0), read-back `list/get_diagram` (P0), modelo de concurrencia/ciclo de vida/single-instance (P0), rustybuzz por ab_glyph (P0), RFC 7386 para patches (P0), taxonomía de errores con codes/hints/caps + deny unknown fields + negociación de versión + `validate_diagram` (P0), onboarding `mcp-config`+QUICKSTART (P1), política de tamaño (P1), plan operativo dagre (P1), `emphasis` (P1), CI/packaging (P1), reglas mecánicas de determinismo (P1). Postergado a backlog F3+: a11y completa, i18n de UI, geometría de ventana persistente, semántica renderizada de `notes[]`, `label_max_width`/wrap, badges/iconos/tooltips, progress/cancellation MCP, minimap, tauri-driver e2e.

---

## 17. Cambios v1.5 (2026-11-03, usuario)

| # | Cambio | Origen |
|---|---|---|
| 1 | El catálogo inicial queda explícito (§6.1): arrancamos con `graph` y `er`, con ejemplo mínimo de cada uno. | *"no veo los tipos de diagrama con los que iniciaremos"* |
| 2 | La ventana pasa a ser un **plano (canvas) con grilla de rombos** (§7.1, RF24). | *"cuando se abra la interfaz tenga un fondo pero solamente el fondo similar a este [referencia]"* |
| 3 | **El usuario controla los colores** de fondo y grilla desde ajustes de la ventana, persistidos (§7.1, RF25). | *"que el usuario pueda controlar sus colores"*, *"como una grilla"* |
| 4 | **Varios diagramas en el plano**, cada uno cerrable, con **panel lateral** para elegir y enfocar (§7.1, RF26–RF28). | *"tener varios diagramas que el usuario los pueda cerrar cuando quiera"* |
| 5 | Colocación: al **centro de la vista actual**, en cascada si hay uno ocupado, con pan automático al diagrama nuevo (RF27). | *"que se agreguen desde el medio sin colocarse uno sobre el otro"* |
| 6 | Elegir un diagrama en el **panel lateral** desplaza la cámara hasta él (RF26). | *"que el usuario pueda seleccionar en el panel lateral el diagrama que quiere y lo desplace a este"* |
| 7 | El agente, al crear un diagrama con el programa abierto, **desplaza al diagrama nuevo**. | *"si el programa está abierto me desplaze al diagrama nuevo"* |
| 8 | Sin arrastre de objetos: pan, zoom, enfocar y cerrar solamente. | *"Solo cerrar y enfocar"* |
| 9 | La grilla es chrome del frontend: los exports salen limpios y el determinismo O8 no se ve afectado. | Coherencia con §8.5 y RF10 |

*La imagen de referencia se usó solo para extraer el fondo y la grilla (base `#1b1b1b`, línea `#ffffff` 14%, separación 59 px, ángulo 32°); el diagrama de ejemplo de esa imagen no se toma como contenido ni como modelo de kinds.*

## 18. Cambios v1.6 (2026-11-03, usuario)

| # | Cambio | Origen |
|---|---|---|
| 1 | El catálogo pasa de **9 a 24 kinds**, ordenados por frecuencia real de pedido (§6): 2 en F1, 10 en F2, 12 en F3. | *"existen tipos diferentes de diagrama, por lo menos unos 20, los más comunes"* |
| 2 | Se explicita el **motor de layout de cada kind**: 13 `layered`, 6 `lanes`, 3 `tree`, 2 especializados (`sankey`, `gitgraph`) — **5 motores** en total. | Evitar que el catálogo prometa 24 motores. |
| 3 | La **IR no crece** por kind: `IrGraph` con `layout_hint` + `IrTimeline` (13 + 6 + 2). | O7 y R5. |
| 4 | Nueva **receta de kind** (§6.2): schema → validate → to_ir → shapes → golden-SVG → registro. Esfuerzo 1 jornada si reusa shapes, 2–3 si necesita shapes nuevas. | Que el catálogo sea una promesa creíble de costo. |
| 5 | `gantt` y `gitgraph` pasan a ser kinds del catálogo (F3); los **gráficos estadísticos** (pie, barras) quedan fuera del alcance. | Coherencia con §3. |

*El catálogo sigue priorizado: ampliarlo después **no mueve fases ya cerradas**; un kind nuevo entra en la fase abierta y no toca `gx-mcp` ni `gx-app` (O7).*

## 19. Backlog de la revisión por pares (v1.7) — P1/P2 abiertos

Una revisión de **4 agentes** (3 *gap reviews* — producto/UX, arquitectura y contrato MCP — más 1 veredicto de escalabilidad) produjo los hallazgos de esta tabla. Los **P0** ya están incorporados en v1.7 (§7.3, §9.1, §10.1, §11.1 y §20); lo que sigue son los **10 hallazgos P1/P2 que quedan abiertos**: cada uno trae su propuesta y su fase sugerida, y ninguno se implementa sin una decisión de producto.

| # | Hallazgo | Prioridad | Propuesta | Fase sugerida |
|---|---|---|---|---|
| 1 | No hay canal humano→agente cuando el diagrama está feo (el único camino es el chat del agente que lo abrió) | P1 | `Ctrl+C` sobre un diagrama enfocado copia su JSON fuente al portapapeles; la ventana no manda eventos al agente en v1 | F1 |
| 2 | RF14 «historial» no define qué es: versiones del documento o historial de cámara | P1 | Definir como navegación de versiones (cada render/update crea una versión, tope 20 por diagrama, solo lectura, indicador «v3/7»); el undo de un nivel ya está acotado en RNF10 | F2 |
| 3 | El panel lateral no distingue diagramas de sesiones distintas ni títulos repetidos | P1 | Título truncado a 40 chars con tooltip, kind legible, `updated_at` en HH:MM local, sufijo `#2`/`#3` por orden de apertura y agrupación por sesión en el panel | F2 |
| 4 | `clear_canvas` vacía el plano del humano sin aviso | P2 | Banner de 6 s «el agente limpió el plano (N diagramas)» con × para descartarlo | F1 |
| 5 | `notes[]` nunca llega al humano (el único campo donde el agente puede explicar el diagrama) | P2 | Panel inferior colapsable con las notas del diagrama enfocado e indicador «N notas» en el objeto | F3 |
| 6 | Onboarding no caminable de punta a punta (no dice de dónde se descarga, cómo se verifica ni qué frase darle al agente) | P1 | `mcp-config` imprime debajo la frase para el agente; `graphium --selftest` renderiza un graph de 3 nodos a stdout y sale 0; QUICKSTART empaquetado en `/usr/share/graphium/` | F1 |
| 7 | No hay forma de encuadrar todos los diagramas abiertos a la vez | P2 | `opts.fit: "all"` encuadra el conjunto; por debajo de zoom 0.3 los diagramas se muestran como recuadros con título | F2 |
| 8 | `startup_ms` y el resto de RNF1 no tienen método de medición declarado | P2 | Medir en build release sobre la máquina de referencia CI (4 vCPU, 8 GB, SSD), mediana de 3 corridas, startup = primer frame pintado | F1 |
| 9 | `version` del documento no distingue la versión de un kind concreto | P2 | Exponer `kind_version` por kind en `get_diagram_schema` y `since_version` por campo (parcialmente cubierto por §10.1) | F2 |
| 10 | No hay telemetría local para decidir la deprecación de un kind | P2 | Contar usos por kind en un archivo local (`~/.graphium/usage.json`), nunca enviado a ningún servidor | F3 |

## 20. Cambios v1.7 (2026-11-03, usuario)

| # | Cambio | Origen |
|---|---|---|
| 1 | Se incorporan los **P0** de la revisión por pares de 4 agentes (3 gap reviews + 1 veredicto de escalabilidad) | Revisión v1.7 |
| 2 | Escalabilidad **documentada con números**: límites por kind, matriz de layout, presupuesto de bytes del SVG, cola y backpressure, cuotas de sesión y de memoria, límites de export, techo de 32 kinds | «que evalúe la escalabilidad documentada del programa» |
| 3 | Decisiones de producto tomadas por el usuario: **Inter 400/600** + tokens neutros congelados · **`opts.extract` en F1** · **degradación en escalones** antes de rechazar · **preset `claro` como tema real en F1** | Preguntas al usuario |
| 4 | Se corrigen contradicciones: ejemplo `er` inválido en §10 · `get_diagram_schema` con `kind` opcional · ciclo de vida de `diagram_id` (nunca se reutiliza) · alcance de sesión de las tools · `update_diagram` transaccional · RNF4 vs `settings.json` | P0 de los reviews |
| 5 | Se especifican cámara, geometría del plano, estados visibles, runtime, render round-trip, forma canónica, paleta parametrizada, matriz de dependencias y mapeo kind→IR | P0 de los reviews |
| 6 | Quedan **10 hallazgos P1/P2** en backlog (§19) con su propuesta | P1/P2 de los reviews |

*Próximo paso: F0 — spike técnico. Requiere tu "arrancá".*
