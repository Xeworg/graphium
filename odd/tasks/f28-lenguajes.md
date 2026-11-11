# f28-lenguajes

Cobertura y auditoría de los veinte lenguajes que el motor ya tiene, más la
utilidad que ese motor le aporta al agente. Not a new language cut: this one
publishes what the book already knows, equalises the number that today cannot be
compared between languages, and closes the cheapest expired reductions. Branch
`feature/f28-lenguajes`, worktree `../graphium-f28`, base `staging` @ c0b555b.

## Specs

S1 (owner, verbatim): "mientras esto esta quiero que pongas en una rama a un
agente a trabajar en la cobertura y auditoria de lenguajes que ya tenemos,
tambien debe analizar la facilidad que esta aportando al agente, con la
intencion que aumente la utilidad y por ciento con los lenguajes debe auditar
los que se redujeron los puntos a implementar tambien debe justificar por asi
funciona mejor o que debe cambiar, pero primero haz una analisis con 3 agentes y
luego haces el worktree con el agente que haga los cambios ordena mi idea"

S2 (what "the percentage" actually is, verbatim from
`crates/gx-index/src/resolution.rs:51-58`): "The published rate: `resolved /
edges`, in percent, `0` for a language with no relations (a rate over an empty
set is not a lie, it is NaN)." Numerator: the edge rows whose folded target name
has 1 to 5 definers in the book (`resolution.rs:117-121`, `MAX_NAME_FANOUT = 5`).
Denominator: every edge row joined to the file of its source symbol
(`resolution.rs:93-98`). Excluded from both: runtime targets, **and only for the
languages that carry a list** (`resolution.rs:110-113`). It is not a percentage
of files, of symbols or of AST nodes.

S3 (the incomparability, verbatim from `resolution.rs:127-129`): "the runtime
exclusion is per-language, so the seventeen others' rates are not comparable."
The lists exist for python, go and java only (`registry.rs:239, 415, 553`;
`builtins: &[]` everywhere else), and a guard test actively forbids a fourth:
`registry.rs:1415-1430` asserts `matches!(grammar.lang, "python" | "go" |
"java")`.

S4 (nobody sees the number, and the book knows what it hides): `Book::resolution()`
(`book.rs:417`) has no caller under `crates/gx-mcp`; no tool lists the files of
the book or the languages it indexed (`dispatch.rs:12-29`); `book.rs:43-66`
computes `files_failed` and `warnings` and `lib.rs:1320-1335` never publishes
them, so "the book has no chapter there" is the same answer for a typo, an
unsupported language, a file over 1 MiB (`lang/mod.rs:88`) and a `target/` path.

S5 (the cuts that expired, from the audit of `f10-motor-lenguajes.md`): (a) the
runtime lists for the five languages `docs/PRD.md:615` already names as next
(swift, kotlin, dart, scala, r) — the reason that deferred them was the
attributability of that cut, not a measurement; (b) Go package-path imports,
recorded as "candidato para un corte aparte" with a measured 83.3 % → 100 % on a
two-file package (`f10` L14); (c) the Dart `mixin C = Base with M` form, deferred
for "rarity" while every other mixin spelling is read (`lang/dart.rs:37-39`).

S6 (the cuts that still pay for themselves, and stay): relation kinds not
invented per language (SQL only `uses`, R/Lua no `inherits`), fields and enum
cases never being declarations, Python `self.x` staying out, `MAX_NAME_FANOUT = 5`,
`.h` belonging to C. Each is pinned by a test that fails if relaxed, and the f10
T6 measurement is the evidence that relaxing them buys false numbers.

S7 (the binary figure is stale): `docs/PRD.md:300-301` and the CI step
`Binary budget (RNF1-quater)` still quote the last measurement taken in f10
(45,806,232 B), although f11 to f27 landed afterwards, including `er_bridge.rs`
in `gx-index`. No decision may cite that number until it is measured again.

## Decisions

D1 (what gets published, and how it stays honest). The rate is published as
`resolution { edges, resolved, percent, comparable }`, where `comparable` is
true only for a language that carries a runtime list. That flag is the cheap
honest fix for S3: instead of a comment in the core, the answer itself says
which numbers may be compared. Nothing is renamed and no percentage changes
value by this decision alone.

D2 (where it is published). A new read-only tool, `code_index`, with no required
arguments and an optional `lang` filter. It answers what no tool answered: the
state of the book, `files { total, done, failed [{ path, reason }] }`, `symbols`,
`languages [{ lang, files, symbols, resolution }]`, and the limits in force. It
is a new tool and not extra fields on every answer because the envelope
(`envelope.rs:22-38`, 13 000 to 24 000 chars) is the scarcest thing the agent
has, and a coverage report is asked for on purpose, not on every call.

D3 (every skip says why). A file that does not reach the book gets a stable
reason code, not prose: `ignored` (gitignore and hard exclusions), `too_large`
(over 1 MiB), `unsupported_language` (no grammar in this build), `unreadable`.
An agent branches on the code; a human reads the hint. This closes S4 and it is
the difference between "the book has no chapter there" and knowing what to fix.

D4 (the guard becomes a census). `registry.rs:1415-1430` stops asserting "only
these three languages have a list" and asserts instead that every language
declared in an explicit census has a folded list, and that the census is not
silently extended. Adding a language stays a deliberate edit that moves a
number — the guard's purpose survives, its veto does not.

D5 (no invented numbers). Every percentage that lands in a test floor or a doc
line is measured by the harness in the same commit. A floor moves down only with
the measurement written beside it, and never up by rounding.

D6 (honest cuts). Three specific answers lie today and are fixed: `code_search`
truncates its candidates at five with no notice (`search.rs:16`), a symbol seed
returns no `files[]` grouping (`lib.rs:645-648`), and `code_er`'s `chars` counts
bytes while every other tool counts characters (`lib.rs:1240`).

D7 (the ceiling is measured before it is quoted). T5 re-measures the release
binary and updates `docs/PRD.md` and the CI gate from the measurement. If it
exceeds the current 50.3 MB gate, that is a finding for the owner, not a number
to adjust quietly.

## Tasks

T1 publish the coverage and the reasons (`gx-index` + `gx-mcp`): the census of
languages with their file and symbol counts, `files_failed` with the reason codes
of D3, the resolution rate with `comparable` (D1), and the `code_index` tool of
D2 with its instructions and descriptions. Test first: RED on a file skipped for
each reason and on `comparable` being false for a language without a list.
worker A. commit: `feat(index): the book publishes its coverage and why a file is missing`.
T2 equalise the denominator: runtime-name lists for swift, kotlin, dart, scala
and r, the census guard of D4, and the floors re-measured (D5). Test first: RED on
a stdlib reference that today counts as unresolved in each of the five. worker B.
commit: `feat(index): five more languages stop counting their runtime as unresolved`.
T3 honest cuts (D6): candidate truncation made visible and pageable, `files[]` for
symbol seeds, `code_er` `chars` in characters. Test first: one RED per lie. worker
A, after T1. commit: `fix(code): three answers stop hiding what they cut`.
T4 the two expired grammar reductions: Go package-path imports and the Dart
`mixin C = Base with M` form, each with the measurement that justifies it (S5).
Test first: RED on the Go package import and on the Dart mixin application. worker
B, after T2. commit: `feat(index): go imports resolve across a package and dart mixins are declarations`.
T5 measure and document (D7): re-measure the release binary, update `docs/PRD.md`
§9/§10 and the CI gate, record the per-language rate table in
`docs/CATALOGO-DIAGRAMAS.md` or a new `docs/LENGUAJES.md`, and run the full gate
set. inline. commit: `docs(odd): f28 records the coverage the book can defend`.

T6 the teaching layer (owner's direction after L5). Split in two once the
extension was studied (L6): the **timely** text belongs to the extension, the
**reference** text to the server.
T6a the extension teaches the moment (`extensions/pi/graphium/`): declare the
server with `exposure: "direct"` so the 16 tools reach the model at all — today
they do not, which is why an agent in this very session found them only by
searching —, rewrite the 169-character `description` so it names the five book
tools (`code_search`, `code_structure`, `code_impact`, `code_diagram`, `code_er`)
instead of two, drop the two false claims it carries (that a PNG always works, and
that the book is always on), and add the guide's own slot for rules keyed by tool
name: `promptGuidelines` from `before_agent_start`, with the sentence that exists
nowhere today — "when you are about to grep for a symbol, `code_impact` already
knows who points at it". `verify.mjs` grows one check per new channel; it keeps
proving the extension boots, and it now also proves what it teaches. worker C.
commit: `feat(extension): the teaching names the moment, not only the tool`.
T6b the server stops competing (`crates/gx-mcp/src/harness.rs`): fix the three
sentences that are false against today's code (the 300-node cap belongs to
`opts.extract`, not `opts.focus`; `interactive` "arrives in F3" and F3 shipped
without it; the error path names `opts.focus.around` where the document writes
`opts.focus.id`), and cut from `INSTRUCTIONS` the teaching the extension now owns,
so the budget does not grow. worker C, after T3. commit: `docs(harness): the
server keeps the contract and hands the timing to the extension`.
## Log

L1 (owner, verbatim): the request of S1.
L2 (evidence, the analysis). Three read-only audits were run before this
document: coverage and the formula (S2 to S4), the reductions and whether their
reasons still hold (S5, S6), and the utility the index gives an agent (the seed
grammar, the fanout drop, the invisible skips, the missing listing tool, the
`chars` unit mismatch). Their findings are the evidence quoted in the specs; no
claim here was inherited from a summary.
L3 (correction, my error in the brief). T2's first dispatch named
`crates/gx-app/tests/languages.rs` as the surface of the floors. That file does
not exist: the per-language floors, the fixtures and the rate tests live in
`crates/gx-index/tests/languages.rs`, and `gx-app` does not depend on `gx-index`
at all, so `Book::resolution()` is not even callable there. The surface and the
check of T2 are corrected to `crates/gx-index/tests/languages.rs` and
`cargo test -p gx-index --test languages`. The BEFORE numbers the worker captured
before touching anything: swift 24.1 %, kotlin 26.9 %, dart 25.9 %, scala 48.5 %,
r 50.0 %.
L4 (evidence, from a live call inside this session). Mid-cut, the product was
called against this worktree for the first time in the session:
`code_structure{file:"crates/gx-core/src/ir.rs"}` and
`code_impact{symbol:"parse_side",direction:"in"}`. The impact answer was correct
and cheap (one caller, `unresolved: 0`, 1.5k), which is the same conclusion the
session had reached by hand with `grep`. The structure answer exposed a defect
that the analysis had missed: `progress.symbols` answers **`0`** with 163 files
indexed, so the field the census is supposed to publish is already published as a
lie, not merely absent. T1 owns the fix (a non-zero pinned count, and the wiring
audited) because the census cannot be honest on top of a counter that reads zero.

L5 (owner, verbatim): "no hay problema en que no sean utiles aun la extension debe
ser estudiada para ensenarle al agente cuando y como usarlas". The bar moves from
"the tools are useful" to "the tools are findable at the moment they are useful":
the teaching text is the deliverable, and T6 is where it lands.
L6 (evidence, the extension study, and the correction of L5's premise). The Pi
extension was studied against `https://pi.dev/docs/latest/extensions`. Three
findings decide the split of T6. (a) **The tools are not declared to the model**:
the extension registers the server with no `exposure`, the guide documents
`exposure: "direct"` as "declared to the model while active", and in this session
Graphium reached the agent only as a `codemode` namespace — which is why the agent
that wrote this document reached for `grep` three times and never saw
`code_impact`. That is a configuration fact, not a habit. (b) **The channel that
can time text is the extension**: the guide documents `promptGuidelines` ("append
tool-specific bullets … when the tool is active"), `before_agent_start` ("Can
inject a message and/or modify the system prompt"), `tool_call` and `tool_result`
("Can modify result"), while its MCP section says nothing about a server's
`instructions` reaching a model. The extension uses none of them: one
169-character `description` is its whole teaching. (c) **The short text is also
the wrong text**: it names two of the five book tools, promises a PNG that needs
`path` and the app, asserts a book that can be `book_disabled`, and omits the one
rule the server does get right ("Never draw to compensate for not understanding
the code: read first"). `verify.mjs` runs 19 checks against a hand-written stub:
it proves the extension boots and never touches whether anything is taught.

