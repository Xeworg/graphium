# f29-componente

Row 12 of `docs/CATALOGO-DIAGRAMAS.md` (system component map) and the debt Etapa 0
declared: the `kind` field on an edge document, which was deferred to its first
real consumer. That consumer is `component`: six roles, nine typed relations, and
the stereotype the roles have always meant. Branch `feature/f29-componente`, base
`staging` @ 78a9610.

## Specs

S1 (catalogue, Etapa 0 remainder, verbatim): "**Falta de la etapa 0:** el campo
`kind` en la arista —el vocabulario de relación tipada escrito en el documento—,
que entra con las nueve relaciones de `component` en la etapa 1; la etapa 0 dejó
la mesa puesta, no el campo."

S2 (catalogue, Etapa 1 row 12, verbatim): "seis roles nuevos + estereotipo
dibujado + las nueve relaciones tipadas de la etapa 0".

S3 (catalogue row 12 cost, verbatim): "barato para roles + estereotipo (dibujo y
contrato derivados); medio … cambia el documento (niveles)".

S4 (the promise this cut must keep, from `f26-adornos.md`): every existing golden
stays byte-identical, including `component-api`, because a document that writes no
`kind` is today's document. The new drawing gets its own witness,
`component-relations`: typed edges and the six new roles, so nothing that exists
today has to move for this cut to be visible.

S5 (one resolution point, from `f27-etapa1.md` D2): an edge that writes a `kind`
resolves its adornments, style and implied label from the kind's table and from
nowhere else; an explicit `head`/`tail` is not consulted when a `kind` is written.
A `kind` the kind's table does not declare is `InvalidEnum`, listing the names
that table admits.

S6 (a field nobody can use is a field that lies): a flow kind that declares no
relations rejects an edge that writes `kind`, instead of accepting it and ignoring
it. The vocabulary is per kind, and a kind without one says so.

## Decisions

D1 (where the field lives). `FlowEdge.kind: Option<String>` in
`crates/gx-kinds/src/flow.rs`, beside `arrow`, which it supersedes when written.
`arrow` stays: documents already carry it and it keeps its meaning where no
relation is declared.

D2 (how it resolves). `flow::to_graph` and `flow::validate_flow` take the kind's
table (`&[EdgeKind]`) as a parameter, so every flow kind walks the same single
resolution path and the compiler names every call site that must pass one. A kind
without a vocabulary passes `&[]`, and S6 turns that empty table into the
rejection of any `kind`.

D3 (the nine relations, named by the catalogue and not by me). Row 12 lists them
verbatim: `uses`, `calls`, `reads`, `writes`, `publishes`, `subscribes`,
`depends_on`, `hosts`, `connects_to`. **Every row names its line**: the document
may override the label, but no relation is silent, because four of them share a
glyph family and the word is what removes the ambiguity. The notation follows one
rule — a solid arrow is a directed action (`calls`, `writes`, `publishes`,
`hosts`), a solid line with no adornment is a symmetric touch (`uses`,
`connects_to`), a dashed arrow is a dependency (`depends_on`) and a dashed open
arrow is an observation (`reads`, `subscribes`). The table is the contract and the
rule line derives from it (the f26 S8 discipline), so a relation can never be
painted with a glyph the contract does not mention.

D4 (the six roles, also named by the catalogue). Row 12 names them verbatim:
`application`, `service`, `infrastructure`, `broker`, `storage`, `runtime`, on top
of the four that exist. The catalogue also settles the drawing: "no hace falta
forma nueva: un broker se lee por su palabra, no por un glifo" — so the six keep
the shared rounded box and write their own name in the stereotype slot
`f27-etapa1` opened, and the four existing roles keep their shapes and stay
silent, which is why no existing golden moves.

D5 (the boundary label is not invented). Level bands and the level vocabulary stay
out: the row closes with roles, relations and stereotype, and the catalogue records
that the levels remain open.

## Tasks

T1 the field and its resolution (`gx-kinds/src/flow.rs` + the call sites of
`to_graph`/`validate_flow` in every flow kind): `FlowEdge.kind`, the table
parameter, the single resolution, S6's rejection. Test first: RED on an unknown
relation, on a relation whose glyph reaches the edge, and on a kind without a
vocabulary refusing `kind`. commit: `feat(flow): an edge names the relation it draws`.
T2 `component`: the nine relations of D3, the six roles of D4, the stereotype per
role, and the rule line derived from the table. Test first: RED on each new role's
shape and on the nine relation rows painting what they declare. commit:
`feat(component): the map names its nine relations and draws its roles`.
T3 the witness and the gates: a `component-relations` scene and golden, the
`component-api` scene updated in the same commit that changes what it draws (S4),
`cargo test --workspace`, clippy, fmt, and the two frontend harnesses. commit:
`test(golden): the component map shows what its relations mean`.
T4 docs: `docs/CATALOGO-DIAGRAMAS.md` §3 row 12, §5 row 12 and §6 Etapa 0/1,
`docs/PRD.md` §10 for the `kind` field and the nine relations. commit:
`docs(odd): f29 closes the component map and the edge vocabulary`.

## Log

L1 (owner, verbatim): "continua con lo de los diagramas puedes seguir con eso?"
L2 (why this row first). The route's R1 and R2 put capacity before a new kind and
the platform before the catalogue; the platform piece Etapa 0 left undone is the
`kind` field, and `component` is the consumer it was deferred to. Rows with a
correctness defect — `swimlane` drawing two outputs of one decision as a sequence —
are the next cut, not this one, because they need a pass in the lane engine.
L3 (evidence and slice boundary). T1 and T2 landed in one commit, `dbfab3c`, not
two: the shared `to_graph`/`validate_flow` signature changes and `component` is a
call site, so separating them leaves either a commit that does not compile or one
that ships a field no kind can use. T3 is `bae5fd5` (the `component-relations`
scene, and every existing golden byte-identical, `component-api` included). The
same clippy rule the repo already honours forced the table into a
`FlowVocabulary` value instead of a seventh argument, with no `#[allow]`. T4
(docs) is still open. Verified in the worktree: `cargo test --workspace` green,
clippy clean, `cargo test -p gx-app --test golden_svg` four cases green. A fresh
worktree has no `crates/gx-app/frontend/dist`, so `tauri::generate_context!`
panics until the Vite bundle exists; the bundle of the same clone stands in
(f29 touches no frontend source).


L4 (surface of T4, and one correction that is not f29's). T4 writes two files:
`docs/CATALOGO-DIAGRAMAS.md` (§3 row 12, §5 row 12, §6 Etapa 0/1) and
`docs/PRD.md` (§10 for the edge `kind` field and the nine relations, §10.1 if
that is where the edge fields live). The `mixin C = Base with M` prose that f28
T4 found wrong is Dart and belongs to `f28-lenguajes` T5, not here: the catalog
has no mixin line, and two cuts editing the same file in different worktrees is
a merge conflict bought for nothing.

L5 (T4 landed, and the rows outside row 12 that the same cut made false). The
docs now match the shipped code in eight places, not four: the §2 table of what
this build has today (ten roles and `edges{kind?}` in the `component` row), §3
row 12's state, §5.1 row 6 — the UML component diagram shares the kind, so its
"Hoy" paragraph and its assembly-vs-delegation bullet were stale too: `edge.kind`
exists, the kind's table simply has no `provides`/`requires` relation yet —, §5.3
row 12 (Hoy, Falta, Costo), §6 Etapa 0 as closed by this cut, §6 Etapa 1 row 12 as
done, §6 R2's `edge.kind` bullet (the platform piece shipped and the engine now
draws from the table instead of ignoring it), and PRD §10, where the edge `kind`
is documented with its rejection rule and the nine relations. Everything is
quoted from `crates/gx-kinds/src/component.rs` (`ROLES` :37, `EXTERNAL_ROLE` :53,
`STEREOTYPED_ROLES` :59, `RELATIONS` :78) and `flow.rs` (`FlowEdge.kind` :69,
`validate_edge_kinds` :142). One correction of my own while checking: the first
draft of the PRD paragraph said the vocabulary never travels in the schema, which
is false — `get_diagram_schema` answers `rules[]`, `rules_of` extends it with
`handler.rules()`, and `component::rules` starts with `relations_rule()`, which
names every relation with its style and glyph. The document says that instead.
The levels stay open (D5), and the `mixin` correction stays in f28 T5 (L4).
Verified: docs only — no test runs, because no code and no schema changed; the
claims were checked line by line against the two source files and against
`crates/gx-mcp/src/contract.rs` (`SchemaResponse`, `rules_of`).
