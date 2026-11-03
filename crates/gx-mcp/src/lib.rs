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
pub mod harness;
pub mod store;
pub mod tools;

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
errors, warnings }. `ok: false` means nothing was placed and `errors[]` \
carries `{ path, code, message, hint? }` with a stable `code`; `ok: true` \
returns the new `diagram_id`, the `rendered_kind`, `metrics { nodes, edges, \
layout_ms, render_ms, svg_bytes }` and `rendered` telling whether the window \
confirmed the paint. `opts` is optional: `opts.extract: { around, depth? }` \
draws only the neighbourhood of one node (default `depth: 1`, at most 300 \
nodes) and the response carries `extracted_from: \"<diagram_id>@<depth>\"`; \
`opts.layout_timeout_ms` sets the layout deadline (default 2000, ceiling \
10000) and a layout that misses it answers `ok: false, code: \
\"layout_timeout\"` with `metrics.elapsed_ms`, leaving the plan untouched. \
`opts.focus`, `opts.replace_id` and `opts.interactive` arrive in F2/F3 and \
are ignored for now.";

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
id was lost, to find the diagram to update or export. It never renders \
anything.";

/// Description of the `get_diagram` tool, written for the agent.
const GET_DIAGRAM_DESCRIPTION: &str = "\
The source document of a diagram, with its current revision. Use it to read \
before an `update_diagram` (the revision is the one to send back), or to \
recover a document after a compaction. It never renders anything.";

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

/// Parameters of the `render_diagram` tool (PRD §11).
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenderDiagramParams {
    /// The complete diagram document, as a JSON object.
    pub diagram: serde_json::Value,
    /// Options of the render: `extract { around, depth? }` and
    /// `layout_timeout_ms`. `focus`, `replace_id` and `interactive` arrive in
    /// F2/F3 and are ignored for now.
    #[serde(default)]
    pub opts: Option<serde_json::Value>,
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
        Self::with_session_id("stdio")
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

    use super::{mcp_config_json, GraphiumServer, RenderDiagramParams};
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

    /// S21: the message of `opts_ignored` names the keys of F2/F3, so the
    /// agent learns when they arrive.
    #[tokio::test]
    async fn opts_ignored_names_the_keys_of_the_later_phases() {
        let _guard = lock_renders().await;
        let server = GraphiumServer::new();
        let diagram = serde_json::from_str(MINIMAL).expect("the example is JSON");

        let result = server
            .render_diagram(Parameters(RenderDiagramParams {
                diagram,
                opts: Some(serde_json::json!({ "focus": "read" })),
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
        for key in ["opts.focus", "opts.replace_id", "opts.interactive"] {
            assert!(message.contains(key), "the message names {key}: {message}");
        }
        assert!(message.contains("F2/F3"), "{message}");
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
}
