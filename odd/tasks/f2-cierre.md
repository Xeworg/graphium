# F2 — cierre: export con clusters/emphasis, replace_id, historial (RF14)

Cola de F2: el export se construyó antes de que los clusters anidados
existieran, `opts.replace_id` está prometido en el contrato y RF14 sigue sin
implementar. Este corte cierra esas tres colas. La parte de UI de RF14
(flechas en la ventana) queda fuera: `gx-app/frontend` está ocupado por el
corte F6 en paralelo; se hace después de su merge.

## Specs

- S1 (estado registrado, 2026-11-04): «Pendiente: F2 restante (export de
  clusters/emphasis, opts.replace_id, RF14 historial)».
- S2 (PRD §7 RF14, verbatim): «**RF14.** Historial de la sesión (flechas en la
  ventana) — por diagrama.»
- S3 (PRD §12 backlog fila 2, verbatim — resolución de RF14): «Definir como
  navegación de versiones (cada render/update crea una versión, tope 20 por
  diagrama, solo lectura, indicador «v3/7»); el undo de un nivel ya está
  acotado en RNF10». Se adopta: versiones = documentos fuente (baratos en
  memoria), el undo/redo de un nivel de RNF10 queda como está.
- S4 (PRD §7, verbatim): «Para actualizar en vez de acumular: `update_diagram`
  (F2) o `opts.replace_id` (si no existe → `unknown_diagram`)».
- S5 (PRD §7 RF1, verbatim): «Un segundo `render_diagram` **crea un diagrama
  nuevo, cierra el anterior** y responde `{ ok, diagram_id, replaced:
  "<id anterior>" | null }`» — `replace_id` reemplaza al diagrama nombrado en
  vez de crear uno nuevo; la respuesta reporta el `diagram_id` afectado.
- S6 (restricción del humano): «de esos worktrees los agentes no pueden hacer
  merge de la rama lo haremos luego de revisar y probar nosotros» — el worker
  no commitea ni merguea; deja el árbol listo para revisión. Integración
  (decisión del 2026-11-05): feature → `staging` (revisión y prueba ahí) →
  PRs encadenados → `main`; nunca directo a `main`.

### Derivados

- R1 (export de clusters/emphasis): golden de export SVG y PNG de un diagrama
  con grupo anidado y con emphasis activo (`focus`/`extract`): el SVG exportado
  contiene las cajas de grupo con su profundidad y las clases de emphasis; si
  el export pierde o deforma algo, se arregla en `gx-render-svg`.
- R2 (`opts.replace_id`): `render_diagram` con `opts.replace_id` reemplaza el
  documento del diagrama nombrado (mismo ciclo que un `update` + re-render);
  si el id no existe → error `unknown_diagram` (nada se crea); sin
  `replace_id` el comportamiento actual no cambia.
- R3 (RF14 backend): el store guarda por diagrama las versiones del documento
  fuente que produjeron renders/updates (tope 20, la más vieja se descarta),
  con número de versión; superficie de lectura MCP para listar versiones y
  obtener el documento de una versión (solo lectura; una versión no se puede
  restaurar con una tool — restaurar es un render/update nuevo).
- R4 (RF14 UI): las flechas en la ventana y el indicador «v3/7» se
  implementan en T5, después del merge de F6 (que ya entró en main:
  `2b55a48`). Decisión del humano (2026-11-05): **flechas en el ítem del
  panel de diagramas + preview renderizado** — cada ítem gana «◀ v3/7 ▶»;
  navegar dibuja la versión vieja sobre el objeto del plano en su mismo
  lugar (solo vista, sin mover el objeto ni escribir el store); a la última
  versión se vuelve a ver el diagrama vivo; con diagrama seleccionado,
  ←/→ también navegan.
- R5 (coexistencia con F6): F6 edita en paralelo `crates/gx-core/src/render.rs`
  y `export.rs` (paleta efectiva). Este corte NO edita esos dos archivos: si
  una corrección los requiere, se reporta y queda para después del merge de
  F6. Los rozos posibles en `crates/gx-mcp/**` y `docs/PRD.md` son aditivos y
  en regiones distintas; el conflicto se resuelve en el merge manual.
- R6: docs: PRD actualizado (RF14 resuelta según S3, `replace_id` sale de
  «llega en F2/F3» a implementado; `interactive` queda F3), `harness.rs`
  (INSTRUCTIONS) y textos de herramienta coherentes con lo nuevo.

### Verificación

- R7: suite completa, clippy y fmt verdes; tests nuevos para R1 (goldens), R2
  (reemplazo, `unknown_diagram`, y que el flujo sin `replace_id` no cambia) y
  R3 (tope 20, descarte de la más vieja, solo lectura).
- R8: revisión en vivo del humano antes del merge (S6): no commitear, no
  merguear.

## Tasks

- T1 (R1, R7): goldens de export con clusters anidados y emphasis; fixes en `gx-render-svg` si algo se deforma. Commit: `96422bb`. No hicieron falta fixes: render_export reusa render_with y las cajas/clases salen completas.
- T2 (R2, R7): `opts.replace_id` en store/dispatch/contract/tools con tests. Commits: `36e0110` + `a91f355`.
- T3 (R3, R7): versiones del documento en el store (tope 20) + superficie MCP de lectura con tests. Commits: `36e0110` + `a91f355`.
- T4 (R6): docs (PRD, harness INSTRUCTIONS, textos de tools) y suite completa verde. Commit: `dd0c89c`. Evidencia: fmt + clippy -D warnings + test --workspace (32 binarios ok).
- T5 (R4): UI de RF14 (flechas «◀ v3/7 ▶» en el ítem del panel + preview
  renderizado en el plano, ←/→ con diagrama seleccionado). Commits:
  `1e01c02` (preview render-only en gx-mcp) + `d15e65a` (ventana: estado,
  puente, panel, teclado, tests de contrato).

## Log

- L1 (2026-11-05): el humano retoma: «perfecto ahora que tenemos pendiente,
  recuerdo que hay pendiente de f2 aun».
- L2 (2026-11-05): secuenciado con F6 en paralelo: RF14 UI queda para después
  del merge de F6; `render.rs`/`export.rs` de gx-core fuera de alcance (R5).
- L3 (2026-11-05): el humano fijó la integración: feature → `staging` →
  PRs encadenados → `main`; nunca directo a `main`.
- L4 (2026-11-05): implementación completa del worker (sin commit); el
  orquestador la empaquetó en cuatro unidades: golden (`96422bb`), store
  (`36e0110`), contrato MCP (`a91f355`), docs (`dd0c89c`).
- L5 (2026-11-05): T5 desbloqueado (F6 ya está en main). El humano eligió la
  UI entre tres opciones: «Flechas en el panel + preview» — ítem con
  «◀ v3/7 ▶», la versión vieja se dibuja sobre el objeto del plano (solo
  vista: el objeto no se mueve, el store no se escribe, ninguna versión se
  restaura), en la última versión se ve el vivo, y ←/→ con diagrama
  seleccionado navegan igual que las flechas del panel. Las alternativas
  descartadas fueron el control flotando sobre el objeto y la lista de
  versiones con visor del documento fuente.
- L6 (2026-11-05): implementación inline del orquestador. Verificación: RED
  observado (2 tests de gx-mcp sin la función; 9 de 10 de contrato de
  frontend sin el JS), después GREEN; `cargo fmt --check`, `cargo clippy
  --workspace --all-targets` y `cargo test --workspace` verdes (0 fallos,
  47 binarios). Chequeo diferencial del código compartido: `panelItem` y
  `materializeObject` conservan su comportamiento anterior (los tests de
  contrato de `panel.rs` y `culling.rs` pasan sin cambios); `insertDiagram`
  cambió a propósito — un re-render del mismo id (`opts.replace_id`,
  `update_diagram`) dejaba el contenedor viejo huérfano en el DOM, así que
  dos dibujos se superponían bajo un id; ahora el viejo sale y el fix quedó
  fijado por `a_re_render_of_the_same_id_replaces_the_object_and_the_view`
  (tests/versions.rs). Frontera de slice: la unidad de la ventana (`d15e65a`,
  617 líneas con sus tests) supera las ~400 porque código y tests del mismo
  comportamiento van juntos; la de backend (`1e01c02`, 110) queda debajo.
- L7 (2026-11-05): pendiente del humano: revisión y prueba en vivo de la TUI
  (mismo S6 que los cortes anteriores) e integración feature → `staging` →
  PRs encadenados → `main` (L3). El worktree anterior de f2-cierre ya no
  existe; esta unidad quedó en la rama `feature/f2-t5-versions-ui` del árbol
  principal, sin pushear.
