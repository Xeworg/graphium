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
