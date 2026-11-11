# f25-arreglos

The three decisions the catalogue left open, taken by the owner: the `er`
drift is resolved by making the primary key optional and **saying so** with a
warning, the `direction` line in the PRD is corrected to match the code, and
`activity` moves to `layered`. Base `staging` @ 3f7ab2e.

## Specs

S1 (user, verbatim): "tomare las recomendaciones" — the three recommendations
of the catalogue's closing report: (a) the code wins on `direction` and the PK
becomes optional **with a warning**, (b) the PRD line about `direction` in `er`
is corrected, (c) `activity` opens on `layered` and the PRD row yields.

S2 (drift a, owner-approved shape): a document of kind `er` with tables that
declare no `key: "pk"` column renders exactly as today (**no new error, no
change to the drawing**) and the response carries **one aggregated warning**
with code `tables_without_key`, naming the tables and saying what the absence
costs: a keyless table cannot be the `from` side of a relation (the `from`
column must be a PK) and the SQL bridge skips foreign keys pointing at a column
that is not a primary key. A document where every table has a PK produces no
such warning. The warning is additive: no contract version moves and no golden
SVG changes.

S3 (drift b, code wins): `docs/PRD.md` stops saying that `direction` is an
`unknown_field` in `er`; it says what the code does — `er` accepts `direction`
and defaults to `LR` (`crates/gx-kinds/src/er.rs:89`, tests at `:561`).

S4 (drift a, the other half): `docs/PRD.md` §10 stops demanding a
`missing_field` error for a table without a `pk` column, and documents the
warning of S2 instead. Reason recorded there: a table without a primary key is
legal in a real database (join tables, logs, tables the parser could not read),
so refusing to draw it is hostile; the drawing says what it cannot anchor.

S5 (decision c): `activity` is filed under `layered`, not `lanes`, and
`docs/PRD.md` §6 and §8.3.1 are corrected with their counts: `layered` goes
from 13 to 14 kinds and `lanes` from 6 to 5. Reason: a UML activity partition
nests (it is a cluster), `LanesEngine` draws no groups at all, and the guard of
a control flow lives on the edge, which lanes cannot label.

S6: `docs/CATALOGO-DIAGRAMAS.md` stops presenting the three items as open
decisions and records them as taken, so the document does not contradict
`docs/PRD.md` after this cut.

## Tasks

T1 the warning channel: `gx-core` gains an optional, default-empty way for a
kind to report a warning, the adapter in `gx-mcp` collects it on render and
validate, and `er` implements the one aggregated `tables_without_key` warning.
Test first: one RED test that a keyless document gets the warning and a keyed
one does not. delegated (worker).
T2 the PRD corrections: §6 row 16 and its counts, §8.3.1 mapping, §10 rules,
§10.1 `direction` line. inline.
T3 catalogue sync: §3, §5.1, §5.2 and §6 record the decisions as taken. inline.
T4 close: fmt/clippy, the kind suite, the goldens (must be byte-identical), and
the citation check over the touched docs. inline.

## Log

L1 (2026-11-11, user, verbatim): "tomare las recomendaciones" — the closing
report of `f24-catalogo` offered three recommendations and the owner took all
three. No new scope: the drift, the `direction` line and the `activity` engine
stay inside what the catalogue already documented.

L2 (2026-11-11, execution, T1, worker): the warning channel landed. `gx-core`
gained `KindWarning { code, message }` plus a default-empty
`DiagramKind::warnings` and an object-safe `KindHandler::warnings` whose adapter
returns nothing for a document it cannot parse — the error path already named
that problem, and a note on top of an error is noise. `er` implements one
aggregated note in document order, first five names then "y N más", and it
counts a `pk_fk` column as a key because the validator already accepts `PkFk`
as a relation origin: a table whose only role is `pk_fk` *has* a key and earns
no warning. Pinned by test. The render path is wired at the single point where
`size_warnings` was already assembled.

L3 (2026-11-11, execution, T1 continued, orchestrator): the worker reported that
`validate_diagram` could not be wired inside its allowed surfaces — the dry run
assembles its warnings in `crates/gx-mcp/src/contract.rs`, not in `tools.rs` —
and left the decision to the owner. It is in scope, not a new decision: PRD §7
RF20 says the dry run gives the same warning in the same words, so leaving one
door warning and the other silent is exactly the kind of drift this cut exists
to remove. Wired there by the orchestrator with its own test
(`the_dry_run_reports_the_tables_without_a_primary_key`), which also proves a
keyed schema carries no note.

L4 (2026-11-11, close, T4): gates of the cut. `cargo fmt --all` (the first
`--check` caught the new test's assertion spread over three lines), then
`--check` clean; `cargo clippy --workspace --all-targets` with **zero** warning
or error lines; `cargo test --workspace` **861 passed, 0 failed** (858 before
this cut, plus the three new tests: the `er` unit test, the render test and the
dry-run test); the touched crates alone re-run after formatting: 405 passed.
**No golden changed**: the six modified files are the four sources and the two
documents, and nothing under the golden directories appears in `git status`.
The drawing is byte-identical, which is the whole point of S2.

f25-arreglos is COMPLETE: T1 the warning channel (worker + the dry-run half by
the orchestrator), T2 the PRD corrections, T3 the catalogue sync, T4 the gates.
Branch `feature/f25-arreglos` on top of staging `3f7ab2e`; push and the merge to
staging remain the owner's decisions.

L5 (2026-11-11, close, T4): the kind now teaches the rule where the agent reads
it. `er.rules()` gained a third line saying that a table with no `pk` column
draws anyway and the answer warns with `tables_without_key`; before, the only
place that said so was the PRD, and the agent reads `get_diagram_schema`'s
`rules[]`, not the PRD (`f23-contrato.md`: the contract teaches what exists).
Re-run after the change: `gx-kinds` 114 + 11 + 12, `gx-mcp` 232, fmt clean,
clippy zero.

**Delivery slice.** The review slice of this feature is the **single commit** on
`feature/f25-arreglos` (four sources and three documents, ~400 changed lines).
The harness figure of 206 commits / 12,711 lines counted since `origin/main` is
the base, not this cut: `feature/f25-arreglos` is stacked on `staging`, which is
203 commits ahead of `main`. Anything that reaches `main` from here is a
staging-release decision, not a feature-PR decision.
