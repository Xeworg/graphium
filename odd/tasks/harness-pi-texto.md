# harness-pi-texto — the text the agents read tells the truth

The quick text cut the owner approved from the three-agent analysis: the
harness stops teaching a wrong model of the answers, teaches the cheap path
first, and the Pi extension sells the index it registers. No behaviour
change, no CONTRACT, no schema.

Branch `feature/harness-pi-texto` from staging `b376fb9`, worktree
`~/Proyectos/graphium-harness`.

## Specs

- **S1** The stale sentence «`opts.replace_id` and `opts.interactive` arrive
  in F2/F3: sending them today costs a warning, never the diagram»
  (harness.rs:65) is replaced by the truth the rest of the same file already
  teaches: `opts.replace_id` is honoured today (an unknown id answers
  `unknown_diagram` and creates nothing); `opts.interactive` still costs a
  warning. The pinning assertion (harness.rs:547) moves with it.
- **S2** The false universal «Every answer of the book is bounded in
  characters … a cut in all three tools» (harness.rs:134-141) names its three
  tools (`code_structure`, `code_impact`, `code_diagram`), and states that
  `code_search` answers one row or `candidates[]` and `code_er` the whole
  document with no envelope — and that `max_chars` on those two is refused
  (`deny_unknown_fields`).
- **S3** INSTRUCTIONS teaches the cheap path before manual authoring: project
  code → `code_search` → `code_structure`/`code_impact` → `code_diagram`; a
  `.sql` file → `code_er`; hand-write JSON only when the subject is not in
  the index. The DB bullet of WHEN TO USE IT (harness.rs:17-20) names
  `code_er` where it promises the schema picture.
- **S4** THE BOOK states the precondition (the session's client declares the
  project — `GRAPHIUM_PROJECT`/cwd; without it the answer is `invalid_value`
  at `session.project`) and the ERRORS list (harness.rs:68-76) gains
  `book_disabled`, `book_unavailable`, `ambiguous_symbol`, `invalid_value`;
  the pinning test of the codes covers the new ones.
- **S5** `code_er`'s description (and its duplicated const
  `CODE_ER_DESCRIPTION`, byte-identical) names the refusal shape — `ok:
  false`, code `invalid_value` at `path` — and that a leading `./` is refused
  too, beside absolute paths and `..`.
- **S6** `client_config("pi")` emits the JSON shape Pi actually reads (the
  `mcp.json` object documented in extensions/pi/graphium/README.md), not the
  TOML it emits today (harness.rs:412-417); the pinning test
  (harness.rs:577-589) moves with it.
- **S7** The Pi extension tells the agent what it registers: `DESCRIPTION`
  names the index and the database route (it names drawing and export only
  today, index.ts:43-45), and `/graphium status` gains one line about the
  book (`GRAPHIUM_BOOK` on/off, whether `GRAPHIUM_PROJECT` was declared).
  READMEs updated (extensions/pi/graphium/README.md, extensions/README.md).
  `verify.mjs` stays green and pins what is observable.
- **S8** Gates: `cargo test -p gx-mcp --lib` (all green, texts pinned),
  `cargo fmt --check`, `cargo clippy -p gx-mcp --all-targets`,
  `node extensions/pi/graphium/verify.mjs`. Recorded pre-existing drift: the
  other four book description consts differ from `harness::description`
  (out of scope; refactor candidate).

## Tasks

- **T1** (S1-S5) harness.rs: the four text fixes + test updates. Route:
  `crates/gx-mcp/src/harness.rs`, `crates/gx-mcp/src/lib.rs` (const sync).
  Commit: (see L2)
- **T2** (S6) client_config pi JSON. Route: `harness.rs`. Commit: (see L2)
- **T3** (S7) Pi extension. Route: `extensions/pi/graphium/`. Commit:
  (see L2)
- **T4** (S8) gates + Log. Route: this file, and the stale pin the worker
  correctly refused to touch: `crates/gx-app/src/lib.rs` (the
  `every_client_gets_its_own_configuration` assertion that pinned the old
  pi TOML). Commit: (see L2)

## Log

- **L1** (2026-11-08, owner): «dale» — approving the recommendation «texto
  (1+2) como un corte rápido» from the ranked improvements of the three-agent
  analysis. The findings this cut fixes are quoted from those reports: the
  `replace_id` sentence three other places of the same file contradict
  (harness.rs:65 vs 102,106,201-203; tools.rs:1005,1030); the envelope
  universal false for two of five book tools (`max_chars` on `code_search`
  is an error, `deny_unknown_fields`); the cheap path buried in tool prose;
  the missing `session.project` precondition; `client_config("pi")` emitting
  TOML while Pi reads JSON (known bug, README.md:96-101); the Pi blurb
  selling draw+export only.
- **L2** (2026-11-08, orchestrator): T1-T4 landed as **one work unit** (one
  commit; the cut is one behaviour: the text the agents read). The worker
  delivered S1-S7 and correctly refused the one edit outside its surfaces:
  the stale pin in `crates/gx-app/src/lib.rs` asserting pi's TOML shape —
  the orchestrator moved it to the JSON shape the cut installs (`mcpServers`
  object), so the suite pins the truth on both doors. **The verified
  wording**: the cheap path stands between HOW IN ORDER and BIG DIAGRAMS;
  `opts.replace_id` "is honoured today" with `unknown_diagram` for an
  unknown id while "`opts.interactive` still arrives in F3" costing
  `opts_ignored`; the envelope paragraph names its three tools and states
  `code_search`/`code_er` have none (and that a stray `max_chars` is refused
  as an unknown field); THE BOOK opens with the `session.project`
  precondition; ERRORS gains `book_disabled`, `book_unavailable`,
  `ambiguous_symbol`, `invalid_value` (the codes test covers them);
  `code_er`'s description and `CODE_ER_DESCRIPTION` say `invalid_value` at
  `path` and the refused leading `./`, byte-identical; pi shares the JSON
  `mcpServers` arm (identity stays the extension's job); the Pi extension
  answers with a real `bookLine()` (identity env + `GRAPHIUM_BOOK`, nothing
  invented) and the command description names the book. **Gates** (real
  exit codes): `cargo test -p gx-mcp --lib` **199 passed**, `cargo test -p
  gx-app --lib` **56 passed**, `cargo fmt --check` 0, `cargo clippy -p
  gx-mcp --all-targets` 0, `cargo clippy -p gx-app --all-targets` 0,
  `node extensions/pi/graphium/verify.mjs` **exit 0**. **A verification
  lesson, proven not assumed**: verify.mjs went red on the fresh worktree —
  the `findBinary` check demands a checkout binary (`target/`) and none
  existed; building the release in the worktree turned it green (exit 0),
  so the red was environmental, not the cut — and the gate honestly needs a
  checkout build anywhere it runs.
  **Deliberately not in this cut**: the four pre-existing
  `*_DESCRIPTION`/`harness::description` drifts (recorded, refactor
  candidate), the `code_er` size contract and `unresolved` in
  `code_diagram` (the f14 candidates), multi-file `er` (f15), and any
  change to `opts.interactive` behaviour. The cut closes on its branch;
  the merge to staging is the owner's call.
