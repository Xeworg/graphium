# f24-catalogo

Graphium's own diagram catalog, adapted from the owner's reference document:
what the fourteen kinds of this build already cover, what each of the
twenty-one proposed diagrams still lacks against its official source, and the
route that closes the gap. Base `staging` @ dde879b, branch
`feature/f24-catalogo`, worktree `~/Proyectos/graphium-staging`. Documentation
only: no kind, no engine, no contract changes.

## Specs

S1 (user, verbatim): "lee esto file:///home/xeworg/Descargas/graphium_catalogo_diagramas_y_arquitectura_propia.md quiero que hagas tu propio documento con lo que falta, pon agentes a estudiar cada diagrama vs las fuentes oficiales para entender que falta, y traza una ruta para lograr implmentar los que hacen falta"

S2 (user, verbatim): "es un documento de referencia adaptalo a lo que tenemos"
— the downloaded file is a reference catalog, not a specification: the
deliverable is Graphium's own document that takes its twenty-one diagrams,
maps each one onto the fourteen kinds this build registers (PRD §6 plus
`crates/gx-kinds/src/lib.rs::register_all`), names the gap per diagram against
the official source, and traces the route for the ones still missing.

S3: every per-diagram gap is researched by a delegated read-only agent against
the official source (OMG UML 2.5.1, ISO 5807, c4model.com, the de-facto
reference of a notation that has no standard) and cites it (`URL` + section)
next to the code evidence (`path:line`) of what Graphium already draws.

S4: the deliverable is one document inside the repo, in Spanish like
`docs/PRD.md`, that ends with an ordered route: which kinds to add, in which
order, with the engine, shape and contract work each one drags, and which
PRD §6 rows it closes or reopens.

S5 (reference §2.1, made operational): no entry claims conformance to a
trademarked notation. Each one declares either "notación propia inspirada en
X" or "implementación conforme a X", and the branding/licensing boundary is
written down where it applies.

S6: the document reconciles the reference catalog's twenty-one diagrams with
the eleven kinds PRD §6 already catalogues and has not built (`block`, `api`,
`trust`, `activity`, `journey`, `deployment`, `network`, `gitgraph`, `sankey`,
`orgchart`, `neural`): the route must say which of the two lists wins for each
overlap and why, instead of leaving two competing backlogs.

## Tasks

T1 inventory (done): the capability matrix of the fourteen registered kinds
(roles, shapes, edge vocabulary, engine, contract fields) from the code, written
in `docs/CATALOGO-DIAGRAMAS.md` §2. inline.
T2 research: six read-only agents, one per cluster (A UML behaviour, B UML
structure, C data, D systems/own notations, E process, F planning), each
returning per diagram the official element set, the Graphium evidence, the
tagged gap (SHAPE/CONTRACT/LAYOUT/SEMANTICS) and the branding boundary.
delegated, background, **at most three running at a time** (L3): a batch of six
at once made two clusters fail and two more report an error. (done: A, B, C, D
and F returned reports; the process cluster failed three delegations and was
researched inline, redacted by one worker — L4.)
T3 write `docs/CATALOGO-DIAGRAMAS.md` from T1+T2: adapted catalog, gap per
diagram, reconciliation with PRD §6, ordered route. inline. (done: 9,4k words,
§1-§8, twenty-one rows.)
T4 cross-link: one pointer from PRD §6 to the new document, and the document's
own index entry if `docs/` has one. inline. (done: `docs/PRD.md` §6 now states
the fourteen kinds this build registers and points at the catalogue.)
T5 close: the document's counts must match the registry (`kind_names()`), its
file:line citations must resolve, its sources must be reachable. docs-only
checks, inline.

## Log

L1 (2026-11-11, user, verbatim): "lee esto file:///home/xeworg/Descargas/graphium_catalogo_diagramas_y_arquitectura_propia.md quiero que hagas tu propio documento con lo que falta, pon agentes a estudiar cada diagrama vs las fuentes oficiales para entender que falta, y traza una ruta para lograr implmentar los que hacen falta"

L2 (2026-11-11, user, verbatim): "es un documento de referencia adaptalo a
lo que tenemos" — the source file is a reference to adapt, not a spec to copy:
the catalog entry for each diagram is written against what the code already
has, not against a greenfield wish list.

L3 (2026-11-11, user, verbatim): "no puedes lanzar todos a la vez dara error
lanza de 3 en tres" — concurrency cap of three read-only research agents.
Evidence at the moment of the instruction: the first batch of six left two
clusters failed (`mwbg9byp-1-cj4e` behaviour, `mwbg9byw-5-0skz` process) with
"assistant reported an error", and two more (`mwbg9bys-2-140g` structure,
`mwbg9byu-3-hc9g` data) reported an error mid-run. The failed clusters are
relaunched in a batch of at most three once the survivors settle.

L4 (2026-11-11, execution, T2): the process cluster (flowchart, DFD, workflow,
functional decomposition, Nassi-Shneiderman) failed three delegations in a row
before doing any work, so its research moved inline: the ISO 5807 symbol
taxonomy was sourced from a descriptive reference plus the Wikipedia summary
(the norm itself is paid and was not read), the `dataflow` module was read, and
the section was delegated to a worker for redaction only.

L5 (2026-11-11, evidence, T3): the data cluster found two documented-but-false
statements in the PRD, now recorded in `docs/CATALOGO-DIAGRAMAS.md` §5.2:
`docs/PRD.md:557` demands a `missing_field` error for a table with no `pk`
column and `crates/gx-kinds/src/er.rs` never emits it; `docs/PRD.md:565` calls
`direction` an `unknown_field` in `er` while `er.rs:89` accepts it and defaults
to `LR` with tests at `:561`. Which side wins is an owner decision, not a
silent edit.

L6 (2026-11-11, close, T5): docs-only cut, so the gates are citation and count
checks instead of tests. Every `path:line` reference in the document was
verified against the code before it was written — the agent reports arrived with
line numbers off by up to a dozen lines in three clusters and each was corrected
— and a script over the finished document resolves **62 citations** with no
missing file and no line past EOF. The counts match the registry: fourteen
`register();` calls in `crates/gx-kinds/src/lib.rs` against the fourteen rows of
§2. What the document says and the code cannot support is written as a drift, not
smoothed over: `docs/PRD.md:557` demands a `missing_field` error that
`crates/gx-kinds/src/er.rs` never emits, and `docs/PRD.md:565` calls `direction`
an `unknown_field` in `er` while `er.rs:89` accepts it with tests at `:561` (L5).
The catalogue is a proposal, not a promise: no code, no kind and no engine
changed in this cut.

f24-catalogo is COMPLETE: T1 inventory, T2 research, T3 the document, T4 the
cross-link, T5 the close.
