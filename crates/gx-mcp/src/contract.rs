//! The contract of the tools: validation and schema (PRD §7 RF3/RF5, §11).
//!
//! `validate_diagram` is the dry run of `render_diagram`: the very same
//! pipeline (registry → schema → `validate()`), without layout and without
//! rendering. An agent uses it to fix a document before opening a window.

use serde::Serialize;
use serde_json::Value;

use gx_core::error::{ErrorCode, ValidationError};

/// The maximum number of errors a tool reports (PRD §7 RF5).
pub const MAX_ERRORS: usize = 20;

/// A validation problem with its stable code and JSON path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ErrorPayload {
    pub path: String,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl From<&ValidationError> for ErrorPayload {
    fn from(error: &ValidationError) -> Self {
        Self {
            path: error.path.clone(),
            code: error.code.as_str().to_string(),
            message: error.message.clone(),
            hint: error.hint.clone(),
        }
    }
}

/// A non-blocking warning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Warning {
    pub code: String,
    pub message: String,
}

impl Warning {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

/// The response of `validate_diagram` (PRD §11).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ValidateResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<ErrorPayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warnings: Option<Vec<Warning>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projected_svg_bytes: Option<usize>,
    /// True when there were more errors than the cap (PRD §7 RF5).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub errors_truncated: bool,
    /// The contract versions this build understands (PRD §7 RF5).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub supported_versions: Vec<u32>,
}

/// Validates a diagram document without rendering it.
///
/// The pipeline is exactly the one of `render_diagram` (PRD §11.1, regla de
/// oro): the kind is resolved through the registry, the document is checked
/// against the schema of that kind and then against the cross rules. The
/// only thing missing is the layout and the render.
pub fn validate_diagram(document: &str) -> ValidateResponse {
    crate::bootstrap();

    let parsed: Value = match serde_json::from_str(document) {
        Ok(value) => value,
        Err(error) => {
            return ValidateResponse {
                ok: false,
                errors: vec![ErrorPayload {
                    path: String::new(),
                    code: ErrorCode::InvalidJson.as_str().to_string(),
                    message: format!("JSON inválido: {error}"),
                    hint: Some(
                        "el documento debe ser un objeto JSON con el campo `kind`".to_string(),
                    ),
                }],
                warnings: None,
                projected_svg_bytes: None,
                errors_truncated: false,
                supported_versions: Vec::new(),
            }
        }
    };

    let kind_name = parsed
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let kind = match gx_core::kind::for_kind(&kind_name) {
        Some(kind) => kind,
        None => {
            let available = gx_core::kind::kind_names().join(", ");
            return ValidateResponse {
                ok: false,
                errors: vec![ErrorPayload {
                    path: "kind".to_string(),
                    code: ErrorCode::UnknownKind.as_str().to_string(),
                    message: format!("kind desconocido: `{kind_name}`"),
                    hint: Some(format!("kinds disponibles: {available}")),
                }],
                warnings: None,
                projected_svg_bytes: None,
                errors_truncated: false,
                supported_versions: Vec::new(),
            };
        }
    };

    let errors = kind.check(&parsed);

    if errors.is_empty() {
        // The dry run reports what the render would cost, so the agent
        // knows the size before opening anything (PRD §7.3, RF20-bis).
        return ValidateResponse {
            ok: true,
            errors: Vec::new(),
            warnings: Some(size_warnings(kind.limits(), &parsed)),
            projected_svg_bytes: Some(projected_svg_bytes(kind.limits(), &parsed)),
            errors_truncated: false,
            supported_versions: Vec::new(),
        };
    }

    let truncated = errors.len() > MAX_ERRORS;
    let mut errors: Vec<ErrorPayload> = errors.iter().map(ErrorPayload::from).collect();
    errors.truncate(MAX_ERRORS);

    ValidateResponse {
        ok: false,
        errors,
        warnings: None,
        projected_svg_bytes: None,
        errors_truncated: truncated,
        supported_versions: Vec::new(),
    }
}

/// The warnings of size, without rejecting the document (PRD §7 RF20).
fn size_warnings(limits: gx_core::kind::KindLimits, document: &Value) -> Vec<Warning> {
    let nodes = document
        .get("nodes")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let edges = document
        .get("edges")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);

    let mut warnings = Vec::new();
    if nodes > 150 {
        warnings.push(Warning::new(
            "too_many_nodes",
            format!(
                "{nodes} nodos: por encima de 150 el diagrama se degrada (labels de aristas omitidos, espaciado al 70%)"
            ),
        ));
    }
    if nodes > limits.max_nodes {
        warnings.push(Warning::new(
            "too_large",
            format!(
                "{nodes} nodos superan el límite duro de {}",
                limits.max_nodes
            ),
        ));
    }
    if edges > limits.max_edges {
        warnings.push(Warning::new(
            "too_large",
            format!(
                "{edges} aristas superan el límite duro de {}",
                limits.max_edges
            ),
        ));
    }
    warnings
}

/// The estimated size of the SVG (PRD §7, RNF1-ter).
fn projected_svg_bytes(limits: gx_core::kind::KindLimits, document: &Value) -> usize {
    let nodes = document
        .get("nodes")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let edges = document
        .get("edges")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let _ = limits;
    900 * nodes + 350 * edges + 40 * 1024
}

/// The response of `get_diagram_schema` (PRD §11.1).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SchemaResponse {
    pub ok: bool,
    /// The index of the kinds, when no kind was asked for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kinds: Option<Vec<KindIndexEntry>>,
    /// The kind that was asked for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// The JSON Schema of the kind.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Value>,
    /// The cross rules that JSON Schema cannot express.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rules: Option<Vec<String>>,
    /// The shapes valid for this kind.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shapes: Option<Vec<String>>,
    /// The required fields of the document.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
    /// The limits of the kind.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limits: Option<LimitsPayload>,
    /// A minimal example of the kind.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examples: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub supported_versions: Vec<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<ErrorPayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_kinds: Option<Vec<String>>,
}

/// One entry of the index of kinds.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct KindIndexEntry {
    pub kind: String,
    pub phase: String,
    pub engine: String,
    pub mermaid_export: String,
    pub limits: LimitsPayload,
    pub min_example: Value,
    pub disponible: bool,
}

/// The limits of a kind.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct LimitsPayload {
    pub max_nodes: usize,
    pub max_edges: usize,
    pub max_density: f64,
}

/// Returns the index of the kinds or the schema of one kind (PRD §11.1).
///
/// The index never carries the schemas: they are asked for one by one.
pub fn get_diagram_schema(kind: Option<&str>) -> SchemaResponse {
    crate::bootstrap();

    let Some(kind_name) = kind else {
        let kinds = gx_core::kind::handlers()
            .into_iter()
            .map(|handler| KindIndexEntry {
                kind: handler.kind().to_string(),
                phase: phase_of(handler.kind()),
                engine: engine_of(handler.layout_hint()).to_string(),
                mermaid_export: mermaid_export_of(handler.kind()).to_string(),
                limits: limits_of(handler.limits()),
                min_example: min_example_of(handler.kind()),
                disponible: true,
            })
            .collect();
        return SchemaResponse {
            ok: true,
            kinds: Some(kinds),
            kind: None,
            schema: None,
            rules: None,
            shapes: None,
            required: None,
            limits: None,
            examples: None,
            supported_versions: supported_versions(),
            errors: Vec::new(),
            available_kinds: None,
        };
    };

    let Some(handler) = gx_core::kind::for_kind(kind_name) else {
        let available = gx_core::kind::kind_names().join(", ");
        return SchemaResponse {
            ok: false,
            kinds: None,
            kind: Some(kind_name.to_string()),
            schema: None,
            rules: None,
            shapes: None,
            required: None,
            limits: None,
            examples: None,
            supported_versions: Vec::new(),
            errors: vec![ErrorPayload {
                path: "kind".to_string(),
                code: ErrorCode::UnknownKind.as_str().to_string(),
                message: format!("kind desconocido: `{kind_name}`"),
                hint: Some(format!("kinds disponibles: {available}")),
            }],
            available_kinds: Some(
                gx_core::kind::kind_names()
                    .iter()
                    .map(|name| name.to_string())
                    .collect(),
            ),
        };
    };

    let schema = serde_json::to_value(handler.schema()).unwrap_or(Value::Null);
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    SchemaResponse {
        ok: true,
        kinds: None,
        kind: Some(kind_name.to_string()),
        schema: Some(schema),
        rules: Some(rules_of(kind_name)),
        shapes: Some(shapes_of(kind_name)),
        required: Some(required),
        limits: Some(limits_of(handler.limits())),
        examples: Some(vec![min_example_of(kind_name)]),
        supported_versions: supported_versions(),
        errors: Vec::new(),
        available_kinds: None,
    }
}

/// The contract versions this build understands (PRD §7 RF5).
fn supported_versions() -> Vec<u32> {
    vec![1]
}

/// The phase of the roadmap a kind belongs to (PRD §6).
fn phase_of(kind: &str) -> String {
    match kind {
        "graph" | "er" => "F1".to_string(),
        "flowchart" | "sequence" | "class" | "state" | "component" | "dataflow" | "tree"
        | "mindmap" | "timeline" | "swimlane" => "F2".to_string(),
        _ => "F3".to_string(),
    }
}

/// The layout engine of a kind (PRD §8.3.1).
fn engine_of(hint: gx_core::ir::LayoutHint) -> &'static str {
    match hint {
        gx_core::ir::LayoutHint::Layered => "layered",
        gx_core::ir::LayoutHint::Tree => "tree",
        gx_core::ir::LayoutHint::Sankey => "sankey",
        gx_core::ir::LayoutHint::Git => "git",
    }
}

/// Whether the kind exports to mermaid and with what loss (PRD §11.1).
fn mermaid_export_of(kind: &str) -> &'static str {
    match kind {
        "graph" | "flowchart" | "er" | "sequence" | "class" | "state" | "gantt" | "gitgraph"
        | "journey" | "sankey" | "timeline" | "mindmap" => "nativa",
        _ => "no_soportada",
    }
}

/// The limits of a kind, as the tool reports them.
fn limits_of(limits: gx_core::kind::KindLimits) -> LimitsPayload {
    LimitsPayload {
        max_nodes: limits.max_nodes,
        max_edges: limits.max_edges,
        max_density: limits.max_density,
    }
}

/// The minimal example of a kind (PRD §6.1).
fn min_example_of(kind: &str) -> Value {
    match kind {
        "er" => serde_json::json!({
            "version": 1,
            "kind": "er",
            "title": "Schema — posts",
            "tables": [
                { "name": "users", "columns": [ { "name": "id", "type": "uuid", "key": "pk" } ] },
                { "name": "posts", "columns": [
                    { "name": "id", "type": "uuid", "key": "pk" },
                    { "name": "user_id", "type": "uuid", "key": "fk" }
                ] }
            ],
            "relations": [
                {
                    "from": { "table": "users", "column": "id" },
                    "to": { "table": "posts", "column": "user_id" },
                    "cardinality": "1:N"
                }
            ]
        }),
        _ => serde_json::json!({
            "version": 1,
            "kind": "graph",
            "title": "parse_config() — flujo",
            "direction": "TB",
            "nodes": [
                { "id": "read", "label": "read file" },
                { "id": "ok", "label": "Config" }
            ],
            "edges": [
                { "from": "read", "to": "ok", "label": "utf-8" }
            ]
        }),
    }
}

/// The cross rules of a kind, written for the agent (PRD §11.1).
fn rules_of(kind: &str) -> Vec<String> {
    let mut rules = vec![
        "`version` debe ser 1 (soportadas: 1).".to_string(),
        "Los campos desconocidos se rechazan con `unknown_field`.".to_string(),
    ];
    match kind {
        "graph" => {
            rules.push("Cada nodo tiene un `id` único dentro del diagrama.".to_string());
            rules.push(
                "Cada arista referencia nodos declarados en `nodes` con `from` y `to`.".to_string(),
            );
        }
        "er" => {
            rules.push("Cada tabla tiene un `name` único dentro del diagrama.".to_string());
            rules.push("Cada relación referencia tablas y columnas declaradas.".to_string());
        }
        _ => {}
    }
    rules
}

/// The shapes valid for a kind (PRD §11.1).
fn shapes_of(kind: &str) -> Vec<String> {
    match kind {
        "graph" => vec![
            "box".to_string(),
            "rounded".to_string(),
            "cylinder".to_string(),
            "diamond".to_string(),
            "ellipse".to_string(),
            "note".to_string(),
        ],
        "er" => vec!["table".to_string(), "column".to_string()],
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::{get_diagram_schema, validate_diagram, MAX_ERRORS};
    use gx_core::error::ErrorCode;

    const MINIMAL: &str = r#"{
      "version": 1,
      "kind": "graph",
      "title": "parse_config() — flujo",
      "direction": "TB",
      "nodes": [ { "id": "read", "label": "read file" }, { "id": "ok", "label": "Config" } ],
      "edges": [ { "from": "read", "to": "ok", "label": "utf-8" } ]
    }"#;

    #[test]
    fn a_valid_document_validates_without_touching_the_window() {
        let response = validate_diagram(MINIMAL);
        assert!(response.ok, "{response:?}");
        assert!(response.errors.is_empty());
        assert!(response.projected_svg_bytes.unwrap() > 0);
    }

    #[test]
    fn an_unknown_ref_is_reported_with_its_path_and_code() {
        let document = r#"{
          "version": 1, "kind": "graph",
          "nodes": [ { "id": "read" } ],
          "edges": [ { "from": "read", "to": "db2" } ]
        }"#;
        let response = validate_diagram(document);
        assert!(!response.ok);
        assert_eq!(response.errors[0].code, "unknown_ref");
        assert_eq!(response.errors[0].path, "edges[0].to");
    }

    #[test]
    fn a_duplicate_id_is_reported() {
        let document = r#"{
          "version": 1, "kind": "graph",
          "nodes": [ { "id": "a" }, { "id": "a" } ]
        }"#;
        let response = validate_diagram(document);
        assert!(!response.ok);
        assert_eq!(response.errors[0].code, "duplicate_id");
        assert_eq!(response.errors[0].path, "nodes[1].id");
    }

    #[test]
    fn an_unknown_field_is_rejected() {
        let document = r#"{
          "version": 1, "kind": "graph",
          "nodes": [ { "id": "a", "color": "red" } ]
        }"#;
        let response = validate_diagram(document);
        assert!(!response.ok);
        assert_eq!(response.errors[0].code, ErrorCode::UnknownField.as_str());
    }

    #[test]
    fn broken_json_is_reported_as_invalid_json() {
        let response = validate_diagram("{not json");
        assert!(!response.ok);
        assert_eq!(response.errors[0].code, "invalid_json");
    }

    #[test]
    fn an_unknown_kind_lists_the_available_ones() {
        let response = validate_diagram(r#"{"version": 1, "kind": "nope"}"#);
        assert!(!response.ok);
        assert_eq!(response.errors[0].code, "unknown_kind");
        assert!(response.errors[0].hint.clone().unwrap().contains("graph"));
    }

    #[test]
    fn the_errors_are_capped_and_the_truncation_is_reported() {
        // 40 nodos con el mismo id: 40 errores de `duplicate_id`.
        let mut nodes = Vec::new();
        for _ in 0..40 {
            nodes.push(serde_json::json!({ "id": "a" }));
        }
        let document = serde_json::json!({ "version": 1, "kind": "graph", "nodes": nodes });
        let response = validate_diagram(&document.to_string());
        assert!(!response.ok);
        assert_eq!(response.errors.len(), MAX_ERRORS);
        assert!(response.errors_truncated);
    }

    #[test]
    fn the_schema_index_lists_the_kinds_without_their_schemas() {
        let response = get_diagram_schema(None);
        assert!(response.ok);
        let kinds = response.kinds.expect("index");
        assert!(!kinds.is_empty());
        assert!(
            response.schema.is_none(),
            "the index never carries the schemas"
        );
        let graph = kinds
            .iter()
            .find(|entry| entry.kind == "graph")
            .expect("graph");
        assert_eq!(graph.phase, "F1");
        assert_eq!(graph.engine, "layered");
        assert_eq!(graph.mermaid_export, "nativa");
        assert_eq!(graph.limits.max_nodes, 300);
        assert_eq!(graph.limits.max_edges, 600);
        assert!(graph.min_example.get("kind").is_some());
    }

    #[test]
    fn the_schema_of_a_kind_carries_the_rules_and_the_shapes() {
        let response = get_diagram_schema(Some("graph"));
        assert!(response.ok);
        assert_eq!(response.kind.as_deref(), Some("graph"));
        assert!(response.schema.is_some());
        assert!(response
            .rules
            .unwrap()
            .iter()
            .any(|rule| rule.contains("id")));
        let shapes = response.shapes.unwrap();
        assert!(shapes.contains(&"rounded".to_string()));
        assert!(!shapes.is_empty());
        assert_eq!(response.required.unwrap(), vec!["version", "kind"]);
    }

    #[test]
    fn an_unknown_kind_in_the_schema_lists_the_available_ones() {
        let response = get_diagram_schema(Some("nope"));
        assert!(!response.ok);
        assert_eq!(response.errors[0].code, "unknown_kind");
        assert!(response
            .available_kinds
            .unwrap()
            .contains(&"graph".to_string()));
    }
}
