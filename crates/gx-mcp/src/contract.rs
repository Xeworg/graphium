//! The contract of the tools: validation and schema (PRD §7 RF3/RF5, §11).
//!
//! `validate_diagram` is the dry run of `render_diagram`: the very same
//! pipeline (registry → schema → `validate()`), without layout and without
//! rendering. An agent uses it to fix a document before opening a window.

use serde::Serialize;
use serde_json::Value;

use gx_core::error::{ErrorCode, ValidationError};

use crate::session::{self, SessionView};

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
            warnings: Some(size_warnings(
                count(&parsed, "nodes"),
                count(&parsed, "edges"),
                kind.limits(),
            )),
            projected_svg_bytes: Some(projected_svg_bytes(kind.limits(), &parsed)),
            errors_truncated: false,
            supported_versions: supported_versions(),
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
        supported_versions: supported_versions(),
    }
}

/// The number of entries of a top-level array of the document.
fn count(document: &Value, field: &str) -> usize {
    document
        .get(field)
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0)
}

/// The warnings of size, without rejecting the document (PRD §7 RF20, S6).
///
/// The render and the dry run report the same thing with the same words: an
/// agent that saw the warning in `validate_diagram` reads it again after the
/// paint instead of learning a second vocabulary.
pub(crate) fn size_warnings(
    nodes: usize,
    edges: usize,
    limits: gx_core::kind::KindLimits,
) -> Vec<Warning> {
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
    /// The contract version the kind serves (`f23-contrato.md`, S2): the
    /// generation the schema below describes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind_version: Option<u32>,
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
///
/// The entry promises what the product has: no `mermaid_export`, because
/// Mermaid is out of the export scope (feature doc `f2-export.md`, L2) and an
/// index that announces a capability the tool cannot deliver is a lie the
/// agent has no way to check.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct KindIndexEntry {
    pub kind: String,
    pub phase: String,
    pub engine: String,
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
    /// The lanes a lanes-diagram admits; `0` when the kind has no lanes.
    pub max_lanes: usize,
    /// The events a lanes-diagram admits; `0` when the kind has no events.
    pub max_events: usize,
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
                phase: handler.phase().to_string(),
                engine: engine_of(handler.layout_hint()).to_string(),
                limits: limits_of(handler.limits()),
                min_example: handler.min_example(),
                disponible: true,
            })
            .collect();
        return SchemaResponse {
            ok: true,
            kinds: Some(kinds),
            kind: None,
            kind_version: None,
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
            kind_version: None,
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
        kind_version: Some(handler.kind_version()),
        schema: Some(annotate_since(schema, handler.kind_version())),
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

/// Stamps every field of the schema with the version it exists since
/// (`f23-contrato.md`, S2): the promise PRD §10.1 makes
/// (`x-graphium-since`, `docs/PRD.md:566`) that the code never published.
/// Every field of the current generation exists since 1 — f22's opt-in
/// fields included — so the default is uniform and a future field declares
/// its own later by carrying the key already.
fn annotate_since(schema: Value, version: u32) -> Value {
    fn walk(value: &mut Value, version: u32) {
        let Value::Object(map) = value else {
            return;
        };
        // Every field of a `properties` map — at the top of the schema or
        // inside `$defs` — is a field of the contract: it says its since.
        if let Some(Value::Object(props)) = map.get_mut("properties") {
            for (_name, prop) in props.iter_mut() {
                if prop.is_object() {
                    if let Value::Object(prop_map) = prop {
                        prop_map
                            .entry("x-graphium-since")
                            .or_insert_with(|| Value::from(version));
                    }
                    walk(prop, version);
                }
            }
        }
        for (key, child) in map.iter_mut() {
            if key != "properties" {
                walk(child, version);
            }
        }
    }
    let mut schema = schema;
    walk(&mut schema, version);
    schema
}

/// The layout engine of a kind (PRD §8.3.1).
fn engine_of(hint: gx_core::ir::LayoutHint) -> &'static str {
    match hint {
        gx_core::ir::LayoutHint::Layered => "layered",
        gx_core::ir::LayoutHint::Tree => "tree",
        gx_core::ir::LayoutHint::Lanes => "lanes",
        gx_core::ir::LayoutHint::Sankey => "sankey",
        gx_core::ir::LayoutHint::Git => "git",
    }
}

/// The limits of a kind, as the tool reports them.
fn limits_of(limits: gx_core::kind::KindLimits) -> LimitsPayload {
    LimitsPayload {
        max_nodes: limits.max_nodes,
        max_edges: limits.max_edges,
        max_density: limits.max_density,
        max_lanes: limits.max_lanes,
        max_events: limits.max_events,
    }
}

/// The minimal example of a kind (PRD §6.1), as the kind itself declares it
/// (`f20-reuso.md`, S9): `gx-mcp` owns no example, so a kind that enters the
/// registry shows its own without touching this crate (O7) and a name outside
/// the registry inherits none.
fn min_example_of(kind: &str) -> Value {
    gx_core::kind::for_kind(kind)
        .map(|handler| handler.min_example())
        .unwrap_or(Value::Null)
}

/// The cross rules of a kind, written for the agent (PRD §11.1): the generic
/// rules every kind enforces, the line of its vocabulary and the rules the kind
/// itself teaches (`f20-reuso.md`, S11). `gx-mcp` owns only what is generic.
fn rules_of(kind: &str) -> Vec<String> {
    let Some(handler) = gx_core::kind::for_kind(kind) else {
        return Vec::new();
    };

    let mut rules = vec![
        "`version` debe ser 1 (soportadas: 1).".to_string(),
        "Los campos desconocidos se rechazan con `unknown_field`.".to_string(),
        // The line-break convention (`f23-contrato.md`, S4), the one thing no
        // schema could say: the drawing never wraps on its own.
        "Una etiqueta larga se corta con `\\n`: el dibujo no envuelve solo y la caja crece hasta la línea más ancha."
            .to_string(),
    ];
    rules.extend(role_rule(handler.roles(), handler.default_role()));
    rules.extend(handler.rules());
    if admits_groups(handler) {
        rules.push(
            "Un miembro de `groups[].members` es un nodo o **otro grupo**: los grupos anidan. Un miembro vive en un solo grupo y un grupo no puede contenerse a si mismo."
                .to_string(),
        );
    }
    rules
}

/// The line that teaches the vocabulary of a kind (`f20-reuso.md`, S11): every
/// role with the shape it draws and which one is the default. It is derived
/// from the kind's own table, so a new role enters the agent's rules without
/// another copy here.
fn role_rule(roles: &[gx_core::kind::Role], default: Option<&str>) -> Option<String> {
    if roles.is_empty() {
        return None;
    }
    let named: Vec<String> = roles
        .iter()
        .map(|role| {
            let shape = shape_in_words(role.shape);
            if Some(role.name) == default {
                format!("`{}` ({shape}, por defecto)", role.name)
            } else {
                format!("`{}` ({shape})", role.name)
            }
        })
        .collect();
    Some(format!(
        "El `role` de un nodo decide su forma: {}.",
        named.join(", ")
    ))
}

/// The Spanish word of a shape, as the rules teach it.
fn shape_in_words(shape: gx_core::ir::Shape) -> &'static str {
    match shape {
        gx_core::ir::Shape::Box => "caja",
        gx_core::ir::Shape::Rounded => "redondeado",
        gx_core::ir::Shape::Cylinder => "cilindro",
        gx_core::ir::Shape::Diamond => "rombo",
        gx_core::ir::Shape::Ellipse => "elipse",
        gx_core::ir::Shape::Note => "nota",
        gx_core::ir::Shape::Circle => "círculo",
        gx_core::ir::Shape::Dot => "punto lleno",
        gx_core::ir::Shape::RingDot => "punto dentro de un anillo",
    }
}

/// Whether the kind admits `groups`, read from its own schema
/// (`f20-reuso.md`, S10): the rule is written when the document shape declares
/// the field, never from a hand list here.
fn admits_groups(handler: &dyn gx_core::kind::KindHandler) -> bool {
    handler
        .schema()
        .get("properties")
        .and_then(|properties| properties.get("groups"))
        .is_some()
}

/// The shapes valid for a kind (PRD §11.1): the truth of the kind — what its
/// roles draw (`f19-formas.md`, S4) — read from the registry, never a hand
/// list here.
fn shapes_of(kind: &str) -> Vec<String> {
    gx_core::kind::handlers()
        .iter()
        .find(|handler| handler.kind() == kind)
        .map(|handler| {
            handler
                .shapes()
                .iter()
                .map(|shape| shape.as_str().to_string())
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Lifecycle of the diagrams on the plan (PRD §11, §11.1)
// ---------------------------------------------------------------------------

/// The response of `list_diagrams` (PRD §11).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ListResponse {
    pub ok: bool,
    pub diagrams: Vec<DiagramEntry>,
    /// The session of the caller: the name the human sees in the window
    /// (S3, D2). It is additive; every other field keeps its place.
    pub session: SessionView,
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

/// The response of `list_diagram_versions` (RF14; feature doc
/// `f2-cierre.md`, R3).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VersionsResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagram_id: Option<String>,
    /// The version the current document is on. The «v3/7» of the window is
    /// the position of this number among the kept versions (`versions.len()`
    /// is the `7`), so a full history reads «v20/20» and never «v25/20».
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_version: Option<u64>,
    /// Oldest first; the current one is the last of the list.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<VersionEntry>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<ErrorPayload>,
}

/// One version of the document, as `list_diagram_versions` reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VersionEntry {
    pub version: u64,
    pub revision: u64,
    pub updated_at: String,
    pub bytes: usize,
}

impl From<crate::store::VersionSummary> for VersionEntry {
    fn from(summary: crate::store::VersionSummary) -> Self {
        Self {
            version: summary.version,
            revision: summary.revision,
            updated_at: summary.updated_at,
            bytes: summary.bytes,
        }
    }
}

/// The response of `get_diagram_version` (RF14, R3).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VersionResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagram_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document: Option<Value>,
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
        session: session::current_view(session_id),
    }
}

/// The response of `register_session` (S3, D2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegisterSessionResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<SessionView>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<ErrorPayload>,
}

/// Names the session of the caller and answers it (S3, D2).
///
/// Without a label it is a `whoami`: the session answers as it is, with the
/// name the human already sees. With one, the label is trimmed and stored;
/// an empty or whitespace-only label is a value that does not fit its field,
/// so it is `invalid_value` at `label` and the session keeps its name — an
/// empty label never resets the name the agent already chose.
pub fn register_session(session_id: &str, label: Option<&str>) -> RegisterSessionResponse {
    crate::bootstrap();

    let Some(label) = label else {
        return RegisterSessionResponse {
            ok: true,
            session: Some(session::current_view(session_id)),
            errors: Vec::new(),
        };
    };

    let label = label.trim();
    if label.is_empty() {
        return RegisterSessionResponse {
            ok: false,
            session: None,
            errors: vec![empty_label()],
        };
    }

    RegisterSessionResponse {
        ok: true,
        session: Some(session::name_session(session_id, label)),
        errors: Vec::new(),
    }
}

/// The error of a label with nothing in it (D2).
fn empty_label() -> ErrorPayload {
    ErrorPayload {
        path: "label".to_string(),
        code: ErrorCode::InvalidValue.as_str().to_string(),
        message: "la etiqueta de la sesión no puede estar vacía".to_string(),
        hint: Some(
            "mandá un nombre corto y reconocible, por ejemplo \"presupuesto API\", o no mandés \
             `label` para solo consultar la sesión"
                .to_string(),
        ),
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

/// The record of a diagram of this session, or the error that says why not.
///
/// The two checks every tool that names an id makes (PRD §11.1): the diagram
/// has to be on the plan and it has to be of the caller. Only the summary is
/// read, because the history tools never need the current document.
fn owned_summary(
    store: &crate::store::DiagramStore,
    session_id: &str,
    diagram_id: &str,
) -> Result<crate::store::DiagramSummary, ErrorPayload> {
    let Some(summary) = store.get(diagram_id) else {
        return Err(unknown_diagram(diagram_id, &store.known_ids()));
    };
    if summary.session_id != session_id {
        return Err(not_owner(diagram_id));
    }
    Ok(summary)
}

/// Lists the versions of the source document of a diagram (RF14, R3).
///
/// It is a read: nothing on the plan, in the document or in the history moves.
pub fn list_diagram_versions(session_id: &str, diagram_id: &str) -> VersionsResponse {
    crate::bootstrap();
    let store = crate::store::diagram_store();

    if let Err(error) = owned_summary(store, session_id, diagram_id) {
        return VersionsResponse {
            ok: false,
            diagram_id: None,
            current_version: None,
            versions: Vec::new(),
            errors: vec![error],
        };
    }

    let versions = store.versions(diagram_id).unwrap_or_default();
    VersionsResponse {
        ok: true,
        diagram_id: Some(diagram_id.to_string()),
        current_version: versions.last().map(|version| version.version),
        versions: versions.into_iter().map(VersionEntry::from).collect(),
        errors: Vec::new(),
    }
}

/// Returns the source document of one version of a diagram (RF14, R3).
///
/// Read-only on purpose: there is no tool that restores a version. Restoring
/// is a `render_diagram` or an `update_diagram` that creates a new version, so
/// the history never rewrites itself.
pub fn get_diagram_version(session_id: &str, diagram_id: &str, version: u64) -> VersionResponse {
    crate::bootstrap();
    let store = crate::store::diagram_store();

    if let Err(error) = owned_summary(store, session_id, diagram_id) {
        return VersionResponse {
            ok: false,
            diagram_id: None,
            version: None,
            revision: None,
            updated_at: None,
            document: None,
            errors: vec![error],
        };
    }

    let Some(found) = store.version(diagram_id, version) else {
        let known = store.versions(diagram_id).unwrap_or_default();
        return VersionResponse {
            ok: false,
            diagram_id: Some(diagram_id.to_string()),
            version: None,
            revision: None,
            updated_at: None,
            document: None,
            errors: vec![unknown_version(diagram_id, version, &known)],
        };
    };

    VersionResponse {
        ok: true,
        diagram_id: Some(diagram_id.to_string()),
        version: Some(found.summary.version),
        revision: Some(found.summary.revision),
        updated_at: Some(found.summary.updated_at),
        document: serde_json::from_str(&found.document).ok(),
        errors: Vec::new(),
    }
}

/// The error of a version the diagram does not keep (RF14).
pub(crate) fn unknown_version(
    diagram_id: &str,
    version: u64,
    known: &[crate::store::VersionSummary],
) -> ErrorPayload {
    let range = match (known.first(), known.last()) {
        (Some(first), Some(last)) => format!(
            "las versiones que guarda son {}..={} ({} en total)",
            first.version,
            last.version,
            known.len()
        ),
        _ => "no guarda ninguna versión".to_string(),
    };
    ErrorPayload {
        path: "version".to_string(),
        code: ErrorCode::InvalidValue.as_str().to_string(),
        message: format!("el diagrama `{diagram_id}` no tiene la versión {version}: {range}"),
        hint: Some("usá `list_diagram_versions` para ver los números que existen".to_string()),
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
///
/// Public because the window answers it too: the user plan asks the store for
/// a clone of a diagram that may have closed, and the human reads the same
/// error any tool would answer in the banner of the plan
/// (`f8-planos.md`, B1).
pub fn unknown_diagram(diagram_id: &str, known_ids: &[String]) -> ErrorPayload {
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
pub(crate) fn not_owner(diagram_id: &str) -> ErrorPayload {
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

    /// V2 de `f8-planos.md` (B1, S4): el plano del usuario es del dueño. Las
    /// tools conocen solo los diagramas del store, así que el id que el dueño
    /// numera para una **instancia de vista** (`c-0001`, el clon de un plano
    /// del usuario) no existe para ninguna de ellas: no se lista, no se lee, no
    /// se actualiza y no se cierra. El id de un clon nunca se confunde con un
    /// `diagram_id`, y el contrato no tiene una tool para tocarlo.
    #[test]
    fn the_tools_only_know_the_diagrams_of_the_store() {
        let _guard = crate::test_support::lock_renders_blocking();
        let id = render_into_store(MINIMAL, "session-a");
        let clone_id = "c-0001";

        let listed = super::list_diagrams("session-a");
        let ids: Vec<&str> = listed
            .diagrams
            .iter()
            .map(|entry| entry.diagram_id.as_str())
            .collect();
        assert!(ids.contains(&id.as_str()));
        assert!(
            !ids.contains(&clone_id),
            "la lista del agente no puede traer una instancia de vista: {ids:?}"
        );

        let read = super::get_diagram("session-a", clone_id);
        assert!(!read.ok);
        assert_eq!(read.errors[0].code, "unknown_diagram");

        let updated = super::update_diagram(
            "session-a",
            clone_id,
            &serde_json::json!({ "title": "otro" }),
            None,
        );
        assert!(!updated.ok);
        assert_eq!(updated.errors[0].code, "unknown_diagram");

        let versions = super::list_diagram_versions("session-a", clone_id);
        assert!(!versions.ok);
        assert_eq!(versions.errors[0].code, "unknown_diagram");
    }

    /// RF14/R3: el historial de un diagrama se lista del más viejo al más
    /// nuevo, con la versión actual para el indicador «v3/7», y el documento
    /// de una versión se lee tal como estaba entonces.
    #[test]
    fn the_history_lists_the_versions_and_returns_their_documents() {
        let _guard = crate::test_support::lock_renders_blocking();
        let id = render_into_store(MINIMAL, "session-a");
        super::update_diagram(
            "session-a",
            &id,
            &serde_json::json!({ "title": "segunda" }),
            Some(1),
        );

        let listed = super::list_diagram_versions("session-a", &id);
        assert!(listed.ok, "{listed:?}");
        assert_eq!(listed.diagram_id.as_deref(), Some(id.as_str()));
        assert_eq!(listed.current_version, Some(2), "la «v2» de «v2/2»");
        assert_eq!(listed.versions.len(), 2, "el render y el update");
        assert_eq!(listed.versions[0].version, 1);
        assert_eq!(listed.versions[0].revision, 1);
        assert_eq!(listed.versions[1].version, 2);
        assert_eq!(listed.versions[1].revision, 2);
        assert!(listed.versions[0].bytes > 0);

        let first = super::get_diagram_version("session-a", &id, 1);
        assert!(first.ok, "{first:?}");
        assert_eq!(first.version, Some(1));
        assert_eq!(first.revision, Some(1));
        assert!(first.updated_at.is_some());
        let document = first.document.expect("la versión trae su documento");
        assert_eq!(document["title"], "parse_config() — flujo");

        let second = super::get_diagram_version("session-a", &id, 2);
        assert!(second.ok, "{second:?}");
        assert_eq!(
            second.document.expect("documento")["title"],
            "segunda",
            "la segunda versión es la del update"
        );
    }

    /// RF14: leer una versión no es restaurarla ni reescribir el historial.
    /// La única forma de volver atrás es un render o un update, que agregan
    /// una versión nueva.
    #[test]
    fn reading_a_version_never_changes_the_diagram() {
        let _guard = crate::test_support::lock_renders_blocking();
        let id = render_into_store(MINIMAL, "session-a");
        let before = super::get_diagram("session-a", &id);

        let listed = super::list_diagram_versions("session-a", &id);
        assert!(listed.ok, "{listed:?}");
        let first = super::get_diagram_version("session-a", &id, 1);
        assert!(first.ok, "{first:?}");

        let after = super::get_diagram("session-a", &id);
        assert_eq!(before.revision, after.revision, "la revisión no se mueve");
        assert_eq!(before.diagram, after.diagram, "el documento no se mueve");
        assert_eq!(
            crate::store::diagram_store().open_count(),
            1,
            "el plano no cambia"
        );

        // Volver atrás es un update: aparece una versión nueva, las viejas
        // siguen donde estaban.
        let restored = first.document.expect("documento de la versión 1");
        let patch = serde_json::json!({ "title": restored["title"] });
        super::update_diagram("session-a", &id, &patch, None);
        let listed = super::list_diagram_versions("session-a", &id);
        let numbers: Vec<u64> = listed.versions.iter().map(|entry| entry.version).collect();
        assert_eq!(numbers, vec![1, 2], "la restauración agrega, no reescribe");
        assert_eq!(listed.current_version, Some(2));
    }

    /// RF14: un número que el diagrama no guarda es `invalid_value` en
    /// `version`, con el rango que sí existe como pista.
    #[test]
    fn an_unknown_version_reports_the_range_and_keeps_everything() {
        let _guard = crate::test_support::lock_renders_blocking();
        let id = render_into_store(MINIMAL, "session-a");

        let response = super::get_diagram_version("session-a", &id, 7);

        assert!(!response.ok, "{response:?}");
        assert_eq!(response.errors[0].code, "invalid_value");
        assert_eq!(response.errors[0].path, "version");
        assert!(
            response.errors[0].message.contains("1..=1"),
            "el error dice qué hay: {:?}",
            response.errors[0]
        );
        assert!(response.errors.iter().all(|error| error.hint.is_some()));
        assert!(response.document.is_none());
        assert_eq!(
            crate::store::diagram_store().list()[0].revision,
            1,
            "nada se movió"
        );
    }

    /// PRD §11.1: el historial respeta el alcance de la sesión y el ciclo de
    /// vida del id, como el resto de las tools.
    #[test]
    fn the_history_respects_the_session_scope_and_the_lifecycle() {
        let _guard = crate::test_support::lock_renders_blocking();
        let id = render_into_store(MINIMAL, "session-a");

        let foreign = super::list_diagram_versions("session-b", &id);
        assert!(!foreign.ok);
        assert_eq!(foreign.errors[0].code, "not_owner");
        let foreign_version = super::get_diagram_version("session-b", &id, 1);
        assert!(!foreign_version.ok);
        assert_eq!(foreign_version.errors[0].code, "not_owner");

        let unknown = super::list_diagram_versions("session-a", "d-9999");
        assert!(!unknown.ok);
        assert_eq!(unknown.errors[0].code, "unknown_diagram");
        assert!(
            unknown.errors[0].message.contains(&id),
            "los ids conocidos viajan: {:?}",
            unknown.errors[0]
        );
        let unknown_version = super::get_diagram_version("session-a", "d-9999", 1);
        assert!(!unknown_version.ok);
        assert_eq!(unknown_version.errors[0].code, "unknown_diagram");
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
        assert_eq!(graph.limits.max_nodes, 300);
        assert_eq!(graph.limits.max_edges, 600);
        assert!(graph.min_example.get("kind").is_some());

        // The entry promises exactly the fields the product has: an index that
        // announces an export format the tool cannot deliver lies to the agent.
        let serialized = serde_json::to_value(graph).expect("the entry is serialized");
        let mut keys: Vec<String> = serialized
            .as_object()
            .expect("the entry is an object")
            .keys()
            .cloned()
            .collect();
        keys.sort();
        assert_eq!(
            keys,
            [
                "disponible",
                "engine",
                "kind",
                "limits",
                "min_example",
                "phase",
            ]
        );
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

    /// S11 de `f20-reuso.md`: la línea del vocabulario se deriva de `roles()`
    /// —cada rol con la forma con que se dibuja— y marca el rol por defecto.
    #[test]
    fn the_role_line_derives_from_the_roles_of_the_kind() {
        let rules = get_diagram_schema(Some("flowchart")).rules.expect("rules");
        let line = rules
            .iter()
            .find(|rule| rule.contains("decide su forma"))
            .expect("la línea del vocabulario");
        assert!(line.contains("`start` (elipse)"), "{line}");
        assert!(
            line.contains("`process` (redondeado, por defecto)"),
            "{line}"
        );
        assert!(line.contains("`note` (nota)"), "{line}");
    }

    /// S4 de `f19-formas.md`: `shapes[]` es la verdad del kind — lo que sus
    /// roles dibujan, no una lista escrita a mano en gx-mcp. El er que
    /// anunciaba `table`/`column` (que nunca fueron variantes de `Shape`) y
    /// los kinds que anunciaban una lista vacía mienten al agente.
    #[test]
    fn the_shapes_of_a_kind_are_its_own_truth() {
        let flowchart = get_diagram_schema(Some("flowchart"));
        assert!(flowchart.ok);
        let shapes = flowchart.shapes.expect("shapes");
        assert!(shapes.contains(&"diamond".to_string()), "{shapes:?}");
        assert!(shapes.contains(&"ellipse".to_string()), "{shapes:?}");
        assert!(shapes.contains(&"note".to_string()), "{shapes:?}");

        let er = get_diagram_schema(Some("er"));
        assert_eq!(
            er.shapes.expect("shapes"),
            vec!["box".to_string()],
            "er draws boxes: the table/column fiction dies"
        );

        let gantt = get_diagram_schema(Some("gantt"));
        assert_eq!(
            gantt.shapes.expect("shapes"),
            vec!["rounded".to_string()],
            "a kind without a role table draws its default"
        );
    }

    /// El índice de kinds le da al agente un ejemplo mínimo de cada kind: si
    /// el ejemplo declara otro `kind`, el agente escribe un documento que el
    /// registro rechaza y el catálogo miente.
    #[test]
    fn every_kind_has_a_minimal_example_of_its_own() {
        gx_kinds::register_all();
        for kind in gx_core::kind::kind_names() {
            let example = super::min_example_of(kind);
            assert_eq!(
                example.get("kind").and_then(|value| value.as_str()),
                Some(kind),
                "el ejemplo mínimo de `{kind}` declara otro kind: {example}"
            );
            assert_eq!(
                example.get("version").and_then(|value| value.as_u64()),
                Some(1)
            );
        }
    }

    /// S5/S6 de `f17-c4-kind.md`: el índice le da al agente el kind `c4` con
    /// sus techos y sus niveles, y el ejemplo mínimo que trae válida y pinta.
    #[test]
    fn the_c4_kind_is_listed_with_its_limits_and_its_levels() {
        let index = get_diagram_schema(None).kinds.expect("index");
        let entry = index
            .iter()
            .find(|entry| entry.kind == "c4")
            .expect("c4 is in the index");
        assert_eq!(entry.engine, "layered");
        assert_eq!(entry.phase, "F3");
        assert_eq!(entry.limits.max_nodes, 300);
        assert_eq!(entry.limits.max_edges, 600);
        assert!(entry.disponible);

        let example = entry.min_example.to_string();
        let validated = validate_diagram(&example);
        assert!(validated.ok, "{:?}", validated.errors);

        let schema = get_diagram_schema(Some("c4"));
        assert!(schema.ok);
        assert_eq!(schema.limits.expect("limits").max_nodes, 300);
        let rules = schema.rules.expect("rules");
        assert!(
            rules
                .iter()
                .any(|rule| rule.contains("level") && rule.contains("context")),
            "the rules teach the level discipline: {rules:?}"
        );
    }

    /// S8 de `f18-gantt.md`: el índice le da al agente el kind `gantt` con
    /// sus techos y sus reglas, y el ejemplo mínimo que trae válida y pinta.
    #[test]
    fn the_gantt_kind_is_listed_with_its_limits_and_its_rules() {
        let index = get_diagram_schema(None).kinds.expect("index");
        let entry = index
            .iter()
            .find(|entry| entry.kind == "gantt")
            .expect("gantt is in the index");
        assert_eq!(entry.engine, "lanes");
        assert_eq!(entry.phase, "F3");
        assert_eq!(entry.limits.max_nodes, 200);
        assert!(entry.disponible);

        let example = entry.min_example.to_string();
        let validated = validate_diagram(&example);
        assert!(validated.ok, "{:?}", validated.errors);

        let schema = get_diagram_schema(Some("gantt"));
        assert!(schema.ok);
        assert_eq!(schema.limits.expect("limits").max_nodes, 200);
        let rules = schema.rules.expect("rules");
        assert!(
            rules.iter().any(|rule| rule.contains("depends_on"))
                && rules.iter().any(|rule| rule.contains("`start`")),
            "the rules teach the spans and the dependencies: {rules:?}"
        );
    }

    /// S10 de `f20-reuso.md`: los límites que el kind declara y gx-core hace
    /// cumplir llegan al agente tal como son: un diagrama de carriles dice
    /// cuántos carriles y cuántos eventos admite, no solo cuántos nodos.
    #[test]
    fn the_limits_payload_carries_the_lanes_and_events_ceilings() {
        let index = get_diagram_schema(None).kinds.expect("index");
        let gantt = index
            .iter()
            .find(|entry| entry.kind == "gantt")
            .expect("gantt is in the index");
        let limits = serde_json::to_value(gantt.limits).expect("limits");
        assert_eq!(limits["max_lanes"], 40, "{limits}");
        assert_eq!(limits["max_events"], 200, "{limits}");

        let graph = index
            .iter()
            .find(|entry| entry.kind == "graph")
            .expect("graph is in the index");
        let limits = serde_json::to_value(graph.limits).expect("limits");
        assert_eq!(
            limits["max_lanes"], 0,
            "un grafo no tiene carriles: {limits}"
        );
        assert_eq!(limits["max_events"], 0, "{limits}");
    }

    /// S9 de `f20-reuso.md`: el ejemplo mínimo lo declara el kind, no una
    /// lista en gx-mcp. El catálogo y el schema de un kind sirven exactamente
    /// el documento del kind, ese documento valida, y un nombre que el
    /// registro no conoce no hereda el ejemplo de `graph`: el catálogo no
    /// puede inventarle un documento al agente.
    #[test]
    fn only_a_registered_kind_has_an_example() {
        gx_kinds::register_all();
        let index = get_diagram_schema(None).kinds.expect("index");
        for kind in gx_core::kind::kind_names() {
            let handler = gx_core::kind::for_kind(kind).expect("registered");
            let own = handler.min_example();
            assert_eq!(
                super::min_example_of(kind),
                own,
                "gx-mcp sirve otro ejemplo que el del kind `{kind}`"
            );
            let entry = index
                .iter()
                .find(|entry| entry.kind == kind)
                .unwrap_or_else(|| panic!("`{kind}` no está en el índice"));
            assert_eq!(
                entry.min_example, own,
                "el índice inventa el ejemplo de `{kind}`"
            );
            assert_eq!(
                get_diagram_schema(Some(kind)).examples.expect("examples"),
                vec![own.clone()],
                "el schema de `{kind}` sirve otro ejemplo"
            );
            let validated = validate_diagram(&own.to_string());
            assert!(
                validated.ok,
                "el ejemplo de `{kind}` no valida: {:?}",
                validated.errors
            );
        }
        assert!(
            super::min_example_of("nope").is_null(),
            "un kind fuera del registro no hereda el ejemplo de otro: {}",
            super::min_example_of("nope")
        );
    }

    /// S10 de `f20-reuso.md`: la fase sale del kind, y el índice sigue
    /// contando el roadmap que el producto promete.
    #[test]
    fn the_index_reports_the_phase_of_every_kind() {
        gx_kinds::register_all();
        let index = get_diagram_schema(None).kinds.expect("index");
        for (kind, phase) in [
            ("graph", "F1"),
            ("er", "F1"),
            ("flowchart", "F2"),
            ("sequence", "F2"),
            ("class", "F2"),
            ("state", "F2"),
            ("component", "F2"),
            ("dataflow", "F2"),
            ("tree", "F2"),
            ("mindmap", "F2"),
            ("timeline", "F2"),
            ("swimlane", "F2"),
            ("c4", "F3"),
            ("gantt", "F3"),
        ] {
            let entry = index
                .iter()
                .find(|entry| entry.kind == kind)
                .unwrap_or_else(|| panic!("`{kind}` no está en el índice"));
            assert_eq!(entry.phase, phase, "la fase de `{kind}`");
        }
    }

    /// S10 de `f20-reuso.md`: la regla de grupos se escribe para los kinds
    /// cuyo schema declara `groups` —los seis que dibujan grupos—, leída del
    /// kind y no de una lista en gx-mcp.
    #[test]
    fn the_groups_rule_follows_the_kinds_that_admit_groups() {
        gx_kinds::register_all();
        let with_groups = [
            "class",
            "component",
            "dataflow",
            "flowchart",
            "graph",
            "state",
        ];
        for kind in gx_core::kind::kind_names() {
            let admits = with_groups.contains(&kind);
            let handler = gx_core::kind::for_kind(kind).expect("registered");
            let declares = handler
                .schema()
                .get("properties")
                .and_then(|properties| properties.get("groups"))
                .is_some();
            assert_eq!(declares, admits, "`{kind}` declara `groups`");
            let rules = get_diagram_schema(Some(kind)).rules.expect("rules");
            assert_eq!(
                rules.iter().any(|rule| rule.contains("groups[].members")),
                admits,
                "`{kind}` enseña la regla de grupos: {rules:?}"
            );
        }
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

    // ---- La sesión en la superficie (S3, D2, D3) ------------------------

    /// An id of this run: the registry and the store are process-wide, so a
    /// test never shares one with its neighbours.
    fn unique(prefix: &str) -> String {
        format!("{prefix}-{}", std::process::id())
    }

    /// Registers a session with the identity a client declares (D1), so the
    /// family and the project of the answer are the ones the test chose.
    fn registered_session(id: &str, client: &str, project: &str) {
        let identity = crate::session::resolve(
            &crate::session::Declared {
                client: Some(client.to_string()),
                session: Some(id.to_string()),
                project: Some(project.to_string()),
            },
            &crate::session::ClientFingerprint::default(),
            &crate::session::ProcessFingerprint::default(),
        );
        crate::session::session_registry().connect(id, &identity);
    }

    /// S3/D2: the label round-trips trimmed and the answer is the session
    /// with which the human sees the agent; without a label it is `whoami`.
    #[test]
    fn register_session_names_the_session_and_answers_it() {
        let id = unique("named");
        registered_session(&id, "pi", "/home/x/graphium");

        let named = super::register_session(&id, Some("  presupuesto API  "));
        assert!(named.ok, "{named:?}");
        let session = named.session.expect("the session travels with the answer");
        assert_eq!(session.id, id);
        assert_eq!(session.family, "pi");
        assert_eq!(session.label, "presupuesto API", "the label is trimmed");
        assert_eq!(session.project.as_deref(), Some("/home/x/graphium"));
        assert_eq!(session.state, "active");

        // It round-trips: the registry kept the trimmed name.
        let stored = crate::session::session_registry()
            .get(&id)
            .expect("the session is registered");
        assert_eq!(stored.label, "presupuesto API");

        // Without a label the tool only answers the session (`whoami`).
        let whoami = super::register_session(&id, None);
        assert!(whoami.ok, "{whoami:?}");
        assert_eq!(
            whoami.session.expect("the session travels").label,
            "presupuesto API",
            "a `whoami` never renames the session"
        );
    }

    /// D2: a label with nothing in it names nothing: it is `invalid_value` at
    /// `label` and the session keeps the name it had, never a silent reset.
    #[test]
    fn an_empty_label_is_rejected_and_the_name_stays() {
        let id = unique("empty-label");
        registered_session(&id, "pi", "/home/x/graphium");
        crate::session::session_registry().set_label(&id, "presupuesto API");

        for label in ["", "   ", "\t\n "] {
            let rejected = super::register_session(&id, Some(label));
            assert!(!rejected.ok, "{label:?} names nothing");
            assert!(rejected.session.is_none(), "{label:?}");
            assert_eq!(rejected.errors.len(), 1, "{rejected:?}");
            assert_eq!(rejected.errors[0].path, "label");
            assert_eq!(rejected.errors[0].code, "invalid_value");
            assert!(
                rejected.errors[0].hint.is_some(),
                "the agent needs a way out of the error: {rejected:?}"
            );
            assert_eq!(
                crate::session::session_registry()
                    .get(&id)
                    .expect("the session is registered")
                    .label,
                "presupuesto API",
                "{label:?} never resets the name"
            );
        }
    }

    /// S3/D2: `list_diagrams` answers with the session of the caller, in the
    /// same shape `register_session` gives, next to the diagrams it already
    /// answered.
    #[test]
    fn list_diagrams_carries_the_session_of_the_caller() {
        let _guard = crate::test_support::lock_renders_blocking();
        let id = unique("listed");
        registered_session(&id, "pi", "/home/x/graphium");
        crate::session::session_registry().set_label(&id, "presupuesto API");
        render_into_store(MINIMAL, &id);

        let listed = super::list_diagrams(&id);
        assert!(listed.ok);
        assert_eq!(listed.diagrams.len(), 1);
        assert_eq!(
            listed.diagrams[0].session_id, id,
            "the cerco of a diagram is still the session"
        );
        assert_eq!(listed.session.id, id);
        assert_eq!(listed.session.family, "pi");
        assert_eq!(listed.session.label, "presupuesto API");
        assert_eq!(listed.session.state, "active");
    }

    /// D3: a session sees only its own diagrams and answers with its own id,
    /// never the id of the session next door.
    #[test]
    fn a_second_session_lists_its_own_and_never_the_neighbours() {
        let _guard = crate::test_support::lock_renders_blocking();
        let first = unique("iso-first");
        let second = unique("iso-second");
        registered_session(&first, "pi", "/work/graphium");
        registered_session(&second, "claude", "/work/api");
        render_into_store(MINIMAL, &first);

        let mine = super::list_diagrams(&second);
        assert!(mine.ok);
        assert!(mine.diagrams.is_empty(), "{:?}", mine.diagrams);
        assert_eq!(mine.session.id, second, "each session answers as itself");
        assert_eq!(mine.session.family, "claude");
        assert_eq!(mine.session.project.as_deref(), Some("/work/api"));

        let theirs = super::list_diagrams(&first);
        assert_eq!(theirs.diagrams.len(), 1);
        assert_eq!(theirs.session.id, first);
    }

    /// PRESERVE: the fields `list_diagrams` already answered keep their names
    /// and their place; `session` is the only key added.
    #[test]
    fn list_diagrams_keeps_the_fields_it_had() {
        let _guard = crate::test_support::lock_renders_blocking();
        let id = unique("preserve-list");
        registered_session(&id, "pi", "/work/graphium");
        render_into_store(MINIMAL, &id);

        let value = serde_json::to_value(super::list_diagrams(&id)).expect("the answer is JSON");
        assert_eq!(value["ok"], true);
        let entry = &value["diagrams"][0];
        for key in [
            "diagram_id",
            "title",
            "kind",
            "updated_at",
            "session_id",
            "revision",
        ] {
            assert!(entry.get(key).is_some(), "falta `{key}`: {entry}");
        }
        assert_eq!(entry["session_id"].as_str(), Some(id.as_str()));

        let object = value.as_object().expect("the answer is an object");
        assert_eq!(
            object.len(),
            3,
            "`session` is the only key added: {object:?}"
        );
        for key in ["ok", "diagrams", "session"] {
            assert!(object.contains_key(key), "falta `{key}`: {object:?}");
        }
    }
}

#[cfg(test)]
mod version_truth {
    use super::{get_diagram_schema, validate_diagram};
    use serde_json::json; // (queda para el assert)

    // T1 (`f23-contrato.md`, S2): the schema answers WHEN a field exists —
    // the kind's contract version travels in the response, and every field
    // of the schema carries `x-graphium-since`.
    #[test]
    fn the_schema_publishes_the_kind_version_and_the_field_since() {
        let response = get_diagram_schema(Some("sequence"));
        assert!(response.ok);
        assert_eq!(response.kind_version, Some(1), "la versión del kind viaja");

        let schema = response.schema.expect("el schema del kind");
        // schemars parks the structs in $defs: the opt-in field of f22 lives
        // there, and the stamp reaches it through the same walk.
        let activation = &schema["$defs"]["Message"]["properties"]["activation"];
        assert_eq!(
            activation["x-graphium-since"],
            json!(1),
            "el campo opt-in dice desde cuándo existe: {activation}"
        );
        assert_eq!(
            schema["properties"]["participants"]["x-graphium-since"],
            json!(1),
            "todo campo declara su since: {schema}"
        );
    }

    // T1: `validate_diagram` answers with the versions this build supports —
    // it always returned an empty list (contract.rs:97-157).
    #[test]
    fn validate_diagram_carries_the_supported_versions() {
        crate::bootstrap();
        let document = r#"{
            "version": 1, "kind": "graph",
            "nodes": [ { "id": "a" }, { "id": "b" } ],
            "edges": [ { "from": "a", "to": "b" } ]
        }"#;
        let response = validate_diagram(document);
        assert!(response.ok, "{response:?}");
        assert_eq!(response.supported_versions, vec![1]);
    }
}

#[cfg(test)]
mod error_paths {
    use crate::contract::validate_diagram;
    use gx_core::error::ErrorCode;

    /// The first error of a document the tool rejects.
    fn first(document: &str) -> crate::contract::ErrorPayload {
        let response = validate_diagram(document);
        assert!(!response.ok, "el documento debe ser inválido: {document}");
        response.errors.into_iter().next().expect("un error")
    }

    // T2 (`f23-contrato.md`, S3): an unknown field says WHERE it is, with the
    // index style every other error uses (`nodes[0].labl`), not a bare name.
    #[test]
    fn an_unknown_field_reports_its_place_in_the_document() {
        let error = first(
            r#"{ "version": 1, "kind": "graph",
                 "nodes": [ { "id": "a", "labl": "A" } ], "edges": [] }"#,
        );
        assert_eq!(error.code, ErrorCode::UnknownField.as_str());
        assert_eq!(
            error.path, "nodes[0].labl",
            "el path dice dónde está el campo: {}",
            error.path
        );
        let hint = error.hint.expect("el hint propone el campo correcto");
        assert!(hint.contains("label"), "did you mean label: {hint}");
    }

    // T2: a field with the wrong type — or a missing required field — reports
    // the path instead of the empty string the schema failure used to return.
    #[test]
    fn a_schema_failure_reports_the_field_it_failed_in() {
        let wrong_type = first(
            r#"{ "version": 1, "kind": "graph",
                 "nodes": "no soy una lista", "edges": [] }"#,
        );
        assert_eq!(wrong_type.path, "nodes", "{}", wrong_type.message);
        assert_eq!(wrong_type.code, ErrorCode::InvalidSchema.as_str());

        // `nodes` carries `serde(default)`: the missing field is `version`.
        let missing = first(r#"{ "kind": "graph", "nodes": [], "edges": [] }"#);
        assert_eq!(missing.path, "version", "{}", missing.message);
        assert_eq!(missing.code, ErrorCode::MissingField.as_str());
        assert!(
            missing.hint.is_some(),
            "todo error dice cómo arreglarse: {missing:?}"
        );

        // Nested: a message without its label.
        let nested = first(
            r#"{ "version": 1, "kind": "sequence",
                 "participants": [ { "id": "api" } ], "messages": [ { "from": "api", "to": "api" } ] }"#,
        );
        assert_eq!(nested.path, "messages[0].label", "{}", nested.message);
    }

    // T2: the empty id — a rule every writer breaks at least once — carries a
    // hint in the kinds that had `None`.
    #[test]
    fn an_empty_id_says_what_to_write() {
        for (kind, document) in [
            (
                "sequence",
                r#"{ "version": 1, "kind": "sequence", "participants": [ { "id": "  " } ] }"#,
            ),
            (
                "graph",
                r#"{ "version": 1, "kind": "graph", "nodes": [ { "id": "" } ], "edges": [] }"#,
            ),
        ] {
            let error = first(document);
            assert_eq!(
                error.path,
                "participants[0].id".replace(
                    "participants",
                    if kind == "graph" {
                        "nodes"
                    } else {
                        "participants"
                    }
                )
            );
            let hint = error
                .hint
                .unwrap_or_else(|| panic!("{kind}: el id vacío dice qué escribir"));
            assert!(!hint.is_empty(), "{kind}: {hint}");
        }
    }
}

#[cfg(test)]
mod teaching_truth {
    use super::get_diagram_schema;

    // T3 (`f23-contrato.md`, S4): the example of a kind shows its opt-in
    // fields — what f22 added is not a secret an agent has to read the whole
    // `$defs` to discover.
    #[test]
    fn the_examples_show_the_fields_the_kinds_added() {
        /// Whether a key appears anywhere in the example.
        fn mentions(value: &serde_json::Value, key: &str) -> bool {
            match value {
                serde_json::Value::Object(map) => {
                    map.contains_key(key) || map.values().any(|child| mentions(child, key))
                }
                serde_json::Value::Array(items) => items.iter().any(|item| mentions(item, key)),
                _ => false,
            }
        }

        for (kind, what) in [
            ("sequence", "activation"),
            ("class", "attributes"),
            ("class", "methods"),
            ("swimlane", "connects"),
            ("timeline", "role"),
        ] {
            let response = get_diagram_schema(Some(kind));
            let example = &response.examples.expect("el ejemplo del kind")[0];
            assert!(
                mentions(example, what),
                "{kind}: el ejemplo no muestra `{what}`: {example}"
            );
        }
    }

    // T3: every kind teaches its own rules — `sequence`, `tree` and `mindmap`
    // answered only the two generic lines.
    #[test]
    fn every_kind_publishes_its_own_rules() {
        let mut thin = Vec::new();
        for kind in gx_core::kind::kind_names() {
            let response = get_diagram_schema(Some(kind));
            let rules = response.rules.unwrap_or_default();
            if rules.len() < 3 {
                thin.push((kind, rules.len()));
            }
        }
        assert!(thin.is_empty(), "kinds sin reglas propias: {thin:?}");
    }

    // T3: the line-break convention is written where the writer reads it —
    // the generic rules of every kind.
    #[test]
    fn the_generic_rules_say_how_a_label_breaks() {
        let response = get_diagram_schema(Some("graph"));
        let rules = response.rules.expect("las reglas del kind");
        assert!(
            rules.iter().any(|rule| rule.contains("\\n")),
            "las reglas dicen cómo se corta una etiqueta: {rules:?}"
        );
    }
}
