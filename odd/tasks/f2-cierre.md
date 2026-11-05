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
  no commitea ni merguea; deja el árbol listo para revisión.

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
- R4 (RF14 UI, fuera de este corte): las flechas en la ventana y el indicador
  «v3/7» se implementan después del merge de F6 (frontend ocupado). Queda
  anotado como tarea abierta para el siguiente corte.
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

- T1 (R1, R7): goldens de export con clusters anidados y emphasis; fixes en `gx-render-svg` si algo se deforma. Commit: pendiente.
- T2 (R2, R7): `opts.replace_id` en store/dispatch/contract/tools con tests. Commit: pendiente.
- T3 (R3, R7): versiones del documento en el store (tope 20) + superficie MCP de lectura con tests. Commit: pendiente.
- T4 (R6): docs (PRD, harness INSTRUCTIONS, textos de tools) y suite completa verde. Commit: pendiente.
- T5 (R4, abierto): UI de RF14 (flechas + «v3/7») — después del merge de F6.

## Log

- L1 (2026-11-05): el humano retoma: «perfecto ahora que tenemos pendiente,
  recuerdo que hay pendiente de f2 aun».
- L2 (2026-11-05): secuenciado con F6 en paralelo: RF14 UI queda para después
  del merge de F6; `render.rs`/`export.rs` de gx-core fuera de alcance (R5).
