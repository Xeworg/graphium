# F9 — el libro del proyecto (Tree-sitter + SQLite + FTS5)

Estructura completa del proyecto indexada en una base SQLite con búsqueda
FTS5: el «libro abierto» que le da al agente conexiones y diagramas baratos
(sin leer código) y al humano un mapa del proyecto. Toda la base del diseño
está en `odd/ideas/f9-libro-del-proyecto.md` (análisis con seis rondas de
exploradores: puerta al agente, schema, consultas, mapa/multi-proyecto,
ubicación, y revisión triple del schema v1.1). Este documento convierte ese
análisis en specs y tareas; los § que cita viven en el doc de ideas.

## Specs

- S1 (pedido original, verbatim): «quiero que agregemos soporte mediante
  treesitter, guardar ese parse en una base de datos sqlite, con la itencion
  de tener una estructura complet del proyecto y usando fts5 hacer una
  busqueda acereleraday mas precisa de la base de datos, la itencion tener
  un libro abierto del proyecto, tener coexiones y ampliar la posblidadde
  los diagramas, que mcp pueda birndar al agente para generar diagramas mas
  baratos, agregaremos 5 lenguajes primero e iremos agregando mas en grupos
  hasta tener los mas usados, este espec se puede trabajar en un worktree en
  una rama aparte hasta que sea estable y funcional, cosas a tener en cuenta,
  si el usuario usa worktrees, como mantendremos el programa en el proyecto,
  seria mejor tener una base de datos en la ruta principal del proyecto se
  debe mantener siempre el monitor del proyecto».
- S2 (ubicación de la base, D4, principio del humano verbatim): «al menos
  pienso que no deberia estar en los archivos del repo, deberia ser
  ignorado, no deberia ser un archivo que pueda participar en commits, la
  unica razon por la que la veo en el directorio es por ser un archivo unico
  en ese momento» → `~/.graphium/books/<slug>-<16hex de sha256(raíz
  canónica)>/libro.sqlite` (ideas §9).
- S3 (lenguajes, D2): Rust, TypeScript/TSX (cubre JS), Python, Go, Java — en
  grupos, no todos de golpe (ideas §7 D2).
- S4 (exclusiones, D3, verbatim): «si ignoraremos lo mismo que git para no
  desperdiciar los recursos del usuario» — el watcher respeta `.gitignore`
  (más `.git` y `.graphium` duros) (ideas §7 D3).
- S5 (puerta al agente, síntesis P3, ideas §7): tres tools question-shaped
  (`code_search`, `code_structure`, `code_impact`) más una que arma el
  documento `graph` (`code_diagram`), todas **return-document**; sin kind
  nuevo; detrás de un flag por proyecto (off por defecto).
- S6 (schema, ideas §10.1): schema v1.1 tras revisión triple — tablas
  `meta`/`file`/`symbol`/`edge` + `symbol_fts` external-content con
  triggers; sin CST, sin closure materializado, sin file-edges materializadas
  en v1; `PRAGMA user_version` con delete-and-reindex como recovery.
- S7 (consultas, ideas §7 y §10.1): resolución por etapas — FTS5 → candidatos
  rankeados → rechazo ante ambigüedad (`candidates[]`) → BFS acotado en Rust
  con guardia de ciclos explícita, depth ≤ 3, budget 300/600 del kind graph,
  paginación por cursor. Toda respuesta lleva `file:line`, `content_hash`,
  `indexed_at` y `book_key`; las edges sintácticas nacen `dst_id IS NULL`
  (heurísticas) y se resuelven por `name_norm`.
- S8 (proceso, S1 verbatim parcial): «este espec se puede trabajar en un
  worktree en una rama aparte hasta que sea estable y funcional» — cada
  corte en su worktree y rama propia; integración feature → `staging` → PRs
  encadenados → `main` (decisión L3 de `f2-cierre.md`); revisión y prueba
  del humano antes del merge.
- S9 (monitor, S1 + D3): el monitoreo arranca con el dueño, con presupuesto
  (lotes por bytes, parseo paralelo con commits seriales, backpressure contra
  el render queue, `meta.progress`) y flag para apagarlo por proyecto.

### Derivados

- D1 (índice derivado): la base es un caché reconstruible — cualquier
  recuperación es delete-and-reindex; nada del libro es fuente de verdad.
- D2 (ruido): `.gitignore`-respetado; `target/`/`node_modules` implícitos;
  archivos binarios y > tope de tamaño se saltan con aviso.
- D3 (relaciones honestas): son sintácticas, no de tipos; el % de resolución
  se publica por lenguaje (referencia: GraphPilot declara 25–35 %).
- D4 (múltiples proyectos, P7): libros por raíz canónica; el routing lee el
  **registro de la sesión** (nunca `current_dir()`); sin consultas
  cross-proyecto en v1.

## Tasks

- C0 (S6, S7, S3-Rust, S4, S9, D1; spike): crate `gx-index` (rusqlite +
  FTS5 + watcher + grammar Rust), schema v1.1 completo, dentro del dueño,
  y la tool `code_search` con su flag. **Criterio de salida medible**:
  buscar un símbolo de este repo responde en <50 ms; el watcher sigue a los
  edits sin reconstruir todo; `cargo test --workspace` verde con el nuevo
  crate. Worktree: `graphium-f9-libro`, rama `feature/f9-libro-c0`.
- C1 (S5, S7, D3): relaciones (`calls`/`imports`/`inherits`/`uses`) para los
  5 lenguajes, `code_structure`, `code_impact` (BFS con guardias) y
  `code_diagram` que arma el documento `graph`.
- C2 (S3, D3): grupos de lenguajes siguientes + % de resolución publicado
  por lenguaje.
- C3 (S2, S8, D4): pulido de worktrees, prune con cap en idle-exit,
  `graphium books list|prune`, hooks opcionales.

## Log

- L1 (2026-11-05): idea original del humano (S1) y decisión de analizarla
  solo con un documento (S2 del doc de ideas).
- L2 (2026-11-05): tras seis rondas de análisis (ideas §7-§10.1) el humano
  aprueba convertir la idea a feature doc y arrancar el spike C0 en
  worktree propio («listo puedes trabajarlo con wortrees y hacerle merge al
  staggin»), con el merge previo del corte A de `f8-planos.md` a `staging`.
