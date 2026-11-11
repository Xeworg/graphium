# f27-etapa1

Etapa 0 (resto) and the cheap rows of Etapa 1 of `docs/CATALOGO-DIAGRAMAS.md`:
the shared vocabulary of a typed relation, the stereotype on ordinary flow
boxes, and the three cheapest rows (DFD number and level, ERD per-end minimum,
tree level and identifier). No new kind enters in this cut. Branch
`feature/f27-etapa1`, base `staging` @ 74d46d2.

## Specs

S1 (owner, verbatim): "Plataforma y lo barato (recomendado)" — the selected
option reads: "f27: EdgeKind (9 relaciones) + estereotipo dibujado en flow + las
3 filas más baratas (dataflow número/nivel, er cardinalidad mínima/opcionalidad,
tree meta/nivel). Cierra filas y desbloquea component, use_case y activity." The
new kinds (`pert`, `use_case`, `activity`, `deployment`) do **not** enter this
cut.

S2 (catalogue R2, verbatim): "**`edge.kind`: vocabulario de relación tipada.**
Hoy ningún kind tiene tipo de arista (solo `er` tiene cardinalidad y `sequence`
tiene `reply`); el vocabulario vive en el texto libre de `label`
(`crates/gx-kinds/src/flow.rs:41`). Lo piden el mapa de componentes, el activity,
el use case («include»/«extend») y el deployment (protocolos). Se implementa una
vez, como campo opcional con vocabulario por kind, y el motor lo ignora para
dibujar."

S3 (catalogue, remainder of Etapa 0, verbatim): "**Falta de la etapa 0:** el
estereotipo dibujado en las cajas de `flow` (vocabulario de nodo, no la ranura de
arista) y el tipo compartido `EdgeKind` para el vocabulario de relación tipada,
que entra con las nueve relaciones de `component` en la etapa 1."

S4 (catalogue row 16, verbatim): "`[CONTRACT]` número de proceso (P1, P2):
`FlowNode` no tiene campo de número (`crates/gx-kinds/src/flow.rs:22-36`); hoy va
escrito a mano en la etiqueta y ninguna regla lo revisa." … "`[CONTRACT]` nivel:
`DataflowInput` no tiene campo (`dataflow.rs:39-59`), así que contexto, nivel 0 y
nivel 1 son documentos sin relación declarada" … "`[SEMANTICS]` numeración única
y niveles coherentes: sin número no hay nada que validar; la regla llega con el
campo." Cost, verbatim: "el más chico de la sección: dos campos, una variante de
dibujo y dos reglas. Motor y layout no cambian; cada nivel es un documento
aparte."

S5 (catalogue row 8, verbatim): "`[SEMANTICS]` participación total/parcial más
allá del booleano `nullable`: el mínimo solo puede ser 0 o 1 y solo se declara en
el extremo destino (`crates/gx-core/src/ir.rs:300`)." Etapa 1's row, verbatim:
"cardinalidad mínima y opcionalidad en el contrato, no en el texto de la
etiqueta".

S6 (catalogue rows 13 and 18, verbatim): "`[CONTRACT]` `meta` por nodo: el canal
existe y hoy nadie lo usa (`IrNode.meta`, `ir.rs:181`, llega intacto a la escena,
`scene.rs:251`)." / "`[CONTRACT]` niveles como dato: `tree` ya los calcula y los
descarta." / "`[CONTRACT]` identificador por nodo (1, 1.1, 1.1.2): `TreeNode` no
tiene campo (`tree.rs:27-35`); la ruta es el id interno, no un número." / Cost,
verbatim: "aditivo y sin renderer si los niveles van como texto: `meta`, nivel y
número en el contrato de `tree`, una regla de coherencia. Las bandas de nivel sí
tocan el motor."

S7 (the golden promise of this cut): every existing golden stays
**byte-identical**. A document that writes none of the new optional fields
produces today's bytes. Two new scenes prove the new drawing:
`dataflow-levels` (process numbers and a declared level) and `tree-numbered`
(identifiers and levels).

S8 (reserved keys): `level` and `number` inside a node's `meta` are **derived**,
so a document that writes them is rejected with an error naming the key; there is
no silent precedence between a derived value and a written one.

## Decisions

D1 (what `EdgeKind` is, and what it is not). A shared row type in `gx-core` —
the shape `class` already carries privately since f26: a relation name, the
`head`/`tail` adornments it draws, the `Style`, and the label it implies. The
kind declares its table (`RELATIONS` becomes the shared type) and the contract
teaches it from there. **The `kind` field on an edge document does not enter
this cut**: S3 gives the type to Etapa 0 and the field to the nine relations of
`component`, and a field no kind can yet use is generality nobody asked for.
This task is therefore a refactor with no document change, and its proof is that
every golden stays byte-identical.

D2 (one resolution point, deferred with the field). When the `kind` field lands
with `component`, an edge that writes both a `kind` and explicit `head`/`tail`
will not fight: the row wins and the explicit glyph is ignored, the same
single-resolution rule f26 imposed on `cardinality`. What this cut delivers is
the one place that decision will read — `EdgeKind::find` over the kind's table —
so the precedence rule is written once when its consumer arrives.

D3 (stereotype placement). `FlowNode.stereotype: Option<String>` reaches
`IrNode.stereotype` untouched; the engine, the renderer, the CSS and the JS twin
already draw it for `class`, so this is a document field and a validation rule,
not a drawing change. The vocabulary is bounded: 1 to 32 characters, and the
same normalisation `class` applies (the word is drawn in `«»`).

D4 (DFD number). `DataflowNode` replaces `FlowNode` inside the `dataflow`
document only, so `number` exists where it means something. Format `P` + digits,
unique across the document, and drawn as the label prefix (`P1 · label`) because
the Gane-Sarson band is a renderer change this cut does not buy; the lack of the
band is recorded, not discovered.

D5 (DFD level). `DataflowInput.level: Option<u32>` in `0..=6` (context is 0). A
level is a property of the whole document, so it has no node of its own: it
reaches the drawing as `IrNode.meta["level"]` written on every node of that
diagram (the channel S6 says already reaches the scene), and it is taught by the
kind's rule line. Coherence rule: the context level admits no numbered process
(the context diagram has no process to number), a document above 0 does.

D6 (ERD minimum, per end). `ErRelation` gains `from_min` and `to_min`, each
`"required" | "optional"` (the closed vocabulary of `CardinalityBound`). Defaults
keep today's bytes: `from_min` is `Required`, `to_min` follows the target
column's `nullable`. An explicit value overrides the derivation, and the rule
line teaches the precedence.

D7 (tree meta, level, number). `TreeNode.meta: Option<BTreeMap<String,
IrValue>>` is copied to `IrNode.meta` verbatim (minus reserved keys, S8);
`level` (depth, 0-based) and `number` (`1`, `1.1`, `1.1.2`) are derived from the
path and written into the same map. `TreeInput.numbering: Option<bool>` (default
false) prefixes the label with the number; with it off, the drawing is today's.

## Tasks

T1 `gx-core/src/kind.rs`: the `EdgeKind` row type lifted out of `class`
(`Relation`, with `is_a` renamed `specialization`), `DiagramKind::EDGE_KINDS`
and `KindHandler::edge_kinds()`, and `class` migrated onto it. Test first: RED on
the table being reachable through the object-safe handler and on the rule line
still being derived from it. Goldens byte-identical is the refactor's proof.
inline. commit: `refactor(core): the relation vocabulary is shared, not class's own`.
T2 `gx-kinds/flow.rs`: `FlowNode.stereotype`, its validation and its trip to
`IrNode.stereotype`; `class` stops being the only kind that can write one. Test
first: RED on the length rule and on the round trip. inline.
commit: `feat(kinds): a flow box can carry the stereotype it draws`.
T3 `gx-kinds/dataflow.rs`: `DataflowNode.number` and `DataflowInput.level` with
their two rules, the label prefix, and the `dataflow-levels` scene + golden. Test
first: RED on duplicate numbers, on the format and on the level coherence (D5).
inline. commit: `feat(dataflow): a process carries its number and the diagram its level`.
T4 `gx-kinds/er.rs`: `from_min`/`to_min` on a relation, the precedence rule and
the derived default. Test first: RED on `from_min: "optional"` reaching the
source end and on the explicit value beating `nullable`. inline.
commit: `feat(er): the minimum of each end is declared, not implied`.
T5 `gx-kinds/tree.rs`: `TreeNode.meta`, the derived `level`/`number`,
`numbering`, S8's rejection, and the `tree-numbered` scene + golden. Test first:
RED on the reserved-key rejection, on the derived numbers and on the byte
identity with `numbering` absent. inline.
commit: `feat(tree): levels, numbers and metadata reach the drawing`.
T6 gates and docs: `cargo test --workspace`, fmt, clippy, the two harnesses
(parity, equivalence), `docs/PRD.md` §6/§10, `docs/CATALOGO-DIAGRAMAS.md` (§3
rows, §5 the closed gaps, §6 Etapa 0 done and Etapa 1 progress) and
`CONTRACT.md` if the frontend contract moves. commit: `docs(odd): f27 records the platform and the three rows it closed`.

## Log

L1 (owner, verbatim): "dale haz pr y luego continuamos con el cambio nuevo de la
revision de diagramas y el agregado de nuevos diagramas"
L2 (owner, verbatim): "Sí: FF main, converger staging, borrar la rama" — the
closing hygiene of f26 executed before this cut: `main` at b6bf154, `staging`
converged at 74d46d2 and pushed.
L3 (owner selected): "Plataforma y lo barato (recomendado)".
