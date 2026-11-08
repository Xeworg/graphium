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
- L7 (2026-11-07, worker, T3 Batch 1: S2 S4): JavaScript/JSX, C, C++ and
  C# landed, and the `lang` values of the book became data-driven — the
  one-time schema evolution that makes batches 2-4 (and any future
  language) a registry entry.
  - **Schema decision (chosen): keep `SCHEMA_VERSION = 1` and drop the
    `lang` CHECK** from `file`; the registry of the build is the only
    list. Acceptance moved to the one insert path
    (`Book::commit_batch` refuses a chapter whose `lang` has no grammar:
    counted as skipped with one stderr line), so the DDL names no
    language and language N+1 never touches the schema again.
  - **Stored v1 books**: a book written before this cut carries the
    check in `sqlite_master`. The open path detects it and rebuilds the
    `file` table in place (`schema::relax_language_check`, called from
    the probe; `foreign_keys` is suspended **outside** the transaction —
    the bundled SQLite enforces FKs by default and `DROP TABLE file`
    would cascade the chapters away — and restored after it). Every row
    and every id survive: no file is parsed again and no index is thrown
    away, so D1 (delete-and-reindex) stays for real doubts. Version
    stayed 1, so a stored book is not rebuilt on open; the step runs once
    per book (`OpenReport::migrated` reports it, `service::ensure` logs
    one line).
  - **Measured migration cost** (a row copy of `file`; `symbol`, `edge`
    and the FTS are not touched): 1.000 files / 10.000 symbols in
    **1,8 ms** (in-memory, printed by the unit test with `--nocapture`);
    file-backed with the delete journal: **0,8 ms** for 1.000 files,
    **12,6 ms** for 20.000 files (200.000 symbols) and **72,6 ms** for
    100.000 files (1M symbols, 23,5 MiB). One-time, only for books that
    still carry the v1 check.
  - **Rejected option**: bump to schema 2 with a table-rebuild migration
    or accepting the D1 delete-and-reindex. It buys nothing the in-place
    relaxation does not: it either adds the migration ladder the schema
    explicitly does not have, or forces every user's book through a full
    reindex (minutes of parsing on a real tree, orders over the 72,6 ms
    row copy) for a constraint that only restricted `INSERT`s. Also
    rejected: a `language` table with `file.lang REFERENCES
    language(name)` — schema state that would need refreshing from the
    registry on every open plus a migration per stored book, to enforce
    in SQL what the insert path already enforces with the runtime source
    of truth.
  - Removed `LANGUAGE_PACK`/`meta.lang_pack`: it was the last second list
    of languages. Stored books keep the old row (nothing reads it); new
    books do not write it. New contract pinned by tests: the DDL carries
    no language list, every registry grammar is accepted, a language absent
    from the registry is refused at insert (integration test with a
    hand-built `ruby` chapter: 1 written, 1 skipped, no row), and a v1
    book is relaxed in place on open, idempotently.
  - **The four languages (Batch 1, list of L6)** — new cargo features,
    all **DEFAULT OFF**:
    - `lang-javascript` (tree-sitter-javascript 0.25.0): `.js`, `.jsx`,
      `.mjs`, `.cjs`. One grammar covers JSX — no second language, and
      TypeScript keeps its own `.ts`/`.tsx` module (no duplication).
      Classes/methods/functions and module-level `const`s by declarator;
      relations `calls`/`imports`/`inherits`. No `uses` (JavaScript has no
      type annotations) and JSX names are markup, not relations.
    - `lang-c` (tree-sitter-c 0.24.2): `.c`, `.h`. Functions, structs,
      unions, enums, typedefs and file-scope variables (const/storage).
      `.h` belongs to C and only to C, so which grammar answers an
      extension never depends on the enabled features (C++ headers are
      `.hpp`/`.hh`/`.hxx`/`.h++`). Relations `calls`/`imports`/`uses`
      (no `inherits`: C has none); a header import is its stem
      (`store.h` → `store`) because the book stores identifiers.
    - `lang-cpp` (tree-sitter-cpp 0.23.4): `.cpp`, `.cc`, `.cxx`, `.c++`,
      `.hpp`, `.hh`, `.hxx`, `.h++`. Classes/structs/enums/aliases, free
      functions and the methods of a class (in-class definitions and
      prototypes, both `field_declaration` and `declaration`); a
      specifier declares only with a body; `namespace` is transparent
      (the file already names the source). Four relation kinds.
    - `lang-csharp` (tree-sitter-c-sharp 0.23.5): `.cs`. Module path from
      the `namespace` the file declares (like Java's `package`), types
      with their methods/constructors/delegates; fields and properties
      are not declarations of the book. Four relation kinds; `using`
      imports carry the last segment (Java's convention). Needs no hook.
  - **Language N+1 after this cut**: one registry entry + one module +
    dependency/feature + fixture/floor + PRD row + re-measure. **No
    schema change** — the L3 checklist minus its schema line.
  - **D3** (`-- --nocapture`, features on). The five of C2 unchanged —
    the differential check (`git archive HEAD` vs this tree, same tests,
    `--nocapture`) gives a **byte-identical** table: TypeScript 48,0 %
    (12/25), Rust 36,4 % (4/11), Java 29,6 % (8/27), Go 17,2 % (5/29),
    Python 14,8 % (4/27). Batch 1, on **its own** fixture tree (adding
    its chapters to the C2 tree would move the fanout of common names
    and false the published C2 numbers — measured and discarded):
    **C 81,8 % (9/11), C++ 71,4 % (10/14), C# 66,7 % (10/15),
    JavaScript 66,7 % (8/12)**. Floors pinned at 79/69/64/64 % (measured
    minus 2–6 points, never inflated).
  - **Binary** (release, opt-level=z, lto, strip): default `graphium`
    **14.358.936 B before → 14.363.032 B after** (+4.096 B, +0,03 %: the
    migration/insert-guard code of the schema cut; the four grammars
    compile to nothing in the default build), 91,5 % of RNF1-quater. The
    same tree with the **nine** features on (`-p gx-app --features
    "lang-javascript,lang-c,lang-cpp,lang-csharp"`, built from a temp copy
    because `gx-mcp`/`gx-app` forward the flags and are outside this
    cut's surfaces): **24.209.176 B** (154,2 % of the ceiling,
    +9.846.144 B) — exactly why the four new grammars stay off by default.
  - **Gates (worker-verified, exit codes without pipes)**: `cargo test
    --workspace` **0** (42 suites ok); `cargo test -p gx-index --features
    "lang-javascript,lang-c,lang-cpp,lang-csharp"` **0** (6 suites ok, the
    new per-language fixtures and floors included); `cargo fmt --check`
    **0**; `cargo clippy --workspace --all-targets` **0** (0 warnings);
    `cargo clippy -p gx-index --all-targets --features …` **0** (0
    warnings). Each new feature alone compiles
    (`--no-default-features --features lang-X`; the C#-only slim build
    warns about `dotted_module`, the same pre-existing pattern of the
    other slim builds). The known `--no-default-features` failures of L5
    are untouched and out of this cut.
  - **Scope note for the reviewer**: `Cargo.lock` (workspace root)
    changed — the four grammar crates and `tree-sitter-language` — and it
    is outside the three surfaces this task listed. The dependencies
    cannot be added without it; the work unit has to include it (the
    alternative is dropping the batch).
- L8 (2026-11-07, worker, T3 Batch 2: S2 S4): PHP, Ruby, SQL and Shell
  landed. **The schema needed nothing**: no `SCHEMA_VERSION` bump, no
  `CHECK`, no migration — `lang` is the registry of the build and the
  insert path is the only gate (L7's contract held for four more
  languages without one line of `schema.rs`).
  - **Grammar provider — one loud deviation.** The `tree-sitter-sql` crate
    is **unviable**: crates.io has only 0.0.1/0.0.2 (2021), built against
    tree-sitter 0.19, with no `tree-sitter-language` `LANGUAGE` fn, so the
    engine cannot load it. SQL landed on **`tree-sitter-sequel` 0.3.11**,
    the maintained SQL grammar at the ABI of the engine (`tree-sitter-language`
    0.1, dev-dep tree-sitter ~0.25). The feature is `lang-sql` — it names
    the language, not the crate. The other three are the plain crates:
    `tree-sitter-php` 0.25.1, `tree-sitter-ruby` 0.23.1 and
    `tree-sitter-bash` 0.25.1. No language was swapped, so no human
    checkpoint was needed for this; the crate substitution is recorded
    here, in `crates/gx-index/Cargo.toml` and in `docs/PRD.md`.
  - **The four languages (Batch 2, features DEFAULT OFF)** — the registry
    entry of each is the whole of what the engine knows:
    - `lang-php` (`.php`, the full PHP grammar, the one that reads the HTML
      around `<?php`): classes, interfaces, traits and enums as
      class/trait/trait/type with their methods, `function`s as `fn`, and a
      `const` group as one `const` per `const_element` (the one hook). A
      property and an enum case are not declarations of the book (a field
      is not one in Java, C# or C++ either). **Module from the `namespace`
      the file declares** (like Java's `package` and C#'s `namespace`), with
      `\` as the `qname` separator — the language's — and the anchor named
      by the last segment of that namespace (`chapter_anchor_name` now
      splits on `\` too; no other grammar of the book spells a module path
      with a backslash and the nine published D3 numbers stayed
      byte-identical). Relations: `calls` (the three call forms and `new`),
      `imports` (the last segment of every `use`, group clauses included),
      `inherits` (`extends`, every `implements`, and `use TraitName` — a
      PHP trait is a mixin), `uses` (parameter, return and property types;
      a primitive is not a declaration and does not resolve).
    - `lang-ruby` (`.rb`): classes and modules as containers (a Ruby
      `module` nests its methods like a class does), a `def` inside them as
      `method` and at the root as `fn`, `def self.x` as `method`, and
      `LIMIT = 3` as `const` (the one hook: an `assignment` whose left is a
      `constant`). Module from the path (`lib.store`), `::` as the `qname`
      separator (the language's). **The metaprogramming decision**: an
      `attr_accessor :name`, a `define_method` or a `class_eval` stays a
      call — a symbol synthesised from a call is a semantic pass, and the
      book publishes only what the syntax tree spells — so an accessor is
      one call and not two declarations. Relations: `calls` (every send by
      its method name; `DiagramStore.new` is a call to the class its
      receiver names), `imports` (`require`/`require_relative`/`load`
      **without a receiver**, by the stem of the path they name), `inherits`
      (the superclass and every `include`/`extend`/`prepend`). No `uses`:
      Ruby has no type annotations (JavaScript's case).
    - `lang-sql` (`.sql`, grammar `tree-sitter-sequel`; the file is the
      module): tables, views (materialized included) and types as `type`,
      functions and triggers as `fn`. Every declaration is a hook: a
      `CREATE` of this grammar has no `name` field and names its object in
      an `object_reference` child. A column is not a declaration, and an
      **index is not one either** — it has no body and no name the language
      refers to; what matters is the table it reads. **The relation
      decision (SQL)**: SQL has no `calls` and no `imports` in the sense the
      other grammars read them — the tree of a statement names objects, not
      call targets or modules — so its only kind is **`uses`**: the tables a
      query reads (`FROM`/`JOIN`, which is how a view reads its sources),
      the table an index is built on (`ON`) and the table a foreign key
      points to (`REFERENCES`). `calls`, `imports` and `inherits` are not
      invented for it, and the fixture measures four edges, not twenty.
      Honest limit of the grammar: a *named* constraint
      (`CONSTRAINT fk FOREIGN KEY …`) is not in its rules and parses as an
      error node, so it yields no relation (the fixture writes the inline
      `REFERENCES` form).
    - `lang-shell` (`.sh`, `.bash`; grammar `tree-sitter-bash`; the file is
      the module): the functions of the script and the variables assigned at
      its root as `const`. No hook at all — `function_definition` and
      `variable_assignment` both spell their name in a field. A `local` of a
      function body is not a declaration (a function is a leaf, its body is
      not walked). Relations: `calls` (every command by the name it writes —
      a shell does not distinguish a function of the script from a system
      program, so the book records the name and D3 reports honestly how many
      of them the worktree answers) and `imports` (`source`/`.` by the stem
      of the path). No `uses` and no `inherits`: a shell has no types.
  - **Language N+1 after this cut**: still one module + one registry entry
    + dependency/feature + fixture/floor + PRD row + re-measure, and still
    **no schema change**. Batch 2 needed no new kind of engine work: four
    table walks, one `Walk::Custom`-free batch, and hooks only where the
    grammar names a declaration in a shape the tables cannot (PHP's `const`
    group, Ruby's constant, SQL's `object_reference`).
  - **D3** (`-- --nocapture`, batch features on). Batch 2, on **its own**
    fixture tree (the rule of L7: a batch is measured on its own chapters,
    never on a pool one batch can move): **PHP 95,0 % (19/20), Ruby 66,7 %
    (6/9), SQL 100 % (4/4), Shell 37,5 % (3/8)**. Floors pinned at
    **91 / 63 / 96 / 34 %** (measured minus 3,7–4 points, never inflated).
    SQL's 4/4 is four edges by decision, not by omission; Shell's 37,5 % is
    the honest number of a language whose calls are mostly system programs
    (`printf`, `echo`) with no declaration in the book. The nine existing
    languages, same tests, same `--nocapture`: **byte-identical** table —
    TypeScript 48,0 % (12/25), Rust 36,4 % (4/11), Java 29,6 % (8/27),
    Go 17,2 % (5/29), Python 14,8 % (4/27), JavaScript 66,7 % (8/12),
    C 81,8 % (9/11), C++ 71,4 % (10/14), C# 66,7 % (10/15).
  - **Binary** (release, opt-level=z, lto, strip): default `graphium`
    **14.363.032 B — byte-identical to L7's after** (the four grammars
    compile to nothing in the default build), 91,5 % of RNF1-quater. The
    same tree with **all thirteen** features on (built from a temp copy
    because `gx-mcp`/`gx-app` do not forward the Batch 1/2 flags and are
    outside this cut's surfaces): **31.217.432 B** (198,8 % of the ceiling,
    +16.854.400 B over the default) — the measurement that keeps the eight
    new grammars off by default.
  - **Gates (exit codes without pipes)**: `cargo test --workspace` **0**
    (42 suites ok, the default build unchanged); `cargo fmt --check` **0**;
    `cargo clippy --workspace --all-targets` **0** (0 warnings);
    `cargo clippy -p gx-index --all-targets --features
    "lang-php,lang-ruby,lang-sql,lang-shell"` **0**; the four gx-index
    integration suites with the batch features **0** (book 22, languages
    21, relations 7, end_to_end 3); each new feature alone compiles
    (`--no-default-features --features lang-X`; the PHP-only slim build
    warns about `dotted_module`, the pre-existing pattern of L7's slim
    builds).
  - **Open blocker (the one line of this cut the worker may not touch)**:
    `cargo test -p gx-index --features "lang-php,lang-ruby,lang-sql,lang-shell"`
    exits **101** on **one stale unit test inside
    `crates/gx-index/src/schema.rs`** — a surface this task forbids — which
    asserts `!accepts_language("ruby")` and `!accepts_language("php")`
    (true when Batch 1 landed, false now). The other 62 tests of that run
    pass and every integration suite is green. The fix is test-only and
    three lines: derive the unknown language from the registry instead of
    hard-coding two languages that Batch 2 now owns. It is reported, not
    edited: the orchestrator authorizes it or accepts the gate as failing
    on that single assertion.
  - **Scope note for the reviewer**: `Cargo.lock` (workspace root) changed
    — `tree-sitter-php`, `tree-sitter-ruby`, `tree-sitter-sequel`,
    `tree-sitter-bash` and `tree-sitter-language` — and it is outside the
    listed surfaces; the dependencies cannot land without it (L7's note
    verbatim). It also **downgrades `cc` 1.11.0 → 1.2.67** for the whole
    workspace, and that is worth knowing: `tree-sitter-sequel` declares its
    build-dependency as `cc = "~1.2.1"` (a `~` pin, so `1.11.0` does not
    satisfy it) and cargo resolves the only version every other crate
    accepts. Nothing in a manifest changed and every grammar of the book
    (the nine and the four) compiles and passes its tests with `cc`
    1.2.67 — the gate runs above are the evidence — but a reviewer watching
    the lockfile will see a shared build dependency move because of the SQL
    grammar crate. `crates/gx-index/tests/book.rs` also changed: the refusal
    test used `ruby` as the archetype of a language the build does not know,
    which Batch 2 made false, so it now derives the unknown language from
    the registry (a later batch: kotlin/swift/dart/scala/zig) and keeps
    meaning under any feature set.
- L9 (2026-11-07, orchestrator, user decision — single binary): the
  DEFAULT build now ships every book language. The user never installs or
  compiles anything; unused grammars are lazy static pages, and a
  project's book only gets chapters for the files it actually has —
  runtime selectivity comes from what the tree contains, not from build
  flags. The cargo features remain for slim opt-out builds.
  - `crates/gx-index/Cargo.toml`: `default` = all thirteen `lang-*`
    features (comment records the decision).
  - `.github/workflows/ci.yml`: RNF1-quater gate 15,700,000 → 41,800,000 B
    (declared ceiling 40 MB, INTERIM — T6 re-measures with the twenty
    grammars and tightens it; batch 3/4 may breach early and the ceiling
    adjusts with the user).
  - PRD rewritten in three places: RNF1 (<15 → <40 MB interim),
    RNF1-quater (evolution note: F0's breakdown never included grammars),
    and the features paragraph (all thirteen ON, measured numbers).
  - Measured: default release **31,217,432 B** with thirteen grammars;
    slim rust+typescript **13,241,496 B** (byte-identical to the batch-1
    record — the engine refactor moved nothing). Gates: default build 0,
    `cargo test --workspace` 0 (42 suites, 0 failed — every language in
    the default run), slim build 0 + slim tests 0.
  - Note: batch-2 worker had already measured all-13 at 31,217,432 B via
    explicit features — both paths agree to the byte.
- L10 (2026-11-07, worker, T3 Batch 3: S2 S4): Swift, Kotlin and Dart
  landed — **and, unlike batches 1-2, inside `default`**: this batch rides
  the single-binary contract of L9, so the three features are listed in
  `default` from the start and the default `graphium` grows. **The schema
  needed nothing again**: no `SCHEMA_VERSION` bump, no `CHECK`, no migration
  (L7's contract held for three more languages).
  - **Grammar provider — one loud deviation (Kotlin).** The crate called
    `tree-sitter-kotlin` on crates.io is **unviable** for this engine: the
    last release is 0.3.8 and it declares `tree-sitter >=0.21, <0.23` as a
    **normal** dependency (not a dev one), so the `Language` it hands out
    belongs to another tree-sitter ABI and cannot be loaded here. Kotlin
    landed on **`tree-sitter-kotlin-ng` 1.1.0** — the
    `tree-sitter-grammars/tree-sitter-kotlin` grammar (the same repository
    the other batch crates come from) published under a free name at the
    engine's ABI (`tree-sitter-language` 0.1, parser ABI 14). The feature is
    `lang-kotlin` — it names the language, not the crate (L8's SQL rule).
    Swift is `tree-sitter-swift` 0.7.4 (ABI 15) and Dart
    `tree-sitter-dart` 0.3.0 (ABI 15); all three were verified by the
    registry test that types every node kind of their tables against the
    grammar (`node_kind_for_id`) before a single fixture ran.
  - **The three languages (Batch 3)** — the registry entry of each is the
    whole of what the engine knows:
    - `lang-swift` (`.swift`): the module is the path of the file
      (`src.store`), because Swift declares no package in the source. Swift
      spells class, struct, actor, enum **and extension** with one node kind
      (`class_declaration`, told apart by its `declaration_kind` keyword), so
      those five kinds are the one hook (`swift::type_declaration`): a class
      and an actor are `class`, a struct and an enum a `type`, a protocol a
      `trait` (the vocabulary's interface), a `typealias` a `type` and an
      `associatedtype` a `type`. **The extension decision:** `extension
      DiagramStore: Printable { … }` adds members and a conformance to a type
      another declaration owns — exactly what a Rust `impl Trait for Type`
      block is — so it is a symbol of kind **`impl`** named after the type it
      extends (the type and its extensions share a `qname` and the book
      numbers the collision the way it numbers two `impl` blocks of one type),
      and the conformance it lists is **`inherits`**: the same relation the
      declaration itself would spell for that protocol, hung from the
      declaration that spells it. A `where T: Loader` clause is a constraint
      on a generic parameter, not a declaration inheriting, so it is not read
      as `inherits`. A property at the root of the file is a `const` and a
      property inside a type is a field — not a declaration of the book — and
      `deinit`/`subscript` spell their name with a keyword and no field (the
      `swift::member` hook). Relations: `calls` (`openStore(input)`,
      `other.load(input:)`, `DiagramStore(…)`), `imports` (the last segment of
      an `import`, so `import struct Store.DiagramStore` → `DiagramStore`),
      `inherits` (every `inheritance_specifier`) and `uses` (parameter,
      return and property types, and the type of a `typealias`).
    - `lang-kotlin` (`.kt`, `.kts`): **Module from the `package` the file
      declares** (like Java's `package` and C#'s `namespace`), with the
      directory as the fallback. Kotlin spells a class and an interface with
      one node kind and marks an enum with a `class_modifier`, so the kind is
      the hook `kotlin::type_declaration`: interface → `trait`, `enum class` →
      `type`, everything else → `class`. **The companion decision:** a
      companion object is a declaration — it has a name, the one it declares
      or **`Companion`**, which is the name the language gives the unnamed one
      and the one the source uses to reach its members — so it is a `class`
      with its members nested under it; its `const val` is a property of that
      class and not a declaration, like every other field. **A data class is a
      class**: the `data` modifier synthesises `copy`/`componentN` and changes
      no declaration, so the book keeps the class the source spells and never
      a synthesised member (Ruby's metaprogramming rule of L8); its
      primary-constructor `val`s are properties, not declarations. A secondary
      constructor and an `init` block declare no name of their own and are not
      symbols; `val LIMIT = 3` at the root of the file is a `const`.
      Relations: `calls`, `imports` (last segment of the path, or the alias),
      `inherits` (every supertype of the `:` list: `Base()`, `Loader`,
      `Loader by delegate`) and `uses` (parameters — `class_parameter`
      included —, return types, property types, the underlying type of a
      `typealias`).
    - `lang-dart` (`.dart`): the module is the path of the file (`lib.store`)
      — a `library` directive names a library, never a path — and a class is a
      `class`, a mixin a `trait`, an enum, an `extension` and an `extension
      type` a `type`, a `typedef` a `type` and a top-level variable a `const`.
      Dart spells a member one wrapper at a time (`member_declaration` →
      `method_declaration` → `method_signature` → `function_signature`): the
      name lives in the innermost node and the span the book needs is the
      outermost (the calls of a body hang from the method that owns them), so
      the declarations are the batch's hook (`dart::declaration`) — a `fn` at
      the root of the file and a `method` inside a type. A constructor is a
      `method` named as the source spells it (the class name for the unnamed
      one, its own name otherwise); a field is not a declaration. **The mixin
      decision:** `class C extends Base with Loggable implements Loader` reads
      `extends`, every `with` and every `implements` as **`inherits`** — a
      Dart mixin is an embedding, the relation PHP gives `use TraitName` and
      Ruby gives `include` — while `mixin Loggable on Base`'s `on` clause is a
      **use-site constraint**, not a supertype, and is read as `uses`. A
      `mixin C = Base with M;` (`mixin_application_class`) is not read as a
      declaration: that form is rare and stays out until a fixture needs it,
      and the module says so. Relations: `calls` (`call_expression`,
      `new_expression`, a named constructor), `imports` (the alias of an
      `import`, else the stem of the URI: `package:graphium/render.dart` →
      `render`; an `export` reads the same), `inherits` and `uses` (parameter,
      return, field and top-level-variable types, and the type an `extension`
      extends).
  - **Language N+1 after this cut**: still one module + one registry entry +
    dependency/feature + fixture/floor + PRD row + re-measure, and still no
    schema change. Batch 3 needed no new kind of engine work either: three
    table walks, four kinds of hook (a node kind that is several declarations
    in Swift and Kotlin, a declaration whose name lives in a child signature
    in Dart, and the properties/constants of the three), and 0 lines of
    `schema.rs`.
  - **D3** (`-- --nocapture`, batch features on). Batch 3, on **its own**
    fixture tree (the rule of L7/L8): **Swift 24,1 % (7/29), Kotlin 26,9 %
    (7/26), Dart 25,9 % (7/27)**. Floors pinned at **20 / 23 / 22 %**
    (measured minus 2,9–4,1 points, never inflated). The three are the
    Python case of the book: their declarations name `Int`, `String`, `int`,
    `Renderer` and other stdlib/external types the book does not hold, and
    the published number says so instead of hiding it. The **thirteen**
    existing languages, same tests, same `--nocapture`: **byte-identical**
    table — TypeScript 48,0 % (12/25), Rust 36,4 % (4/11), Java 29,6 %
    (8/27), Go 17,2 % (5/29), Python 14,8 % (4/27), JavaScript 66,7 %
    (8/12), C 81,8 % (9/11), C++ 71,4 % (10/14), C# 66,7 % (10/15), PHP
    95,0 % (19/20), Ruby 66,7 % (6/9), SQL 100 % (4/4), Shell 37,5 % (3/8).
    Differential check of the reusable technique of L5: the base tree
    (`git archive HEAD` into a `mktemp -d`, its own target dir, same command
    and `--nocapture`) prints the **same four lines byte for byte** for the
    thirteen (the base tree has no batch-3 line), exit **0** — the change
    moved nothing of the languages that already existed.
  - **Binary — MEASURED AND REPORTED LOUDLY.** Default `graphium`
    (release, opt-level=z, lto, strip, the CI command
    `cargo build --release --bin graphium`): **40.318.232 B**, from
    31.217.432 B with thirteen grammars — **+9.100.800 B** for the three
    new grammars (~3,03 MB each). That is **96,5 % of the 41.800.000 B CI
    gate** and — the number that matters for the next decision — **0,8 %
    over the declared 40 MB ceiling** of L9, which T6 re-measures with the
    twenty grammars. The gate itself is **not** breached, so nothing in
    `ci.yml` was touched (the ceiling is not this cut's), but batch 4
    (Scala, R, Lua, Zig) cannot add four grammars of this size under
    41.800.000 B: the orchestrator and the owner decide the ceiling before
    that batch, not after. Verified `-v`: the release unit of `gx-index`
    carries all **sixteen** `lang-*` features, so the number is the default
    build's and not a slim one's. The **slim** reference is byte-identical to
    L9: `-p gx-app --no-default-features --features "lang-rust,lang-typescript"`
    = **13.241.496 B** (the engine refactor and the three grammars cost the
    slim build nothing).
  - **Gates (exit codes without pipes)**: `cargo test --workspace` **0** (42
    suites ok, 0 failed — every language of the sixteen in the default run);
    `cargo test -p gx-index` **0** (lib 78, book 22, end_to_end 3, languages
    37, relations 7); `cargo fmt --check` **0**;
    `cargo clippy --workspace --all-targets` **0** (0 warnings); each new
    feature alone compiles (`-p gx-index --no-default-features --features
    lang-X` **0** for the three; the Kotlin-only slim build warns about
    `dotted_module`, the same pre-existing pattern of L7's C#-only build).
    The known `--no-default-features` failures of L5 are untouched and out of
    this cut.
  - **One stale unit test fixed in passing (inside this cut's surfaces)**:
    `crates/gx-index/src/lang/mod.rs` asserted `grammar("kotlin").is_none()`
    as the archetype of a language the build does not know; Kotlin is now one
    of the sixteen, so the assertion moved to `scala` (Batch 4). The book's
    refusal test derives its unknown language from the registry and still
    finds `scala`/`zig`.
  - **Scope note for the reviewer**: `Cargo.lock` (workspace root) changed —
    three packages added (`tree-sitter-swift` 0.7.4, `tree-sitter-kotlin-ng`
    1.1.0, `tree-sitter-dart` 0.3.0) and **nothing moved**: no version of an
    existing crate changed this time (`cc` stays 1.2.67, `tree-sitter-language`
    stays 0.1.9), unlike L8's `cc` downgrade. The lock is outside the listed
    surfaces and the dependencies cannot land without it (L7's note verbatim).
    No other surface was touched: `crates/gx-app`, `gx-mcp`, `gx-core`,
    `gx-kinds`, `gx-layout`, `schema.rs` and `.github/workflows/ci.yml` are
    untouched.
- L11 (2026-11-07, orchestrator): the declared ceiling grows 40 → 55 MB
  (CI gate 41,800,000 → 57,600,000 B). Batch 3 measured the default at
  40,318,232 B — under the gate but 0.8% over the interim 40 MB — and
  batch 4 (Scala, R, Lua, Zig at batch-3's ~2.3 MB average) cannot fit
  under the old gate. 55 MB covers the twenty-grammar estimate (~52 MB);
  T6 closes the number with the real measurement and may tighten it.
  PRD updated in RNF1, RNF1-quater and the features paragraph; ci.yml
  gate moved. This is arithmetic under the owner's single-binary policy
  (2026-11-07), not a new decision.

- L12 (2026-11-07, worker, T3 Batch 4: S2 S4): Scala, R, Lua and Zig landed —
  **the twenty languages of the approved list are complete.** Inside `default`
  from the start (the single-binary contract of L9), and **the schema needed
  nothing for the fourth time**: no `SCHEMA_VERSION` bump, no `CHECK`, no
  migration (L7's contract held for four more languages without one line of
  `schema.rs`, which this cut may not touch).
  - **Grammar providers — no substitution this time.** `tree-sitter-scala`
    0.26.9, `tree-sitter-r` 1.3.0, `tree-sitter-lua` 0.5.0 and
    `tree-sitter-zig` 1.1.2: all four publish `tree-sitter-language` 0.1 (the
    ABI of the engine) as a **normal** dependency and their parsers declare
    language version 14 or 15, so every `Language` loads in this engine as-is.
    The registry test that types every node kind of every table against its
    grammar (`node_kind_for_id`) passed for the four before a fixture ran, and
    it caught nothing — unlike L8's SQL and L10's Kotlin, no crate had to be
    swapped.
  - **The four languages (Batch 4)** — the registry entry of each is the whole
    of what the engine knows, and each decision below is written in its module
    and in the entry's doc:
    - `lang-scala` (`.scala`, `.sc`): **module from the `package` the file
      declares** (like Java, C#, PHP and Kotlin), directory as the fallback.
      **The `object` decision:** a Scala `object` defines a class *and its only
      instance* — the spec's singleton object, and the compiler emits a real
      class — so it is a **`class`** of the book and its members nest under it
      (`graphium.store.Defaults.open`); it is **not** the `module` kind, because
      an object is a value that can extend a class, implement a trait and be
      passed around, and the fixture's `object Defaults extends Loader` reads as
      `inherits` from it. **A `case class` is a class** (the `case` modifier
      synthesises `apply`/`unapply`/`copy` and changes no declaration the source
      spells: Kotlin's data-class rule), a `trait` is a `trait`, a Scala 3 `enum`
      a `type`, a `type X = Y` a `type`. `val`/`var` name their binding in the
      `pattern` field — the one hook — so a root-level binding is a `const` and a
      `val`/`var` inside a type is a field (no declaration, the rule of every
      chapter), and an abstract `val` of a trait declares no value. A companion
      class and object share a name and the book numbers the second symbol with
      the contract it already has (`#2`), pinned by a unit test. Not read, on
      purpose and documented: a `given` (it names itself only sometimes), an
      `extension` (its type lives in the parameter list) and a `package object`
      (it *is* the package) are not declarations, and a `derives` clause is a
      generated instance, not `inherits` (Swift's `where` rule). Relations: all
      four kinds — `calls` (`parseConfig(x)`, `store.load(x)`, the `apply` sugar
      `DiagramStore(…)` and `new DiagramStore(…)`), `imports` (last segment, and
      the **alias** of a renamed import: `import a.b.{Painter => Renamed}` binds
      `Renamed`), `inherits` (every type of an `extends` clause, `with`
      included) and `uses` (parameter, return, `val`/`var` and type-alias types;
      a path type names its last segment).
    - `lang-r` (`.r`, `.R` — a script is spelled both ways and the extension
      never depends on the case the author chose): the file is the module
      (`R/store.R` → `R.store`). **The R decision: an assignment creates a
      variable, and `f <- function(…)` is a `fn`.** The grammar gives
      `function_definition` the keyword (`function`) in its `name` field and
      never the bound identifier, so the name comes from the assignment: the one
      hook reads the `operator` **field** of `binary_operator` — `<-`, `<<-`,
      `=` and `:=` bind the left name, `->`/`->>` the right one — and writes a
      `fn` when the value is a function definition and a `const` otherwise
      (R has no types, so a binding is a binding; Python's module-assignment
      rule). A binding inside a body is not a declaration of the chapter.
      `assign("x", 3)`, `setClass("Foo", …)` and the R6 `R6Class("Foo", …)`
      **stay calls**: a class synthesised from a string argument is a semantic
      pass, and the book publishes what the syntax tree spells (Ruby's
      metaprogramming rule of L8). Honest limit, in the module: a function
      written the right way round (`function(…) … -> f`) parses the operator
      *inside* the function body, so it declares no `fn` because the grammar
      says it is a body. Relations: **`calls`** (every `call`, with the name of
      the member for `store$load(…)`/`pkg::fun(…)`) and **`imports`** (`source`/
      `sys.source` by the stem of the path they read, `library`/`require`/
      `requireNamespace` by the package they name — Shell's rule of L8) — and
      **no `inherits` and no `uses`**: R's inheritance lives in string arguments
      of calls and a type is a runtime attribute, never syntax this grammar
      marks. Two relation kinds by decision, and the fixture measures two.
    - `lang-lua` (`.lua`): the file is the module (`src/store.lua` →
      `src.store`; `init.lua` names its directory, the language's convention).
      **The Lua decision: a table is the module, and a table of functions is
      what the language calls a module.** So `local Store = {}` is a **`module`**
      symbol, `function Store.open(…)` a **`fn`** under it and
      `function Store:load(…)` a **`method`** under it — the colon form takes the
      implicit `self`, which is the one thing Lua calls a method. **The two
      spellings are one declaration**: `M.f = function(…) end` is sugar-identical
      to `function M.f(…)`, so both write `fn f` under `M`; any other root
      binding is a `const` (`local LIMIT = 3`, `local painter = require(…)`), and
      a field that is only data (`M.count = 1`) is not a declaration (the field
      rule of every chapter). A table the file never declares still gets its
      `module` symbol — created at the table the first function names and
      **reused** when the declaration comes later, so a chapter never holds two
      symbols of one `qname` by accident (pinned by a unit test). Relations:
      **`calls`** (every `function_call`: `parse_config(x)`, `Store.load(x)`,
      `self.load(x)`) and **`imports`** (`require("lib.render")` → `render`) —
      no `inherits` and no `uses`, because the language has neither types nor
      supertypes.
    - `lang-zig` (`.zig`): the file is the module (`src/store.zig` →
      `src.store`). **The Zig decision: a `const` that holds a type is a `type`,
      and the declarations of that type live inside the `const`.** Zig has no
      class and no namespace keyword: a type is
      `const X = struct { pub fn m() … }`, and the grammar spells
      `variable_declaration` with **no field at all** (the identifier is the
      first named child and the value the last), so one hook reads it: a
      `struct`/`union`/`enum`/`opaque` value makes a **`type` container** and the
      `fn`s inside it are its **`method`s** (`src.store.DiagramStore.load`); any
      other value is a **`const`** of the file (`const LIMIT: usize = 3`), which
      is the binding the language refers to. A `fn` at the root is a `fn`, a
      `container_field` is a field (no declaration), a `test "…"` block declares
      nothing, and `const std = @import("std")` is a `const` like any other
      binding. **No `inherits`**: Zig is not object-oriented and has no
      supertypes — the kind is not invented for it. Relations: `calls`
      (`parseConfig(a, b)`, `store.load(x)`, `DiagramStore.init(p)`),
      `imports` (`@import("render.zig")` → `render`: a builtin named `@import`
      is the import statement the language gives) and `uses` (parameter, return,
      `const` and field types; a user type is a plain `identifier` in this
      grammar and a `builtin_type` — `usize`, `u8` — is not a declaration).
  - **Language N+1 after this cut**: still one module + one registry entry +
    dependency/feature + fixture/floor + PRD row + re-measure, and still **no
    schema change**. Batch 4 needed no new kind of engine work: four table
    walks, **six hooks in total** (Scala's two `val`/`var` definitions, R's one
    assignment, Lua's two, Zig's one `const`), one file (`emit.rs`) widened by
    one `cfg` so a Scala-only build compiles the directory fallback. The list is
    now complete, so there is no language N+1.
  - **D3** (`-- --nocapture`, the four features on). Batch 4, on **its own**
    fixture tree (the rule of L7/L8/L10), which for Scala, R and Lua also holds a
    second file of the same language with what the first one imports
    (`graphium.render`, `R/render.R`, `lua/render.lua`) — a real tree has those,
    and D3 is measured on a tree: **Scala 48,5 % (16/33), R 50,0 % (4/8),
    Lua 80,0 % (4/5), Zig 87,5 % (7/8)**. Floors pinned at **45 / 46 / 76 / 83 %**
    (measured minus 3,5–4 points, never inflated). Scala and R are the Python/
    Swift case — their signatures name `Int`, `String` and functions of packages
    the book does not hold — while Lua and Zig resolve most of what they declare
    because the declarations and the calls live in one file; Zig has one import
    edge per chapter (`@import` names a file, not a symbol) and R and Lua declare
    two relation kinds by decision, so their edge counts are small on purpose.
    The **sixteen** existing languages, same tests, same `--nocapture`:
    **byte-identical** table — TypeScript 48,0 % (12/25), Rust 36,4 % (4/11),
    Java 29,6 % (8/27), Go 17,2 % (5/29), Python 14,8 % (4/27), JavaScript
    66,7 % (8/12), C 81,8 % (9/11), C++ 71,4 % (10/14), C# 66,7 % (10/15), PHP
    95,0 % (19/20), Ruby 66,7 % (6/9), SQL 100 % (4/4), Shell 37,5 % (3/8),
    Swift 24,1 % (7/29), Kotlin 26,9 % (7/26), Dart 25,9 % (7/27). Differential
    check of the technique of L5/L10: the base tree (`git archive HEAD` into a
    `mktemp -d`, its own target dir, same command and `--nocapture`) prints the
    **same four lines byte for byte** — the base tree has no batch-4 line — exit
    **0**: the cut moved nothing of the languages that already existed.
  - **Binary — MEASURED, UNDER THE GATE.** Default `graphium` (release,
    opt-level=z, lto, strip, the CI command `cargo build --release --bin
    graphium`): **40.318.232 B before → 45.789.848 B after**, **+5.471.616 B**
    for the four grammars (≈1,37 MB each, half of batch 3's ≈3,03 MB) — **79,5 %
    of the 57.600.000 B gate** and comfortably under the declared 55 MB, below
    the ~52 MB the L11 ceiling change assumed. `ci.yml` untouched (the ceiling is
    not this cut's). The **slim reference is byte-identical to L5/L9/L10**:
    `-p gx-app --no-default-features --features "lang-rust,lang-typescript"` =
    **13.241.496 B** — the four grammars cost the slim build nothing. Node kinds
    of all four (`template_body`, `method_index_expression`, `container_field`,
    plus Swift's `inheritance_specifier`) were found in the stripped binary, so
    the measurement is the twenty-language default and not a slim one.
    **Why the CI command measures twenty and `-p gx-app` does not** (verified
    both ways on this tree, not inferred): a bare `cargo build` in a virtual
    workspace selects **every** default member, so `gx-index` is a selected
    package and its own `default` (the twenty `lang-*` features) is enabled;
    `cargo build --release --bin graphium -p gx-app` selects only the app, which
    forwards five languages through `gx-mcp`, and measures **14.363.032 B with
    no batch-4 node kind in the binary** (byte-identical to the five-language
    default of L7). The gate is the first command, so the published number is the
    twenty-language one — and the forwarding of `gx-app`/`gx-mcp` (four
    languages of the book, outside this cut's surfaces: L9's single-binary
    decision landed the grammars in `gx-index`'s `default` and did not touch the
    two forwarding lists) is a finding for the orchestrator, not a change this
    batch could make.
  - **Gates (exit codes without pipes)**: `cargo test --workspace` **0** (42
    suites ok, 0 failed — every one of the twenty languages in the default run);
    `cargo test -p gx-index` **0** (lib 88, book 22, end_to_end 3, languages 46,
    relations 7); `cargo fmt --check` **0**;
    `cargo clippy --workspace --all-targets` **0** (0 warnings);
    `cargo build --release --bin graphium` **0**;
    `cargo build -p gx-index --no-default-features --features lang-X` **0** for
    each of the four (the Scala-only slim build warns about `dotted_module`, the
    pre-existing pattern of L7/L8/L10's slim builds: this grammar takes the
    directory fallback and not the dotted module path). The known
    `--no-default-features` failures of L5 are untouched and out of this cut.
  - **Two stale tests fixed in passing (inside this cut's surfaces)**:
    `crates/gx-index/src/lang/mod.rs` asserted `grammar("scala").is_none()` as
    the archetype of a language the build does not know; Scala is now one of the
    twenty, so the assertion moved to `julia` (an alternate of L6, not a
    language of the book). `crates/gx-index/tests/book.rs` derived its unknown
    language from `["kotlin", "swift", "dart", "scala", "zig"]`, which this batch
    emptied — it now derives it from the alternates that no batch landed
    (`julia`/`elixir`/`ocaml`/`objectivec`), so it keeps meaning whichever
    features built the registry.
  - **Scope note for the reviewer**: `Cargo.lock` (workspace root) changed — four
    packages added (`tree-sitter-scala` 0.26.9, `tree-sitter-r` 1.3.0,
    `tree-sitter-lua` 0.5.0, `tree-sitter-zig` 1.1.2, +44 lines) and **nothing
    moved**: no version of an existing crate changed (`cc` stays 1.2.67,
    `tree-sitter-language` stays 0.1.9), unlike L8's `cc` downgrade. The lock is
    outside the listed surfaces and the dependencies cannot land without it (L7's
    note verbatim). No other surface was touched: `crates/gx-app`, `gx-mcp`,
    `gx-core`, `gx-kinds`, `gx-layout`, `schema.rs` and `.github/workflows/ci.yml`
    are untouched. `docs/PRD.md` is updated in RNF1, RNF1-quater, the D3
    paragraph and the features paragraph (twenty, measured numbers, the four
    grammar crates).
- L13 (2026-11-07, worker, T6: S1 S4 — the FINAL task of the cut): the
  coverage of the weak languages, the metric's honest fix, and the cut
  closed. **The user's original ask of L1/S1 is this task materialized**:
  «la cobertura de los lenguajes que tenemos es variable y baja en algunos,
  hay que hacer modificaciones para mejorar esto».
  - **The two kinds of weakness of L3, split before deciding.** (a)
    RESOLUTION targets that cannot exist — Python `str`/`int`/`len`/`dict`,
    Go `fmt`/`len`/`make`/`string`, Rust `Path`: no declaration of a
    worktree can ever answer them. (b) SYMBOLS that exist but are not
    declared — Python `self.x`, Go package paths. Both were measured
    before deciding anything, and **the decision went to (a) alone**.
  - **CHOSEN (a): exclusion by knowledge, as data of the registry.**
    `Grammar` gained `builtins: &'static [&'static str]`
    (`lang/engine.rs`) and the registry carries three lists —
    `PYTHON_BUILTINS`, `GO_BUILTINS`, `JAVA_BUILTINS` — of the names the
    language's **runtime and standard library** provide, folded the way
    `name_norm` folds a target. `Book::resolution()`
    (`crates/gx-index/src/resolution.rs`) leaves a relation whose target is
    one of them **out of the numerator and out of the denominator**: the
    book indexes a worktree, and no worktree declares `str`, `len` or
    `String`, so counting them measures what the book cannot be. The rate
    then answers what **can** resolve, which is what D3 was always meant to
    publish.
  - **The guard that makes the rule impossible to abuse** (the reason the
    metric stays honest): **a name any chapter of the book declares is
    measured like any other name**. The predicate is
    `!definers.contains_key(norm) && runtime.get(lang).contains(norm)`, so
    a worktree that spells `def len` counts the edge to `len` and the rule
    can only remove what was already unresolvable by design — it can never
    hide a resolution, and applying it can only move a rate up. Pinned by
    the new test `the_rate_leaves_out_the_names_the_runtime_provides_and_
    only_those` (a Python chapter that declares `len`: 1/1 = 100 %; the same
    chapter without the declaration: `len` and `int` leave the measure and
    the call to `helper` stays).
  - **The graph does not change.** The relation is still read, stored and
    answered by `code_impact`: the rule is the metric's, never the book's
    (asserted in the same test: the chapter keeps its 3 edges while the
    report measures 1). No schema, no insert path, no retriever touched.
  - **Scope of the lists, stated on purpose: only the three weak languages
    of this cut carry one.** The other seventeen keep publishing their rate
    over every relation they read, so **their published numbers do not move
    one digit** — the improvement stays attributable and the differential
    check of the house (below) stays a byte-diff. The trade-off is on the
    record: for those seventeen the rate still counts runtime names, so a
    reader compares Python's 40,0 % with Swift's 24,1 % knowing that the two
    denominators are defined differently. Filling a list for another
    language is one line of data + a measured cut, not code; the recommended
    next one is Swift/Kotlin/Dart/Scala/R, the five that measure 24–50 % for
    the Python reason.
  - **REJECTED (b1) Python `self.x` — implemented, measured, reverted.** An
    attribute assignment `self.x = …` inside a method declaring a
    `const`-kind symbol anchored to its class was cheap (one hook on
    `function_definition` replacing the table leaf, ~35 lines). Measured on
    the C2 tree: Python **40,0 % (4/10) → 60,0 % (9/15)** and the symbol
    count 11 → 12 — and the whole gain is a lie: the five extra edges are
    `Path`/`path` (`pathlib.Path`, `os.path`) resolving against the
    instance attribute `path`, and the new definer **also changes Rust's
    published number, 36,4 % (4/11) → 54,5 % (6/11)**, by giving another
    language's `Path` edge a target it does not have. It also breaks the
    book-wide rule that a field is not a declaration (L7 C#, L12 Go
    `struct` fields and Zig `container_field`). Not landed.
  - **REJECTED (b2) Go package paths — measured, already resolved.** A
    two-file Go package (`internal/store/store.go` + `cmd/main.go` with
    `store.OpenStore()` and `s.Load("y")`) measures **5/6 = 83,3 %** with the
    rule of (a): the package-qualified call and the method through the
    receiver resolve **by name today** (the resolver is book-wide), and the
    one edge left is exactly L3's weakness — `imports store`, because a
    chapter only declares its `module` anchor when a file-scope relation
    needs it (`relations::attach`). Closing it means declaring the anchor in
    every chapter, which changes the symbol list of all twenty chapters and
    the fanout of common module names (`store`, `utils`, `types`) — i.e. it
    moves the published numbers of languages that must not move. Recorded as
    the follow-up if the owner wants a second pass; **not** part of this cut.
  - **`MAX_NAME_FANOUT` — evaluated and left at 5.** After (a) the fanout is
    what the remaining unresolved edges of the three are made of
    (`load` with 10 definers, `DiagramStore` with 6); raising the cap would
    convert those into false positives and buy the same number by lying.
    Untouched, as instructed.
  - **D3 before/after (the numbers that decide the cut).** Same tree, same
    report, `-- --nocapture`:

    | language   | before           | after            | floor pinned |
    |------------|------------------|------------------|--------------|
    | Python     | 4/27 = 14,8 %    | **4/10 = 40,0 %**| 36 % (was 12)|
    | Go         | 5/29 = 17,2 %    | **5/10 = 50,0 %**| 46 % (was 15)|
    | Java       | 8/27 = 29,6 %    | **8/12 = 66,7 %**| 63 % (was 27)|
    | TypeScript | 12/25 = 48,0 %   | 12/25 = 48,0 %   | 45 % (held)  |
    | Rust       | 4/11 = 36,4 %    | 4/11 = 36,4 %    | — (held)     |

    The numerator did not move in any of the three: what changed is what
    counts. What is left unresolved in them after the rule is **only** the
    fanout ceiling plus, in Python, one relative import of a module the tree
    does not hold. The other seventeen, byte-identical: JavaScript 66,7 %
    (8/12), C 81,8 % (9/11), C++ 71,4 % (10/14), C# 66,7 % (10/15), PHP
    95,0 % (19/20), Ruby 66,7 % (6/9), SQL 100 % (4/4), Shell 37,5 % (3/8),
    Swift 24,1 % (7/29), Kotlin 26,9 % (7/26), Dart 25,9 % (7/27), Scala
    48,5 % (16/33), R 50,0 % (4/8), Lua 80,0 % (4/5), Zig 87,5 % (7/8).
  - **Differential check (the technique of L5/L10/L12)**: the base tree
    (`git archive HEAD` into a `mktemp -d`, its own target dir, same command
    and `--nocapture`) printed the same four batch lines; `diff` of the
    sorted tables has **exactly one differing line** — the C2 line — and
    inside it Rust and TypeScript are byte-identical: only Python, Go and
    Java moved. Exit 0 both sides.
  - **Binary — the final measurement of RNF1-quater with the twenty
    grammars** (release, opt-level=z, lto, strip, the CI command
    `cargo build --release --bin graphium`): **45.789.848 B before →
    45.806.232 B after**, **+16.384 B** — the three runtime name lists are
    data the linker keeps, and nothing else of T6 costs a byte.
    **79,5 % of the 57.600.000 B gate**, 43,7 MiB, comfortably under the
    declared 55 MB. The **slim** reference is byte-identical to
    L5/L9/L10/L12: `-p gx-app --no-default-features --features
    "lang-rust,lang-typescript"` = **13.241.496 B**. **Recommendation to the
    orchestrator/owner: tighten the declared ceiling 55 → 48 MB** (gate
    50.331.648 B, 9,9 % of air over the measurement — room for one more
    grammar the size of the biggest of batch 3, not for silent
    accumulation). The ceiling and `ci.yml` are not this cut's surfaces, so
    no byte of `ci.yml` moved.
  - **Gates (exit codes without pipes)**: `cargo test --workspace` **0**
    (42 suites, 0 failed — the twenty languages in the default run;
    `-p gx-index`: lib 89, book 22, end_to_end 3, languages 47, relations 7);
    `cargo fmt --check` **0**; `cargo clippy --workspace` **0** (0
    warnings); `cargo build --release --bin graphium` **0**; the slim build
    **0**. The known `--no-default-features` failures of L5 are untouched and
    out of this cut.
  - **PRD closed**: RNF1 and RNF1-quater carry the final measurement and the
    tightening recommendation; the D3 paragraph carries the new numbers of
    the three (with the before numbers so the change is auditable) and a new
    bullet for the runtime rule — what it is, why it is honest, the guard,
    the scope of the lists, and the two options rejected with their
    measurements; the features paragraph carries the final size.
  - **Scope note for the reviewer**: no `Cargo.toml`, no `Cargo.lock`, no
    `schema.rs`, no `.github/workflows/ci.yml`, and nothing under
    `crates/gx-app`, `gx-mcp`, `gx-core`, `gx-kinds` or `gx-layout` moved.
    Files touched: `crates/gx-index/src/lang/engine.rs` (the field),
    `crates/gx-index/src/lang/registry.rs` (the three lists + the field on
    all twenty entries + one data test), `crates/gx-index/src/resolution.rs`
    (the rule), `crates/gx-index/tests/languages.rs` (the three floors moved
    up + the new guard test), `docs/PRD.md` and this doc.
