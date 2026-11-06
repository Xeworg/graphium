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

    /// The books of the user, for the human door of `graphium books list|prune`
    /// (T4/C3): the application reaches the engine through this crate and never
    /// depends on `gx-index` directly (decision L3 of `f9-libro`).
    pub use gx_index::book::iso8601_utc;
    pub use gx_index::location::{
        BookEntry, BookRegistry, BookSummary, BooksRoot, PruneCandidate, PruneOutcome, PruneReason,
        KEEP_NEWEST_BOOKS,
    };

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

    /// The direction a chapter seed follows when the agent does not say: the
    /// file as a node answers its whole neighbourhood (T5).
    pub const DEFAULT_FILE_DIRECTION: &str = "both";

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
    pub fn code_structure(
        session_id: &str,
        file: Option<&str>,
        symbol: Option<&str>,
        request: &gx_index::StructureRequest,
    ) -> Value {
        let config = gx_index::BooksConfig::from_env();
        code_structure_query_with(
            &config,
            project_of(session_id).as_deref(),
            file,
            symbol,
            request,
        )
    }

    /// Runs `code_structure` with an explicit configuration and project (S5).
    pub fn code_structure_with(
        config: &gx_index::BooksConfig,
        project: Option<&str>,
        file: Option<&str>,
        symbol: Option<&str>,
    ) -> Value {
        code_structure_query_with(
            config,
            project,
            file,
            symbol,
            &gx_index::StructureRequest::default(),
        )
    }

    /// Runs `code_structure` with the knobs of the bounded answer (T5).
    ///
    /// The default answer is the **outline**: the declarations of the chapter
    /// whose parent is `NULL`, with the count of the whole chapter and the
    /// cursor that reads the children. `after_line` and `limit` walk the same
    /// span-ordered list by range, so reading the whole tree is a sequence of
    /// monotone pages instead of one unbounded answer; `kinds` filters the rows
    /// and never the `total`.
    pub fn code_structure_query_with(
        config: &gx_index::BooksConfig,
        project: Option<&str>,
        file: Option<&str>,
        symbol: Option<&str>,
        request: &gx_index::StructureRequest,
    ) -> Value {
        if let Some(after_line) = request.after_line {
            if after_line < 0 {
                return failure(
                    "after_line",
                    "invalid_value",
                    "`after_line` is negative",
                    Some("lines start at 1; `after_line: 0` asks for the list from its first row"),
                );
            }
        }
        if request.limit == Some(0) {
            return failure(
                "limit",
                "invalid_value",
                "`limit` is 0",
                Some("send how many rows the answer may carry; 200 by default and 200 at most"),
            );
        }
        for kind in &request.kinds {
            if !gx_index::StructureRequest::kind_is_known(kind) {
                return failure(
                    "kinds",
                    "invalid_value",
                    &format!("`{kind}` is not a declaration kind"),
                    Some(
                        "send the kinds the book declares: `fn`, `method`, `type`, `class`, `trait`, `impl`, `module`, `const`, `static`",
                    ),
                );
            }
        }

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

        let path = structure.file.clone();
        let answer = structure.answer(request);
        let mut response = base;
        response["ok"] = json!(true);
        response["file"] = json!(path);
        response["lang"] = json!(structure.lang);
        response["content_hash"] = json!(structure.content_hash);
        response["indexed_at"] = json!(gx_index::book::iso8601_utc(structure.indexed_at));
        response["count"] = json!(answer.symbols.len());
        response["total"] = json!(answer.total);
        response["symbols"] = json!(structure_symbols_json(&structure.symbols, &answer.symbols));
        if let Some(hit) = asked {
            response["symbol"] = hit_json(&hit);
        }
        if let Some(collapsed) = &answer.collapsed {
            response["collapsed"] = json!({
                "hidden_symbols": collapsed.hidden_symbols,
                "declarations_with_children": collapsed.declarations_with_children,
                "next": structure_next(&path, collapsed.after_line, None, &request.kinds),
            });
        }
        response["truncated"] = match &answer.truncated {
            Some(truncation) => {
                let next = match truncation.reason {
                    gx_index::CutReason::Collapsed => answer.collapsed.as_ref().map(|collapsed| {
                        structure_next(&path, collapsed.after_line, None, &request.kinds)
                    }),
                    gx_index::CutReason::Nodes => answer.symbols.last().map(|row| {
                        structure_next(&path, row.line, Some(answer.symbols.len()), &request.kinds)
                    }),
                    _ => None,
                };
                truncated_json(truncation, next)
            }
            None => Value::Null,
        };
        response
    }

    /// The question of one `code_impact` call (T5): exactly one seed, and the
    /// knobs of the walk.
    #[derive(Debug, Clone, Default)]
    pub struct ImpactQuestion {
        /// The symbol to start from: a name, as `code_search` takes it.
        pub symbol: Option<String>,
        /// The chapter to walk as a node: a path relative to the project root.
        pub file: Option<String>,
        /// Hops to follow: 1 by default, clamped to 3.
        pub depth: Option<u32>,
        /// `in`, `out` or `both`.
        pub direction: Option<String>,
        /// The edge kinds to follow; empty follows the whole vocabulary.
        pub edges: Option<Vec<String>>,
        /// The query budget in nodes: 600 by default, 2000 at most.
        pub budget: Option<usize>,
    }

    /// Runs `code_impact` for the session with the configuration of the owner.
    pub fn code_impact(session_id: &str, question: &ImpactQuestion) -> Value {
        let config = gx_index::BooksConfig::from_env();
        code_impact_query_with(&config, project_of(session_id).as_deref(), question)
    }

    /// Runs the symbol walk of `code_impact` with an explicit configuration and
    /// project (S7, D3), the shape C1 published.
    pub fn code_impact_with(
        config: &gx_index::BooksConfig,
        project: Option<&str>,
        symbol: &str,
        depth: Option<u32>,
        direction: Option<&str>,
    ) -> Value {
        code_impact_query_with(
            config,
            project,
            &ImpactQuestion {
                symbol: Some(symbol.to_string()),
                depth,
                direction: direction.map(str::to_string),
                ..ImpactQuestion::default()
            },
        )
    }

    /// Runs `code_impact` with the bounded answer of T5.
    ///
    /// The seed is exactly one of `symbol` or `file`. A file seed walks from
    /// every declaration of the chapter — the file as a node — and adds the
    /// grouping of the connected files. The walk follows at most 3 hops, the
    /// edge kinds of `edges`, and its own query budget (`budget` nodes, 600 by
    /// default, 2000 at most), decoupled from the 300/600 ceilings of the
    /// `graph` kind.
    pub fn code_impact_query_with(
        config: &gx_index::BooksConfig,
        project: Option<&str>,
        question: &ImpactQuestion,
    ) -> Value {
        let symbol = optional_text(question.symbol.as_deref());
        let file = optional_text(question.file.as_deref());
        match (file, symbol) {
            (Some(_), Some(_)) => {
                return failure(
                    "file",
                    "invalid_value",
                    "send one of `symbol` or `file`, not both",
                    Some("`file` walks a whole chapter; `symbol` walks the connections of one declaration"),
                );
            }
            (None, None) => {
                return failure(
                    "file",
                    "invalid_value",
                    "`symbol` or `file` is required",
                    Some("send `symbol` (a name, as `code_search` takes it) or `file` (a path relative to the project root, with `/`)"),
                );
            }
            _ => {}
        }
        let edges = match edges_argument(question.edges.as_deref()) {
            Ok(edges) => edges,
            Err(response) => return response,
        };
        if question.budget == Some(0) {
            return failure(
                "budget",
                "invalid_value",
                "`budget` is 0",
                Some("send how many nodes the answer may carry; 600 by default and 2000 at most"),
            );
        }
        // A symbol answers the impact of a change (`in`); a chapter answers its
        // whole neighbourhood, which is what a file as a node is for.
        let default_direction = if file.is_some() {
            DEFAULT_FILE_DIRECTION
        } else {
            DEFAULT_DIRECTION
        };
        let direction = match direction_argument(question.direction.as_deref(), default_direction) {
            Ok(direction) => direction,
            Err(response) => return response,
        };
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
        let depth = question.depth.unwrap_or(DEFAULT_DEPTH);
        let limits = gx_index::WalkLimits::query(question.budget);
        let mut seed_args = serde_json::Map::new();
        let mut asked = None;
        let mut seed_file = None;
        let (walked, grouped) = match (file, &symbol) {
            (Some(path), _) => {
                let request = gx_index::FileImpactRequest {
                    depth,
                    direction,
                    edges: edges.clone(),
                    limits,
                };
                let impact = match book.file_impact(path, &request) {
                    Ok(Some(impact)) => impact,
                    Ok(None) => {
                        return failure_with(
                            base,
                            "file",
                            "invalid_value",
                            "the book has no chapter there",
                            Some("paths are relative to the project root with `/` separators; `progress.state == \"indexing\"` means the first pass is still running"),
                        );
                    }
                    Err(error) => {
                        return failure_with(
                            base,
                            "book",
                            UNAVAILABLE_CODE,
                            &error.to_string(),
                            None,
                        );
                    }
                };
                seed_args.insert("file".to_string(), json!(impact.file));
                seed_file = Some(impact.file.clone());
                (impact.impact, impact.files)
            }
            (None, Some(symbol)) => {
                let hit = match one_symbol(book, base.clone(), symbol, "symbol") {
                    Ok(hit) => hit,
                    Err(answer) => return answer,
                };
                let request = gx_index::ImpactRequest::symbol(hit.id, depth, direction)
                    .with_edges(edges.clone())
                    .with_limits(limits);
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
                        return failure_with(
                            base,
                            "book",
                            UNAVAILABLE_CODE,
                            &error.to_string(),
                            None,
                        );
                    }
                };
                seed_args.insert("symbol".to_string(), json!(symbol));
                asked = Some(hit);
                (impact, Vec::new())
            }
            // The grammar above rejected zero and two selectors.
            (None, None) => unreachable!("the selector grammar runs first"),
        };

        let mut response = base;
        response["ok"] = json!(true);
        if let Some(hit) = &asked {
            response["symbol"] = hit_json(hit);
        }
        if let Some(file) = &seed_file {
            response["file"] = json!(file);
        }
        response["direction"] = json!(direction.as_str());
        response["depth"] = json!(walked.depth);
        response["budget"] = json!({
            "max_nodes": limits.max_nodes,
            "max_edges": limits.max_edges,
        });
        response["truncated"] = match &walked.truncated {
            Some(truncation) => {
                let next = match truncation.reason {
                    gx_index::CutReason::Nodes | gx_index::CutReason::Edges => {
                        // The follow-up call asks the same question with twice
                        // the budget; at the ceiling there is no next call.
                        let bigger = limits
                            .max_nodes
                            .saturating_mul(2)
                            .min(gx_index::impact::MAX_QUERY_NODES);
                        (bigger > limits.max_nodes)
                            .then(|| walk_call(&seed_args, depth, direction, &edges, Some(bigger)))
                    }
                    _ => None,
                };
                truncated_json(truncation, next)
            }
            None => Value::Null,
        };
        response["unresolved"] = json!(walked.unresolved);
        response["count"] = json!(walked.nodes.len());
        response["nodes"] = json!(nodes_json(&walked.nodes));
        response["edges"] = json!(edges_json(&walked.edges));
        if seed_file.is_some() {
            response["files"] = json!(files_json(&grouped));
        }
        response
    }

    /// Runs `code_diagram` for the session with the configuration of the owner.
    pub fn code_diagram(
        session_id: &str,
        symbol: &str,
        depth: Option<u32>,
        edges: Option<&[String]>,
    ) -> Value {
        let config = gx_index::BooksConfig::from_env();
        code_diagram_query_with(
            &config,
            project_of(session_id).as_deref(),
            symbol,
            depth,
            edges,
        )
    }

    /// Builds the `graph` document of one symbol's neighbourhood (S5).
    pub fn code_diagram_with(
        config: &gx_index::BooksConfig,
        project: Option<&str>,
        symbol: &str,
        depth: Option<u32>,
    ) -> Value {
        code_diagram_query_with(config, project, symbol, depth, None)
    }

    /// Builds the `graph` document of one symbol's neighbourhood (S5, T5).
    ///
    /// The answer is a **document**, not a paint: the agent sends `diagram` to
    /// `render_diagram` (or to `validate_diagram` first) unchanged. The
    /// document never leaves with a density the kind would reject, so it is
    /// renderable as it arrives. `edges` chooses which kinds the walk follows;
    /// the walk keeps the 300/600 ceilings of the kind, because a bigger
    /// neighbourhood is a `code_impact` question, not a document.
    pub fn code_diagram_query_with(
        config: &gx_index::BooksConfig,
        project: Option<&str>,
        symbol: &str,
        depth: Option<u32>,
        edges: Option<&[String]>,
    ) -> Value {
        let symbol = match text_argument(symbol, "symbol") {
            Ok(symbol) => symbol,
            Err(response) => return response,
        };
        let edges = match edges_argument(edges) {
            Ok(edges) => edges,
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
        let depth = depth.unwrap_or(DEFAULT_DEPTH);
        let direction = gx_index::Direction::Both;
        let limits = gx_index::WalkLimits::GRAPH;
        let request = gx_index::ImpactRequest::symbol(hit.id, depth, direction)
            .with_edges(edges.clone())
            .with_limits(limits);
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
        let document_edges: Vec<Value> = impact
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
        let dropped = impact.edges.len().saturating_sub(document_edges.len());
        let document = json!({
            "version": 1,
            "kind": "graph",
            "title": format!("{} — relations", hit.qname),
            "nodes": nodes,
            "edges": document_edges,
        });

        // The follow-up of a cut diagram is the connections view: the same
        // walk, with its own query budget and without the document ceiling.
        let mut seed_args = serde_json::Map::new();
        seed_args.insert("symbol".to_string(), json!(symbol));
        let next = walk_call(
            &seed_args,
            depth,
            direction,
            &edges,
            Some(gx_index::impact::DEFAULT_QUERY_NODES),
        );
        let truncated = match &impact.truncated {
            // The walk was cut, so the document is a slice of a slice: the
            // walk's own reason wins over the density of what reached it, and
            // `shown` counts what the document carries (the density trim may
            // have taken edges away after the walk stopped).
            Some(truncation) => {
                let shown = match truncation.reason {
                    gx_index::CutReason::Edges => document["edges"]
                        .as_array()
                        .map(Vec::len)
                        .unwrap_or_default(),
                    _ => document["nodes"]
                        .as_array()
                        .map(Vec::len)
                        .unwrap_or_default(),
                };
                Some(truncated_json(
                    &gx_index::Truncation::new(truncation.reason, shown, truncation.total),
                    Some(next),
                ))
            }
            None if dropped > 0 => Some(truncated_json(
                &gx_index::Truncation::new(
                    gx_index::CutReason::Density,
                    document["edges"]
                        .as_array()
                        .map(Vec::len)
                        .unwrap_or_default(),
                    Some(impact.edges.len()),
                ),
                Some(next),
            )),
            None => None,
        };

        let mut response = base;
        response["ok"] = json!(true);
        response["symbol"] = hit_json(&hit);
        response["direction"] = json!(direction.as_str());
        response["depth"] = json!(impact.depth);
        response["budget"] = json!({
            "max_nodes": limits.max_nodes,
            "max_edges": limits.max_edges,
        });
        response["truncated"] = truncated.unwrap_or(Value::Null);
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

    /// A selector or an option of the tool: trimmed, and empty is absent.
    fn optional_text(raw: Option<&str>) -> Option<&str> {
        raw.map(str::trim).filter(|text| !text.is_empty())
    }

    /// The `edges` filter of a walk: the four kinds of the schema (T5).
    ///
    /// Absent follows the whole vocabulary. An empty list is a mistake the
    /// agent would read as «no connections», so it is rejected instead of
    /// silently following everything.
    fn edges_argument(raw: Option<&[String]>) -> Result<Vec<String>, Value> {
        let Some(raw) = raw else {
            return Ok(Vec::new());
        };
        if raw.is_empty() {
            return Err(failure(
                "edges",
                "invalid_value",
                "`edges` is empty",
                Some("send at least one of `calls`, `imports`, `inherits`, `uses`, or omit it to follow the four"),
            ));
        }
        for kind in raw {
            if !gx_index::RELATION_KINDS.contains(&kind.as_str()) {
                return Err(failure(
                    "edges",
                    "invalid_value",
                    &format!("`{kind}` is not an edge kind"),
                    Some("send `calls`, `imports`, `inherits` or `uses`"),
                ));
            }
        }
        Ok(raw.to_vec())
    }

    /// The copy-pasteable call that reads the rest of a chapter (T5).
    fn structure_next(
        file: &str,
        after_line: u32,
        limit: Option<usize>,
        kinds: &[String],
    ) -> Value {
        let mut args = serde_json::Map::new();
        args.insert("file".to_string(), json!(file));
        args.insert("after_line".to_string(), json!(after_line));
        if let Some(limit) = limit {
            args.insert("limit".to_string(), json!(limit));
        }
        if !kinds.is_empty() {
            args.insert("kinds".to_string(), json!(kinds));
        }
        json!({ "tool": "code_structure", "args": Value::Object(args) })
    }

    /// The copy-pasteable call that continues one walk (T5): the same seed and
    /// the same question, with a budget the `graph` kind would never allow.
    fn walk_call(
        seed: &serde_json::Map<String, Value>,
        depth: u32,
        direction: gx_index::Direction,
        edges: &[String],
        budget: Option<usize>,
    ) -> Value {
        let mut args = seed.clone();
        args.insert("depth".to_string(), json!(depth));
        args.insert("direction".to_string(), json!(direction.as_str()));
        if !edges.is_empty() {
            args.insert("edges".to_string(), json!(edges));
        }
        if let Some(budget) = budget {
            args.insert("budget".to_string(), json!(budget));
        }
        json!({ "tool": "code_impact", "args": Value::Object(args) })
    }

    /// The machine-readable notice of a cut (T5): the reason, the numbers and
    /// the exact follow-up call.
    fn truncated_json(truncation: &gx_index::Truncation, next: Option<Value>) -> Value {
        json!({
            "reason": truncation.reason.as_str(),
            "shown": truncation.shown,
            "total": truncation.total,
            "next": next,
        })
    }

    /// The files connected to a chapter seed (T5), as the agent reads them.
    fn files_json(files: &[gx_index::FileLink]) -> Vec<Value> {
        files
            .iter()
            .map(|link| {
                json!({
                    "file": link.file,
                    "imports_out": link.imports_out,
                    "referenced_by_in": link.referenced_by_in,
                    "symbols": link.symbols,
                })
            })
            .collect()
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
    fn direction_argument(raw: Option<&str>, fallback: &str) -> Result<gx_index::Direction, Value> {
        let text = raw
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .unwrap_or(fallback);
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
    ///
    /// `all` is the whole chapter and `shown` the rows of this answer, because
    /// an outline hides the parent of a child it does show.
    fn structure_symbols_json(
        all: &[gx_index::StructureSymbol],
        shown: &[gx_index::StructureSymbol],
    ) -> Vec<Value> {
        let qnames: std::collections::BTreeMap<i64, &str> = all
            .iter()
            .map(|symbol| (symbol.id, symbol.qname.as_str()))
            .collect();
        shown
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
const CODE_STRUCTURE_DESCRIPTION: &str =
    "Read the structure of one chapter of the book. The default answer is the \
outline: only the declarations of the chapter whose parent is `NULL`, ordered \
by span, each with `name`, `kind`, `qname`, `line`, `column`, `end_line`, \
`end_column` and `parent` (the qname of the declaration that contains it). The \
answer carries `count` and `total` (every declaration of the chapter, filter \
included) and, when children were hidden, \
`collapsed: {hidden_symbols, declarations_with_children, next}`, where `next` \
is the copy-pasteable call that reads them. To read the rest of the tree, or a \
slice of it, send `after_line` (start strictly after that line) and `limit` \
(1..=200): they are a range over the same span-ordered list, never a mode. \
`kinds` filters the rows by declaration kind (`fn`, `method`, `type`, `class`, \
`trait`, `impl`, `module`, `const`, `static`) and never changes `total`. Ask \
with `file` (a path relative to the project root, with `/` separators) or with \
`symbol` (a name, as `code_search` takes it): exactly one of the two, and a \
symbol that fits more than one declaration answers `ok: false` with code \
`ambiguous_symbol` and `candidates[]`. When a cut happens, `truncated` says \
`{reason, shown, total, next}` with reason `collapsed` (the outline) or \
`nodes` (the limit). The answer carries the `content_hash` and `indexed_at` of \
the chapter plus the `book_key` of the book that answered. \
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
    /// T5: start strictly after this line. With `limit` it is a range over
    /// the span-ordered list; without either, the answer is the outline.
    #[serde(default)]
    pub after_line: Option<i64>,
    /// T5: how many rows the answer carries (default 200, at most 200).
    #[serde(default)]
    pub limit: Option<usize>,
    /// T5: the declaration kinds to answer; empty answers all of them.
    #[serde(default)]
    pub kinds: Option<Vec<String>>,
}

/// Description of the `code_impact` tool, written for the agent (S5, S7, D3).
const CODE_IMPACT_DESCRIPTION: &str =
    "Walk the connections of one symbol, or of one whole chapter, in the book: who \
points at it, what it reaches, or both. The seed is exactly one of `symbol` (a \
name, as `code_search` takes it) or `file` (a path relative to the project \
root): a file seed is the connections view — every declaration of the chapter \
starts the walk, the answer adds `file` and a `files` grouping of the \
connected files (`imports_out`, `referenced_by_in`, `symbols`; the seed file is \
never a row of `files`), and its \
default `direction` is `both`. `direction` is `in` (the default of a symbol \
seed: the edges that point at it, so what a change would affect), `out` (what \
the symbol calls, imports or uses) or `both`; `edges` narrows which kinds the \
walk follows (`calls`, `imports`, `inherits`, `uses`). The edges are \
**syntactic, not typed** (a call by name, an import by its bound segment, an \
`impl Trait for Type`, a type reference), so a common name may be a false \
positive and `unresolved` counts the targets the book could not pin to one \
declaration. The walk has three guards and the answer states them: `depth` is \
clamped to 3 hops, a cycle is answered once per symbol and never followed \
twice, and the answer is bounded by `budget` nodes (600 by default, 2000 at \
most, with the edges at twice the nodes): a cut is `truncated: {reason, \
shown, total, next}`, where `next` is the copy-pasteable call with the bigger \
budget, or `null` when the 3-hop cap is the reason and no follow-up exists. \
Every node carries `file`, `line` and the hop it was reached at; the seed \
travels with `content_hash` and `indexed_at`, plus the `book_key` of the book. \
A name that fits more than one declaration answers `ok: false` with \
`ambiguous_symbol` and `candidates[]`; the book is on by default, so \
`book_disabled` only comes from an explicit `GRAPHIUM_BOOK=off`. Read-only: it \
never touches the plan.";

/// Parameters of the `code_impact` tool (F9, C1).
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CodeImpactParams {
    /// The symbol to start from: a name, as `code_search` takes it. Exactly
    /// one of `symbol` or `file`.
    #[serde(default)]
    pub symbol: Option<String>,
    /// T5: the chapter to walk as a node (the connections view). Exactly one
    /// of `symbol` or `file`.
    #[serde(default)]
    pub file: Option<String>,
    /// Hops to follow: 1 by default, clamped to 3.
    #[serde(default)]
    pub depth: Option<u32>,
    /// `in` (default for a symbol), `both` (default for a file) or `out`.
    #[serde(default)]
    pub direction: Option<String>,
    /// T5: the edge kinds to follow (`calls`, `imports`, `inherits`,
    /// `uses`); empty follows the whole vocabulary.
    #[serde(default)]
    pub edges: Option<Vec<String>>,
    /// T5: the query budget in nodes (default 600, at most 2000).
    #[serde(default)]
    pub budget: Option<usize>,
}

/// Description of the `code_diagram` tool, written for the agent (S5).
const CODE_DIAGRAM_DESCRIPTION: &str =
    "Build the diagram of one symbol's neighbourhood from the book, without reading \
any file. The answer is a complete `graph` document (`version: 1`, `kind: \
\"graph\"`, one node per symbol with its `qname` as the label, one edge per \
connection labelled with its kind) in `diagram`: send it to `render_diagram` \
as it is, or to `validate_diagram` first. The document respects the contract of \
the kind (at most 300 nodes, 600 edges and a density of 3), so it is \
renderable with no repair. The walk is the same as `code_impact` with \
`direction: both` and `depth` clamped to 3 hops; `edges` narrows which kinds \
it follows. `truncated` is the object `{reason, shown, total, next}`: \
`density` means the document was trimmed to render, `nodes`/`edges` mean the \
walk was cut before the document existed (the answer carries the 300/600 \
ceilings of the kind in `budget`), and `next` is the `code_impact` call that \
answers the same question with its own query budget. The edges are syntactic, \
not typed (D3): the picture is the book's reading of the code, not a \
compiler's. The start symbol travels with `content_hash`, `indexed_at` and the \
`book_key` of the book; an ambiguous name answers `ok: false` with \
`ambiguous_symbol` and `candidates[]`, and the book is on by default \
(`book_disabled` only with an explicit `GRAPHIUM_BOOK=off`). It never renders \
and never touches the plan: the diagram is a document.";

/// Parameters of the `code_diagram` tool (F9, C1).
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CodeDiagramParams {
    /// The symbol to draw the neighbourhood of: a name, as `code_search` takes it.
    pub symbol: String,
    /// Hops to follow: 1 by default, clamped to 3.
    #[serde(default)]
    pub depth: Option<u32>,
    /// T5: the edge kinds to follow (`calls`, `imports`, `inherits`,
    /// `uses`); empty follows the whole vocabulary.
    #[serde(default)]
    pub edges: Option<Vec<String>>,
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
        let request = gx_index::StructureRequest {
            after_line: params.after_line,
            limit: params.limit,
            kinds: params.kinds.unwrap_or_default(),
        };
        let response = tokio::task::spawn_blocking(move || {
            book::code_structure(
                &session_id,
                params.file.as_deref(),
                params.symbol.as_deref(),
                &request,
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
        let question = book::ImpactQuestion {
            symbol: params.symbol,
            file: params.file,
            depth: params.depth,
            direction: params.direction,
            edges: params.edges,
            budget: params.budget,
        };
        let response =
            tokio::task::spawn_blocking(move || book::code_impact(&session_id, &question))
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
            book::code_diagram(
                &session_id,
                &params.symbol,
                params.depth,
                params.edges.as_deref(),
            )
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

        /// Opens the book and waits until it is **settled**: the test's own
        /// pass and the watcher's first pass have both finished and no pass is
        /// writing.
        ///
        /// The watcher runs exactly one full pass on its own thread, and a
        /// synchronous pass here does not cancel it. Polling the `progress` row
        /// alone was a race — it can say `indexed` and be overwritten to
        /// `indexing` a millisecond later by the watcher's pass, which is the
        /// flake of T4/C3 (three tests at once). Two completed passes plus no
        /// pass in flight is the deterministic barrier: after it the row cannot
        /// change again.
        fn indexed(&self) -> std::sync::Arc<gx_index::service::BookHandle> {
            let handle = gx_index::service::ensure(&self.config(), &self.root()).expect("the book");
            handle.indexer().run_full().expect("the first index");
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while handle.indexer().passes_done() < 2 || handle.indexer().is_indexing() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "the scratch book never settled its passes: passes_done={}, is_indexing={}",
                    handle.indexer().passes_done(),
                    handle.indexer().is_indexing()
                );
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            handle
        }
    }

    impl Drop for BookScratch {
        fn drop(&mut self) {
            // Only the book of this scratch: `shutdown_all` here tore down the
            // books of the tests that were still answering, and their next tool
            // call reopened one whose first pass re-published `progress` (the
            // flake of T4/C3).
            gx_index::service::shutdown(&self.root());
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

    /// S3/D3 (T3/C2): las dos tools contestan un árbol con un capítulo por
    /// lenguaje — el lenguaje real de cada capítulo y una llamada que cruza de
    /// Python/Go/Java a la declaración de TypeScript, resuelta por nombre.
    #[test]
    fn the_tools_answer_a_tree_with_one_chapter_per_language() {
        let scratch = BookScratch::new("languages");
        scratch.write(
            "src/shared.ts",
            "export class Store {}\nexport function openStore(): Store {\n  return new Store();\n}\n",
        );
        scratch.write(
            "src/caller.py",
            "from shared import openStore\n\n\ndef build():\n    return openStore()\n",
        );
        scratch.write(
            "src/tool.go",
            "package tool\n\nfunc Use() int {\n\treturn openStore()\n}\n",
        );
        scratch.write(
            "src/App.java",
            "package com.graphium.app;\n\npublic class App {\n    public int run() {\n        return openStore();\n    }\n}\n",
        );
        let handle = scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();

        // `code_structure`: cada capítulo declara su lenguaje.
        for (file, lang) in [
            ("src/shared.ts", "typescript"),
            ("src/caller.py", "python"),
            ("src/tool.go", "go"),
            ("src/App.java", "java"),
        ] {
            let response = crate::book::code_structure_with(
                &scratch.config(),
                Some(&project),
                Some(file),
                None,
            );
            assert_eq!(response["ok"], true, "{response}");
            assert_eq!(response["lang"], lang, "the language of `{file}`");
            assert!(
                response["count"].as_u64().is_some_and(|count| count >= 1),
                "`{file}` declares something: {response}"
            );
        }

        // `code_impact`: los llamadores de los otros lenguajes resuelven.
        let response = crate::book::code_impact_with(
            &scratch.config(),
            Some(&project),
            "openStore",
            Some(1),
            Some("in"),
        );
        assert_eq!(response["ok"], true, "{response}");
        assert_eq!(response["book_key"], handle.book().key(), "{response}");
        let nodes = response["nodes"].as_array().expect("the nodes");
        let callers: Vec<&str> = nodes
            .iter()
            .filter(|node| node["depth"] == 1)
            .filter_map(|node| node["file"].as_str())
            .collect();
        for expected in ["src/caller.py", "src/tool.go", "src/App.java"] {
            assert!(
                callers.contains(&expected),
                "`{expected}` must be reached: {response}"
            );
        }
        assert_eq!(response["unresolved"], 0, "{response}");
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
        // T5: the query budget of `code_impact` is its own, decoupled from
        // the 300/600 ceilings of the `graph` kind (L7).
        assert_eq!(dependents["budget"]["max_nodes"], 600, "{dependents}");
        assert_eq!(dependents["budget"]["max_edges"], 1200, "{dependents}");

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

    // -----------------------------------------------------------------
    // F9, T5 — la respuesta acotada (S7, L7: outline, semilla de archivo)
    // -----------------------------------------------------------------

    /// A chapter with nested declarations: two top-level items contain
    /// others, so the outline hides something.
    ///
    /// ```text
    /// 1  pub struct Config;
    /// 2  impl Config {
    /// 3      pub fn load(&self) {}
    /// 4      pub fn save(&self) {}
    /// 5  }
    /// 6  pub fn build() {}
    /// 7  mod inner {
    /// 8      pub fn hidden() {}
    /// 9  }
    /// ```
    const NESTED: &str = "\
pub struct Config;\n\
impl Config {\n\
    pub fn load(&self) {}\n\
    pub fn save(&self) {}\n\
}\n\
pub fn build() {}\n\
mod inner {\n\
    pub fn hidden() {}\n\
}\n";

    /// A complete digraph: each declaration calls the other four.
    const CLIQUE: &str = "\
pub fn a() { b(); c(); d(); }\n\
pub fn b() { a(); c(); d(); }\n\
pub fn c() { a(); b(); d(); }\n\
pub fn d() { a(); b(); c(); }\n";

    /// Five declarations that all call each other: twenty edges over five
    /// nodes, so the density ceiling of the `graph` kind bites.
    const DENSE: &str = "\
pub fn a() { b(); c(); d(); e(); }\n\
pub fn b() { a(); c(); d(); e(); }\n\
pub fn c() { a(); b(); d(); e(); }\n\
pub fn d() { a(); b(); c(); e(); }\n\
pub fn e() { a(); b(); c(); d(); }\n";

    /// The chapters of the file-seed tests: `api.rs` imports and calls into
    /// `store.rs`, and `store.rs` calls back into `api.rs`.
    fn write_file_seed_chapters(scratch: &BookScratch) {
        scratch.write(
            "src/store.rs",
            "pub fn open_store() -> u8 { 1 }\npub fn check() -> u8 { handler(); 0 }\n",
        );
        scratch.write(
            "src/api.rs",
            "use crate::store::open_store;\npub fn build() -> u8 { open_store() }\npub fn handler() -> u8 { 1 }\n",
        );
    }

    /// T5/S7: `code_structure` answers the OUTLINE by default — only the
    /// declarations whose parent is `NULL` — with `total` and a `collapsed`
    /// block that carries the exact call that reads the hidden children.
    #[test]
    fn the_code_structure_answer_is_the_outline_by_default() {
        let scratch = BookScratch::new("t5-outline");
        scratch.write("src/nested.rs", NESTED);
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();

        let response = crate::book::code_structure_with(
            &scratch.config(),
            Some(&project),
            Some("src/nested.rs"),
            None,
        );
        assert_eq!(response["ok"], true, "{response}");
        assert_eq!(response["total"], 7, "{response}");
        assert_eq!(response["count"], 4, "{response}");
        let symbols = response["symbols"].as_array().expect("the rows travel");
        let names: Vec<&str> = symbols
            .iter()
            .filter_map(|row| row["name"].as_str())
            .collect();
        assert_eq!(names, ["Config", "Config", "build", "inner"], "{response}");
        let kinds: Vec<&str> = symbols
            .iter()
            .filter_map(|row| row["kind"].as_str())
            .collect();
        assert_eq!(kinds, ["type", "impl", "fn", "module"], "{response}");
        for row in symbols {
            assert!(
                row["parent"].is_null(),
                "the outline keeps the top level: {response}"
            );
        }

        let collapsed = &response["collapsed"];
        assert_eq!(collapsed["hidden_symbols"], 3, "{response}");
        assert_eq!(collapsed["declarations_with_children"], 2, "{response}");
        assert_eq!(collapsed["next"]["tool"], "code_structure", "{response}");
        assert_eq!(
            collapsed["next"]["args"]["file"], "src/nested.rs",
            "{response}"
        );
        assert_eq!(
            collapsed["next"]["args"]["after_line"], 3,
            "the first hidden child's line: {response}"
        );

        let truncated = &response["truncated"];
        assert_eq!(truncated["reason"], "collapsed", "{response}");
        assert_eq!(truncated["shown"], 4, "{response}");
        assert_eq!(truncated["total"], 7, "{response}");
        assert_eq!(truncated["next"], collapsed["next"], "{response}");
    }

    /// T5/S7: `after_line`/`limit` slice the span-ordered list monotonically:
    /// the pages concatenated are the whole list, once, in order.
    #[test]
    fn after_line_and_limit_slice_the_span_ordered_rows() {
        let scratch = BookScratch::new("t5-slice");
        scratch.write("src/nested.rs", NESTED);
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();

        let query = |after_line: Option<i64>, limit: Option<usize>| {
            crate::book::code_structure_query_with(
                &scratch.config(),
                Some(&project),
                Some("src/nested.rs"),
                None,
                &gx_index::StructureRequest {
                    after_line,
                    limit,
                    kinds: Vec::new(),
                },
            )
        };

        let mut seen: Vec<i64> = Vec::new();
        let mut after_line = Some(0);
        let mut pages = 0;
        loop {
            let page = query(after_line, Some(3));
            assert_eq!(page["ok"], true, "{page}");
            assert_eq!(page["total"], 7, "{page}");
            assert!(
                page["collapsed"].is_null(),
                "a range is not an outline: {page}"
            );
            let rows = page["symbols"].as_array().expect("the rows travel");
            seen.extend(rows.iter().filter_map(|row| row["line"].as_i64()));
            pages += 1;
            if page["truncated"].is_null() {
                break;
            }
            assert_eq!(page["truncated"]["reason"], "nodes", "{page}");
            assert_eq!(page["truncated"]["shown"], 3, "{page}");
            assert_eq!(page["truncated"]["total"], 7, "{page}");
            assert_eq!(
                page["truncated"]["next"]["tool"], "code_structure",
                "{page}"
            );
            assert_eq!(
                page["truncated"]["next"]["args"]["file"], "src/nested.rs",
                "{page}"
            );
            assert_eq!(
                page["truncated"]["next"]["args"]["after_line"],
                seen.last().copied().unwrap_or_default(),
                "the cursor is the last shown line: {page}"
            );
            assert_eq!(page["truncated"]["next"]["args"]["limit"], 3, "{page}");
            after_line = seen.last().copied();
        }
        assert_eq!(
            seen,
            vec![1, 2, 3, 4, 6, 7, 8],
            "the slices are the span-ordered list"
        );
        assert_eq!(pages, 3, "{seen:?}");
    }

    /// T5: `kinds` filters the rows and `total` keeps counting the chapter, so
    /// a filter is never read as a smaller file.
    #[test]
    fn the_kinds_filter_narrows_the_rows_and_total_stays_the_chapter() {
        let scratch = BookScratch::new("t5-kinds");
        scratch.write("src/nested.rs", NESTED);
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();

        let methods = crate::book::code_structure_query_with(
            &scratch.config(),
            Some(&project),
            Some("src/nested.rs"),
            None,
            &gx_index::StructureRequest {
                after_line: Some(0),
                limit: None,
                kinds: vec!["method".to_string()],
            },
        );
        assert_eq!(methods["ok"], true, "{methods}");
        assert_eq!(methods["total"], 7, "{methods}");
        assert_eq!(methods["count"], 2, "{methods}");
        let kinds: Vec<&str> = methods["symbols"]
            .as_array()
            .expect("the rows travel")
            .iter()
            .filter_map(|row| row["kind"].as_str())
            .collect();
        assert_eq!(kinds, ["method", "method"], "{methods}");

        let types = crate::book::code_structure_query_with(
            &scratch.config(),
            Some(&project),
            Some("src/nested.rs"),
            None,
            &gx_index::StructureRequest {
                kinds: vec!["type".to_string()],
                ..gx_index::StructureRequest::default()
            },
        );
        assert_eq!(types["total"], 7, "{types}");
        assert_eq!(types["count"], 1, "{types}");
        assert!(types["collapsed"].is_null(), "{types}");

        let unknown = crate::book::code_structure_query_with(
            &scratch.config(),
            Some(&project),
            Some("src/nested.rs"),
            None,
            &gx_index::StructureRequest {
                kinds: vec!["whatever".to_string()],
                ..gx_index::StructureRequest::default()
            },
        );
        assert_eq!(unknown["ok"], false, "{unknown}");
        assert_eq!(unknown["errors"][0]["code"], "invalid_value", "{unknown}");
        assert_eq!(unknown["errors"][0]["path"], "kinds", "{unknown}");
        assert!(
            unknown["errors"][0]["hint"]
                .as_str()
                .is_some_and(|hint| hint.contains("method")),
            "the hint lists the vocabulary: {unknown}"
        );
    }

    /// T5/S5: a file seed turns a chapter into a node: every declaration of it
    /// is the frontier, the answer keeps the impact shape and adds the file
    /// and the grouping of the connected files.
    #[test]
    fn the_file_seed_walks_the_chapter_and_groups_by_file() {
        let scratch = BookScratch::new("t5-file-seed");
        write_file_seed_chapters(&scratch);
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();

        let response = crate::book::code_impact_query_with(
            &scratch.config(),
            Some(&project),
            &crate::book::ImpactQuestion {
                file: Some("src/api.rs".to_string()),
                ..crate::book::ImpactQuestion::default()
            },
        );
        assert_eq!(response["ok"], true, "{response}");
        assert_eq!(
            response["direction"], "both",
            "a chapter answers its neighbourhood: {response}"
        );
        assert_eq!(response["file"], "src/api.rs", "{response}");
        assert!(
            response["symbol"].is_null(),
            "a file seed has no `symbol`: {response}"
        );
        let nodes = response["nodes"].as_array().expect("nodes");
        assert_eq!(
            nodes.iter().filter(|node| node["depth"] == 0).count(),
            3,
            "every declaration of the chapter starts the walk: {response}"
        );
        assert!(
            nodes
                .iter()
                .any(|node| node["qname"] == "store::open_store" && node["depth"] == 1),
            "{response}"
        );
        let files = response["files"].as_array().expect("the grouping travels");
        assert_eq!(files.len(), 1, "{response}");
        assert_eq!(files[0]["file"], "src/store.rs", "{response}");
        assert_eq!(
            files[0]["imports_out"], 2,
            "the import and the call: {response}"
        );
        assert_eq!(files[0]["referenced_by_in"], 1, "the call back: {response}");
        assert_eq!(
            files[0]["symbols"], 2,
            "the declarations of the connected file: {response}"
        );
        assert_eq!(response["unresolved"], 0, "{response}");

        // The symbol seed keeps the shape it had: no `file`, no `files`.
        let symbol = crate::book::code_impact_query_with(
            &scratch.config(),
            Some(&project),
            &crate::book::ImpactQuestion {
                symbol: Some("build".to_string()),
                direction: Some("out".to_string()),
                ..crate::book::ImpactQuestion::default()
            },
        );
        assert_eq!(symbol["ok"], true, "{symbol}");
        assert!(symbol["file"].is_null(), "{symbol}");
        assert!(symbol["files"].is_null(), "{symbol}");
        assert_eq!(symbol["symbol"]["qname"], "api::build", "{symbol}");
    }

    /// T5: a seed is exactly one of `symbol` or `file`, with the same grammar
    /// as `code_structure`: zero or two selectors is `invalid_value`.
    #[test]
    fn a_file_seed_takes_exactly_one_selector() {
        let scratch = BookScratch::new("t5-seed-grammar");
        write_file_seed_chapters(&scratch);
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();
        let ask = |question: crate::book::ImpactQuestion| {
            crate::book::code_impact_query_with(&scratch.config(), Some(&project), &question)
        };

        let none = ask(crate::book::ImpactQuestion::default());
        assert_eq!(none["ok"], false, "{none}");
        assert_eq!(none["errors"][0]["code"], "invalid_value", "{none}");
        assert_eq!(none["errors"][0]["path"], "file", "{none}");

        let both = ask(crate::book::ImpactQuestion {
            symbol: Some("build".to_string()),
            file: Some("src/api.rs".to_string()),
            ..crate::book::ImpactQuestion::default()
        });
        assert_eq!(both["ok"], false, "{both}");
        assert_eq!(both["errors"][0]["code"], "invalid_value", "{both}");
        assert_eq!(both["errors"][0]["path"], "file", "{both}");

        let unknown = ask(crate::book::ImpactQuestion {
            file: Some("src/nope.rs".to_string()),
            ..crate::book::ImpactQuestion::default()
        });
        assert_eq!(unknown["ok"], false, "{unknown}");
        assert_eq!(unknown["errors"][0]["code"], "invalid_value", "{unknown}");
        assert_eq!(unknown["errors"][0]["path"], "file", "{unknown}");
        assert!(unknown["errors"][0]["hint"].is_string(), "{unknown}");
    }

    /// T5: `edges` chooses which kinds the walk follows, on `code_impact` and
    /// on `code_diagram`; a kind the book does not have is rejected with the
    /// vocabulary in the hint.
    #[test]
    fn the_edges_filter_narrows_the_kinds_of_both_walks() {
        let scratch = BookScratch::new("t5-edges");
        write_file_seed_chapters(&scratch);
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();
        let ask = |edges: Option<&[&str]>| {
            crate::book::code_impact_query_with(
                &scratch.config(),
                Some(&project),
                &crate::book::ImpactQuestion {
                    symbol: Some("open_store".to_string()),
                    direction: Some("both".to_string()),
                    edges: edges.map(|edges| edges.iter().map(|edge| edge.to_string()).collect()),
                    ..crate::book::ImpactQuestion::default()
                },
            )
        };

        let all = ask(None);
        assert_eq!(all["ok"], true, "{all}");
        let kinds: Vec<&str> = all["edges"]
            .as_array()
            .expect("edges")
            .iter()
            .filter_map(|edge| edge["kind"].as_str())
            .collect();
        assert!(kinds.contains(&"imports"), "{all}");
        assert!(kinds.contains(&"calls"), "{all}");

        let imports = ask(Some(&["imports"]));
        let kinds: Vec<&str> = imports["edges"]
            .as_array()
            .expect("edges")
            .iter()
            .filter_map(|edge| edge["kind"].as_str())
            .collect();
        assert_eq!(kinds, ["imports"], "{imports}");

        let calls = ask(Some(&["calls"]));
        let kinds: Vec<&str> = calls["edges"]
            .as_array()
            .expect("edges")
            .iter()
            .filter_map(|edge| edge["kind"].as_str())
            .collect();
        assert!(kinds.iter().all(|kind| *kind == "calls"), "{calls}");

        // The diagram follows the same filter.
        let diagram = crate::book::code_diagram_query_with(
            &scratch.config(),
            Some(&project),
            "open_store",
            None,
            Some(&["imports".to_string()]),
        );
        assert_eq!(diagram["ok"], true, "{diagram}");
        let labels: Vec<&str> = diagram["diagram"]["edges"]
            .as_array()
            .expect("the document edges")
            .iter()
            .filter_map(|edge| edge["label"].as_str())
            .collect();
        assert_eq!(labels, ["imports"], "{diagram}");

        let wrong = ask(Some(&["sideways"]));
        assert_eq!(wrong["ok"], false, "{wrong}");
        assert_eq!(wrong["errors"][0]["code"], "invalid_value", "{wrong}");
        assert_eq!(wrong["errors"][0]["path"], "edges", "{wrong}");
        assert!(
            wrong["errors"][0]["hint"]
                .as_str()
                .is_some_and(|hint| hint.contains("inherits")),
            "the hint lists the four kinds: {wrong}"
        );

        // An empty list would read as «no connections»: it is rejected.
        let empty = ask(Some(&[]));
        assert_eq!(empty["ok"], false, "{empty}");
        assert_eq!(empty["errors"][0]["path"], "edges", "{empty}");
    }

    /// T5/S7: the query budget of `code_impact` is 600 nodes by default and
    /// 2000 at most, decoupled from the 300/600 of the `graph` kind, which
    /// `code_diagram` keeps because the document has to render.
    #[test]
    fn the_query_budget_is_decoupled_from_the_diagram_ceilings() {
        let scratch = BookScratch::new("t5-budget");
        write_file_seed_chapters(&scratch);
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();
        let ask = |budget: Option<usize>| {
            crate::book::code_impact_query_with(
                &scratch.config(),
                Some(&project),
                &crate::book::ImpactQuestion {
                    symbol: Some("build".to_string()),
                    budget,
                    ..crate::book::ImpactQuestion::default()
                },
            )
        };

        let default = ask(None);
        assert_eq!(default["budget"]["max_nodes"], 600, "{default}");
        assert_eq!(default["budget"]["max_edges"], 1200, "{default}");

        let small = ask(Some(10));
        assert_eq!(small["budget"]["max_nodes"], 10, "{small}");
        assert_eq!(small["budget"]["max_edges"], 20, "{small}");

        let huge = ask(Some(5000));
        assert_eq!(
            huge["budget"]["max_nodes"], 2000,
            "a query budget never leaves the ceiling: {huge}"
        );
        assert_eq!(huge["budget"]["max_edges"], 4000, "{huge}");

        let zero = ask(Some(0));
        assert_eq!(zero["ok"], false, "{zero}");
        assert_eq!(zero["errors"][0]["code"], "invalid_value", "{zero}");
        assert_eq!(zero["errors"][0]["path"], "budget", "{zero}");

        let diagram =
            crate::book::code_diagram_with(&scratch.config(), Some(&project), "build", None);
        assert_eq!(diagram["budget"]["max_nodes"], 300, "{diagram}");
        assert_eq!(diagram["budget"]["max_edges"], 600, "{diagram}");
    }

    /// T5/S7: every cut is an object with the reason, the numbers and the
    /// exact call that continues the answer (the Graphify «showing 67 of 182»
    /// pattern, machine readable).
    #[test]
    fn the_walk_cut_names_the_reason_the_numbers_and_the_next_call() {
        let scratch = BookScratch::new("t5-cut");
        scratch.write(
            "src/store.rs",
            "pub fn hub() {}\npub fn c1() { hub(); }\npub fn c2() { hub(); }\npub fn c3() { hub(); }\npub fn c4() { hub(); }\npub fn c5() { hub(); }\n",
        );
        scratch.write("src/clique.rs", CLIQUE);
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();
        let ask = |symbol: &str, depth: Option<u32>, direction: &str, budget: Option<usize>| {
            crate::book::code_impact_query_with(
                &scratch.config(),
                Some(&project),
                &crate::book::ImpactQuestion {
                    symbol: Some(symbol.to_string()),
                    depth,
                    direction: Some(direction.to_string()),
                    budget,
                    ..crate::book::ImpactQuestion::default()
                },
            )
        };

        // The node budget: the walk had six nodes in hand and showed three.
        let nodes = ask("hub", Some(1), "in", Some(3));
        assert_eq!(nodes["truncated"]["reason"], "nodes", "{nodes}");
        assert_eq!(nodes["truncated"]["shown"], 3, "{nodes}");
        assert_eq!(nodes["truncated"]["total"], 6, "{nodes}");
        assert_eq!(nodes["truncated"]["next"]["tool"], "code_impact", "{nodes}");
        assert_eq!(
            nodes["truncated"]["next"]["args"]["symbol"], "hub",
            "{nodes}"
        );
        assert_eq!(nodes["truncated"]["next"]["args"]["budget"], 6, "{nodes}");
        assert_eq!(nodes["count"], 3, "{nodes}");

        // The edge budget: eight edges shown of the twelve the walk had.
        let edges = ask("a", Some(2), "both", Some(4));
        assert_eq!(edges["truncated"]["reason"], "edges", "{edges}");
        assert_eq!(edges["truncated"]["shown"], 8, "{edges}");
        assert_eq!(edges["truncated"]["total"], 12, "{edges}");
        assert_eq!(edges["truncated"]["next"]["args"]["budget"], 8, "{edges}");

        // The hop cap: only when the walk still had somewhere to go.
        scratch.write(
            "src/chain.rs",
            "pub fn x() { y(); }\npub fn y() { z(); }\npub fn z() { w(); }\npub fn w() { v(); }\npub fn v() {}\n",
        );
        scratch.indexed();
        let deep = ask("x", Some(9), "out", None);
        assert_eq!(deep["truncated"]["reason"], "depth", "{deep}");
        assert_eq!(deep["truncated"]["shown"], 4, "{deep}");
        assert!(deep["truncated"]["total"].is_null(), "{deep}");
        assert!(deep["truncated"]["next"].is_null(), "{deep}");

        // Asking for the cap is not a cut, and a finished walk is not one.
        let asked = ask("x", Some(3), "out", None);
        assert!(asked["truncated"].is_null(), "{asked}");
        let done = ask("build", None, "out", None);
        assert!(done["truncated"].is_null(), "{done}");
    }

    /// T5/S5: `code_diagram` splits the density trim (the document has to
    /// render) from the walk cut, and the next call is the connections view
    /// that answers the same question without the document ceiling.
    #[test]
    fn the_code_diagram_names_the_density_trim() {
        let scratch = BookScratch::new("t5-density");
        scratch.write("src/dense.rs", DENSE);
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();

        let response =
            crate::book::code_diagram_with(&scratch.config(), Some(&project), "a", Some(2));
        assert_eq!(response["ok"], true, "{response}");
        assert_eq!(response["truncated"]["reason"], "density", "{response}");
        assert_eq!(
            response["truncated"]["shown"], 15,
            "three edges per node: {response}"
        );
        assert_eq!(response["truncated"]["total"], 20, "{response}");
        assert_eq!(
            response["truncated"]["next"]["tool"], "code_impact",
            "{response}"
        );
        assert_eq!(
            response["truncated"]["next"]["args"]["direction"], "both",
            "{response}"
        );
        assert_eq!(
            response["truncated"]["next"]["args"]["budget"], 600,
            "{response}"
        );
        assert_eq!(response["nodes"], 5, "{response}");
        assert_eq!(response["edges"], 15, "{response}");
        assert_eq!(
            response["diagram"]["edges"].as_array().map(Vec::len),
            Some(15),
            "{response}"
        );
        let validated = crate::contract::validate_diagram(&response["diagram"].to_string());
        assert!(validated.ok, "{:?}", validated.errors);

        // A diagram that fits needs no cut at all.
        let small = crate::book::code_diagram_with(&scratch.config(), Some(&project), "a", Some(1));
        assert!(small["truncated"].is_null(), "{small}");
    }

    /// T5: the four schemas and the descriptions teach the outline default,
    /// the file seed and the query budget, so an agent can pick them without
    /// reading any other document.
    #[test]
    fn the_schemas_and_the_descriptions_teach_the_bounded_answer() {
        let structure = crate::dispatch::tool_schema("code_structure");
        assert!(
            structure["properties"]["after_line"].is_object(),
            "{structure}"
        );
        assert!(structure["properties"]["limit"].is_object(), "{structure}");
        assert_eq!(
            structure["properties"]["kinds"]["type"],
            serde_json::json!(["array", "null"]),
            "{structure}"
        );

        let impact = crate::dispatch::tool_schema("code_impact");
        assert!(impact["properties"]["file"].is_object(), "{impact}");
        assert!(impact["properties"]["edges"].is_object(), "{impact}");
        assert!(impact["properties"]["budget"].is_object(), "{impact}");
        assert!(
            impact.get("required").is_none(),
            "exactly one of `symbol` or `file`: {impact:?}"
        );

        let diagram = crate::dispatch::tool_schema("code_diagram");
        assert!(diagram["properties"]["edges"].is_object(), "{diagram}");
        assert!(
            diagram["properties"].get("budget").is_none(),
            "the diagram keeps the ceilings of the kind: {diagram}"
        );

        let structure_text = crate::harness::description("code_structure");
        for needle in ["outline", "after_line", "collapsed", "truncated"] {
            assert!(
                structure_text.contains(needle),
                "code_structure lacks `{needle}`"
            );
        }
        let impact_text = crate::harness::description("code_impact");
        for needle in ["`file`", "budget", "edges", "files", "truncated"] {
            assert!(impact_text.contains(needle), "code_impact lacks `{needle}`");
        }
        let diagram_text = crate::harness::description("code_diagram");
        for needle in ["edges", "density"] {
            assert!(
                diagram_text.contains(needle),
                "code_diagram lacks `{needle}`"
            );
        }
        assert!(crate::harness::INSTRUCTIONS.contains("outline"));
        assert!(crate::harness::INSTRUCTIONS.contains("file seed"));

        // The router of the stdio server publishes the same arguments: the
        // stdio schema is derived from the params structs, and a knob that
        // exists in the dispatcher only is invisible to that client.
        let tools = GraphiumServer::tool_router().list_all();
        let advertised = |name: &str| {
            tools
                .iter()
                .find(|tool| tool.name == name)
                .unwrap_or_else(|| panic!("{name} is advertised"))
                .input_schema
                .clone()
        };
        let structure = advertised("code_structure");
        for key in ["after_line", "limit", "kinds"] {
            assert!(
                structure["properties"].get(key).is_some(),
                "code_structure takes `{key}`: {structure:?}"
            );
        }
        let impact = advertised("code_impact");
        for key in ["file", "edges", "budget"] {
            assert!(
                impact["properties"].get(key).is_some(),
                "code_impact takes `{key}`: {impact:?}"
            );
        }
        assert!(
            impact.get("required").is_none(),
            "exactly one of `symbol` or `file`: {impact:?}"
        );
        let diagram = advertised("code_diagram");
        assert!(
            diagram["properties"].get("edges").is_some(),
            "code_diagram takes `edges`: {diagram:?}"
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
        // T5: `code_impact` takes a seed (`symbol` or `file`), so no single
        // argument is required; `code_diagram` keeps the symbol.
        assert!(
            crate::dispatch::tool_schema("code_impact")
                .get("required")
                .is_none(),
            "exactly one of `symbol` or `file`"
        );
        assert_eq!(
            crate::dispatch::tool_schema("code_diagram")["required"][0],
            "symbol"
        );
        let error = crate::dispatch::call_tool("code_diagram", &serde_json::json!({}), "s")
            .expect_err("`symbol` is required");
        assert!(error.contains("symbol"), "{error}");

        // The dispatcher hands the seed grammar to the tool, so an empty call
        // is the same `invalid_value` the stdio server answers.
        let empty = crate::dispatch::call_tool("code_impact", &serde_json::json!({}), "s")
            .expect("the grammar is an answer");
        assert_eq!(empty["ok"], false, "{empty}");
        assert_eq!(empty["errors"][0]["code"], "invalid_value", "{empty}");
    }
}
