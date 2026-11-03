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

// ---------------------------------------------------------------------------
// Lifecycle of the diagrams on the plan (PRD §11, §11.1)
// ---------------------------------------------------------------------------

/// The response of `list_diagrams` (PRD §11).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ListResponse {
    pub ok: bool,
    pub diagrams: Vec<DiagramEntry>,
}

/// One diagram of the plan, as `list_diagrams` reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiagramEntry {
    pub diagram_id: String,
    pub title: String,
    pub kind: String,
    pub updated_at: String,
    pub session_id: String,
    pub revision: u64,
}

/// The response of `get_diagram` (PRD §11).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GetResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagram: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<ErrorPayload>,
}

/// The response of `clear_canvas` (PRD §11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClearResponse {
    pub ok: bool,
    pub removed: usize,
}

/// The response of `update_diagram` (PRD §11).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UpdateResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<ErrorPayload>,
    pub applied: bool,
    /// The revision after the update, or the current one when it did not
    /// apply.
    pub revision: u64,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub errors_truncated: bool,
}

/// Lists the diagrams of the session (PRD §11.1: session scope by default).
pub fn list_diagrams(session_id: &str) -> ListResponse {
    crate::bootstrap();
    let store = crate::store::diagram_store();
    ListResponse {
        ok: true,
        diagrams: store
            .list_for_session(session_id)
            .into_iter()
            .map(|summary| DiagramEntry {
                diagram_id: summary.diagram_id,
                title: summary.title,
                kind: summary.kind,
                updated_at: summary.updated_at,
                session_id: summary.session_id,
                revision: summary.revision,
            })
            .collect(),
    }
}

/// Returns the source document of a diagram (PRD §11).
pub fn get_diagram(session_id: &str, diagram_id: &str) -> GetResponse {
    crate::bootstrap();
    let store = crate::store::diagram_store();

    let Some(record) = store.record(diagram_id) else {
        return GetResponse {
            ok: false,
            diagram: None,
            revision: None,
            errors: vec![unknown_diagram(diagram_id, &store.known_ids())],
        };
    };

    if record.summary.session_id != session_id {
        return GetResponse {
            ok: false,
            diagram: None,
            revision: None,
            errors: vec![not_owner(diagram_id)],
        };
    }

    GetResponse {
        ok: true,
        diagram: serde_json::from_str(&record.document).ok(),
        revision: Some(record.summary.revision),
        errors: Vec::new(),
    }
}

/// Empties the plan: only the session, or the whole plan (PRD §11, §11.1).
pub fn clear_canvas(session_id: &str, scope: Option<&str>) -> ClearResponse {
    crate::bootstrap();
    let store = crate::store::diagram_store();
    let removed = match scope {
        Some("plano") => store.clear_all(),
        _ => store.clear_session(session_id),
    };
    ClearResponse { ok: true, removed }
}

/// Applies an RFC 7386 merge patch to a diagram, transactionally (PRD §11.1).
///
/// The merge runs on a copy, the result goes through the very same pipeline
/// as `render_diagram` and only then replaces the stored document: when the
/// patch is invalid the diagram and its render stay untouched.
pub fn update_diagram(
    session_id: &str,
    diagram_id: &str,
    patch: &Value,
    expected_revision: Option<u64>,
) -> UpdateResponse {
    crate::bootstrap();
    let store = crate::store::diagram_store();

    let Some(record) = store.record(diagram_id) else {
        return UpdateResponse {
            ok: false,
            errors: vec![unknown_diagram(diagram_id, &store.known_ids())],
            applied: false,
            revision: 0,
            errors_truncated: false,
        };
    };
    if record.summary.session_id != session_id {
        return UpdateResponse {
            ok: false,
            errors: vec![not_owner(diagram_id)],
            applied: false,
            revision: record.summary.revision,
            errors_truncated: false,
        };
    }
    if let Some(expected) = expected_revision {
        if expected != record.summary.revision {
            return UpdateResponse {
                ok: false,
                errors: vec![ErrorPayload {
                    path: "revision".to_string(),
                    code: ErrorCode::StaleRevision.as_str().to_string(),
                    message: format!(
                        "la revisión {expected} está vieja: la actual es {}",
                        record.summary.revision
                    ),
                    hint: Some("volvé a leer el diagrama con get_diagram".to_string()),
                }],
                applied: false,
                revision: record.summary.revision,
                errors_truncated: false,
            };
        }
    }

    let mut document: Value = match serde_json::from_str(&record.document) {
        Ok(value) => value,
        Err(_) => Value::Null,
    };

    if !patch.is_object() {
        return UpdateResponse {
            ok: false,
            errors: vec![ErrorPayload {
                path: "patch".to_string(),
                code: ErrorCode::InvalidPatch.as_str().to_string(),
                message: "el patch tiene que ser un objeto JSON".to_string(),
                hint: Some("un merge patch RFC 7386 es un objeto; para reemplazar todo, mandá el documento completo".to_string()),
            }],
            applied: false,
            revision: record.summary.revision,
            errors_truncated: false,
        };
    }

    merge_patch(&mut document, patch);

    // The kind cannot change under an existing diagram (PRD §11.1).
    if document.get("kind").and_then(Value::as_str) != Some(record.summary.kind.as_str()) {
        return UpdateResponse {
            ok: false,
            errors: vec![ErrorPayload {
                path: "patch.kind".to_string(),
                code: ErrorCode::KindChangeNotAllowed.as_str().to_string(),
                message: "el `kind` de un diagrama no se puede cambiar".to_string(),
                hint: Some("creá un diagrama nuevo con render_diagram".to_string()),
            }],
            applied: false,
            revision: record.summary.revision,
            errors_truncated: false,
        };
    }

    let candidate = document.to_string();
    let validation = validate_diagram(&candidate);
    if !validation.ok {
        return UpdateResponse {
            ok: false,
            errors: validation.errors,
            applied: false,
            revision: record.summary.revision,
            errors_truncated: validation.errors_truncated,
        };
    }

    let title = document
        .get("title")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| record.summary.title.clone());
    let Some(summary) = store.replace(diagram_id, title, candidate) else {
        return UpdateResponse {
            ok: false,
            errors: vec![unknown_diagram(diagram_id, &store.known_ids())],
            applied: false,
            revision: record.summary.revision,
            errors_truncated: false,
        };
    };

    UpdateResponse {
        ok: true,
        errors: Vec::new(),
        applied: true,
        revision: summary.revision,
        errors_truncated: false,
    }
}

/// RFC 7386: merge a patch into a document.
///
/// `null` removes a key and arrays are replaced whole (PRD §7, RF9).
fn merge_patch(target: &mut Value, patch: &Value) {
    let Value::Object(patch_object) = patch else {
        *target = patch.clone();
        return;
    };
    if !target.is_object() {
        *target = Value::Object(serde_json::Map::new());
    }
    let Value::Object(target_object) = target else {
        return;
    };
    for (key, value) in patch_object {
        if value.is_null() {
            target_object.remove(key);
        } else {
            let slot = target_object
                .entry(key.clone())
                .or_insert_with(|| Value::Object(serde_json::Map::new()));
            merge_patch(slot, value);
        }
    }
}

/// The error of a diagram that is not on the plan (PRD §11.1).
fn unknown_diagram(diagram_id: &str, known_ids: &[String]) -> ErrorPayload {
    ErrorPayload {
        path: "diagram_id".to_string(),
        code: ErrorCode::UnknownDiagram.as_str().to_string(),
        message: format!(
            "el diagrama `{diagram_id}` no está en el plano; conocidos: {known_ids:?}"
        ),
        hint: Some(
            "el diagrama no está en el plano (quizá lo cerró el humano); usá list_diagrams()"
                .to_string(),
        ),
    }
}

/// The error of a diagram of another session (PRD §11.1).
fn not_owner(diagram_id: &str) -> ErrorPayload {
    ErrorPayload {
        path: "diagram_id".to_string(),
        code: ErrorCode::NotOwner.as_str().to_string(),
        message: format!("el diagrama `{diagram_id}` pertenece a otra sesión MCP"),
        hint: Some("ese diagrama pertenece a otra sesión MCP".to_string()),
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

    // ---- Lifecycle (PRD §11, §11.1) -------------------------------------

    /// Renders a document into the store and returns its id.
    fn render_into_store(document: &str, session_id: &str) -> String {
        let response = crate::tools::render_diagram(&crate::tools::RenderRequest {
            document: document.to_string(),
            session_id: session_id.to_string(),
        });
        response.diagram_id.expect("the render stores the diagram")
    }

    #[test]
    fn list_returns_only_the_diagrams_of_the_session() {
        let _guard = crate::test_support::lock_renders_blocking();
        render_into_store(MINIMAL, "session-a");
        render_into_store(MINIMAL, "session-b");

        let mine = super::list_diagrams("session-a");
        assert!(mine.ok);
        assert_eq!(mine.diagrams.len(), 1);
        assert_eq!(mine.diagrams[0].session_id, "session-a");

        let all = crate::store::diagram_store().list();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn get_returns_the_document_of_the_session() {
        let _guard = crate::test_support::lock_renders_blocking();
        let id = render_into_store(MINIMAL, "session-a");

        let response = super::get_diagram("session-a", &id);
        assert!(response.ok, "{response:?}");
        assert_eq!(response.revision, Some(1));
        let document = response.diagram.expect("document");
        assert_eq!(document["kind"], "graph");
        assert_eq!(document["nodes"][0]["id"], "read");
    }

    #[test]
    fn get_of_another_session_is_not_owner() {
        let _guard = crate::test_support::lock_renders_blocking();
        let id = render_into_store(MINIMAL, "session-a");

        let response = super::get_diagram("session-b", &id);
        assert!(!response.ok);
        assert_eq!(response.errors[0].code, "not_owner");
    }

    #[test]
    fn an_unknown_diagram_lists_the_known_ids() {
        let _guard = crate::test_support::lock_renders_blocking();
        let id = render_into_store(MINIMAL, "session-a");

        let response = super::get_diagram("session-a", "d-9999");
        assert!(!response.ok);
        assert_eq!(response.errors[0].code, "unknown_diagram");
        assert!(
            response.errors[0].message.contains(&id),
            "{}",
            response.errors[0].message
        );
    }

    #[test]
    fn clear_canvas_removes_only_the_session_by_default() {
        let _guard = crate::test_support::lock_renders_blocking();
        render_into_store(MINIMAL, "session-a");
        render_into_store(MINIMAL, "session-b");

        let cleared = super::clear_canvas("session-a", None);
        assert!(cleared.ok);
        assert_eq!(cleared.removed, 1);
        assert_eq!(crate::store::diagram_store().list().len(), 1);
    }

    #[test]
    fn clear_canvas_with_the_plan_scope_removes_everything() {
        let _guard = crate::test_support::lock_renders_blocking();
        render_into_store(MINIMAL, "session-a");
        render_into_store(MINIMAL, "session-b");

        let cleared = super::clear_canvas("session-a", Some("plano"));
        assert_eq!(cleared.removed, 2);
        assert!(crate::store::diagram_store().list().is_empty());
    }

    #[test]
    fn an_update_applies_and_grows_the_revision() {
        let _guard = crate::test_support::lock_renders_blocking();
        let id = render_into_store(MINIMAL, "session-a");

        let patch = serde_json::json!({ "title": "nuevo título" });
        let response = super::update_diagram("session-a", &id, &patch, Some(1));
        assert!(response.ok, "{response:?}");
        assert!(response.applied);
        assert_eq!(response.revision, 2);

        let document = super::get_diagram("session-a", &id)
            .diagram
            .expect("document");
        assert_eq!(document["title"], "nuevo título");
    }

    #[test]
    fn an_update_with_a_broken_patch_keeps_the_document_intact() {
        let _guard = crate::test_support::lock_renders_blocking();
        let id = render_into_store(MINIMAL, "session-a");

        // `null` borra la clave (RFC 7386) y `kind` es obligatorio.
        let patch = serde_json::json!({ "kind": null });
        let response = super::update_diagram("session-a", &id, &patch, None);
        assert!(!response.ok);
        assert!(!response.applied);
        assert_eq!(
            response.revision, 1,
            "la revisión no avanza si no se aplicó"
        );

        let document = super::get_diagram("session-a", &id)
            .diagram
            .expect("document");
        assert_eq!(document["kind"], "graph", "el documento no cambia");
    }

    #[test]
    fn an_update_with_a_stale_revision_is_rejected() {
        let _guard = crate::test_support::lock_renders_blocking();
        let id = render_into_store(MINIMAL, "session-a");

        let patch = serde_json::json!({ "title": "x" });
        super::update_diagram("session-a", &id, &patch, Some(1));

        let stale = super::update_diagram("session-a", &id, &patch, Some(1));
        assert!(!stale.ok);
        assert_eq!(stale.errors[0].code, "stale_revision");
        assert_eq!(stale.revision, 2);
    }

    #[test]
    fn an_update_cannot_change_the_kind() {
        let _guard = crate::test_support::lock_renders_blocking();
        let id = render_into_store(MINIMAL, "session-a");

        let patch = serde_json::json!({ "kind": "er" });
        let response = super::update_diagram("session-a", &id, &patch, None);
        assert!(!response.ok);
        assert_eq!(response.errors[0].code, "kind_change_not_allowed");
    }

    #[test]
    fn an_update_that_is_not_an_object_is_a_invalid_patch() {
        let _guard = crate::test_support::lock_renders_blocking();
        let id = render_into_store(MINIMAL, "session-a");

        let response = super::update_diagram("session-a", &id, &serde_json::json!([1, 2]), None);
        assert!(!response.ok);
        assert_eq!(response.errors[0].code, "invalid_patch");
    }

    #[test]
    fn the_merge_patch_of_rfc_7386_replaces_arrays_whole() {
        let mut target = serde_json::json!({ "nodes": [1, 2, 3], "title": "viejo", "extra": true });
        let patch = serde_json::json!({ "nodes": [9], "title": null });
        super::merge_patch(&mut target, &patch);
        assert_eq!(target["nodes"], serde_json::json!([9]));
        assert!(target.get("title").is_none(), "null borra la clave");
        assert_eq!(target["extra"], true);
    }

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
