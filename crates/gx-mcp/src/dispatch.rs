//! One dispatcher for the tools of Graphium.
//!
//! The stdio server reaches the tools through the typed methods of rmcp and
//! the single instance relay reaches them through this dispatcher. Both end
//! in the same functions, so a tool exists once: a second MCP client
//! forwarded to the owner of the window gets exactly the same answers.

use serde_json::{json, Value};

use crate::contract;

/// Every tool of the server, in the order the index reports them.
pub const TOOL_NAMES: [&str; 7] = [
    "render_diagram",
    "validate_diagram",
    "get_diagram_schema",
    "list_diagrams",
    "get_diagram",
    "update_diagram",
    "clear_canvas",
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
            let response = crate::tools::render_diagram(&crate::tools::RenderRequest {
                document: document.to_string(),
                session_id: session_id.to_string(),
            });
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
        "clear_canvas" => {
            let scope = arguments.get("scope").and_then(Value::as_str);
            let response = contract::clear_canvas(session_id, scope);
            serde_json::to_value(&response).map_err(|error| error.to_string())
        }
        other => Err(format!("unknown tool: {other}")),
    }
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
        "clear_canvas" => json!({
            "type": "object",
            "properties": { "scope": { "type": ["string", "null"] } }
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

    #[test]
    fn the_lifecycle_is_reachable_through_the_dispatcher() {
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
}
