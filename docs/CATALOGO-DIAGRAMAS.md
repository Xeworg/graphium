# Catálogo de diagramas de Graphium — adaptado a lo que el binario tiene

**Estado:** borrador de trabajo, corte `f24-catalogo` (rama `feature/f24-catalogo`,
base `staging` @ dde879b).
**Origen:** adaptación del documento de referencia del dueño
(`graphium_catalogo_diagramas_y_arquitectura_propia.md`), que propone 21
diagramas. Ese documento es una **referencia**, no una especificación: este
documento lo traduce contra lo que este build ya registra, dice qué falta
exactamente y en qué orden se cierra.

---

## 1. Reglas de lectura

1. **Un diagrama del catálogo no es un kind.** La unidad de costo de Graphium es
   el *kind* (schema + traducción a la IR + formas + golden + registro, PRD
   §6.2). Un diagrama de la referencia se resuelve de una de tres maneras:
   **(a) capacidad de un kind que ya existe** (roles, campos, reglas nuevas),
   **(b) kind nuevo**, o **(c) fuera del catálogo**, con su razón escrita.
2. **Lo que el código hace manda sobre lo que la referencia propone.** Cada
   afirmación de "esto ya está" lleva evidencia `archivo:línea`. Lo que la
   referencia pide y el código no tiene se marca como faltante, aunque suene
   obvio.
3. **Ninguna conformidad se promete.** Cada entrada declara si Graphium dibuja
   *notación propia inspirada en X* o una *implementación conforme a X*. La
   regla viene del §2.1 del documento de referencia y se aplica acá en cada
   fila (§7).
4. **El techo de 32 kinds del PRD (RF29/R10) es un límite real.** La unión del
   catálogo de la referencia con el catálogo del PRD §6 lo supera si cada
   diagrama se convierte en un kind: por eso el §4 reconcilia las dos listas
   antes de prometer nada.

## 2. Lo que este build tiene hoy

Catorce kinds registrados (`crates/gx-kinds/src/lib.rs::register_all`), nueve
formas de nodo (`crates/gx-core/src/ir.rs`, `Shape::ALL`), dos estilos de línea
(`Style: Solid | Dashed`), un solo adorno de arista (`arrow: bool`) y cinco
motores de layout (`LayoutHint: Layered | Tree | Lanes | Sankey | Git`).

| Kind | Motor | Roles / tipos | Contrato propio | Fase |
|---|---|---|---|---|
| `graph` | layered | — (forma libre por nodo) | `nodes{id,label,shape,emphasis,style}`, `edges{from,to,label,emphasis,style,arrow}`, `groups` | F1 |
| `er` | layered | `pk`/`fk`/`pk_fk` por columna | `tables{name,label,columns{name,type,key,nullable,note}}`, `relations{from,to,cardinality}` | F1 |
| `flowchart` | layered | `start`, `end`, `process`, `decision`, `data`, `note` | `nodes{role}`, `edges`, `groups` | F2 |
| `sequence` | lanes | — | `participants{id,label}`, `messages{from,to,label,reply,activation}` | F2 |
| `class` | layered | `class`, `abstract`, `interface`, `enum` | `nodes{attributes[],methods[]}`, `edges{relation}` | F2 |
| `state` | layered | `initial` (Dot), `state`, `final` (RingDot) | `nodes{role}`, `edges`, `groups` | F2 |
| `component` | layered | `container`, `component`, `database`, `external` | `groups` = límites anidables, `edges` | F2 |
| `dataflow` | layered | `process`, `store`, `external` | `nodes{role}`, `edges`, `groups` | F2 |
| `tree` | tree | — | `root{name,note,children[]}` recursivo | F2 |
| `mindmap` | tree | — | `root{topic,emphasis,branches[]}` recursivo | F2 |
| `timeline` | lanes | `milestone` | `lanes`, `events{id,label,lane,order,end,role}` | F2 |
| `swimlane` | lanes | — | `lanes`, `steps{id,label,lane,order,connects[]}` | F2 |
| `c4` | layered | `person`/`system`/`container`/`component` según `level` | `level`, `elements{description,technology}`, `relations` | F3 |
| `gantt` | lanes | — | `lanes`, `tasks{id,label,lane,start,end,depends_on[]}` | F3 |

Lo compartido por todos: `version` opcional (=1), `title` de 1 a 120
caracteres, `groups` anidados en los kinds de grafo, `emphasis` por nodo y por
arista, límites por kind y una suite por kind registrado.

**Palanca que la referencia no conoce:** dos tools ya convierten código real en
documentos. `code_er` lee el DDL SQL de un proyecto y produce el documento `er`
(`crates/gx-index/src/lang/er_bridge.rs`); `code_diagram` convierte la vecindad
de un símbolo en un documento `graph`. Cualquier fila del catálogo que pueda
alimentarse de código vale más que una que solo se escribe a mano.

## 3. El catálogo adaptado (21 diagramas de la referencia)

Siete columnas: la fila de la referencia, el kind que la cubre hoy, el veredicto
y el trabajo que falta. `—` en "kind" significa que no hay kind que la cubra.

| # | Diagrama (referencia) | Kind hoy | Veredicto | Falta |
|---|---|---|---|---|
| 1 | Class Diagram | `class` | capacidad | agregación, multiplicidad/rol por extremo, clase asociación, adorno de arista (triángulo hueco, rombo) |
| 2 | Sequence Diagram | `sequence` | capacidad | fragmentos (alt/opt/loop/par), guardas, crear/destruir, orden explícito |
| 3 | Use Case Diagram | — | kind nuevo | actores, casos de uso, límite del sistema, «include»/«extend» |
| 4 | Activity Diagram | — | kind nuevo | nodo inicial/final, acción, decisión/merge, fork/join (forma nueva), particiones; **decidido: motor `layered`**, y el PRD ya lo dice (§6 fila 16 y §8.3.1) |
| 5 | State Transition Diagram | `state` | capacidad | estado compuesto, entry/exit/do, choice/history, evento[guarda]/acción |
| 6 | Component Diagram | `component` | capacidad | puertos, lollipop/socket, ensamblaje vs delegación, «subsystem» en la caja del límite |
| 7 | Deployment Diagram | — | kind nuevo | nodos/dispositivos, entornos de ejecución y artefactos; pide caja 3D e icono de documento (forma nueva) y vocabulario de grupo |
| 8 | ERD | `er` (+ `code_er`) | capacidad | entidad débil (forma nueva), relación con atributos/roles, clave compuesta, participación total/parcial |
| 9 | Relational Schema | `er` (+ `code_er`) | capacidad | UNIQUE, CHECK, DEFAULT, acciones referenciales, índices, PK/FK compuestas |
| 10 | Document Database Diagram | — | kind nuevo | anidamiento de documentos dentro de una colección; `IrNode` tiene filas, no hijos |
| 11 | Normalization Diagram | capacidad de `er` | capacidad | dependencia funcional como arista de conjunto a conjunto; el cálculo de la forma normal no es un kind |
| 12 | System Component Map | `component` / `c4` | capacidad | tipos de componente y tipos de relación semánticos del §5 de la referencia; hoy las aristas solo llevan texto |
| 13 | Hierarchical Diagram | `tree` / `mindmap` | capacidad | metadatos por nodo (canal sin usar), niveles como dato; las agrupaciones no: el motor de árbol no dibuja clusters |
| 14 | Timeline Diagram | `timeline` | casi hecho | dependencias entre eventos (el eje, los intervalos y el hito ya existen) |
| 15 | Flowchart | `flowchart` | capacidad | paralogramo de entrada/salida (hoy `data` es una caja lisa, glifo equivocado), conectores y el resto de los símbolos ISO: la factura de formas más grande del catálogo |
| 16 | Data Flow Diagram | `dataflow` | capacidad | número de proceso y niveles; el nombre del dato en el flujo y las dos reglas de flujo ilegal ya existen |
| 17 | Workflow Diagram | `swimlane` | capacidad | rol de decisión, condición en `connects`, dueño por carril, entradas y resultados |
| 18 | Functional Decomposition | `tree` | capacidad | nivel e identificador como dato: el mismo canal `meta` que falta en la fila 13 (nunca IDEF0) |
| 19 | Nassi-Shneiderman | — | kind nuevo + motor | bloques anidados sin aristas: el único de los 21 que pide motor nuevo |
| 20 | Project Schedule | `gantt` | capacidad | progreso, hito de duración cero con glifo, tipos de dependencia FS/SS/FF/SF, guardia de ciclos |
| 21 | PERT | — | kind nuevo (barato) | estimación de tres puntos, ES/EF/LS/LF, holgura y camino crítico: layered lo hospeda, no pide forma ni motor |

Borrador: los "falta" de esta tabla los confirman (o corrigen) los informes de
los seis agentes de investigación, con la fuente oficial al lado. La tabla final
del §5 lleva la evidencia `archivo:línea` de cada faltante.

## 4. Reconciliación con el catálogo del PRD §6

El PRD ya tiene su propia lista, con once filas sin construir: `block` (#13),
`api` (#14), `trust` (#15), `activity` (#16), `journey` (#17), `deployment`
(#18), `network` (#19), `gitgraph` (#21), `sankey` (#22), `orgchart` (#23) y
`neural` (#24) — más `c4`, que se construyó (`crates/gx-kinds/src/c4.rs`).

Las dos listas se solapan en **dos** filas: `activity` (referencia #4) y
`deployment` (referencia #7). La referencia aporta **diez** diagramas que el PRD
no tiene: casos de uso, esquema relacional, documento, normalización, mapa de
componentes del sistema, jerárquico, workflow, descomposición funcional,
Nassi-Shneiderman y PERT. El PRD aporta **nueve** que la referencia no tiene.

La unión cruda serían 35 kinds y el techo declarado es 32 (R10). Por eso el §3
no convierte diagramas en kinds por reflejo: lo que se puede expresar como
capacidad de un kind existente se queda como capacidad. Con esa regla, la
referencia suma **cuatro** kinds (`use_case`, `deployment`, `document_db`,
`nassi_shneiderman`) más una capacidad transversal de `er` (normalización), y el
resto entra como roles, campos o reglas.

## 5. El faltante, diagrama por diagrama

Cada entrada: qué pide la fuente oficial, qué hay hoy con evidencia, qué falta
tagado (`[SHAPE]` glifo nuevo · `[CONTRACT]` campo/rol/relación nueva ·
`[LAYOUT]` motor o capacidad de motor · `[SEMANTICS]` regla de validación ·
`[DRAW]` solo dibujo) y cuánto cuesta.

### 5.1 Modelado de software

Las siete filas de esta categoría, en el orden de la referencia.

#### 1. Class Diagram → `class`

**Fuente:** UML 2.5.1 (`formal/2017-12-05`), §9 Classification, §11.4 Classes y
§11.5 Associations. Los números de sección se leyeron del índice del PDF
official; **los cuerpos de notación no se pudieron bajar** (429 del servidor de
OMG), así que los nombres de glifo de esta entrada quedan marcados como *no
verificados contra la prosa de OMG*: sirven de lista de trabajo, no de cita.

**Marca:** OMG permite implementar sus especificaciones («write software
implementations that conform to the specifications, and use them, give them
away, or sell them») sin licencia; sus marcas se pueden usar en una **referencia
fáctica** y nunca como nombre de producto ni en «UML certified» sin permiso
escrito (omg.org/gettingstarted/overview.htm, omg.org/legal/tm_guidelines.htm).
De ahí la línea del §7: la etiqueta «diagrama de clases» es descriptiva, el
producto no se llama UML y no se declara certificado.

**Hoy:** cuatro roles —`class`, `abstract`, `interface`, `enum`
(`crates/gx-kinds/src/class.rs:30`)—, con estereotipo escrito solo para
`abstract` e `interface` y explícitamente **no** para `enum`
(`class.rs:349`). Atributos y métodos son filas de la tabla compartida, con la
primera mitad separada por divisor (`class.rs:139`, `:363`) y el compartimento
dibujado por el renderer (`crates/gx-render-svg/src/renderer.rs:594`). Cinco
relaciones: `association`, `extends`, `implements`, `composition`, `dependency`
(`class.rs:74`); cada una aporta estilo, flecha y palabra, y **la arista se
queda solo con eso** (`class.rs:376`) — la semántica de la relación no llega a
la IR.

**Falta:**

- `[CONTRACT]` la agregación no existe en la tabla de relaciones
  (`class.rs:74`); hay composición y no hay agregación.
- `[SEMANTICS]` la composición es una **palabra**, no un todo/parte con rombo
  lleno: la arista lleva `arrow: true` y una etiqueta.
- `[CONTRACT]` los extremos de una asociación no llevan nombre de rol,
  multiplicidad ni navegabilidad (`class.rs:96`); la única cardinalidad del
  sistema es la de `er` y se dibuja pata de cuervo
  (`crates/gx-core/src/ir.rs:245`, `crates/gx-render-svg/src/renderer.rs:443`).
- `[DRAW]` no hay triángulo hueco (herencia), rombo hueco/lleno, flecha abierta
  ni círculo de interfaz: el renderer emite **un solo marcador**, un triángulo
  relleno, y su id solo varía por énfasis, no por tipo
  (`renderer.rs:409`, `:432`).
- `[CONTRACT]` no hay clase asociación, paquete ni plantilla.
- `[DRAW]` `enum` no lleva «enumeration» arriba.

**Costo:** formas se reusan; lo que hace falta es **adorno de arista** (cabeza y
cola en la IR más los dos pintores, Rust y el gemelo JS). Ni motor ni layout.

#### 2. Sequence Diagram → `sequence`

**Fuente:** UML 2.5.1 §17.1 (resumen, variantes de diagrama de interacción) y
§17.2 Interactions. El PDF oficial devolvió 429 dos veces: los números de
sección salen de pasajes del propio texto devueltos por búsqueda y del árbol de
capítulos, y las subcláusulas de clase quedan *no verificadas*.

**Marca:** igual que las filas anteriores: el kind se llama `sequence` y es
notación propia inspirada en UML; «UML Sequence Diagram» como implementación
conforme no se declara.

**Hoy:** los participantes son columnas y el orden del array de mensajes es el
orden en el tiempo (`crates/gx-kinds/src/sequence.rs:13`); una respuesta se
dibuja punteada (`:176`); la activación es un `bool` que viaja en `kind_data`
(`:44`, `:179`) y se pinta como barra delgada sobre la línea de vida
(`crates/gx-render-svg/src/renderer.rs:152`, ancho en
`crates/gx-render-svg/src/geometry.rs:26`).

**Falta:**

- `[CONTRACT]` fragmentos combinados: operador (`alt`, `opt`, `loop`, `par`,
`break`…) y guarda por operando. El contrato solo tiene participantes y
mensajes.
- `[CONTRACT]` clase de mensaje: `reply: bool` es todo el vocabulario; falta
síncrono, asíncrono, crear y destruir.
- `[DRAW]` cabeza abierta para asíncrono contra cabeza rellena: hay **un solo**
marcador (`renderer.rs:432`) y con `arrow: bool` no se distinguen.
- `[LAYOUT]` bandas de operando para el marco del fragmento: un mensaje es una
fila y nada agrupa filas (`crates/gx-layout/src/lib.rs:1199`).
- `[CONTRACT]` `ref`, puertas y mensajes perdidos/encontrados: hoy los dos
extremos tienen que ser participantes declarados.
- `[CONTRACT]` invariantes de estado, restricciones de duración y profundidad de
anidamiento de activaciones (la activación es un booleano).

**Costo:** lo que existe se reusa (motor `lanes`). Los fragmentos son marco más
etiqueta de operador (`[DRAW]`) más bandas verticales (`[LAYOUT]`), en los dos
emisores —incluido el gemelo `crates/gx-app/frontend/src/lib/scene/glyph.js`—;
las clases de mensaje son un campo de IR y una forma de marcador. Sin motor
nuevo.

#### 3. Use Case Diagram → kind nuevo `use_case`

**Fuente:** UML 2.5.1 §18.1.4 (notación), §18.2.1 Actor, §18.2.2 Extend, §18.2.3
ExtensionPoint, §18.2.4 Include; árbol de capítulos observado, subcláusula de
UseCase *no verificada*.

**Marca:** notación propia inspirada en UML; nada de «UML Use Case Diagram»
conforme.

**Hoy:** **nada.** No hay kind que lo cubra. Lo reutilizable: `Shape::Ellipse`
para el caso de uso, los `groups` para el límite del sistema (se dibujan como
cajas punteadas anidadas) y el estereotipo, que existe pero **solo en nodos y
aristas no** (`crates/gx-core/src/ir.rs:186`).

**Falta:**

- `[SHAPE]` el actor: ninguna de las nueve formas es una figura. Mitigación que
la propia notación admite: caja con estereotipo `actor` —y el canal ya se pinta.
- `[CONTRACT]` «include» y «extend» son relaciones, no etiquetas: sin `edge.kind`
son la misma línea con texto distinto.
- `[CONTRACT]` puntos de extensión: pertenecen al caso de uso, no a la etiqueta.
- `[SEMANTICS]` el sujeto contiene casos de uso y no actores: hoy un `group`
acepta cualquier miembro.
- `[DRAW]` dependencia punteada con cabeza abierta: el punteado existe, la
cabeza abierta no.

**Costo:** kind nuevo, barato: `layered`, `Ellipse` para el caso de uso, caja con
estereotipo para el actor, `groups` para el límite. `flowchart` con roles **no**
lo puede hospedar: el documento es un grafo bipartito actor/sujeto cuyas aristas
son include/extend, y ningún rol pone un actor afuera del límite.

#### 4. Activity Diagram → kind nuevo `activity`

**Fuente:** UML 2.5.1 §15.1, §15.2 Activity Edges (notación), §15.3 Control
Nodes (notación), §15.4 Object Nodes, §15.5 Executable Nodes, §15.6 Activity
Groups (particiones y regiones interrumpibles). A nivel de capítulo está
observado; el detalle de subcláusula no.

**Marca:** notación propia **inspirada** en el modelado de actividades de UML.
El PRD ya lo llama «Activity/BPMN-lite» (`docs/PRD.md:102`), que es la línea
correcta: el nombre de BPMN no se usa.

**Hoy:** no hay kind. Lo más cerca: `flowchart` (roles `start`, `end`,
`process`, `decision`, `data`, `note`; regla de exactamente un `start`;
`Diamond` para la decisión), `swimlane` (carriles y `connects`), los `groups`
para anidar y los glifos `Dot`/`RingDot` que ya usa `state`.

**Falta:**

- `[SHAPE]` la barra de fork/join: no existe como variante de `Shape`.
- `[SHAPE]`/`[DRAW]` final de actividad contra final de flujo: hoy hay un solo
`RingDot`.
- `[CONTRACT]` nodos de objeto, pines y nodos de parámetro: `IrNode` no tiene
modelo de entrada/salida.
- `[CONTRACT]` la guarda como campo, no como texto libre en la etiqueta.
- `[LAYOUT]`/`[SEMANTICS]` particiones con reglas propias y regiones
interrumpibles: los carriles de `swimlane` son pasivos y sus conectores van sin
etiqueta.
- `[DRAW]` cabezas de flecha abiertas: imposible con un solo marcador.
- **Decisión tomada (motor):** el PRD archivaba `activity` bajo `IrTimeline` y
  las particiones como clusters piden `layered`. Se resolvió a favor de
  `layered` y el PRD ya lo refleja: la fila 16 de §6 y el mapeo de §8.3.1
  movieron `activity` al grupo `IrGraph` y los conteos pasaron de 13/6 a 14/5.
  Razón: una partición anida, `LanesEngine` no dibuja grupos, y la guarda vive
  en la arista, que los carriles no etiquetan.

**Costo:** kind propio. `flowchart` con roles hospeda el inicio, la acción, la
decisión y el merge, y nada más: fork/join es una forma, la guarda es un campo y
los nodos de objeto son contrato. Es el kind más caro del bloque de
comportamiento.

#### 5. State Transition Diagram → `state`

**Fuente:** UML 2.5.1 §14.1 y §14.2 Behavior StateMachines (semántica y
notación, seudostados).

**Marca:** el PRD y el catálogo ya lo llaman «State Transition Diagram» y lo
tratan como capacidad, no como conformidad; esa es la línea.

**Hoy:** tres roles —`initial` es `Dot`, `state` es caja redondeada, `final` es
`RingDot` (`crates/gx-kinds/src/state.rs:29`)—, regla de un solo `initial`, y
anidamiento por `groups` (`:100`). La transición es una arista cuyo `label` es el
evento: no hay campos de disparador, guarda ni efecto.

**Falta:**

- `[SHAPE]` choice, junction, history, entry/exit points y terminate: `Shape` se
  queda en nueve variantes, y la «H» dentro de un círculo no se puede escribir
  porque el glifo `Circle` pone la etiqueta **al lado**, no adentro
(`crates/gx-render-svg/src/geometry.rs:141`).
- `[CONTRACT]` transición partida en disparador, `[guarda]` y `/efecto`.
- `[CONTRACT]` estado compuesto como objeto con su propio `initial` y sus
regiones: hoy es un `group` con miembros, sin semántica propia.
- `[LAYOUT]` regiones ortogonales (compartimentos paralelos de un mismo estado):
los clusters son de una sola banda.
- `[CONTRACT]` entry/exit/do: las filas se pueden reusar, pero no tienen tipo.
- `[DRAW]` transiciones internas y auto-transiciones: sin ruteo de lazo, solo
`arrow: bool` (`crates/gx-core/src/ir.rs:224`).

**Costo:** casi todo reuso —roles sobre el motor layered y campos nuevos—. Las
formas de history, choice y terminate piden los dos emisores y un golden nuevo.
Sin motor nuevo.

#### 6. Component Diagram → `component`

**Fuente:** UML 2.5.1 §11.6 Components, §11.3 Encapsulated Classifiers (puertos)
y §11.5 Associations (conectores de ensamblaje y delegación); mismo estado de
verificación que la fila 1.

**Marca:** igual que la fila 1: nombre descriptivo sí, producto o certificación
no.

**Hoy:** cuatro roles —`container`, `component`, `database`, `external`
(`crates/gx-kinds/src/component.rs:29`)— y solo `external` cambia algo más que la
forma: borde punteado por defecto (`:135`). Los límites son los `groups`
compartidos: anidan, un solo padre, sin ciclos
(`crates/gx-kinds/src/groups.rs:35`). Las aristas son el `FlowEdge` genérico,
sin tipo (`crates/gx-kinds/src/flow.rs:41`).

**Falta:**

- `[CONTRACT]` puertos: no hay campo en el nodo (`crates/gx-core/src/ir.rs:175`)
  y un `Role` es solo `{nombre, forma}` (`crates/gx-core/src/kind.rs:96`).
- `[SHAPE]`/`[DRAW]` ni lollipop (círculo con tallo) ni socket (semicírculo):
  `Shape` tiene nueve miembros y ninguno es esos (`ir.rs:45`).
- `[CONTRACT]` interfaz provista vs requerida y ensamblaje vs delegación: sin
  `edge.kind` las dos son la misma línea.
- `[SEMANTICS]` una interfaz solo se puede simular como caja redondeada; no hay
  rol `interface` en este kind.
- `[DRAW]` la caja de un límite no puede decir «subsystem»: `IrGroup` es
  `{id, label, members}` y no tiene estereotipo (`ir.rs:343`).
- `[LAYOUT]` el anidamiento se valida, pero solo los grupos hoja llegan a
dagre; las cajas de afuera se recalculan localmente (`crates/gx-layout/src/lib.rs:223`).

**Costo:** forma nueva (lollipop o puerto) **más** el trabajo de adorno de
arista de la etapa 0. Motor: ninguno, layered con `groups` ya hospeda.

#### 7. Deployment Diagram → kind nuevo `deployment`

**Fuente:** UML 2.5.1 §19 Deployments, §19.2 y §19.3 Artifacts; mismo estado de
verificación.

**Marca:** «diagrama de despliegue» es referencia fáctica; el PRD ya llama al
kind `deployment` y no «UML Deployment» (PRD §6 fila 18), que es la línea
correcta.

**Hoy:** **nada.** No está en `register_all` (`crates/gx-kinds/src/lib.rs:31`) y
la cadena «deployment» no aparece en `crates/` salvo en datos de prueba de
gx-index. El PRD lo tiene como fila 18, F3, motor `layered`, ya archivado bajo
`IrGraph`+`Layered`.

**Falta:**

- `[CONTRACT]` el kind entero: schema, `DiagramKind`, roles, validación.
- `[SHAPE]` caja 3D de nodo/dispositivo y el icono de documento del artefacto:
  `Shape` no tiene ninguno de los dos (`ir.rs:45`).
- `[CONTRACT]` un límite no puede decir **qué** es: «device» y
  «executionEnvironment» no tienen dónde vivir (`IrGroup`, `ir.rs:343`).
- `[DRAW]` el artefacto no se puede dibujar: no hay glifo de documento ni canal
de estereotipo en las cajas de grupo (el estereotipo existe solo en nodos,
  `ir.rs:188`).
- `[SEMANTICS]` «este artefacto corre en ese nodo» no tiene representación:
  las aristas no tienen tipo (`ir.rs:218`).
- `[LAYOUT]` el anidamiento de tres niveles: la profundidad real del motor no
  está verificada (solo los grupos hoja se miden con dagre).

**Costo:** kind nuevo del escalón barato del PRD §6.2 **más** al menos una
forma nueva (caja 3D) **más** vocabulario de grupo. Motor: ninguno, aunque la
profundidad de anidamiento hay que confirmarla antes de prometer tres niveles.

### 5.2 Bases de datos

#### 8. ERD → `er` (+ `code_er`)

**Fuente:** Chen 1976 (`doi.org/10.1145/320434.320440`; el texto completo no se pudo
leer, la notación se contrastó con el resumen enciclopédico) y la convención
pata de cuervo, que **no tiene especificación única**: hay variantes por
herramienta y un límite declarado en la más difundida (draw.io: «no indica
límites distintos de uno o cero»). **Marca:** «entidad-relación»/ERD es término
académico, libre; **no** se puede decir «notación de Chen» —se dibuja pata de
cuervo, que debe más a los diagramas de Bachman—, y ninguna conformidad es
posible donde no hay norma.

**Hoy:** `ErColumn{name,type,key,nullable,note}` (`crates/gx-kinds/src/er.rs:20`),
`ErTable`, `ErRelation{from,to,cardinality}` **columna a columna** (`:61`);
`key_of` produce `Pk`/`Fk`/`PkFk` (`:104`) y la validación cubre referencias,
unicidad y que el origen sea PK y el destino FK (`:113`). La IR dibuja cajas con
filas que llevan tipo y nota, arista sin flecha, anclada a la fila
(`:259`), y la cardinalidad sale de `1:N` más la nulabilidad del destino. El
puente SQL (`crates/gx-index/src/lang/er_bridge.rs:110`) arma el documento
desde el DDL real, exige **exactamente una** columna PK para anclar
(`:216`) y salta FK compuestas o contra una no-PK (`:63`).

**Falta:**

- `[CONTRACT]` la relación como objeto de primera clase: atributos propios,
nombres de rol y relaciones n-arias. El rombo existe como forma, así que puede
ser un nodo.
- `[SHAPE]`+`[CONTRACT]` entidad débil y relación identificadora: no hay forma de
doble borde.
- `[CONTRACT]`+`[DRAW]` clave parcial: no hay subrayado punteado (la fila sabe
decir `row-key` y nada más).
- `[SEMANTICS]` participación total/parcial más allá del booleano `nullable`: el
mínimo solo puede ser 0 o 1 y solo se declara en el extremo destino
(`crates/gx-core/src/ir.rs:300`).
- `[CONTRACT]`+`[LAYOUT]` clave compuesta como unidad y FK compuesta: el ancla de
fila es **un solo** `u32` por extremo (`ir.rs:227`).
- `[CONTRACT]` nombres de rol e identificadora vs no identificadora.
- `[SEMANTICS]` niveles conceptual, lógico y físico: hoy el documento no los
distingue.

**Deriva documental, resuelta** (corte `f25-arreglos`, decisión del dueño): el
PRD exigía «toda tabla necesita al menos una columna con `key: \"pk\"`
(`missing_field`)» y `er.rs` nunca emitió ese error; y el PRD llamaba
`unknown_field` a `direction` en `er` cuando `er.rs:89` lo acepta con `LR` por
defecto (pruebas en `:561`). Las dos líneas se corrigieron a favor del código,
y la PK opcional ganó un aviso propio en vez de un error: una tabla sin clave
se dibuja y la respuesta trae `tables_without_key` diciendo qué no puede
anclar. La razón está escrita en `docs/PRD.md` §10: una tabla sin PK es legal
en una base real, así que el dibujo no se niega, avisa.

**Costo:** casi todo reuso —pata de cuervo, badges, anclaje de fila, punteado y
estereotipo ya están—. Entidad débil: una forma nueva. Clave y FK compuestas: un
campo nuevo (lista de anclas) que obliga a tocar layout, no a agregar motor.

#### 9. Relational Schema → `er` (+ `code_er`)

**Fuente:** ISO/IEC 9075 (SQL). La norma define el **lenguaje**, no una notación
de diagrama: no se leyó ninguna cláusula normativa y la ausencia de notación es
una inferencia marcada como *no verificada*. Las restricciones concretas se
contrastaron con la documentación de PostgreSQL, §5.5 Constraints (CHECK,
UNIQUE, PK, FK, DEFAULT). **Marca:** «SQL» como nombre de lenguaje es uso
descriptivo; «PostgreSQL» es marca registrada de la PostgreSQL Community
Association, y su política admite el uso nominativo («diagrama el esquema de
PostgreSQL»), nunca el nombre de producto.

**Hoy:** el tipo es texto libre en la nota de la fila (`er.rs:20`), la
nulabilidad se dibuja, y `pk`/`fk`/`pk_fk` son por columna (`er.rs:104`). El
detalle que el libro guarda del DDL es solo `name, type_name, not_null,
primary_key, references` (`crates/gx-index/src/lang/model.rs:104`) más las FK
(`:136`); el parser de SQL **descarta** UNIQUE, INDEX y CHECK con su razón
escrita (`crates/gx-index/src/lang/sql.rs:300`) y pierde el nombre de las
restricciones nombradas (`:74`).

**Falta:**

- `[CONTRACT]`+`[SEMANTICS]` UNIQUE, y CHECK (con `[DRAW]`: necesita
compartimento, que el divisor ya permite abrir).
- `[CONTRACT]` DEFAULT e identidad; acciones referenciales (`ON DELETE`,
`ON UPDATE`) y deferrabilidad.
- `[CONTRACT]`+`[DRAW]` índices como objetos del esquema.
- `[CONTRACT]`+`[LAYOUT]` PK y FK compuestas como conjuntos: el ancla de fila es
única y el puente las salta.
- `[CONTRACT]` nombres de restricción: limitación del grammar, no del kind.
- `[CONTRACT]` vistas y esquemas/namespaces; tipos estructurados y dominios.

**Costo:** el primer corte es reuso puro (badges extra y un compartimento para
restricciones); las claves compuestas piden el mismo campo nuevo que la fila 8.

#### 10. Document Database Diagram → kind nuevo `document_db`

**Fuente:** manual de MongoDB (documentos embebidos, referencias, referencias
de base de datos). Es una referencia **de facto**: no hay estándar y no se
declara conformidad con ninguna herramienta. «Base de datos documental» es
genérico y libre.

**Hoy:** **nada.** No hay kind ni lector de JSON/BSON en `gx-index`: solo SQL
lleva detalle de tablas. Lo más cerca para reusar es `er` (nodos con filas) y
`class` (estereotipo y divisor).

**Falta:**

- `[SHAPE]`+`[LAYOUT]` anidamiento real: un documento dentro de otro o un array
de documentos. `IrNode` tiene **filas**, no hijos, y el mapeo de filas es de un
solo nivel (`crates/gx-render-svg/src/geometry.rs:370`).
- `[CONTRACT]`+`[DRAW]` embebido vs referenciado (el punteado ya existe para el
segundo) y `_id` como clave implícita.
- `[DRAW]` marca de array y de tipo por campo.
- `[SEMANTICS]` resolución de referencias entre colecciones.

**Costo:** un MVP plano es reuso (colección = nodo, campos = filas, embebido =
arista «contains»). El anidamiento de verdad pide hijos en el nodo o una
convención de ruta con prefijo, y eso toca layout y render: es el segundo kind
más caro de la sección, después de Nassi-Shneiderman.

#### 11. Normalization Diagram → capacidad de `er`

**Fuente:** Codd 1970 (modelo relacional), 1971/1972 (2FN y 3FN) y Boyce y Codd
1974 (BCNF). **No existe notación gráfica estándar**: la literatura propone
notaciones y la práctica es una tabla más una lista de dependencias. Dato que
manda el diseño: decidir si una relación está en BCNF es **NP-completo**, así
que el diagrama tiene que aceptar dependencias **declaradas**, no inferirlas.

**Hoy:** nada específico. `er` da tablas, columnas y PK por columna, sin
conjunto de claves candidatas ni concepto de dependencia funcional; sus aristas
son FK de columna a columna.

**Falta:**

- `[CONTRACT]`+`[LAYOUT]` la dependencia funcional como arista de conjunto a
conjunto: el ancla múltiple no existe (`ir.rs:227`).
- `[CONTRACT]` claves candidatas como conjuntos.
- `[CONTRACT]` etiqueta de forma normal por tabla, dibujada gratis con el
  estereotipo que ya se pinta.
- `[DRAW]` violación resaltada, gratis con `Emphasis::{Highlight,Muted}`.
- `[SEMANTICS]` el cálculo de la forma normal: cierre de atributos y búsqueda de
claves, aceptando lo declarado y nunca adivinando.

**Costo:** «tablas + flechas de dependencia + estereotipo de forma normal +
violaciones resaltadas» es reuso salvo un campo (anclas múltiples). El
razonamiento automático **no es un kind**: es un módulo de análisis aparte, y
por eso no entra en el techo de 32.

### 5.3 Modelado de sistemas y notaciones propias

#### 12. System Component Map → capacidad de `component`

**Fuente:** no hay estándar. El pariente publicado más cercano es el diagrama de
contenedores de C4 (c4model.com/diagrams): cuatro abstracciones, siete tipos de
diagrama, notación independiente, **un solo alcance por diagrama** y el tipo de
cada elemento declarado explícitamente. ArchiMate 3.2 aporta el vocabulario de
relaciones (composición, agregación, asignación, realización, serving, acceso,
influencia, asociación, triggering, flujo, especialización). **Marca:** se puede
decir «System Component Map de Graphium» y «el pariente más cercano es el
diagrama de contenedores de C4»; no se puede decir «diagrama C4» —no tiene
persona ni nivel y agrega brokers e infraestructura— ni «ArchiMate».

**Hoy:** cuatro roles —`container`, `component`, `database`, `external`
(`crates/gx-kinds/src/component.rs:29`)— y solo `external` carga semántica más
allá de la forma: borde punteado (`:138`). Las relaciones son **texto libre**:
`FlowEdge` tiene `label` y nada más (`crates/gx-kinds/src/flow.rs:41`) y el
propio kind la describe como «el protocolo o la llamada» (`component.rs:155`);
`IrEdge` no tiene campo de tipo (`crates/gx-core/src/ir.rs:218`). Los límites
anidan, con un solo padre y sin ciclos (`crates/gx-kinds/src/groups.rs:88`).
Puertos: no existen; lo más cerca es el anclaje de fila (`ir.rs:226`). **El
tipo no se dibuja:** `IrNode.stereotype` existe y se pinta arriba de la etiqueta
(`ir.rs:184`, `crates/gx-render-svg/src/renderer.rs:820`) pero `component` nunca
lo llena; `c4` usa filas en su lugar (`crates/gx-kinds/src/c4.rs:270`).

**Falta:**

- `[CONTRACT]` los seis tipos de componente que faltan (`application`, `service`,
  `infrastructure`, `broker`, `storage`, `runtime`) como roles: una línea por rol,
  y el contrato y el schema se derivan solos de `roles()`.
- `[DRAW]` el tipo escrito arriba de la caja: se llena `stereotype`, que ya se
  pinta. No hace falta forma nueva: un broker se lee por su palabra, no por un
  glifo.
- `[CONTRACT]` **vocabulario de relación tipada**: `uses`, `calls`, `reads`,
  `writes`, `publishes`, `subscribes`, `depends_on`, `hosts`, `connects_to`.
  Esto no es un campo de `component`: hoy **ningún kind** tiene tipo de arista
  (solo `er` tiene cardinalidad y `sequence` tiene `reply`), así que es un
  concepto compartido nuevo: `edge.kind` opcional con vocabulario por kind, que
  validan los kinds y que el motor ignora para dibujar (el render ya decide
  estilo por rol).
- `[CONTRACT]` puertos: explícitamente **fuera** de este corte; la referencia
  misma los marca opcionales, y sin contrato de arista tipada no significan nada.
- `[LAYOUT]` ninguno: layered con clusters alcanza, y el tope de hijos por
  cluster ya está documentado (PRD §8.4).

**Costo:** barato para roles + estereotipo (dibujo y contrato derivados); medio
para el vocabulario de relación tipada, que es plataforma y no kind. Un kind
nuevo no aporta nada acá: el precedente `c4` se creó como segundo kind porque
**cambia el documento** (niveles), no porque `component` no diera abasto.

#### 13. Hierarchical Diagram → capacidad de `tree`

**Fuente:** no hay estándar con ese nombre. Vecinos: `Grouping` de ArchiMate
(«agrega o compone conceptos que pertenecen juntos») y la frase de C4 sobre
abstracciones jerárquicas. **Marca:** notación propia; comparación segura con
la composición/Grouping de ArchiMate, nunca «un diagrama ArchiMate».

**Hoy:** el documento de `tree` anida (`crates/gx-kinds/src/tree.rs:27`), la
profundidad tiene tope (`:101`) y se calcula en `walk` (`:59`) y después **se
tira** (`:167`); el `note` se funde con la etiqueta como segunda línea (`:173`)
y nunca llega como dato. Ni `TreeInput` (`:40`) ni `MindmapInput`
(`crates/gx-kinds/src/mindmap.rs:38`) aceptan `groups` ni `meta`; `mindmap` solo
lleva `emphasis` (`:29`). Los grafos sí anidan con `groups`, pero `IrGroup` es
solo id/etiqueta/miembros (`crates/gx-core/src/ir.rs:343`). Y el motor de árbol
**no dibuja clusters**: `TreeEngine` devuelve `groups: Vec::new()`
(`crates/gx-layout/src/lib.rs:1088`), mientras layered calcula cajas anidadas
(`:393`).

**Falta:**

- `[CONTRACT]` `meta` por nodo: el canal existe y hoy nadie lo usa
  (`IrNode.meta`, `ir.rs:181`, llega intacto a la escena, `scene.rs:251`).
- `[CONTRACT]` niveles como dato: `tree` ya los calcula y los descarta.
- `[SEMANTICS]` `tree`/`mindmap` rechazan `groups`, así que una agrupación
  independiente del parentesco no se puede expresar.
- `[LAYOUT]` bandas de nivel y clusters en el motor de árbol —o mover la
  jerarquía a layered, que cambia el dibujo.

**Costo:** `tree` + `meta` contesta niveles, etiquetas y metadatos como cambio
puro y aditivo (cero trabajo de renderer y de layout). Las **agrupaciones** no:
las contesta el motor de árbol (clusters) o no se contestan. Decisión: la fila
13 se cierra con metadatos y niveles; las agrupaciones quedan declaradas como
lo único que un kind nuevo (`hierarchy` sobre layered) compraría, y eso se
decide recién si aparece un caso real.

#### 14. Timeline Diagram → `timeline`, casi completo

**Fuente:** no hay estándar. La referencia de facto es el `timeline` de Mermaid
(`{period} : {event}`, períodos de texto, `section` agrupa períodos, una columna
por período, sin intervalos y sin dependencias); el `gantt` de Mermaid sí tiene
inicio/fin, hitos y dependencias, pero es un cronograma, no una cronología.
**Marca:** «cronología con eje ordinal»; no se puede decir «escala temporal» ni
«Gantt» —las columnas son de paso fijo 150 (`crates/gx-layout/src/lib.rs:1134`)
y las marcas imprimen el entero crudo (`crates/gx-render-svg/src/renderer.rs:179`)—.

**Hoy:** intervalos sí (`order` + `end` → span, `crates/gx-kinds/src/timeline.rs:49`,
`:204`); hitos sí, con `end` rechazado (`:29`, `:152`); marcas del eje una por
columna conocida con el valor `i64` (`gx-layout/src/lib.rs:1495`);
dependencias no: `timeline` fija `depends_on: Vec::new()` (`timeline.rs:220`);
categorías no: los carriles son filas, no grupos de columnas
(`crates/gx-kinds/src/lanes.rs:19`).

**Falta:** `[SEMANTICS]` el campo de dependencia (el patrón es `gantt.rs:47` y
el dibujo ya existe en `scene.rs:62`); `[CONTRACT]` unidad/formato de la marca;
`[LAYOUT]` escala proporcional en vez de paso ordinal; `[CONTRACT]` categorías
de columna (hoy el modelo no las tiene).

**Costo:** dependencias: un campo, validación y `to_ir`, un día, sin motor.
Escala proporcional: cambio real en `LanesEngine` (ordinal → proporcional) más
un modelo de eje con unidad y texto; sigue sin ser un motor nuevo. Categorías:
no hay modelo, es lo más caro de la fila.



### 5.4 Procesos y lógica

Los cinco diagramas describen un procedimiento: pasos, decisiones, responsables y datos que se mueven. Cuatro se resuelven como capacidad de un kind existente; el quinto pide kind y motor nuevos.

#### 15. Flowchart → `flowchart`

**Fuente:** ISO 5807:1985, norma **paga** y no comprada: la taxonomía sale de la referencia descriptiva de KymoStudio (`https://docs.kymo.studio/reference/flowchart-notation`, §6 a §9) y de `https://en.wikipedia.org/wiki/ISO_5807`, marcada como *no verificada contra el texto de la norma*. Símbolos: terminal (óvalo), proceso (rectángulo), decisión (rombo), entrada/salida (paralelogramo); conector de página (círculo con etiqueta) y fuera de página (pentágono); proceso predefinido (doble borde vertical), preparación (hexágono), documento (borde ondulado), documentos apilados, operación manual (trapecio), entrada manual (borde inclinado), datos almacenados (cilindro), pantalla (redondeado abierto) y demora (D). Convenciones: un solo terminal de inicio, la decisión es el único símbolo con varias salidas etiquetadas y la flecha es obligatoria cuando el flujo no baja ni va hacia la derecha.

**Hoy:** seis roles (`crates/gx-kinds/src/flowchart.rs:28`): `start` y `end` en elipse, `process` en caja redondeada, `decision` en rombo, `data` en caja y `note` en nota. `data` es un `Box` liso (`flowchart.rs:33`), glifo equivocado para la entrada/salida de ISO. La regla del inicio único ya existe (`flowchart.rs:101-129`). Los conectores no son nodos: `FlowNode` es `{id, label, role, emphasis, style}` (`flow.rs:22-36`) y las aristas son el `FlowEdge` compartido (`flow.rs:41-55`).

**Falta:**

- `[SHAPE]` paralelogramo, hexágono, trapecio, rectángulo inclinado, forma de D, documento, apilados, pantalla y pentágono: `Shape` tiene nueve miembros (`crates/gx-core/src/ir.rs:45`, `:70`) y el renderer pinta esos nueve (`crates/gx-render-svg/src/renderer.rs:487-495`).
- `[SHAPE]`/`[DRAW]` proceso predefinido: doble borde vertical, variante de dibujo de la caja y no un rol nuevo.
- `[CONTRACT]` rol de entrada/salida separado de `data`: el único rol de dato no distingue E/S de dato persistido (`flowchart.rs:28-35`).
- `[CONTRACT]` conectores: necesitan destino declarado y el nodo no tiene dónde apuntar; el flujo solo se corta con aristas (`flow.rs:41-55`).
- `[SEMANTICS]` la decisión debería ser el único nodo con varias salidas etiquetadas; hoy cualquiera admite N aristas y la etiqueta es opcional (`flow.rs:49`), sin regla que cuente salidas.
- `[SEMANTICS]` flecha obligatoria contra el sentido: `arrow` se puede apagar (`flow.rs:54`) y ninguna regla lo ata a la dirección.

**Costo:** la factura de formas más grande del catálogo: diez glifos o variantes, un rol de entrada/salida, dos roles de conector y tres reglas. El layout no cambia, salvo el conector: un flujo que salta de página no se dibuja como arista común.

#### 16. Data Flow Diagram → `dataflow`

**Fuente:** DFD de Yourdon/DeMarco y Gane-Sarson. No hay texto oficial gratuito de ninguna de las dos escuelas: la descripción se apoya en la literatura de notación, no en un documento citable. Cuatro clases de elemento: proceso, almacén, entidad externa y flujo etiquetado. Convenciones: numeración de procesos (P1, P2...), descomposición por niveles (contexto o nivel 0, después 1, 2...) y, en Gane-Sarson, glifos propios de proceso (caja redondeada con banda de número) y almacén (rectángulo abierto).

**Hoy:** tres roles (`crates/gx-kinds/src/dataflow.rs:30`): `process` en caja redondeada, `store` en cilindro y `external` en caja. Los flujos llevan el nombre del dato en la etiqueta (`dataflow.rs:53`) y las dos reglas de flujo ilegal existen (`dataflow.rs:8-12`): un depósito no alimenta a otro y una entidad externa no alimenta a otra. Los `groups` anidados permiten cajas visuales dentro del dibujo.

**Falta:**

- `[CONTRACT]` número de proceso (P1, P2): `FlowNode` no tiene campo de número (`crates/gx-kinds/src/flow.rs:22-36`); hoy va escrito a mano en la etiqueta y ninguna regla lo revisa.
- `[CONTRACT]` nivel: `DataflowInput` no tiene campo (`dataflow.rs:39-59`), así que contexto, nivel 0 y nivel 1 son documentos sin relación declarada; `groups` es una caja visual dentro de un dibujo, no una jerarquía entre dibujos (`crates/gx-core/src/ir.rs:343-347`).
- `[SHAPE]` variante de almacén de Gane-Sarson (rectángulo abierto): `store` fija cilindro y no hay segundo glifo (`dataflow.rs:30-34`); el renderer tiene una sola rama de cilindro (`renderer.rs:489`).
- `[DRAW]` banda del número de Gane-Sarson: la caja redondeada es lisa y el número solo puede ir en la etiqueta (`renderer.rs:488`).
- `[SEMANTICS]` numeración única y niveles coherentes: sin número no hay nada que validar; la regla llega con el campo.

**Costo:** el más chico de la sección: dos campos, una variante de dibujo y dos reglas. Motor y layout no cambian; cada nivel es un documento aparte.

#### 17. Workflow Diagram → capacidad de `swimlane`

**Fuente:** notación propia de Graphium; no se presenta como BPMN ni comparte su vocabulario de eventos y compuertas. La fila de la referencia se llama "Workflow Diagram"; el PRD describe el kind como "proceso con carriles" (PRD §6, fila 12). No hay norma que citar.

**Hoy:** `swimlane` va sobre `IrTimeline` y el motor `lanes`: carriles (`Lane {id, label}`, `crates/gx-kinds/src/lanes.rs:19-25`) y pasos (`Step {id, label, lane, order, connects}`, `swimlane.rs:24-40`). El orden del array es el del proceso, cada paso declara su carril y `connects` es optativo (`swimlane.rs:35-40`); el motor enruta esas conexiones (`crates/gx-layout/src/lib.rs:1521-1533`).

**Falta:**

- `[CONTRACT]` decisión: el paso no tiene rol y el evento siempre sale `Shape::Rounded` (`swimlane.rs:178`), aunque `IrEvent` admite forma (`crates/gx-core/src/ir.rs:383`).
- `[CONTRACT]` condición de salida: `connects` guarda ids (`swimlane.rs:39`) e `IrConnection` es `{from, to}` (`ir.rs:413-418`); dos salidas de una decisión quedan sin «sí» ni «no».
- `[CONTRACT]` dueño por carril: `Lane` solo tiene `id` y `label` (`lanes.rs:19-25`); área y responsable comparten línea y el dibujo no los separa.
- `[CONTRACT]` entradas y resultados: `SwimlaneInput` solo tiene `lanes` y `steps` (`swimlane.rs:45-62`); una entrada puede ser un paso, pero entonces cuenta como orden.
- `[LAYOUT]` la rama: el motor ubica por columna de `order` y rutea conexiones (`lib.rs:1521-1533`); dos salidas de un mismo paso se dibujan como dos pasos posteriores, no como un corte.

**Costo:** cero formas nuevas si la decisión reusa el rombo con un rol de paso. El costo es contrato (rol, condición de `connects`, campos de carril, entradas y salidas) más una pasada de rama en el motor de carriles. Si la rama se acepta como secuencia, el motor no cambia y el diagrama miente.

#### 18. Functional Decomposition Diagram → capacidad de `tree`

**Fuente:** notación propia de Graphium, nunca IDEF0. IDEF0 (FIPS 183) es público y la distancia hay que decirla: el diagrama no tiene entradas, controles, salidas ni mecanismos, los cuatro lados de una caja IDEF0. Tampoco hay norma llamada "functional decomposition": la fuente es la práctica de descomponer una función en subfunciones y dibujarlas como jerarquía.

**Hoy:** `tree` anida (`crates/gx-kinds/src/tree.rs:27-35`), el documento es `root {name, note, children}` (`tree.rs:40-51`) y el tope de profundidad es 12 (`tree.rs:22`, aplicado en `:101`). La traducción deriva los ids de la ruta y funde `note` con el nombre en una segunda línea (`tree.rs:172-176`).

**Falta:**

- `[CONTRACT]` nivel como dato: `walk` calcula la profundidad y `to_ir` la descarta (`tree.rs:172`); el dibujo no numera ni agrupa por nivel.
- `[CONTRACT]` identificador por nodo (1, 1.1, 1.1.2): `TreeNode` no tiene campo (`tree.rs:27-35`); la ruta es el id interno, no un número.
- `[CONTRACT]` `meta` por nodo: el canal existe (`ir.rs:181`) y nadie lo llena (`ir.rs:202` lo crea vacío); `TreeInput` no tiene campo que lo reciba (`tree.rs:40-51`). Es el canal que también falta en la fila 13.
- `[SEMANTICS]` la distancia con IDEF0 es solo de documento: ninguna regla impide escribir entradas, controles y mecanismos en etiquetas, y el contrato no los declara fuera de alcance.
- `[LAYOUT]` bandas de nivel: el motor de árbol devuelve `groups: Vec::new()` (`crates/gx-layout/src/lib.rs:1088`) y no numera niveles.

**Costo:** aditivo y sin renderer si los niveles van como texto: `meta`, nivel y número en el contrato de `tree`, una regla de coherencia. Las bandas de nivel sí tocan el motor. Es el canal que paga la fila 13: una vez para las dos.

#### 19. Nassi-Shneiderman → kind nuevo con motor nuevo

**Fuente:** Nassi-Shneiderman (1973). Técnica bajo copyright del artículo: se implementa y no se reproducen sus figuras. El diagrama es una grilla de cajas anidadas para secuencia, selección (if/else) e iteración (while/case). No tiene aristas: la estructura se lee por contención, no por flechas.

**Hoy:** no hay nada: no hay kind registrado y ningún módulo se llama así. La IR solo conoce `Ir::Graph | Ir::Timeline` (`ir.rs:429-432`), y todo grafo pasa por un motor que ubica nodos y aristas: `Layouts` despacha `Tree` y `Lanes` y manda el resto a layered (`crates/gx-layout/src/lib.rs:1118-1122`). Ningún motor apila cajas: layered une con aristas, el árbol pone padres arriba y los carriles reparten filas.

**Falta:**

- `[CONTRACT]` el kind entero: schema de bloques (secuencia, selección, iteración), roles y validación.
- `[CONTRACT]` la forma en la IR: un grafo sin aristas se puede escribir, pero `IrNode` no tiene hijos y `IrGroup` es un cluster con etiqueta (`ir.rs:343-347`), no contención recursiva.
- `[LAYOUT]` brazo propio: `Layouts` despacha Tree, Lanes y el resto a layered (`lib.rs:1101-1123`); ninguno calcula alturas de bloques anidados.
- `[DRAW]` la grilla: el renderer pinta una forma por nodo (`renderer.rs:487-495`) y no hay llave de selección, esquina de iteración ni borde compartido.
- `[SEMANTICS]` reglas de bloque: un bloque tiene un solo padre, la iteración necesita cuerpo y la selección necesita dos ramas; hoy no existen porque no existe el contrato.

**Costo:** el más alto del catálogo: kind nuevo, forma nueva en la IR (o un canal de bloques), brazo de layout y brazo de renderer. Es el único de los 21 que pide motor nuevo, por una razón estructural: sin aristas no hay grafo que los motores existentes ubiquen.

### 5.5 Planificación y gestión

#### 20. Project Schedule Diagram → `gantt`

**Fuente:** no hay notación oficial; el cronograma es convención de facto y
cada herramienta define su juego de campos (PlantUML `@startgantt`, Tempo, MS
Project). Lo que un cronograma completo necesita, según esas referencias: eje
temporal con unidad, carriles de recurso, tareas con inicio/fin, dependencias
con **tipo** (FS, SS, FF, SF) y retardo, **hito de duración cero dibujado como
rombo**, **progreso dentro de la barra**, línea base vs real, y barrido de
resumen.

**Hoy:** `GanttTask {id,label,lane,start,end,depends_on}`
(`crates/gx-kinds/src/gantt.rs:34`), validado en `:89`; `depends_on` **sí se
dibuja** como una arista por entrada (`crates/gx-render-svg/src/scene.rs:61`),
con un codo FS fijo de borde derecho a borde izquierdo, sin etiqueta
(`crates/gx-render-svg/src/geometry.rs:95`). Un hito se puede *escribir*
(`end == start` evita el error de `end < start`, `gantt.rs:130`) pero no se
dibuja: una columna, ninguna barra, ningún rombo (`geometry.rs:75`), y `gantt`
no tiene tabla de roles — el rombo de `timeline` sí existe
(`crates/gx-kinds/src/timeline.rs:29`). El eje dibuja marcas solo para `start`
(`crates/gx-layout/src/lib.rs:1495`).

**Falta:**

- `[CONTRACT]` `progress` (0–100) por tarea; hoy no hay campo ni ocurrencia de
  `progress|percent|baseline` en `crates/`.
- `[CONTRACT]` dependencia como objeto `{to, type: fs|ss|ff|sf, lag?}` en vez
  de id suelto — cambio de contrato, no aditivo.
- `[SHAPE]` ninguno nuevo: el rombo del hito se reusa con el mecanismo de rol de
  `timeline`, y la banda de progreso es una clase de paleta, no una forma.
- `[SEMANTICS]` validar ciclos en `depends_on` (hoy se dibujan, `gantt.rs:153`), y validar coherencia entre `depends_on` y `start`/`end`
  (una tarea puede empezar antes de que termine su predecesora, en silencio).
- `[DRAW]` barra de progreso (segundo rect en los dos emisores), rombo del
  hito, y las rutas SS/FF/SF además del codo FS.
- `[LAYOUT]` las columnas son autoritativas por `start`: un SS/FF/SF con
  retardo necesita colocación derivada o, como mínimo, una verificación de
  coherencia antes de dibujar.

**Costo:** sin motor nuevo (el motor `lanes` sigue siendo columnar). Progreso y
rombo de hito: un campo y dibujo, con goldens. Tipos de dependencia y retardo:
medio (contrato, anclas y variantes de ruta). Línea base vs real: medio (segundo
span, segunda banda). Contexto: `f18-gantt.md:67` ya declaró fuera de v1 el
progreso y las fechas de calendario.

#### 21. PERT → kind nuevo `pert` (barato)

**Fuente:** técnica de dominio público, sin dueño de notación: origen 1958
(US Navy, Polaris), estimación de tres puntos `TE = (O + 4M + P) / 6` y
`σ = (P − O) / 6` (Malcolm, Roseboom, Clark y Fazar, 1959), camino crítico por
recorrido hacia adelante (ES/EF) y hacia atrás (LS/LF) con `holgura = LS − ES`:
holgura cero es crítico. Forma clásica: actividad-en-arista con un evento de
inicio y uno de fin, red acíclica.

**Hoy:** no hay kind (`crates/gx-kinds/src/lib.rs:20`), ninguna ocurrencia de
PERT, tres puntos ni camino crítico en `crates/`. Lo más cerca: las aristas de
`depends_on` de `gantt` (`crates/gx-render-svg/src/scene.rs:61`) y `IrGraph` con
motor `layered`.

**Falta:** `[CONTRACT]` el kind entero (actividades con O/M/P, red, salida de
holgura y camino); `[SEMANTICS]` los dos recorridos y el rechazo de ciclos —el
patrón reutilizable es el detector de `crates/gx-kinds/src/class.rs:194`—;
`[DRAW]` resaltar el camino crítico y escribir TE y holgura.

**Costo:** un kind con cero formas nuevas y cero motores nuevos. El camino
crítico es **semántica, no layout**: se calcula en el kind y se expresa con lo
que ya existe —`Emphasis::{Highlight, Muted}`
(`crates/gx-core/src/ir.rs:102`), `Style::Dashed`, `IrEdge.label` y
`IrNode.meta` con `IrValue::{Int,Float}` (`ir.rs:119`)—. `IrGraph` + `layered` en `LR`
ordena actividad-en-nodo exactamente por profundidad de dependencia. Antes de
prometerlo hace falta la guardia de ciclos (dagre rompe ciclos en silencio) y
que TE/holgura se dibujen como texto. Es del tamaño del escalón de una jornada
del PRD §6.2.

**Decisión:** va como kind propio, no como modo de `gantt`. El contrato de
`gantt` son barras sobre un eje temporal; el de PERT es una red sin eje. Meter
uno en el otro obliga a que `gantt` valide dos documentos distintos. Paga el
techo de 32 con una fila, y a cambio es el kind más barato del catálogo: no
pide forma ni motor.


## 6. La ruta

Cinco reglas de orden, y después las etapas.

**R1. Capacidad antes que kind.** Cada fila que se cierra con un campo, un rol o
una regla no gasta techo de 32. El mapa de componentes, el jerárquico, el
cronograma, la cronología, el workflow y el esquema relacional se cierran así.

**R2. Plataforma antes que catálogo.** Hay tres piezas que varias filas necesitan
y que hoy no existen en ningún kind:

- **`edge.kind`: vocabulario de relación tipada.** Hoy ningún kind tiene tipo de
  arista (solo `er` tiene cardinalidad y `sequence` tiene `reply`); el
  vocabulario vive en el texto libre de `label` (`crates/gx-kinds/src/flow.rs:41`).
  Lo piden el mapa de componentes, el activity, el use case («include»/«extend»)
  y el deployment (protocolos). Se implementa una vez, como campo opcional con
  vocabulario por kind, y el motor lo ignora para dibujar.
- **Anclas múltiples de fila.** Un extremo de arista se ancla a **una** fila
  (`from_row`/`to_row` son un `u32`, `crates/gx-core/src/ir.rs:227`). Sin lista
  de anclas no hay clave compuesta, ni FK compuesta, ni dependencia funcional
  de conjunto a conjunto: las tres filas de datos del §5.2 chocan contra el
  mismo límite.
- **Adornos de arista: cabeza y cola.** Hoy una arista solo sabe `arrow: bool`
  (`crates/gx-core/src/ir.rs:218`). UML necesita triángulo hueco (herencia),
  rombo hueco/lleno (agregación/composición), círculo (interfaz) y flecha
  abierta. Es una extensión del renderer **más su gemelo JS**
  (`crates/gx-app/frontend/src/lib/scene/`), así que se hace una vez y sirve a
  clases, componentes y deployment.

**R3. El código real manda el orden de la demanda.** Las filas que se alimentan
de un proyecto —`er` y el esquema relacional vía `code_er`, `class` y `graph`
vía `code_diagram`— valen más que las que solo se escriben a mano, porque el
agente puede llenarlas sin que el humano redacte JSON.

**R4. Ningún kind nuevo sin documento nuevo.** Un kind nuevo se justifica cuando
el documento tiene otra forma (un caso de uso no es un grafo con roles; un
bloque anidado no es un grafo). Cambiar de vocabulario no justifica un kind: se
justifica con roles.

**R5. El techo se mide antes de prometer.** Con la regla R4 la referencia suma
cuatro kinds (§4); agregar los nueve del PRD §6 que faltan lleva el total a 27
de 32. Cualquier fila que además quiera kind propio obliga a retirar otra.

### Etapa 0 — vocabulario compartido (plataforma)

`edge.kind` + adornos de cabeza y cola + estereotipo dibujado en las cajas de
flow. No cierra ninguna fila por sí sola: desbloquea la etapa 1. Es el trabajo
más caro de la ruta en proporción a su tamaño, y el único que toca los dos
emisores (Rust y el gemelo JS) con goldens nuevos.

**Hecho en `f26-adornos` (2026-11-11):** la mitad de aristas. `IrEdge` dejó
`arrow: bool` por `head`/`tail` opcionales con un vocabulario cerrado
(`arrow`, `open_arrow`, `hollow_triangle`, `diamond`, `hollow_diamond`), una sola
resolución (`IrEdge::head_end`/`tail_end`) en la que una `cardinality` se queda
con los dos extremos, y los dos pintores escriben cualquier glifo en cualquier
extremo (`marker-end`/`marker-start`, el de origen espejado, las reglas CSS solo
de las familias que el documento usa). `class` estrena la notación que nombraba
desde siempre: rombo lleno en `composition` y la fila nueva `aggregation` con
rombo hueco. `er` dejó de apagar la flecha a mano. Único golden que se movió:
`class-shop`. **Falta de la etapa 0:** el estereotipo dibujado en las cajas de
`flow` (vocabulario de nodo, no la ranura de arista) y el tipo compartido
`EdgeKind` para el vocabulario de relación tipada, que entra con las nueve
relaciones de `component` en la etapa 1.

### Etapa 1 — capacidades sobre kinds existentes (cierra diez filas sin gastar techo)

| Fila | Kind | Trabajo |
|---|---|---|
| 20 Cronograma | `gantt` | `progress`, hito con rombo, dependencia `{to,type,lag}`, guardia de ciclos y coherencia con `start`/`end` |
| 14 Cronología | `timeline` | campo de dependencias (el dibujo ya existe en `scene.rs`) |
| 12 Mapa de componentes | `component` | seis roles nuevos + estereotipo dibujado + las nueve relaciones tipadas de la etapa 0 |
| 13 Jerárquico | `tree` | `meta` y nivel como dato por el canal existente; agrupaciones **no** (requieren clusters en el motor de árbol) |
| 8 ERD | `er` | cardinalidad mínima y opcionalidad en el contrato, no en el texto de la etiqueta |
| 9 Esquema relacional | `er` | índices, UNIQUE/CHECK y PK compuesta marcada; el puente SQL (`er_bridge.rs`) ya trae tipos y FK |
| 15 Flowchart | `flowchart` | **primera tanda:** el paralelogramo de entrada/salida (hoy `data` es una caja, que para ISO está mal) y el conector de página |
| 16 DFD | `dataflow` | número de proceso y nivel: **el más barato de la sección**, dos campos y dos reglas |
| 17 Workflow | `swimlane` | rol de decisión (reusa el rombo), condición en `connects`, dueño por carril y entradas/salidas |
| 18 Descomposición funcional | `tree` | nivel e identificador como dato por el canal `meta`: la misma pieza que paga la fila 13 |

Todas son aditivas salvo la dependencia tipada de `gantt`, que cambia la forma
de un campo existente y por eso necesita migración de documento con versión.

**La moneda de las formas.** Cada glifo nuevo cuesta en los dos emisores más un
golden, así que no entra todo junto: el juego ISO 5807 completo pide hasta nueve
formas (paralogramo de E/S, hexágono de preparación, rectángulo de doble borde,
documento de borde ondulado, trapecio, entrada manual, display, demora,
conector/periférico) y por eso `flowchart` entra en dos tandas: la de arriba
—las dos que arreglan lo que hoy dice mal— y una segunda con el resto, decidida
solo si aparece un uso real.

**La trampa del workflow.** `swimlane` ubica por columna de `order` y rutea
`connects` como secuencia: dos salidas de una misma decisión se dibujan como dos
pasos posteriores, no como un corte. Si la rama se acepta así, el motor no
cambia **y el diagrama miente**. Es la única fila de la etapa 1 que exige una
pasada real en el motor de carriles.

### Etapa 2 — kinds nuevos baratos (documento nuevo, cero forma y cero motor)

- **`pert`** (§5.5): O/M/P, ES/EF/LS/LF, holgura y camino crítico resaltado. El
  camino crítico es semántica; layered hospeda la red en `LR`. El kind más
  barato del catálogo y el que más se nota en una planificación real.
- **`use_case`** (§5.1): actores, casos de uso, límite del sistema y
  «include»/«extend» — esto último depende de `edge.kind` (etapa 0). Necesita
  una forma de actor (monigote) o un rol con estereotipo: se decide en el corte.

### Etapa 3 — kinds nuevos con estructura nueva

Cada uno de estos pide algo que ningún documento de hoy tiene:

- **`activity`** (§5.1): fork/join (barra) y particiones. La barra es una forma
  nueva; las particiones reusan `groups` del motor layered. Motor **decidido:
  `layered`** —el PRD ya lo refleja— porque una partición anida, `LanesEngine`
  no dibuja grupos y la guarda vive en la arista.
- **`deployment`** (§5.2): nodos que contienen entornos que contienen
  artefactos — anidamiento de tres niveles con etiquetas por nivel; el contrato
  de `groups` de hoy anida pero no etiqueta por nivel con vocabulario propio.
- **`document_db`** (§5.3): documentos embebidos recursivos dentro de la caja de
  la colección. Se parece a `tree` pero dentro de un nodo del grafo.
- **`nassi_shneiderman`** (§5.4): bloques anidados. **Ningún motor actual los
dibuja**: no hay aristas, la estructura se lee por contención, y la IR solo
  conoce grafo y línea de tiempo. Es el único de los 21 que pide motor nuevo, y
  por eso va último de la etapa.

Además, `flowchart` completa su segunda tanda de símbolos ISO en esta etapa o
nunca: son siete formas más que solo valen si alguien las usa.

### Etapa 4 — lo caro o lo que depende de una medición

- **Normalización** (fila 11) como capacidad de `er`: dependencias funcionales
  dibujables y una lectura 1FN→3FN/BCNF. La referencia misma dice que no hay
  notación gráfica universal, así que primero hay que decidir el dibujo.
- **Escala temporal proporcional** en `timeline`: cambiar el paso ordinal de
  `LanesEngine` por un mapeo tiempo→x, con unidad y formato en el eje.
- **Categorías de columna** en `timeline`: no hay modelo, es lo más caro de esa
  fila.

### Lo que la ruta no incluye

Los nueve kinds del PRD §6 que la referencia no pide (`block`, `api`, `trust`,
`journey`, `network`, `gitgraph`, `sankey`, `orgchart`, `neural`) siguen en pie
y no los toca este documento. El orden entre las dos listas es una decisión de
producto pendiente: hoy hay 14 + 4 de la referencia + 9 del PRD = 27 de 32, y
`orgchart` probablemente se cierre con la etapa 1 del jerárquico (`tree` +
metadatos + roles) sin gastar fila.


## 7. Marca, licencia y conformidad

El documento de referencia lo pide en su §2.1 y tiene razón: **una notación, una
especificación, una implementación y una marca son cuatro cosas distintas**.
Esta sección fija la regla operativa de este catálogo; no es una opinión legal.

**La regla:** ninguna fila de §3/§5 promete conformidad. Una implementación pasa a
decir «conforme a X» solo después de que exista una suite de conformidad
corrida contra la especificación, y hoy no existe para ninguna. Mientras tanto
toda fila dice «notación propia inspirada en X».

Lo que sí está verificado, fuente por fuente:

| Fuente | Estado de la fuente | Qué implica para Graphium |
|---|---|---|
| UML 2.5.1 (OMG) | Descarga libre, `formal/2017-12-05`, modo de propiedad intelectual **RF-Limited** (omg.org/spec/UML/2.5.1) | Los conceptos y la notación se pueden implementar; el PDF, sus figuras y su texto no se reproducen. No se declara «compatible» ni «certificado». |
| BPMN 2.0 (OMG) | Especificación propia, marca de OMG | El `workflow` propio no se llama BPMN y no comparte su vocabulario de eventos/gateways. |
| ArchiMate (The Open Group) | Especificación y *reference cards* propias | El mapa de componentes no se llama ArchiMate; se lo compara con su capa de aplicación, nada más. |
| C4 (c4model.com) | Sitio y figuras bajo licencia Creative Commons según su propio sitio | El kind `c4` **sí** declara el nivel y usa los nombres de C4, así que su deuda es distinta: tiene que implementar los cuatro niveles o dejar de llamarse `c4` (§5.2). |
| ISO 5807:1985 | Norma **paga** (CHF 135 / €130 en el catálogo de ISO) | Los símbolos de flowchart se implementan por convención conocida, no citando la norma. No se declara conformidad con ISO 5807: ni siquiera se compró el texto. |
| IDEF0 (FIPS 183) | Documento público del gobierno de EE. UU. | El nombre se puede usar, pero el `functional decomposition` propio **no** implementa entradas/controles/salidas/mecanismos, así que no se presenta como IDEF0. |
| Nassi-Shneiderman | Técnica de 1973; el artículo está bajo copyright | La técnica se implementa; las figuras del artículo no se reproducen. |
| PERT | Técnica de dominio público (1958) | Sin restricción de nombre; es el kind más limpio del catálogo en este eje. |
| MongoDB / bases documentales | No hay estándar; la documentación del proveedor es la referencia de facto | El diagrama de documentos se describe como notación propia; no se declara compatibilidad con ninguna herramienta. |

**Lo que este repositorio ya tiene resuelto:** el dibujo es propio (no hay
Mermaid ni librería JS de diagramas), la única tipografía es Inter bajo OFL
embebida (`include_bytes!`), y las formas son nuestras. La superficie de riesgo
de licencia no está en el render: está en el **nombre** de cada notación y en lo
que el nombre promete.

## 8. Cómo se mantiene este documento

1. **Los conteos salen del registro, no de la memoria.** El §2 lista los kinds
   que `register_all` registra; si un kind entra, su fila entra y el total del
   §3 y del §4 se rehace. Un documento que dice «catorce» cuando el binario
   registra quince es peor que no tener documento.
2. **Una fila por decisión, no por deseo.** Cuando una entrada del catálogo se
   implementa, su fila cambia de «falta» a «hecho» citando el corte que la cerró
   (el patrón es `odd/tasks/f22-dibujo.md`).
3. **Un faltante sin etiqueta no existe.** `[SHAPE]`, `[CONTRACT]`, `[LAYOUT]`,
   `[SEMANTICS]`, `[DRAW]` son las únicas categorías de trabajo que este
   documento reconoce; una frase suelta no entra al §5.
4. **La ruta del §6 se consume de arriba hacia abajo**, y el techo de 32 kinds
   del PRD (RF29/R10) se mide antes de prometer una etapa, no después.
5. **Mecanismo que falta y conviene decidir:** una prueba que lea este
   documento y falle si el número de kinds del §2 no coincide con
   `kind_names()`. No está implementada; es la única forma de que el punto 1 no
   dependa de la disciplina de quien edite.
