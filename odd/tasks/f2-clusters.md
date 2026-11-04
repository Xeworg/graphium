# F2 — Clusters: grupos anidados en el motor de grafo

Corte de F2 (`docs/PRD.md` RF15). Hoy `kind: "component"` ya acepta `groups`
de **un nivel** (`crates/gx-kinds/src/component.rs`), el motor los layoutea
como nodos compuestos de dagre (`crates/gx-layout/src/lib.rs:84-90`) y el
renderer dibuja una caja por grupo (`crates/gx-render-svg/src/renderer.rs:857`).
RF15 pide **grupos anidados**; además el documento `kind: "graph"` del PRD ya
muestra `groups` y hoy se rechaza como `unknown_field`.

Criterio de salida: un documento con un grupo dentro de otro se valida, se
layoutea y se dibuja (caja externa detrás de la interna, etiqueta sangrada por
profundidad); los kinds que el PRD documenta con `groups` lo publican en
`get_diagram_schema`; suite, clippy y fmt verdes.

## Specs

- **S1** (PRD §7 RF15, verbatim): «**RF15.** Grupos anidados (clusters) en el motor de grafo.»
- **S2** (PRD §9, ejemplo de `kind: "graph"`, verbatim): «```json
  "groups": [
    { "id": "io", "label": "I/O", "members": ["read"] }
  ]
  ```» — el campo es `groups` con `members[]`; no se inventa un campo `clusters`.
- **S3** (PRD §9, verbatim): «struct IrGraph  { direction: Direction, layout_hint: LayoutHint, nodes: Vec<IrNode>, edges: Vec<IrEdge>, groups: Vec<IrGroup> }»
- **S4** (PRD §7 RF9, verbatim): «**RF9.** Tool `update_diagram(diagram_id, patch)`: **RFC 7386 (JSON Merge Patch)** sobre el documento fuente; arrays (`nodes`, `edges`, `groups`, tablas) se reemplazan completos — merge por id no existe; reglas documentadas en el schema.»
- **S5** (PRD §9, validación, verbatim): «**Validación semántica (todas con `path` + `code`):** refs existentes (`unknown_ref`), ids únicos (`duplicate_id`), `cardinality ∈ {1:1, 1:N, N:M}` (`invalid_enum`), límites de tamaño (RF20, `too_large`).»
- **S6** (PRD §8.4, motor, verbatim): «**Grafo dirigido:** crate `dagre` (port completo de dagre.js a Rust, Apache-2.0): cycle removal, network simplex, Brandes-Kopf, crossing minimization, **clusters**, 4 direcciones.»
- **S7** (PRD §7 RNF1-bis, verbatim): «**RNF1-bis — Matriz de layout (F0 la produce, F1 la gatea).** Se mide con 4 casos fijos por motor, no con un número suelto: `100n/150e` sin grupos · `100n/400e` sin grupos · `100n/150e` + 8 clusters · `300n/600e` sin grupos. Objetivo p95 en la máquina de referencia del CI (Linux x86_64, 4 vCPU, 8 GB, SSD): **< 500 ms** para los tres primeros y **< 1.5 s** para el de 300 nodos, con `DagreEngine` en frío (no hay caché de layout, por diseño determinista).»
- **S8** (PRD §10.1, evolución del contrato, verbatim): «Agregar un campo opcional con default es **aditivo y NO sube `version`** (sigue en `1`). Un campo obligatorio nuevo, un cambio de tipo o el borrado de un campo **sí** suben `version`.»
- **S9** (decisión de este corte, no cita del PRD — alcance de kinds): `groups` existe en `component` (ya) y se agrega a `graph`, `flow`, `state`, `class` y `dataflow`: son los kinds con documento `nodes` + `edges` y `LayoutHint::Layered`. `er` (su documento es `tables`), `tree`, `mindmap`, `sequence`, `timeline` y `swimlane` siguen rechazando `groups` con `unknown_field`, porque su motor (`tree`, `lanes`) no dibuja cajas de grupo (`crates/gx-layout/src/lib.rs:706`, `:952`).
- **S10** (decisión de este corte, no cita del PRD — ciclo de anidamiento): un grupo que se contiene a sí mismo, directa o transitivamente, se rechaza con `invalid_value` en `groups[i].members[j]`. No se agrega un código nuevo: el catálogo de códigos estables de RF5 (PRD §11.1) no tiene código de ciclo y agregarlo sería un cambio de contrato público.

### Reglas de validación que introduce el corte

Todas con `path` + `code`, sobre el helper compartido:

1. `groups[i].id` único entre grupos y **también** contra los ids de `nodes` (un grupo y un nodo no pueden compartir id: dagre usa el mismo espacio de nombres) → `duplicate_id`.
2. `groups[i].members` no vacío → `missing_field` (ya existe en `component`).
3. cada miembro resuelve a un `node` o a otro `group` → si no, `unknown_ref`.
4. un nodo pertenece a un solo grupo, y un grupo tiene un solo padre → `duplicate_id` (ya existe para nodos en `component`).
5. sin ciclos → `invalid_value` (S10).

## Tasks

| ID | Qué | Specs | Ruta | Commit |
|---|---|---|---|---|
| T1 | Validación y layout del anidamiento: `validate_groups` compartido en `gx-core`, `DagreEngine` encadena `set_parent` grupo→grupo, `LayoutGroup` gana `depth` | S1, S3, S5, S10 | inline | pending |
| T2 | Render anidado: cajas ordenadas por profundidad (externa detrás), etiqueta sangrada por profundidad, `drawing_bbox` incluye la caja externa | S1, S3 | inline | pending |
| T3 | `groups` en `kind: "graph"` (el ejemplo del PRD): schema, parseo, validación por el helper, `xs-graphium-since`/version sin subir | S2, S5, S8 | inline | pending |
| T4 | `groups` en `flow`, `state`, `class` y `dataflow` con la misma forma y el mismo helper | S2, S5, S8 | inline | pending |
| T5 | `component` pasa su validación de grupos al helper compartido y acepta anidamiento | S1, S5, S10 | inline | pending |
| T6 | Docs y evidencia: secciones de kind del PRD, `rules_of`/`min_example_of` en `contract.rs`, texto de `harness.rs`, golden de un cluster anidado, caso RNF1-bis, suite + clippy + fmt | S1, S4, S6, S7, S8 | inline | pending |
| T7 | Cierre: rama `feature/f2-clusters`, commits por unidad de trabajo, PR con cuerpo, merge sin tag | — | parent | pending |

Ruta: la cadena T1 → T2 → T3/T4 → T5 → T6 tiene dependencias, así que no hay
unidades independientes que justifiquen escritores en paralelo (regla de
Writer: solo por razón, no por cantidad de archivos). T3 y T4 son mecánicos una
vez que existe el helper de T1.

## Log

- **L1** (el humano, verbatim): «por donde quedamos» y, ante la pregunta de con
  qué corte seguir para cerrar F2, eligió «Clusters primero (RF15) — El ítem más
  pesado: grupos anidados desde los kinds hasta dagre y el render. Toca layout y
  SVG.»
- **L2** (evidencia de exploración, 2026-11-04): `IrGroup` es
  `{id, label: Option<String>, members: Vec<String>}` (`crates/gx-core/src/ir.rs:276-282`) y
  `component` es el único productor no vacío (`component.rs:202-215`); el resto
  escribe `groups: Vec::new()`. `DagreEngine` construye dagre con `compound: true`
  y hace `set_parent(member, group.id)` sólo para miembros que son nodos
  (`gx-layout/src/lib.rs:84-90`), y devuelve `LayoutGroup { id, label, bbox }`
  sin profundidad ni padre (`gx-layout/src/lib.rs:183-200`). El renderer dibuja
  `layer-groups` y ordena **por id** (`renderer.rs:244-248`, `:857-880`). El
  schema por kind lo genera `schemars` (`contract.rs:346`), así que sumar el
  campo al struct lo publica solo; hay que tocar a mano `rules_of`
  (`contract.rs:626-631`) y `min_example_of` (`contract.rs:422-424`).
- **L3** (corrección de estado, 2026-11-04): de lo que se creía pendiente en F2,
  `emphasis` y `update_diagram` **ya están** implementados; lo que falta del
  corte es `opts.replace_id`, clusters (este documento) e historial RF14.
