//! The `render_diagram` tool (PRD §9.1, §11, S21).
//!
//! Every render goes through the single engine door
//! ([`gx_core::engine::render_json`]): resolve the kind, validate against its
//! schema, build the IR, lay out and render. There is no second pipeline here.
//!
//! The finished SVG is handed to the window through the render hook that
//! `gx-app` installs (PRD §8.6.1 step 3): the hook emits
//! `graphium://render { req_id, diagram_id, svg, place }` and answers whether
//! the frontend confirmed the paint after two animation frames. Without a hook
//! the diagram is rendered and stored but not painted.

use std::sync::{Arc, OnceLock, RwLock};

use gx_core::error::ValidationError;
use gx_core::render::{RenderMetrics, RenderOpts};
use serde::Serialize;

use crate::store::diagram_store;

/// A request to render one diagram document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderRequest {
    /// The diagram document, already serialized as JSON.
    pub document: String,
    /// The MCP session that owns the diagram (PRD §11.1).
    pub session_id: String,
}

/// The response of `render_diagram` (PRD §11, S21).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RenderResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagram_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendered_kind: Option<String>,
    pub rendered: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<Metrics>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<ErrorPayload>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<Warning>,
}

/// Cost of a render, reported to the agent (PRD §7.3, RF20-bis).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Metrics {
    pub nodes: u64,
    pub edges: u64,
    pub layout_ms: u64,
    pub render_ms: u64,
    pub svg_bytes: u64,
}

impl From<RenderMetrics> for Metrics {
    fn from(metrics: RenderMetrics) -> Self {
        Self {
            nodes: metrics.nodes as u64,
            edges: metrics.edges as u64,
            layout_ms: metrics.layout_ms,
            render_ms: metrics.render_ms,
            svg_bytes: metrics.svg_bytes as u64,
        }
    }
}

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

/// A non-fatal note about a render.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Warning {
    pub code: String,
    pub message: String,
}

impl Warning {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
        }
    }
}

/// Everything `gx-app` needs to paint a diagram (PRD §8.6.1 step 3).
#[derive(Debug, Clone, PartialEq)]
pub struct RenderHookPayload {
    pub diagram_id: String,
    pub kind: String,
    pub svg: String,
    /// Width of the diagram in plan units, from the layout bounding box.
    pub width: f64,
    /// Height of the diagram in plan units, from the layout bounding box.
    pub height: f64,
    pub metrics: Metrics,
}

/// The callback that paints a diagram in the window.
///
/// Returning `true` means the window confirmed the paint (`render_ack`).
pub type RenderHook = Arc<dyn Fn(&RenderHookPayload) -> bool + Send + Sync>;

fn render_hook_slot() -> &'static RwLock<Option<RenderHook>> {
    static SLOT: OnceLock<RwLock<Option<RenderHook>>> = OnceLock::new();
    SLOT.get_or_init(|| RwLock::new(None))
}

/// Installs (or removes) the hook that paints the diagram in the window.
///
/// `gx-app` owns the webview and the round-trip; `None` means headless, and
/// every render reports `rendered: false`.
pub fn set_render_hook(hook: Option<RenderHook>) {
    *render_hook_slot()
        .write()
        .expect("the render hook slot is not poisoned") = hook;
}

/// The installed hook, if any.
fn installed_hook() -> Option<RenderHook> {
    render_hook_slot()
        .read()
        .expect("the render hook slot is not poisoned")
        .clone()
}

/// Renders a diagram document through the single engine door
/// ([`gx_core::engine::render_json`]) and stores it in the diagram store.
///
/// The door is the only path from JSON to SVG: it resolves the kind through
/// the registry, validates against the kind schema, builds the IR, measures
/// the text, lays out and renders. On success the diagram is stored and then
/// handed to the render hook; `rendered` is whatever the hook answered.
pub fn render_diagram(request: &RenderRequest) -> RenderResponse {
    crate::bootstrap();

    let outcome = match gx_core::engine::render_json(&request.document, &RenderOpts::default()) {
        Ok(outcome) => outcome,
        Err(errors) => {
            return RenderResponse {
                ok: false,
                diagram_id: None,
                rendered_kind: None,
                rendered: false,
                metrics: None,
                errors: errors.iter().map(ErrorPayload::from).collect(),
                warnings: Vec::new(),
            };
        }
    };

    // The document is valid JSON from here on: the door just accepted it.
    let document: serde_json::Value =
        serde_json::from_str(&request.document).unwrap_or(serde_json::Value::Null);
    let kind = document
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string();
    let title = document
        .get("title")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .unwrap_or("Untitled")
        .to_string();

    let metrics = Metrics::from(outcome.metrics);
    // Bounding box of the layout: the size of the diagram in plan units.
    let width = outcome.bbox.w;
    let height = outcome.bbox.h;

    let store = diagram_store();
    let diagram_id = store.next_id();
    store.insert(
        diagram_id.clone(),
        title,
        kind.clone(),
        request.session_id.clone(),
    );

    let mut warnings = Vec::new();
    let rendered = match installed_hook() {
        Some(hook) => hook(&RenderHookPayload {
            diagram_id: diagram_id.clone(),
            kind: kind.clone(),
            svg: outcome.svg,
            width,
            height,
            metrics,
        }),
        None => {
            warnings.push(Warning::new(
                "no_window",
                "the diagram was rendered and stored but not painted: no window hook is installed",
            ));
            false
        }
    };

    RenderResponse {
        ok: true,
        diagram_id: Some(diagram_id),
        rendered_kind: Some(kind),
        rendered,
        metrics: Some(metrics),
        errors: Vec::new(),
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::{render_diagram, set_render_hook, RenderHook, RenderRequest, RenderResponse};
    use crate::store::diagram_store;
    use crate::test_support::{lock_renders, MINIMAL};

    fn render(document: &str) -> RenderResponse {
        render_diagram(&RenderRequest {
            document: document.to_string(),
            session_id: "session-test".to_string(),
        })
    }

    fn error_codes(response: &RenderResponse) -> Vec<String> {
        response
            .errors
            .iter()
            .map(|error| error.code.clone())
            .collect()
    }

    #[test]
    fn renders_the_minimal_example_of_the_prd() {
        let _guard = lock_renders();

        let response = render(MINIMAL);

        assert!(response.ok, "{:?}", response.errors);
        assert_eq!(response.diagram_id.as_deref(), Some("d-0001"));
        assert_eq!(response.rendered_kind.as_deref(), Some("graph"));
        assert!(response.errors.is_empty());
        let metrics = response.metrics.expect("metrics are reported");
        assert_eq!(metrics.nodes, 2);
        assert_eq!(metrics.edges, 1);
        assert!(metrics.svg_bytes > 0, "{metrics:?}");
        // No hook: rendered, stored, but not painted.
        assert!(!response.rendered);
        assert_eq!(response.warnings[0].code, "no_window");

        // The wire shape of S21: empty error and warning lists are omitted.
        let json = serde_json::to_value(&response).expect("the response serializes");
        assert_eq!(json["ok"], true);
        assert_eq!(json["diagram_id"], "d-0001");
        assert_eq!(json["rendered_kind"], "graph");
        assert_eq!(json["rendered"], false);
        assert!(json["metrics"].is_object(), "{json}");
        assert!(json["warnings"].is_array(), "{json}");
        assert!(json.get("errors").is_none(), "{json}");
    }

    #[test]
    fn ids_are_monotonic_across_renders() {
        let _guard = lock_renders();

        let first = render(MINIMAL);
        let second = render(MINIMAL);

        assert_eq!(first.diagram_id.as_deref(), Some("d-0001"));
        assert_eq!(second.diagram_id.as_deref(), Some("d-0002"));
        let listed = diagram_store().list();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].diagram_id, "d-0001");
        assert_eq!(listed[1].diagram_id, "d-0002");
        assert_eq!(listed[0].title, "parse_config() — flujo");
        assert_eq!(listed[0].session_id, "session-test");
    }

    #[test]
    fn invalid_json_is_reported() {
        let _guard = lock_renders();

        let response = render("{ \"version\": 1, ");

        assert!(!response.ok);
        assert_eq!(error_codes(&response), vec!["invalid_json"]);
        assert_eq!(response.diagram_id, None);
        assert_eq!(response.metrics, None);
        assert!(!response.rendered);
        assert!(diagram_store().list().is_empty());
    }

    #[test]
    fn an_unknown_kind_is_reported() {
        let _guard = lock_renders();

        let response = render(r#"{ "version": 1, "kind": "er" }"#);

        assert!(!response.ok);
        assert_eq!(error_codes(&response), vec!["unknown_kind"]);
        assert_eq!(response.errors[0].path, "kind");
        assert!(response.errors[0].hint.is_some());
        assert!(diagram_store().list().is_empty());
    }

    #[test]
    fn an_unknown_field_is_reported() {
        let _guard = lock_renders();

        let response = render(
            r#"{
              "version": 1,
              "kind": "graph",
              "nodes": [ { "id": "a", "colour": "red" } ]
            }"#,
        );

        assert!(!response.ok);
        assert_eq!(error_codes(&response), vec!["unknown_field"]);
        assert!(diagram_store().list().is_empty());
    }

    #[test]
    fn the_render_hook_confirms_the_paint() {
        let _guard = lock_renders();

        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorder = Arc::clone(&seen);
        let hook: RenderHook = Arc::new(move |payload| {
            recorder
                .lock()
                .expect("the recorder is not poisoned")
                .push((
                    payload.diagram_id.clone(),
                    payload.kind.clone(),
                    payload.svg.clone(),
                    payload.width,
                    payload.height,
                ));
            true
        });
        set_render_hook(Some(hook));

        let response = render(MINIMAL);

        assert!(response.ok);
        assert!(response.rendered);
        assert!(response.warnings.is_empty(), "{:?}", response.warnings);
        let seen = seen.lock().expect("the recorder is not poisoned");
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].0, "d-0001");
        assert_eq!(seen[0].1, "graph");
        assert!(seen[0].2.starts_with("<svg"), "{}", seen[0].2);
        assert!(seen[0].3 > 0.0);
        assert!(seen[0].4 > 0.0);

        set_render_hook(None);
    }

    #[test]
    fn a_rejected_paint_keeps_the_diagram_unrendered() {
        let _guard = lock_renders();

        set_render_hook(Some(Arc::new(|_payload| false)));
        let response = render(MINIMAL);
        set_render_hook(None);

        assert!(response.ok);
        assert!(!response.rendered);
        assert_eq!(response.diagram_id.as_deref(), Some("d-0001"));
    }
}
