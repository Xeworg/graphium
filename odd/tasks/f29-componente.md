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
stays byte-identical, because a document that writes no `kind` is today's
document. One declared exception is allowed and must be named here before the gate
runs: the `component-api` scene gains typed relations, so its drawing changes.

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

D3 (the nine relations of `component`). `uses`, `provides`, `consumes`,
`depends_on`, `implements`, `extends`, `aggregates`, `composes`, `deploys` — each
row with its style, its `head`/`tail` glyph from the closed vocabulary of
`f26-adornos.md`, and the word it implies when the document writes no label. The
table is the contract: the rule line derives from it (the f26 S8 discipline), so a
relation cannot be painted with a glyph the contract does not mention.

D4 (six roles). `container`, `component`, `database`, `external` stay, and
`subsystem` and `actor` join them, each with the shape the catalogue's row 12 asks
for and the stereotype the role means (S2). The stereotype rides the node field
`f27-etapa1` added, so the drawing already exists.

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
