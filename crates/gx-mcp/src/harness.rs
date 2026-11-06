//! The harness of the agent: when to use Graphium and when not.
//!
//! The MCP server ships this text as its `instructions`, and every tool
//! carries its own description. The goal is that an agent that has never
//! seen Graphium knows two things without reading any other document: which
//! tool to call for what, and when a diagram is the wrong answer.
//!
//! The text is written for an LLM: imperative, concrete, with the exact
//! names of the tools and the codes of the errors. It is not marketing copy.

/// The instructions the server sends to the client at `initialize`.
pub const INSTRUCTIONS: &str = "\
Graphium draws software diagrams and places them on a plan the human can pan and \
zoom. A diagram is a JSON document with a `kind`; the plan lives in a window that \
Graphium owns, so nothing is written to disk.

WHEN TO USE IT. When a picture explains the answer better than prose: the flow of \
a function or a module, dependencies between packages, the schema of a database \
(tables, columns, keys, cardinality), or an architecture of services. The human \
asked to see it, or the explanation needs a shape that words make long. \
WHEN NOT TO USE IT. For a single fact, a list of files, a diff, a stack trace or \
anything that reads better as text: a diagram there costs a window and adds \
nothing. Never draw to compensate for not understanding the code: read first.

HOW, IN ORDER.
1. `register_session` with a `label` names this session for the human (\"presupuesto \
API\"); without `label` it only answers the session you are in. The name is what the \
session selector of the window shows, so a human with two agents working on two \
projects knows which one drew what. It is optional: without it the session appears \
with its automatic name and works the same.
2. `get_diagram_schema` WITHOUT `kind` lists the available kinds with their \
limits and a minimal example. Call it with `kind` to get that kind's JSON Schema, \
its cross rules, its valid shapes and its limits. Read it before writing a \
document: the schema is the same contract the render validates, and unknown \
fields are rejected.
3. Build the document. Keep it small: a diagram that needs more than ~40 nodes to \
say one thing is several diagrams.
4. `validate_diagram` with the document. It runs exactly the same validation as \
the render and opens nothing. Fix what `errors[]` reports and call it again until \
`ok` is true. This loop is free: no window, no layout.
5. `render_diagram` with the same document. `ok: true` means the diagram is on the \
plan and `diagram_id` identifies it; `rendered: true` means the window confirmed \
the paint, and `rendered: false` comes with a `reason` (`headless`, `no_window`, \
`ui_timeout`, `ui_error`) that you can report to the human without retrying. Every \
answer also carries `session`: the id, family, label, project and state with which \
the human sees you.
6. To change a diagram you already have, use `update_diagram` with an RFC 7386 \
merge patch and the `revision` you read: it is transactional, so a bad patch \
leaves the diagram untouched. Do not render a new one to replace it.

BIG DIAGRAMS. Hundreds of nodes are not one picture: they are a view of one. \
`render_diagram` accepts `opts.extract: { around: \"<id>\", depth }` (default \
`depth: 1`, at most 300 nodes in the subgraph) and draws only the neighbourhood \
of that node, following the edges in both directions. The subgraph keeps the \
`diagram_id` of the parent and the response names it in `extracted_from` \
(`\"<diagram_id>@<depth>\"`); `get_diagram` still returns the whole document, \
because extract is a view and never an edit. Every layout also has a deadline: \
`opts.layout_timeout_ms` defaults to 2000 ms with a ceiling of 10000, and a \
layout that misses it answers `ok: false` with code `layout_timeout` and \
`metrics.elapsed_ms`, leaving the plan untouched. Renders queue in a FIFO of \
depth 8 with two laying out at a time: when the queue is full the answer is \
`ok: false` with code `busy`, `queue_depth` and `retry_after_ms`, so wait that \
long and send it again instead of retrying in a loop. `opts.focus: { id, depth? }` \
puts the camera on one node and, with `depth`, on its neighbourhood (default 1, \
max 300 nodes). `opts.replace_id` and `opts.interactive` arrive in F2/F3: \
sending them today costs a warning, never the diagram.

ERRORS. Every failure carries `{ path, code, message, hint? }` with a stable code \
and the JSON path of the problem. `unknown_ref` and `duplicate_id` are rules of a \
kind, `unknown_field` means a key the schema does not admit (`hint` carries the \
closest name), `unknown_kind` lists the kinds that exist, `too_large` means the \
document is over the limit of its kind, `stale_revision` means you read an old \
version, `layout_timeout` means the layout missed `opts.layout_timeout_ms`, and \
`busy` means the queue of renders is full: wait `retry_after_ms` and try again. Fix \
the path the error names instead of rewriting the whole document.

THE PLAN. Diagrams stay on the plan until the human closes them or \
`clear_canvas` removes them. `list_diagrams` and `get_diagram` are how you find \
out what is there and what its document was, which is what you need after a \
context compaction: call `list_diagrams` before assuming an id. The plan belongs \
to the human: the colors and the grid are their settings, not yours, and a \
diagram that is not on the screen is still on the plan.

EXPORTING. `export_diagram` takes the diagram out of the window. Two ways, and \
they are not interchangeable. `format: \"svg\"` with `opts.inline: true` returns \
the SVG as text in `content` and touches no disk: use it when the human wants \
to paste the diagram in a README, an issue or a message. With `opts.path` \
(relative to the exports root, `~/.graphium/exports/` unless configured) the \
file is written instead: `svg` for a file they will edit or scale, `png` for one \
they will look at. `png` requires `path`, because it is bytes; `scale` doubles \
the pixels for a retina README, and `max_px` is the ceiling. `inline` and `path` \
together are an error, never one winning silently. An existing file is not \
replaced unless you send `opts.overwrite: true`, and absolute paths, `..` and \
symlinks are refused with `export_path_outside_root`: the root is the only \
place Graphium writes, and a refused export writes nothing. Exporting \
never changes the plan: no diagram is opened, closed or re-rendered, so asking \
for an export is always safe. Past 1 MB of inline text the answer is \
`too_large` with `size_bytes`: send `path` or narrow the diagram with \
`opts.extract`.

HISTORY. Every render that creates a diagram and every update or \
`opts.replace_id` leaves a version of the source document behind, capped at 20 \
with the oldest discarded. `list_diagram_versions` shows them (with the current \
one) and `get_diagram_version` returns the exact document of one version; both \
are read-only, so browsing history never touches the plan. To go back, take the \
document of the version and render it again (with `opts.replace_id` to stay on \
the same diagram). Use them when the human asks what changed or wants an older \
shape back; the versions live only while the diagram is open.

THE BOOK. `code_search` answers from the index of the project instead of \
reading files: it returns the `file`, `line`, `kind`, `name`, `qname`, \
`content_hash` and `indexed_at` of one symbol, plus the `book_key` of the book \
that answered. Use it to locate the code behind a diagram, or to find a \
function by name before drawing its flow. Every word of `query` has to match \
and the tool never guesses: a query that fits more than one declaration \
answers `ok: false` with `code_search`'s `ambiguous_symbol` code and \
`candidates[]`, so send the words of one candidate's `qname` to narrow it. \
`code_structure` reads a chapter (`file`) or the chapter that declares one \
symbol (`symbol`) and answers the outline by default: only the declarations \
whose parent is `NULL`, with `total` and \
`collapsed: {hidden_symbols, declarations_with_children, next}`, where `next` \
is the copy-pasteable call that reads the hidden children. Ask for a slice of \
the tree with `after_line` + `limit` (a range over the same span-ordered list, \
never a mode) and filter the rows with `kinds`; every cut is an object, \
`truncated: {reason, shown, total, next}`. `code_impact` walks the connections \
of a symbol — `direction: \"in\"` by default (what points at it, so what a \
change would affect), `out` (what it calls or imports) or `both` — or of a \
whole chapter with a file seed, `file`: the file as a node (the connections \
view), where every declaration of it starts the walk and the answer adds a \
`files` grouping of `imports_out`, `referenced_by_in` and `symbols` per \
connected file (the seed file is `file`, never a row of `files`). `edges` narrows the kinds it follows (`calls`, `imports`, `inherits`, \
`uses`) and `budget` sets the query budget in nodes (600 by default, 2000 at \
most, the edges at twice the nodes — its own number, decoupled from the 300/600 \
a diagram document can render). The walk stops at 3 hops, answers a cycle once, \
and states every cut in `truncated: {reason, shown, total, next}`, with `next` \
as the call that continues it. `unresolved` counts the names the book could not \
pin down (resolution is by name, only while a name has at most five definers). \
Those connections are syntactic, not typed: they are read from the code and may \
be false positives. `code_diagram` turns the same neighbourhood into a `graph` \
document in `diagram`, ready to send to `render_diagram` with no repair: that \
is the cheap diagram, drawn from the book instead of from the code; it takes \
the same `edges` filter and keeps the 300/600 ceilings, so `truncated.reason` \
says `density` when the document was trimmed and `nodes`/`edges` when the walk \
was cut. The book is \
on by default: `book_disabled` only answers when the owner opted out with \
`GRAPHIUM_BOOK=off`, and a book that cannot be opened answers \
`book_unavailable`. `progress.state` tells whether the first pass is still \
`indexing`, so a symbol that is not there yet may appear in a moment. The \
book is a cache of the project and never changes the plan.";

/// The description of `register_session`, written for the agent (S3, D2).
///
/// The relay and the stdio server publish this same text, so the tool is
/// described once.
pub const REGISTER_SESSION_DESCRIPTION: &str = "\
Name this session for the human, or ask which session you are in. With `label` \
the session appears in the window's session selector with that name: use a short \
name of the work (\"presupuesto API\"), not of the tool, so a human with two agents \
working on two projects knows which one drew what. The label is trimmed before it \
is stored; an empty or whitespace-only one answers `ok: false` with code \
`invalid_value` at `path: \"label\"` and the session keeps the name it had. Without \
`label` the tool only answers the session, which is how you find out how the human \
sees you. The response is { ok, session: { id, family, label, project, state } }: \
`family` is `pi|claude|opencode|ui|other`, `project` is the directory the client \
declared and `state` is `active|in_grace`. The same `session` object travels with \
`render_diagram` and `list_diagrams`, and a session only ever sees its own.";

/// The description of a tool, written for the agent.
///
/// The relay and the stdio server both read this table, so a tool exists once
/// with one description.
pub fn description(name: &str) -> &'static str {
    match name {
        "register_session" => REGISTER_SESSION_DESCRIPTION,
        "render_diagram" => {
            "\
Draw a diagram and place it on the plan. Send the whole document as a JSON \
object in `diagram`, not as a string. The kind is required; `get_diagram_schema` \
gives its contract and `validate_diagram` checks a document without opening \
anything, so call it first when the document is long or the last render failed. \
`ok: true` returns `diagram_id`, `rendered_kind`, `metrics { nodes, edges, \
layout_ms, render_ms, svg_bytes }` and `rendered`, which tells whether the window \
confirmed the paint; when `rendered` is false, `reason` says why. `ok: false` \
means nothing was placed and `errors[]` carries `{ path, code, message, hint? }`. \
For a diagram of hundreds of nodes, `opts.extract: { around, depth }` (default \
`depth: 1`, at most 300 nodes) draws only the neighbourhood of that node and the \
response carries `extracted_from` (`\"<diagram_id>@<depth>\"`); the document \
itself is unchanged. `opts.layout_timeout_ms` (default 2000, ceiling 10000) is \
the deadline of the layout: missing it answers `ok: false`, code \
`layout_timeout`, with `metrics.elapsed_ms` and nothing on the plan. \
`opts.focus` puts the camera on a node and its neighbourhood. `opts.replace_id` \
replaces the document of the named diagram instead of creating another one: an \
unknown id answers `unknown_diagram` and creates nothing. `opts.interactive` \
arrives in F3."
        }
        "list_diagram_versions" => {
            "\
List the source-document versions of one diagram (RF14): every render that \
creates a diagram and every update or `opts.replace_id` adds one, capped at 20 \
with the oldest discarded. Read-only: a version is never restored through a \
tool — take its document with `get_diagram_version` and render it again. \
Answers `ok: true` with `current_version` and `versions[]`; an unknown or \
foreign `diagram_id` answers `unknown_diagram` or `not_owner`."
        }
        "get_diagram_version" => {
            "\
Get the exact source document of one version of a diagram (RF14): pass the \
`version` from `list_diagram_versions` and the tool answers the document that \
produced that paint. Read-only; feed the document back to `render_diagram` (or \
use `opts.replace_id`) to restore it on the plan. An unknown version answers \
`invalid_value` on path `version`; an unknown or foreign `diagram_id` answers \
`unknown_diagram` or `not_owner`."
        }
        "validate_diagram" => {
            "\
Check a diagram document WITHOUT opening anything: the same validation the \
render runs, no layout, no window. Use it before `render_diagram` when the \
document is long or the last render failed, and fix the `path` that `errors[]` \
names. `ok: true` also returns `projected_svg_bytes`, so you know the size before \
drawing. `errors_truncated` is true when there were more errors than the cap of \
20."
        }
        "get_diagram_schema" => {
            "\
The contract of a diagram kind. Without `kind` it returns the index of the \
available kinds with their phase, engine, limits and a minimal example, and \
never their schemas. With `kind` it returns that kind's JSON Schema, its cross \
rules (the ones the schema cannot express: references, uniqueness), its valid \
shapes, its limits and an example. Read it before writing a document: unknown \
fields are rejected."
        }
        "list_diagrams" => {
            "\
The diagrams of this session that are on the plan: id, title, kind, revision and \
when they were updated. Call it after a compaction or when an id was lost, before \
assuming which diagram to update or export. The answer also carries `session`, the \
name the human sees for you. It never renders anything."
        }
        "get_diagram" => {
            "\
The source document of a diagram with its current revision. Use it to read before \
an `update_diagram` (send that revision back) or to recover a document after a \
compaction. It never renders anything."
        }
        "update_diagram" => {
            "\
Change an existing diagram instead of creating another one. `patch` is an RFC 7386 \
merge patch: objects merge key by key, arrays are replaced whole, `null` removes a \
key. The merge is transactional: the result is validated with the same pipeline as \
the render and only then replaces the document, so a bad patch leaves the diagram \
untouched and returns `applied: false` with the errors. Send the `revision` you \
read, so a stale write is rejected with `stale_revision`. The `kind` cannot change."
        }
        "export_diagram" => {
            "\
Take the diagram out of the window, as text or as a file. With `format: \"svg\"` \
and `opts.inline: true` the SVG comes back in `content` and nothing is written: \
that is the path for pasting it in a README or a message. With `opts.path` the \
file lands inside the exports root (`~/.graphium/exports/` unless configured): \
relative, no `..`, no symlinks — an absolute path or an escape answers \
`export_path_outside_root` and writes nothing. `svg` and `png` both accept \
`path`; `png` REQUIRES it, because it is bytes. An existing file is replaced \
only with `opts.overwrite: true`; without it the answer is `export_exists` and \
the old bytes stay. `diagram_id` is optional: without it the tool exports the \
only open diagram of the session and answers `ambiguous_diagram` when there are \
several. `opts.scale` (default 1.0, ceiling 3.0) and `opts.max_px` (default \
16 Mpx, ceiling 64 Mpx) govern the PNG; inline SVG is capped at 1 MB. Exporting \
never changes the plan."
        }
        "code_search" => {
            "\
Search the book of this project: the symbols the indexer extracted, without \
reading the files. It answers the places of ONE symbol with `file`, `line`, \
`kind`, `name`, `qname`, `content_hash` and `indexed_at`, plus the `book_key` \
of the book that answered. `query` is split into words and every word has to \
match (`render config` finds a symbol whose name carries both); CamelCase and \
snake_case are both indexed. When the query fits more than one declaration \
the answer is `ok: false` with code `ambiguous_symbol` and `candidates[]` \
(`name`, `kind`, `qname`, `file`, `line`, `occurrences`): send the words of \
one candidate's qname to narrow it, never guess. The book is on by default: \
`book_disabled` only answers when the owner opted out with \
`GRAPHIUM_BOOK=off`, a book that cannot be opened answers `book_unavailable`, \
and `progress.state == \"indexing\"` means the first pass is still running. \
Read-only: it never touches the plan."
        }
        "code_structure" => {
            "Read the structure of one chapter of the book, and it answers the outline by \
default: only the declarations whose parent is `NULL`, ordered by span, each \
with `name`, `kind`, `qname`, `line`, `column`, `end_line`, `end_column` and \
`parent` (the qname of the declaration that contains it). The answer carries \
`count`, `total` (every declaration of the chapter, filter included) and, when \
children were hidden, \
`collapsed: {hidden_symbols, declarations_with_children, next}`, where `next` \
is the copy-pasteable call that reads them. To read a slice of the tree, send \
`after_line` (start strictly after that line) and `limit` (1..=200): they are \
a range over the same span-ordered list, never a mode, so the whole tree is a \
sequence of monotone pages. `kinds` filters the rows by declaration kind and \
never changes `total`. Send exactly one selector: `file` (a path relative to \
the project root, with `/`) or `symbol` (a name, as `code_search` takes it). A \
symbol that fits more than one declaration answers `ok: false` with \
`ambiguous_symbol` and `candidates[]`; an unknown path answers `invalid_value` \
at `path: \"file\"` with a hint, never an empty tree that reads as «no \
declarations». A cut answers `truncated: {reason, shown, total, next}` with \
reason `collapsed` (the outline hid children) or `nodes` (the limit cut the \
range). The answer carries the `content_hash` and `indexed_at` of the chapter, \
the `book_key` of the book and `progress.state` (`indexing` means the first \
pass is running). The book is on by default: `book_disabled` only answers when \
the owner opted out with `GRAPHIUM_BOOK=off`, and a book that cannot be opened \
answers `book_unavailable`. Read-only: it never touches the plan."
        }
        "code_impact" => {
            "Walk the connections of one symbol, or of one whole chapter, in the book: who \
points at it, what it reaches, or both. The seed is exactly one of `symbol` (a \
name, as `code_search` takes it) or `file` (a path relative to the project \
root, with `/`): a file seed is the connections view — the file as a node — \
where every declaration of the chapter starts the walk and the answer adds \
`file` and a `files` grouping of the connected files (`imports_out`, \
`referenced_by_in`, `symbols`). `direction` is `in` for a symbol (the default: \
what a change would affect), `out` (what the symbol calls, imports or uses) or \
`both` (the default of a file seed); `edges` narrows which kinds the walk \
follows (`calls`, `imports`, `inherits`, `uses`). The edges are syntactic, not \
typed — a call by name, an import by its bound segment, an `impl Trait for \
Type`, a type reference — so a common name may be a false positive, and \
`unresolved` counts the targets the book could not pin to one declaration \
(resolution is by name, only while the name has at most five definers). The \
walk has three guards and the answer states them: `depth` is clamped to 3 \
hops, a cycle is answered once per symbol and never followed twice, and the \
answer is bounded by `budget` nodes (600 by default, 2000 at most, with the \
edges at twice the nodes). A cut answers `truncated: {reason, shown, total, \
next}`: `nodes` or `edges` when the budget bit (then `next` is the same call \
with twice the budget), `depth` when the request asked for more hops than 3 \
and the walk still had somewhere to go (`next` is `null`: the cap is the \
answer). Nodes carry `file`, `line` and the hop they were reached at; the seed \
travels with `content_hash`, `indexed_at` and the `book_key` of the book. An \
ambiguous name answers `ok: false` with `ambiguous_symbol` and `candidates[]`; \
the book is on by default, so `book_disabled` only comes from an explicit \
`GRAPHIUM_BOOK=off`. Read-only: it never touches the plan."
        }
        "code_diagram" => {
            "Build the diagram of one symbol's neighbourhood from the book, without \
reading any file: the answer is a complete `graph` document (`version: 1`, \
`kind: \"graph\"`, one node per symbol labelled with its `qname`, one edge per \
connection labelled with its kind) in `diagram`. Send it to `render_diagram` \
as it is, or to `validate_diagram` first: it respects the contract of the kind \
(300 nodes, 600 edges, density 3), so it needs no repair. The walk is \
`code_impact` with `direction: both` and `depth` clamped to 3 hops; `edges` \
narrows which kinds it follows. `truncated` is `{reason, shown, total, next}` \
and tells the two cuts apart: `density` means the document was trimmed to \
render (the walk itself was complete) and `nodes`/`edges` mean the walk was cut \
first, because `budget` keeps the 300/600 ceilings of the kind; in both cases \
`next` is the `code_impact` call that answers the same question with the query \
budget of the connections view. The connections are syntactic, not typed: the \
picture is the book's reading of the code. The start symbol travels with \
`content_hash`, `indexed_at` and the `book_key` of the book; an ambiguous name \
answers `ok: false` with `ambiguous_symbol` and `candidates[]`, and the book is \
on by default (`book_disabled` only with an explicit `GRAPHIUM_BOOK=off`). It \
never renders and never touches the plan."
        }
        "clear_canvas" => {
            "\
Remove diagrams from the plan. Without `scope` it clears only the diagrams of this \
session; with `scope: \"plano\"` it clears the whole plan, including the ones of \
other sessions. It does not close the window and does not stop the MCP session."
        }
        _ => "A Graphium tool.",
    }
}

/// The JSON configuration of a client, ready to paste (PRD §7, RF19).
///
/// Every client speaks the same stdio transport; what changes is the shape of
/// the file it expects.
pub fn client_config(client: &str, command: &str) -> String {
    match client {
        "pi" => format!(
            "[mcp_servers.graphium]\ncommand = {command:?}\nargs = [\"--stdio\"]\n"
        ),
        "claude" => format!(
            "{{\n  \"mcpServers\": {{\n    \"graphium\": {{\n      \"command\": {command:?},\n      \"args\": [\"--stdio\"]\n    }}\n  }}\n}}"
        ),
        "cursor" => format!(
            "{{\n  \"mcpServers\": {{\n    \"graphium\": {{\n      \"command\": {command:?},\n      \"args\": [\"--stdio\"]\n    }}\n  }}\n}}"
        ),
        _ => format!("{{\"command\":{command:?},\"args\":[\"--stdio\"]}}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{client_config, description, INSTRUCTIONS};

    #[test]
    fn the_instructions_say_when_to_use_it_and_when_not() {
        assert!(INSTRUCTIONS.contains("WHEN TO USE IT"));
        assert!(INSTRUCTIONS.contains("WHEN NOT TO USE IT"));
        assert!(INSTRUCTIONS.len() > 1200, "the agent needs real guidance");
    }

    #[test]
    fn the_instructions_name_every_tool() {
        for name in gx_core::kind::kind_names()
            .iter()
            .map(|_| "")
            .chain(std::iter::empty())
        {
            let _ = name;
        }
        for tool in crate::dispatch::TOOL_NAMES {
            assert!(
                INSTRUCTIONS.contains(tool),
                "the instructions have to mention `{tool}`"
            );
        }
    }

    #[test]
    fn the_instructions_name_the_codes_the_agent_has_to_branch_on() {
        for code in [
            "unknown_ref",
            "duplicate_id",
            "unknown_field",
            "unknown_kind",
            "too_large",
            "stale_revision",
            "layout_timeout",
            "busy",
        ] {
            assert!(INSTRUCTIONS.contains(code), "falta `{code}`");
        }
    }

    #[test]
    fn every_tool_has_a_description_of_its_own() {
        for tool in crate::dispatch::TOOL_NAMES {
            let text = description(tool);
            assert!(text.len() > 120, "{tool} needs a real description");
            assert_ne!(text, description("nope"), "{tool} is a fallback");
        }
    }

    /// S5/L5: the book is on by default, so no description tells the human to
    /// turn it on: `GRAPHIUM_BOOK=off` is the explicit opt-out.
    #[test]
    fn the_book_descriptions_say_the_book_is_on_by_default() {
        for tool in [
            "code_search",
            "code_structure",
            "code_impact",
            "code_diagram",
        ] {
            let text = description(tool);
            assert!(
                text.contains("on by default"),
                "{tool} has to say the book is on by default"
            );
            assert!(
                text.contains("GRAPHIUM_BOOK=off"),
                "{tool} has to name the opt-out"
            );
            assert!(
                !text.contains("GRAPHIUM_BOOK=on"),
                "{tool} must not ask to enable the book"
            );
        }
        assert!(INSTRUCTIONS.contains("GRAPHIUM_BOOK=off"));
        assert!(!INSTRUCTIONS.contains("GRAPHIUM_BOOK=on"));
    }

    /// S1-S5: the agent learns how an export travels and what it costs,
    /// without reading any other document.
    #[test]
    fn the_harness_teaches_the_two_ways_of_exporting() {
        for expected in [
            "EXPORTING",
            "inline",
            "opts.path",
            "png` requires `path",
            "export_path_outside_root",
            "overwrite",
            "too_large",
            "never changes the plan",
        ] {
            assert!(
                INSTRUCTIONS.contains(expected),
                "the harness has to say `{expected}`"
            );
        }
    }

    /// S21: the agent learns that `opts.extract`, `opts.layout_timeout_ms` and
    /// `extracted_from` exist without reading any other document.
    #[test]
    fn the_harness_teaches_opts_extract_and_the_layout_deadline() {
        let render = description("render_diagram");
        for needle in [
            "opts.extract",
            "opts.layout_timeout_ms",
            "extracted_from",
            "layout_timeout",
        ] {
            assert!(render.contains(needle), "render_diagram lacks `{needle}`");
            assert!(
                INSTRUCTIONS.contains(needle),
                "the instructions lack `{needle}`"
            );
        }
        for future in ["opts.replace_id", "opts.interactive"] {
            assert!(render.contains(future), "render_diagram lacks `{future}`");
            assert!(
                INSTRUCTIONS.contains(future),
                "the instructions lack `{future}`"
            );
        }
    }

    /// S3/D2: the agent learns how to name its session, and that the name is
    /// what the human sees in the window.
    #[test]
    fn the_harness_teaches_how_to_name_the_session() {
        let register = description("register_session");
        for needle in ["label", "session", "invalid_value"] {
            assert!(
                register.contains(needle),
                "register_session lacks `{needle}`"
            );
        }

        assert!(
            INSTRUCTIONS.contains("register_session"),
            "the flow has to say when the session is named"
        );
        assert!(
            INSTRUCTIONS.contains("label"),
            "the flow has to say that the name is optional"
        );
    }

    #[test]
    fn the_pi_config_is_toml_and_the_rest_is_json() {
        let pi = client_config("pi", "/usr/local/bin/graphium");
        assert!(pi.contains("[mcp_servers.graphium]"));
        assert!(pi.contains("args = [\"--stdio\"]"));

        for client in ["claude", "cursor", "generic"] {
            let config = client_config(client, "/usr/local/bin/graphium");
            let value: serde_json::Value = serde_json::from_str(&config).expect("json");
            assert!(config.contains("--stdio"), "{client}");
            if client == "generic" {
                assert_eq!(value["command"], "/usr/local/bin/graphium");
            } else {
                assert_eq!(
                    value["mcpServers"]["graphium"]["command"],
                    "/usr/local/bin/graphium"
                );
            }
        }
    }
}
