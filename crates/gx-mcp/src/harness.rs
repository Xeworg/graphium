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
(tables, columns, keys, cardinality), which `code_er` draws from a `.sql` file, or \
an architecture of services. The human \
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

THE CHEAP PATH. If the subject is in this project's index, do not write JSON by \
hand: `code_search` locates a symbol, `code_diagram` draws its neighbourhood as a \
ready `graph` document, and a `.sql` file becomes the `er` document of `code_er`; \
the two documents go to `validate_diagram` and `render_diagram` unchanged. \
Hand-write a document only when the subject is not in the index.

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
max 300 nodes). `opts.replace_id` is honoured today: it replaces the document of \
the named diagram, and an unknown id answers `unknown_diagram` and creates \
nothing. `opts.interactive` still arrives in F3: sending it today costs a warning \
(`opts_ignored`), never the diagram.

ERRORS. Every failure carries `{ path, code, message, hint? }` with a stable code \
and the JSON path of the problem. `unknown_ref` and `duplicate_id` are rules of a \
kind, `unknown_field` means a key the schema does not admit (`hint` carries the \
closest name), `unknown_kind` lists the kinds that exist, `too_large` means the \
document is over the limit of its kind, `stale_revision` means you read an old \
version, `layout_timeout` means the layout missed `opts.layout_timeout_ms`, and \
`busy` means the queue of renders is full: wait `retry_after_ms` and try again. In \
the book, `invalid_value` means an argument the tool cannot use, `ambiguous_symbol` \
means the name fits more than one declaration and `candidates[]` lists them, \
`book_disabled` means the owner opted out with `GRAPHIUM_BOOK=off`, and \
`book_unavailable` means the book of the project could not be opened. Fix \
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

THE BOOK. The book answers the project the session's client declared \
(`GRAPHIUM_PROJECT`, or the working directory when it declares none): a session \
without a project gets `ok: false` with `invalid_value` at `session.project`. \
`code_search` answers from the index of the project instead of \
reading files: it returns the `file`, `line`, `kind`, `name`, `qname`, \
`content_hash` and `indexed_at` of one symbol, plus the `book_key` of the book \
that answered. Use it to locate the code behind a diagram, or to find a \
function by name before drawing its flow. Every word of `query` has to match \
and the tool never guesses: a query that fits more than one declaration \
answers `ok: false` with `code_search`'s `ambiguous_symbol` code and \
`candidates[]`, so send the words of one candidate's `qname` to narrow it. Its \
candidate list stops at five and says so: `truncated: {reason: \"nodes\", \
shown, total, next}`, where `total` is how many declaration places the query \
fits — so `shown < total` means the query is still too broad — and `next` is \
`null`, because there is no page cursor: narrowing the query is the way to the \
rest, never a guess at the hidden candidates. `code_structure` reads a \
chapter (`file`) or the chapter that declares one \
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
connected file (the seed file is `file`, never a row of `files`). A symbol \
seed answers the same `files[]`, derived from the walk it already carries: \
`imports_out` towards the file, `referenced_by_in` from it, `symbols` the \
nodes of the answer that file declares, and the chapter of the seed never a \
row — so «what does changing this symbol affect» arrives grouped by file \
instead of as a flat `nodes` list. `edges` narrows the kinds it follows (`calls`, `imports`, `inherits`, \
`uses`) and `budget` sets the query budget in nodes (600 by default, 2000 at \
most, the edges at twice the nodes — its own number, decoupled from the 300/600 \
a diagram document can render). The walk stops at 3 hops, answers a cycle once, \
and states every cut in `truncated: {reason, shown, total, next}`, with `next` \
as the call that continues it. `unresolved` counts the names the book could not \
pin down (resolution is by name, only while a name has at most five definers). \
Those connections are syntactic, not typed: they are read from the code and may \
be false positives. Only `code_structure`, `code_impact` and `code_diagram` take \
`max_chars` and report `chars` (what it spends) and `max_chars` (the envelope it \
had — the ladder of the book by indexed files, 13,000 up to 150, 18,000 up to \
500, 24,000 beyond, overridable with the `max_chars` argument up to 1.5 times); \
past the envelope the head keeps the full rows, the tail cliffs to one-line \
`pointers`, and the cut says `reason: \"budget\"` with the exact wider call in \
`next`. `code_search` and `code_er` have no envelope: `code_search` answers one \
row or `candidates[]`, `code_er` answers the whole document, and a `max_chars` \
sent to either is refused as an unknown field (`deny_unknown_fields`). \
`code_er`'s `chars` counts **characters**, the unit `chars` has in every \
answer of this server, so a `.sql` file with `é` in a table name is not \
counted twice. \
`code_diagram` turns the same neighbourhood into a `graph` \
document in `diagram`, ready to send to `render_diagram` with no repair: that \
is the cheap diagram, drawn from the book instead of from the code; it takes \
the same `edges` filter and keeps the 300/600 ceilings, so `truncated.reason` \
says `density` when the document was trimmed and `nodes`/`edges` when the walk \
was cut. `code_er` hands you the `er` document of one `.sql` file of the \
project (`path`, relative to the project root with `/` separators) — or, with \
`scope: \"project\"`, of every `.sql` chapter of the book merged into one \
document, with `files` counting the chapters it parsed: its tables \
as compartments with their columns and the keys between them, in `diagram`, \
ready for `validate_diagram` and then `render_diagram` — the picture of a \
database read live from the file instead of from the code. \
`code_index` answers the census of the book itself, with no subject to look up: \
the state of the first pass, `files { total, done, failed_total, by_reason, \
failed[] }`, `symbols`, `languages[]` and the `limits` in force. Ask it when the \
human wants to know what the index covers before trusting a search: a symbol that \
is not there may live in a file the book never took. Every `failed[]` row carries \
the stable code of the reason plus a hint a human reads: `ignored` (it is one of \
the directories the book never walks — `.git`, `.graphium`, `target`, \
`node_modules` — and a pruned subtree is one row; a path `.gitignore` hides is \
pruned by the walker before the book sees it and is not named in the census), \
`too_large` (over \
the 1 MiB ceiling), `unsupported_language` (no grammar of this build answers the \
extension) and `unreadable` (it does not read as source: a read error, a binary, \
or a grammar that refused it). Branch on `reason`, never on the sentence. The list \
is a sample of at most fifty rows per reason — `failed_total` and `by_reason` \
count every skip — so a file the book does not have is never a silent absence. \
Each row of `languages` reports `files`, `symbols` and `resolution { edges, \
resolved, percent, comparable }`: `percent` is `resolved / edges`, never a share \
of files or of symbols, and `comparable` is true only for a language that carries \
a runtime-name list, so read two rates against each other only when both are \
`true` — a `false` row counts its runtime targets as unresolved. `limits` names \
the ceilings in force (`max_file_bytes`, `max_name_fanout`, `max_parse_threads`, \
`max_batch_bytes`, `hard_exclusions`). It takes no required argument and `lang` \
only narrows `languages`. The book is \
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
    // One text per tool (`f23-contrato.md`, S4): the constants of `lib.rs` are
    // the ones the tool declarations register, so this table points at them
    // instead of copying them — a copy is what let the two drift apart.
    match name {
        "render_diagram" => crate::RENDER_DIAGRAM_DESCRIPTION,
        "export_diagram" => crate::EXPORT_DIAGRAM_DESCRIPTION,
        "validate_diagram" => crate::VALIDATE_DIAGRAM_DESCRIPTION,
        "get_diagram_schema" => crate::GET_DIAGRAM_SCHEMA_DESCRIPTION,
        "list_diagrams" => crate::LIST_DIAGRAMS_DESCRIPTION,
        "get_diagram" => crate::GET_DIAGRAM_DESCRIPTION,
        "list_diagram_versions" => crate::LIST_DIAGRAM_VERSIONS_DESCRIPTION,
        "get_diagram_version" => crate::GET_DIAGRAM_VERSION_DESCRIPTION,
        "clear_canvas" => crate::CLEAR_CANVAS_DESCRIPTION,
        "update_diagram" => crate::UPDATE_DIAGRAM_DESCRIPTION,
        "register_session" => crate::REGISTER_SESSION_DESCRIPTION,
        "code_search" => crate::CODE_SEARCH_DESCRIPTION,
        "code_structure" => crate::CODE_STRUCTURE_DESCRIPTION,
        "code_impact" => crate::CODE_IMPACT_DESCRIPTION,
        "code_diagram" => crate::CODE_DIAGRAM_DESCRIPTION,
        "code_er" => crate::CODE_ER_DESCRIPTION,
        "code_index" => crate::CODE_INDEX_DESCRIPTION,
        _ => "",
    }
}

/// The JSON configuration of a client, ready to paste (PRD §7, RF19).
///
/// Every client speaks the same stdio transport; what changes is the shape of
/// the file it expects.
pub fn client_config(client: &str, command: &str) -> String {
    match client {
        // S6: Pi reads a JSON `mcp.json` (`~/.pi/agent/mcp.json`), with the
        // same `mcpServers` object Claude and Cursor read; the TOML of
        // `[mcp_servers.graphium]` is the shape of another client entirely.
        "pi" | "claude" | "cursor" => format!(
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
            "book_disabled",
            "book_unavailable",
            "ambiguous_symbol",
            "invalid_value",
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
            "code_er",
            "code_index",
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
    ///
    /// S1: and the sentence about `opts.replace_id`/`opts.interactive` says the
    /// truth: `replace_id` is honoured today and only `interactive` still waits.
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
        // S1: the stale F2/F3 sentence is gone; `replace_id` is honoured now
        // and only `interactive` still costs the warning.
        assert!(
            INSTRUCTIONS.contains("`opts.replace_id` is honoured today"),
            "the instructions have to say `replace_id` is honoured"
        );
        assert!(
            INSTRUCTIONS.contains("an unknown id answers `unknown_diagram` and creates nothing"),
            "the instructions have to say what an unknown `replace_id` does"
        );
        assert!(
            INSTRUCTIONS.contains("`opts.interactive` still arrives in F3"),
            "the instructions have to say `interactive` is still not honoured"
        );
        assert!(
            INSTRUCTIONS.contains("costs a warning"),
            "the instructions have to say what `interactive` costs"
        );
        assert!(
            !INSTRUCTIONS.contains("arrive in F2/F3"),
            "the stale sentence is gone"
        );
    }

    /// S2: the character envelope is a fact of the three tools that take
    /// `max_chars`, and the other two answer without one — the instructions
    /// must not promise it for all five.
    #[test]
    fn the_instructions_bound_only_the_three_tools_with_an_envelope() {
        for tool in ["code_structure", "code_impact", "code_diagram"] {
            assert!(
                INSTRUCTIONS.contains(tool),
                "the instructions have to name the bounded tool `{tool}`"
            );
        }
        assert!(
            INSTRUCTIONS.contains("Only `code_structure`, `code_impact` and `code_diagram` take"),
            "the bound is stated for its three tools"
        );
        assert!(
            INSTRUCTIONS.contains("13,000 up to 150, 18,000 up to 500, 24,000 beyond"),
            "the ladder numbers stay"
        );
        assert!(
            INSTRUCTIONS.contains("up to 1.5 times"),
            "the 1.5x ceiling stays"
        );
        assert!(
            INSTRUCTIONS.contains("`code_search` and `code_er` have no envelope"),
            "the two tools without an envelope are named"
        );
        assert!(
            INSTRUCTIONS.contains("`deny_unknown_fields`"),
            "`max_chars` on those two is refused as an unknown field"
        );
        assert!(
            !INSTRUCTIONS.contains("Every answer of the book is bounded"),
            "the false universal is gone"
        );
    }

    /// S3: the cheap path comes before BIG DIAGRAMS: a subject in the index is
    /// drawn from the book, and JSON is hand-written only when it is not there.
    #[test]
    fn the_instructions_teach_the_cheap_path_of_the_book() {
        let cheap = INSTRUCTIONS
            .find("THE CHEAP PATH.")
            .expect("the instructions name the cheap path");
        let big = INSTRUCTIONS
            .find("BIG DIAGRAMS.")
            .expect("BIG DIAGRAMS stays");
        let manual = INSTRUCTIONS.find("HOW, IN ORDER.").expect("HOW stays");
        assert!(
            manual < cheap && cheap < big,
            "the cheap path comes after HOW and before BIG DIAGRAMS"
        );
        for needle in [
            "`code_search` locates a symbol",
            "`code_diagram` draws its neighbourhood as a ready `graph` document",
            "`.sql` file becomes the `er` document of `code_er`",
            "go to `validate_diagram` and `render_diagram` unchanged",
            "Hand-write a document only when the subject is not in the index",
        ] {
            assert!(
                INSTRUCTIONS.contains(needle),
                "the cheap path lacks `{needle}`"
            );
        }
        assert!(
            INSTRUCTIONS.contains("which `code_er` draws from a `.sql` file"),
            "the WHEN TO USE IT database bullet names `code_er`"
        );
    }

    /// S4: the book states its precondition (the declared project) and the
    /// instructions name the codes of its refusals.
    #[test]
    fn the_instructions_state_the_precondition_of_the_book() {
        for needle in [
            "`GRAPHIUM_PROJECT`, or the working directory when it declares none",
            "without a project gets `ok: false` with `invalid_value` at `session.project`",
        ] {
            assert!(
                INSTRUCTIONS.contains(needle),
                "the book precondition lacks `{needle}`"
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

    /// S6: Pi reads a JSON `mcp.json`, so the config printed for it is the
    /// `mcpServers` object its README documents; claude, cursor and generic
    /// keep exactly the shapes they had.
    #[test]
    fn the_pi_config_is_json_and_the_rest_keeps_its_shape() {
        let pi = client_config("pi", "/usr/local/bin/graphium");
        let value: serde_json::Value = serde_json::from_str(&pi).expect("pi reads JSON");
        assert_eq!(
            value["mcpServers"]["graphium"]["command"],
            "/usr/local/bin/graphium"
        );
        assert_eq!(
            value["mcpServers"]["graphium"]["args"],
            serde_json::json!(["--stdio"])
        );
        assert!(
            !pi.contains("[mcp_servers.graphium]"),
            "pi is not TOML: {pi}"
        );

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

    /// f13 S5: `code_er` is a named tool of the door with a description of
    /// its own, and it follows the book family's contract: the book is on by
    /// default and the description names the explicit opt-out.
    #[test]
    fn the_code_er_description_follows_the_book_family_contract() {
        assert!(
            crate::dispatch::TOOL_NAMES.contains(&"code_er"),
            "the tool is named in TOOL_NAMES: {:?}",
            crate::dispatch::TOOL_NAMES
        );
        let text = description("code_er");
        assert!(text.len() > 120, "code_er needs a real description");
        assert_ne!(text, description("nope"), "code_er is a fallback");
        assert!(text.contains("on by default"), "{text}");
        assert!(text.contains("GRAPHIUM_BOOK=off"), "{text}");
        assert!(!text.contains("GRAPHIUM_BOOK=on"), "{text}");
        // S5: the refusal shape is named, and a leading `./` is refused like
        // the absolute path and the `..` beside it (the guard reads components).
        for needle in [
            "`ok: false`",
            "code `invalid_value` at `path`",
            "a leading `./`",
            "the guard reads path components",
        ] {
            assert!(text.contains(needle), "code_er lacks `{needle}`: {text}");
        }
    }

    /// f15 S6: the two copies of the `code_er` description — the const the
    /// stdio router publishes and `description()`, which the harness and the
    /// relay read — are one text, byte for byte, and both name the new scope
    /// and the count a merged document answers with.
    #[test]
    fn the_code_er_description_names_the_scope_in_both_copies() {
        let text = description("code_er");
        assert_eq!(
            text,
            crate::CODE_ER_DESCRIPTION,
            "the duplicated copies of the `code_er` description drifted apart"
        );
        for needle in ["`scope`", "`\"project\"`", "`\"file\"`", "`files`"] {
            assert!(text.contains(needle), "code_er lacks `{needle}`: {text}");
        }
    }

    /// f13 S6: the instructions teach the tool's place in the book section,
    /// next to the tools it shares the project with.
    #[test]
    fn the_instructions_present_code_er_next_to_the_book() {
        assert!(
            INSTRUCTIONS.contains("code_er"),
            "THE BOOK section has to name the tool"
        );
    }

    /// f28 S4/D1/D2/D3: the agent learns that the census exists, that a rate
    /// says whether it may be compared, and which codes a file that did not
    /// reach the book carries.
    #[test]
    fn the_harness_teaches_the_census_of_the_book() {
        assert!(
            crate::dispatch::TOOL_NAMES.contains(&"code_index"),
            "the census is a tool: {:?}",
            crate::dispatch::TOOL_NAMES
        );
        let text = description("code_index");
        assert!(text.len() > 120, "code_index needs a real description");
        assert_ne!(text, description("nope"), "code_index is a fallback");
        for needle in [
            "comparable",
            "`lang`",
            "ignored",
            "too_large",
            "unsupported_language",
            "unreadable",
            "on by default",
            "GRAPHIUM_BOOK=off",
        ] {
            assert!(text.contains(needle), "code_index lacks `{needle}`: {text}");
            assert!(
                INSTRUCTIONS.contains(needle),
                "the instructions lack `{needle}`"
            );
        }
        assert!(
            INSTRUCTIONS.contains("`code_index`"),
            "THE BOOK section has to name the census tool"
        );
    }

    /// D6 (f28): both seeds of `code_impact` answer the `files[]` grouping, and
    /// the agent learns it without reading any other document — including what
    /// the row of a symbol seed counts.
    #[test]
    fn the_harness_teaches_the_files_grouping_of_a_symbol_seed() {
        let impact = description("code_impact");
        for needle in [
            "`files[]`",
            "`imports_out`",
            "`referenced_by_in`",
            "never a row",
        ] {
            assert!(impact.contains(needle), "code_impact lacks `{needle}`");
            assert!(
                INSTRUCTIONS.contains(needle),
                "the instructions lack `{needle}`"
            );
        }
        assert!(
            INSTRUCTIONS.contains("derived from the walk it already carries"),
            "the instructions say where a symbol seed's grouping comes from"
        );
    }

    /// D6 (f28): `chars` is characters everywhere in this server, and the
    /// `code_er` answer says so before the agent compares it with a byte count.
    #[test]
    fn the_harness_says_chars_counts_characters() {
        let er = description("code_er");
        for text in [er, INSTRUCTIONS] {
            assert!(
                text.contains("**characters**"),
                "`chars` is taught in characters: {text}"
            );
            assert!(
                !text.contains("the byte length"),
                "the byte-length sentence is gone: {text}"
            );
        }
    }

    /// D6 (f28): the candidate cut of `code_search` is visible, and the agent
    /// learns what `shown` and `total` count — plus that there is no page.
    #[test]
    fn the_harness_teaches_the_visible_candidate_cut() {
        let search = description("code_search");
        for text in [search, INSTRUCTIONS] {
            for needle in [
                "truncated",
                "shown",
                "total",
                "\"nodes\"",
                "declaration places",
                "no page cursor",
            ] {
                assert!(
                    text.contains(needle),
                    "the candidate cut lacks `{needle}`: {text}"
                );
            }
        }
        // The reason is the shared vocabulary of the other cuts.
        assert!(
            search.contains("`reason: \"nodes\"`"),
            "code_search names the reason of the cut: {search}"
        );
        assert!(
            !search.contains("`next` is a page"),
            "there is no page cursor to promise: {search}"
        );
    }
}
