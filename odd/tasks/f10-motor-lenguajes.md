# f10-motor-lenguajes — a robust reusable language engine + 15 more languages

Branch `feature/f10-motor-lenguajes` from `staging` @ ab12f60 (where the
`lang/` engine of F9 lives). The engine refactor comes FIRST; the 15
languages ride on it.

## Specs

- S1 (coverage problem): user verbatim — «la cobertura de los lenguajes
  que temenos es variable y baja en algunos, hay que hacer modificaciones
  para mejorar esto». Measured today (D3, PRD): TS 48.0%, Rust 36.4%,
  Java 29.6%, Go 17.2%, Python 14.8%. Improving the LOW ones is in scope,
  not only adding new ones.
- S2 (15 new languages): user verbatim — «considero necesario agregar 15
  lenguajes mas». The exact list is a human checkpoint AFTER the T1
  analysis proposes it ranked by usage and grammar feasibility.
- S3 (robust reusable engine): user verbatim — «para hacer una
  modificacion que al final sean para un motor mas robusto ya que mucho
  del codigo se podra reutilizar entre lenguajes». The five current
  extractors (rust, typescript, python, go, java + shared emit.rs) migrate
  to the reusable engine with no D3 regression before any new language
  lands.
- S4 (usage-ranked + extensible): user verbatim — «hay que ver cuales son
  los mas usados y hacer una analis de la mejora como tal permita agregar
  los luego mas lenguajes sin dificultad». T1 ranks candidates by usage
  (Octoverse/TIOBE/SO Survey/RedMonk, already fetched) and the refactor
  must make adding language N+1 a bounded, mostly-declarative job.

## Tasks

- T1 (S2 S4; route: explore+orchestrator; commit: —): the analysis —
  ranked language proposal (15 + alternates, usage data + tree-sitter
  grammar feasibility + per-grammar binary cost vs RNF1-quater), the
  engine reuse map (what is shared vs duplicated across the five
  extractors, where D3 resolution is lost per language), and the refactor
  design. Ends in a HUMAN CHECKPOINT: list + design approved.
- T2 (S1 S3; route: worker; commit: —): the engine refactor — shared
  core (declarative extractors, per-language name_norm/module anchors in
  one registry), the five languages migrated, D3 floors not lower than
  today, feature-flags per language kept.
- T3..T5 (S2; route: worker; commit: —): the 15 languages in grammar-
  family batches, each batch with D3 measured and published and its
  feature-flag; batching decided at the T1 checkpoint.
- T6 (S1 S4; commit: —): coverage improvements for the weak ones
  (Python, Go) guided by what the new engine makes cheap; PRD updated;
  RNF1-quater re-measured with 20 grammars; final gates.

## Log

- L1 (2026-11-07, user): «ahora viene una decicion importante, la
  cobertura de los lenguajes que temenos es variable y baja en algunos,
  hay que hacer modificaciones para mejorar esto, entonces para poder
  hacer considero necesario agregar 15 lenguajes mas, para hacer una
  modificacion que al final sean para un motor mas robusto ya que mucho
  del codigo se podra reutilizar entre lenguajes hay que ver cuales son
  los mas usados y hacer una analis de la mejora como tal permita
  agregar los luego mas lenguajes sin dificultad»
- L2 (2026-11-07, orchestrator): worktree graphium-f10 from staging
  ab12f60. Usage data fetched (responseId mw5wn3eidsnlu3): Octoverse
  2025 — TypeScript #1 by contributors, ~80% of new repos in
  Python/JS/TS/Java/C++/C#; TIOBE Oct-Nov 2026 — Python 19-21%, C, C++,
  Java, C#, Dart enters top 20; SO 2025 — JS 66%, SQL 58.6%, Python
  57.9%, Rust most admired; RedMonk Jan 2025. Engine map delegated to
  gentle-ai-explore (background). Binary ceiling is a first-class
  constraint: at 5 languages the full build already sits at 91.3% of
  RNF1-quater; the T1 proposal must size 20 grammars under feature-flags.
- L3 (2026-11-07, orchestrator, T1 result): the analysis, from the engine
  map taken on this branch (staging @ ab12f60 base).
  - Engine reuse map: the `parse()` skeleton is a 30-line identical copy in
    all 5 extractors (node-kind→kind tables + transparent-container lists)
    — the main declarative-izable piece. Genuinely imperative per-language
    hooks that stay: Go `attach_receivers` 2nd pass, Rust impl naming, TS
    export unwrap, Java package path, `called_name` chains.
  - Schema: `schema.rs:425` CHECK hardcodes the 5 lang values → every new
    language without parameterization costs a `SCHEMA_VERSION` bump +
    delete-and-reindex. T2 makes the lang values data-driven WITHOUT a
    schema bump (stored data of the 5 existing languages stays valid).
  - D3 weaknesses in code: stdlib/builtin targets never resolve (Python
    `str`/`int`/`len`, Go `fmt`/`len`, Rust `Path`), Python declares only
    module-scope constants (`self.x` deliberately not symbols), Go package
    imports unresolvable, name fanout cap `MAX_NAME_FANOUT=5`.
  - Cost of language N+1 today: 3 Cargo.toml manifests + module + GRAMMARS
    line + kind mapping + schema CHECK + per-language fixture/floor + PRD
    row + binary re-measure.
  - Binary: full 14.32 MB = 91.2% of the RNF1-quater ceiling; slim lever
    measured -1.12 MB for 3 grammars → the 15 new grammars must be
    default-off (feature flags, decided in f9 C3).
  - Proposal (usage-ranked, Octoverse 2025 / TIOBE / SO 2025 / RedMonk):
    JavaScript/JSX, C, C++, C#, PHP, Ruby, SQL, Shell, Swift, Kotlin,
    Dart, Scala, R, Lua, Zig (alternates: Objective-C, Julia, Elixir,
    OCaml).
- L4 (2026-11-07, user, verbatim): «luego segui con el cambio del motor
  este es importante ya que lleva bastante escritura». Checkpoint outcome:
  the ENGINE REFACTOR (T2) is approved to start; the 15-language LIST
  stays an open checkpoint until the first grammar batch (T3) — T2 does
  not land any new language.
- L5 (2026-11-07, worker+orchestrator, T2: S1 S3): the engine refactor
  landed — behaviour-preserving, differential-verified.
  - Design: NEW `lang/engine.rs` holds the shared pipeline (`Grammar::parse`
    → `extract`: parser → module path → walk → relations → anchor → attach
    → `ParsedFile`), the declarative types (`Rules`/`Container`/`Leaf`/
    `Walk`/`ModuleSource`/`Anchor`/`Hook`/`Nesting`/`Body`) and the table
    walker; NEW `lang/registry.rs` holds the five `Grammar` entries with
    their node-kind tables side by side, feature-gated, plus `GRAMMARS`.
    Per-language modules keep only `LANGUAGE`, `language()`,
    `module_path()`/Java `package` path, `relations()` and the imperative
    hooks of L3 (Go `Walk::Custom(go::walk)` for `attach_receivers`, Rust
    `impl` naming, TS declarators, Python `module_constant`, Java
    `ModuleSource::Tree`). `Leaf.inside` makes context-dependent kinds data
    (`fn` in `impl`/`trait` → `method`). Net −293 lines.
  - Schema untouched: `SCHEMA_VERSION` stays 1, the `lang` CHECK stays the
    v1 pack (stored indexes stay valid; no reindex). NEW tests: schema pins
    version 1, accepts every registry language, refuses `ruby`; the
    registry validates every table node kind against its tree-sitter
    grammar (`node_kind_for_id`) so a table typo fails loudly. **Open for
    T3**: the first new language needs the planned data-driven lang CHECK
    evolution (schema decision at that cut, not a silent CHECK widen).
  - D3 unchanged (floors held): typescript 12/25=48.0%, rust 4/11=36.4%,
    java 8/27=29.6%, go 5/29=17.2%, python 4/27=14.8% — measured by
    `cargo test -p gx-index --test languages … -- --nocapture`.
  - Binary (release, opt-level=z, lto, strip): full `graphium`
    14,363,032 → 14,358,936 B (−4 KB); `--no-default-features` 9,165,208 B
    identical; `--features "lang-rust,lang-typescript"` 13,241,496 B.
  - Parity technique (reusable for T3/T6): base tree via `git archive HEAD
    | tar -x -C <mktemp>`, one scratch integration test in both trees
    parsing a fixed corpus through the stable `lang::{grammar_for, parse}`
    API, dumps of every symbol field + edge diffed — 125 chapters
    byte-identical.
  - Gates (orchestrator-verified): `cargo fmt --check` 0; `cargo clippy
    --workspace` 0; `cargo test --workspace` 0 (42 suites ok, 0 failed).
    Pre-existing, NOT regressions: `cargo test -p gx-index
    --no-default-features` exits 101 with 10 integration failures at the
    base commit too (book.rs/watcher assume ≥1 grammar); the zero-language
    lib build warns about 5 unused emit helpers (11 at base).
  - Payoff for T3 (what language N+1 provides now): one registry entry
    (tables + extensions + module/anchor + relations), one module with
    `LANGUAGE`/`language()`/`module_path()`/`relations()` and — only if
    genuinely needed — imperative hooks; plus the manifest/fixture/floor
    checklist of L3.
  - Slice boundary: this branch carries inherited staging history (78
    commits vs origin/main); the feature's own slice after this commit is
    3 commits ≈ 900 authored lines (2 docs + T2 unit) — one unit, review
   able as such.
- L6 (2026-11-07, user decision via orchestrator): the T1 checkpoint is
  CLOSED — «la lista ya la tenias, pero si no es asi busca los mas usados y
  agrega eso». The approved list is the T1 proposal, usage-ranked
  (Octoverse 2025 / TIOBE Oct-Nov 2026 / SO 2025 / RedMonk): JavaScript/JSX,
  C, C++, C#, PHP, Ruby, SQL, Shell, Swift, Kotlin, Dart, Scala, R, Lua,
  Zig (alternates if a grammar proves unviable: Objective-C, Julia,
  Elixir, OCaml). Batching by grammar family (each batch: registry entry +
  module + default-OFF feature + fixtures/floors + D3 published + PRD row +
  binary re-measure):
  - Batch 1: JavaScript/JSX, C, C++, C# — plus the ONE schema evolution to
    data-driven lang values (L5's open item): the worker evaluates the
    options with evidence and lands it once, so batches 2-4 ride free.
  - Batch 2: PHP, Ruby, SQL, Shell.  Batch 3: Swift, Kotlin, Dart.
  - Batch 4: Scala, R, Lua, Zig.
  Default build keeps the original five ON and the new ones OFF: the
  default binary must stay ~14.36 MB per batch (RNF1-quater); the
  20-grammar re-measure is T6.
