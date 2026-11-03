//! Test-only helpers shared by the crate tests.
//!
//! The stub engines keep the tests of `gx-mcp` free of `gx-layout`,
//! `gx-render-svg`, Tauri, webkit and the webview (PRD §9.1): the render path
//! runs through the single engine door with injected stubs.

use std::sync::{Arc, Once};

use gx_core::engine::{install, Engines};
use gx_core::geom::{Point, Rect, Size};
use gx_core::ir::{Ir, IrGraph};
use gx_core::layout::{LayoutEdge, LayoutEngine, LayoutGraph, NodeBox};
use gx_core::render::{Palette, SvgRenderer};
use gx_core::text::{TextMetrics, TextStyle};

use crate::store::diagram_store;
use crate::tools;

/// The minimal `graph` example of PRD §6.1.
pub(crate) const MINIMAL: &str = r#"{
  "version": 1,
  "kind": "graph",
  "title": "parse_config() — flujo",
  "direction": "TB",
  "nodes": [
    { "id": "read", "label": "read file" },
    { "id": "ok",   "label": "Config" }
  ],
  "edges": [
    { "from": "read", "to": "ok", "label": "utf-8" }
  ]
}"#;

/// A layout stub: a single row, no measuring of its own.
struct StubLayout;

impl LayoutEngine for StubLayout {
    fn layout(&self, graph: &IrGraph, _opts: &gx_core::layout::LayoutOpts) -> LayoutGraph {
        let mut nodes = std::collections::BTreeMap::new();
        let mut x = 0.0;
        let mut height = 0.0_f64;
        for node in &graph.nodes {
            let w = node.size.w.max(40.0);
            let h = node.size.h.max(20.0);
            height = height.max(h);
            nodes.insert(
                node.id.clone(),
                NodeBox {
                    center: Point::new(x + w / 2.0, h / 2.0),
                    size: Size::new(w, h),
                },
            );
            x += w + 40.0;
        }
        let edges = graph
            .edges
            .iter()
            .map(|edge| LayoutEdge {
                from: edge.from.clone(),
                to: edge.to.clone(),
                points: Vec::new(),
                label_at: None,
            })
            .collect();
        LayoutGraph {
            nodes,
            edges,
            groups: Vec::new(),
            bbox: Rect::new(0.0, 0.0, x, height),
        }
    }
}

/// A metrics stub: half an em per character, one line per `\n`.
struct StubMetrics;

impl TextMetrics for StubMetrics {
    fn measure(&self, text: &str, style: &TextStyle) -> Size {
        let lines = text.lines().count().max(1) as f64;
        let width = text.chars().count() as f64 * style.size * 0.5;
        Size::new(
            width.max(style.size),
            lines * style.size * style.line_height,
        )
    }
}

/// A renderer stub: valid, non-empty SVG with the labels of nodes and edges.
struct StubRenderer;

/// When set, the stub renderer pads its SVG past the byte budget of
/// PRD §7.3, so the cut of the ladder can be tested without a real
/// 2 MB document.
static OVERSIZE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Makes the stub renderer produce an SVG past the budget (or not).
pub(crate) fn set_oversize(on: bool) {
    OVERSIZE.store(on, std::sync::atomic::Ordering::SeqCst);
}

impl SvgRenderer for StubRenderer {
    fn render(&self, ir: &Ir, layout: &LayoutGraph, _palette: &Palette) -> String {
        let graph = match ir {
            Ir::Graph(graph) => graph,
            Ir::Timeline(_) => return String::new(),
        };
        let mut svg = String::from("<svg xmlns=\"http://www.w3.org/2000/svg\">");
        for node in &graph.nodes {
            if let Some(box_) = layout.nodes.get(&node.id) {
                svg.push_str(&format!(
                    "<text x=\"{}\" y=\"{}\">{}</text>",
                    box_.center.x, box_.center.y, node.label
                ));
            }
        }
        // The edge labels: a renderer that hides them is the first step of
        // the degradation ladder (PRD §7.3).
        for edge in &graph.edges {
            if let Some(label) = &edge.label {
                svg.push_str(&format!("<text class=\"edge-label\">{label}</text>"));
            }
        }
        svg.push_str("</svg>");
        if OVERSIZE.load(std::sync::atomic::Ordering::SeqCst) {
            svg.push_str(&"<".repeat(tools::SVG_BUDGET_BYTES));
        }
        svg
    }
}

fn install_stub_engines() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        install(Engines {
            layout: Arc::new(StubLayout),
            metrics: Arc::new(StubMetrics),
            renderer: Arc::new(StubRenderer),
        });
    });
}

/// Serializes the tests that render into the global diagram store.
///
/// The lock is a `tokio` mutex on purpose: the async tests hold it across
/// `await` points and a blocking mutex would tie up the executor.
static RENDER_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Prepares the global state the render path touches: an empty store, no
/// window sink and the stub engines.
fn prepare() {
    diagram_store().reset_for_tests();
    tools::set_render_hook(None);
    set_oversize(false);
    install_stub_engines();
}

/// Serializes the tests that render, for the async tests.
pub(crate) async fn lock_renders() -> tokio::sync::MutexGuard<'static, ()> {
    let guard = RENDER_LOCK.lock().await;
    prepare();
    guard
}

/// Serializes the tests that render, for the synchronous tests.
pub(crate) fn lock_renders_blocking() -> tokio::sync::MutexGuard<'static, ()> {
    let guard = RENDER_LOCK.blocking_lock();
    prepare();
    guard
}
