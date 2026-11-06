//! `gx-mcp` — the MCP server of Graphium (PRD §8.6.1, §11).
//!
//! The crate owns the stdio MCP server, the `render_diagram` tool and the
//! in-memory [`store::DiagramStore`]. It reaches a kind only through the
//! registry (`gx_core::kind::for_kind`, PRD §9.1) and renders only through the
//! single engine door ([`gx_core::engine::render_json`]): there is no second
//! pipeline here and no test needs Tauri, webkit or a webview.
//!
//! **stdout is JSON-RPC only.** Every diagnostic goes to stderr; the transport
//! takes stdout in exclusive mode and nothing in this crate writes to it
//! (PRD §8.6.1, RNF7).

pub mod contract;
pub mod dispatch;
pub mod export;
pub mod harness;
pub mod session;
pub mod store;
pub mod tools;

/// The door to the book of the project (`gx-index`) for the owner (F9, C0).
///
/// The engine lives in its own crate and `gx-app` never depends on it: the
/// owner reaches it through this module, which is the only place that knows
/// both the session record (to resolve the project, never `current_dir()`,
/// D4) and the shape an agent reads.
pub mod book {
    use serde_json::{json, Value};

    use crate::contract::ErrorPayload;

    /// The book is only off when the owner opted out: `GRAPHIUM_BOOK=off`
    /// (S5, L5 — the book is on by default).
    pub const DISABLED_CODE: &str = "book_disabled";
    /// The book could not be opened: the answer is an error, never a silent
    /// empty result that the agent would read as «the symbol does not exist».
    pub const UNAVAILABLE_CODE: &str = "book_unavailable";
    /// The query fits more than one declaration: `candidates[]` names them
    /// (P3, condition 2).
    pub const AMBIGUOUS_CODE: &str = "ambiguous_symbol";

    /// How deep a walk goes when the agent does not say.
    pub const DEFAULT_DEPTH: u32 = 1;

    /// The direction `code_impact` follows when the agent does not say: a
    /// change affects what points at the symbol.
    pub const DEFAULT_DIRECTION: &str = "in";

    /// The project of the session: the record of the session, never the cwd
    /// of the process (D4, ideas §7 explore 3).
    pub fn project_of(session_id: &str) -> Option<String> {
        crate::session::session_registry()
            .get(session_id)
            .and_then(|record| record.project)
            .filter(|project| !project.trim().is_empty())
    }

    /// The configuration of the book of the owner: `GRAPHIUM_BOOK` and the
    /// books root, read once (S5, D4). An absent flag means **on**.
    pub fn config_from_env() -> gx_index::BooksConfig {
        gx_index::BooksConfig::from_env()
    }

    /// Keeps a book running for every project of the open sessions (S9).
    ///
    /// The owner calls this from its poll loop; opening a book is idempotent,
    /// so a project that already has one costs a map lookup.
    pub fn maintain(config: &gx_index::BooksConfig, projects: &[String]) {
        gx_index::service::maintain(config, projects);
    }

    /// Stops every watcher and closes every book cleanly (the idle exit).
    pub fn shutdown_all() {
        gx_index::service::shutdown_all();
    }

    /// Runs `code_search` for the session with the configuration of the owner.
    pub fn code_search(session_id: &str, query: &str) -> Value {
        let config = gx_index::BooksConfig::from_env();
        code_search_with(&config, project_of(session_id).as_deref(), query)
    }

    /// Runs `code_search` with an explicit configuration and project.
    ///
    /// Both are parameters instead of environment reads, so the answer of the
    /// tool can be checked for a project that is not this machine's and for
    /// the disabled path without racing the process environment.
    pub fn code_search_with(
        config: &gx_index::BooksConfig,
        project: Option<&str>,
        query: &str,
    ) -> Value {
        let query = match text_argument(query, "query") {
            Ok(query) => query,
            Err(response) => return response,
        };
        let opened = match open(config, project) {
            Ok(opened) => opened,
            Err(response) => return response,
        };
        let book = opened.handle.book();
        let outcome = match book.search(&gx_index::SearchRequest {
            query: query.to_string(),
        }) {
            Ok(outcome) => outcome,
            Err(error) => return failure("book", UNAVAILABLE_CODE, &error.to_string(), None),
        };

        let mut base = base(&opened);
        base["query"] = json!(query);
        match outcome {
            gx_index::SearchOutcome::Empty => {
                base["ok"] = json!(true);
                base["results"] = json!([]);
                base["count"] = json!(0);
                base
            }
            gx_index::SearchOutcome::Hits(hits) => {
                let results: Vec<Value> = hits.iter().map(hit_json).collect();
                base["ok"] = json!(true);
                base["count"] = json!(results.len());
                base["results"] = json!(results);
                base
            }
            gx_index::SearchOutcome::Ambiguous(candidates) => {
                ambiguous_answer(base, "query", query, &candidates)
            }
        }
    }

    /// Runs `code_structure` for the session with the configuration of the owner.
    pub fn code_structure(session_id: &str, file: Option<&str>, symbol: Option<&str>) -> Value {
        let config = gx_index::BooksConfig::from_env();
        code_structure_with(&config, project_of(session_id).as_deref(), file, symbol)
    }

    /// Runs `code_structure` with an explicit configuration and project (S5).
    ///
    /// The structure of a chapter is its containment tree: the declarations of
    /// one file, ordered by span, with the `parent_id` of each one translated
    /// to the qname of the parent, so the agent can rebuild the tree without
    /// another call.
    pub fn code_structure_with(
        config: &gx_index::BooksConfig,
        project: Option<&str>,
        file: Option<&str>,
        symbol: Option<&str>,
    ) -> Value {
        let file = file.map(str::trim).filter(|text| !text.is_empty());
        let symbol = symbol.map(str::trim).filter(|text| !text.is_empty());
        match (file, symbol) {
            (Some(_), Some(_)) => {
                return failure(
                    "file",
                    "invalid_value",
                    "send one of `file` or `symbol`, not both",
                    Some("`file` reads a whole chapter; `symbol` reads the chapter that declares one symbol"),
                );
            }
            (None, None) => {
                return failure(
                    "file",
                    "invalid_value",
                    "`file` or `symbol` is required",
                    Some("send `file` (a path relative to the project root, with `/`) or `symbol` (a name, as `code_search` takes it)"),
                );
            }
            _ => {}
        }
        let symbol = match symbol {
            Some(symbol) => match text_argument(symbol, "symbol") {
                Ok(symbol) => Some(symbol),
                Err(response) => return response,
            },
            None => None,
        };
        let opened = match open(config, project) {
            Ok(opened) => opened,
            Err(response) => return response,
        };
        let book = opened.handle.book();
        let base = base(&opened);

        let (structure, asked) = match symbol {
            Some(symbol) => {
                let hit = match one_symbol(book, base.clone(), symbol, "symbol") {
                    Ok(hit) => hit,
                    Err(answer) => return answer,
                };
                let structure = book.structure_of_symbol(hit.id);
                (structure, Some(hit))
            }
            None => {
                let file = file.expect("a selector was required above");
                (book.structure_at_path(file), None)
            }
        };
        let structure = match structure {
            Ok(Some(structure)) => structure,
            Ok(None) => {
                return failure_with(
                    base,
                    if file.is_some() { "file" } else { "symbol" },
                    "invalid_value",
                    "the book has no chapter there",
                    Some("paths are relative to the project root with `/` separators; `progress.state == \"indexing\"` means the first pass is still running"),
                );
            }
            Err(error) => {
                return failure_with(base, "book", UNAVAILABLE_CODE, &error.to_string(), None);
            }
        };

        let mut response = base;
        response["ok"] = json!(true);
        response["file"] = json!(structure.file);
        response["lang"] = json!(structure.lang);
        response["content_hash"] = json!(structure.content_hash);
        response["indexed_at"] = json!(gx_index::book::iso8601_utc(structure.indexed_at));
        response["count"] = json!(structure.symbols.len());
        response["symbols"] = json!(structure_symbols_json(&structure.symbols));
        if let Some(hit) = asked {
            response["symbol"] = hit_json(&hit);
        }
        response
    }

    /// Runs `code_impact` for the session with the configuration of the owner.
    pub fn code_impact(
        session_id: &str,
        symbol: &str,
        depth: Option<u32>,
        direction: Option<&str>,
    ) -> Value {
        let config = gx_index::BooksConfig::from_env();
        code_impact_with(
            &config,
            project_of(session_id).as_deref(),
            symbol,
            depth,
            direction,
        )
    }

    /// Runs `code_impact` with an explicit configuration and project (S7, D3).
    ///
    /// The walk is bounded on purpose: `depth` is clamped to the 3 hops of the
    /// design and the answer is capped at the ceilings of the `graph` kind
    /// (300 nodes / 600 edges), so it always fits in a document the agent can
    /// render. A cycle is answered once per symbol, never followed twice.
    pub fn code_impact_with(
        config: &gx_index::BooksConfig,
        project: Option<&str>,
        symbol: &str,
        depth: Option<u32>,
        direction: Option<&str>,
    ) -> Value {
        let symbol = match text_argument(symbol, "symbol") {
            Ok(symbol) => symbol,
            Err(response) => return response,
        };
        let direction = match direction_argument(direction) {
            Ok(direction) => direction,
            Err(response) => return response,
        };
        let opened = match open(config, project) {
            Ok(opened) => opened,
            Err(response) => return response,
        };
        let book = opened.handle.book();
        let base = base(&opened);
        let hit = match one_symbol(book, base.clone(), symbol, "symbol") {
            Ok(hit) => hit,
            Err(answer) => return answer,
        };
        let request = gx_index::ImpactRequest {
            start: hit.id,
            depth: depth.unwrap_or(DEFAULT_DEPTH),
            direction,
        };
        let impact = match book.impact(&request) {
            Ok(Some(impact)) => impact,
            Ok(None) => {
                return failure_with(
                    base,
                    "symbol",
                    "invalid_value",
                    "the symbol is no longer in the book",
                    Some("the indexer replaced the chapter since the search: call the tool again"),
                );
            }
            Err(error) => {
                return failure_with(base, "book", UNAVAILABLE_CODE, &error.to_string(), None);
            }
        };

        let mut response = base;
        response["ok"] = json!(true);
        response["symbol"] = hit_json(&hit);
        response["direction"] = json!(direction.as_str());
        response["depth"] = json!(impact.depth);
        response["budget"] = json!({
            "max_nodes": gx_index::impact::MAX_NODES,
            "max_edges": gx_index::impact::MAX_EDGES,
        });
        response["truncated"] = json!(impact.truncated);
        response["unresolved"] = json!(impact.unresolved);
        response["count"] = json!(impact.nodes.len());
        response["nodes"] = json!(nodes_json(&impact.nodes));
        response["edges"] = json!(edges_json(&impact.edges));
        response
    }

    /// Runs `code_diagram` for the session with the configuration of the owner.
    pub fn code_diagram(session_id: &str, symbol: &str, depth: Option<u32>) -> Value {
        let config = gx_index::BooksConfig::from_env();
        code_diagram_with(&config, project_of(session_id).as_deref(), symbol, depth)
    }

    /// Builds the `graph` document of one symbol's neighbourhood (S5).
    ///
    /// The answer is a **document**, not a paint: the agent sends `diagram` to
    /// `render_diagram` (or to `validate_diagram` first) unchanged. The
    /// document never leaves with a density the kind would reject, so it is
    /// renderable as it arrives.
    pub fn code_diagram_with(
        config: &gx_index::BooksConfig,
        project: Option<&str>,
        symbol: &str,
        depth: Option<u32>,
    ) -> Value {
        let symbol = match text_argument(symbol, "symbol") {
            Ok(symbol) => symbol,
            Err(response) => return response,
        };
        let opened = match open(config, project) {
            Ok(opened) => opened,
            Err(response) => return response,
        };
        let book = opened.handle.book();
        let base = base(&opened);
        let hit = match one_symbol(book, base.clone(), symbol, "symbol") {
            Ok(hit) => hit,
            Err(answer) => return answer,
        };
        let request = gx_index::ImpactRequest {
            start: hit.id,
            depth: depth.unwrap_or(DEFAULT_DEPTH),
            direction: gx_index::Direction::Both,
        };
        let impact = match book.impact(&request) {
            Ok(Some(impact)) => impact,
            Ok(None) => {
                return failure_with(
                    base,
                    "symbol",
                    "invalid_value",
                    "the symbol is no longer in the book",
                    Some("the indexer replaced the chapter since the search: call the tool again"),
                );
            }
            Err(error) => {
                return failure_with(base, "book", UNAVAILABLE_CODE, &error.to_string(), None);
            }
        };

        let nodes: Vec<Value> = impact
            .nodes
            .iter()
            .map(|node| json!({ "id": node_id(node.id), "label": node.qname }))
            .collect();
        // The `graph` kind rejects a density over 3.0 at render time: the
        // document is trimmed here so it never needs a repair before drawing.
        let ceiling = impact
            .nodes
            .len()
            .saturating_mul(MAX_DENSITY)
            .min(gx_index::impact::MAX_EDGES);
        let edges: Vec<Value> = impact
            .edges
            .iter()
            .take(ceiling)
            .map(|edge| {
                json!({
                    "from": node_id(edge.from),
                    "to": node_id(edge.to),
                    "label": edge.kind,
                })
            })
            .collect();
        let dropped = impact.edges.len().saturating_sub(edges.len());
        let document = json!({
            "version": 1,
            "kind": "graph",
            "title": format!("{} — relations", hit.qname),
            "nodes": nodes,
            "edges": edges,
        });

        let mut response = base;
        response["ok"] = json!(true);
        response["symbol"] = hit_json(&hit);
        response["direction"] = json!(gx_index::Direction::Both.as_str());
        response["depth"] = json!(impact.depth);
        response["truncated"] = json!(impact.truncated || dropped > 0);
        response["nodes"] = json!(document["nodes"].as_array().map(Vec::len));
        response["edges"] = json!(document["edges"].as_array().map(Vec::len));
        response["diagram"] = document;
        response
    }

    /// The most edges a `graph` document may carry for each node: the density
    /// ceiling of the kind (PRD §7.3).
    const MAX_DENSITY: usize = 3;

    /// One open book and the project it belongs to.
    struct Opened {
        handle: std::sync::Arc<gx_index::service::BookHandle>,
        project: String,
    }

    /// The book of the session's project, or the answer that says why not.
    fn open(config: &gx_index::BooksConfig, project: Option<&str>) -> Result<Opened, Value> {
        if !config.is_enabled() {
            return Err(failure(
                "book",
                DISABLED_CODE,
                "the book of the project is off: the owner opted out",
                Some("the book is on by default; only GRAPHIUM_BOOK=off turns it off, and the human decides that"),
            ));
        }
        let Some(project) = project.filter(|project| !project.trim().is_empty()) else {
            return Err(failure(
                "session.project",
                "invalid_value",
                "the session has no project: the book cannot be resolved without one",
                Some("the client declares the project with GRAPHIUM_PROJECT or through its working directory"),
            ));
        };
        match gx_index::service::ensure(config, std::path::Path::new(project)) {
            Ok(handle) => Ok(Opened {
                handle,
                project: project.to_string(),
            }),
            Err(error) => Err(failure("book", UNAVAILABLE_CODE, &error.to_string(), None)),
        }
    }

    /// The fields every answer of the book carries (S5/S7): the book that
    /// answered, the project it is the book of, and how far the first pass is.
    fn base(opened: &Opened) -> Value {
        let book = opened.handle.book();
        let progress = book.progress().unwrap_or_default();
        json!({
            "book_key": book.key(),
            "root": book.location().root_string(),
            "project": opened.project,
            "progress": {
                "state": progress.state,
                "files_total": progress.files_total,
                "files_done": progress.files_done,
                "symbols": progress.symbols,
            },
        })
    }

    /// A text argument: trimmed, non-empty and not a sentence.
    fn text_argument<'a>(value: &'a str, field: &str) -> Result<&'a str, Value> {
        let value = value.trim();
        if value.is_empty() {
            return Err(failure(
                field,
                "invalid_value",
                &format!("`{field}` is empty"),
                Some("send a symbol name, for example `render_diagram`"),
            ));
        }
        if value.chars().count() > gx_index::search::MAX_QUERY_CHARS {
            return Err(failure(
                field,
                "invalid_value",
                &format!(
                    "`{field}` is longer than {} characters",
                    gx_index::search::MAX_QUERY_CHARS
                ),
                Some("send the name of one symbol, not a sentence"),
            ));
        }
        Ok(value)
    }

    /// The direction of a walk, from its three names.
    fn direction_argument(raw: Option<&str>) -> Result<gx_index::Direction, Value> {
        let text = raw
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .unwrap_or(DEFAULT_DIRECTION);
        gx_index::Direction::parse(text).ok_or_else(|| {
            failure(
                "direction",
                "invalid_value",
                &format!("`{text}` is not a direction"),
                Some("send `in` (the default: the edges that point at the symbol), `out` (what it reaches) or `both`"),
            )
        })
    }

    /// The one declaration a question names, or the answer that says why it is
    /// not one (P3, condition 2). `base` is consumed because every failure
    /// carries it: the agent always learns which book answered.
    fn one_symbol(
        book: &gx_index::Book,
        base: Value,
        symbol: &str,
        field: &str,
    ) -> Result<gx_index::SymbolHit, Value> {
        let outcome = match book.search(&gx_index::SearchRequest {
            query: symbol.to_string(),
        }) {
            Ok(outcome) => outcome,
            Err(error) => {
                return Err(failure_with(
                    base,
                    "book",
                    UNAVAILABLE_CODE,
                    &error.to_string(),
                    None,
                ));
            }
        };
        match outcome {
            gx_index::SearchOutcome::Hits(hits) => match hits.into_iter().next() {
                Some(hit) => Ok(hit),
                None => Err(failure_with(
                    base,
                    field,
                    "invalid_value",
                    &format!("no declaration matches `{symbol}`"),
                    Some("use `code_search` first: it lists the declarations the book has"),
                )),
            },
            gx_index::SearchOutcome::Ambiguous(candidates) => {
                Err(ambiguous_answer(base, field, symbol, &candidates))
            }
            gx_index::SearchOutcome::Empty => Err(failure_with(
                base,
                field,
                "invalid_value",
                &format!("no declaration matches `{symbol}`"),
                Some("use `code_search` first: it lists the declarations the book has"),
            )),
        }
    }

    /// One candidate as the agent reads it: the same shape `code_search` uses.
    fn candidate_json(candidate: &gx_index::SymbolCandidate) -> Value {
        json!({
            "name": candidate.name,
            "kind": candidate.kind,
            "qname": candidate.qname,
            "file": candidate.file,
            "line": candidate.line,
            "occurrences": candidate.occurrences,
        })
    }

    /// The answer of a question whose subject fits more than one declaration.
    fn ambiguous_answer(
        base: Value,
        field: &str,
        query: &str,
        candidates: &[gx_index::SymbolCandidate],
    ) -> Value {
        let candidates: Vec<Value> = candidates.iter().map(candidate_json).collect();
        let mut response = base;
        response["ok"] = json!(false);
        response["candidates"] = json!(candidates);
        response["errors"] = json!([ErrorPayload {
            path: field.to_string(),
            code: AMBIGUOUS_CODE.to_string(),
            message: format!(
                "`{query}` fits more than one declaration: no single answer would be honest"
            ),
            hint: Some(
                "send the qname words of one candidate (for example `gx_mcp tools render`) to narrow the search".to_string()
            ),
        }]);
        response
    }

    /// A failure that still carries the book it was asked about.
    fn failure_with(
        mut base: Value,
        path: &str,
        code: &str,
        message: &str,
        hint: Option<&str>,
    ) -> Value {
        base["ok"] = json!(false);
        base["errors"] = json!([ErrorPayload {
            path: path.to_string(),
            code: code.to_string(),
            message: message.to_string(),
            hint: hint.map(str::to_string),
        }]);
        base
    }

    /// The id of a symbol inside an answer: stable inside the book, and unique
    /// even when two chapters carry the same qname.
    fn node_id(id: i64) -> String {
        format!("s{id}")
    }

    /// The nodes of a walk as the agent reads them.
    fn nodes_json(nodes: &[gx_index::ImpactNode]) -> Vec<Value> {
        nodes
            .iter()
            .map(|node| {
                json!({
                    "id": node_id(node.id),
                    "name": node.name,
                    "kind": node.kind,
                    "qname": node.qname,
                    "file": node.file,
                    "line": node.line,
                    "depth": node.depth,
                })
            })
            .collect()
    }

    /// The edges of a walk as the agent reads them.
    fn edges_json(edges: &[gx_index::ImpactEdge]) -> Vec<Value> {
        edges
            .iter()
            .map(|edge| {
                json!({
                    "from": node_id(edge.from),
                    "to": node_id(edge.to),
                    "kind": edge.kind,
                })
            })
            .collect()
    }

    /// The declarations of a chapter, with the parent as the qname it belongs
    /// to: the tree is rebuildable from this list alone.
    fn structure_symbols_json(symbols: &[gx_index::StructureSymbol]) -> Vec<Value> {
        let qnames: std::collections::BTreeMap<i64, &str> = symbols
            .iter()
            .map(|symbol| (symbol.id, symbol.qname.as_str()))
            .collect();
        symbols
            .iter()
            .map(|symbol| {
                json!({
                    "name": symbol.name,
                    "kind": symbol.kind,
                    "qname": symbol.qname,
                    "line": symbol.line,
                    "column": symbol.column,
                    "end_line": symbol.end_line,
                    "end_column": symbol.end_column,
                    "parent": symbol.parent.and_then(|parent| qnames.get(&parent).copied()),
                })
            })
            .collect()
    }

    /// One hit as the agent reads it: `file:line` plus what S7 requires.
    fn hit_json(hit: &gx_index::SymbolHit) -> Value {
        json!({
            "file": hit.file,
            "line": hit.line,
            "column": hit.column,
            "kind": hit.kind,
            "name": hit.name,
            "qname": hit.qname,
            "content_hash": hit.content_hash,
            "indexed_at": gx_index::book::iso8601_utc(hit.indexed_at),
        })
    }

    /// The answer of a call that never reached the search.
    fn failure(path: &str, code: &str, message: &str, hint: Option<&str>) -> Value {
        json!({
            "ok": false,
            "errors": [ErrorPayload {
                path: path.to_string(),
                code: code.to_string(),
                message: message.to_string(),
                hint: hint.map(str::to_string),
            }],
        })
    }
}

#[cfg(test)]
mod test_support;

use std::path::Path;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Implementation, ServerCapabilities, ServerConfig};
use rmcp::{tool, tool_handler, tool_router, ErrorData, ServerHandler, ServiceExt};

/// Registers the kinds of `gx-kinds` in the global registry. Idempotent.
pub fn bootstrap() {
    static REGISTERED: std::sync::Once = std::sync::Once::new();
    REGISTERED.call_once(gx_kinds::register_all);
}

/// The JSON printed by `graphium mcp-config` (PRD §8.6.1, S15):
/// `{"command": "<absolute path of the binary>", "args": ["--stdio"]}`.
pub fn mcp_config_json(binary: &Path) -> String {
    mcp_config_for("generic", binary)
}

/// The configuration of `graphium mcp-config --client <client>` (PRD §7 RF19).
///
/// Every client speaks the same stdio transport; what changes is the shape of
/// the configuration file. `generic` is the shape of PRD §8.6.1.
pub fn mcp_config_for(client: &str, binary: &Path) -> String {
    // `gx-app` passes `current_exe()`, which is already absolute; a relative
    // path is resolved here so the printed config is always usable.
    let command = std::path::absolute(binary).unwrap_or_else(|_| binary.to_path_buf());
    let command = command.to_string_lossy().into_owned();
    crate::harness::client_config(client, &command)
}

/// Runs the MCP server on stdio until EOF (PRD §8.6.1).
///
/// The transport runs on a dedicated thread with its own `current-thread`
/// runtime, so the blocking read of stdin never sits in the caller's loop
/// (PRD §8.6.1, S14). stdout stays pure JSON-RPC: diagnostics go to stderr.
pub fn serve_stdio() -> Result<(), Box<dyn std::error::Error>> {
    bootstrap();

    let worker = std::thread::Builder::new()
        .name("gx-mcp-stdio".to_string())
        .spawn(run_stdio_service)?;
    worker.join().map_err(|_| -> Box<dyn std::error::Error> {
        "the gx-mcp stdio thread panicked".into()
    })??;
    Ok(())
}

/// Description of the `render_diagram` tool, written for the agent.
const RENDER_DIAGRAM_DESCRIPTION: &str = "\
Render a Graphium diagram and place it on the plan. Pass the whole document as \
a JSON object in `diagram`, not as a string. A `graph` document looks like: \
{ \"version\": 1, \"kind\": \"graph\", \"title\": \"parse_config() — flujo\", \
\"direction\": \"TB\", \"nodes\": [ { \"id\": \"read\", \"label\": \"read file\" } ], \
\"edges\": [ { \"from\": \"read\", \"to\": \"ok\", \"label\": \"utf-8\" } ] }. \
`kind` is required and selects the diagram kind; an unknown kind comes back \
as `unknown_kind` with the available kinds in `hint`. Every node needs a \
unique `id`; edges reference node ids with `from` and `to`. \
Unknown fields are rejected, so send only documented keys. The response is \
JSON: { ok, diagram_id, rendered_kind, rendered, metrics, extracted_from, \
replaced, errors, warnings, session }. `ok: false` means nothing was placed and \
`errors[]` \
carries `{ path, code, message, hint? }` with a stable `code`; `ok: true` \
returns the `diagram_id` that was placed —the new one, or the one \
`opts.replace_id` named—, the `rendered_kind`, `metrics { nodes, edges, \
layout_ms, render_ms, svg_bytes }` and `rendered` telling whether the window \
confirmed the paint. `opts` is optional: `opts.extract: { around, depth? }` \
draws only the neighbourhood of one node (default `depth: 1`, at most 300 \
nodes) and the response carries `extracted_from: \"<diagram_id>@<depth>\"`; \
`opts.layout_timeout_ms` sets the layout deadline (default 2000, ceiling \
10000) and a layout that misses it answers `ok: false, code: \
\"layout_timeout\"` with `metrics.elapsed_ms`, leaving the plan untouched. \
`opts.focus: { id, depth? }` puts the camera on that node and, with `depth`, on \
its neighbourhood. `opts.replace_id: \"<diagram_id>\"` replaces the document of \
that diagram instead of creating another one — the same cycle as an update plus \
the render —, an unknown id answers `unknown_diagram` and creates nothing, and \
the answer names the diagram in `replaced`; without it the render sums to the \
plan. `opts.interactive` arrives in F3 and is ignored for now.";

/// Description of the `export_diagram` tool, written for the agent.
const EXPORT_DIAGRAM_DESCRIPTION: &str = "\
Exports a diagram of this session as SVG or PNG. With `format: \"svg\"` and \
`opts.inline: true` (or no `opts`) the SVG comes back as text in `content`, \
with zero disk access: paste it in a README or in a message. With `opts.path` \
the export writes a file instead, and the path is **relative to the exports \
root** (`~/.graphium/exports/` or the configured root): absolute paths, `..` \
and symlinks are refused with `export_path_outside_root`. `svg` and `png` \
both write to a path; `png` **requires** `opts.path`, because it is bytes and \
not text. An existing file is not replaced unless you send `opts.overwrite: \
true`; without it the answer is `export_exists` and the old bytes stay. \
`diagram_id` is optional: without it the tool exports the only open diagram \
of the session, and answers `ambiguous_diagram` with the candidates when \
there are two or more. The response is JSON: { ok, content, path, format, \
diagram_id, bytes, size_bytes, errors }. Inline SVG is capped at 1 MB (`ok: \
false, code: \"too_large\", size_bytes` past it: send `opts.path` or narrow \
the diagram), a PNG accepts `opts.scale` (default 1.0, ceiling 3.0) and \
`opts.max_px` (default 16 Mpx, ceiling 64 Mpx), and the whole export has a \
5 s deadline (`export_timeout`). Exporting never changes the plan.";

/// Description of the `validate_diagram` tool, written for the agent.
const VALIDATE_DIAGRAM_DESCRIPTION: &str = "\
Check a diagram document WITHOUT opening anything. Use it before \
`render_diagram` when the document is long or the last render failed: it \
runs the same validation and returns the same errors, but no window opens \
and no diagram is drawn. The response is JSON: { ok, errors, warnings, \
projected_svg_bytes, errors_truncated }. `ok: false` means the document is \
wrong and `errors[]` carries `{ path, code, message, hint? }` with a stable \
`code` and the JSON path of the problem; fix the path and call again. \
`ok: true` means the document is valid and `projected_svg_bytes` says how \
big the diagram would be.";

/// Description of the `get_diagram_schema` tool, written for the agent.
const GET_DIAGRAM_SCHEMA_DESCRIPTION: &str = "\
The contract of a diagram kind. Call it without arguments to get the index \
of the available kinds with their limits and a minimal example, or with \
`kind` (for example \"graph\") to get its JSON Schema, the cross rules, the \
valid shapes, the limits and an example. The schema is the same contract \
that `render_diagram` validates against: read it before writing a \
document, and use the `rules` to know what the schema cannot express.";

/// Parameters of the `validate_diagram` tool (PRD §11).
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ValidateDiagramParams {
    /// The complete diagram document, as a JSON object.
    pub diagram: serde_json::Value,
}

/// Parameters of the `get_diagram_schema` tool (PRD §11).
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetDiagramSchemaParams {
    /// The kind to describe. Without it, the index of the kinds.
    #[serde(default)]
    pub kind: Option<String>,
}

/// Description of the `list_diagrams` tool, written for the agent.
const LIST_DIAGRAMS_DESCRIPTION: &str = "\
The diagrams of this session that are on the plan: their ids, titles, kinds, \
revisions and when they were updated. Use it after a compaction, or when an \
id was lost, to find the diagram to update or export. The answer also carries \
`session`, the name the human sees for you. It never renders anything.";

/// Description of the `get_diagram` tool, written for the agent.
const GET_DIAGRAM_DESCRIPTION: &str = "\
The source document of a diagram, with its current revision. Use it to read \
before an `update_diagram` (the revision is the one to send back), or to \
recover a document after a compaction. It never renders anything.";

/// Description of the `list_diagram_versions` tool, written for the agent
/// (RF14; feature doc `f2-cierre.md`, R3).
const LIST_DIAGRAM_VERSIONS_DESCRIPTION: &str = "\
The versions of a diagram's source document, oldest first. Every \
`render_diagram` and every accepted `update_diagram` writes one; the store \
keeps the last 20 of each diagram and discards the oldest. The response is \
JSON: { ok, diagram_id, current_version, versions: [{ version, revision, \
updated_at, bytes }], errors? }. `current_version` is the version the diagram \
is on now; the «v3/7» of the window is its position among the kept versions \
(`versions.len()` is the 7), so a full history reads «v20/20». It is \
read-only and never renders: use `get_diagram_version` to read the document \
of one version, and `render_diagram` or `update_diagram` to go back, which \
writes a new version instead of rewriting the history.";

/// Description of the `get_diagram_version` tool, written for the agent
/// (RF14, R3).
const GET_DIAGRAM_VERSION_DESCRIPTION: &str = "\
The source document of one version of a diagram: { ok, diagram_id, version, \
revision, updated_at, document, errors? }. Call `list_diagram_versions` \
first to know which numbers exist; a number the diagram does not keep answers \
`invalid_value` at `version` with the range it has. It is read-only: no tool \
restores a version. To restore one, send that document to `render_diagram` \
(or patch the current document with `update_diagram`): that writes a new \
version and never rewrites the history.";

/// Description of the `clear_canvas` tool, written for the agent.
const CLEAR_CANVAS_DESCRIPTION: &str = "\
Removes diagrams from the plan. Without `scope` it clears only the diagrams \
of this session; with `scope: \"plano\"` it clears the whole plan, \
including the diagrams of other sessions. It does not close the window and \
does not stop the MCP session.";

/// Description of the `update_diagram` tool, written for the agent.
const UPDATE_DIAGRAM_DESCRIPTION: &str = "\
Changes an existing diagram instead of creating another one. `patch` is an \
RFC 7386 merge patch: objects merge key by key, arrays are replaced whole, \
`null` removes a key. The merge is transactional: the result is validated \
with the same pipeline as `render_diagram` and only then replaces the \
document, so a bad patch leaves the diagram untouched and returns \
`applied: false` with the errors. Send the `revision` you read, so a stale \
write is rejected with `stale_revision`. The `kind` cannot change.";

/// Description of the `register_session` tool, written for the agent (S3, D2).
///
/// The text lives in the harness, which is where the relay reads it too: the
/// tool is described once and both surfaces publish the same words.
const REGISTER_SESSION_DESCRIPTION: &str = crate::harness::REGISTER_SESSION_DESCRIPTION;

/// Parameters of the `register_session` tool (S3, D2).
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RegisterSessionParams {
    /// The name the human sees for this session. Without it the tool only
    /// answers the session it is in.
    #[serde(default)]
    pub label: Option<String>,
}

/// Parameters of the `clear_canvas` tool (PRD §11).
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClearCanvasParams {
    /// `"sesion"` (default) or `"plano"`.
    #[serde(default)]
    pub scope: Option<String>,
}

/// Parameters of the `get_diagram` tool (PRD §11).
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetDiagramParams {
    /// The id returned by `render_diagram` or `list_diagrams`.
    pub diagram_id: String,
}

/// Parameters of the `list_diagram_versions` tool (RF14).
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListDiagramVersionsParams {
    /// The id returned by `render_diagram` or `list_diagrams`.
    pub diagram_id: String,
}

/// Parameters of the `get_diagram_version` tool (RF14).
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetDiagramVersionParams {
    /// The id returned by `render_diagram` or `list_diagrams`.
    pub diagram_id: String,
    /// The version number reported by `list_diagram_versions`, from `1`.
    pub version: u64,
}

/// Parameters of the `update_diagram` tool (PRD §11).
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateDiagramParams {
    /// The id returned by `render_diagram` or `list_diagrams`.
    pub diagram_id: String,
    /// The RFC 7386 merge patch to apply.
    pub patch: serde_json::Value,
    /// The revision the caller read; a stale one is rejected.
    #[serde(default)]
    pub revision: Option<u64>,
}

/// The options of an export (PRD §11, RF10).
///
/// Every field is optional so `opts: {}` works: the defaults are the ones of
/// PRD §7 RF10-bis (inline for `svg`, scale 1.0 and 16 Mpx for `png`).
#[derive(Debug, Clone, Default, serde::Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct ExportOpts {
    /// Returns the SVG as text instead of writing a file.
    pub inline: Option<bool>,
    /// The path relative to the exports root where the file is written.
    pub path: Option<String>,
    /// Whether an existing file may be replaced.
    pub overwrite: Option<bool>,
    /// Scale of the PNG (default 1.0, ceiling 3.0).
    pub scale: Option<f64>,
    /// Pixel ceiling of the PNG (default 16 Mpx, ceiling 64 Mpx).
    pub max_px: Option<u64>,
}

/// Parameters of the `export_diagram` tool (PRD §11).
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportDiagramParams {
    /// The id returned by `render_diagram` or `list_diagrams`. Without it the
    /// tool exports the only open diagram of the session.
    #[serde(default)]
    pub diagram_id: Option<String>,
    /// `svg` or `png`.
    pub format: String,
    /// The options of the export.
    #[serde(default)]
    pub opts: Option<ExportOpts>,
}

/// Parameters of the `render_diagram` tool (PRD §11).
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenderDiagramParams {
    /// The complete diagram document, as a JSON object.
    pub diagram: serde_json::Value,
    /// Options of the render: `extract { around, depth? }`,
    /// `focus { id, depth? }`, `replace_id` and `layout_timeout_ms`.
    /// `interactive` arrives in F3 and is ignored for now.
    #[serde(default)]
    pub opts: Option<serde_json::Value>,
}

/// Description of the `code_search` tool, written for the agent (S5, S7).
const CODE_SEARCH_DESCRIPTION: &str = "\
Search the book of this project: the symbols that the indexer extracted, \
without reading the files. It answers the places of ONE symbol: `file`, \
`line`, `kind`, `name`, `qname`, `content_hash` and `indexed_at`, plus the \
`book_key` of the book that answered. The query is split into words and every \
word has to match (`render config` finds a symbol whose name carries both); \
CamelCase and snake_case are both indexed. When the query fits more than one \
declaration the tool answers `ok: false` with code `ambiguous_symbol` and \
`candidates[]` (`name`, `kind`, `qname`, `file`, `line`, `occurrences`): send \
the words of one candidate's qname to narrow it, never guess. The book is on \
by default: `book_disabled` only answers when the owner opted out with \
`GRAPHIUM_BOOK=off`, and a book that cannot be opened answers \
`book_unavailable`. `progress.state == \"indexing\"` means the first pass is \
still running, so a missing symbol may be added in a moment.";

/// Parameters of the `code_search` tool (F9, C0).
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CodeSearchParams {
    /// The symbol to look for: one or more words of its name or qname.
    pub query: String,
}

/// Description of the `code_structure` tool, written for the agent (S5, S7).
const CODE_STRUCTURE_DESCRIPTION: &str = "\
Read the structure of one chapter of the book: the declarations of a file, \
ordered by span, each with `name`, `kind`, `qname`, `line`, `column`, \
`end_line`, `end_column` and `parent` (the qname of the declaration that \
contains it), so the containment tree is rebuildable from one answer. Ask with \
`file` (a path relative to the project root, with `/` separators) or with \
`symbol` (a name, as `code_search` takes it): exactly one of the two, and a \
symbol that fits more than one declaration answers `ok: false` with code \
`ambiguous_symbol` and `candidates[]`. The answer carries the `content_hash` \
and `indexed_at` of the chapter plus the `book_key` of the book that answered. \
`progress.state == \"indexing\"` means the first pass is still running. The \
book is on by default: `book_disabled` only answers when the owner opted out \
with `GRAPHIUM_BOOK=off`, and an unopenable book answers `book_unavailable`. \
Read-only: it never touches the plan.";

/// Parameters of the `code_structure` tool (F9, C1).
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CodeStructureParams {
    /// The path of the chapter, relative to the project root, with `/`.
    #[serde(default)]
    pub file: Option<String>,
    /// The symbol whose chapter to read: a name, as `code_search` takes it.
    #[serde(default)]
    pub symbol: Option<String>,
}

/// Description of the `code_impact` tool, written for the agent (S5, S7, D3).
const CODE_IMPACT_DESCRIPTION: &str = "\
Walk the connections of one symbol in the book: who points at it, what it \
reaches, or both. `direction` is `in` (default: the edges that point at the \
symbol, so what a change would affect), `out` (what the symbol calls, imports \
or uses) or `both`. The edges are **syntactic, not typed** (a call by name, an \
import by its bound segment, an `impl Trait for Type`, a type reference), so a \
common name may be a false positive and `unresolved` counts the targets the \
book could not pin to one declaration. The walk has three guards and the \
answer states them: `depth` is clamped to 3 hops, a cycle is answered once \
per symbol and never followed twice, and the answer is capped at the ceilings \
of the `graph` kind (300 nodes / 600 edges) with `truncated: true` when the \
cap bites. Every node carries `file`, `line` and the hop it was reached at; \
the symbol you asked about travels with `content_hash` and `indexed_at`, plus \
the `book_key` of the book. A name that fits more than one declaration \
answers `ok: false` with `ambiguous_symbol` and `candidates[]`; the book is \
on by default, so `book_disabled` only comes from an explicit \
`GRAPHIUM_BOOK=off`. Read-only: it never touches the plan.";

/// Parameters of the `code_impact` tool (F9, C1).
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CodeImpactParams {
    /// The symbol to start from: a name, as `code_search` takes it.
    pub symbol: String,
    /// Hops to follow: 1 by default, clamped to 3.
    #[serde(default)]
    pub depth: Option<u32>,
    /// `in` (default), `out` or `both`.
    #[serde(default)]
    pub direction: Option<String>,
}

/// Description of the `code_diagram` tool, written for the agent (S5).
const CODE_DIAGRAM_DESCRIPTION: &str = "\
Build the diagram of one symbol's neighbourhood from the book, without reading \
any file. The answer is a complete `graph` document (`version: 1`, `kind: \
\"graph\"`, one node per symbol with its `qname` as the label, one edge per \
connection labelled with its kind) in `diagram`: send it to `render_diagram` \
as it is, or to `validate_diagram` first. The document respects the contract of \
the kind (at most 300 nodes, 600 edges and a density of 3), so it is \
renderable with no repair. The walk is the same as `code_impact` with \
`direction: both` and `depth` clamped to 3 hops; `truncated` says whether a \
guard cut it. The edges are syntactic, not typed (D3): the picture is the \
book's reading of the code, not a compiler's. The start symbol travels with \
`content_hash`, `indexed_at` and the `book_key` of the book; an ambiguous name \
answers `ok: false` with `ambiguous_symbol` and `candidates[]`, and the book is \
on by default (`book_disabled` only with an explicit `GRAPHIUM_BOOK=off`). \
It never renders and never touches the plan: the diagram is a document.";

/// Parameters of the `code_diagram` tool (F9, C1).
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CodeDiagramParams {
    /// The symbol to draw the neighbourhood of: a name, as `code_search` takes it.
    pub symbol: String,
    /// Hops to follow: 1 by default, clamped to 3.
    #[serde(default)]
    pub depth: Option<u32>,
}

/// The MCP server of Graphium.
///
/// It is a plain [`ServerHandler`]: the tool router comes from the
/// `render_diagram` method and the transport is chosen by the caller.
#[derive(Debug, Clone)]
pub struct GraphiumServer {
    session_id: String,
}

impl GraphiumServer {
    /// A server for the stdio session started by [`serve_stdio`].
    pub fn new() -> Self {
        Self::with_session_id(session::STDIO_SESSION_ID)
    }

    /// A server whose diagrams are tagged with `session_id` (PRD §11.1).
    pub fn with_session_id(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
        }
    }
}

impl Default for GraphiumServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router]
impl GraphiumServer {
    #[tool(
        name = "render_diagram",
        description = RENDER_DIAGRAM_DESCRIPTION,
        annotations(
            title = "Render diagram",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn render_diagram(
        &self,
        Parameters(params): Parameters<RenderDiagramParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let document = serde_json::to_string(&params.diagram).map_err(|error| {
            ErrorData::invalid_params(format!("`diagram` is not valid JSON: {error}"), None)
        })?;
        let request = tools::RenderRequest {
            document,
            session_id: self.session_id.clone(),
        };
        let raw_opts = params.opts.clone();

        // Validate + IR + layout + SVG run off the async loop, under the
        // deadline of `opts.layout_timeout_ms` (PRD §8.6.1, §7.3). The opts
        // are parsed by `gx-mcp`, in one place, so this path and the socket
        // relay cannot diverge.
        let response = tokio::task::spawn_blocking(move || {
            // The indexer parses with one thread while a render is in flight
            // (ideas §10.1, «Backpressure»).
            let _render = gx_index::service::RenderGuard::enter();
            tools::render_diagram_from_json(&request, raw_opts.as_ref())
        })
        .await
        .map_err(|error| {
            ErrorData::internal_error(format!("the render task failed: {error}"), None)
        })?;

        let value = serde_json::to_value(&response).map_err(|error| {
            ErrorData::internal_error(format!("cannot serialize the response: {error}"), None)
        })?;
        Ok(CallToolResult::structured(value))
    }

    #[tool(
        name = "list_diagrams",
        description = LIST_DIAGRAMS_DESCRIPTION,
        annotations(
            title = "List diagrams",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn list_diagrams(&self) -> Result<CallToolResult, ErrorData> {
        let response = contract::list_diagrams(&self.session_id);
        structured(response)
    }

    #[tool(
        name = "get_diagram",
        description = GET_DIAGRAM_DESCRIPTION,
        annotations(
            title = "Get diagram",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn get_diagram(
        &self,
        Parameters(params): Parameters<GetDiagramParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let response = contract::get_diagram(&self.session_id, &params.diagram_id);
        structured(response)
    }

    #[tool(
        name = "list_diagram_versions",
        description = LIST_DIAGRAM_VERSIONS_DESCRIPTION,
        annotations(
            title = "List diagram versions",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn list_diagram_versions(
        &self,
        Parameters(params): Parameters<ListDiagramVersionsParams>,
    ) -> Result<CallToolResult, ErrorData> {
        structured(contract::list_diagram_versions(
            &self.session_id,
            &params.diagram_id,
        ))
    }

    #[tool(
        name = "get_diagram_version",
        description = GET_DIAGRAM_VERSION_DESCRIPTION,
        annotations(
            title = "Get diagram version",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn get_diagram_version(
        &self,
        Parameters(params): Parameters<GetDiagramVersionParams>,
    ) -> Result<CallToolResult, ErrorData> {
        structured(contract::get_diagram_version(
            &self.session_id,
            &params.diagram_id,
            params.version,
        ))
    }

    #[tool(
        name = "export_diagram",
        description = EXPORT_DIAGRAM_DESCRIPTION,
        annotations(
            title = "Export diagram",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn export_diagram(
        &self,
        Parameters(params): Parameters<ExportDiagramParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let opts = params.opts.unwrap_or_default();
        let request = export::ExportRequest {
            session_id: self.session_id.clone(),
            diagram_id: params.diagram_id,
            format: params.format,
            inline: opts.inline.unwrap_or(false),
            path: opts.path,
            overwrite: opts.overwrite.unwrap_or(false),
            scale: opts.scale,
            max_px: opts.max_px,
        };

        // The render, the rasterization and the write run off the async loop,
        // under the deadline of RF10-bis (PRD §7).
        let response = tokio::task::spawn_blocking(move || export::export_diagram(&request))
            .await
            .map_err(|error| {
                ErrorData::internal_error(format!("the export task failed: {error}"), None)
            })?;
        structured(response)
    }

    #[tool(
        name = "clear_canvas",
        description = CLEAR_CANVAS_DESCRIPTION,
        annotations(
            title = "Clear canvas",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn clear_canvas(
        &self,
        Parameters(params): Parameters<ClearCanvasParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let response = contract::clear_canvas(&self.session_id, params.scope.as_deref());
        structured(response)
    }

    #[tool(
        name = "update_diagram",
        description = UPDATE_DIAGRAM_DESCRIPTION,
        annotations(
            title = "Update diagram",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn update_diagram(
        &self,
        Parameters(params): Parameters<UpdateDiagramParams>,
    ) -> Result<CallToolResult, ErrorData> {
        // The merge, the validation and the re-render run off the async loop.
        let session_id = self.session_id.clone();
        let response = tokio::task::spawn_blocking(move || {
            contract::update_diagram(
                &session_id,
                &params.diagram_id,
                &params.patch,
                params.revision,
            )
        })
        .await
        .map_err(|error| {
            ErrorData::internal_error(format!("the update task failed: {error}"), None)
        })?;
        structured(response)
    }

    #[tool(
        name = "validate_diagram",
        description = VALIDATE_DIAGRAM_DESCRIPTION,
        annotations(
            title = "Validate diagram",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn validate_diagram(
        &self,
        Parameters(params): Parameters<ValidateDiagramParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let document = serde_json::to_string(&params.diagram).map_err(|error| {
            ErrorData::invalid_params(format!("`diagram` is not valid JSON: {error}"), None)
        })?;

        // The dry run is pure validation: no layout, no render, no window.
        let response = contract::validate_diagram(&document);
        let value = serde_json::to_value(&response).map_err(|error| {
            ErrorData::internal_error(format!("cannot serialize the response: {error}"), None)
        })?;
        Ok(CallToolResult::structured(value))
    }

    #[tool(
        name = "get_diagram_schema",
        description = GET_DIAGRAM_SCHEMA_DESCRIPTION,
        annotations(
            title = "Get diagram schema",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn get_diagram_schema(
        &self,
        Parameters(params): Parameters<GetDiagramSchemaParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let response = contract::get_diagram_schema(params.kind.as_deref());
        let value = serde_json::to_value(&response).map_err(|error| {
            ErrorData::internal_error(format!("cannot serialize the response: {error}"), None)
        })?;
        Ok(CallToolResult::structured(value))
    }

    #[tool(
        name = "register_session",
        description = REGISTER_SESSION_DESCRIPTION,
        annotations(
            title = "Register session",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn register_session(
        &self,
        Parameters(params): Parameters<RegisterSessionParams>,
    ) -> Result<CallToolResult, ErrorData> {
        structured(contract::register_session(
            &self.session_id,
            params.label.as_deref(),
        ))
    }

    #[tool(
        name = "code_search",
        description = CODE_SEARCH_DESCRIPTION,
        annotations(
            title = "Search the book of the project",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn code_search(
        &self,
        Parameters(params): Parameters<CodeSearchParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let session_id = self.session_id.clone();
        // The search opens the book of the session's project and reads it with
        // a connection of its own: it runs off the async loop.
        let response =
            tokio::task::spawn_blocking(move || book::code_search(&session_id, &params.query))
                .await
                .map_err(|error| {
                    ErrorData::internal_error(format!("the search task failed: {error}"), None)
                })?;
        structured(response)
    }

    #[tool(
        name = "code_structure",
        description = CODE_STRUCTURE_DESCRIPTION,
        annotations(
            title = "Read the structure of a chapter of the book",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn code_structure(
        &self,
        Parameters(params): Parameters<CodeStructureParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let session_id = self.session_id.clone();
        // Reading the book opens it in the owner's process: off the async loop.
        let response = tokio::task::spawn_blocking(move || {
            book::code_structure(
                &session_id,
                params.file.as_deref(),
                params.symbol.as_deref(),
            )
        })
        .await
        .map_err(|error| {
            ErrorData::internal_error(format!("the structure task failed: {error}"), None)
        })?;
        structured(response)
    }

    #[tool(
        name = "code_impact",
        description = CODE_IMPACT_DESCRIPTION,
        annotations(
            title = "Walk the connections of a symbol",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn code_impact(
        &self,
        Parameters(params): Parameters<CodeImpactParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let session_id = self.session_id.clone();
        let response = tokio::task::spawn_blocking(move || {
            book::code_impact(
                &session_id,
                &params.symbol,
                params.depth,
                params.direction.as_deref(),
            )
        })
        .await
        .map_err(|error| {
            ErrorData::internal_error(format!("the impact task failed: {error}"), None)
        })?;
        structured(response)
    }

    #[tool(
        name = "code_diagram",
        description = CODE_DIAGRAM_DESCRIPTION,
        annotations(
            title = "Build the diagram of a symbol from the book",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn code_diagram(
        &self,
        Parameters(params): Parameters<CodeDiagramParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let session_id = self.session_id.clone();
        let response = tokio::task::spawn_blocking(move || {
            book::code_diagram(&session_id, &params.symbol, params.depth)
        })
        .await
        .map_err(|error| {
            ErrorData::internal_error(format!("the diagram task failed: {error}"), None)
        })?;
        structured(response)
    }
}

#[tool_handler(name = "graphium")]
impl ServerHandler for GraphiumServer {
    /// The instructions the client shows to the agent.
    ///
    /// They come from [`crate::harness`] instead of the attribute of the
    /// macro, so the text has one source and can be tested.
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("graphium", env!("CARGO_PKG_VERSION")))
            .with_instructions(crate::harness::INSTRUCTIONS.to_string())
    }
}

/// Serializes a tool response into a structured result.
fn structured<T: serde::Serialize>(response: T) -> Result<CallToolResult, ErrorData> {
    let value = serde_json::to_value(&response).map_err(|error| {
        ErrorData::internal_error(format!("cannot serialize the response: {error}"), None)
    })?;
    Ok(CallToolResult::structured(value))
}

/// Builds the runtime, serves stdio and waits for EOF.
fn run_stdio_service() -> Result<(), String> {
    // The stdio session of the owner is a visible session too (feature
    // `f3-sesiones`, D1/D4): it registers with what the client declared and
    // what the process says. The tools keep answering with `session_id:
    // "stdio"`, so this changes nothing they return.
    session::register_transport_session(session::STDIO_SESSION_ID);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("cannot build the stdio runtime: {error}"))?;

    runtime.block_on(async {
        let service = GraphiumServer::new()
            .serve(rmcp::transport::stdio())
            .await
            .map_err(|error| format!("MCP handshake failed: {error}"))?;
        let reason = service
            .waiting()
            .await
            .map_err(|error| format!("MCP service failed: {error}"))?;
        eprintln!("gx-mcp: stdio server stopped ({reason:?})");
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use rmcp::handler::server::wrapper::Parameters;
    use rmcp::ServiceExt;

    use super::{
        mcp_config_json, ExportDiagramParams, GetDiagramVersionParams, GraphiumServer,
        ListDiagramVersionsParams, RegisterSessionParams, RenderDiagramParams, UpdateDiagramParams,
    };
    use crate::test_support::{lock_renders, MINIMAL};

    #[test]
    fn mcp_config_prints_an_absolute_command_with_the_stdio_flag() {
        let printed = mcp_config_json(Path::new("/opt/graphium/bin/graphium"));
        assert_eq!(
            printed,
            r#"{"command":"/opt/graphium/bin/graphium","args":["--stdio"]}"#
        );
        let config: serde_json::Value = serde_json::from_str(&printed).expect("the config is JSON");

        let command = config["command"].as_str().expect("command is a string");
        assert_eq!(command, "/opt/graphium/bin/graphium");
        assert!(Path::new(command).is_absolute(), "{command}");
        assert_eq!(config["args"], serde_json::json!(["--stdio"]));
    }

    #[test]
    fn mcp_config_makes_a_relative_binary_absolute() {
        let config: serde_json::Value =
            serde_json::from_str(&mcp_config_json(Path::new("graphium")))
                .expect("the config is JSON");

        let command = config["command"].as_str().expect("command is a string");
        assert!(Path::new(command).is_absolute(), "{command}");
        assert!(command.ends_with("graphium"), "{command}");
    }

    #[test]
    fn the_server_advertises_render_diagram() {
        let tools = GraphiumServer::tool_router().list_all();
        let tool = tools
            .iter()
            .find(|tool| tool.name == "render_diagram")
            .expect("render_diagram is advertised");

        let description = tool.description.as_deref().unwrap_or_default();
        assert!(description.contains("diagram"), "{description}");
        assert!(description.len() > 80, "the agent needs a real description");

        let properties = tool.input_schema["properties"]
            .as_object()
            .expect("the input schema has properties");
        assert!(properties.contains_key("diagram"), "{properties:?}");
        assert!(properties.contains_key("opts"), "{properties:?}");
    }

    /// S3: la tool existe en el router, con la descripción que el agente
    /// necesita para elegir entre inline y archivo.
    #[test]
    fn the_server_advertises_export_diagram() {
        let tools = GraphiumServer::tool_router().list_all();
        let tool = tools
            .iter()
            .find(|tool| tool.name == "export_diagram")
            .expect("export_diagram is advertised");

        let description = tool.description.as_deref().unwrap_or_default();
        for expected in ["inline", "path", "overwrite", "png"] {
            assert!(
                description.contains(expected),
                "the description names `{expected}`: {description}"
            );
        }

        let properties = tool.input_schema["properties"]
            .as_object()
            .expect("the input schema has properties");
        assert!(properties.contains_key("diagram_id"), "{properties:?}");
        assert!(properties.contains_key("format"), "{properties:?}");
        assert!(properties.contains_key("opts"), "{properties:?}");
    }

    /// S3/D2: the router of the stdio server advertises the tool that names
    /// the session, with an optional `label`. The relay reaches it through
    /// the dispatcher; an agent that speaks stdio to the owner has to see the
    /// same tool.
    #[test]
    fn the_server_advertises_register_session() {
        let tools = GraphiumServer::tool_router().list_all();
        let tool = tools
            .iter()
            .find(|tool| tool.name == "register_session")
            .expect("register_session is advertised");

        let description = tool.description.as_deref().unwrap_or_default();
        for expected in ["label", "session", "invalid_value"] {
            assert!(
                description.contains(expected),
                "the description names `{expected}`: {description}"
            );
        }

        let properties = tool.input_schema["properties"]
            .as_object()
            .expect("the input schema has properties");
        assert!(properties.contains_key("label"), "{properties:?}");
        let required = tool
            .input_schema
            .get("required")
            .and_then(serde_json::Value::as_array);
        assert!(
            required.is_none_or(|required| required.is_empty()),
            "`label` is optional: {:?}",
            tool.input_schema
        );
    }

    /// D2: the agent names its session through the tool, the name is trimmed
    /// and it survives a second call without a label; an empty label is the
    /// error the agent can branch on and never a rename.
    #[tokio::test]
    async fn the_register_session_tool_names_the_session() {
        let id = format!("tool-session-{}", std::process::id());
        let server = GraphiumServer::with_session_id(&id);

        let named = server
            .register_session(Parameters(RegisterSessionParams {
                label: Some("  presupuesto API  ".to_string()),
            }))
            .await
            .expect("answering never fails")
            .structured_content
            .expect("structured content");
        assert_eq!(named["ok"], true, "{named}");
        assert_eq!(named["session"]["label"], "presupuesto API", "{named}");
        assert_eq!(named["session"]["state"], "active", "{named}");
        let answered_id = named["session"]["id"]
            .as_str()
            .expect("the session names its id")
            .to_string();
        assert!(!answered_id.is_empty(), "{named}");

        let whoami = server
            .register_session(Parameters(RegisterSessionParams { label: None }))
            .await
            .expect("answering never fails")
            .structured_content
            .expect("structured content");
        assert_eq!(whoami["session"]["label"], "presupuesto API", "{whoami}");
        assert_eq!(whoami["session"]["id"], answered_id, "{whoami}");

        let rejected = server
            .register_session(Parameters(RegisterSessionParams {
                label: Some("   ".to_string()),
            }))
            .await
            .expect("a rejected label is an answer")
            .structured_content
            .expect("structured content");
        assert_eq!(rejected["ok"], false, "{rejected}");
        assert_eq!(rejected["errors"][0]["code"], "invalid_value", "{rejected}");
        assert_eq!(rejected["errors"][0]["path"], "label", "{rejected}");
        assert!(rejected["errors"][0]["hint"].is_string(), "{rejected}");
    }

    /// S3: `opts` es opcional y un `opts: {}` vacío tiene que funcionar.
    #[test]
    fn an_empty_opts_is_the_default_export() {
        let params: ExportDiagramParams = serde_json::from_value(serde_json::json!({
            "format": "svg",
            "opts": {}
        }))
        .expect("an empty opts is valid");
        assert!(params.diagram_id.is_none());
        let opts = params.opts.expect("opts is there");
        assert_eq!(opts.inline, None);
        assert_eq!(opts.path, None);
        assert_eq!(opts.overwrite, None);
        assert_eq!(opts.scale, None);
        assert_eq!(opts.max_px, None);

        let bare: ExportDiagramParams =
            serde_json::from_value(serde_json::json!({ "format": "svg" }))
                .expect("opts is optional");
        assert!(bare.opts.is_none());
    }

    #[tokio::test]
    async fn the_tool_renders_and_reports_opts_as_a_warning() {
        let _guard = lock_renders().await;
        let server = GraphiumServer::new();
        let diagram = serde_json::from_str(MINIMAL).expect("the example is JSON");

        let result = server
            .render_diagram(Parameters(RenderDiagramParams {
                diagram,
                opts: Some(serde_json::json!({ "interactive": false })),
            }))
            .await
            .expect("a valid document never fails the tool");

        let value = result.structured_content.expect("structured content");
        assert_eq!(value["ok"], true, "{value}");
        assert_eq!(value["diagram_id"], "d-0001");
        assert_eq!(value["rendered_kind"], "graph");
        assert_eq!(value["rendered"], false);
        let warnings: Vec<&str> = value["warnings"]
            .as_array()
            .expect("warnings are reported")
            .iter()
            .filter_map(|warning| warning["code"].as_str())
            .collect();
        assert!(warnings.contains(&"opts_ignored"), "{warnings:?}");
        assert!(warnings.contains(&"no_window"), "{warnings:?}");
    }

    /// RF14/R3: the router advertises the two read-only history tools with the
    /// arguments the dispatcher reads. A tool that exists in the dispatcher
    /// and not here is invisible to a stdio client.
    #[test]
    fn the_server_advertises_the_history_tools() {
        let tools = GraphiumServer::tool_router().list_all();

        for (name, required) in [
            ("list_diagram_versions", "diagram_id"),
            ("get_diagram_version", "version"),
        ] {
            let tool = tools
                .iter()
                .find(|tool| tool.name == name)
                .unwrap_or_else(|| panic!("{name} is advertised"));
            let description = tool.description.as_deref().unwrap_or_default();
            assert!(description.len() > 80, "{name} needs a real description");
            assert!(
                tool.annotations
                    .as_ref()
                    .is_some_and(|annotations| annotations.read_only_hint.unwrap_or(false)),
                "{name} is read-only: {:?}",
                tool.annotations
            );
            let properties = tool.input_schema["properties"]
                .as_object()
                .expect("the input schema has properties");
            assert!(properties.contains_key("diagram_id"), "{properties:?}");
            assert!(
                properties.contains_key(required),
                "{name} takes `{required}`: {properties:?}"
            );
        }
    }

    /// RF14/R3: the stdio server reaches the history through the same
    /// `contract` functions the relay uses.
    #[tokio::test]
    async fn the_history_tools_read_the_versions_of_a_diagram() {
        let _guard = lock_renders().await;
        let server = GraphiumServer::new();
        let diagram = serde_json::from_str(MINIMAL).expect("the example is JSON");

        let rendered = server
            .render_diagram(Parameters(RenderDiagramParams {
                diagram,
                opts: None,
            }))
            .await
            .expect("a valid document never fails the tool")
            .structured_content
            .expect("structured content");
        let diagram_id = rendered["diagram_id"]
            .as_str()
            .expect("the render names the diagram")
            .to_string();

        server
            .update_diagram(Parameters(UpdateDiagramParams {
                diagram_id: diagram_id.clone(),
                patch: serde_json::json!({ "title": "segunda" }),
                revision: None,
            }))
            .await
            .expect("the update answers");

        let listed = server
            .list_diagram_versions(Parameters(ListDiagramVersionsParams {
                diagram_id: diagram_id.clone(),
            }))
            .await
            .expect("the history answers")
            .structured_content
            .expect("structured content");
        assert_eq!(listed["ok"], true, "{listed}");
        assert_eq!(listed["current_version"], 2, "{listed}");

        let first = server
            .get_diagram_version(Parameters(GetDiagramVersionParams {
                diagram_id,
                version: 1,
            }))
            .await
            .expect("the version answers")
            .structured_content
            .expect("structured content");
        assert_eq!(first["ok"], true, "{first}");
        assert_eq!(first["version"], 1, "{first}");
        assert_eq!(
            first["document"]["title"], "parse_config() — flujo",
            "{first}"
        );
    }

    /// S21: `opts.extract` is honoured and, when every key is supported,
    /// there is no `opts_ignored` warning at all.
    #[tokio::test]
    async fn opts_extract_is_honored_without_an_opts_ignored_warning() {
        let _guard = lock_renders().await;
        let server = GraphiumServer::new();
        let diagram = serde_json::from_str(MINIMAL).expect("the example is JSON");

        let result = server
            .render_diagram(Parameters(RenderDiagramParams {
                diagram,
                opts: Some(serde_json::json!({ "extract": { "around": "read" } })),
            }))
            .await
            .expect("a valid document never fails the tool");

        let value = result.structured_content.expect("structured content");
        assert_eq!(value["ok"], true, "{value}");
        assert_eq!(value["extracted_from"], "d-0001@1");
        assert_eq!(value["metrics"]["nodes"], 2);
        let warnings: Vec<&str> = value["warnings"]
            .as_array()
            .expect("warnings are reported")
            .iter()
            .filter_map(|warning| warning["code"].as_str())
            .collect();
        assert!(
            !warnings.contains(&"opts_ignored"),
            "every key is supported, nothing is ignored: {warnings:?}"
        );
    }

    /// S21: the message of `opts_ignored` names the keys of the later phases,
    /// so the agent learns when they arrive. `opts.focus` y `opts.replace_id`
    /// ya se honran en F2, así que el aviso queda para `opts.interactive`.
    #[tokio::test]
    async fn opts_ignored_names_the_keys_of_the_later_phases() {
        let _guard = lock_renders().await;
        let server = GraphiumServer::new();
        let diagram = serde_json::from_str(MINIMAL).expect("the example is JSON");

        let result = server
            .render_diagram(Parameters(RenderDiagramParams {
                diagram,
                opts: Some(serde_json::json!({ "interactive": true })),
            }))
            .await
            .expect("a valid document never fails the tool");

        let value = result.structured_content.expect("structured content");
        let warning = value["warnings"]
            .as_array()
            .expect("warnings are reported")
            .iter()
            .find(|warning| warning["code"] == "opts_ignored")
            .expect("the ignored key is reported");
        let message = warning["message"].as_str().expect("the warning says why");
        assert!(
            message.contains("opts.interactive"),
            "the message names the key of the later phase: {message}"
        );
        assert!(
            !message.contains("opts.replace_id`, `opts.interactive")
                && message.contains("honra `opts.extract`, `opts.focus`, `opts.replace_id`"),
            "el aviso dice que replace_id ya se honra: {message}"
        );
    }

    /// PRD §7 (RF27): `opts.focus` llega a la ventana como una caja a mostrar
    /// y no deja ningún aviso de opción ignorada.
    #[tokio::test]
    async fn opts_focus_is_honored_without_a_warning() {
        let _guard = lock_renders().await;
        let server = GraphiumServer::new();
        let diagram = serde_json::from_str(MINIMAL).expect("the example is JSON");

        let result = server
            .render_diagram(Parameters(RenderDiagramParams {
                diagram,
                opts: Some(serde_json::json!({ "focus": { "id": "read", "depth": 0 } })),
            }))
            .await
            .expect("the tool answers");

        let value = result.structured_content.expect("structured content");
        assert_eq!(value["ok"], true, "{value}");
        let codes: Vec<&str> = value["warnings"]
            .as_array()
            .map(|warnings| {
                warnings
                    .iter()
                    .filter_map(|warning| warning["code"].as_str())
                    .collect()
            })
            .unwrap_or_default();
        assert!(
            !codes.contains(&"opts_ignored"),
            "`opts.focus` se honra: {codes:?}"
        );
    }

    /// A malformed `opts.extract` never falls back to the whole diagram: it is
    /// an actionable error and nothing is stored.
    #[tokio::test]
    async fn a_malformed_extract_is_reported_and_stores_nothing() {
        let _guard = lock_renders().await;
        let server = GraphiumServer::new();
        let diagram = serde_json::from_str(MINIMAL).expect("the example is JSON");

        let result = server
            .render_diagram(Parameters(RenderDiagramParams {
                diagram,
                opts: Some(serde_json::json!({ "extract": { "depth": 2 } })),
            }))
            .await
            .expect("a malformed option is an answer, not a crash");

        let value = result.structured_content.expect("structured content");
        assert_eq!(value["ok"], false, "{value}");
        assert_eq!(value["errors"][0]["path"], "opts.extract.around");
        assert_eq!(value["errors"][0]["code"], "missing_field");
        assert!(value.get("diagram_id").is_none(), "{value}");
        assert!(
            crate::store::diagram_store().list().is_empty(),
            "a malformed option never reaches the plane"
        );
    }

    /// PRESERVE: the stdio server keeps tagging its diagrams with
    /// `session_id: "stdio"` (PRD §11.1). The session registry describes the
    /// session; it does not move the cerco.
    #[tokio::test]
    async fn the_stdio_server_still_tags_its_diagrams_with_stdio() {
        let _guard = lock_renders().await;
        let server = GraphiumServer::new();
        let diagram = serde_json::from_str(MINIMAL).expect("the example is JSON");

        server
            .render_diagram(Parameters(RenderDiagramParams {
                diagram,
                opts: None,
            }))
            .await
            .expect("a valid document never fails the tool");

        let listed = server
            .list_diagrams()
            .await
            .expect("list_diagrams never fails")
            .structured_content
            .expect("structured content");
        assert_eq!(listed["ok"], true, "{listed}");
        assert_eq!(listed["diagrams"][0]["session_id"], "stdio", "{listed}");
    }

    /// Reads one line and decodes it as JSON-RPC: anything else on the wire is
    /// a bug (stdout is JSON-RPC only, PRD §8.6.1).
    async fn read_json_rpc_line<R>(reader: &mut R) -> serde_json::Value
    where
        R: tokio::io::AsyncBufRead + Unpin,
    {
        use tokio::io::AsyncBufReadExt;

        let mut line = String::new();
        let read = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            reader.read_line(&mut line),
        )
        .await
        .expect("the server answers within the timeout")
        .expect("the transport is readable");
        assert!(read > 0, "the server closed the transport");
        serde_json::from_str(&line)
            .unwrap_or_else(|error| panic!("stdout must be pure JSON-RPC, got {error}: {line:?}"))
    }

    #[tokio::test]
    async fn the_service_answers_json_rpc_over_the_transport() {
        use tokio::io::{AsyncWriteExt, BufReader};

        let _guard = lock_renders().await;
        let (client, server) = tokio::io::duplex(64 * 1024);
        let (server_read, server_write) = tokio::io::split(server);
        let (client_read, mut client_write) = tokio::io::split(client);

        let running = tokio::spawn(async move {
            let service = GraphiumServer::new()
                .serve((server_read, server_write))
                .await
                .map_err(|error| error.to_string())?;
            service.waiting().await.map_err(|error| error.to_string())?;
            Ok::<(), String>(())
        });

        let mut reader = BufReader::new(client_read);
        client_write
            .write_all(
                b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-06-18\",\"capabilities\":{},\"clientInfo\":{\"name\":\"graphium-test\",\"version\":\"0\"}}}\n",
            )
            .await
            .expect("the initialize request is written");
        let initialized = read_json_rpc_line(&mut reader).await;
        assert_eq!(initialized["id"], 1, "{initialized}");
        assert_eq!(initialized["result"]["serverInfo"]["name"], "graphium");

        client_write
            .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
            .await
            .expect("the initialized notification is written");
        client_write
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n")
            .await
            .expect("the tools/list request is written");
        let listed = read_json_rpc_line(&mut reader).await;
        assert_eq!(listed["id"], 2, "{listed}");
        let tools = listed["result"]["tools"]
            .as_array()
            .expect("tools/list returns tools");
        assert!(
            tools.iter().any(|tool| tool["name"] == "render_diagram"),
            "{tools:?}"
        );

        let call = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "render_diagram",
                "arguments": {
                    "diagram": serde_json::from_str::<serde_json::Value>(MINIMAL)
                        .expect("the example is JSON")
                }
            }
        });
        client_write
            .write_all(format!("{call}\n").as_bytes())
            .await
            .expect("the tools/call request is written");
        let called = read_json_rpc_line(&mut reader).await;
        assert_eq!(called["id"], 3, "{called}");
        assert_eq!(called["result"]["structuredContent"]["ok"], true);
        assert_eq!(
            called["result"]["structuredContent"]["diagram_id"],
            "d-0001"
        );

        // Closing both halves of the client is the EOF of the last client
        // (PRD §8.6): the service stops on its own.
        drop(reader);
        drop(client_write);
        let finished = tokio::time::timeout(std::time::Duration::from_secs(10), running)
            .await
            .expect("the service stops on EOF")
            .expect("the service task does not panic");
        assert!(finished.is_ok(), "{finished:?}");
    }

    // -----------------------------------------------------------------
    // F9, C0 — `code_search`, the door to the book of the project
    // -----------------------------------------------------------------

    /// A worktree and a books root inside the system temp directory.
    struct BookScratch {
        base: std::path::PathBuf,
    }

    impl BookScratch {
        fn new(name: &str) -> Self {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static COUNTER: AtomicUsize = AtomicUsize::new(0);
            let base = std::env::temp_dir().join(format!(
                "gx-mcp-book-{name}-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::SeqCst)
            ));
            std::fs::create_dir_all(base.join("worktree/src")).expect("the scratch tree");
            Self { base }
        }

        fn root(&self) -> std::path::PathBuf {
            self.base.join("worktree")
        }

        fn config(&self) -> gx_index::BooksConfig {
            gx_index::BooksConfig::at(self.base.join("books"))
        }

        fn write(&self, relative: &str, text: &str) {
            let path = self.root().join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("the parent");
            }
            std::fs::write(path, text).expect("the file");
        }

        /// Opens the book and indexes it once, synchronously.
        fn indexed(&self) -> std::sync::Arc<gx_index::service::BookHandle> {
            let handle = gx_index::service::ensure(&self.config(), &self.root()).expect("the book");
            handle.indexer().run_full().expect("the first index");
            // The watcher's own async first pass re-runs and may still hold the
            // progress row at `indexing` when run_full returns; the answers the
            // contract tests read must see the settled state (the live flake of
            // `the_code_structure_contract_answers_a_file_and_a_symbol`).
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while std::time::Instant::now() < deadline {
                match handle.book().progress() {
                    Ok(progress) if progress.state == "indexed" => return handle,
                    _ => std::thread::sleep(std::time::Duration::from_millis(20)),
                }
            }
            panic!("the scratch book never reached the indexed state");
        }
    }

    impl Drop for BookScratch {
        fn drop(&mut self) {
            gx_index::service::shutdown_all();
            let _ = std::fs::remove_dir_all(&self.base);
        }
    }

    /// S5/S7: the answer carries `file:line`, `kind`, `qname`, `content_hash`,
    /// `indexed_at` and the `book_key` of the book that answered.
    #[test]
    fn the_code_search_contract_carries_the_location_and_the_book_key() {
        let scratch = BookScratch::new("contract");
        scratch.write(
            "src/tools.rs",
            "pub fn render_diagram() {}\npub struct Session;\n",
        );
        let handle = scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();

        let response =
            crate::book::code_search_with(&scratch.config(), Some(&project), "render_diagram");
        assert_eq!(response["ok"], true, "{response}");
        assert_eq!(response["count"], 1, "{response}");
        assert_eq!(response["book_key"], handle.book().key(), "{response}");
        let hit = &response["results"][0];
        assert_eq!(hit["file"], "src/tools.rs", "{response}");
        assert_eq!(hit["line"], 1, "{response}");
        assert_eq!(hit["kind"], "fn", "{response}");
        assert_eq!(hit["name"], "render_diagram", "{response}");
        assert_eq!(hit["qname"], "tools::render_diagram", "{response}");
        assert!(
            hit["content_hash"]
                .as_str()
                .is_some_and(|hash| hash.len() == 64),
            "{response}"
        );
        assert!(
            hit["indexed_at"]
                .as_str()
                .is_some_and(|at| at.ends_with('Z') && at.contains('T')),
            "indexed_at is ISO-8601 at the edge of the tool: {response}"
        );
        assert_eq!(response["progress"]["state"], "indexed", "{response}");

        // An unknown name is an empty answer, not an error.
        let empty =
            crate::book::code_search_with(&scratch.config(), Some(&project), "nothing_like_this");
        assert_eq!(empty["ok"], true, "{empty}");
        assert_eq!(empty["count"], 0, "{empty}");
        assert_eq!(empty["results"].as_array().map(Vec::len), Some(0));
    }

    /// S5/L5: the book is **on by default** — an absent flag with a books root
    /// answers normally, with no `GRAPHIUM_BOOK=on` anywhere.
    #[test]
    fn the_book_answers_when_the_flag_is_absent() {
        let scratch = BookScratch::new("default-on");
        scratch.write("src/tools.rs", "pub fn render_diagram() {}\n");
        let project = scratch.root().to_string_lossy().to_string();
        // The first pass runs inside the watcher thread: finish it here so the
        // answer is about the flag and not about a pass in flight.
        scratch.indexed();
        // `None` is what an environment without GRAPHIUM_BOOK gives.
        let config = gx_index::BooksConfig::from_parts(None, Some(scratch.base.join("books")));
        assert!(config.is_enabled(), "an absent flag keeps the book on");

        let response = crate::book::code_search_with(&config, Some(&project), "render_diagram");
        assert_eq!(response["ok"], true, "{response}");
        assert_eq!(response["count"], 1, "{response}");
        assert_eq!(response["progress"]["state"], "indexed", "{response}");
    }

    /// S5/L5: only the explicit opt-out answers `book_disabled`, and it never
    /// touches the books root.
    #[test]
    fn the_disabled_path_answers_book_disabled() {
        let scratch = BookScratch::new("disabled");
        let project = scratch.root().to_string_lossy().to_string();
        let config =
            gx_index::BooksConfig::from_parts(Some("off".into()), Some(scratch.base.join("books")));
        assert!(!config.is_enabled(), "GRAPHIUM_BOOK=off is the opt-out");
        let response = crate::book::code_search_with(&config, Some(&project), "anything");
        assert_eq!(response["ok"], false, "{response}");
        assert_eq!(
            response["errors"][0]["code"],
            crate::book::DISABLED_CODE,
            "{response}"
        );
        assert_eq!(response["errors"][0]["path"], "book", "{response}");
        assert!(
            !scratch.base.join("books").exists(),
            "the disabled path never creates a books root"
        );
    }

    /// P3, condition 2: an ambiguous name answers `ambiguous_symbol` with
    /// `candidates[]`, never one confident answer.
    #[test]
    fn an_ambiguous_query_answers_the_code_and_the_candidates() {
        let scratch = BookScratch::new("ambiguous");
        scratch.write("src/one.rs", "pub fn parse() {}\n");
        scratch.write("src/two.rs", "pub fn parse() {}\n");
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();

        let response = crate::book::code_search_with(&scratch.config(), Some(&project), "parse");
        assert_eq!(response["ok"], false, "{response}");
        assert_eq!(
            response["errors"][0]["code"],
            crate::book::AMBIGUOUS_CODE,
            "{response}"
        );
        assert_eq!(response["errors"][0]["path"], "query", "{response}");
        let candidates = response["candidates"]
            .as_array()
            .expect("the candidates travel");
        assert_eq!(candidates.len(), 2, "{response}");
        let qnames: Vec<&str> = candidates
            .iter()
            .filter_map(|candidate| candidate["qname"].as_str())
            .collect();
        assert!(qnames.contains(&"one::parse"), "{qnames:?}");
        assert!(qnames.contains(&"two::parse"), "{qnames:?}");
        assert!(candidates
            .iter()
            .all(|candidate| candidate["occurrences"] == 2));

        // Narrowing with the words of one qname answers that one symbol.
        let narrowed =
            crate::book::code_search_with(&scratch.config(), Some(&project), "one parse");
        assert_eq!(narrowed["ok"], true, "{narrowed}");
        assert_eq!(narrowed["results"][0]["qname"], "one::parse", "{narrowed}");
    }

    /// D4: the book is the one of the session's project, read from the
    /// session record, never from the process cwd.
    #[test]
    fn the_project_of_a_search_is_the_project_of_the_session_record() {
        let scratch = BookScratch::new("session");
        let project = scratch.root().to_string_lossy().to_string();
        let identity = crate::session::ResolvedIdentity {
            id: Some("book-session".to_string()),
            family: crate::session::SessionFamily::Other,
            client_name: None,
            client_version: None,
            project: Some(project.clone()),
            source: crate::session::IdentitySource::Declared,
        };
        crate::session::session_registry().connect("book-session", &identity);

        assert_eq!(
            crate::book::project_of("book-session").as_deref(),
            Some(project.as_str())
        );
        assert!(
            crate::book::project_of("a-session-that-never-existed").is_none(),
            "an unknown session resolves no project: the cwd is never the fallback"
        );
    }

    /// The dispatcher of the owner reaches the same implementation, and an
    /// empty call is rejected before touching any book.
    #[test]
    fn the_dispatcher_exposes_code_search_and_requires_a_query() {
        let schema = crate::dispatch::tool_schema("code_search");
        assert_eq!(schema["required"][0], "query");

        let error = crate::dispatch::call_tool("code_search", &serde_json::json!({}), "s")
            .expect_err("`query` is required");
        assert!(error.contains("query"), "{error}");
        assert!(
            crate::dispatch::TOOL_NAMES.contains(&"code_search"),
            "the tool has to be in the index of the server"
        );
    }

    // -----------------------------------------------------------------
    // F9, C1 — `code_structure`, `code_impact` and `code_diagram`
    // -----------------------------------------------------------------

    /// S5/S7: `code_structure` answers the containment tree of a chapter and
    /// the same tree when the question names one symbol, with `file:line`,
    /// `content_hash`, `indexed_at` and the `book_key` of the book.
    #[test]
    fn the_code_structure_contract_answers_a_file_and_a_symbol() {
        let scratch = BookScratch::new("structure");
        scratch.write(
            "src/tools.rs",
            "pub fn render_diagram() {}\npub struct Session;\n",
        );
        let handle = scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();

        let by_file = crate::book::code_structure_with(
            &scratch.config(),
            Some(&project),
            Some("src/tools.rs"),
            None,
        );
        assert_eq!(by_file["ok"], true, "{by_file}");
        assert_eq!(by_file["book_key"], handle.book().key(), "{by_file}");
        assert_eq!(by_file["file"], "src/tools.rs", "{by_file}");
        assert_eq!(by_file["count"], 2, "{by_file}");
        assert_eq!(by_file["progress"]["state"], "indexed", "{by_file}");
        assert!(
            by_file["content_hash"]
                .as_str()
                .is_some_and(|hash| hash.len() == 64),
            "{by_file}"
        );
        assert!(
            by_file["indexed_at"]
                .as_str()
                .is_some_and(|at| at.ends_with('Z')),
            "{by_file}"
        );
        let symbols = by_file["symbols"].as_array().expect("the symbols travel");
        assert_eq!(symbols[0]["name"], "render_diagram", "{by_file}");
        assert_eq!(symbols[0]["qname"], "tools::render_diagram", "{by_file}");
        assert_eq!(symbols[0]["line"], 1, "{by_file}");
        assert!(symbols[0]["parent"].is_null(), "{by_file}");

        let by_symbol = crate::book::code_structure_with(
            &scratch.config(),
            Some(&project),
            None,
            Some("render_diagram"),
        );
        assert_eq!(by_symbol["ok"], true, "{by_symbol}");
        assert_eq!(by_symbol["file"], "src/tools.rs", "{by_symbol}");
        assert_eq!(by_symbol["symbol"]["name"], "render_diagram", "{by_symbol}");
        assert_eq!(by_symbol["count"], 2, "{by_symbol}");
    }

    /// S5: the tool takes exactly one selector, and an unknown path is an
    /// error with a hint instead of an empty chapter that reads as «no
    /// symbols».
    #[test]
    fn code_structure_rejects_zero_or_two_selectors_and_unknown_files() {
        let scratch = BookScratch::new("structure-args");
        scratch.write("src/one.rs", "pub fn one() {}\n");
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();

        let none = crate::book::code_structure_with(&scratch.config(), Some(&project), None, None);
        assert_eq!(none["ok"], false, "{none}");
        assert_eq!(none["errors"][0]["code"], "invalid_value", "{none}");

        let both = crate::book::code_structure_with(
            &scratch.config(),
            Some(&project),
            Some("src/one.rs"),
            Some("one"),
        );
        assert_eq!(both["ok"], false, "{both}");
        assert_eq!(both["errors"][0]["code"], "invalid_value", "{both}");

        let unknown = crate::book::code_structure_with(
            &scratch.config(),
            Some(&project),
            Some("src/nope.rs"),
            None,
        );
        assert_eq!(unknown["ok"], false, "{unknown}");
        assert_eq!(unknown["errors"][0]["code"], "invalid_value", "{unknown}");
        assert_eq!(unknown["errors"][0]["path"], "file", "{unknown}");
        assert!(unknown["errors"][0]["hint"].is_string(), "{unknown}");

        let no_symbol = crate::book::code_structure_with(
            &scratch.config(),
            Some(&project),
            None,
            Some("nothing_like_this"),
        );
        assert_eq!(no_symbol["ok"], false, "{no_symbol}");
        assert_eq!(no_symbol["errors"][0]["path"], "symbol", "{no_symbol}");
    }

    /// P3, condition 2: a name that fits more than one declaration is never
    /// answered with one confident structure.
    #[test]
    fn code_structure_answers_ambiguous_symbol() {
        let scratch = BookScratch::new("structure-ambiguous");
        scratch.write("src/one.rs", "pub fn parse() {}\n");
        scratch.write("src/two.rs", "pub fn parse() {}\n");
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();

        let response = crate::book::code_structure_with(
            &scratch.config(),
            Some(&project),
            None,
            Some("parse"),
        );
        assert_eq!(response["ok"], false, "{response}");
        assert_eq!(
            response["errors"][0]["code"],
            crate::book::AMBIGUOUS_CODE,
            "{response}"
        );
        let candidates = response["candidates"]
            .as_array()
            .expect("the candidates travel");
        assert_eq!(candidates.len(), 2, "{response}");
    }

    /// S7/D3: the impact walk follows the syntactic edges in the direction
    /// asked, with a cycle guard and a depth cap of 3 that hold even when the
    /// request asks for more.
    #[test]
    fn the_code_impact_walks_the_edges_with_its_guards() {
        let scratch = BookScratch::new("impact");
        scratch.write(
            "src/store.rs",
            "pub fn open_store() -> u8 { 1 }\npub fn build() -> u8 { open_store() }\n",
        );
        // A cycle: the walk must not spend a hop going back to where it came.
        scratch.write("src/cycle.rs", "pub fn a() { b(); }\npub fn b() { a(); }\n");
        // A chain longer than the cap, so the cap is observable.
        scratch.write(
            "src/chain.rs",
            "pub fn x() { y(); }\npub fn y() { z(); }\npub fn z() { w(); }\npub fn w() { v(); }\npub fn v() {}\n",
        );
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();

        // The default direction is the impact of a change: who points at it.
        let dependents = crate::book::code_impact_with(
            &scratch.config(),
            Some(&project),
            "open_store",
            None,
            None,
        );
        assert_eq!(dependents["ok"], true, "{dependents}");
        assert_eq!(dependents["direction"], "in", "{dependents}");
        assert_eq!(
            dependents["symbol"]["qname"], "store::open_store",
            "{dependents}"
        );
        let nodes = dependents["nodes"].as_array().expect("nodes");
        assert!(
            nodes
                .iter()
                .any(|node| node["qname"] == "store::build" && node["depth"] == 1),
            "{dependents}"
        );
        let edges = dependents["edges"].as_array().expect("edges");
        assert_eq!(edges.len(), 1, "{dependents}");
        assert_eq!(edges[0]["kind"], "calls", "{dependents}");
        assert_eq!(dependents["budget"]["max_nodes"], 300, "{dependents}");
        assert_eq!(dependents["budget"]["max_edges"], 600, "{dependents}");

        // The other direction: what the symbol reaches.
        let callees = crate::book::code_impact_with(
            &scratch.config(),
            Some(&project),
            "build",
            None,
            Some("out"),
        );
        assert_eq!(callees["ok"], true, "{callees}");
        assert!(
            callees["nodes"].as_array().is_some_and(|nodes| nodes
                .iter()
                .any(|node| node["qname"] == "store::open_store")),
            "{callees}"
        );

        // A cycle answers once per symbol instead of hanging.
        let cycle = crate::book::code_impact_with(
            &scratch.config(),
            Some(&project),
            "a",
            Some(3),
            Some("both"),
        );
        assert_eq!(cycle["ok"], true, "{cycle}");
        assert_eq!(cycle["count"], 2, "{cycle}");
        assert_eq!(cycle["depth"], 3, "{cycle}");

        // The cap: asking for more than 3 hops keeps the answer at 3.
        let capped = crate::book::code_impact_with(
            &scratch.config(),
            Some(&project),
            "x",
            Some(9),
            Some("out"),
        );
        assert_eq!(capped["ok"], true, "{capped}");
        assert_eq!(capped["depth"], 3, "the cap is 3: {capped}");
        let reached: Vec<&str> = capped["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .filter_map(|node| node["qname"].as_str())
            .collect();
        assert!(reached.contains(&"chain::z"), "{reached:?}");
        assert!(
            !reached.contains(&"chain::v"),
            "hop 4 stays out: {reached:?}"
        );

        // An unknown direction is rejected, it never silently picks one.
        let wrong = crate::book::code_impact_with(
            &scratch.config(),
            Some(&project),
            "build",
            None,
            Some("sideways"),
        );
        assert_eq!(wrong["ok"], false, "{wrong}");
        assert_eq!(wrong["errors"][0]["path"], "direction", "{wrong}");
    }

    #[test]
    fn code_impact_answers_ambiguous_symbol() {
        let scratch = BookScratch::new("impact-ambiguous");
        scratch.write("src/one.rs", "pub fn parse() {}\n");
        scratch.write("src/two.rs", "pub fn parse() {}\n");
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();
        let response =
            crate::book::code_impact_with(&scratch.config(), Some(&project), "parse", None, None);
        assert_eq!(response["ok"], false, "{response}");
        assert_eq!(
            response["errors"][0]["code"],
            crate::book::AMBIGUOUS_CODE,
            "{response}"
        );
        assert_eq!(
            response["candidates"].as_array().map(Vec::len),
            Some(2),
            "{response}"
        );
    }

    #[test]
    fn code_impact_answers_book_disabled() {
        let scratch = BookScratch::new("impact-disabled");
        let project = scratch.root().to_string_lossy().to_string();
        let config =
            gx_index::BooksConfig::from_parts(Some("off".into()), Some(scratch.base.join("books")));
        let response =
            crate::book::code_impact_with(&config, Some(&project), "anything", None, None);
        assert_eq!(response["ok"], false, "{response}");
        assert_eq!(
            response["errors"][0]["code"],
            crate::book::DISABLED_CODE,
            "{response}"
        );
        assert!(
            !scratch.base.join("books").exists(),
            "the disabled path never creates a books root"
        );
    }

    /// S5: `code_diagram` returns a document, not a paint: the `graph` document
    /// it builds passes the same validation the render runs.
    #[test]
    fn the_code_diagram_emits_a_graph_document_the_validation_accepts() {
        let scratch = BookScratch::new("diagram");
        scratch.write(
            "src/store.rs",
            "pub fn open_store() -> u8 { 1 }\npub fn build() -> u8 { open_store() }\npub fn lonely() {}\n",
        );
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();

        let response =
            crate::book::code_diagram_with(&scratch.config(), Some(&project), "open_store", None);
        assert_eq!(response["ok"], true, "{response}");
        assert_eq!(response["progress"]["state"], "indexed", "{response}");
        let document = &response["diagram"];
        assert_eq!(document["version"], 1, "{response}");
        assert_eq!(document["kind"], "graph", "{response}");
        let nodes = document["nodes"].as_array().expect("the nodes travel");
        assert_eq!(nodes.len(), 2, "{response}");
        assert!(
            nodes
                .iter()
                .any(|node| node["label"] == "store::open_store"),
            "{response}"
        );
        let edges = document["edges"].as_array().expect("the edges travel");
        assert_eq!(edges.len(), 1, "{response}");
        assert_eq!(edges[0]["label"], "calls", "{response}");

        // The document goes to `render_diagram` as it is: the validation of
        // the render accepts it with no repair.
        let validated = crate::contract::validate_diagram(&document.to_string());
        assert!(validated.ok, "{:?}", validated.errors);

        // A symbol with no relations is a one-node document, not an error.
        let lonely =
            crate::book::code_diagram_with(&scratch.config(), Some(&project), "lonely", None);
        assert_eq!(lonely["ok"], true, "{lonely}");
        assert_eq!(
            lonely["diagram"]["nodes"].as_array().map(Vec::len),
            Some(1),
            "{lonely}"
        );
        let validated = crate::contract::validate_diagram(&lonely["diagram"].to_string());
        assert!(validated.ok, "{:?}", validated.errors);
    }

    #[test]
    fn code_diagram_answers_ambiguous_symbol() {
        let scratch = BookScratch::new("diagram-ambiguous");
        scratch.write("src/one.rs", "pub fn parse() {}\n");
        scratch.write("src/two.rs", "pub fn parse() {}\n");
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();
        let response =
            crate::book::code_diagram_with(&scratch.config(), Some(&project), "parse", None);
        assert_eq!(response["ok"], false, "{response}");
        assert_eq!(
            response["errors"][0]["code"],
            crate::book::AMBIGUOUS_CODE,
            "{response}"
        );
    }

    #[test]
    fn code_diagram_answers_book_disabled() {
        let scratch = BookScratch::new("diagram-disabled");
        let project = scratch.root().to_string_lossy().to_string();
        let config =
            gx_index::BooksConfig::from_parts(Some("off".into()), Some(scratch.base.join("books")));
        let response = crate::book::code_diagram_with(&config, Some(&project), "anything", None);
        assert_eq!(response["ok"], false, "{response}");
        assert_eq!(
            response["errors"][0]["code"],
            crate::book::DISABLED_CODE,
            "{response}"
        );
    }

    /// The three tools exist once: the dispatcher of the owner publishes them
    /// with their schemas, exactly like `code_search`.
    #[test]
    fn the_dispatcher_exposes_the_book_tools() {
        for name in ["code_structure", "code_impact", "code_diagram"] {
            assert!(
                crate::dispatch::TOOL_NAMES.contains(&name),
                "`{name}` has to be in the index of the server"
            );
            let schema = crate::dispatch::tool_schema(name);
            assert!(schema["properties"].is_object(), "{name}: {schema}");
        }
        assert_eq!(
            crate::dispatch::tool_schema("code_impact")["required"][0],
            "symbol"
        );
        assert_eq!(
            crate::dispatch::tool_schema("code_diagram")["required"][0],
            "symbol"
        );
        for name in ["code_impact", "code_diagram"] {
            let error = crate::dispatch::call_tool(name, &serde_json::json!({}), "s")
                .expect_err("`symbol` is required");
            assert!(error.contains("symbol"), "{name}: {error}");
        }
    }
}
