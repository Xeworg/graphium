//! End to end test of the F0 path: JSON → IR → layout → SVG → response.
//!
//! This is the exit criterion of the spike (PRD §12): the same door the MCP
//! server uses, without Tauri, webkit or a webview (RNF6).

use std::sync::Arc;

use gx_core::engine::render_json;
use gx_core::layout::LayoutEngine;
use gx_core::render::RenderOpts;
use gx_core::render::SvgRenderer;
use gx_core::text::TextMetrics;

const MINIMAL: &str = r#"{
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

fn install_engines() {
    let metrics: Arc<dyn TextMetrics> =
        Arc::new(gx_render_svg::InterMetrics::new().expect("fonts"));
    // El despachador, no un motor suelto: el golden tiene que ser el dibujo
    // que el producto produce, y el producto elige el motor por `layout_hint`.
    let layout: Arc<dyn LayoutEngine> = Arc::new(gx_layout::Layouts::new(metrics.clone()));
    let renderer: Arc<dyn SvgRenderer> = Arc::new(gx_render_svg::SvgRenderer::new(metrics.clone()));
    gx_core::engine::install(gx_core::engine::Engines {
        layout,
        metrics,
        renderer,
    });
}

#[test]
fn the_minimal_example_renders_to_an_svg() {
    install_engines();
    gx_kinds::register_all();

    let outcome = render_json(MINIMAL, &RenderOpts::default()).expect("renders");
    assert!(outcome.svg.starts_with("<svg"), "{}", outcome.svg);
    assert!(outcome.svg.contains("read file"));
    assert!(outcome.svg.contains("utf-8"));
    assert_eq!(outcome.metrics.nodes, 2);
    assert_eq!(outcome.metrics.edges, 1);
    assert!(outcome.metrics.svg_bytes > 0);
    assert!(outcome.bbox.w > 0.0 && outcome.bbox.h > 0.0);
}

#[test]
fn the_same_document_renders_the_same_bytes() {
    install_engines();
    gx_kinds::register_all();

    let first = render_json(MINIMAL, &RenderOpts::default()).expect("renders");
    let second = render_json(MINIMAL, &RenderOpts::default()).expect("renders");
    assert_eq!(first.svg, second.svg);
}

#[test]
fn an_unknown_kind_is_rejected_with_its_code() {
    install_engines();
    gx_kinds::register_all();

    let errors = render_json(r#"{"version":1,"kind":"nope"}"#, &RenderOpts::default())
        .expect_err("must fail");
    assert_eq!(errors[0].code, gx_core::error::ErrorCode::UnknownKind);
}

#[test]
fn the_plan_places_the_diagram_on_the_center_of_the_view() {
    let camera = gx_app::camera::CameraSnapshot::new(0.0, 0.0, 1.0);
    let viewport = gx_core::geom::Size::new(1280.0, 800.0);
    let rect = gx_app::camera::place(gx_core::geom::Size::new(200.0, 100.0), &camera, viewport);
    assert_eq!(rect.center().x, 640.0);
    assert_eq!(rect.center().y, 400.0);
}
