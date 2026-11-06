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
  análisis en §8):** RNF4 (PRD §7 «No funcionales», `docs/PRD.md:301`) limita
  las escrituras de Graphium a `~/.graphium/`; la base en el worktree exige
  enmienda y el nombre `.graphium` colisiona con el directorio de datos del
  usuario, la clase CSS de los exports y `graphium.json` (RF21). Las tres
  opciones fueron defendidas por un explore cada una y su análisis completo
  está en §8. **Decisión pendiente del humano.**
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
