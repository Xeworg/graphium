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
        let max_chars = match max_chars_argument(request.max_chars) {
            Ok(max_chars) => max_chars,
            Err(response) => return response,
        };

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
        let envelope = match envelope_of(book, &base, max_chars) {
            Ok(envelope) => envelope,
            Err(answer) => return answer,
        };

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
        let mut pack = envelope.pack();
        let packed = pack_structure_rows(&structure.symbols, &answer.symbols, &mut pack);
        let mut response = base;
        response["ok"] = json!(true);
        response["file"] = json!(path);
        response["lang"] = json!(structure.lang);
        response["content_hash"] = json!(structure.content_hash);
        response["indexed_at"] = json!(gx_index::book::iso8601_utc(structure.indexed_at));
        response["count"] = json!(packed.shown());
        response["total"] = json!(answer.total);
        response["symbols"] = json!(packed.rows);
        if !packed.pointers.is_empty() {
            response["pointers"] = json!(packed.pointers);
        }
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
        // The notice names the cut that bound first: the book's own (the
        // outline or the range) or, when the book answered everything and the
        // answer still did not fit, the character envelope (L7).
        let filtered_total = answer
            .truncated
            .as_ref()
            .and_then(|truncation| truncation.total)
            .unwrap_or(answer.symbols.len());
        let shown = packed.shown();
        let reason = answer
            .truncated
            .as_ref()
            .map(|truncation| truncation.reason)
            .or(packed.cliffed.then_some(gx_index::CutReason::Budget));
        let next = match reason {
            Some(gx_index::CutReason::Collapsed) => answer
                .collapsed
                .as_ref()
                .map(|collapsed| structure_next(&path, collapsed.after_line, None, &request.kinds)),
            Some(gx_index::CutReason::Nodes) => packed.last_line.map(|line| {
                let limit = if packed.cliffed {
                    Some(
                        filtered_total
                            .saturating_sub(shown)
                            .clamp(1, gx_index::structure::MAX_LIMIT),
                    )
                } else {
                    Some(answer.symbols.len())
                };
                structure_next(&path, line, limit, &request.kinds)
            }),
            Some(gx_index::CutReason::Budget) if envelope.can_grow() => {
                Some(structure_wide_next(&path, request, envelope.ceiling_chars))
            }
            Some(gx_index::CutReason::Budget) => packed.last_line.map(|line| {
                structure_next(
                    &path,
                    line,
                    Some(
                        filtered_total
                            .saturating_sub(shown)
                            .clamp(1, gx_index::structure::MAX_LIMIT),
                    ),
                    &request.kinds,
                )
            }),
            _ => None,
        };
        response["max_chars"] = json!(envelope.max_chars);
        response["chars"] = json!(pack.used());
        response["truncated"] = match reason {
            Some(reason) => truncated_json(
                &gx_index::Truncation::new(reason, shown, Some(filtered_total)),
                next,
            ),
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
        /// The character envelope of the answer: the ladder of the book when
        /// absent, clamped to its hard ceiling.
        pub max_chars: Option<usize>,
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
        let max_chars = match max_chars_argument(question.max_chars) {
            Ok(max_chars) => max_chars,
            Err(response) => return response,
        };
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
        let envelope = match envelope_of(book, &base, max_chars) {
            Ok(envelope) => envelope,
            Err(answer) => return answer,
        };
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

        let mut pack = envelope.pack();
        let extras = files_json(&grouped);
        let packed = pack_walk(&walked.nodes, &walked.edges, &extras, &mut pack);
        let shown = match (&walked.truncated, packed.cliffed) {
            // The walk bound first: its own cut keeps its own number.
            (Some(truncation), false) => truncation.shown,
            (Some(truncation), true) => match truncation.reason {
                gx_index::CutReason::Edges => packed.edges.len(),
                _ => packed.shown(),
            },
            (None, _) => packed.shown(),
        };
        // The follow-up: the envelope first when it bound and the book is
        // under its ceiling (a bigger answer, one call away), then the walk's
        // own doubling, and at the ceiling a narrower walk (L7).
        let next = if packed.cliffed && envelope.can_grow() {
            Some(with_max_chars(
                walk_call(&seed_args, depth, direction, &edges, Some(limits.max_nodes)),
                Some(envelope.ceiling_chars),
            ))
        } else if let Some(truncation) = &walked.truncated {
            match truncation.reason {
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
            }
        } else if packed.cliffed {
            // The envelope is at its ceiling: the same question, one hop less.
            (depth > 1).then(|| walk_call(&seed_args, 1, direction, &edges, Some(limits.max_nodes)))
        } else {
            None
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
        let reason = walked
            .truncated
            .as_ref()
            .map(|truncation| truncation.reason)
            .or(packed.cliffed.then_some(gx_index::CutReason::Budget));
        let total = walked
            .truncated
            .as_ref()
            .and_then(|truncation| truncation.total)
            .or(packed.cliffed.then_some(walked.nodes.len()));
        response["max_chars"] = json!(envelope.max_chars);
        response["chars"] = json!(pack.used());
        response["truncated"] = match reason {
            Some(reason) => truncated_json(&gx_index::Truncation::new(reason, shown, total), next),
            None => Value::Null,
        };
        response["unresolved"] = json!(walked.unresolved);
        response["count"] = json!(packed.shown());
        response["nodes"] = json!(packed.nodes);
        response["edges"] = json!(packed.edges);
        if !packed.pointers.is_empty() {
            response["pointers"] = json!(packed.pointers);
        }
        if seed_file.is_some() {
            response["files"] = json!(packed.extras);
        }
        response
    }

    /// Runs `code_diagram` for the session with the configuration of the owner.
    pub fn code_diagram(
        session_id: &str,
        symbol: &str,
        depth: Option<u32>,
        edges: Option<&[String]>,
        max_chars: Option<usize>,
    ) -> Value {
        let config = gx_index::BooksConfig::from_env();
        code_diagram_query_with(
            &config,
            project_of(session_id).as_deref(),
            symbol,
            depth,
            edges,
            max_chars,
        )
    }

    /// Builds the `graph` document of one symbol's neighbourhood (S5).
    pub fn code_diagram_with(
        config: &gx_index::BooksConfig,
        project: Option<&str>,
        symbol: &str,
        depth: Option<u32>,
    ) -> Value {
        code_diagram_query_with(config, project, symbol, depth, None, None)
    }

    /// Builds the `graph` document of one symbol's neighbourhood (S5, T5).
    ///
    /// The answer is a **document**, not a paint: the agent sends `diagram` to
    /// `render_diagram` (or to `validate_diagram` first) unchanged. The
    /// document never leaves with a density the kind would reject, so it is
    /// renderable as it arrives. `edges` chooses which kinds the walk follows;
    /// the walk keeps the 300/600 ceilings of the kind, because a bigger
    /// neighbourhood is a `code_impact` question, not a document. Past those
    /// ceilings the character envelope (`max_chars`, L7) trims the document,
    /// never its validity: an edge always connects nodes the document carries.
    pub fn code_diagram_query_with(
        config: &gx_index::BooksConfig,
        project: Option<&str>,
        symbol: &str,
        depth: Option<u32>,
        edges: Option<&[String]>,
        max_chars: Option<usize>,
    ) -> Value {
        let max_chars = match max_chars_argument(max_chars) {
            Ok(max_chars) => max_chars,
            Err(response) => return response,
        };
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
        let envelope = match envelope_of(book, &base, max_chars) {
            Ok(envelope) => envelope,
            Err(answer) => return answer,
        };
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

        // The document keeps the ceilings of the kind first — at most 300
        // nodes, 600 edges and a density of 3, every edge between nodes the
        // document carries — and the envelope trims after them (L7).
        let node_rows: Vec<Value> = impact
            .nodes
            .iter()
            .map(|node| json!({ "id": node_id(node.id), "label": node.qname }))
            .collect();
        let edge_rows: Vec<Value> = impact
            .edges
            .iter()
            .map(|edge| {
                json!({
                    "from": node_id(edge.from),
                    "to": node_id(edge.to),
                    "label": edge.kind,
                })
            })
            .collect();
        let mut pack = envelope.pack();
        let room = pack.room();
        // Nodes and edges share the room: the head of the document is the
        // ranked nodes, and the edges take what the nodes left — a document
        // has no pointer tail, so nothing else competes for the rest.
        let head = if rows_total(node_rows.iter().cloned())
            .saturating_add(rows_total(edge_rows.iter().cloned()))
            <= room
        {
            room
        } else {
            room * HEAD_SHARE_PERCENT / 100
        };
        let mut nodes: Vec<Value> = Vec::new();
        let mut kept: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for (node, row) in impact.nodes.iter().zip(node_rows) {
            if !pack.take_under(&row, head) {
                break;
            }
            kept.insert(node_id(node.id));
            nodes.push(row);
        }
        let node_count = nodes.len();
        let mut document_edges: Vec<Value> = Vec::new();
        let mut dropped_by_density = 0usize;
        let mut dropped_by_envelope = 0usize;
        for (edge, row) in impact.edges.iter().zip(edge_rows) {
            // A node the envelope left out takes its edges with it: the
            // document never dangles.
            if !kept.contains(&node_id(edge.from)) || !kept.contains(&node_id(edge.to)) {
                continue;
            }
            if document_edges.len() >= node_count.saturating_mul(MAX_DENSITY)
                || document_edges.len() >= gx_index::impact::MAX_EDGES
            {
                dropped_by_density += 1;
                continue;
            }
            if !pack.take_under(&row, room) {
                dropped_by_envelope += 1;
                continue;
            }
            document_edges.push(row);
        }
        let edge_count = document_edges.len();
        let nodes_dropped = node_count < impact.nodes.len();
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
                    gx_index::CutReason::Edges => edge_count,
                    _ => node_count,
                };
                Some(truncated_json(
                    &gx_index::Truncation::new(truncation.reason, shown, truncation.total),
                    Some(next),
                ))
            }
            // The envelope trimmed a document the walk completed: the answer
            // says so and names the exact wider call, or the connections view
            // when the envelope is already at its ceiling.
            None if nodes_dropped || dropped_by_envelope > 0 => {
                let (shown, total) = if nodes_dropped {
                    (node_count, Some(impact.nodes.len()))
                } else {
                    (edge_count, Some(impact.edges.len()))
                };
                let next = if envelope.can_grow() {
                    diagram_call(symbol, depth, &edges, envelope.ceiling_chars)
                } else {
                    next
                };
                Some(truncated_json(
                    &gx_index::Truncation::new(gx_index::CutReason::Budget, shown, total),
                    Some(next),
                ))
            }
            None if dropped_by_density > 0 => Some(truncated_json(
                &gx_index::Truncation::new(
                    gx_index::CutReason::Density,
                    edge_count,
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
        response["max_chars"] = json!(envelope.max_chars);
        response["chars"] = json!(pack.used());
        response["truncated"] = truncated.unwrap_or(Value::Null);
        // f14 S4: the walk already knows how many names it could not pin down
        // (`impact.unresolved`); the document publishes the same count under
        // the same name `code_impact` uses, so «no relations» and «nothing
        // resolved» stop looking identical. The document is untouched.
        response["unresolved"] = json!(impact.unresolved);
        response["nodes"] = json!(node_count);
        response["edges"] = json!(edge_count);
        response["diagram"] = document;
        response
    }

    /// The most edges a `graph` document may carry for each node: the density
    /// ceiling of the kind (PRD §7.3).
    const MAX_DENSITY: usize = 3;

    /// The share of the envelope the ranked head of a walk may spend before
    /// the cliff (L7): the full rows take two thirds, the edges that explain
    /// them the next sixth, and the pointer tail what is left. A diagram has
    /// no pointer tail, so its edges take the rest of the room.
    const HEAD_SHARE_PERCENT: usize = 66;

    /// The cumulative share of the head and the edges of a walk: the pointer
    /// tail keeps the last sixth of the envelope.
    const EDGES_SHARE_PERCENT: usize = 83;

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

    /// The breadth of a `code_er` answer (f15, S2): the one file `path` names
    /// (the default — f13's answer, byte for byte) or every `.sql` chapter of
    /// the book merged into one document.
    pub const SCOPE_FILE: &str = "file";
    /// Every `.sql` chapter the book knows, parsed live and merged (f15, S2/S3).
    pub const SCOPE_PROJECT: &str = "project";

    /// The `scope` of a `code_er` call: `file` (the default) or `project`.
    ///
    /// Absent and empty are the default, because `scope` is additive (S6); any
    /// other value is refused at `scope` with both values named, never folded
    /// into the default.
    fn scope_argument(raw: Option<&str>) -> Result<&'static str, Value> {
        let text = raw
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .unwrap_or(SCOPE_FILE);
        match text {
            SCOPE_FILE => Ok(SCOPE_FILE),
            SCOPE_PROJECT => Ok(SCOPE_PROJECT),
            other => Err(failure(
                "scope",
                "invalid_value",
                &format!("`{other}` is not a scope"),
                Some(
                    "send `file` (the default: only the file `path` names) or `project` (every `.sql` chapter of the book, merged into one document)",
                ),
            )),
        }
    }

    /// Runs `code_er` for the session with the configuration of the owner (f13,
    /// f15).
    pub fn code_er(session_id: &str, path: &str, scope: Option<&str>) -> Value {
        let config = gx_index::BooksConfig::from_env();
        code_er_query_with(&config, project_of(session_id).as_deref(), path, scope)
    }

    /// The `er` document of a `.sql` file of the project (f13, S1/S2), or of
    /// every `.sql` chapter of the book merged (f15, S2/S3).
    ///
    /// The answer is a **document**, not a paint: the agent sends `diagram` to
    /// `render_diagram` (or to `validate_diagram` first) unchanged, the same
    /// contract `code_diagram` keeps. The mapping is the bridge's own
    /// `gx_index::lang::sql::er_bridge::chapter_document` — the chapter's parse
    /// plus `document_json`, never a second mapping that could drift from it —
    /// and the source is read **live** from the file the path names, because
    /// the schema keeps no row for a column. `path` is relative to the root of
    /// the project, the same shape the book stores, and a path that leaves the
    /// project is refused before anything is opened (S3).
    ///
    /// `scope` chooses the breadth and never the mapping: under
    /// [`SCOPE_FILE`] the document is the bridge's `chapter_document`, and
    /// under [`SCOPE_PROJECT`] the details of the book's `.sql` chapters are
    /// concatenated in the book's order and fed to the bridge's own
    /// `document_json` — the door re-maps nothing (f15, S4).
    pub fn code_er_query_with(
        config: &gx_index::BooksConfig,
        project: Option<&str>,
        path: &str,
        scope: Option<&str>,
    ) -> Value {
        let path = match text_argument(path, "path") {
            Ok(path) => path,
            Err(response) => return response,
        };
        // S3: the path stays inside the project. The guard reads path
        // components, not strings, so `a/../../b` is refused the same as `..`
        // and an absolute path, and nothing outside the project is opened.
        let relative = std::path::Path::new(path);
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| !matches!(component, std::path::Component::Normal(_)))
        {
            return failure(
                "path",
                "invalid_value",
                "the path has to be relative to the root of the project",
                Some(
                    "name the file the way the book stores it, with `/` separators — `db/schema.sql`, never an absolute path or a `..`",
                ),
            );
        }
        let scope = match scope_argument(scope) {
            Ok(scope) => scope,
            Err(response) => return response,
        };
        let opened = match open(config, project) {
            Ok(opened) => opened,
            Err(response) => return response,
        };
        match scope {
            SCOPE_PROJECT => project_document(&opened, path),
            // S6: the default is f13's answer, one file in and the bridge's
            // own document out.
            _ => file_document(&opened, path),
        }
    }

    /// The document of the one file `path` names (`scope: "file"`): the
    /// bridge's `chapter_document`, f13's answer byte for byte.
    fn file_document(opened: &Opened, path: &str) -> Value {
        let source = match read_source(opened, path) {
            Ok(source) => source,
            Err(response) => return response,
        };
        // The bridge exists only when the SQL grammar is compiled in; without
        // the feature the tool answers the same refusal a file of another
        // language gets, and the hint names the build as the cause (S4).
        #[cfg(feature = "lang-sql")]
        let document = gx_index::lang::sql::er_bridge::chapter_document(path, &source);
        #[cfg(not(feature = "lang-sql"))]
        let document: Option<String> = None;
        document_answer(opened, path, document, None)
    }

    /// The document of the whole project (`scope: "project"`, f15 S2/S3): the
    /// detail of every `.sql` chapter of the book, parsed **live** from disk in
    /// the book's chapter order and merged, with the named file part of the
    /// merge like any other chapter's.
    ///
    /// The chapter list is the book's ([`gx_index::Book::paths_of_language`]),
    /// never a walk of the directory; a chapter whose file no longer reads is
    /// skipped, and `files` counts the chapters that were actually parsed. The
    /// document is the bridge's own `document_json` over the merged slice, so
    /// the mapping stays in `gx-index` and the door only concatenates (S4).
    fn project_document(opened: &Opened, path: &str) -> Value {
        // `path` stays required under both scopes, so the named file is read
        // first and strictly: a path that does not read is the same failure
        // here as under `scope: "file"`.
        let source = match read_source(opened, path) {
            Ok(source) => source,
            Err(response) => return response,
        };
        #[cfg(feature = "lang-sql")]
        {
            let mut chapters = match opened
                .handle
                .book()
                .paths_of_language(gx_index::lang::sql::LANGUAGE)
            {
                Ok(chapters) => chapters,
                Err(error) => {
                    return failure("book", UNAVAILABLE_CODE, &error.to_string(), None);
                }
            };
            // The named file takes part like any chapter, even when the book
            // has not caught up with it yet (a file indexed a moment later, or
            // one outside the language the book stored).
            if !chapters.iter().any(|chapter| chapter == path) {
                chapters.push(path.to_string());
            }
            let mut merged: Vec<gx_index::lang::model::ParsedTable> = Vec::new();
            let mut files = 0usize;
            for chapter in &chapters {
                let text = if chapter == path {
                    source.clone()
                } else {
                    match std::fs::read_to_string(
                        std::path::Path::new(&opened.project).join(chapter),
                    ) {
                        Ok(text) => text,
                        // S3: a chapter that no longer reads is skipped, and
                        // `files` counts only what was parsed.
                        Err(_) => continue,
                    }
                };
                let Ok(parsed) = gx_index::lang::parse(chapter, text.as_bytes(), 0) else {
                    continue;
                };
                files += 1;
                if let Some(tables) = parsed.tables {
                    merged.extend(tables);
                }
            }
            let document = gx_index::lang::sql::er_bridge::document_json(&merged);
            document_answer(opened, path, document, Some(files))
        }
        #[cfg(not(feature = "lang-sql"))]
        {
            let _ = &source;
            document_answer(opened, path, None, Some(0))
        }
    }

    /// Reads the chapter `path` names under the root of the project, or the
    /// answer that says why it does not read.
    fn read_source(opened: &Opened, path: &str) -> Result<String, Value> {
        let absolute = std::path::Path::new(&opened.project).join(path);
        std::fs::read_to_string(&absolute).map_err(|error| {
            failure(
                "path",
                "invalid_value",
                &format!("the file does not read: {error}"),
                Some("the path is relative to the root of the project the session declared"),
            )
        })
    }

    /// The answer both scopes share (f14 S2/S3, f15 S3): the document the
    /// bridge handed over, its counts, `chars`, the optional `files` count of
    /// a merged document and the `er_over_kind` warning — one path for both
    /// scopes, never two copies that could drift.
    fn document_answer(
        opened: &Opened,
        path: &str,
        document: Option<String>,
        files: Option<usize>,
    ) -> Value {
        let Some(document) = document else {
            return failure(
                "path",
                "invalid_value",
                "the file produces no `er` document",
                Some(
                    "a document only comes from a file whose grammar reads tables with columns: a `.sql` chapter with `CREATE TABLE` and column lists — a file of another language, a DDL with no column list, or a build without the SQL grammar all land here",
                ),
            );
        };
        // f14 S2: `chars` is the size of the string the bridge handed over,
        // measured on that string before it is parsed — never a re-serialized
        // copy that could differ from the document the agent sends onward.
        let chars = document.len();
        let document: Value = match serde_json::from_str(&document) {
            Ok(document) => document,
            Err(error) => {
                return failure(
                    "book",
                    UNAVAILABLE_CODE,
                    &format!("the bridge produced an unreadable document: {error}"),
                    None,
                );
            }
        };
        let tables = document
            .get("tables")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        let relations = document
            .get("relations")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        let mut base = base(opened);
        base["ok"] = json!(true);
        base["file"] = json!(path);
        base["tables"] = json!(tables);
        base["relations"] = json!(relations);
        base["chars"] = json!(chars);
        // f15 S3: a merged document counts the chapters that were parsed; the
        // file scope keeps f13's answer with no such field (S6).
        if let Some(files) = files {
            base["files"] = json!(files);
        }
        // f14 S3: the render would refuse this document with `too_large`, and
        // the agent learns it here, before spending the round trip. `ok` stays
        // `true` and the document travels whole (f13, S6).
        if let Some(warning) = over_kind_warning(tables, relations) {
            base["warnings"] = json!([warning]);
        }
        base["diagram"] = document;
        base
    }

    /// The `er_over_kind` warning of a document past the ceilings of the `er`
    /// kind (f14, S3), or `None` when the render would accept it.
    ///
    /// The predicate is the one the render applies to an `er` document: the
    /// engine (`gx_core::engine`) compares the graph it built against the
    /// `limits()` of the registered kind, and that `check_limits` is private,
    /// so the same three comparisons run here over the document counts — the
    /// bridge maps one table to one node and one relation to one edge, which
    /// is exactly what the kind's `to_ir` does (gx-kinds `er.rs:118-124`).
    fn over_kind_warning(tables: usize, relations: usize) -> Option<Value> {
        crate::bootstrap();
        let limits = gx_core::kind::for_kind("er")?.limits();
        let density = relations as f64 / tables.max(1) as f64;
        let over = tables > limits.max_nodes
            || relations > limits.max_edges
            || density > limits.max_density;
        if !over {
            return None;
        }
        // The object keeps the keys the render's warnings[] and errors[] share
        // (contract.rs): `code` and `message`, plus the optional `hint` an
        // error carries.
        Some(json!({
            "code": "er_over_kind",
            "message": format!(
                "the `er` document carries {tables} tables and {relations} relations, over the ceilings of the kind ({} tables, {} relations, density {:.1})",
                limits.max_nodes, limits.max_edges, limits.max_density
            ),
            "hint": "`render_diagram` will refuse this document with `too_large`: split the schema file into smaller `.sql` files and draw each one",
        }))
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

    /// The same call with a bigger envelope (L7): the exact widening a budget
    /// cut proposes while the book is under its hard ceiling.
    fn with_max_chars(mut call: Value, max_chars: Option<usize>) -> Value {
        if let (Some(max_chars), Some(args)) = (max_chars, call["args"].as_object_mut()) {
            args.insert("max_chars".to_string(), json!(max_chars));
        }
        call
    }

    /// The same `code_structure` call with a bigger envelope (L7): the
    /// `after_line`/`limit`/`kinds` of this answer are kept, so the wider call
    /// asks exactly the same question.
    fn structure_wide_next(
        file: &str,
        request: &gx_index::StructureRequest,
        max_chars: usize,
    ) -> Value {
        let mut args = serde_json::Map::new();
        args.insert("file".to_string(), json!(file));
        if let Some(after_line) = request.after_line {
            args.insert("after_line".to_string(), json!(after_line));
        }
        if let Some(limit) = request.limit {
            args.insert("limit".to_string(), json!(limit));
        }
        if !request.kinds.is_empty() {
            args.insert("kinds".to_string(), json!(request.kinds));
        }
        args.insert("max_chars".to_string(), json!(max_chars));
        json!({ "tool": "code_structure", "args": Value::Object(args) })
    }

    /// The same `code_diagram` call with a bigger envelope (L7).
    fn diagram_call(symbol: &str, depth: u32, edges: &[String], max_chars: usize) -> Value {
        let mut args = serde_json::Map::new();
        args.insert("symbol".to_string(), json!(symbol));
        args.insert("depth".to_string(), json!(depth));
        if !edges.is_empty() {
            args.insert("edges".to_string(), json!(edges));
        }
        args.insert("max_chars".to_string(), json!(max_chars));
        json!({ "tool": "code_diagram", "args": Value::Object(args) })
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

    /// The `max_chars` envelope of the three answering tools (L7): absent is
    /// the ladder of the book, and `0` is a mistake the agent would read as
    /// «no answer at all».
    fn max_chars_argument(raw: Option<usize>) -> Result<Option<usize>, Value> {
        if raw == Some(0) {
            return Err(failure(
                "max_chars",
                "invalid_value",
                "`max_chars` is 0",
                Some("send how many characters the answer may carry: the default is the ladder of the book (13,000 up to 150 indexed files, 18,000 up to 500, 24,000 beyond) and the ceiling is 1.5 times that"),
            ));
        }
        Ok(raw)
    }

    /// The envelope of one answer: the ladder of the book it was asked of, or
    /// the caller's `max_chars` clamped to the hard ceiling of that rung (L7).
    fn envelope_of(
        book: &gx_index::Book,
        base: &Value,
        requested: Option<usize>,
    ) -> Result<gx_index::Envelope, Value> {
        let (files, _symbols) = book.counts().map_err(|error| {
            failure_with(
                base.clone(),
                "book",
                UNAVAILABLE_CODE,
                &error.to_string(),
                None,
            )
        })?;
        Ok(gx_index::Envelope::resolve(files, requested))
    }

    /// What the envelope made of the rows of one chapter (L7): the full rows
    /// at the head and the pointer tail after the cliff.
    #[derive(Debug, Default)]
    struct PackedRows {
        rows: Vec<Value>,
        pointers: Vec<Value>,
        /// The line of the last row the answer carries: the cursor of the
        /// follow-up call.
        last_line: Option<u32>,
        /// Whether the envelope stopped the full rows.
        cliffed: bool,
    }

    impl PackedRows {
        /// The entries the answer carries: full rows and pointers.
        fn shown(&self) -> usize {
            self.rows.len() + self.pointers.len()
        }
    }

    /// What the envelope made of one walk (L7).
    #[derive(Debug, Default)]
    struct PackedWalk {
        nodes: Vec<Value>,
        edges: Vec<Value>,
        /// The grouping of a file seed, trimmed with the envelope.
        extras: Vec<Value>,
        pointers: Vec<Value>,
        /// Whether the envelope stopped the full nodes.
        cliffed: bool,
    }

    impl PackedWalk {
        /// The entries the answer carries: full nodes and pointers.
        fn shown(&self) -> usize {
            self.nodes.len() + self.pointers.len()
        }
    }

    /// The characters a list of serialized rows spends, separators included.
    fn rows_total<I>(rows: I) -> usize
    where
        I: IntoIterator<Item = Value>,
    {
        rows.into_iter().fold(0usize, |total, row| {
            total.saturating_add(gx_index::envelope::measure(&row).saturating_add(1))
        })
    }

    /// Packs the rows of a chapter into the envelope (L7): the span-ordered
    /// head as full rows, the tail as `{name, line, kind}` pointers. Past the
    /// cliff the `collapsed`/`after_line` cursor keeps reading the chapter.
    fn pack_structure_rows(
        all: &[gx_index::StructureSymbol],
        rows: &[gx_index::StructureSymbol],
        pack: &mut gx_index::Pack,
    ) -> PackedRows {
        let qnames: std::collections::BTreeMap<i64, &str> = all
            .iter()
            .map(|symbol| (symbol.id, symbol.qname.as_str()))
            .collect();
        let room = pack.room();
        let whole = rows_total(
            rows.iter()
                .map(|symbol| structure_symbol_json(&qnames, symbol)),
        );
        let head = if whole <= room {
            room
        } else {
            room * HEAD_SHARE_PERCENT / 100
        };
        let mut packed = PackedRows::default();
        for symbol in rows {
            if !packed.cliffed {
                let row = structure_symbol_json(&qnames, symbol);
                if pack.take_under(&row, head) {
                    packed.rows.push(row);
                    packed.last_line = Some(symbol.line);
                    continue;
                }
                packed.cliffed = true;
            }
            let pointer = json!({
                "name": symbol.name,
                "line": symbol.line,
                "kind": symbol.kind,
            });
            if !pack.take(&pointer) {
                break;
            }
            packed.pointers.push(pointer);
            packed.last_line = Some(symbol.line);
        }
        packed
    }

    /// Packs one walk into the envelope (L7): the ranked nodes as full rows,
    /// the edges between the rows the answer carries, the `extras` of the
    /// answer (the grouping of a file seed) and the rest of the nodes as
    /// `{name, file, line}` pointers.
    fn pack_walk(
        nodes: &[gx_index::ImpactNode],
        edges: &[gx_index::ImpactEdge],
        extras: &[Value],
        pack: &mut gx_index::Pack,
    ) -> PackedWalk {
        let room = pack.room();
        let whole = rows_total(nodes.iter().map(node_json))
            .saturating_add(rows_total(edges.iter().map(edge_json)))
            .saturating_add(rows_total(extras.iter().cloned()));
        let (head, connective) = if whole <= room {
            (room, room)
        } else {
            (
                room * HEAD_SHARE_PERCENT / 100,
                room * EDGES_SHARE_PERCENT / 100,
            )
        };
        let mut packed = PackedWalk::default();
        let mut full: std::collections::BTreeSet<i64> = std::collections::BTreeSet::new();
        for node in nodes {
            let row = node_json(node);
            if !pack.take_under(&row, head) {
                packed.cliffed = true;
                break;
            }
            full.insert(node.id);
            packed.nodes.push(row);
        }
        // An edge connects rows the answer fully carries: a pointer has no id,
        // so an edge to it would reference nothing.
        for edge in edges {
            if !full.contains(&edge.from) || !full.contains(&edge.to) {
                continue;
            }
            let row = edge_json(edge);
            if !pack.take_under(&row, connective) {
                break;
            }
            packed.edges.push(row);
        }
        // The grouping of a file seed is part of what the window pays for, so
        // the envelope trims it too.
        for extra in extras {
            if !pack.take(extra) {
                packed.cliffed = true;
                break;
            }
            packed.extras.push(extra.clone());
        }
        let cliff = if packed.cliffed {
            packed.nodes.len()
        } else {
            nodes.len()
        };
        for node in &nodes[cliff..] {
            let pointer = json!({
                "name": node.name,
                "file": node.file,
                "line": node.line,
            });
            if !pack.take(&pointer) {
                break;
            }
            packed.pointers.push(pointer);
        }
        packed
    }

    /// One node of a walk as the agent reads it.
    fn node_json(node: &gx_index::ImpactNode) -> Value {
        json!({
            "id": node_id(node.id),
            "name": node.name,
            "kind": node.kind,
            "qname": node.qname,
            "file": node.file,
            "line": node.line,
            "depth": node.depth,
        })
    }

    /// One edge of a walk as the agent reads it.
    fn edge_json(edge: &gx_index::ImpactEdge) -> Value {
        json!({
            "from": node_id(edge.from),
            "to": node_id(edge.to),
            "kind": edge.kind,
        })
    }

    /// One declaration of a chapter, with the parent as the qname it belongs
    /// to: the tree is rebuildable from this list alone.
    ///
    /// `qnames` is the whole chapter, because an outline hides the parent of a
    /// child it does show.
    fn structure_symbol_json(
        qnames: &std::collections::BTreeMap<i64, &str>,
        symbol: &gx_index::StructureSymbol,
    ) -> Value {
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
The answer is bounded in characters too (L7): it carries `chars` (what it \
spends) and `max_chars` (the envelope it had — the ladder of the book by \
indexed files, 13,000 up to 150, 18,000 up to 500, 24,000 beyond, which the \
`max_chars` argument overrides up to 1.5 times). Past the envelope the full \
rows stop and the rest of the span-ordered tail arrives as one-line \
`pointers: [{name, line, kind}]`, so the `after_line` cursor still reads what \
was left; that cut says `reason: \"budget\"` and its `next` is the exact \
wider call (`max_chars` at the ceiling) or, at the ceiling, the cursor page. \
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
    /// T4/C3: the character envelope of the answer (L7). Absent is the ladder
    /// of the book; the hard ceiling is 1.5 times its rung.
    #[serde(default)]
    pub max_chars: Option<usize>,
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
The answer is bounded in characters too (L7): it carries `chars` and \
`max_chars` (the ladder of the book by indexed files, which the `max_chars` \
argument overrides up to 1.5 times), and the nodes are ranked by relevance — \
the hop first, then the edges of the answer that touch each node — so a \
capped answer leads with the most connected. Past the envelope the full rows \
stop and the rest arrives as one-line `pointers: [{name, file, line}]`; that \
cut says `reason: \"budget\"` and `next` is the exact wider call, or a \
narrower walk (`depth: 1`) when the envelope is already at its ceiling. \
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
    /// T4/C3: the character envelope of the answer (L7). Absent is the ladder
    /// of the book; the hard ceiling is 1.5 times its rung.
    #[serde(default)]
    pub max_chars: Option<usize>,
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
ceilings of the kind in `budget`), `budget` means the character envelope of \
the answer (`max_chars`, L7 — the ladder of the book by indexed files, \
overridable up to 1.5 times) trimmed nodes or edges after the kind's ceilings, \
and `next` is the `code_impact` call that answers the same question with its \
own query budget or, when the envelope can grow, the same `code_diagram` with \
a bigger `max_chars`. A trimmed document always renders: an edge never points \
at a node the document dropped. The edges are syntactic, \
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
    /// T4/C3: the character envelope of the answer (L7). Absent is the ladder
    /// of the book; the hard ceiling is 1.5 times its rung.
    #[serde(default)]
    pub max_chars: Option<usize>,
}

/// Description of the `code_er` tool, written for the agent (f13, S5).
// The text is duplicated from `harness::description("code_er")`, like every
// other book tool const, so the harness and the stdio router say the same.
const CODE_ER_DESCRIPTION: &str =
    "Build the `er` document of one `.sql` file of the project — or of the whole \
project with `scope: \"project\"` — its tables as \
compartments with their columns and the keys between them — without reading the \
file yourself: the answer carries the document in `diagram` (`version: 1`, \
`kind: \"er\"`), ready to send to `validate_diagram` and then `render_diagram` as \
it is, the same contract `code_diagram` keeps. `path` is the file relative to \
the project root with `/` separators, the way the book stores it \
(`db/schema.sql`); absolute paths, `..` and a leading `./` are refused the same \
way, with `ok: false`, code `invalid_value` at `path`, because the guard reads \
path components before anything is opened. The source is read live, \
so the document is the file as it is now, not the indexed snapshot. A file that \
produces no document — another language, a DDL with no column list, or a build \
without the SQL grammar — answers `ok: false` with the three causes named. \
`scope` chooses the breadth, never the contract: `\"file\"` (the default) draws \
only the file `path` names; `\"project\"` parses every `.sql` chapter the book \
knows live from disk, merges them in the book's chapter order — a table name \
declared twice is drawn once, a foreign key that points at a table in another \
file anchors — and answers one document, with `files` reporting how many \
chapters were parsed and a chapter that no longer reads skipped. `path` stays \
required under both scopes: it names the file of the answer, and under \
`\"project\"` its tables take part in the merge like any other chapter's. \
Beside `diagram` the answer reports `tables`, `relations` and `chars` (the byte \
length of the document), so its size is read without counting the JSON. The \
ceilings of the `er` kind apply at the render — a schema bigger than the kind \
accepts is refused there with `too_large` — and a document already over them \
warns first, with `er_over_kind` in `warnings[]`: `ok` stays `true`, the \
document travels whole, and splitting the schema file is the way to render it. \
The book is on by default \
(`book_disabled` only with an explicit `GRAPHIUM_BOOK=off`). It never renders \
and never touches the plan.";

/// Parameters of the `code_er` tool (f13, S2; f15, S2).
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CodeErParams {
    /// The `.sql` file of the project, relative to its root, with `/`.
    pub path: String,
    /// f15: `file` (the default) or `project`, which merges every `.sql`
    /// chapter of the book into one document.
    #[serde(default)]
    pub scope: Option<String>,
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
            max_chars: params.max_chars,
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
            max_chars: params.max_chars,
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
                params.max_chars,
            )
        })
        .await
        .map_err(|error| {
            ErrorData::internal_error(format!("the diagram task failed: {error}"), None)
        })?;
        structured(response)
    }

    #[tool(
        name = "code_er",
        description = CODE_ER_DESCRIPTION,
        annotations(
            title = "Build the er document of a .sql file from the book",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn code_er(
        &self,
        Parameters(params): Parameters<CodeErParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let session_id = self.session_id.clone();
        let scope = params.scope.clone();
        // The source is read live from disk: the read runs off the async loop.
        let response = tokio::task::spawn_blocking(move || {
            book::code_er(&session_id, &params.path, scope.as_deref())
        })
        .await
        .map_err(|error| ErrorData::internal_error(format!("the er task failed: {error}"), None))?;
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

    /// f13/S1: every tool the dispatcher answers is advertised over stdio too.
    /// A tool that exists in the dispatcher and not here is invisible to a
    /// stdio client, and every client config speaks stdio.
    #[test]
    fn every_dispatcher_tool_is_advertised_over_stdio() {
        let tools = GraphiumServer::tool_router().list_all();
        for name in crate::dispatch::TOOL_NAMES {
            assert!(
                tools.iter().any(|tool| tool.name == name),
                "{name} is advertised over stdio"
            );
        }
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

        // f14 S4: every name of this neighbourhood resolves, so the honest
        // count of the walk is zero — the field is the walk's own number, not
        // a symptom of a missing document.
        assert_eq!(response["unresolved"], 0, "{response}");

        // A symbol with no relations is a one-node document, not an error.
        let lonely =
            crate::book::code_diagram_with(&scratch.config(), Some(&project), "lonely", None);
        assert_eq!(lonely["ok"], true, "{lonely}");
        assert_eq!(
            lonely["diagram"]["nodes"].as_array().map(Vec::len),
            Some(1),
            "{lonely}"
        );
        assert_eq!(
            lonely["unresolved"], 0,
            "a symbol with no calls resolves nothing wrongly: {lonely}"
        );
        let validated = crate::contract::validate_diagram(&lonely["diagram"].to_string());
        assert!(validated.ok, "{:?}", validated.errors);
    }

    /// f14 S4: the diagram walk publishes the same `unresolved` that
    /// `code_impact` publishes — the names the walk could not pin down — so
    /// «no relations» and «nothing resolved» stop looking identical.
    #[test]
    fn code_diagram_reports_the_names_the_walk_could_not_resolve() {
        let scratch = BookScratch::new("diagram-unresolved");
        scratch.write(
            "src/lonely.rs",
            "pub fn build() -> u8 { missing_helper() }\n",
        );
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();

        let response =
            crate::book::code_diagram_with(&scratch.config(), Some(&project), "build", None);
        assert_eq!(response["ok"], true, "{response}");
        let unresolved = response["unresolved"]
            .as_u64()
            .expect("the answer says how many names it could not resolve");
        assert!(
            unresolved > 0,
            "`missing_helper` has no declaration in the book: {response}"
        );
        assert!(
            response["diagram"]["nodes"]
                .as_array()
                .is_some_and(|nodes| !nodes.is_empty()),
            "the document still travels: {response}"
        );

        // The same walk, the same number: `code_impact` was already honest
        // about it and the diagram does not disagree.
        let impact = crate::book::code_impact_with(
            &scratch.config(),
            Some(&project),
            "build",
            Some(1),
            Some("both"),
        );
        assert_eq!(impact["ok"], true, "{impact}");
        assert_eq!(
            impact["unresolved"], unresolved,
            "the two tools report the same count: {response}"
        );
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
                    max_chars: None,
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
                max_chars: None,
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
            None,
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

    // -----------------------------------------------------------------
    // f13 — `code_er`: a `.sql` file in, the `er` document out
    // -----------------------------------------------------------------

    /// The DDL of the proof: two tables, one inline foreign key.
    const CODE_ER_DDL: &str = "\
CREATE TABLE users (\n\
  id INTEGER PRIMARY KEY,\n\
  name TEXT NOT NULL\n\
);\n\
CREATE TABLE posts (\n\
  id INTEGER PRIMARY KEY,\n\
  user_id INTEGER NOT NULL REFERENCES users(id),\n\
  title TEXT\n\
);\n";

    /// S2: the tool answers the bridge's own document — the exact one
    /// `chapter_document` builds, never a second mapping — with the book's
    /// fields around it, and the relation anchors from the primary key to the
    /// foreign key.
    #[test]
    fn code_er_answers_the_document_the_bridge_builds() {
        let scratch = BookScratch::new("code-er");
        scratch.write("db/schema.sql", CODE_ER_DDL);
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();

        let answer = crate::book::code_er_query_with(
            &scratch.config(),
            Some(&project),
            "db/schema.sql",
            None,
        );
        assert_eq!(answer["ok"], true, "{answer}");
        assert_eq!(answer["file"], "db/schema.sql", "{answer}");
        assert!(answer["book_key"].is_string(), "{answer}");

        let document = &answer["diagram"];
        assert_eq!(document["kind"], "er", "{answer}");
        assert_eq!(document["version"], 1, "{answer}");
        let tables = document["tables"].as_array().expect("the tables travel");
        let names: Vec<&str> = tables
            .iter()
            .map(|table| table["name"].as_str().expect("a table has a name"))
            .collect();
        assert_eq!(names, ["users", "posts"], "{answer}");

        let relations = document["relations"]
            .as_array()
            .expect("the relations travel");
        assert_eq!(relations.len(), 1, "{answer}");
        assert_eq!(relations[0]["from"]["table"], "users", "{answer}");
        assert_eq!(relations[0]["from"]["column"], "id", "{answer}");
        assert_eq!(relations[0]["to"]["table"], "posts", "{answer}");
        assert_eq!(relations[0]["to"]["column"], "user_id", "{answer}");

        // No second mapping: the document is the bridge's own output, the one
        // the door already renders (f12 T3 proved the paint of this shape).
        let source = std::fs::read_to_string(scratch.root().join("db/schema.sql")).expect("read");
        let direct = gx_index::lang::sql::er_bridge::chapter_document("db/schema.sql", &source)
            .expect("the bridge builds it");
        assert_eq!(
            document,
            &serde_json::from_str::<serde_json::Value>(&direct).expect("the document parses"),
            "the tool hands the bridge's document, not a copy of it"
        );
    }

    /// f14 S2: the answer says how big the document is — the counts the agent
    /// would otherwise read out of the JSON, and `chars`, the byte length of
    /// the string the bridge handed over (the same meaning `code_structure`'s
    /// `chars` has). The document itself is untouched (f13, S6).
    #[test]
    fn code_er_reports_the_size_of_the_document_it_answers() {
        let scratch = BookScratch::new("code-er-size");
        scratch.write("db/schema.sql", CODE_ER_DDL);
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();

        let answer = crate::book::code_er_query_with(
            &scratch.config(),
            Some(&project),
            "db/schema.sql",
            None,
        );
        assert_eq!(answer["ok"], true, "{answer}");
        assert_eq!(answer["tables"], 2, "the two tables of the DDL");
        assert_eq!(answer["relations"], 1, "the one foreign key of the DDL");
        assert_eq!(
            answer["diagram"]["tables"].as_array().map(Vec::len),
            Some(2),
            "the document still travels whole"
        );

        // `chars` is the bridge's own string measured byte for byte: the
        // answer never re-serializes the document to guess its size.
        let source = std::fs::read_to_string(scratch.root().join("db/schema.sql")).expect("read");
        let direct = gx_index::lang::sql::er_bridge::chapter_document("db/schema.sql", &source)
            .expect("the bridge builds it");
        assert_eq!(
            answer["chars"],
            direct.len(),
            "chars is the byte length of the bridge's document"
        );

        // A document inside the ceilings of the kind carries no warning: the
        // field only appears when there is something to say (S3).
        assert!(
            answer["warnings"].is_null(),
            "a small schema needs no warning: {}",
            answer["warnings"]
        );
    }

    /// f14 S3: a schema past the ceilings of the `er` kind answers `ok: true`
    /// with the whole document — never trimmed — and one `er_over_kind`
    /// warning that names the refusal the render would give and the way out.
    #[test]
    fn code_er_warns_before_a_document_over_the_kind_ceilings() {
        let scratch = BookScratch::new("code-er-over-kind");
        // 301 tables is one past the 300 the `er` kind accepts; the only
        // relation points at the first table.
        let mut ddl = String::from("CREATE TABLE t000 (\n  id INTEGER PRIMARY KEY\n);\n");
        for index in 1..301 {
            ddl.push_str(&format!(
                "CREATE TABLE t{index:03} (\n  id INTEGER PRIMARY KEY"
            ));
            if index == 1 {
                ddl.push_str(",\n  ref INTEGER REFERENCES t000(id)");
            }
            ddl.push_str("\n);\n");
        }
        scratch.write("db/big.sql", &ddl);
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();

        let answer =
            crate::book::code_er_query_with(&scratch.config(), Some(&project), "db/big.sql", None);
        assert_eq!(answer["ok"], true, "the answer is still a success");
        assert_eq!(answer["tables"], 301, "the count travels");
        assert_eq!(answer["relations"], 1, "the count travels");
        // f13's contract: the document travels whole, never trimmed.
        assert_eq!(
            answer["diagram"]["tables"].as_array().map(Vec::len),
            Some(301),
            "the over-kind document is not trimmed"
        );
        let warnings = answer["warnings"]
            .as_array()
            .expect("the over-kind warning travels");
        assert_eq!(warnings.len(), 1, "one warning is enough: {warnings:?}");
        assert_eq!(warnings[0]["code"], "er_over_kind", "{warnings:?}");
        assert!(
            warnings[0]["message"]
                .as_str()
                .is_some_and(|message| message.contains("301")),
            "the message names the size: {warnings:?}"
        );
        assert!(
            warnings[0]["hint"]
                .as_str()
                .is_some_and(|hint| hint.contains("too_large")),
            "the hint names the refusal the render would give: {warnings:?}"
        );
    }

    /// S3: the path never leaves the project — absolute, `..`, an escape
    /// through a nested directory and a decorating `.` are all refused before
    /// anything is opened.
    #[test]
    fn code_er_refuses_paths_that_leave_the_project() {
        let scratch = BookScratch::new("code-er-guard");
        scratch.write("db/schema.sql", CODE_ER_DDL);
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();

        for path in [
            "/etc/passwd",
            "../outside.sql",
            "db/../../outside.sql",
            "./db/schema.sql",
        ] {
            let answer =
                crate::book::code_er_query_with(&scratch.config(), Some(&project), path, None);
            assert_eq!(answer["ok"], false, "{path}: {answer}");
            assert_eq!(
                answer["errors"][0]["code"], "invalid_value",
                "{path}: {answer}"
            );
            assert_eq!(answer["errors"][0]["path"], "path", "{path}: {answer}");
            assert_eq!(
                answer["errors"][0]["message"],
                "the path has to be relative to the root of the project",
                "{path}: {answer}"
            );
            assert!(answer["errors"][0]["hint"].is_string(), "{path}: {answer}");
        }
    }

    /// S4: the refusal names what a document needs — the three causes — and a
    /// file that does not read is its own message, never an empty success.
    #[test]
    fn code_er_refuses_a_file_no_document_comes_from() {
        let scratch = BookScratch::new("code-er-refuse");
        scratch.write("src/main.rs", "pub fn main() {}\n");
        scratch.write("db/bare.sql", "CREATE TABLE bare ();\n");
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();

        let rust =
            crate::book::code_er_query_with(&scratch.config(), Some(&project), "src/main.rs", None);
        assert_eq!(rust["ok"], false, "{rust}");
        assert_eq!(rust["errors"][0]["code"], "invalid_value", "{rust}");
        assert!(
            rust["errors"][0]["message"]
                .as_str()
                .is_some_and(|message| message.contains("no `er` document")),
            "{rust}"
        );
        let hint = rust["errors"][0]["hint"]
            .as_str()
            .expect("the hint names the causes");
        assert!(hint.contains("another language"), "{rust}");
        assert!(hint.contains("SQL grammar"), "{rust}");

        let bare =
            crate::book::code_er_query_with(&scratch.config(), Some(&project), "db/bare.sql", None);
        assert_eq!(bare["ok"], false, "{bare}");
        assert!(
            bare["errors"][0]["message"]
                .as_str()
                .is_some_and(|message| message.contains("no `er` document")),
            "{bare}"
        );

        let missing =
            crate::book::code_er_query_with(&scratch.config(), Some(&project), "db/nope.sql", None);
        assert_eq!(missing["ok"], false, "{missing}");
        assert!(
            missing["errors"][0]["message"]
                .as_str()
                .is_some_and(|message| message.contains("does not read")),
            "{missing}"
        );
    }

    /// S4: the book's own switch governs the tool — `GRAPHIUM_BOOK=off` is
    /// the owner's explicit opt-out, and the tool does not read around it.
    #[test]
    fn code_er_honours_the_book_switch() {
        let scratch = BookScratch::new("code-er-switch");
        scratch.write("db/schema.sql", CODE_ER_DDL);
        let project = scratch.root().to_string_lossy().to_string();
        let off =
            gx_index::BooksConfig::from_parts(Some("off".into()), Some(scratch.base.join("books")));

        let answer = crate::book::code_er_query_with(&off, Some(&project), "db/schema.sql", None);
        assert_eq!(answer["ok"], false, "{answer}");
        assert_eq!(
            answer["errors"][0]["code"],
            crate::book::DISABLED_CODE,
            "{answer}"
        );
    }

    /// The two chapters of the project-scope fixtures: `users` in one file and
    /// `posts`, whose foreign key points at `users`, in another. Per file the
    /// key vanishes; merged it anchors (the finding of f15 S1).
    const CODE_ER_USERS: &str = "\
CREATE TABLE users (\n\
  id INTEGER PRIMARY KEY,\n\
  name TEXT NOT NULL\n\
);\n";

    const CODE_ER_POSTS: &str = "\
CREATE TABLE posts (\n\
  id INTEGER PRIMARY KEY,\n\
  user_id INTEGER NOT NULL REFERENCES users(id),\n\
  title TEXT\n\
);\n";

    /// f15 S2/S3/S5: `scope: "project"` answers **one** document built from
    /// every `.sql` chapter of the book — the two tables of the two files, the
    /// cross-file foreign key anchored, and `files` counting what it parsed.
    /// The same call with `scope: "file"` is still today's one-file answer.
    #[test]
    fn code_er_project_merges_every_sql_chapter_into_one_document() {
        let scratch = BookScratch::new("code-er-project");
        scratch.write("db/users.sql", CODE_ER_USERS);
        scratch.write("db/posts.sql", CODE_ER_POSTS);
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();

        let merged = crate::book::code_er_query_with(
            &scratch.config(),
            Some(&project),
            "db/posts.sql",
            Some("project"),
        );
        assert_eq!(merged["ok"], true, "{merged}");
        // The named file stays the anchor of the answer, under both scopes.
        assert_eq!(merged["file"], "db/posts.sql", "{merged}");
        assert_eq!(merged["files"], 2, "the two chapters it parsed: {merged}");
        assert_eq!(merged["tables"], 2, "both chapters' tables: {merged}");
        assert_eq!(merged["relations"], 1, "the cross-file key: {merged}");

        let document = &merged["diagram"];
        let names: Vec<&str> = document["tables"]
            .as_array()
            .expect("the tables travel")
            .iter()
            .map(|table| table["name"].as_str().expect("a table has a name"))
            .collect();
        assert_eq!(
            names,
            ["posts", "users"],
            "the merge follows the book's chapter order: {merged}"
        );
        let relations = document["relations"]
            .as_array()
            .expect("the relations travel");
        assert_eq!(relations.len(), 1, "{merged}");
        assert_eq!(relations[0]["from"]["table"], "users", "{merged}");
        assert_eq!(relations[0]["from"]["column"], "id", "{merged}");
        assert_eq!(relations[0]["to"]["table"], "posts", "{merged}");
        assert_eq!(relations[0]["to"]["column"], "user_id", "{merged}");

        // S6: the file scope is exactly today's answer — one file in, the
        // bridge's own document out, no `files` and no relation to a table the
        // file does not carry.
        let single = crate::book::code_er_query_with(
            &scratch.config(),
            Some(&project),
            "db/posts.sql",
            Some("file"),
        );
        assert_eq!(single["ok"], true, "{single}");
        assert_eq!(single["tables"], 1, "{single}");
        assert_eq!(single["relations"], 0, "{single}");
        assert!(single["files"].is_null(), "no count of files: {single}");
        assert_eq!(
            single["diagram"]["tables"]
                .as_array()
                .expect("the table travels")
                .len(),
            1,
            "{single}"
        );
    }

    /// f15 S3: the chapter list is the book's, and a chapter whose file no
    /// longer reads is skipped — `files` counts only what was parsed, never
    /// what the book remembers.
    #[test]
    fn code_er_project_skips_a_chapter_that_no_longer_reads() {
        let scratch = BookScratch::new("code-er-project-gone");
        scratch.write("db/users.sql", CODE_ER_USERS);
        scratch.write("db/posts.sql", CODE_ER_POSTS);
        scratch.write(
            "db/tags.sql",
            "CREATE TABLE tags (\n  id INTEGER PRIMARY KEY,\n  label TEXT\n);\n",
        );
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();

        std::fs::remove_file(scratch.root().join("db/tags.sql")).expect("the chapter goes away");

        let merged = crate::book::code_er_query_with(
            &scratch.config(),
            Some(&project),
            "db/users.sql",
            Some("project"),
        );
        assert_eq!(merged["ok"], true, "{merged}");
        assert_eq!(
            merged["files"], 2,
            "only the chapters that parsed: {merged}"
        );
        assert_eq!(merged["tables"], 2, "{merged}");
        let names: Vec<&str> = merged["diagram"]["tables"]
            .as_array()
            .expect("the tables travel")
            .iter()
            .map(|table| table["name"].as_str().expect("a table has a name"))
            .collect();
        assert!(
            !names.contains(&"tags"),
            "the chapter that no longer reads contributes nothing: {merged}"
        );
    }

    /// f15 S2: `scope` accepts `file` or `project` and refuses anything else
    /// with `invalid_value` at `scope`, naming both values it takes.
    #[test]
    fn code_er_refuses_an_unknown_scope() {
        let scratch = BookScratch::new("code-er-scope");
        scratch.write("db/users.sql", CODE_ER_USERS);
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();

        for scope in ["nope", "files", "project2"] {
            let answer = crate::book::code_er_query_with(
                &scratch.config(),
                Some(&project),
                "db/users.sql",
                Some(scope),
            );
            assert_eq!(answer["ok"], false, "{scope}: {answer}");
            assert_eq!(
                answer["errors"][0]["code"], "invalid_value",
                "{scope}: {answer}"
            );
            assert_eq!(answer["errors"][0]["path"], "scope", "{scope}: {answer}");
            let hint = answer["errors"][0]["hint"]
                .as_str()
                .expect("the hint names the values it takes");
            assert!(hint.contains("file"), "{scope}: {answer}");
            assert!(hint.contains("project"), "{scope}: {answer}");
        }
    }

    /// f15 S6: an absent (or empty) `scope` is the `"file"` answer, byte for
    /// byte — the additive change never moves the default.
    #[test]
    fn the_absent_scope_answers_the_same_json_as_file_scope() {
        let scratch = BookScratch::new("code-er-scope-default");
        scratch.write("db/users.sql", CODE_ER_USERS);
        scratch.write("db/posts.sql", CODE_ER_POSTS);
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();

        let absent = crate::book::code_er_query_with(
            &scratch.config(),
            Some(&project),
            "db/posts.sql",
            None,
        );
        let empty = crate::book::code_er_query_with(
            &scratch.config(),
            Some(&project),
            "db/posts.sql",
            Some(""),
        );
        let file = crate::book::code_er_query_with(
            &scratch.config(),
            Some(&project),
            "db/posts.sql",
            Some("file"),
        );
        assert_eq!(absent, file, "absent is `file`, not another answer");
        assert_eq!(empty, file, "empty is `file`, not another answer");
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

    // -----------------------------------------------------------------
    // F9, T4/C3 — the answer envelope (L7)
    // -----------------------------------------------------------------

    /// A chapter of `count` top-level declarations: enough rows to pass the
    /// envelope of a small book, so the answer has to cliff its tail.
    fn write_many_rows(scratch: &BookScratch, count: usize) {
        let mut text = String::new();
        for index in 0..count {
            text.push_str(&format!("pub fn handler_{index:03}() {{}}\n"));
        }
        scratch.write("src/many.rs", &text);
    }

    /// A hub and its callers: a wide answer with one ranked head.
    fn write_hub(scratch: &BookScratch, callers: usize) {
        let mut text = String::from("pub fn hub() {}\n");
        for index in 0..callers {
            text.push_str(&format!("pub fn caller_{index:03}() {{ hub(); }}\n"));
        }
        scratch.write("src/fan.rs", &text);
    }

    /// The characters of one answer, as the agent's window pays for them.
    fn answer_chars(response: &serde_json::Value) -> usize {
        serde_json::to_string(response)
            .expect("the answer is JSON")
            .chars()
            .count()
    }

    /// L7: the envelope is the ladder of the book and its hard ceiling is 1.5×
    /// the rung: 151 indexed chapters are the second rung.
    #[test]
    fn the_envelope_reports_the_ladder_of_the_book() {
        let scratch = BookScratch::new("envelope-ladder");
        for index in 0..151 {
            scratch.write(
                &format!("src/f{index:03}.rs"),
                &format!("pub fn f{index:03}() {{}}\n"),
            );
        }
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();
        let ask = |max_chars: Option<usize>| {
            crate::book::code_structure_query_with(
                &scratch.config(),
                Some(&project),
                Some("src/f000.rs"),
                None,
                &gx_index::StructureRequest {
                    max_chars,
                    ..gx_index::StructureRequest::default()
                },
            )
        };

        let default = ask(None);
        assert_eq!(default["ok"], true, "{default}");
        assert_eq!(
            default["max_chars"], 18_000,
            "151 chapters are the second rung: {default}"
        );
        assert!(answer_chars(&default) <= 18_000, "{default}");

        let ceiling = ask(Some(1_000_000));
        assert_eq!(
            ceiling["max_chars"], 27_000,
            "the hard ceiling of the second rung: {ceiling}"
        );
    }

    /// L7: past the envelope the full rows stop and the tail becomes one-line
    /// pointers, with the row cursor of the answer still valid.
    #[test]
    fn the_structure_answer_cliffs_to_pointers_inside_the_envelope() {
        let scratch = BookScratch::new("envelope-structure");
        write_many_rows(&scratch, 180);
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();

        let response = crate::book::code_structure_query_with(
            &scratch.config(),
            Some(&project),
            Some("src/many.rs"),
            None,
            &gx_index::StructureRequest::default(),
        );
        assert_eq!(response["ok"], true, "{response}");
        assert_eq!(response["max_chars"], 13_000, "{response}");
        let chars = response["chars"]
            .as_u64()
            .expect("the answer says what it used") as usize;
        assert!(
            chars <= 13_000,
            "the notice stays inside the envelope: {response}"
        );
        assert!(
            answer_chars(&response) <= 13_000,
            "the window pays at most the envelope: {} > 13,000",
            answer_chars(&response)
        );
        assert_eq!(response["total"], 180, "{response}");
        assert_eq!(response["truncated"]["reason"], "budget", "{response}");

        let rows = response["symbols"].as_array().expect("the full rows");
        let pointers = response["pointers"].as_array().expect("the pointer tail");
        assert!(!rows.is_empty(), "the head keeps full rows: {response}");
        assert!(
            !pointers.is_empty(),
            "the tail cliffs to pointers: {response}"
        );
        assert!(rows.len() < 180, "{response}");
        assert!(
            pointers[0]["name"].is_string()
                && pointers[0]["line"].is_number()
                && pointers[0]["kind"].is_string(),
            "a pointer names one line: {response}"
        );
        assert!(
            pointers[0].get("column").is_none() && pointers[0].get("parent").is_none(),
            "a pointer is not a row: {response}"
        );
        assert_eq!(
            pointers[0]["line"].as_i64(),
            rows.last()
                .and_then(|row| row["line"].as_i64())
                .map(|line| line + 1),
            "the cliff is contiguous and after the full rows: {response}"
        );
        let shown = response["truncated"]["shown"]
            .as_u64()
            .expect("the notice says how many") as usize;
        assert_eq!(shown, rows.len() + pointers.len(), "{response}");
        assert_eq!(response["count"], shown, "{response}");
        assert!(shown < 180, "the whole tail did not fit: {response}");

        let next = &response["truncated"]["next"];
        assert_eq!(next["tool"], "code_structure", "{response}");
        assert_eq!(next["args"]["file"], "src/many.rs", "{response}");
        assert_eq!(
            next["args"]["max_chars"], 19_500,
            "the exact widening call: {response}"
        );
    }

    /// L7: the walk ranks and cliffs too: the head keeps the ranked nodes with
    /// their `id`, the tail becomes `{name, file, line}` pointers, and every
    /// edge connects nodes the answer fully carries.
    #[test]
    fn the_impact_answer_cliffs_to_pointers_inside_the_envelope() {
        let scratch = BookScratch::new("envelope-impact");
        write_hub(&scratch, 300);
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();

        let response = crate::book::code_impact_query_with(
            &scratch.config(),
            Some(&project),
            &crate::book::ImpactQuestion {
                symbol: Some("hub".to_string()),
                direction: Some("in".to_string()),
                ..crate::book::ImpactQuestion::default()
            },
        );
        assert_eq!(response["ok"], true, "{response}");
        assert_eq!(response["max_chars"], 13_000, "{response}");
        assert!(
            answer_chars(&response) <= 13_000,
            "the window pays at most the envelope: {} > 13,000",
            answer_chars(&response)
        );
        assert_eq!(response["truncated"]["reason"], "budget", "{response}");
        assert_eq!(response["truncated"]["total"], 301, "{response}");

        let nodes = response["nodes"].as_array().expect("the full nodes");
        let pointers = response["pointers"].as_array().expect("the pointer tail");
        assert_eq!(nodes[0]["name"], "hub", "the seed leads: {response}");
        assert!(!pointers.is_empty(), "{response}");
        assert!(nodes.len() + pointers.len() < 301, "{response}");
        assert_eq!(
            response["count"],
            nodes.len() + pointers.len(),
            "{response}"
        );
        assert_eq!(
            response["truncated"]["shown"], response["count"],
            "{response}"
        );
        assert_eq!(pointers[0]["file"], "src/fan.rs", "{response}");
        assert!(
            pointers[0].get("id").is_none() && pointers[0].get("qname").is_none(),
            "a pointer is compact: {response}"
        );

        let ids: std::collections::BTreeSet<&str> = nodes
            .iter()
            .filter_map(|node| node["id"].as_str())
            .collect();
        for edge in response["edges"].as_array().expect("the edges") {
            for end in ["from", "to"] {
                assert!(
                    ids.contains(edge[end].as_str().expect("an id")),
                    "an edge never dangles from a pointer: {response}"
                );
            }
        }
        assert_eq!(
            response["truncated"]["next"]["args"]["max_chars"], 19_500,
            "{response}"
        );
    }

    /// L7: the kept nodes are ranked by relevance — hop depth first, then the
    /// distinct edges that touch them — so a capped hub answer leads with what
    /// the walk found most connected instead of the alphabet.
    #[test]
    fn the_impact_answer_ranks_the_neighbours_by_degree() {
        let scratch = BookScratch::new("envelope-rank");
        scratch.write(
            "src/rank.rs",
            "pub fn hub() {}\n\
             pub fn zulu() { hub(); }\n\
             pub fn alpha() { hub(); }\n\
             pub fn z_one() { zulu(); }\n\
             pub fn z_two() { zulu(); }\n\
             pub fn z_three() { zulu(); }\n",
        );
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();

        let response = crate::book::code_impact_query_with(
            &scratch.config(),
            Some(&project),
            &crate::book::ImpactQuestion {
                symbol: Some("hub".to_string()),
                depth: Some(2),
                direction: Some("in".to_string()),
                ..crate::book::ImpactQuestion::default()
            },
        );
        assert_eq!(response["ok"], true, "{response}");
        let nodes = response["nodes"].as_array().expect("the nodes");
        assert_eq!(nodes[0]["name"], "hub", "the seed leads: {response}");
        let neighbours: Vec<&str> = nodes
            .iter()
            .filter(|node| node["depth"] == 1)
            .filter_map(|node| node["name"].as_str())
            .collect();
        assert_eq!(
            neighbours,
            ["zulu", "alpha"],
            "the degree ranks the answer, not the alphabet: {response}"
        );
        assert!(
            response["truncated"].is_null(),
            "the fixture fits the envelope: {response}"
        );
    }

    /// L7: `max_chars` overrides the rung, the hard ceiling clamps it, and `0`
    /// is a mistake the agent would read as «no answer»: all three tools say
    /// `invalid_value`.
    #[test]
    fn the_max_chars_override_is_clamped_and_zero_is_invalid_value() {
        let scratch = BookScratch::new("envelope-override");
        write_many_rows(&scratch, 180);
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();
        let ask = |max_chars: Option<usize>| {
            crate::book::code_structure_query_with(
                &scratch.config(),
                Some(&project),
                Some("src/many.rs"),
                None,
                &gx_index::StructureRequest {
                    max_chars,
                    ..gx_index::StructureRequest::default()
                },
            )
        };

        let tight = ask(Some(2_000));
        assert_eq!(tight["ok"], true, "{tight}");
        assert_eq!(tight["max_chars"], 2_000, "{tight}");
        assert!(answer_chars(&tight) <= 2_000, "{tight}");
        assert_eq!(tight["truncated"]["reason"], "budget", "{tight}");
        assert_eq!(
            tight["truncated"]["next"]["args"]["max_chars"], 19_500,
            "the widening call: {tight}"
        );

        let huge = ask(Some(1_000_000));
        assert_eq!(
            huge["max_chars"], 19_500,
            "the hard ceiling of the rung: {huge}"
        );

        let zero = ask(Some(0));
        assert_eq!(zero["ok"], false, "{zero}");
        assert_eq!(zero["errors"][0]["code"], "invalid_value", "{zero}");
        assert_eq!(zero["errors"][0]["path"], "max_chars", "{zero}");

        let impact = crate::book::code_impact_query_with(
            &scratch.config(),
            Some(&project),
            &crate::book::ImpactQuestion {
                symbol: Some("handler_000".to_string()),
                max_chars: Some(0),
                ..crate::book::ImpactQuestion::default()
            },
        );
        assert_eq!(impact["ok"], false, "{impact}");
        assert_eq!(impact["errors"][0]["path"], "max_chars", "{impact}");

        let diagram = crate::book::code_diagram_query_with(
            &scratch.config(),
            Some(&project),
            "handler_000",
            None,
            None,
            Some(0),
        );
        assert_eq!(diagram["ok"], false, "{diagram}");
        assert_eq!(diagram["errors"][0]["code"], "invalid_value", "{diagram}");
        assert_eq!(diagram["errors"][0]["path"], "max_chars", "{diagram}");
    }

    /// L7: the document is trimmed inside the envelope and still renders: the
    /// kind ceilings come first, the envelope takes nodes and edges after, and
    /// no edge is left pointing at a node the document dropped.
    #[test]
    fn the_diagram_stays_renderable_inside_the_envelope() {
        let scratch = BookScratch::new("envelope-diagram");
        write_hub(&scratch, 240);
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();

        let response = crate::book::code_diagram_query_with(
            &scratch.config(),
            Some(&project),
            "hub",
            Some(1),
            None,
            Some(3_000),
        );
        assert_eq!(response["ok"], true, "{response}");
        assert_eq!(response["max_chars"], 3_000, "{response}");
        assert!(
            answer_chars(&response) <= 3_000,
            "the window pays at most the envelope: {} > 3,000",
            answer_chars(&response)
        );
        assert_eq!(response["truncated"]["reason"], "budget", "{response}");
        assert_eq!(response["truncated"]["total"], 241, "{response}");

        let document = &response["diagram"];
        assert_eq!(document["kind"], "graph", "{response}");
        let nodes = document["nodes"].as_array().expect("the document nodes");
        assert!(!nodes.is_empty() && nodes.len() < 241, "{response}");
        let kept: std::collections::BTreeSet<&str> = nodes
            .iter()
            .filter_map(|node| node["id"].as_str())
            .collect();
        for edge in document["edges"].as_array().expect("the document edges") {
            for end in ["from", "to"] {
                assert!(
                    kept.contains(edge[end].as_str().expect("an id")),
                    "the trimmed document never dangles: {response}"
                );
            }
        }
        let validated = crate::contract::validate_diagram(&document.to_string());
        assert!(
            validated.ok,
            "the trimmed document still renders: {:?}",
            validated.errors
        );
        assert_eq!(response["nodes"], nodes.len(), "{response}");
        assert_eq!(response["truncated"]["shown"], nodes.len(), "{response}");
        assert_eq!(
            response["truncated"]["next"]["args"]["max_chars"], 19_500,
            "the wider document is one call away: {response}"
        );
    }

    /// L7: the node budget and the character envelope compose — whichever
    /// binds first cuts, and the reason says which one it was.
    #[test]
    fn the_budget_and_the_envelope_compose_and_the_reason_says_which_bound_first() {
        let scratch = BookScratch::new("envelope-compose");
        write_hub(&scratch, 60);
        scratch.indexed();
        let project = scratch.root().to_string_lossy().to_string();
        let ask = |budget: Option<usize>, max_chars: Option<usize>| {
            crate::book::code_impact_query_with(
                &scratch.config(),
                Some(&project),
                &crate::book::ImpactQuestion {
                    symbol: Some("hub".to_string()),
                    budget,
                    max_chars,
                    ..crate::book::ImpactQuestion::default()
                },
            )
        };

        let nodes = ask(Some(30), None);
        assert_eq!(nodes["truncated"]["reason"], "nodes", "{nodes}");
        assert_eq!(nodes["truncated"]["total"], 61, "{nodes}");
        assert_eq!(nodes["truncated"]["shown"], 30, "{nodes}");
        assert!(nodes["pointers"].is_null(), "{nodes}");

        let envelope = ask(None, Some(3_000));
        assert_eq!(
            envelope["truncated"]["reason"], "budget",
            "the envelope binds the complete walk: {envelope}"
        );
        assert_eq!(envelope["truncated"]["total"], 61, "{envelope}");
        assert!(answer_chars(&envelope) <= 3_000, "{envelope}");
        assert!(
            envelope["truncated"]["shown"].as_u64().unwrap() < 30,
            "{envelope}"
        );

        let both = ask(Some(30), Some(3_000));
        assert_eq!(
            both["truncated"]["reason"], "nodes",
            "the walk stopped first: {both}"
        );
        assert_eq!(both["truncated"]["total"], 61, "{both}");
        assert!(
            both["truncated"]["shown"].as_u64().unwrap() < 30,
            "the envelope also cut: {both}"
        );
        assert_eq!(
            both["truncated"]["next"]["args"]["max_chars"], 19_500,
            "the envelope is the tighter bound, so the next call widens it: {both}"
        );
    }

    /// T4/C3 (PRESERVE): an answer that fits the envelope keeps the shape T5
    /// published — the outline, its rows and `truncated: null` — and now
    /// reports the envelope it had and what it used.
    #[test]
    fn a_small_answer_keeps_its_shape_and_reports_the_envelope() {
        let scratch = BookScratch::new("envelope-preserve");
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
        assert_eq!(response["count"], 4, "{response}");
        assert_eq!(response["total"], 7, "{response}");
        assert_eq!(response["collapsed"]["hidden_symbols"], 3, "{response}");
        assert_eq!(response["truncated"]["reason"], "collapsed", "{response}");
        assert_eq!(response["truncated"]["shown"], 4, "{response}");
        assert!(
            response["pointers"].is_null(),
            "nothing was cliffed: {response}"
        );
        assert_eq!(response["max_chars"], 13_000, "{response}");
        let chars = response["chars"]
            .as_u64()
            .expect("the envelope is reported") as usize;
        assert!(chars <= 13_000, "{response}");
        assert!(
            answer_chars(&response) <= chars,
            "the estimate is an upper bound: {} > {chars}",
            answer_chars(&response)
        );
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

        // T4/C3: the envelope is a knob of the three tools and a fact of
        // every answer they publish.
        for tool in ["code_structure", "code_impact", "code_diagram"] {
            let schema = crate::dispatch::tool_schema(tool);
            assert!(
                schema["properties"]["max_chars"].is_object(),
                "{tool} takes `max_chars`: {schema}"
            );
        }

        let structure_text = crate::harness::description("code_structure");
        for needle in [
            "outline",
            "after_line",
            "collapsed",
            "truncated",
            "pointers",
        ] {
            assert!(
                structure_text.contains(needle),
                "code_structure lacks `{needle}`"
            );
        }
        let impact_text = crate::harness::description("code_impact");
        for needle in [
            "`file`",
            "budget",
            "edges",
            "files",
            "truncated",
            "pointers",
        ] {
            assert!(impact_text.contains(needle), "code_impact lacks `{needle}`");
        }
        let diagram_text = crate::harness::description("code_diagram");
        for needle in ["edges", "density", "max_chars"] {
            assert!(
                diagram_text.contains(needle),
                "code_diagram lacks `{needle}`"
            );
        }
        assert!(crate::harness::INSTRUCTIONS.contains("max_chars"));
        assert!(crate::harness::INSTRUCTIONS.contains("pointer"));
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
        for key in ["after_line", "limit", "kinds", "max_chars"] {
            assert!(
                structure["properties"].get(key).is_some(),
                "code_structure takes `{key}`: {structure:?}"
            );
        }
        let impact = advertised("code_impact");
        for key in ["file", "edges", "budget", "max_chars"] {
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
        for key in ["edges", "max_chars"] {
            assert!(
                diagram["properties"].get(key).is_some(),
                "code_diagram takes `{key}`: {diagram:?}"
            );
        }
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
