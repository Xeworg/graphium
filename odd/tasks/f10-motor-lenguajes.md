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
