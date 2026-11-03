# F2 — Catálogo: los 10 kinds

Segundo corte de F2 (`docs/PRD.md` §12): el catálogo pasa de **2 kinds a 12**.
El corte elegido por el humano el 2026-11-03 fue «los 10 kinds nuevos», con la
advertencia implícita del PRD (R5): es el trabajo más grande de la fase, así
que entra por partes — motores primero, después kinds de a dos o tres.

Criterio de salida (PRD §12, fila F2, verbatim): «suite completa de
validación/layout/render; RSS < 250 MB con 12 diagramas de 300 nodos».

## Specs

- **S1** (PRD §6, catálogo — filas de F2, verbatim):
  - «`flowchart` | Proceso con decisiones (if/else, aprobaciones, loops) | layered | F2»
  - «`sequence` | Interacciones temporales: cliente↔API↔servicios↔BD | lanes | F2»
  - «`class` | OOP: clases, interfaces, herencia, composición | layered | F2»
  - «`state` | Máquinas de estado: order, session, pipeline de CI | layered | F2»
  - «`component` | Arquitectura de sistemas (estilo C4): containers, boundaries | layered | F2»
  - «`dataflow` | Pipeline de datos, ETL, DFD de procesos | layered | F2»
  - «`tree` | Árbol de archivos, jerarquía de módulos y directorios | tree | F2»
  - «`mindmap` | Jerarquías, descomposición de features | tree | F2»
  - «`timeline` | Roadmap, cronología de eventos | lanes | F2»
  - «`swimlane` | Proceso con carriles (soporte, negocio, equipos) | lanes | F2»
- **S2** (PRD §6, lectura, verbatim): «de los **24 kinds**, **13 usan `layered`**, **6 usan `lanes`**, **3 usan `tree`** y **2 son especializados** (`sankey`, `gitgraph`)» y «Agregar un kind es *schema + traducción a la IR*, no motor nuevo (O7)».
- **S3** (PRD §6.2, la receta, verbatim): «1. **Schema** en `gx-kinds`: struct serde con `deny_unknown_fields` + `schemars`. 2. **Semántica**: `validate()` propio (refs, unicidad, enums, límites de tamaño) → `ValidationError { path, code, message, hint }`. 3. **Traducción**: `to_ir()` → una de las dos IR (`IrGraph` con `layout_hint`, o `IrTimeline`). 4. **Shapes**: solo si el kind necesita formas que no existen (cylinder, diamond, hexagon, actor…). Si reusa las existentes, `gx-render-svg` no se toca. 5. **Un golden-SVG** (hash en CI) + un test de validación con su caso de error. 6. **Una línea** en el registro de kinds.» y «Ni `gx-mcp` ni `gx-app` cambian (O7).»
- **S4** (PRD §6, O7, verbatim): «Cada uno es *schema + traducción al IR*: **no tocan el servidor MCP ni la app de ventana** (O7).»
- **S5** (PRD §7, RF29, ciclo de vida del catálogo, verbatim de la fila F2): «**RF29** (ciclo de vida del catálogo: techo 32 kinds)» — un kind nuevo entra con su paquete completo de artefactos y el CI corre la suite por cada kind registrado.
- **S6** (PRD §12, fila F2, salida, verbatim): «suite completa de validación/layout/render; RSS < 250 MB con 12 diagramas de 300 nodos».
- **S7** (PRD §6.1, verbatim): «Los otros **22** kinds del catálogo (§6) entran por fases: **10 en F2** y **12 en F3**.»
- **S8** (PRD §8.4.1 / RNF6, ya vigente y aplicable a cada kind nuevo): todo kind atraviesa la **puerta única** `render_json` y se prueba sin Tauri ni webview.

Fuera de este corte (siguen en F2 pero son otro corte): export (RF10/10.1/10-bis), clusters, `emphasis`, historial, `opts.replace_id`, `opts.interactive`.

## Tasks

| ID | Tarea | Specs | Ruta | Commit |
|---|---|---|---|---|
| T1 | Feature doc del corte + mirror + lista de tareas | S1-S8 | inline | este PR |
| T2 | Motor `tree`: layout jerárquico propio (sin `dagre`), con tests de forma y determinismo | S2, S3 | inline | este PR — `TreeEngine`, `Layouts` (despacho por `layout_hint`), 8 tests en `tests/tree.rs`, 250 en verde |
| T3 | Kind `tree`: schema de árbol de archivos/módulos, `to_ir`, golden y caso de error | S1, S3, S4 | inline | este PR — 5 tests en `gx-kinds`, golden `tree-src`, verificado en la ventana |
| T4 | Kind `mindmap`: jerarquía de descomposición sobre el motor `tree` | S1, S3 | inline | este PR — contrato `topic`/`branches` con `emphasis`, 5 tests, golden `mindmap-feature` |
| T5 | Motor `lanes`: carriles horizontales con orden de columnas. Se parte en T5a (layout: `layout_timeline` en el trait, el door rutea `Ir::Timeline`, `LanesEngine`) y T5b (el renderer dibuja la timeline) | S2, S3 | inline | T5a — este PR: 7 tests en `tests/lanes.rs`, 267 en verde |
| T6 | Kind `sequence`: actores y mensajes en orden temporal (IR `Timeline`) | S1, S3 | inline | — |
| T7 | Kind `timeline` y kind `swimlane` sobre el motor `lanes` | S1, S3 | inline | — |
| T8 | Kinds `layered` que reusan las shapes de hoy: `flowchart`, `state`, `dataflow` | S1, S3, S4 | inline | — |
| T9 | Kinds `layered` con semántica propia: `class` (herencia/composición) y `component` (boundaries) | S1, S3 | inline | — |
| T10 | Cierre: suite por kind registrado en CI, RSS con 12 diagramas de 300 nodos, catálogo y harness al día | S5, S6 | inline | — |

## Log

- **L1** (2026-11-03, arranque del corte): el humano eligió «los 10 kinds nuevos del catálogo» como segundo corte de F2, sabiendo el costo y el riesgo de scope creep que marca el PRD (R5). Se parte en diez tareas: primero los dos motores que faltan (`tree`, `lanes`), después los kinds de a dos o tres.
