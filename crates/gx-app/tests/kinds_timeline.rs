//! Los kinds `timeline` y `swimlane` por la puerta única (PRD §6, S1, S8).
//!
//! Los dos van sobre la IR de timeline y el motor `lanes`, así que el test los
//! mira por donde el producto los mira: `render_json`, sin Tauri ni webview
//! (RNF6). Lo que se afirma es lo que hace legible el dibujo: cada carril es
//! una banda con su nombre, cada evento cae en su carril y la columna la
//! decide el tiempo, no el orden del documento.

use std::sync::Arc;

use gx_core::engine::render_json;
use gx_core::layout::LayoutEngine;
use gx_core::render::{RenderOpts, SvgRenderer};
use gx_core::text::TextMetrics;

const ROADMAP: &str = r#"{
  "version": 1,
  "kind": "timeline",
  "title": "Roadmap 2026",
  "lanes": [
    { "id": "q1", "label": "Q1" },
    { "id": "q2", "label": "Q2" }
  ],
  "events": [
    { "label": "spike F0", "lane": "q1" },
    { "label": "MVP", "lane": "q1", "end": 2 },
    { "label": "los 10 kinds", "lane": "q2" }
  ]
}"#;

/// El orden de los eventos es al revés del tiempo: la columna la decide
/// `order`, no la posición en el array.
const UNSORTED: &str = r#"{
  "version": 1,
  "kind": "timeline",
  "title": "desordenado",
  "lanes": [ { "id": "unico", "label": "Todo" } ],
  "events": [
    { "id": "tarde", "label": "tarde", "lane": "unico", "order": 5 },
    { "id": "temprano", "label": "temprano", "lane": "unico", "order": 0 }
  ]
}"#;

const SOPORTE: &str = r#"{
  "version": 1,
  "kind": "swimlane",
  "title": "Soporte — pedido trabado",
  "lanes": [
    { "id": "cliente", "label": "Cliente" },
    { "id": "soporte", "label": "Soporte" },
    { "id": "logistica", "label": "Logística" }
  ],
  "steps": [
    { "label": "reclama el pedido", "lane": "cliente" },
    { "label": "abre el ticket", "lane": "soporte" },
    { "label": "consulta el envío", "lane": "logistica" },
    { "label": "responde", "lane": "soporte" }
  ]
}"#;

fn install_engines() {
    let metrics: Arc<dyn TextMetrics> =
        Arc::new(gx_render_svg::InterMetrics::new().expect("fonts"));
    let layout: Arc<dyn LayoutEngine> = Arc::new(gx_layout::Layouts::new(metrics.clone()));
    let renderer: Arc<dyn SvgRenderer> = Arc::new(gx_render_svg::SvgRenderer::new(metrics.clone()));
    gx_core::engine::install(gx_core::engine::Engines {
        layout,
        metrics,
        renderer,
    });
    gx_kinds::register_all();
}

fn render(document: &str) -> gx_core::render::RenderOutcome {
    install_engines();
    render_json(document, &RenderOpts::default()).unwrap_or_else(|errors| panic!("{errors:?}"))
}

/// S1 (`timeline`): los carriles son bandas con su nombre y los eventos son
/// las cajas del dibujo.
#[test]
fn a_timeline_draws_its_lanes_and_its_events() {
    let outcome = render(ROADMAP);

    assert_eq!(outcome.metrics.nodes, 3, "un evento es un nodo del dibujo");
    assert_eq!(outcome.metrics.edges, 0, "una timeline no tiene aristas");
    assert_eq!(
        outcome.svg.matches("class=\"lane-band\"").count(),
        2,
        "una banda por carril declarado"
    );
    assert!(outcome.svg.contains(">Q1<"), "el carril se nombra");
    assert!(outcome.svg.contains(">Q2<"), "el carril se nombra");
    for label in ["spike F0", "MVP", "los 10 kinds"] {
        assert!(outcome.svg.contains(label), "falta el evento `{label}`");
    }
}

/// S1: el eje horizontal es el tiempo. El `order` decide la columna, así que
/// un documento desordenado se dibuja ordenado.
#[test]
fn the_time_axis_follows_the_order_not_the_document() {
    let outcome = render(UNSORTED);

    let x_of = |id: &str| {
        outcome
            .layout
            .nodes
            .get(id)
            .unwrap_or_else(|| panic!("falta el evento `{id}`"))
            .center
            .x
    };
    assert!(
        x_of("temprano") < x_of("tarde"),
        "el evento con `order` 0 va a la izquierda del de `order` 5"
    );
}

/// S1 (`swimlane`): cada paso cae en la banda de su carril.
#[test]
fn a_swimlane_puts_every_step_in_its_lane() {
    let outcome = render(SOPORTE);

    assert_eq!(outcome.metrics.nodes, 4);
    assert_eq!(outcome.svg.matches("class=\"lane-band\"").count(), 3);
    for label in ["Cliente", "Soporte", "Logística"] {
        assert!(outcome.svg.contains(label), "falta el carril `{label}`");
    }
    for label in [
        "reclama el pedido",
        "abre el ticket",
        "consulta el envío",
        "responde",
    ] {
        assert!(outcome.svg.contains(label), "falta el paso `{label}`");
    }

    // El paso de logística está en la banda del medio: abajo del de cliente y
    // arriba del de soporte, que es el tercero.
    let y_of = |id: &str| outcome.layout.nodes.get(id).expect("el paso").center.y;
    let etapa = |id: &str| {
        outcome
            .layout
            .groups
            .iter()
            .find(|group| group.id == id)
            .unwrap_or_else(|| panic!("falta el carril `{id}`"))
            .bbox
    };
    let dentro = |y: f64, lane: &str| {
        let band = etapa(lane);
        y > band.y && y < band.y + band.h
    };
    assert!(dentro(y_of("step-0"), "cliente"));
    assert!(dentro(y_of("step-1"), "soporte"));
    assert!(dentro(y_of("step-2"), "logistica"));
    assert!(dentro(y_of("step-3"), "soporte"));
}

/// O8: el mismo documento produce los mismos bytes.
#[test]
fn the_timeline_renders_the_same_bytes_every_time() {
    install_engines();
    let first = render_json(ROADMAP, &RenderOpts::default()).expect("renders");
    let second = render_json(ROADMAP, &RenderOpts::default()).expect("renders");

    assert_eq!(first.svg, second.svg);
}

/// S3: una referencia a un carril que no existe se reporta con su path.
#[test]
fn an_event_in_an_undeclared_lane_is_reported() {
    install_engines();
    let errors = render_json(
        r#"{ "version": 1, "kind": "timeline",
             "lanes": [ { "id": "q1" } ],
             "events": [ { "label": "hola", "lane": "q9" } ] }"#,
        &RenderOpts::default(),
    )
    .expect_err("el carril no existe");

    assert_eq!(errors[0].code, gx_core::error::ErrorCode::UnknownRef);
    assert_eq!(errors[0].path, "events[0].lane");
    assert!(errors[0].hint.is_some());
}

/// S3: un carril de proceso sin pasos no dice nada, y un paso sin carril
/// tampoco: los dos errores nombran el campo que falta.
#[test]
fn a_swimlane_without_lanes_or_a_step_without_its_lane_is_reported() {
    install_engines();

    let sin_carriles = render_json(
        r#"{ "version": 1, "kind": "swimlane", "lanes": [],
             "steps": [ { "label": "hola", "lane": "soporte" } ] }"#,
        &RenderOpts::default(),
    )
    .expect_err("un proceso sin carriles no es un swimlane");
    assert_eq!(
        sin_carriles[0].code,
        gx_core::error::ErrorCode::MissingField
    );
    assert_eq!(sin_carriles[0].path, "lanes");

    let sin_carril = render_json(
        r#"{ "version": 1, "kind": "swimlane",
             "lanes": [ { "id": "soporte" } ],
             "steps": [ { "label": "hola", "lane": "nadie" } ] }"#,
        &RenderOpts::default(),
    )
    .expect_err("el paso nombra un carril que no existe");
    assert_eq!(sin_carril[0].code, gx_core::error::ErrorCode::UnknownRef);
    assert_eq!(sin_carril[0].path, "steps[0].lane");
}
