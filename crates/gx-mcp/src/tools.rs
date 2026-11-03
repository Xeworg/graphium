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
    /// Why the diagram was not painted: `headless`, `no_window`,
    /// `ui_timeout` or `ui_error` (PRD §8.6.1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
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
///
/// One type for the whole crate: the render and the dry run report warnings
/// with the same shape and the same vocabulary (PRD §7 RF20).
pub use crate::contract::Warning;

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

/// The outcome of the paint in the window (PRD §8.6.1).
///
/// The reasons of a `rendered: false` are stable: the agent branches on
/// them, so they never change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderStatus {
    /// The window confirmed the paint.
    Painted,
    /// There is no display: the process works headless.
    Headless,
    /// There is no window to paint in.
    NoWindow,
    /// The window did not answer within the timeout.
    Timeout,
    /// The window refused the render or failed.
    Error,
}

impl RenderStatus {
    /// The stable reason of the status, `None` when the diagram was painted.
    pub fn reason(&self) -> Option<&'static str> {
        match self {
            RenderStatus::Painted => None,
            RenderStatus::Headless => Some("headless"),
            RenderStatus::NoWindow => Some("no_window"),
            RenderStatus::Timeout => Some("ui_timeout"),
            RenderStatus::Error => Some("ui_error"),
        }
    }

    /// Whether the diagram reached the screen.
    pub fn painted(&self) -> bool {
        matches!(self, RenderStatus::Painted)
    }
}

/// The callback that paints a diagram in the window.
///
/// The status is what the window knows: only it can tell a timeout from a
/// refusal or from the absence of a display.
pub type RenderHook = Arc<dyn Fn(&RenderHookPayload) -> RenderStatus + Send + Sync>;

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
                reason: None,
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
    // The size of what was just painted is reported, never hidden: the same
    // warning the dry run gives, in the same words (PRD §7 RF20, S6).
    let mut warnings = match gx_core::kind::for_kind(&kind) {
        Some(registered) => crate::contract::size_warnings(
            outcome.metrics.nodes,
            outcome.metrics.edges,
            registered.limits(),
        ),
        None => Vec::new(),
    };
    // Bounding box of the layout: the size of the diagram in plan units.
    let width = outcome.bbox.w;
    let height = outcome.bbox.h;

    let store = diagram_store();
    let diagram_id = store.next_id();
    // The document is kept normalized: `get_diagram` returns exactly this.
    let document = serde_json::to_string(
        &serde_json::from_str::<serde_json::Value>(&request.document)
            .unwrap_or(serde_json::Value::Null),
    )
    .unwrap_or_else(|_| request.document.clone());
    store.insert(
        diagram_id.clone(),
        title,
        kind.clone(),
        request.session_id.clone(),
        document,
    );

    let status = match installed_hook() {
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
            RenderStatus::NoWindow
        }
    };
    if !status.painted() {
        if let Some(reason) = status.reason() {
            warnings.push(Warning::new(
                reason,
                format!("the diagram was rendered and stored but not painted: {reason}"),
            ));
        }
    }

    RenderResponse {
        ok: true,
        diagram_id: Some(diagram_id),
        rendered_kind: Some(kind),
        rendered: status.painted(),
        reason: status.reason().map(str::to_string),
        metrics: Some(metrics),
        errors: Vec::new(),
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::{
        render_diagram, set_render_hook, RenderHook, RenderRequest, RenderResponse, RenderStatus,
    };
    use crate::store::diagram_store;
    use crate::test_support::{lock_renders_blocking, MINIMAL};

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

    fn warning_codes(response: &RenderResponse) -> Vec<String> {
        response
            .warnings
            .iter()
            .map(|warning| warning.code.clone())
            .collect()
    }

    /// A chain of `nodes` nodes: wide enough to test the size ladder.
    fn chain(nodes: usize) -> String {
        let nodes: Vec<String> = (0..nodes)
            .map(|i| format!(r#"{{ "id": "n{i}", "label": "nodo {i}" }}"#))
            .collect();
        let edges: Vec<String> = (0..nodes.len().saturating_sub(1))
            .map(|i| format!(r#"{{ "from": "n{i}", "to": "n{}" }}"#, i + 1))
            .collect();
        format!(
            r#"{{ "version": 1, "kind": "graph", "title": "cadena",
                 "nodes": [{}], "edges": [{}] }}"#,
            nodes.join(", "),
            edges.join(", ")
        )
    }

    /// PRD §7 RF20, S6: between 150 nodes and the hard limit the diagram is
    /// not rejected; the render reports the size in `warnings[]`.
    #[test]
    fn a_big_diagram_renders_with_the_size_warning() {
        let _guard = lock_renders_blocking();

        let response = render(&chain(160));

        assert!(response.ok, "{:?}", response.errors);
        assert_eq!(response.metrics.expect("metrics").nodes, 160);
        let codes = warning_codes(&response);
        assert!(
            codes.contains(&"too_many_nodes".to_string()),
            "the render has to report the size it just painted: {codes:?}"
        );
    }

    /// The hard limit is the one thing that rejects (PRD §7.3, S6): the
    /// error names the count it received and the maximum, and the plane is
    /// left untouched.
    #[test]
    fn a_diagram_over_the_hard_limit_is_rejected_and_not_stored() {
        let _guard = lock_renders_blocking();

        let response = render(&chain(301));

        assert!(!response.ok);
        assert_eq!(error_codes(&response), vec!["too_large"]);
        assert!(
            response.errors[0].message.contains("301"),
            "the error names the count it received: {:?}",
            response.errors[0]
        );
        assert!(response.errors[0].message.contains("300"));
        assert!(response.errors[0].hint.is_some());
        assert!(response.metrics.is_none());
        assert!(
            diagram_store().list().is_empty(),
            "a rejected diagram never reaches the plane"
        );
    }

    #[test]
    fn a_diagram_under_the_warning_line_has_no_size_warning() {
        let _guard = lock_renders_blocking();

        let response = render(&chain(150));

        assert!(response.ok, "{:?}", response.errors);
        let codes = warning_codes(&response);
        assert!(
            !codes.contains(&"too_many_nodes".to_string()),
            "150 nodes are exactly the line: {codes:?}"
        );
    }

    #[test]
    fn renders_the_minimal_example_of_the_prd() {
        let _guard = lock_renders_blocking();

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
        let _guard = lock_renders_blocking();

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
        let _guard = lock_renders_blocking();

        let response = render("{ \"version\": 1, ");

        assert!(!response.ok);
        assert_eq!(error_codes(&response), vec!["invalid_json"]);
        assert_eq!(response.diagram_id, None);
        assert_eq!(response.metrics, None);
        assert!(!response.rendered);
        assert!(diagram_store().list().is_empty());
    }

    /// The `er` kind draws tables with their columns and a crow's foot
    /// relation (PRD §6.1, H2).
    #[test]
    fn the_er_kind_renders_a_schema() {
        let _guard = lock_renders_blocking();

        let document = r#"{
          "version": 1, "kind": "er", "title": "Schema — posts",
          "tables": [
            { "name": "users", "columns": [ { "name": "id", "type": "uuid", "key": "pk" } ] },
            { "name": "posts", "columns": [
                { "name": "id", "type": "uuid", "key": "pk" },
                { "name": "user_id", "type": "uuid", "key": "fk", "nullable": true }
            ] }
          ],
          "relations": [
            { "from": { "table": "users", "column": "id" },
              "to": { "table": "posts", "column": "user_id" },
              "cardinality": "1:N" }
          ]
        }"#;
        let response = render(document);

        assert!(response.ok, "{response:?}");
        assert_eq!(response.rendered_kind.as_deref(), Some("er"));
        assert_eq!(response.metrics.expect("metrics").nodes, 2);
    }

    #[test]
    fn an_unknown_kind_is_reported() {
        let _guard = lock_renders_blocking();

        let response = render(r#"{ "version": 1, "kind": "sankey" }"#);

        assert!(!response.ok);
        assert_eq!(error_codes(&response), vec!["unknown_kind"]);
        assert_eq!(response.errors[0].path, "kind");
        assert!(response.errors[0].hint.is_some());
        assert!(diagram_store().list().is_empty());
    }

    #[test]
    fn an_unknown_field_is_reported() {
        let _guard = lock_renders_blocking();

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
        let _guard = lock_renders_blocking();

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
            RenderStatus::Painted
        });
        set_render_hook(Some(hook));

        let response = render(MINIMAL);

        assert!(response.ok);
        assert!(response.rendered);
        assert_eq!(response.reason, None);
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
        let _guard = lock_renders_blocking();

        set_render_hook(Some(Arc::new(|_payload| RenderStatus::Error)));
        let response = render(MINIMAL);
        set_render_hook(None);

        assert!(response.ok);
        assert!(!response.rendered);
        assert_eq!(response.diagram_id.as_deref(), Some("d-0001"));
        assert_eq!(response.reason.as_deref(), Some("ui_error"));
    }

    /// The reasons of a render that did not reach the screen are stable:
    /// the agent branches on them (PRD §8.6.1).
    #[test]
    fn every_reason_of_an_unpainted_render_is_stable() {
        let _guard = lock_renders_blocking();

        for (status, reason) in [
            (RenderStatus::Headless, "headless"),
            (RenderStatus::NoWindow, "no_window"),
            (RenderStatus::Timeout, "ui_timeout"),
            (RenderStatus::Error, "ui_error"),
        ] {
            set_render_hook(Some(Arc::new(move |_payload| status)));
            let response = render(MINIMAL);
            set_render_hook(None);

            assert!(!response.rendered, "{reason}");
            assert_eq!(response.reason.as_deref(), Some(reason));
            assert!(
                response.warnings.iter().any(|w| w.code == reason),
                "the warning has to carry the reason: {:?}",
                response.warnings
            );
        }
    }
}
