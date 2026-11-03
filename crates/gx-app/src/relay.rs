//! Relay of MCP calls to the process that owns the window (PRD §8.6).
//!
//! The owner of the socket serves a minimal JSON-RPC dispatcher: the
//! forwarded calls are the same tool calls the stdio server serves, so a
//! second MCP client gets the same answers from the same window.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{json, Value};

/// Counter used to build the session id of a forwarded connection.
static CONNECTION_COUNTER: AtomicU64 = AtomicU64::new(0);

/// The session id of a forwarded connection.
pub fn forwarded_session_id() -> String {
    let counter = CONNECTION_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("sock-{}-{}", std::process::id(), counter)
}

/// The schema of the `render_diagram` tool, as the MCP clients see it.
pub fn render_diagram_tool() -> Value {
    json!({
        "name": "render_diagram",
        "description": "Render a diagram on the Graphium plan. The diagram is a JSON document with a `kind` field; the kind must be available in the catalog.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "diagram": { "type": "object", "description": "The diagram document." },
                "opts": { "type": "object", "description": "Render options." }
            },
            "required": ["diagram"]
        }
    })
}

/// Handles one JSON-RPC line and returns the response line.
///
/// Notifications (requests without `id`) return an empty string: nothing is
/// written back to the client.
pub fn handle_request(line: &str, session_id: &str) -> String {
    let request: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(error) => return error_response(Value::Null, -32700, &format!("parse error: {error}")),
    };

    let id = request.get("id").cloned();
    let method = request
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();

    if id.is_none() {
        // Notification: no response.
        return String::new();
    }
    let id = id.unwrap_or(Value::Null);

    match method {
        "initialize" => success_response(
            id,
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "graphium", "version": env!("CARGO_PKG_VERSION") }
            }),
        ),
        "tools/list" => success_response(id, json!({ "tools": [render_diagram_tool()] })),
        "tools/call" => {
            let params = request.get("params").cloned().unwrap_or(json!({}));
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if name != "render_diagram" {
                return error_response(id, -32601, &format!("unknown tool: {name}"));
            }
            let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
            let document = arguments.get("diagram").cloned().unwrap_or(json!({}));
            let request = gx_mcp::tools::RenderRequest {
                document: document.to_string(),
                session_id: session_id.to_string(),
            };
            let response = gx_mcp::tools::render_diagram(&request);
            let payload = serde_json::to_value(&response).unwrap_or(json!({}));
            success_response(
                id,
                json!({
                    "content": [ { "type": "text", "text": payload.to_string() } ],
                    "isError": !response.ok
                }),
            )
        }
        other => error_response(id, -32601, &format!("method not found: {other}")),
    }
}

fn success_response(id: Value, result: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()
}

fn error_response(id: Value, code: i64, message: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
    .to_string()
}

/// Serves the socket until the process ends: one thread per connection.
pub fn serve(listener: UnixListener) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        std::thread::spawn(move || {
            let session_id = forwarded_session_id();
            serve_connection(stream, &session_id);
        });
    }
}

fn serve_connection(stream: UnixStream, session_id: &str) {
    let reader = BufReader::new(match stream.try_clone() {
        Ok(clone) => clone,
        Err(_) => return,
    });
    let mut writer = stream;
    for line in reader.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let response = handle_request(&line, session_id);
        if response.is_empty() {
            continue;
        }
        if writeln!(writer, "{response}").is_err() || writer.flush().is_err() {
            break;
        }
    }
}

/// Forwards the stdio of this process to the owner of the window.
///
/// Only requests with an `id` wait for an answer; notifications are relayed
/// without blocking.
pub fn forward(stream: UnixStream, input: &mut dyn BufRead, output: &mut dyn Write) {
    let mut writer = match stream.try_clone() {
        Ok(clone) => clone,
        Err(_) => return,
    };
    let mut reader = BufReader::new(stream);

    for line in input.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        if writeln!(writer, "{line}").is_err() || writer.flush().is_err() {
            break;
        }
        let expects_answer = serde_json::from_str::<Value>(&line)
            .ok()
            .and_then(|value| value.get("id").cloned())
            .is_some();
        if !expects_answer {
            continue;
        }
        let mut response = String::new();
        match reader.read_line(&mut response) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => break,
        }
        if writeln!(output, "{}", response.trim_end()).is_err() || output.flush().is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{handle_request, render_diagram_tool};
    use serde_json::Value;

    #[test]
    fn initialize_answers_with_the_server_info() {
        let response = handle_request(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#,
            "session-1",
        );
        let value: Value = serde_json::from_str(&response).expect("json");
        assert_eq!(value["id"], 1);
        assert_eq!(value["result"]["serverInfo"]["name"], "graphium");
    }

    #[test]
    fn notifications_get_no_answer() {
        let response = handle_request(
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            "session-1",
        );
        assert!(response.is_empty());
    }

    #[test]
    fn tools_list_returns_the_render_tool() {
        let response = handle_request(
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            "session-1",
        );
        let value: Value = serde_json::from_str(&response).expect("json");
        assert_eq!(value["result"]["tools"][0]["name"], "render_diagram");
    }

    #[test]
    fn unknown_methods_return_an_error() {
        let response = handle_request(r#"{"jsonrpc":"2.0","id":3,"method":"nope"}"#, "session-1");
        let value: Value = serde_json::from_str(&response).expect("json");
        assert_eq!(value["error"]["code"], -32601);
    }

    #[test]
    fn broken_json_returns_a_parse_error() {
        let response = handle_request("{not json", "session-1");
        let value: Value = serde_json::from_str(&response).expect("json");
        assert_eq!(value["error"]["code"], -32700);
    }

    #[test]
    fn the_tool_declares_its_description() {
        let tool = render_diagram_tool();
        assert!(tool["description"].as_str().unwrap().len() > 20);
        assert_eq!(tool["inputSchema"]["required"][0], "diagram");
    }
}
