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
| T6 | Kind `sequence`: actores y mensajes en orden temporal. Va sobre `IrGraph` con `LayoutHint::Lanes` (los mensajes son aristas, no eventos: el contrato pide de quién a quién) | S1, S3 | inline | este PR — `LanesEngine` para grafos, 5 tests en `gx-kinds`, golden `sequence-order` |
| T7 | Kind `timeline` y kind `swimlane` sobre el motor `lanes` | S1, S3 | inline | PR de T7 — `timeline` con carriles opcionales y tramos (`end`), `swimlane` con carriles obligatorios; el renderer pinta bandas de carril, sus nombres y los eventos; 2 goldens nuevos, 20 tests nuevos, 298 en verde |
| T8 | Kinds `layered` que reusan las shapes de hoy: `flowchart`, `state`, `dataflow` | S1, S3, S4 | inline | `flowchart` en este PR: `gx-kinds::flow` (nodo con `role` → forma, aristas con etiqueta, reglas compartidas), golden `flowchart-approval`, 6 tests. `state` y `dataflow` siguen en ramas propias |
| T9 | Kinds `layered` con semántica propia: `class` (herencia/composición) y `component` (boundaries) | S1, S3 | inline | `class` (PR #22): roles `class`/`abstract`/`interface`/`enum`, `relation` que decide la línea (`association` por defecto, `extends`, `implements`, `composition`, `dependency`), la línea se nombra con su relación cuando no hay `label` (la asociación no, porque no afirma nada) y **la herencia no puede tener ciclos**; golden `class-shop`, 7 tests. `component` sigue |
| T10 | Cierre: suite por kind registrado en CI, RSS con 12 diagramas de 300 nodos, catálogo y harness al día | S5, S6 | inline | — |

## Log

- **L1** (2026-11-03, arranque del corte): el humano eligió «los 10 kinds nuevos del catálogo» como segundo corte de F2, sabiendo el costo y el riesgo de scope creep que marca el PRD (R5). Se parte en diez tareas: primero los dos motores que faltan (`tree`, `lanes`), después los kinds de a dos o tres.

- **L2** (2026-11-03, un defecto que trajo una duda del humano): mirando el diagrama de secuencia en la ventana preguntó por qué se veía «desarmado» mientras el SVG del cambio tenía un diagrama armado. Tenía razón dos veces. (a) Faltaban las **líneas de vida**: sin la vertical punteada bajo cada actor, cada mensaje flota y no se ve a qué columna pertenece; ahora el renderer las dibuja para el layout `lanes`, de la base del actor al fondo del dibujo, con su regla de tema. (b) El SVG del cambio estaba generado por **el motor equivocado**: `golden_svg.rs` y `end_to_end.rs` instalaban `DagreEngine` en vez de `Layouts`, así que los goldens de `tree`, `mindmap` y `sequence` eran dibujos de `dagre` y no del motor que el producto usa. El golden de `sequence` tenía las tres líneas de vida en la misma x y un ancho de 71 px: la prueba estaba comparando un dibujo que la ventana nunca muestra. Los dos harness ahora instalan el despachador.
- **L2** (2026-11-03, T7 implementado): `timeline` y `swimlane` entran por el motor `lanes` sobre `IrTimeline`. Contratos: `timeline` es una cronología con `lanes` **opcionales** (sin carriles la IR crea el carril `cronologia` y el documento no repite nada por evento), `order` opcional (por defecto la posición, así el array **puede** venir desordenado y la columna la decide el tiempo) y `end` opcional, que convierte el punto en un tramo; `swimlane` es un proceso con `lanes` **obligatorios** y `steps` que referencian un carril declarado, sin tramos. Los dos comparten `gx-kinds::lanes` (id de carril no vacío y único, `unknown_lane` con los carriles declarados en el `hint`, ids de evento derivados de la posición `event-N` / `step-N` para que el dibujo sea determinista).
- **L3** (2026-11-03, T7, renderer): hasta hoy `Ir::Timeline` cruzaba layout pero no se pintaba: el renderer dibujaba `IrGraph` y una timeline salía como un lienzo vacío con el bbox correcto. Ahora pinta `layer-lanes` (una banda `lane-band` por carril, con su nombre `lane-label` en la canaleta que el layout reserva) y los eventos como cajas con su texto, en orden de id y siempre por debajo de las cajas. Las bandas y su nombre necesitan regla en `frontend/style.css` porque la CSP del canvas bloquea el `<style>` del SVG (lo verifica `grid.rs` con el golden nuevo en la lista). Verificado con capturas headless sobre el fondo `#1b1b1b` del plano: `timeline-roadmap` y `swimlane-soporte` se leen con sus carriles y sus cajas.
- **L4** (2026-11-03, T7, defecto encontrado y arreglado): el índice de `get_diagram_schema` devolvía como «ejemplo mínimo» el de `graph` para `tree`, `mindmap` y `sequence`, así que el agente copiaba un documento de otro kind y el registro lo rechazaba. Se agregaron los ejemplos mínimos de los cinco kinds y un test (`every_kind_has_a_minimal_example_of_its_own`) que recorre el registro y exige que el ejemplo declare su propio `kind`. Es el tipo de defecto que S5 (paquete completo por kind) tenía que haber cazado antes: el test lo caza de ahora en más.
- **L5** (2026-11-03, T7, límite conocido): un `swimlane` no dibuja flechas entre pasos, porque `IrTimeline` no tiene aristas y el motor devuelve eventos en columnas: el proceso se lee por el orden de izquierda a derecha y por el carril, no por un lazo. Si el humano quiere el flujo dibujado, es un corte aparte (conectores sobre `IrTimeline`), no una corrección de T7.
- **L6** (2026-11-03, T7, rebanadas de revisión): T7 no entra en una sola unidad de revisión (1549 líneas autoradas, la más grande del corte), así que se commitea en tres, en este orden: `feat(gx-kinds)` los dos kinds y el catálogo (917), `feat(gx-render-svg)` el dibujo de la timeline y la hoja del canvas (272), `test(gx-app)` goldens, end-to-end y este documento (360). La suite completa queda en 298 tests en verde, clippy y fmt limpios.
- **L7** (2026-11-03, T8 arranca con `flowchart`): los tres kinds de T8 son `layered` y no piden formas nuevas, así que la diferencia es **vocabulario**: el `role` del nodo decide la forma (`start`/`end` → elipse, `process` → redondeado por defecto, `decision` → rombo, `data` → caja, `note` → nota) y la etiqueta de la arista es la condición del camino. Eso vive en `gx-kinds::flow`, compartido: `validate_flow` (id único y no vacío, rol admitido con la lista en el `hint`, referencias declaradas) y `to_graph` (rol → forma, etiqueta vacía no se dibuja). `state` y `dataflow` van a reusar el mismo módulo con su propia lista de roles, así que agregar el segundo kind no repite sesenta líneas de validación. Límite conocido, igual que en `graph`: la etiqueta de un rombo puede sobresalir un poco de la forma cuando es más ancha que el área interna; se ve en la captura del golden y no rompe la lectura.
- **L10** (2026-11-03, T9 rebanada A, `class`): `flow` se parte en piezas reusables (`validate_nodes`, `validate_refs`, `to_ir_nodes`) porque este kind tiene su propia arista —`relation` con estilo, flecha y la marca de *es un*— y no le servía la arista genérica. Dos decisiones de producto: (1) **la línea se nombra con su relación** cuando el documento no escribe `label`, porque sin formas UML (triángulo hueco, rombo) dos líneas iguales dirían cosas distintas y nadie sabría cuál es cuál; (2) **la herencia no puede tener ciclos** (solo `extends`/`implements`; una composición de ida y vuelta sí vale, que es cómo se modela un pedido y sus líneas). El error imprime el ciclo entero (`a → b → c → a`) y el `path` de la arista que lo cierra.
- **L11** (2026-11-03, revisión del PR #22 antes de mergear): hallazgo propio — el default de `relation` era `extends`, así que **una línea sin palabras afirmaba una herencia**. Corregido: el default es `association` (sólida, con flecha, sin etiqueta por defecto) y los cuatro vínculos con semántica (`extends`, `implements`, `composition`, `dependency`) se declaran. Los dos tests del ciclo ahora declaran `extends` de forma explícita, que es lo que el caso prueba; se agregó el test de la asociación sin etiqueta. La regla queda: **el default es el vínculo que menos afirma**.
