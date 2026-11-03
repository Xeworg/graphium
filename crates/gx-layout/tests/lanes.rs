//! The `lanes` layout engine (PRD §6: `sequence`, `timeline`, `swimlane`).
//!
//! Un timeline es una tabla: los carriles son filas y el orden de los eventos
//! es el eje horizontal. Nada de eso lo resuelve `dagre`, y el orden tiene que
//! leerse de izquierda a derecha sin adivinar.

use gx_core::geom::Size;
use gx_core::ir::{IrEvent, IrLane, IrTimeline};
use gx_core::layout::{LayoutEngine, LayoutOpts};
use gx_layout::LanesEngine;

/// Un medidor con ancho fijo por carácter: el layout no depende de la fuente.
struct FakeMetrics;

impl gx_core::text::TextMetrics for FakeMetrics {
    fn measure(&self, text: &str, style: &gx_core::text::TextStyle) -> Size {
        let lines = text.lines().count().max(1) as f64;
        Size::new(
            text.chars().count() as f64 * style.size * 0.5,
            lines * style.size * style.line_height,
        )
    }
}

fn engine() -> LanesEngine {
    LanesEngine::new(std::sync::Arc::new(FakeMetrics))
}

fn lane(id: &str, label: &str) -> IrLane {
    IrLane {
        id: id.to_string(),
        label: Some(label.to_string()),
    }
}

fn event(id: &str, lane: &str, label: &str, order: i64) -> IrEvent {
    IrEvent {
        id: id.to_string(),
        lane: lane.to_string(),
        label: label.to_string(),
        order,
        span: None,
        kind_data: Default::default(),
    }
}

fn timeline(lanes: Vec<IrLane>, events: Vec<IrEvent>) -> IrTimeline {
    IrTimeline { lanes, events }
}

#[test]
fn each_lane_is_a_row_in_the_order_of_the_document() {
    let timeline = timeline(
        vec![lane("cliente", "Cliente"), lane("api", "API")],
        vec![
            event("e1", "cliente", "pide", 0),
            event("e2", "api", "responde", 1),
        ],
    );

    let layout = engine().layout_timeline(&timeline, &LayoutOpts::default());

    assert_eq!(layout.nodes.len(), 2);
    let cliente = layout.nodes.get("e1").expect("e1");
    let api = layout.nodes.get("e2").expect("e2");
    assert!(
        api.center.y > cliente.center.y,
        "el segundo carril va debajo del primero"
    );
    assert_eq!(layout.groups.len(), 2, "un grupo por carril");
    assert_eq!(layout.groups[0].id, "cliente");
    assert_eq!(layout.groups[0].label.as_deref(), Some("Cliente"));
    assert!(layout.groups[0].bbox.max_y() <= layout.groups[1].bbox.y);
}

#[test]
fn the_events_advance_along_the_time_axis() {
    let timeline = timeline(
        vec![lane("api", "API")],
        vec![
            event("e1", "api", "recibe", 0),
            event("e2", "api", "valida", 1),
            event("e3", "api", "responde", 2),
        ],
    );

    let layout = engine().layout_timeline(&timeline, &LayoutOpts::default());

    let mut previous = f64::NEG_INFINITY;
    for id in ["e1", "e2", "e3"] {
        let placed = layout.nodes.get(id).expect(id);
        assert!(
            placed.center.x > previous,
            "{id} tiene que avanzar en el tiempo: {}",
            placed.center.x
        );
        assert_eq!(placed.center.y, layout.nodes["e1"].center.y, "mismo carril");
        previous = placed.center.x;
    }
}

#[test]
fn the_order_field_wins_over_the_order_of_the_document() {
    // El documento puede llegar desordenado: manda `order`, que es el dato del
    // timeline, no la posición en el array.
    let timeline = timeline(
        vec![lane("api", "API")],
        vec![
            event("late", "api", "tercero", 3),
            event("early", "api", "primero", 1),
        ],
    );

    let layout = engine().layout_timeline(&timeline, &LayoutOpts::default());

    assert!(
        layout.nodes["early"].center.x < layout.nodes["late"].center.x,
        "el orden declarado manda: {:?} vs {:?}",
        layout.nodes["early"].center,
        layout.nodes["late"].center
    );
}

#[test]
fn a_span_makes_an_event_wider() {
    let mut wide = event("wide", "api", "espera", 1);
    wide.span = Some(gx_core::ir::Span {
        start: 1.0,
        end: 4.0,
    });
    let timeline = timeline(
        vec![lane("api", "API")],
        vec![event("point", "api", "pide", 0), wide],
    );

    let layout = engine().layout_timeline(&timeline, &LayoutOpts::default());

    let point = layout.nodes["point"].size.w;
    let wide = layout.nodes["wide"].size.w;
    assert!(
        wide > point * 2.0,
        "un span de tres columnas tiene que ser bastante más ancho: {wide} vs {point}"
    );
}

#[test]
fn the_same_timeline_is_laid_out_the_same_way() {
    let timeline = timeline(
        vec![lane("a", "A"), lane("b", "B")],
        vec![
            event("e1", "a", "uno", 0),
            event("e2", "b", "dos", 1),
            event("e3", "a", "tres", 2),
        ],
    );

    let first = engine().layout_timeline(&timeline, &LayoutOpts::default());
    let second = engine().layout_timeline(&timeline, &LayoutOpts::default());

    assert_eq!(first.nodes, second.nodes);
    assert_eq!(first.groups, second.groups);
    assert_eq!(first.bbox, second.bbox);
}

#[test]
fn an_event_of_an_unknown_lane_is_still_placed() {
    // Un evento sin carril no puede desaparecer sin decir nada: se dibuja
    // igual, en la fila que le toca cuando el carril no existe.
    let timeline = timeline(
        vec![lane("api", "API")],
        vec![event("huerfano", "no-existe", "suelto", 0)],
    );

    let layout = engine().layout_timeline(&timeline, &LayoutOpts::default());

    assert!(layout.nodes.contains_key("huerfano"));
    let placed = layout.nodes["huerfano"];
    assert!(placed.size.w > 0.0 && placed.size.h > 0.0);
    assert_eq!(placed.size.w, Size::new(placed.size.w, placed.size.h).w);
}

#[test]
fn the_drawing_starts_at_the_origin() {
    let timeline = timeline(
        vec![lane("api", "API")],
        vec![event("e1", "api", "primero", 0)],
    );

    let layout = engine().layout_timeline(&timeline, &LayoutOpts::default());

    assert!(
        layout.bbox.x >= 0.0 && layout.bbox.y >= 0.0,
        "{:?}",
        layout.bbox
    );
}
