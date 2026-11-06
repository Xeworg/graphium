//! One dispatcher for the tools of Graphium.
//!
//! The stdio server reaches the tools through the typed methods of rmcp and
//! the single instance relay reaches them through this dispatcher. Both end
//! in the same functions, so a tool exists once: a second MCP client
//! forwarded to the owner of the window gets exactly the same answers.

use serde_json::{json, Value};

use crate::contract;

/// Every tool of the server, in the order the index reports them.
pub const TOOL_NAMES: [&str; 15] = [
    "render_diagram",
    "validate_diagram",
    "get_diagram_schema",
    "list_diagrams",
    "get_diagram",
    "update_diagram",
    "export_diagram",
    "list_diagram_versions",
    "get_diagram_version",
    "clear_canvas",
    "register_session",
    "code_search",
    "code_structure",
    "code_impact",
    "code_diagram",
];

/// Calls a tool by name with its JSON arguments.
///
/// `session_id` is the session that owns the call: it scopes the diagrams
/// (PRD §11.1). An unknown tool is an error, never a silent success.
pub fn call_tool(name: &str, arguments: &Value, session_id: &str) -> Result<Value, String> {
    crate::bootstrap();

    match name {
        "render_diagram" => {
            let document = arguments
                .get("diagram")
                .ok_or_else(|| "`diagram` is required".to_string())?;
            // The same implementation the stdio server uses: `opts` cannot
            // work on one path and be ignored on the other.
            // The indexer parses with one thread while a render is in flight
            // (ideas §10.1, «Backpressure»).
            let _render = gx_index::service::RenderGuard::enter();
            let response = crate::tools::render_diagram_from_json(
                &crate::tools::RenderRequest {
                    document: document.to_string(),
                    session_id: session_id.to_string(),
                },
                arguments.get("opts"),
            );
            serde_json::to_value(&response).map_err(|error| error.to_string())
        }
        "validate_diagram" => {
            let document = arguments
                .get("diagram")
                .ok_or_else(|| "`diagram` is required".to_string())?;
            let response = contract::validate_diagram(&document.to_string());
            serde_json::to_value(&response).map_err(|error| error.to_string())
        }
        "get_diagram_schema" => {
            let kind = arguments.get("kind").and_then(Value::as_str);
            let response = contract::get_diagram_schema(kind);
            serde_json::to_value(&response).map_err(|error| error.to_string())
        }
        "list_diagrams" => {
            let response = contract::list_diagrams(session_id);
            serde_json::to_value(&response).map_err(|error| error.to_string())
        }
        "get_diagram" => {
            let diagram_id = arguments
                .get("diagram_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "`diagram_id` is required".to_string())?;
            let response = contract::get_diagram(session_id, diagram_id);
            serde_json::to_value(&response).map_err(|error| error.to_string())
        }
        "update_diagram" => {
            let diagram_id = arguments
                .get("diagram_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "`diagram_id` is required".to_string())?;
            let patch = arguments
                .get("patch")
                .ok_or_else(|| "`patch` is required".to_string())?;
            let revision = arguments.get("revision").and_then(Value::as_u64);
            let response = contract::update_diagram(session_id, diagram_id, patch, revision);
            serde_json::to_value(&response).map_err(|error| error.to_string())
        }
        "export_diagram" => {
            let format = arguments
                .get("format")
                .and_then(Value::as_str)
                .ok_or_else(|| "`format` is required".to_string())?;
            let opts = arguments.get("opts").cloned().unwrap_or(Value::Null);
            // The same implementation the stdio server uses, with the same
            // reading of `opts`: a forwarded client cannot get a different
            // export than a local one.
            let request = crate::export::ExportRequest {
                session_id: session_id.to_string(),
                diagram_id: arguments
                    .get("diagram_id")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                format: format.to_string(),
                inline: opts.get("inline").and_then(Value::as_bool).unwrap_or(false),
                path: opts.get("path").and_then(Value::as_str).map(str::to_string),
                overwrite: opts
                    .get("overwrite")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                scale: opts.get("scale").and_then(Value::as_f64),
                max_px: opts.get("max_px").and_then(Value::as_u64),
            };
            let response = crate::export::export_diagram(&request);
            serde_json::to_value(&response).map_err(|error| error.to_string())
        }
        "list_diagram_versions" => {
            let diagram_id = arguments
                .get("diagram_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "`diagram_id` is required".to_string())?;
            let response = contract::list_diagram_versions(session_id, diagram_id);
            serde_json::to_value(&response).map_err(|error| error.to_string())
        }
        "get_diagram_version" => {
            let diagram_id = arguments
                .get("diagram_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "`diagram_id` is required".to_string())?;
            let version = arguments
                .get("version")
                .and_then(Value::as_u64)
                .ok_or_else(|| "`version` is required".to_string())?;
            let response = contract::get_diagram_version(session_id, diagram_id, version);
            serde_json::to_value(&response).map_err(|error| error.to_string())
        }
        "clear_canvas" => {
            let scope = arguments.get("scope").and_then(Value::as_str);
            let response = contract::clear_canvas(session_id, scope);
            serde_json::to_value(&response).map_err(|error| error.to_string())
        }
        "register_session" => {
            let label = arguments.get("label").and_then(Value::as_str);
            let response = contract::register_session(session_id, label);
            serde_json::to_value(&response).map_err(|error| error.to_string())
        }
        "code_search" => {
            let query = arguments
                .get("query")
                .and_then(Value::as_str)
                .ok_or_else(|| "`query` is required".to_string())?;
            // The same implementation the stdio server uses, with the same
            // reading of the session: a forwarded client searches the book of
            // its own project, never another one (D4).
            Ok(crate::book::code_search(session_id, query))
        }
        "code_structure" => {
            // Exactly one selector: `file` or `symbol`. Both are optional in
            // the schema and the tool decides, so an empty call is the same
            // answer on both doors.
            let file = arguments.get("file").and_then(Value::as_str);
            let symbol = arguments.get("symbol").and_then(Value::as_str);
            let request = gx_index::StructureRequest {
                after_line: arguments.get("after_line").and_then(Value::as_i64),
                limit: arguments
                    .get("limit")
                    .and_then(Value::as_u64)
                    .map(|limit| usize::try_from(limit).unwrap_or(usize::MAX)),
                kinds: string_array(arguments.get("kinds")).unwrap_or_default(),
            };
            Ok(crate::book::code_structure(
                session_id, file, symbol, &request,
            ))
        }
        "code_impact" => {
            // The seed grammar (one of `symbol` or `file`) belongs to the
            // tool: the relay answers the same `invalid_value` the stdio
            // server does instead of a transport error.
            let question = crate::book::ImpactQuestion {
                symbol: arguments
                    .get("symbol")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                file: arguments
                    .get("file")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                depth: arguments
                    .get("depth")
                    .and_then(Value::as_u64)
                    .map(|depth| depth.min(u32::MAX as u64) as u32),
                direction: arguments
                    .get("direction")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                edges: string_array(arguments.get("edges")),
                budget: arguments
                    .get("budget")
                    .and_then(Value::as_u64)
                    .map(|budget| usize::try_from(budget).unwrap_or(usize::MAX)),
            };
            Ok(crate::book::code_impact(session_id, &question))
        }
        "code_diagram" => {
            let symbol = arguments
                .get("symbol")
                .and_then(Value::as_str)
                .ok_or_else(|| "`symbol` is required".to_string())?;
            let depth = arguments
                .get("depth")
                .and_then(Value::as_u64)
                .map(|depth| depth.min(u32::MAX as u64) as u32);
            let edges = string_array(arguments.get("edges"));
            Ok(crate::book::code_diagram(
                session_id,
                symbol,
                depth,
                edges.as_deref(),
            ))
        }
        other => Err(format!("unknown tool: {other}")),
    }
}

/// A JSON array of strings, or `None` when the key is absent.
fn string_array(value: Option<&Value>) -> Option<Vec<String>> {
    value.and_then(Value::as_array).map(|values| {
        values
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect()
    })
}

/// The schema of the arguments of a tool, as the MCP clients see it.
pub fn tool_schema(name: &str) -> Value {
    match name {
        "render_diagram" => json!({
            "type": "object",
            "properties": {
                "diagram": { "type": "object", "description": "The diagram document." },
                "opts": { "type": "object", "description": "Render options." }
            },
            "required": ["diagram"]
        }),
        "validate_diagram" => json!({
            "type": "object",
            "properties": {
                "diagram": { "type": "object", "description": "The diagram document." }
            },
            "required": ["diagram"]
        }),
        "get_diagram_schema" => json!({
            "type": "object",
            "properties": {
                "kind": { "type": ["string", "null"], "description": "The kind to describe." }
            }
        }),
        "list_diagrams" => json!({ "type": "object", "properties": {} }),
        "get_diagram" => json!({
            "type": "object",
            "properties": { "diagram_id": { "type": "string" } },
            "required": ["diagram_id"]
        }),
        "update_diagram" => json!({
            "type": "object",
            "properties": {
                "diagram_id": { "type": "string" },
                "patch": { "type": "object" },
                "revision": { "type": ["integer", "null"] }
            },
            "required": ["diagram_id", "patch"]
        }),
        "export_diagram" => json!({
            "type": "object",
            "properties": {
                "diagram_id": { "type": ["string", "null"] },
                "format": { "type": "string", "enum": ["svg", "png"] },
                "opts": {
                    "type": ["object", "null"],
                    "properties": {
                        "inline": { "type": ["boolean", "null"] },
                        "path": { "type": ["string", "null"] },
                        "overwrite": { "type": ["boolean", "null"] },
                        "scale": { "type": ["number", "null"] },
                        "max_px": { "type": ["integer", "null"] }
                    }
                }
            },
            "required": ["format"]
        }),
        "list_diagram_versions" => json!({
            "type": "object",
            "properties": { "diagram_id": { "type": "string" } },
            "required": ["diagram_id"]
        }),
        "get_diagram_version" => json!({
            "type": "object",
            "properties": {
                "diagram_id": { "type": "string" },
                "version": { "type": "integer" }
            },
            "required": ["diagram_id", "version"]
        }),
        "clear_canvas" => json!({
            "type": "object",
            "properties": { "scope": { "type": ["string", "null"] } }
        }),
        "register_session" => json!({
            "type": "object",
            "properties": {
                "label": {
                    "type": ["string", "null"],
                    "description": "The name the human sees for this session; it is trimmed. Without it the tool only answers the session. An empty or whitespace-only label is `invalid_value`."
                }
            }
        }),
        "code_search" => json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The symbol to look for: one or more words of its name or qname. Every word has to match; an ambiguous query answers `ambiguous_symbol` with `candidates[]`."
                }
            },
            "required": ["query"]
        }),
        "code_structure" => json!({
            "type": "object",
            "properties": {
                "file": {
                    "type": ["string", "null"],
                    "description": "The path of the chapter, relative to the project root, with `/` separators."
                },
                "symbol": {
                    "type": ["string", "null"],
                    "description": "The symbol whose chapter to read: a name, as `code_search` takes it. Send exactly one of `file` or `symbol`."
                },
                "after_line": {
                    "type": ["integer", "null"],
                    "description": "Start strictly after this line: with `limit` it is a range over the span-ordered list of the chapter. Without it the answer is the outline."
                },
                "limit": {
                    "type": ["integer", "null"],
                    "description": "How many rows the answer carries: 200 by default and 200 at most."
                },
                "kinds": {
                    "type": ["array", "null"],
                    "items": {
                        "type": "string",
                        "enum": ["fn", "method", "type", "class", "trait", "impl", "module", "const", "static"]
                    },
                    "description": "The declaration kinds to answer; `total` keeps counting the whole chapter."
                }
            }
        }),
        "code_impact" => json!({
            "type": "object",
            "properties": {
                "symbol": {
                    "type": ["string", "null"],
                    "description": "The symbol to start from: a name, as `code_search` takes it. Send exactly one of `symbol` or `file`."
                },
                "file": {
                    "type": ["string", "null"],
                    "description": "The chapter to walk as a node: the connections view, grouped by file. Send exactly one of `symbol` or `file`."
                },
                "depth": {
                    "type": ["integer", "null"],
                    "description": "Hops to follow: 1 by default, clamped to 3."
                },
                "direction": {
                    "type": ["string", "null"],
                    "enum": ["in", "out", "both", null],
                    "description": "`in` (default of a symbol): the edges that point at it; `out`: what it reaches; `both` (default of a file)."
                },
                "edges": {
                    "type": ["array", "null"],
                    "items": {
                        "type": "string",
                        "enum": ["calls", "imports", "inherits", "uses"]
                    },
                    "description": "The edge kinds the walk follows; omitted follows all four."
                },
                "budget": {
                    "type": ["integer", "null"],
                    "description": "The query budget in nodes: 600 by default and 2000 at most (the edges are twice the nodes)."
                }
            }
        }),
        "code_diagram" => json!({
            "type": "object",
            "properties": {
                "symbol": {
                    "type": "string",
                    "description": "The symbol to draw the neighbourhood of: a name, as `code_search` takes it."
                },
                "depth": {
                    "type": ["integer", "null"],
                    "description": "Hops to follow: 1 by default, clamped to 3."
                },
                "edges": {
                    "type": ["array", "null"],
                    "items": {
                        "type": "string",
                        "enum": ["calls", "imports", "inherits", "uses"]
                    },
                    "description": "The edge kinds the walk follows; omitted follows all four."
                }
            },
            "required": ["symbol"]
        }),
        _ => json!({ "type": "object" }),
    }
}

#[cfg(test)]
mod tests {
    use super::call_tool;

    const MINIMAL: &str = r#"{
      "version": 1, "kind": "graph",
      "nodes": [ { "id": "a", "label": "A" }, { "id": "b", "label": "B" } ],
      "edges": [ { "from": "a", "to": "b" } ]
    }"#;

    #[test]
    fn an_unknown_tool_is_an_error() {
        let error = call_tool("nope", &serde_json::json!({}), "s").expect_err("must fail");
        assert!(error.contains("unknown tool"));
    }

    #[test]
    fn validate_and_schema_are_reachable_through_the_dispatcher() {
        let validated = call_tool(
            "validate_diagram",
            &serde_json::json!({ "diagram": serde_json::from_str::<serde_json::Value>(MINIMAL).unwrap() }),
            "s",
        )
        .expect("valid");
        assert_eq!(validated["ok"], true);

        let schema = call_tool("get_diagram_schema", &serde_json::json!({}), "s").expect("schema");
        assert_eq!(schema["ok"], true);
        assert!(schema["kinds"]
            .as_array()
            .is_some_and(|kinds| !kinds.is_empty()));
    }

    /// PRD §7.3, S21: an agent that reaches the window owner through the
    /// socket gets the same answer as one that speaks stdio to it. Before
    /// this test the relay silently dropped `opts`, so `extract` and
    /// `layout_timeout_ms` worked on one path and vanished on the other.
    #[test]
    fn the_relay_honours_opts_like_the_stdio_server() {
        let _guard = crate::test_support::lock_renders_blocking();

        let document = serde_json::from_str::<serde_json::Value>(MINIMAL).unwrap();
        let narrowed = call_tool(
            "render_diagram",
            &serde_json::json!({
                "diagram": document,
                "opts": { "extract": { "around": "a", "depth": 1 } }
            }),
            "session-relay",
        )
        .expect("rendered");
        assert_eq!(narrowed["ok"], true, "{narrowed}");
        assert!(
            narrowed["extracted_from"].is_string(),
            "the relay has to honour opts.extract: {narrowed}"
        );

        let ignored = call_tool(
            "render_diagram",
            &serde_json::json!({
                "diagram": document,
                "opts": { "interactive": false }
            }),
            "session-relay",
        )
        .expect("rendered");
        assert_eq!(ignored["ok"], true, "{ignored}");
        assert!(
            ignored["warnings"]
                .as_array()
                .is_some_and(|warnings| warnings
                    .iter()
                    .any(|warning| warning["code"] == "opts_ignored")),
            "the relay has to report the keys of the later phases: {ignored}"
        );
    }

    /// S1-S3, S5 through the relay: a forwarded client exports exactly like a
    /// local one. Without this arm in the dispatcher the tool would exist in
    /// the stdio server and be invisible to the window owner's socket.
    #[test]
    fn the_export_is_reachable_through_the_dispatcher() {
        let document = serde_json::json!({
            "version": 1, "kind": "graph",
            "nodes": [ { "id": "a", "label": "A" }, { "id": "b", "label": "B" } ],
            "edges": [ { "from": "a", "to": "b" } ]
        });
        // The store and the engines are process-wide: without the lock, a
        // test that resets them runs in between and the export sees no
        // diagram at all.
        let _guard = crate::test_support::lock_renders_blocking();

        call_tool(
            "render_diagram",
            &serde_json::json!({ "diagram": document }),
            "session-export",
        )
        .expect("rendered");

        let exported = call_tool(
            "export_diagram",
            &serde_json::json!({ "format": "svg", "opts": { "inline": true } }),
            "session-export",
        )
        .expect("exported");
        assert_eq!(exported["ok"], true, "{exported}");
        let content = exported["content"].as_str().expect("the SVG travels");
        assert!(content.starts_with("<svg"), "{content}");
        assert!(
            content.contains("class=\"bg\""),
            "the forwarded export paints its canvas too: {content}"
        );

        // The ceilings are the same on both paths.
        let wrong = call_tool(
            "export_diagram",
            &serde_json::json!({ "format": "png", "opts": { "inline": true } }),
            "session-export",
        )
        .expect("answered");
        assert_eq!(wrong["ok"], false, "{wrong}");
        assert_eq!(wrong["errors"][0]["code"], "invalid_value", "{wrong}");
    }

    /// S5/R2: `opts.replace_id` also arrives through the relay, because the
    /// dispatcher hands the raw `opts` to the same implementation the stdio
    /// server uses. A forwarded client therefore replaces instead of piling
    /// up diagrams, exactly like a local one.
    #[test]
    fn the_relay_honours_replace_id() {
        let _guard = crate::test_support::lock_renders_blocking();

        let document = serde_json::from_str::<serde_json::Value>(MINIMAL).unwrap();
        let first = call_tool(
            "render_diagram",
            &serde_json::json!({ "diagram": document }),
            "session-replace",
        )
        .expect("rendered");
        let diagram_id = first["diagram_id"]
            .as_str()
            .expect("the render names the diagram")
            .to_string();

        let replaced = call_tool(
            "render_diagram",
            &serde_json::json!({
                "diagram": { "version": 1, "kind": "graph", "title": "reemplazado",
                    "nodes": [{ "id": "a", "label": "A" }], "edges": [] },
                "opts": { "replace_id": diagram_id }
            }),
            "session-replace",
        )
        .expect("rendered");
        assert_eq!(replaced["ok"], true, "{replaced}");
        assert_eq!(replaced["diagram_id"], diagram_id.as_str(), "{replaced}");
        assert_eq!(replaced["replaced"], diagram_id.as_str(), "{replaced}");

        let listed =
            call_tool("list_diagrams", &serde_json::json!({}), "session-replace").expect("listed");
        let diagrams = listed["diagrams"].as_array().expect("diagrams");
        assert_eq!(
            diagrams.len(),
            1,
            "the relay replaced, it did not pile up: {listed}"
        );
        assert_eq!(diagrams[0]["title"], "reemplazado", "{listed}");
    }

    #[test]
    fn the_lifecycle_is_reachable_through_the_dispatcher() {
        // The global store is shared: the test that renders serializes with
        // the rest of the suite.
        let _guard = crate::test_support::lock_renders_blocking();

        let rendered = call_tool(
            "render_diagram",
            &serde_json::json!({ "diagram": serde_json::from_str::<serde_json::Value>(MINIMAL).unwrap() }),
            "session-dispatch",
        )
        .expect("rendered");
        assert_eq!(rendered["ok"], true);
        let id = rendered["diagram_id"].as_str().expect("id").to_string();

        let listed =
            call_tool("list_diagrams", &serde_json::json!({}), "session-dispatch").expect("listed");
        assert!(listed["diagrams"]
            .as_array()
            .is_some_and(|diagrams| diagrams
                .iter()
                .any(|entry| entry["diagram_id"] == id.as_str())));

        let got = call_tool(
            "get_diagram",
            &serde_json::json!({ "diagram_id": id }),
            "session-dispatch",
        )
        .expect("got");
        assert_eq!(got["ok"], true);

        let updated = call_tool(
            "update_diagram",
            &serde_json::json!({ "diagram_id": id, "patch": { "title": "x" } }),
            "session-dispatch",
        )
        .expect("updated");
        assert_eq!(updated["applied"], true);

        let cleared =
            call_tool("clear_canvas", &serde_json::json!({}), "session-dispatch").expect("cleared");
        assert_eq!(cleared["ok"], true);
    }

    /// S3/D2: naming the session is reachable through the dispatcher too, so
    /// a client forwarded through the socket names its session exactly as one
    /// that speaks stdio, and the empty label is the same code on both paths.
    #[test]
    fn register_session_is_reachable_through_the_dispatcher() {
        let session = format!("dispatch-session-{}", std::process::id());

        let named = call_tool(
            "register_session",
            &serde_json::json!({ "label": "  presupuesto API  " }),
            &session,
        )
        .expect("answered");
        assert_eq!(named["ok"], true, "{named}");
        assert_eq!(named["session"]["label"], "presupuesto API", "{named}");
        assert_eq!(named["session"]["state"], "active", "{named}");
        let answered_id = named["session"]["id"]
            .as_str()
            .expect("the session names its id")
            .to_string();
        assert!(!answered_id.is_empty(), "{named}");

        // Without a label it answers the session it kept (`whoami`).
        let whoami =
            call_tool("register_session", &serde_json::json!({}), &session).expect("answered");
        assert_eq!(whoami["session"]["label"], "presupuesto API", "{whoami}");
        assert_eq!(whoami["session"]["id"], answered_id, "{whoami}");

        let empty = call_tool(
            "register_session",
            &serde_json::json!({ "label": "   " }),
            &session,
        )
        .expect("answered");
        assert_eq!(empty["ok"], false, "{empty}");
        assert_eq!(empty["errors"][0]["code"], "invalid_value", "{empty}");
        assert_eq!(empty["errors"][0]["path"], "label", "{empty}");
        assert!(empty["errors"][0]["hint"].is_string(), "{empty}");
    }

    /// RF14/R3: the history is readable through the relay too, because the
    /// dispatcher is the second door to the same `contract` functions a stdio
    /// client reaches.
    #[test]
    fn the_history_is_reachable_through_the_dispatcher() {
        let _guard = crate::test_support::lock_renders_blocking();

        let document = serde_json::from_str::<serde_json::Value>(MINIMAL).unwrap();
        let rendered = call_tool(
            "render_diagram",
            &serde_json::json!({ "diagram": document }),
            "session-history",
        )
        .expect("rendered");
        let diagram_id = rendered["diagram_id"]
            .as_str()
            .expect("the render names the diagram")
            .to_string();

        call_tool(
            "update_diagram",
            &serde_json::json!({ "diagram_id": diagram_id, "patch": { "title": "segunda" } }),
            "session-history",
        )
        .expect("updated");

        let listed = call_tool(
            "list_diagram_versions",
            &serde_json::json!({ "diagram_id": diagram_id }),
            "session-history",
        )
        .expect("listed");
        assert_eq!(listed["ok"], true, "{listed}");
        assert_eq!(listed["current_version"], 2, "{listed}");
        assert_eq!(
            listed["versions"].as_array().expect("versions").len(),
            2,
            "{listed}"
        );

        let first = call_tool(
            "get_diagram_version",
            &serde_json::json!({ "diagram_id": diagram_id, "version": 1 }),
            "session-history",
        )
        .expect("read");
        assert_eq!(first["ok"], true, "{first}");
        assert_eq!(
            first["document"]["nodes"][0]["id"], "a",
            "the first version is the document the render stored: {first}"
        );

        let missing = call_tool(
            "get_diagram_version",
            &serde_json::json!({ "diagram_id": diagram_id, "version": 9 }),
            "session-history",
        )
        .expect("answered");
        assert_eq!(missing["ok"], false, "{missing}");
        assert_eq!(missing["errors"][0]["code"], "invalid_value", "{missing}");
    }

    /// The schema the relay publishes for the new tool: `label` is optional
    /// and, when it comes, it is a string.
    #[test]
    fn the_schema_of_register_session_has_an_optional_label() {
        let schema = super::tool_schema("register_session");
        assert!(schema["properties"]["label"].is_object(), "{schema}");
        let required = schema.get("required").and_then(serde_json::Value::as_array);
        assert!(
            required.is_none_or(|required| required.is_empty()),
            "`label` is optional: {schema}"
        );
    }
}
