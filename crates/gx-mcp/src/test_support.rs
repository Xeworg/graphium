//! Test-only helpers shared by the crate tests.
//!
//! The stub engines keep the tests of `gx-mcp` free of `gx-layout`,
//! `gx-render-svg`, Tauri, webkit and the webview (PRD §9.1): the render path
//! runs through the single engine door with injected stubs.

use std::sync::{Arc, Once};

use gx_core::engine::{install, Engines};
use gx_core::export::{RasterError, RasterImage, RasterOptions, Rasterizer};
use gx_core::geom::{Point, Rect, Size};
use gx_core::ir::{Ir, IrGraph};
use gx_core::layout::{LayoutEdge, LayoutEngine, LayoutGraph, NodeBox};
use gx_core::render::{Palette, SvgRenderer};
use gx_core::text::{TextMetrics, TextStyle};
use gx_core::{
    Scene, SceneBand, SceneEdge, SceneEvent, SceneNode, ScenePalette, SceneTokens, TOKENS,
};

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
        let delay = LAYOUT_DELAY_MS.load(std::sync::atomic::Ordering::SeqCst);
        if delay > 0 {
            std::thread::sleep(std::time::Duration::from_millis(delay));
        }
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
            activations: Vec::new(),
            axis: None,
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

/// How long the layout stub takes, in milliseconds. A slow layout is the
/// only way to miss the deadline of PRD §7.3 without a huge diagram.
static LAYOUT_DELAY_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Makes the next layouts of the stub take `ms` milliseconds (0: instant).
pub(crate) fn set_layout_delay(ms: u64) {
    LAYOUT_DELAY_MS.store(ms, std::sync::atomic::Ordering::SeqCst);
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

    /// The export drawing: the plan drawing plus the `class="bg"` rectangle
    /// the real renderer paints (PRD §8.5.2, S7). The marker is the same one
    /// `gx-render-svg` uses, so the engine door can be checked without a
    /// dependency on it (PRD §9.1).
    fn render_export(&self, ir: &Ir, layout: &LayoutGraph, palette: &Palette) -> String {
        let svg = self.render(ir, layout, palette);
        match svg.find('>') {
            Some(open) => {
                let mut exported = svg[..=open].to_string();
                exported.push_str("<rect class=\"bg\"/>");
                exported.push_str(&svg[open + 1..]);
                exported
            }
            None => svg,
        }
    }

    /// The scene of the stub (`f11-elementos.md` T3/C): since the flip the
    /// paint payload is the elements, so the test double has to produce one.
    ///
    /// It is the minimum the paint contract needs — the canvas of the layout,
    /// one `g-<id>` per placed node, the routed edges, the bands and the
    /// events — with the frozen tokens of the drawing. `gx-mcp` does not
    /// depend on the emitter (PRD §9.1), so the glyph constants are mirrored
    /// here: the *product* renderer is the one that writes the real numbers,
    /// and `crates/gx-app/tests/scene_payload.rs` checks that one.
    fn scene(&self, ir: &Ir, layout: &LayoutGraph, palette: &Palette) -> Option<Scene> {
        let graph = ir.graph();
        let timeline = ir.timeline();
        let mut nodes: Vec<SceneNode> = graph
            .map(|graph| {
                let mut ordered: Vec<_> = graph.nodes.iter().collect();
                ordered.sort_by(|left, right| left.id.cmp(&right.id));
                ordered
                    .into_iter()
                    .filter_map(|node| {
                        let placed = layout.nodes.get(&node.id)?;
                        Some(SceneNode {
                            id: format!("g-{}", node.id),
                            label: node.label.clone(),
                            shape: node.shape,
                            emphasis: node.emphasis,
                            style: node.style,
                            center: placed.center,
                            size: placed.size,
                            label_at: None,
                            rows: node.rows.clone(),
                            stereotype: node.stereotype.clone(),
                            meta: node.meta.clone(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        nodes.sort_by(|left, right| left.id.cmp(&right.id));

        let mut edges: Vec<SceneEdge> = graph
            .map(|graph| {
                graph
                    .edges
                    .iter()
                    .filter_map(|edge| {
                        let route = layout
                            .edges
                            .iter()
                            .find(|route| route.from == edge.from && route.to == edge.to)?;
                        Some(SceneEdge {
                            id: format!("m-{}-{}", edge.from, edge.to),
                            from: edge.from.clone(),
                            to: edge.to.clone(),
                            label: edge.label.clone(),
                            emphasis: edge.emphasis,
                            style: edge.style,
                            head: edge.head_end(),
                            tail: edge.tail_end(),
                            from_row: edge.from_row,
                            to_row: edge.to_row,
                            points: route.points.clone(),
                            label_at: route.label_at,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        edges.sort_by(|left, right| left.id.cmp(&right.id));

        let mut groups: Vec<_> = layout.groups.iter().collect();
        groups.sort_by(|left, right| {
            left.depth
                .cmp(&right.depth)
                .then_with(|| left.id.cmp(&right.id))
        });
        let bands: Vec<SceneBand> = groups
            .into_iter()
            .map(|group| SceneBand {
                id: if timeline.is_some() {
                    format!("lane-{}", group.id)
                } else {
                    format!("cluster-{}", group.id)
                },
                label: group.label.clone(),
                bbox: group.bbox,
                depth: group.depth,
                members: group.members.clone(),
                gutter: group.gutter,
                kind: if timeline.is_some() {
                    gx_core::BandKind::Lane
                } else {
                    gx_core::BandKind::Cluster
                },
            })
            .collect();

        let events: Vec<SceneEvent> = timeline
            .map(|timeline| {
                let mut ordered: Vec<_> = timeline.events.iter().collect();
                ordered.sort_by(|left, right| left.id.cmp(&right.id));
                ordered
                    .into_iter()
                    .filter_map(|event| {
                        let placed = layout.nodes.get(&event.id)?;
                        Some(SceneEvent {
                            id: format!("g-{}", event.id),
                            lane: event.lane.clone(),
                            label: event.label.clone(),
                            order: event.order,
                            shape: event.shape,
                            span: event.span,
                            center: placed.center,
                            size: placed.size,
                            // The bar of an event that lasts, mirrored from
                            // `geometry::event_bar` for the same reason as the
                            // tokens above: `gx-mcp` does not depend on
                            // `gx-render-svg` (PRD §9.1).
                            bar: event.span.filter(|span| span.end > span.start).map(|_| {
                                let inset = TOKENS.border_width;
                                let height = 5.0;
                                Rect::new(
                                    placed.center.x - placed.size.w / 2.0 + inset,
                                    placed.center.y + placed.size.h / 2.0 - inset - height,
                                    placed.size.w - 2.0 * inset,
                                    height,
                                )
                            }),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Some(Scene {
            axis: None,
            activations: Vec::new(),
            canvas: layout.bbox,
            tokens: scene_tokens(),
            direction: graph.map(|graph| graph.direction),
            layout_hint: graph.map(|graph| graph.layout_hint),
            palette: ScenePalette::css_vars(palette.theme),
            nodes,
            edges,
            bands,
            events,
        })
    }
}

/// The frozen glyph numbers the stub scene carries: the tokens of
/// `gx-core/src/tokens.rs` plus the constants of the emitter, mirrored here
/// because `gx-mcp` does not depend on it (PRD §9.1).
fn scene_tokens() -> SceneTokens {
    SceneTokens {
        text_size: TOKENS.text_size,
        node_padding: TOKENS.node_padding,
        corner_radius: TOKENS.corner_radius,
        border_width: TOKENS.border_width,
        line_height: TOKENS.line_height,
        regular_weight: TOKENS.regular_weight,
        semibold_weight: TOKENS.semibold_weight,
        arrow_length: 10.0,
        crow_foot: 22.0,
        crow_foot_spread: 0.85,
        badge_width: 26.0,
        muted_opacity: 0.55,
        lifeline_dash: [4.0, 4.0],
        dash_pattern: [6.0, 4.0],
        group_dash: [8.0, 5.0],
        circle_diameter: 24.0,
        activation_width: 10.0,
    }
}

/// The bytes the fake rasterizer returns for any SVG.
///
/// Deterministic on purpose: the PNG is outside the deterministic pipeline
/// (PRD §8.5.1, S10), so no test compares the bytes of a real `resvg` run.
pub(crate) const FAKE_PNG: &[u8] = b"\x89PNG\r\n\x1a\nrasterizador-falso";

/// The size the fake rasterizer reports for any SVG.
pub(crate) const FAKE_PNG_WIDTH: u32 = 320;
pub(crate) const FAKE_PNG_HEIGHT: u32 = 180;

/// The options the fake rasterizer saw in its last call.
static LAST_RASTER_OPTIONS: std::sync::Mutex<Option<RasterOptions>> = std::sync::Mutex::new(None);

/// A rasterizer stub: deterministic bytes and the size the test expects.
///
/// `gx-mcp` cannot depend on `gx-render-svg` (PRD §9.1), so the tests of the
/// export install this through the same seam `gx-app` uses.
struct FakeRasterizer;

impl Rasterizer for FakeRasterizer {
    fn rasterize(&self, _svg: &str, options: &RasterOptions) -> Result<RasterImage, RasterError> {
        *LAST_RASTER_OPTIONS
            .lock()
            .expect("the raster options slot is not poisoned") = Some(*options);
        Ok(RasterImage {
            png: FAKE_PNG.to_vec(),
            width: FAKE_PNG_WIDTH,
            height: FAKE_PNG_HEIGHT,
        })
    }
}

/// Installs the fake rasterizer in the process-wide seam (PRD §7 RF10-bis).
///
/// The tests share one process, so the seam only goes from empty to installed:
/// the "no rasterizer installed" branch cannot be tested here.
pub(crate) fn install_fake_rasterizer() {
    gx_core::export::install_rasterizer(Arc::new(FakeRasterizer));
}

/// The options the fake rasterizer saw in its last call.
pub(crate) fn last_raster_options() -> Option<RasterOptions> {
    *LAST_RASTER_OPTIONS
        .lock()
        .expect("the raster options slot is not poisoned")
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
    set_layout_delay(0);
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
