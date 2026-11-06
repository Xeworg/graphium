# Idea — F9: el libro del proyecto (Tree-sitter + SQLite + FTS5)

**Estado: idea en análisis.** Este documento no autoriza implementación.
Nació de una propuesta del humano (2026-11-05, texto completo en S1) y de un
resumen de arquitectura que trajo de otra conversación (§3). El resumen **no
es una guía**: abajo se marca qué se adopta, qué se descarta y qué se
questiona. Nada de esto está decidido hasta que el humano lo apruebe.

## 1. La idea en palabras del humano

- S1 (verbatim): «quiero que agregemos soporte mediante treesitter, guardar
  ese parse en una base de datos sqlite, con la itencion de tener una
  estructura complet del proyecto y usando fts5 hacer una busqueda
  acereleraday mas precisa de la base de datos, la itencion tener un libro
  abierto del proyecto, tener coexiones y ampliar la posblidadde los
  diagramas, que mcp pueda birndar al agente para generar diagramas mas
  baratos, agregaremos 5 lenguajes primero e iremos agregando mas en grupos
  hasta tener los mas usados, este espec se puede trabajar en un worktree en
  una rama aparte hasta que sea estable y funcional, cosas a tener en cuenta,
  si el usuario usa worktrees, como mantendremos el programa en el proyecto,
  seria mejor tener una base de datos en la ruta principal del proyecto se
  debe mantener siempre el monitor del proyecto».
- S2 (verbatim, sobre el proceso): «tengo una idea que sera necesario
  analizar cuidadosamente, asi que no haremos nada solo un documento para
  apoyarnos solo es una idea».

## 2. Qué es «el libro abierto del proyecto»

La metáfora vale para fijar el objetivo antes de elegir tecnología. El libro
tiene:

- **Capítulos**: los archivos del proyecto, identificados por ruta relativa,
  hash de contenido y lenguaje.
- **Entradas**: los símbolos que Tree-sitter puede nombrar con confianza
  (funciones, tipos, métodos, imports) con su ubicación exacta (archivo,
  línea, columna, rango de bytes).
- **Vínculos**: las relaciones que se pueden extraer sin análisis semántico
  completo — imports, llamadas por nombre, herencia/impl, uso de tipos. Con
  su honestidad escrita: son relaciones **sintácticas**, no resolución de
  tipos; va a haber falsos positivos y eso se declara en la respuesta de las
  tools.
- **Índice**: FTS5 sobre identificadores y rutas — la búsqueda acelerada.
- **Ilustraciones**: la parte que solo Graphium puede dar — cada vínculo y
  cada subgrafo del libro se puede pedir como documento de diagrama ya
  armado, para que el agente lo renderice sin leer código.

El libro es **derivado del código, nunca fuente de verdad**: se puede borrar
la base de datos entera y reconstruirla desde los archivos. Eso define el
contrato de toda la feature: la base es un caché de lectura, no un almacén.

## 3. Del resumen que trajo el humano: qué se adopta, qué se descarta

### Se adopta

1. **Modelo relacional con lista de adyacencia** para los símbolos y las
   relaciones (`ast_nodes`/`code_relations` con `parent_id` y
   `sibling_index`, recorridos con `WITH RECURSIVE`). Es lo correcto para
   las consultas de impacto y los cierres transitivos.
2. **Descarte del CST completo.** El resumen lo deja como opción (modelo
   documental JSON); aquí se descarta como persistencia: guardar el árbol
   completo infla la base sin valor de consulta para el libro. Del CST solo
   sobrevive lo podado (declaraciones de alto nivel + spans). El parseo
   completo es un paso transitorio en memoria.
3. **Enfoque híbrido FTS5 + relacional**: FTS5 resuelve y ubica símbolos en
   milisegundos; el análisis relacional profundo va contra las tablas.
4. **Watcher con debounce + parseo incremental**: ventana de consolidación
   de 300–500 ms y re-parseo solo del subárbol afectado.
5. **SQLite en WAL** con `busy_timeout` y transacciones atómicas
   (`BEGIN IMMEDIATE`/`COMMIT`).
6. **Escritor único**: un solo proceso encola y ejecuta las escrituras. En
   Graphium esto ya existe: es el **dueño** (`daemon.rs` + socket de
   instancia única). El resumen propone un binario daemon nuevo; aquí se
   adopta la idea del escritor único pero sobre el dueño que ya hay (ver
   descarte 8).
7. **Compilación estática de parsers** vía el crate `cc`, y distribución por
   el `install.sh` y releases que el proyecto ya tiene (F7). El costo de
   ~6 MB por 10–15 lenguajes hay que medirlo contra el binario actual
   (~6,4 MB release) — el presupuesto de binario (RNF1-quater) tiene que
   dar con la feature adentro, y eso es una restricción dura del PRD.
8. **Watchers acotados al raíz de cada worktree** y reindexación diferencial
   al alternar ramas (hooks `post-checkout`/`post-merge` como optimización,
   no como requisito).

### Se descarta

1. **Persistir el CST completo como JSON** (razón en «adopta 2»): la base
   guarda símbolos, relaciones y hashes, no árboles.
2. **Exportar a Mermaid/Graphviz** (punto 4 del resumen): el PRD retiró
   Mermaid (§21) a propósito — Graphium renderiza sus propios kinds. La
   exportación del libro son **documentos de diagrama de Graphium** (los
   kinds que ya existen o uno nuevo), no dialectos externos.
3. **Daemon/binario CLI nuevo separado**: duplicaría la lógica de instancia
   única, el ciclo de vida y el tray que ya tiene el dueño. El indexador
   vive en el dueño, en threads de fondo, con presupuesto de CPU acotado.
4. **OCC con SHA-256 por commit de agente** (punto 6 del resumen): los
   agentes no escriben en la base — solo leen. El hash de contenido sirve
   para deduplicar trabajo del watcher (no re-parsear lo que no cambió), no
   para control de concurrencia de escritura; la escritura ya es de un solo
   proceso.
5. **`.ast_cache/` para proyectos sin git** (punto 7 del resumen): v1 no lo
   necesita — el directorio de datos del proyecto (`.graphium/`) no depende
   de git. Cae en la lista de «después si hace falta».

### Se cuestiona (decisión abierta del humano)

1. **¿Dónde vive la base de datos?** El humano inclinó: «seria mejor tener
   una base de datos en la ruta principal del proyecto». El problema: una
   base compartida entre worktrees guarda el *mismo archivo* con *contenido
   distinto* según la rama de cada worktree — el libro se corrompe o hay
   que versionarlo por worktree adentro de una sola base (más storage, más
   complejidad, consultas con filtro en todo).
   - **Opción A (recomendada)**: una base por worktree en
     `<raíz del worktree>/.graphium/libro.sqlite`. Cada worktree ve su libro
     según lo que tiene checked out; la ruta principal también tiene el
     suyo. Simple, correcto, aislado. El «libro del proyecto» completo es la
     suma de libros si se quisiera, pero nadie lo necesita para dibujar.
   - **Opción B**: base única en la ruta principal con columna de worktree.
     Solo tiene sentido si el humano quiere consultar el libro de otro
     worktree desde la ventana — hoy no hay caso de uso para eso.
2. **¿Cuáles son los 5 lenguajes primeros?** Propuesta: **Rust,
   TypeScript/TSX (cubre JS), Python, Go, Java** — los más usados en
   flujos con agentes. C entra gratis como dependencia de los grammars.
3. **¿Cómo llega el libro al agente?** Tres formas, no excluyentes:
   - **Tools de lectura nuevas** (`code_search`, `code_structure`,
     `code_impact`) que devuelven JSON del libro.
   - **Una tool que arma el documento del diagrama** (por ejemplo
     `code_impact_diagram(simbolo)` que devuelve el JSON de un kind `graph`
     listo para `render_diagram`) — el «diagrama barato»: el agente no lee
     código para dibujar, la base ya tiene las conexiones.
   - **Un kind nuevo `code`** con engine propio que consulta la base.
     Más caro de mantener; solo si las dos anteriores quedan cortas.
4. **¿El monitoreo es siempre automático?** El humano dijo «se debe mantener
   siempre el monitor del proyecto». En repos grandes el primer indexado
   puede llevar minutos y gastar CPU: se propone arrancar encendido con
   presupuesto (parseo en segundo plano, cola priorizada por
   recencia/tamaño, `target/`/`node_modules`/`.git` excluidos por
   `.gitignore`) y una bandera para apagarlo por proyecto.

## 4. Restricciones del proyecto que muerden

- **RNF10 / RNF1-quater**: el binario crece con los parsers y la base es
  memoria del proceso del dueño. La base vive en disco (SQLite), no en RAM;
  las consultas son acotadas; el presupuesto de RSS tiene que re-medirse con
  el indexador corriendo.
- **El store de diagramas no cambia**: el libro no entra al plano salvo como
  documentos de diagrama que el agente renderiza con las tools de siempre.
- **Main protegida + PRs encadenados**: cuando esto se apruebe, se trabaja
  en worktree y rama aparte hasta estable, como propuso el humano (S1).
- **CI solo con etiquetas de versión**: los pipelines de compilación cruzada
  del resumen se adaptan a esa política.
- **Windows**: los grammars de tree-sitter compilan con `cc`; el CI de
  Windows del proyecto (si existe para release) necesita validar esa ruta.

## 5. Riesgos honestos

1. **Las relaciones son heurísticas.** Sin resolución de tipos,
   `calls`/`uses` por nombre da falsos positivos (dos funciones con el mismo
   nombre en módulos distintos). Se declara en la respuesta de la tool
   (`confidence` o `exact: false`) y se puntualiza por lenguaje.
2. **API de tree-sitter inestable entre versiones** (0.20 → 0.24 cambió
   shapes): fijar versiones de grammars por lenguaje y encapsular el acceso
   detrás de un módulo propio (`gx-index`), que es el único que toca
   tree-sitter.
3. **El watcher es la parte que más se rompe en la práctica** (inotify
   límites, eventos de git que tocan miles de archivos, editores que escriben
   a trozos). El debounce + hash de contenido + reindexación diferencial por
   `git status` en vez de confiar ciegamente en eventos es la defensa.
4. **Tamaño de la base** en repos grandes: acotar con exclusiones y con
   índices por archivo; borrar el capítulo (las filas del archivo) cuando el
   archivo muere.

## 6. Forma que tendría si se aprueba (borrador, no compromiso)

- **Corte C0 — spike**: módulo `gx-index` (SQLite + FTS5 + watcher + 1
  lenguaje, Rust), schema v1, dentro del dueño, una tool de lectura
  (`code_search`). Criterio de salida: buscar un símbolo de este repo
  responde en <50 ms y el watcher sigue a los edits sin reconstruir todo.
- **Corte C1 — conexiones y diagramas baratos**: extracción de relaciones
  para los 5 lenguajes, `code_impact` con `WITH RECURSIVE`, y la tool que
  arma el documento de diagrama para el agente.
- **Corte C2 — grupos de lenguajes** y refinamiento por lenguaje.
- **Corte C3 — pulido de worktrees/hooks** y lo que la práctica pida.

## 7. Decisiones y preguntas abiertas

### Decidido por el humano (2026-11-05)

- **D1 — ubicación de la base (§3, cuestionado 1):** «usaremos lo más
  efectivo» → **opción A**: una base por worktree en
  `<raíz del worktree>/.graphium/libro.sqlite`, con la ruta canónica del
  worktree grabada adentro para detectar moves y reindexar. El análisis
  comparativo (A vs base en la ruta principal vs directorio común de git)
  quedó en la conversación del 2026-11-05: A gana porque el índice siempre
  corresponde al contenido checked out, las consultas no filtran por
  worktree y el watcher queda acotado.
- **D2 — lenguajes primeros (§3, cuestionado 2):** Rust, TypeScript/TSX
  (cubre JS), Python, Go, Java.
- **D3 — exclusiones (§3, cuestionado 4):** «si ignoraremos lo mismo que git
  para no desperdiciar los recursos del usuario» — el watcher respeta
  `.gitignore` (más los excluidos duros: `.git`, `.graphium`); el monitoreo
  no gasta recursos en lo que git ya ignora.

### Resuelto — P3: la puerta al agente (2026-11-05, tres explores + síntesis)

Los tres exploradores convergieron: **tools de lectura gruesas + el documento
de diagrama como artefacto a pedido, sin kind nuevo en v1.**

- **Costo (explore 1):** agregar una tool son ~6 puntos de edición localizados
  en `gx-mcp` (params con schemars, un brazo de dispatch, respuesta, harness)
  y **no hay techo de tools en el PRD**; un kind nuevo exige cambiar el trait
  `to_ir` de `gx-core` (sin canal de I/O hacia la base), 5 brazos per-kind en
  `contract.rs`, 2 goldens + test de degradación, y gasta uno de los 32 slots
  de RF29. Un kind que consulta la base además rompe el determinismo de los
  goldens. Veredicto: tools, no kind — y el objetivo ya termina en
  `render_diagram` con un documento `graph` plano.
- **Forma (explore 2):** el kind que calza es **`graph`** (símbolos = nodos,
  llamadas/imports = aristas con `label`; 300 nodos / 600 edges). La tool
  **devuelve el documento JSON** (`return-document`) y el agente lo pasa por
  `render_diagram`: cruza el contexto dos veces, pero es correctable con
  `validate_diagram` y recuperable con `get_diagram` tras una compactación;
  render-in-tool sería más barato en tokens pero invisible e incorregible.
  Con return-document el problema de propiedad se disuelve: el documento es
  texto, cualquier sesión puede renderizar su propia copia.
- **Patrones externos (explore 3):** los referentes serios usan pocas tools
  question-shaped (GraphPilot: 4; mcp-ctags: 3) y publican honestamente la
  calidad de resolución (GraphPilot declara 25–35 % de edges). Anti-patrones
  con evidencia: staleness silencioso, DB con inode fijado tras recrear el
  worktree (golpea directo a D1: detectar y reabrir), tools muertas que
  siguen en `tools/list`, bloat de schemas, respuesta única ante un nombre
  ambiguo.

**Condiciones que la síntesis fija para el futuro feature doc:**

1. Tres tools question-shaped: `code_search`, `code_structure`, `code_impact`
   — más la tool que arma el documento `graph` (candidato: `code_diagram`).
2. Toda respuesta lleva `file:line` + `content_hash` + `indexed_at`; nombre
   ambiguo → candidatos + código de ambigüedad, nunca una respuesta única
   confidente; edges sintácticas con `exact: false` y % de resolución
   publicado por lenguaje.
3. Camino de rebuild forzado (tool o bandera) y detección de base borrada/
   recreada (el inode no es identidad).
4. Las tools del libro viven detrás de un flag por proyecto (off por
   defecto en proyectos donde no se pidió), porque cada tool también cuesta
   tokens de schema en cada sesión.
5. Costo abierto que el feature doc tendrá que resolver: cómo resuelve la
   tool la ruta del proyecto de la sesión para elegir la base del libro
   (`session.rs` ya lleva `project: Option<String>`; falta el plumbing).

### Resuelto — análisis de la base (segunda ronda de explores, 2026-11-05)

Tres exploradores con ideas no convergentes sobre schema, capa de consultas
y mapa/multi-proyecto. Síntesis:

- **Schema (explore 1) — recomendación: «idea B»**, tablas tipadas duales
  (`file`, `symbol`, `edge`) con FTS5 `contentless_delete` sobre rangos de
  id contiguos por archivo (borrar un capítulo es `DELETE ... BETWEEN`),
  `SAVEPOINT` por archivo y lotes de ≤32 archivos por transacción, migración
  por `PRAGMA user_version` con delete-and-reindex (el libro es derivado: no
  hace falta escalera de migraciones). Las tres ideas evaluadas: (A) un solo
  grafo de adyacencia con todo computado — complica distinguir `contains`
  de `calls` en cada salto; (B) tablas duales — la recomendada; (C)
  capítulo-JSON + proyección caliente — el blob es un schema sin testear.
  Tradeoffs aceptados: sin closure transitivo materializado (impact con
  `depth ≤ 3` y `LIMIT` explícito) y sin texto FTS almacenado (no hay
  `snippet()`/`highlight()`: el `file:line` sale de `symbol`).
- **Consultas (explore 2) — recomendación: híbrido por etapas**: FTS5
  resuelve nombres → candidatos rankeados → rechazo obligatorio ante
  ambigüedad → BFS acotado en el app. Hechos duros de SQLite: `unicode61`
  trata `_` como separador (snake_case tokeniza gratis; **CamelCase hay que
  partirlo al indexar** con texto de búsqueda derivado — ninguna opción de
  tokenizer parte camelCase), no hay límite documentado de iteración para
  CTEs recursivos (guardia de ciclos explícita con visited set o columna
  depth), y las respuestas llevan un objeto budget fijado a los techos del
  kind `graph` (300/600) con paginación por cursor.
- **Mapa y multi-proyecto (explore 3)**: file-edges por `GROUP BY`
  on-the-fly (memoizadas por stamp), mapa como **texto rankeado primero**
  (ranking tipo aider: `mul·√refs`, penalizar >5 definidores y el `_`
  inicial) y documento `graph` a pedido, con contracción a carpeta por
  encima de los 300 archivos. El dueño sirve **todos** los proyectos de la
  máquina (socket por usuario, `MAX_SESSIONS = 8`): el routing del libro lee
  el **registro de la sesión** (`GRAPHIUM_PROJECT` o el cwd del forwarder,
  `session.rs:283-285`), nunca `current_dir()`. Sin consultas
  cross-proyecto en v1.
- **Riesgo nuevo para D1**: WAL es inválido en filesystems de red y sufre
  starvation de checkpoint con lectores largos — la base dentro del worktree
  puede estar en una carpeta sincronizada a la nube (D1 según PRD §7.3
  requiere enmienda por RNF4: ver P8). El feature doc tendrá que decidir la
  ubicación final y documentar esta restricción.

### En análisis

- **P6 — el mapa de conexiones entre archivos (pedido del humano,
  2026-11-05):** «haz tenido en cuenta guardar también las conexiones entre
  archivos esto nos dara un mapa que incluso el agente podria usar como
  herramienta de analisis rapido». El explore de mapa (2026-11-05) propuso:
  file-edges por **GROUP BY on-the-fly** sobre las relaciones de símbolos
  (memoizadas después por stamp del grafo), mapa entregado como **texto
  rankeado primero** — con ranking tipo aider (peso `mul·√refs`, penalizar
  nombres con >5 definidores y el `_` inicial) — y el documento `graph` solo
  a pedido, con **contracción a carpeta** por encima de los 300 archivos
  (techo del kind `graph`: un mapa solo-diagrama degrada apenas se pasa).
  El ranking de aider no persiste file-edges: reconstruye el grafo en
  memoria por corrida desde un caché de tags con clave mtime — patrón
  compatible con un v1 sin tablas materializadas.
- **P7 — múltiples proyectos conectados (pedido del humano, 2026-11-05):**
  «hay que pensar como manejaremos multiples proyectos de usuario
  conectados». Correcciones de premisa del explore (2026-11-05): el dueño
  sirve **todos** los proyectos de la máquina (el socket es por usuario,
  `MAX_SESSIONS = 8`), no un solo repo; y el proyecto de una sesión sale de
  `GRAPHIUM_PROJECT` o del **cwd del forwarder** (`session.rs:283-285`),
  nunca del cliente — el routing del libro debe leer el **registro de la
  sesión**, no `current_dir()`. Recomendación: libros por raíz canónica de
  worktree y **sin consultas cross-proyecto en v1**.
- **P8 — colisión de RNF4 con la ubicación D1 (tres explores, 2026-11-05,
  análisis en §8, decidido en §9):** RNF4 (PRD §7 «No funcionales",
  `docs/PRD.md:301`) limita las escrituras de Graphium a `~/.graphium/`; la
  base en el worktree exige enmienda y el nombre `.graphium` colisiona con
  el directorio de datos del usuario, la clase CSS de los exports y
  `graphium.json` (RF21). Las tres opciones fueron defendidas por un explore
  cada una y su análisis completo está en §8. La decisión quedó tomada por
  el principio del humano (§9).
- **P5 — cuándo se vuelve feature doc (§3, cuestionado 5):** el humano dijo
  «analizaremos más»; el doc sigue en estado idea y la conversión a
  S#/T# espera.

### Histórico (respondido en la conversación del 2026-11-05)

- Cuestionado 1 (ubicación) → decidido (D1).
- Cuestionado 2 (lenguajes) → decidido (D2).
- Cuestionado 3 (puerta al agente) → resuelto (síntesis P3).
- Cuestionado 4 (monitoreo) → decidido (D3).

## 8. P8 — la ubicación del libro, defendida por tres exploradores (2026-11-05)

Terreno común: RNF4 (`docs/PRD.md:301`, §7 «No funcionales») limita las
escrituras a `~/.graphium/`; el nombre `.graphium` ya significa tres cosas
(data dir del usuario, clase CSS del render, `graphium.json` de RF21); WAL
falla en NFS/SMB y las carpetas sincronizadas replican mal un `-wal` vivo; el
libro es derivado (borrarlo solo cuesta reindexar); el dueño es el único
escritor.

### Opción A — dot-dir en el worktree: `<raíz>/.graphium-book/index.sqlite`

- **A favor:** el índice es literalmente el espejo del checkout — cero
  ambigüedad de identidad; el watcher queda acotado; el «reflejo» humano de
  borrarlo junto al worktree funciona.
- **Enmienda:** RNF4-bis, una oración: el **dueño** (nunca la IA) escribe un
  caché derivado y descartable por worktree; la IA sigue en
  `~/.graphium/exports/`. Debe ser un módulo de política aparte
  (`book_root()`), nunca un segundo exports root.
- **Mitigaciones:** `journal_mode=DELETE` para esta base (el dueño es
  escritor único; mata la replicación del `-wal` vivo); marcador
  trust-on-first-byte en `meta` (un repo clonado puede **pre-sembrar**
  `index.sqlite`); identidad = `git rev-parse --git-dir` + raíz canónica +
  `HEAD` + `indexed_at`; ignore en el `.gitignore` **trackeado** del
  proyecto.
- **Debilidades admitidas:** pone escrituras dentro de repos que Graphium no
  creó (read-only, NFS, contenedores, repos ajenos — y el pre-seeding es una
  trust boundary nueva); el índice puede ser copia stale/frozen que *parsea
  bien*: «el libro puede estar equivocado; nunca con confianza».

### Opción B — common dir de git: `<git-common-dir>/graphium-book/<hash>.sqlite`

- **A favor:** no ensucia ningún árbol de trabajo; el common dir ya
  namespacea el clon; ninguna operación documentada de git lo toca (pero es
  tolerancia observada, no contrato).
- **Descarte interno:** la variante `.git/worktrees/<id>/` **muere** —
  `git gc` ejecuta `worktree prune --expire 3.months.ago` y borraria la base
  con el worktree ausente.
- **Enmienda:** RNF4 en dos cláusulas (el common dir como única excepción
  para el índice derivado; la IA sin cambios).
- **Mitigaciones:** barrido de huérfanas al abrir (ruta inexistente +
  `rev-parse` que no apunta a este common dir + no-locked + mtime > 30 días);
  detección de libro stale por `meta.worktree_root` + `HEAD` + hashes de
  contenido, **nunca por inode**; derivación con
  `git rev-parse --path-format=absolute --git-common-dir`.
- **Debilidades admitidas:** reintroduce la dependencia de git (proyectos
  sin git quedan sin libro — segundo camino de rutas y tests duplicados); un
  bug de cleanup dentro de `.git` tiene radio de daño sobre el directorio
  sagrado; los backups de `.git` arrastran DB+WAL calientes.

### Opción C — centralizado en el usuario: `~/.graphium/books/<slug>-<16hex>/libro.sqlite`

- **A favor:** cumple RNF4 **sin enmienda** (la cláusula `(settings +
  exports)` es descriptiva — el PRD ya planea `usage.json` ahí); fuera de
  carpetas sincronizadas por defecto (dota macOS iCloud, OneDrive KFM); y
  **fortalece** la cláusula de seguridad: bajo D1 el agente (con cwd del
  repo) podría falsificar el libro y envenenar el análisis — bajo C solo el
  uid del dueño escribe libros.
- **Diseño:** un **directorio** por libro (contiene `-wal`/`-shm`; un libro
  se borra con un `rm -rf`); `GRAPHIUM_BOOKS_ROOT` espeja
  `GRAPHIUM_EXPORTS_ROOT`; registry en `books/index.json` (root,
  common dir, first_root_commit, head, bytes, last_opened_at); re-key por
  `first_root_commit` cuando la raíz grabada ya no existe (maneja `mv`);
  stamp `(HEAD, mtime del git index, inode)` contra re-clones en la misma
  ruta; probe de arranque → filesystem remoto ⇒ `journal_mode=DELETE` +
  warning; prune en el idle-exit de 60 s del dueño con cap global LRU.
- **Debilidades admitidas:** la identidad pasa a «path→lookup» — un libro
  equivocado es silencioso, no imposible, y el plumbing de `session.rs` (que
  solo lleva `GRAPHIUM_PROJECT`/cwd sin canonicalizar) se vuelve load-bearing
  cuando RF30 declara que la identidad de sesión **no** es frontera; y la
  superficie operacional crece (registry, quota, prune, re-key, probe) donde
  A no agrega nada de eso. El usuario pierde el «borro la carpeta del repo y
  listo».

### Puntos transversales que los tres acuerdan

1. El nombre `.graphium` para el directorio del libro está descartado en
   todas las opciones (colisiona tres veces); candidatas
   `.graphium-book/`, `.gx-book/`, o nada (C).
2. El filesystem remoto se **detecta y adapta** (journal mode), no se elude
   mudando la ruta: ninguna opción escapa de WAL-vs-nube por ubicación sola.
3. La identidad del libro nunca es el inode ni solo la ruta: raíz canónica +
   `HEAD` + hashes de contenido + `indexed_at` en cada respuesta.
4. El libro debe ser siempre **borrable sin consecuencias** — cualquier
   opción que lo vuelva irreemplazable queda descartada por definición.

## 9. P8 — decisión (2026-11-05)

El humano fijó el principio que decide: «al menos pienso que no deberia
estar en los archivos del repo, deberia ser ignorado, no deberia ser un
archivo que pueda participar en commits, la unica razon por la que la veo en
directorio es por ser un archivo unico en ese momento».

Por ese principio:

- **D4 — la ubicación del libro es la opción C**:
  `~/.graphium/books/<slug>-<16hex de sha256(raíz canónica)>/libro.sqlite`,
  con directorio por libro (contiene `-wal`/`-shm`), registry en
  `books/index.json`, re-key por `first_root_commit` para manejar `mv`,
  stamp `(HEAD, mtime del git index, hashes)` contra re-clones en la misma
  ruta, probe de arranque que pasa a `journal_mode=DELETE` en filesystem
  remoto o sincronizado, prune con cap en el idle-exit de 60 s del dueño, y
  `indexed_at` + `book_key` en cada respuesta. **RNF4 no necesita enmienda**
  y su cláusula de seguridad se fortalece: el agente no puede tocar ni
  falsificar el libro.
- Las opciones A y B quedan registradas en §8 con sus debilidades; B queda
  como fallback si algún día se quiere aislamiento por clon sin tocar el
  directorio del usuario. El D1 original (`<worktree>/.graphium/`) queda
  **reemplazado** por D4; el resto de D1 (una base por worktree, identidad
  por raíz canónica) sobrevive en D4 vía el hash de la raíz.
- Consecuencia explícita: el plumbing de `session.rs` (proyecto de la
  sesión → ruta canónica del worktree) pasa a ser load-bearing del routing
  del libro, y el análisis de multi-proyecto (P7) hereda la recomendación:
  libros por raíz canónica, sin consultas cross-proyecto en v1.

## 10. Borrador del schema v1 (propuesta, no contrato)

Idea B del análisis de schema (§7), sobre D4 (`~/.graphium/books/<key>/`).
Sin CST, sin contenidos de archivo, sin cierres transitivos materializados,
sin tabla de file-edges (el mapa es `GROUP BY` on-the-fly).

```sql
-- PRAGMA user_version = 1   (migración: mismatch → delete-and-reindex;
--                             el libro es derivado, no hay escalera)

CREATE TABLE meta (
  key   TEXT PRIMARY KEY,   -- 'schema_version', 'worktree_root' (canónica),
                            -- 'git_dir', 'head_sha', 'first_root_commit',
                            -- 'created_at', 'nonce', 'owner_uid', 'lang_pack'
  value TEXT NOT NULL
);  -- marcador trust-on-first-byte (§8-A): sin fila 'nonce' válida
    -- la base se ignora y se reconstruye, nunca se abre en escritura

CREATE TABLE file (
  id           INTEGER PRIMARY KEY,
  path         TEXT NOT NULL UNIQUE,  -- relativa a la raíz del worktree, '/'
  lang         TEXT NOT NULL,         -- 'rust' | 'typescript' | 'python' | 'go' | 'java'
  content_hash TEXT NOT NULL,         -- sha256 de los bytes al indexar
  size         INTEGER NOT NULL,
  mtime_ns     INTEGER NOT NULL,
  indexed_at   TEXT NOT NULL          -- ISO-8601 UTC; viaja en toda respuesta
);

-- Los ids de symbol se asignan en bloques contiguos por archivo (un capítulo
-- es un rango): borrar un archivo es un BETWEEN, y las estadísticas por
-- capítulo no necesitan GROUP BY sobre la tabla grande.
CREATE TABLE symbol (
  id         INTEGER PRIMARY KEY,
  file_id    INTEGER NOT NULL REFERENCES file(id) ON DELETE CASCADE,
  parent_id  INTEGER REFERENCES symbol(id),  -- contención (árbol del archivo)
  kind       TEXT NOT NULL,  -- 'fn'|'method'|'type'|'class'|'trait'|'impl'|'module'|'const'
  name       TEXT NOT NULL,  -- nombre simple: 'render_with'
  qname      TEXT NOT NULL,  -- calificado: 'gx_mcp::tools::render_with'
  start_line INTEGER NOT NULL, start_col INTEGER NOT NULL,
  end_line   INTEGER NOT NULL, end_col   INTEGER NOT NULL,
  start_byte INTEGER NOT NULL, end_byte INTEGER NOT NULL
);
CREATE INDEX symbol_file   ON symbol(file_id);
CREATE INDEX symbol_name   ON symbol(name);
CREATE INDEX symbol_qname  ON symbol(qname);
CREATE INDEX symbol_parent ON symbol(parent_id);

CREATE TABLE edge (
  id       INTEGER PRIMARY KEY,
  src_id   INTEGER NOT NULL REFERENCES symbol(id) ON DELETE CASCADE,
  dst_id   INTEGER REFERENCES symbol(id),  -- resuelta solo si única; NULL si ambigua
  dst_name TEXT NOT NULL,                  -- el objetivo textual, SIEMPRE presente
  kind     TEXT NOT NULL,                  -- 'calls'|'imports'|'inherits'|'uses'|'type_ref'
  exact    INTEGER NOT NULL,               -- 1 = resuelta única; 0 = heurística (exact:false)
  file_id  INTEGER NOT NULL REFERENCES file(id) ON DELETE CASCADE  -- dónde se halló
);
CREATE INDEX edge_src      ON edge(src_id);
CREATE INDEX edge_dst      ON edge(dst_id);
CREATE INDEX edge_dst_name ON edge(dst_name);   -- la resolución por nombre es el join caliente
CREATE INDEX edge_file     ON edge(file_id);

-- FTS5 con contenido EXTERNO sobre symbol (symbol.id es rowid):
--   DELETE FROM symbol_fts WHERE rowid IN (SELECT id FROM symbol WHERE file_id=?)
--   y SIEMPRE borrar acá antes que en symbol; external-content no soporta
--   REPLACE: el ciclo por archivo es DELETE → INSERT, jamás REPLACE.
-- El texto de búsqueda es DERIVADO al indexar (unicode61 no parte camelCase):
--   search_text = 'renderWith render with RenderWith'  (variantes partidas)
-- tokenchars '_' mantiene snake_case entero. Tradeoff aceptado (§7): sin
-- texto almacenado no hay snippet()/highlight() — el file:line sale de symbol.
CREATE VIRTUAL TABLE symbol_fts USING fts5(
  name, qname, search_text,
  content='symbol', content_rowid='id',
  tokenize='unicode61 tokenchars _'
);
```

### Cómo se sirven las tres tools con este schema

- **`code_search(q)`** → `symbol_fts MATCH` (frase, no AND: los
  identificadores son frases de tokens derivados) → join a `symbol`/`file`
  → `{file:line, kind, qname, content_hash, indexed_at}`. Ambiguo →
  `candidates[]` con código de ambigüedad (P3, condición 2).
- **`code_structure(file|symbol)`** → `symbol WHERE file_id=?` ordenado por
  spans (el árbol de contención lo reconstruye el app con `parent_id`).
- **`code_impact(symbol)`** → BFS **en Rust**, no `WITH RECURSIVE`: cada
  salto es un `SELECT` indexado por `dst_id` (o `dst_name` para las
  `exact=0`), con visited-set en memoria (guardia de ciclos explícita —
  SQLite no documenta límite de iteración), `depth ≤ 3` y `LIMIT` por nivel;
  budget de respuesta fijado a los techos del kind `graph` (300/600) con
  paginación por cursor.
- **Mapa de archivos (P6)** → `SELECT f1.path, f2.path, COUNT(*) FROM edge
  JOIN symbol src ... JOIN file f1, f2 ... GROUP BY` on-the-fly; sin tabla
  materializada en v1 (se memoiza más adelante si el costo lo justifica).

### Presupuesto y límites declarados

- ~150 MB para 5k archivos (estimación del análisis de schema, no medición).
- Toda mutación: `BEGIN IMMEDIATE` + `SAVEPOINT` por archivo (un parseo malo
  cuesta un capítulo, no el lote), lotes de ≤32 archivos.
- `journal_mode` decidido en el arranque por el probe (§9-D4): WAL solo si
  el root no es remoto/sincronizado; si no, `DELETE` + aviso.
- Lo que **no** está y por qué: contenidos de archivo (el libro no duplica
  el código), CST completo (§3 descarte 2), file-edges materializadas
  (GROUP BY basta en v1), closure transitivo (depth ≤ 3 explícito).

### 10.1 — Schema v1.1: deltas aplicados tras la revisión (2026-11-05)

Tres revisores independientes (normalización, escalabilidad, mecánica SQLite)
sobre el borrador de §10. Los deltas que sobreviven a los tres:

**Corregidos (eran errores, no elecciones):**

1. **`search_text` es columna real de `symbol`** (`TEXT NOT NULL`, derivado
   al indexar). Con contenido externo, FTS5 lee los valores de las columnas
   **desde la content table** para borrar tokens y para `rebuild`: una
   columna FTS sin contraparte rompe el borrado, el recovery y el ranking a
   la vez. Corrección de trio: normalización, escalabilidad y mecánica la
   marcaron por separado. Bonus: con contenido externo `snippet()`
   **sí funciona** (los valores salen de `symbol`) — el tradeoff «sin
   snippet» solo valía para contentless.
2. **`edge.exact` eliminado**: dependencia funcional con
   `dst_id IS NOT NULL`; el branch heurístico es `WHERE dst_id IS NULL`. El
   fallo de mantenerla es un diagrama más corto sin error — silent
   under-approximation en `code_impact`.
3. **`edge.file_id` eliminado** (dependencia transitiva de `src_id`); el
   mapa de archivos (P6) lo calcula por joins. Optimización diferida: si el
   GROUP BY del mapa midiera lento, una columna `dst_file_id` mantenida por
   el escritor único lo convierte en un covering-index scan — se decide con
   medición, no por fe.
4. **`dst_id INTEGER REFERENCES symbol(id) ON DELETE SET NULL`** +
   `PRAGMA foreign_keys=ON` **explícito** (el default es OFF, y rusqlite
   *bundled* compila el default contrario a un sqlite de pkg-config: se fija
   en código, no se hereda). Sin esto, borrar un capítulo aborta el lote o
   deja huérfanos silenciosos.
5. **Tokenizer sin `tokenchars '_'`**: añadirlo **elimina** el split gratis
   de snake_case (`_` es separador por default). El texto de búsqueda
   derivado (`renderWith → "render with renderWith"`) más matching por
   AND de tokens con filtros de columna (`{name qname} : x`), nunca frases
   cosidas; `prefix='2 3'` si se quiere `ren*`; `ORDER BY rank` (no bm25
   directo) y **sin** `columnsize=0` (rompe bm25); la entrada de MATCH se
   escapa siempre.
6. **Colación unificada**: FTS es case-insensitive pero
   `dst_name = symbol.name` es BINARY — se agrega `symbol.name_norm`
   (normalizado, indexado) y **toda** resolución de edges corre por ahí, o
   `code_search` y `code_impact` resuelven distinto el mismo nombre.
7. **`indexed_at` INTEGER** (unixepoch ms) junto a `mtime_ns`; ISO-8601 solo
   en el borde de la tool. `UNIQUE(file_id, qname)` y `CHECK`s de kinds y
   spans ordenados.
8. **El esquema de bloques de ids contiguos se elimina**: sin AUTOINCREMENT
   el rowid se reutiliza (`max+1`) y el rango puede aliasear un archivo
   borrado con uno nuevo — el invariante miente. Borrado por
   `WHERE file_id=?`, ids monótonos.
9. **Sincronización FTS por triggers documentados**, no por
   `DELETE ... WHERE rowid IN (SELECT ...)`: el trigger con el comando
   especial `('delete', old.id, old.name, old.qname, old.search_text)` es la
   única operación de fila única garantizada (los valores pasados tienen que
   coincidir con lo indexado). Recovery: `'rebuild'` (relee `symbol`, no hay
   reparseo); integridad FTS con `('integrity-check', 1)` — el `1` es
   requerido contra contenido externo; `.recover` no existe en rusqlite
   bundled ⇒ **delete-and-reindex es la recuperación final** (confirmado).
10. **`WITHOUT ROWID`: no** — `id INTEGER PRIMARY KEY` ya es rowid alias
    (clusteriza), y en `edge` es imposible (`dst_id` es nullable; WITHOUT
    ROWID fuerza NOT NULL en todo el PK).

**Decisiones de operación:**

- **Tamaño real**: 50k archivos ≈ **8–11 GB** (no 1–5 GB); tuning para 10 GB.
- **Pragmas**: `page_size=4096` **antes** de crear tablas y antes del probe
  (solo aplica en archivo vacío); `synchronous` **sigue al probe**
  (WAL ⇒ NORMAL, DELETE ⇒ FULL); `wal_autocheckpoint=0` + checkpoints
  PASSIVE solo con la cola del indexer vacía + `TRUNCATE` en idle-exit;
  `journal_size_limit=64MB`; `auto_vacuum=INCREMENTAL` solo en creación;
  `cache_size=-65536`; `mmap_size=256MB` (solo lectores); `temp_store=MEMORY`;
  `busy_timeout=5000` + transacciones de lectura cortas (una `Connection` no
  es `Sync`: Mutex o conexión por thread); `analysis_limit=400` +
  `PRAGMA optimize` al cerrar.
- **Lotes acotados por bytes** (≤2 MB de fuente; `cache_spill` toma lock
  EXCLUSIVE), co-localizados por `(dir, path)`, size-ascending dentro del
  dir; parseo paralelo con **commits seriales**; backpressure (1 thread de
  parseo mientras hay un render en vuelo, ≤4 total); `meta.progress` para el
  «indexing 40%». First index de 50k: **45–90 s** (medir en C0).
- **Cold start sin `integrity_check`** (6–10 min a 10 GB): probe de `meta`
  (<10 ms) + `user_version`/`nonce`/root/head; `quick_check` solo tras
  shutdown sucio; FTS `integrity-check` puntual; desync de índice ⇒
  `'rebuild'`; lo demás ⇒ delete-and-reindex.
- **BFS**: ~3 statements por nivel (out-hop por `src_id IN`, in-hop por
  `dst_id IN`, resolución de ambiguos con `GROUP BY name HAVING COUNT(*) ≤ 5`),
  ~9 a depth 3; `IN` por chunks ≤500 (límite 32766); los
  `dst_id IS NULL` **no se resuelven más allá del salto 1** (el fan-out de
  nombres genéricos es el costo real); índices
  `symbol(file_id, start_byte)` y `edge(src_id, dst_id, kind)` para hops
  index-only.
- **Imports**: un `symbol` sintético `kind='module'` por archivo
  (`qname` = module path) y los specifiers resuelven a él — sin tabla nueva,
  sin envenenar el % de resolución.

**Lo que quedó como estaba (defendido por dos o tres revisores):**
`dst_name` NOT NULL (procedencia), `qname` inline + `UNIQUE(file_id, qname)`,
spans inline, `lang` TEXT+CHECK, sin tabla `unresolved_edge`, sin
file-edges materializadas en v1, delete-and-reindex como recovery final.
