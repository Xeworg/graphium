# f16-groups-emphasis — the neighbourhood reads like code, not like a list

`code_diagram`'s document gains the two fields the `graph` kind already
accepts and the builder never used (audit A §4.4): `groups` — one group per
file when the neighbourhood spans more than one — and `emphasis` on the seed
node, so the reader sees which symbol the question was about.

Branch `feature/f16-groups-emphasis` from staging `dfc9208`, worktree
`~/Proyectos/graphium-f16`.

## Specs

- **S1** Owner (2026-11-08): «groups + emphasis en graph» — approved from
  the audit: «groups is legal on `graph` and every `ImpactNode` knows its
  file, yet the document is version/kind/title/nodes/edges only; a
  300-node neighbourhood of qualified names is unreadable as a flat graph»
  and `emphasis` «is never used to mark the seed node, so a reader cannot
  tell which node the agent asked about».
- **S2** The document gains `groups` (the `graph` contract's own shape,
  `crates/gx-kinds/src/groups.rs` — read it first): one flat group per
  distinct file of the neighbourhood, id and label derived from the file
  path, members = the node ids declared in that file. Groups are emitted
  only when the neighbourhood spans **more than one** file; a one-file
  neighbourhood carries no groups (a single group wrapping everything is
  noise, not structure).
- **S3** The seed node always carries `emphasis` — the contract variant
  that reads as «this is the one» (`graph.rs:31`; read the `Emphasis`
  enum and pick the right variant). No other node is emphasised.
- **S4** The document stays **renderable as it arrives**: the existing
  renderability test extends to a document that carries groups and
  emphasis (validate it the same way that test does). Every field the
  answer had before remains; nothing else in the document changes.
- **S5** `description("code_diagram")` and the PRD row/prose mention the
  grouping by file and the seed emphasis, in one clause each.
- **S6** Gates: RED→GREEN (the field's absence is the RED — a test
  asserting groups/emphasis fails before the builder changes), gx-mcp lib,
  workspace 0 failed, fmt/clippy, goldens intact.

## Tasks

- **T1** (S2,S3,S4) The builder: groups by file + seed emphasis, with the
  RED test and the extended renderability assertion. Route:
  `crates/gx-mcp/src/lib.rs` + tests. Commit: (pending)
- **T2** (S5) Text: description + PRD. Route: `crates/gx-mcp/src/harness.rs`,
  `docs/PRD.md`. Commit: (pending)
- **T3** (S6) Gates + Log. Route: this file. Commit: (pending)

## Log

- **L1** (2026-11-08, owner): selected «groups + emphasis en graph» from
  the improvement menu, together with the whole roadmap (C4, gantt,
  git graph, deployment as kinds; ambiguity escape, bm25, SQL quirks as
  later cuts). f16 is the first of the sequence.
- **L2** (2026-11-08, orchestrator): T1-T3 landed as **one work unit**
  (222 authored lines, commit 7047ec1). The worker's shapes follow the
  contract exactly: `emphasis: highlight` on the seed only (S3); flat
  per-file groups built from the **kept** nodes only — a trimmed document
  never names a node it dropped — emitted only when the neighbourhood
  spans more than one file, charged to the envelope first and **dropped
  whole** when it cannot carry them, because half a grouping would lie
  about the files it leaves out (S2 with the L7 accounting). The
  renderability pin extends to a groups+emphasis document (S4). **Gates**
  (orchestrator-verified on the finished tree): gx-mcp **210 passed**,
  workspace **40 suites, 0 failed** (goldens intact, no
  `UPDATE_GOLDEN`), `cargo fmt --check` 0, `cargo clippy --workspace` 0
  warnings. The cut closes on its branch; the merge rides with the next
  integration wave (staging is already at PR #53's squash `f44988c` on
  origin/main — the next wave opens its own PR).
