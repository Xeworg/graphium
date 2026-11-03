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

pub mod store;
pub mod tools;

#[cfg(test)]
mod test_support;

use std::path::Path;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_handler, tool_router, ErrorData, ServerHandler, ServiceExt};

/// Registers the kinds of `gx-kinds` in the global registry. Idempotent.
pub fn bootstrap() {
    static REGISTERED: std::sync::Once = std::sync::Once::new();
    REGISTERED.call_once(gx_kinds::register_all);
}

/// The JSON printed by `graphium mcp-config` (PRD §8.6.1, S15):
/// `{"command": "<absolute path of the binary>", "args": ["--stdio"]}`.
pub fn mcp_config_json(binary: &Path) -> String {
    // `gx-app` passes `current_exe()`, which is already absolute; a relative
    // path is resolved here so the printed config is always usable.
    let command = std::path::absolute(binary).unwrap_or_else(|_| binary.to_path_buf());
    // Built by hand so the key order of PRD §8.6.1 is byte-stable (a JSON
    // object does not depend on it, but the printed line does).
    let command = serde_json::Value::String(command.to_string_lossy().into_owned());
    format!("{{\"command\":{command},\"args\":[\"--stdio\"]}}")
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
    worker
        .join()
        .map_err(|_| -> Box<dyn std::error::Error> {
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
JSON: { ok, diagram_id, rendered_kind, rendered, metrics, errors, warnings }. \
`ok: false` means nothing was placed and `errors[]` carries `{ path, code, \
message, hint? }` with a stable `code`; `ok: true` returns the new \
`diagram_id`, the `rendered_kind`, `metrics { nodes, edges, layout_ms, \
render_ms, svg_bytes }` and `rendered` telling whether the window confirmed \
the paint. `opts` is accepted for forward compatibility and currently \
ignored.";

/// Parameters of the `render_diagram` tool (PRD §11).
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenderDiagramParams {
    /// The complete diagram document, as a JSON object.
    pub diagram: serde_json::Value,
    /// Reserved for later phases (`focus`, `extract`, `replace_id`,
    /// `interactive`); F0 honors nothing yet.
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

        // Validate + IR + layout + SVG run off the async loop (PRD §8.6.1).
        let mut response = tokio::task::spawn_blocking(move || tools::render_diagram(&request))
            .await
            .map_err(|error| {
                ErrorData::internal_error(format!("the render task failed: {error}"), None)
            })?;

        if params
            .opts
            .as_ref()
            .and_then(serde_json::Value::as_object)
            .is_some_and(|opts| !opts.is_empty())
        {
            response.warnings.push(tools::Warning {
                code: "opts_ignored".to_string(),
                message: "F0 accepts `opts` but honors no option yet".to_string(),
            });
        }

        let value = serde_json::to_value(&response).map_err(|error| {
            ErrorData::internal_error(format!("cannot serialize the response: {error}"), None)
        })?;
        Ok(CallToolResult::structured(value))
    }
}

#[tool_handler(
    name = "graphium",
    instructions = "Graphium draws software diagrams from a JSON document and places them on a plan. Call `render_diagram` with a complete diagram document and check `ok`: when it is false, fix the problems listed in `errors` (stable codes such as `invalid_json`, `unknown_kind`, `unknown_field`, `duplicate_id` and `unknown_ref`) and call again; when it is true the diagram is on the plan and `diagram_id` identifies it."
)]
impl ServerHandler for GraphiumServer {}

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
        let _guard = lock_renders();
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

        let _guard = lock_renders();
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
