//! The scene of a render (`f11-elementos.md`, T1, S3-S5).
//!
//! The SVG string stops being the runtime path: the window mounts a structured
//! scene instead. This suite is the witness of the contract of task A here, on
//! the product engines — no stub renderer, no Tauri:
//!
//! - the scene mirrors the drawing (canvas, ids, boxes, orders, bands, events),
//! - the size of a node is the one the layout measured (S4), never a number the
//!   JS would have to compute,
//! - the payload of the window carries it and the export path carries none
//!   (S2; after the flip of T3/C the string is not a runtime payload any
//!   more),
//! - the payload weighs what the feature doc says it weighs: the numbers of
//!   `L4` come out of [`the_scene_of_three_hundred_nodes_weighs_less_than_the_svg`].

use std::sync::{Arc, Mutex};

use gx_core::engine::render_json;
use gx_core::layout::LayoutEngine;
use gx_core::render::{RenderOpts, SvgRenderer};
use gx_core::text::TextMetrics;
use gx_mcp::tools::{
    render_diagram, set_render_hook, RenderHook, RenderHookPayload, RenderRequest, RenderStatus,
};

/// The `component` document of the export golden: nested containers with
/// members, the case the layout used to drop (`IrGroup.members`).
const NESTED: &str = r#"{
  "version": 1,
  "kind": "component",
  "title": "Pedidos — fronteras",
  "nodes": [
    { "id": "web", "label": "Web", "role": "container" },
    { "id": "api", "label": "API", "role": "component", "emphasis": "highlight" },
    { "id": "db", "label": "Postgres", "role": "database" },
    { "id": "pagos", "label": "Pagos", "role": "external", "emphasis": "muted" },
    { "id": "jobs", "label": "worker de envíos" }
  ],
  "groups": [
    { "id": "plataforma", "label": "Plataforma", "members": ["web", "servicio"] },
    { "id": "servicio", "label": "Servicio", "members": ["api", "db", "jobs"] }
  ],
  "edges": [
    { "from": "web", "to": "api", "label": "HTTPS" },
    { "from": "api", "to": "db", "label": "SQL", "emphasis": "highlight" },
    { "from": "api", "to": "pagos", "label": "cobro" },
    { "from": "api", "to": "jobs", "label": "encola", "emphasis": "muted" }
  ]
}"#;

/// A timeline with two lanes and a span: the kind whose boxes the lanes engine
/// measures (`gx-layout`, T1) and whose bands carry a gutter.
const TIMELINE: &str = r#"{
  "version": 1,
  "kind": "timeline",
  "title": "roadmap",
  "lanes": [
    { "id": "now", "label": "ahora" },
    { "id": "next", "label": "después" }
  ],
  "events": [
    { "id": "e1", "lane": "now", "label": "motor", "order": 0 },
    { "id": "e2", "lane": "now", "label": "lienzo", "order": 1 },
    { "id": "e3", "lane": "next", "label": "elementos", "order": 2 }
  ]
}"#;

fn install_engines() {
    let metrics: Arc<dyn TextMetrics> =
        Arc::new(gx_render_svg::InterMetrics::new().expect("fonts"));
    // El despachador, no un motor suelto: el escenario tiene que ser el dibujo
    // que el producto produce.
    let layout: Arc<dyn LayoutEngine> = Arc::new(gx_layout::Layouts::new(metrics.clone()));
    let renderer: Arc<dyn SvgRenderer> = Arc::new(gx_render_svg::SvgRenderer::new(metrics.clone()));
    gx_core::engine::install(gx_core::engine::Engines {
        layout,
        metrics,
        renderer,
    });
    gx_kinds::register_all();
}

/// A chain of `count` nodes, the fixture of RNF10 and of the size report.
fn chain(count: usize) -> String {
    let ids: Vec<String> = (0..count)
        .map(|i| format!(r#"{{ "id": "n{i}", "label": "nodo {i} del pipeline" }}"#))
        .collect();
    let edges: Vec<String> = (0..count.saturating_sub(1))
        .map(|i| format!(r#"{{ "from": "n{i}", "to": "n{}" }}"#, i + 1))
        .collect();
    format!(
        r#"{{ "version": 1, "kind": "graph", "title": "grande", "direction": "LR",
             "nodes": [{}], "edges": [{}] }}"#,
        ids.join(", "),
        edges.join(", ")
    )
}

fn render(document: &str) -> gx_core::RenderOutcome {
    install_engines();
    render_json(document, &RenderOpts::default())
        .unwrap_or_else(|errors| panic!("the document renders: {errors:?}"))
}

/// S3: the scene mirrors the drawing of a graph. The elements are the ones the
/// SVG string writes —same ids— with the box the layout measured.
#[test]
fn the_scene_mirrors_the_drawing_of_a_graph() {
    let outcome = render(NESTED);
    let scene = outcome
        .elements
        .as_ref()
        .expect("the paint path builds a scene");

    // The canvas is the layout bbox with the air of the frame (`f21-roto.md`,
    // S3): padded by 12 plan units on every side, so strokes and edge labels
    // are never cut by the viewBox.
    let bbox = outcome.bbox;
    assert_eq!(
        scene.canvas,
        gx_core::Rect::new(bbox.x - 12.0, bbox.y - 12.0, bbox.w + 24.0, bbox.h + 24.0),
        "the canvas is the padded layout bbox"
    );
    assert_eq!(scene.nodes.len(), outcome.metrics.nodes);
    assert_eq!(scene.edges.len(), outcome.metrics.edges);

    // Every node of the scene is the element of the string: the ids are the
    // same ones, so task C can compare element by element (S3).
    for node in &scene.nodes {
        assert!(
            outcome.svg.contains(&format!("id=\"{}\"", node.id)),
            "the node {} is in the drawing",
            node.id
        );
        assert!(node.id.starts_with("g-"), "the id of the drawing");
        assert!(node.size.w > 0.0 && node.size.h > 0.0, "a measured box");
    }
    for edge in &scene.edges {
        assert!(
            outcome.svg.contains(&format!("id=\"{}\"", edge.id)),
            "the edge {} is in the drawing",
            edge.id
        );
        assert_eq!(edge.id, format!("m-{}-{}", edge.from, edge.to));
        assert!(!edge.points.is_empty(), "a drawn edge has a route");
    }

    // The nested containers travel with their members: the box and what it
    // owns, which the layout used to drop after placing the box.
    assert_eq!(scene.bands.len(), 2, "the two containers of the document");
    let outer = scene
        .bands
        .iter()
        .find(|band| band.id == "cluster-plataforma")
        .expect("the outer container");
    assert_eq!(outer.members, vec!["web", "servicio"]);
    assert_eq!(outer.depth, 0);
    let inner = scene
        .bands
        .iter()
        .find(|band| band.id == "cluster-servicio")
        .expect("the inner container");
    assert_eq!(inner.members, vec!["api", "db", "jobs"]);
    assert_eq!(inner.depth, 1, "an inner box is one level deeper");
    assert_eq!(outer.gutter, None, "a container has no gutter");
}

/// S4: the JS never measures. A label arrives broken with `\n` and the box is
/// the one the layout measured, so the painter only places what it is given.
#[test]
fn the_scene_carries_the_broken_label_and_the_measured_box() {
    let outcome = render(NESTED);
    let scene = outcome.elements.as_ref().expect("a scene");
    let node = scene
        .nodes
        .iter()
        .find(|node| node.id == "g-web")
        .expect("the Web node");

    assert_eq!(node.label, "Web", "the label travels as the IR has it");
    let placed = outcome
        .layout
        .nodes
        .get("web")
        .expect("the layout placed it");
    assert_eq!(node.center, placed.center, "the center of the measured box");
    assert_eq!(
        node.size, placed.size,
        "the measured box, not a recomputation"
    );
}

/// S3, S4: a timeline has no direction and no edges; its lanes are the bands
/// with the gutter the layout reserved, its events carry the measured box.
#[test]
fn the_scene_of_a_timeline_carries_its_lanes_and_events() {
    let outcome = render(TIMELINE);
    let scene = outcome.elements.as_ref().expect("a scene");

    assert_eq!(scene.direction, None, "a timeline has no direction");
    assert!(scene.edges.is_empty());
    assert!(scene.nodes.is_empty());

    // The bands come in id order, the order the SVG emitter writes them in
    // (PRD §8.5.1): `lane-next` before `lane-now`.
    let lanes: Vec<&str> = scene.bands.iter().map(|band| band.id.as_str()).collect();
    assert_eq!(lanes, vec!["lane-next", "lane-now"]);
    for band in &scene.bands {
        assert!(
            band.gutter.map(|gutter| gutter > 0.0).unwrap_or(false),
            "the lanes layout reserves a gutter for the names: {band:?}"
        );
    }
    assert_eq!(scene.bands[0].members, vec!["e3"]);
    assert_eq!(scene.bands[1].members, vec!["e1", "e2"]);

    let events: Vec<&str> = scene.events.iter().map(|event| event.id.as_str()).collect();
    assert_eq!(events, vec!["g-e1", "g-e2", "g-e3"]);
    assert_eq!(scene.events.len(), outcome.metrics.nodes);
    for event in &scene.events {
        let placed = outcome
            .layout
            .nodes
            .get(event.id.trim_start_matches("g-"))
            .expect("the lanes engine placed it");
        assert_eq!(event.center, placed.center);
        assert_eq!(event.size, placed.size, "the lanes engine measured it");
    }
}

/// S2: the export is an output artifact, not part of the paint path. The
/// exported bytes never build a scene.
#[test]
fn the_export_does_not_build_a_scene() {
    install_engines();
    let exported = render_json(
        NESTED,
        &RenderOpts {
            export: true,
            ..RenderOpts::default()
        },
    )
    .expect("the document renders for export");

    assert!(exported.svg.contains("<rect class=\"bg\""), "closed canvas");
    assert_eq!(
        exported.elements, None,
        "the export travels as bytes; the window is what mounts a scene"
    );
}

/// T3/C: the payload of the window carries the scene, and the SVG string of
/// the export is not part of the runtime wire any more (S1).
#[test]
fn the_render_payload_carries_the_scene() {
    install_engines();
    let painted: Arc<Mutex<Vec<gx_core::Scene>>> = Arc::new(Mutex::new(Vec::new()));
    let recorder = Arc::clone(&painted);
    let hook: RenderHook = Arc::new(move |payload: &RenderHookPayload| {
        recorder
            .lock()
            .expect("the recorder is not poisoned")
            .push(payload.elements.clone());
        RenderStatus::Painted
    });
    set_render_hook(Some(hook));

    let response = render_diagram(&RenderRequest {
        document: NESTED.to_string(),
        session_id: format!("scene-payload-{}", std::process::id()),
    });

    set_render_hook(None);
    assert!(response.ok, "the render works: {:?}", response.errors);
    assert!(response.rendered, "the hook confirmed the paint");

    let scenes = painted
        .lock()
        .expect("the recorder is not poisoned")
        .clone();
    let scene = scenes.first().expect("the hook ran").clone();
    assert_eq!(scene.nodes.len(), 5);
    assert_eq!(scene.edges.len(), 4);
    assert_eq!(scene.bands.len(), 2);
    assert_eq!(
        scene.palette.theme,
        gx_core::Theme::Dark,
        "the palette travels as the theme's CSS variables"
    );
}

/// The numbers of `L4` of `f11-elementos.md`: how much the scene weighs next to
/// the SVG string it replaces, on the 300-node fixture of RNF10.
///
/// The measurement is a report, and the assertions are the promise: the scene
/// mirrors the whole drawing (every node, every edge) and it is not heavier
/// than the string it replaces. The numbers this run prints are the ones `L4`
/// of the feature doc records.
#[test]
fn the_scene_of_three_hundred_nodes_weighs_less_than_the_svg() {
    let outcome = render(&chain(300));
    let scene = outcome.elements.as_ref().expect("a scene");

    assert_eq!(scene.nodes.len(), 300);
    assert_eq!(scene.edges.len(), 299);
    let json = serde_json::to_string(scene).expect("the scene serialises");
    println!(
        "f11-elementos L4: {} nodos, SVG {} bytes, elements {} bytes",
        scene.nodes.len(),
        outcome.svg.len(),
        json.len()
    );
    assert!(
        json.len() < outcome.svg.len(),
        "the scene ({} bytes) is lighter than the SVG string ({} bytes)",
        json.len(),
        outcome.svg.len()
    );
}

/// The scene is deterministic: the same document produces the same bytes twice,
/// the promise O8 makes of the drawing (PRD §8.5).
#[test]
fn the_scene_of_a_document_is_byte_identical_twice() {
    let first = render(NESTED);
    let second = render(NESTED);

    let json = |outcome: &gx_core::RenderOutcome| {
        serde_json::to_string(outcome.elements.as_ref().expect("a scene"))
            .expect("the scene serialises")
    };
    assert_eq!(json(&first), json(&second));
}
