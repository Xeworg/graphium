# f13-er-tool — the door hands the `er` document to the agent

The MCP tool `code_er(path)`: a `.sql` file of the project in, the `er`
version-1 document out — the seam f12 refused to touch because it is a door
surface, now built on top of `chapter_document` without moving it.

Branch `feature/f13-er-tool` from staging `f874530`, worktree
`~/Proyectos/graphium-f13`.

## Specs

- **S1** «f13: tool MCP del er» — a new door tool that hands the `er` document
  to the agent without a Rust caller. The gap f12 T3/L4 named verbatim: «what
  does not exist — on purpose, because it is a CONTRACT touch — is the MCP tool
  answer that would hand the document to the agent without a Rust caller».
- **S2** «code_er(path) → documento» — the tool is `code_er`, takes `path`
  (required, relative to the project root, `/` separators) and answers the `er`
  version-1 document in the answer. **Document, not paint**: the agent sends it
  to `render_diagram` (or `validate_diagram` first) unchanged — the
  `code_diagram` precedent («the answer is a **document**, not a paint»).
- **S3** The source is read **live** from disk (`chapter_document`: «the detail
  is read live, because the schema keeps no row for a column»); `path` resolves
  against the session's project root; an absolute path or one that escapes the
  root (`..`) is refused with `invalid_value` and nothing outside the project
  is ever read.
- **S4** Honest errors in the door's vocabulary: book off → `book_disabled`;
  session without project → `session.project` `invalid_value`; missing `path`
  → «`path` is required» (dispatch, like `code_diagram`'s symbol); a file that
  produces no document (another language, DDL with no column list, grammar not
  compiled in) answers one failure that names the three causes with a hint.
- **S5** The tool's description follows the book family contract: it says the
  book is «on by default» and names «GRAPHIUM_BOOK=off» as the opt-out;
  `TOOL_NAMES` grows to 16 and `every_tool_has_a_description_of_its_own`
  covers it.
- **S6** `INSTRUCTIONS` (THE BOOK section) mentions `code_er` in its own line;
  the existing harness tests stay green.
- **S7** Additive only: no existing behaviour changes (`er-posts` golden
  byte-identical, all goldens intact); the frontend `CONTRACT.md` and
  `gx-index/src/schema.rs` are not touched.
- **S8** Gates: RED→GREEN per task; `cargo test --workspace` 0 failed; `cargo
  fmt --check` and `cargo clippy --workspace` clean.

## Tasks

- **T1** (S1,S2,S3,S4) `book::code_er` in `crates/gx-mcp/src/lib.rs`: project
  resolution through `open()`, path guard (relative, no `..`, inside the
  project), live read, `chapter_document`, answer `ok: true` with the document
  in `diagram` — RED tests first. Route: `lib.rs` + tests. Commit: 4abdefc
- **T2** (S2,S5,S6) The surface: dispatch arm + `TOOL_NAMES` 16 +
  `tool_schema` entry + `description("code_er")` + INSTRUCTIONS line, with the
  harness tests updated (the book-family assertions include `code_er`). Route:
  `dispatch.rs`, `harness.rs`. Commit: 4abdefc
- **T3** (S7,S8) Full gates + `er-posts` byte-identical + this Log closed and
  the docs of the door updated where the book tools are listed. Route: docs,
  this file. Commit: 4abdefc (PRD), (this file: see L2)

## Log

- **L1** (2026-11-08, owner): «f13: tool MCP del er» — chosen from the pause
  list «tool MCP que entregue el er document al agente (cambio de CONTRACT,
  gap nombrado en f12 T3/L4)». Shape asked and answered: **«code_er(path) →
  documento»** — «Nueva tool: recibe path del .sql y devuelve el documento er
  JSON. El agente lo manda a render_diagram/validate_diagram sin cambios, igual
  que hace hoy con code_diagram → render_diagram. Consistente con la puerta.»
  The paint-tool and auto-discovery options were offered and declined.
- **L2** (2026-11-08, orchestrator): T1+T2+T3 landed as **one work unit**
  (364 authored lines, commit 4abdefc — the cut is one behaviour, so one
  reviewable slice; nothing else rides this branch). **RED observed**: five
  tests through the public seams (`call_tool`, `tool_schema`, `description`,
  `TOOL_NAMES`) failed for absence — `unknown tool: code_er`, the fallback
  description, the schema fallback — before any implementation existed.
  **GREEN**: 9 `code_er` tests (document == `chapter_document`'s output, so no
  second mapping can drift; the four guarded refusals; the three-cause
  refusal; the book switch), gx-mcp lib **195 passed**, workspace **29 suites,
  0 failed** with the goldens intact (no `UPDATE_GOLDEN`; `er-posts`
  byte-identical), `cargo fmt --check` exit 0, `cargo clippy --workspace`
  exit 0. **Environment note** (a known trapa, met again): a fresh worktree
  needs `npm ci && npm run build` in `crates/gx-app/frontend` before any
  `cargo test --workspace` — tauri's `generate_context!` panics without
  `frontend/dist` (the first workspace run died there, not on the change).
  **Covered**: S1-S6; S7 (additive only: the socket relay derives its tool
  list from `TOOL_NAMES`, so a forwarded client sees `code_er` with the same
  description and schema — `every_tool_is_forwarded_with_its_description`
  keeps both sides equal by construction; PRD table row + F9 clause; the
  frontend `CONTRACT.md` and `gx-index/src/schema.rs` untouched); S8.
  **Deliberate pin**: a decorating `./` in the path is refused too — the
  guard reads components, not strings (`Component::Normal` only) — asserted
  by `code_er_refuses_paths_that_leave_the_project`.
  **Deliberately not built**: no paint branch (declined by the owner), no
  auto-discovery of `.sql` files (declined), no character envelope (the
  document travels whole; the `er` kind ceilings apply at the render, and the
  description says so), no composite-key or named-FK-name mapping (f12's
  documented grammar limits, unchanged here), no frontend change, no schema
  change. The cut closes: staging keeps `f874530` as its tip until the owner
  merges this branch.
