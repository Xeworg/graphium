//! Relay of MCP calls to the process that owns the window (PRD §8.6).
//!
//! The owner of the socket serves a minimal JSON-RPC dispatcher: the
//! forwarded calls are the same tool calls the stdio server serves, so a
//! second MCP client gets the same answers from the same window.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};

/// Counter used to build the session id of a forwarded connection.
static CONNECTION_COUNTER: AtomicU64 = AtomicU64::new(0);

/// The session id of a forwarded connection.
pub fn forwarded_session_id() -> String {
    let counter = CONNECTION_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("sock-{}-{}", std::process::id(), counter)
}

/// The tools of the server, as a forwarded client sees them.
///
/// The list comes from the single dispatcher of `gx-mcp`, so a client that
/// reaches the owner through the socket sees the same tools as one that
/// speaks stdio to the owner directly.
pub fn tools() -> Vec<Value> {
    gx_mcp::dispatch::TOOL_NAMES
        .iter()
        .map(|name| {
            json!({
                "name": name,
                "description": gx_mcp::harness::description(name),
                "inputSchema": gx_mcp::dispatch::tool_schema(name)
            })
        })
        .collect()
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
        "tools/list" => success_response(id, json!({ "tools": tools() })),
        "tools/call" => {
            let params = request.get("params").cloned().unwrap_or(json!({}));
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
            match gx_mcp::dispatch::call_tool(name, &arguments, session_id) {
                Ok(payload) => {
                    let failed = payload.get("ok") == Some(&Value::Bool(false));
                    success_response(
                        id,
                        json!({
                            "content": [ { "type": "text", "text": payload.to_string() } ],
                            "structuredContent": payload,
                            "isError": failed
                        }),
                    )
                }
                Err(message) => error_response(id, -32602, &message),
            }
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

/// The maximum number of MCP sessions a single owner serves (PRD RNF9).
pub const MAX_SESSIONS: usize = 8;

/// Serves the socket until the process ends: one thread per connection.
pub fn serve(listener: UnixListener) {
    let sessions = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        // The ninth session is refused with `too_many_sessions` (PRD RNF9).
        if sessions.fetch_add(1, Ordering::SeqCst) >= MAX_SESSIONS {
            sessions.fetch_sub(1, Ordering::SeqCst);
            refuse(stream);
            continue;
        }
        let counter = Arc::clone(&sessions);
        std::thread::spawn(move || {
            let session_id = forwarded_session_id();
            serve_connection(stream, &session_id);
            counter.fetch_sub(1, Ordering::SeqCst);
        });
    }
}

/// Answers a session that exceeds the quota and closes it.
fn refuse(mut stream: UnixStream) {
    let error = json!({
        "jsonrpc": "2.0",
        "id": Value::Null,
        "error": {
            "code": -32000,
            "message": "too_many_sessions",
            "data": { "code": "too_many_sessions", "limit": MAX_SESSIONS }
        }
    });
    let _ = writeln!(stream, "{error}");
    let _ = stream.flush();
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
    use super::{handle_request, tools};
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
    fn tools_list_returns_every_tool() {
        let response = handle_request(
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            "session-1",
        );
        let value: Value = serde_json::from_str(&response).expect("json");
        let names: Vec<&str> = value["result"]["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect();
        assert!(names.contains(&"render_diagram"));
        assert!(names.contains(&"validate_diagram"));
        assert!(names.contains(&"list_diagrams"));
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
    fn every_tool_is_forwarded_with_its_description() {
        let tools = tools();
        assert_eq!(tools.len(), gx_mcp::dispatch::TOOL_NAMES.len());
        for tool in &tools {
            assert!(tool["description"].as_str().unwrap().len() > 20, "{tool}");
            assert!(tool["inputSchema"].is_object(), "{tool}");
        }
        let render = tools
            .iter()
            .find(|tool| tool["name"] == "render_diagram")
            .expect("render");
        assert_eq!(render["inputSchema"]["required"][0], "diagram");
    }
}
