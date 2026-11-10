//! The `lanes` layout engine (PRD §6: `sequence`, `timeline`, `swimlane`).
//!
//! Un timeline es una tabla: los carriles son filas y el orden de los eventos
//! es el eje horizontal. Nada de eso lo resuelve `dagre`, y el orden tiene que
//! leerse de izquierda a derecha sin adivinar.

use gx_core::geom::Size;
use gx_core::ir::{Direction, IrEvent, IrGraph, IrLane, IrNode, IrTimeline, LayoutHint, Shape};
use gx_core::layout::{LayoutEngine, LayoutOpts};
use gx_layout::LanesEngine;

fn graph_node(id: &str, w: f64, h: f64) -> IrNode {
    let mut node = IrNode::new(id, id);
    node.size = Size::new(w, h);
    node
}

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
        shape: Shape::Rounded,
        id: id.to_string(),
        lane: lane.to_string(),
        label: label.to_string(),
        order,
        span: None,
        kind_data: Default::default(),
        depends_on: Vec::new(),
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

/// El mensaje que un actor se manda a sí mismo sale y vuelve a **su línea de
/// vida**, no al borde de su caja: si arrancara en el borde, el lazo quedaría
/// flotando al costado de la vertical y no se leería.
#[test]
fn a_self_message_leaves_and_returns_to_its_lifeline() {
    let timeline = timeline(
        vec![lane("api", "API")],
        vec![
            event("pide", "api", "pide", 0),
            event("reintento", "api", "reintento", 1),
        ],
    );

    // Una timeline no rutea aristas: el lazo es cosa del grafo de secuencia.
    let layout = engine().layout_timeline(&timeline, &LayoutOpts::default());
    assert!(
        layout.edges.is_empty(),
        "una timeline no dibuja el lazo: {:?}",
        layout.edges
    );

    let graph = sequence_graph();
    let layout = engine().layout(&graph, &LayoutOpts::default());
    let route = layout
        .edges
        .iter()
        .find(|edge| edge.from == "api" && edge.to == "api")
        .expect("el lazo del actor");

    assert_eq!(route.points.len(), 4, "el lazo tiene cuatro tramos");
    let lifeline = layout.nodes["api"].center.x;
    assert!(
        (route.points[0].x - lifeline).abs() < 1e-6,
        "el lazo sale de la línea de vida: {} vs {lifeline}",
        route.points[0].x
    );
    assert!(
        (route.points[3].x - lifeline).abs() < 1e-6,
        "y vuelve a ella: {}",
        route.points[3].x
    );
    assert!(route.points[1].x > lifeline, "sale hacia un costado");
    // El nombre va sobre el tramo de arriba del lazo, no sobre la vertical.
    let label = route.label_at.expect("el lazo lleva su nombre");
    assert!(
        label.x > lifeline,
        "el nombre no se apoya en la línea de vida"
    );
    assert!(label.y < route.points[0].y, "y va arriba del lazo");
}

/// Un grafo de secuencia con dos actores y un mensaje a sí mismo.
fn sequence_graph() -> gx_core::ir::IrGraph {
    use gx_core::ir::{IrEdge, IrGraph, IrNode, LayoutHint};
    let mut api = IrNode::new("api", "API");
    api.size = Size::new(90.0, 40.0);
    let mut cliente = IrNode::new("cliente", "Cliente");
    cliente.size = Size::new(90.0, 40.0);
    let mut loop_edge = IrEdge::new("api", "api");
    loop_edge.label = Some("reintento".to_string());
    IrGraph {
        direction: gx_core::ir::Direction::TB,
        layout_hint: LayoutHint::Lanes,
        nodes: vec![cliente, api],
        edges: vec![IrEdge::new("cliente", "api"), loop_edge],
        groups: Vec::new(),
    }
}

/// S4 (`f21-roto.md`): two tasks of one lane whose spans overlap in time go
/// to different sub-rows — no two bars of one lane share a region, and the
/// lane grows to hold them.
#[test]
fn overlapping_tasks_of_one_lane_pack_into_sub_rows() {
    let engine = LanesEngine::new(std::sync::Arc::new(FakeMetrics));
    let timeline = IrTimeline {
        lanes: vec![IrLane {
            id: "diseno".to_string(),
            label: Some("Diseño".to_string()),
        }],
        events: vec![
            IrEvent {
                shape: Shape::Rounded,
                id: "planos".to_string(),
                lane: "diseno".to_string(),
                label: "Planos".to_string(),
                order: 0,
                span: Some(gx_core::ir::Span {
                    start: 0.0,
                    end: 3.0,
                }),
                depends_on: Vec::new(),
                kind_data: Default::default(),
            },
            IrEvent {
                shape: Shape::Rounded,
                id: "permiso".to_string(),
                lane: "diseno".to_string(),
                label: "Permiso municipal".to_string(),
                order: 2,
                span: Some(gx_core::ir::Span {
                    start: 2.0,
                    end: 5.0,
                }),
                depends_on: Vec::new(),
                kind_data: Default::default(),
            },
        ],
    };
    let layout = engine.layout_timeline(&timeline, &LayoutOpts::default());

    let planos = layout.nodes.get("planos").expect("planos placed");
    let permiso = layout.nodes.get("permiso").expect("permiso placed");
    assert_ne!(planos.center.y, permiso.center.y, "sub-rows, not overlap");
    let no_vertical_meet =
        (planos.center.y - permiso.center.y).abs() >= (planos.size.h + permiso.size.h) / 2.0;
    assert!(no_vertical_meet, "the two bars share a vertical region");
    // The lane grew to hold both rows.
    let lane = &layout.groups[0];
    assert!(lane.bbox.h >= 2.0 * planos.size.h, "the lane did not grow");
}

/// S5 (`f21-roto.md`): sequential steps of a lane get the gap between boxes,
/// not the fixed column step minus the box width — the swimlane steps used to
/// touch (0.33 px).
#[test]
fn sequential_steps_keep_air_between_boxes() {
    let engine = LanesEngine::new(std::sync::Arc::new(FakeMetrics));
    let timeline = IrTimeline {
        lanes: vec![IrLane {
            id: "cliente".to_string(),
            label: Some("Cliente".to_string()),
        }],
        events: vec![
            IrEvent {
                shape: Shape::Rounded,
                id: "s0".to_string(),
                lane: "cliente".to_string(),
                label: "reclama el pedido trabado en la aduana".to_string(),
                order: 0,
                span: None,
                depends_on: Vec::new(),
                kind_data: Default::default(),
            },
            IrEvent {
                shape: Shape::Rounded,
                id: "s1".to_string(),
                lane: "cliente".to_string(),
                label: "vuelve a reclamar el pedido trabado en la aduana".to_string(),
                order: 1,
                span: None,
                depends_on: Vec::new(),
                kind_data: Default::default(),
            },
        ],
    };
    let layout = engine.layout_timeline(&timeline, &LayoutOpts::default());

    let first = layout.nodes.get("s0").expect("s0 placed");
    let second = layout.nodes.get("s1").expect("s1 placed");
    let gap = second.center.x - second.size.w / 2.0 - (first.center.x + first.size.w / 2.0);
    assert!(gap >= 2.0 * 16.0 - 0.01, "the steps touch: gap {gap}");
}

/// The time axis says what the columns mean (`f22-dibujo.md`, S2): one tick
/// per known column, at the column center, under the last lane — and no axis
/// where there is no time.
#[test]
fn the_time_axis_names_what_the_columns_mean() {
    let timeline = timeline(
        vec![lane("a", "A"), lane("b", "B")],
        vec![event("e1", "a", "uno", 0), event("e2", "b", "dos", 2)],
    );

    let layout = engine().layout_timeline(&timeline, &LayoutOpts::default());

    // Column 1 was never mentioned: the axis ticks only what exists.
    let axis = layout.axis.expect("the timeline draws its axis");
    let values: Vec<i64> = axis.ticks.iter().map(|(_, value)| *value).collect();
    assert_eq!(values, vec![0, 2], "one tick per known column, in order");

    // Each tick sits at the center of its own column: the column starts at
    // its left edge and is one step wide (a wide point event widens its own
    // column, so the tick center is left + step/2, not the event's center).
    let e1 = layout.nodes.get("e1").expect("e1");
    let e2 = layout.nodes.get("e2").expect("e2");
    let e1_left = e1.center.x - e1.size.w / 2.0;
    let e2_left = e2.center.x - e2.size.w / 2.0;
    // The tick is the time position of the column: left edge + half a step —
    // NOT the event box center, because a wide point event (e1's label is
    // 118 wide: max(label+2p, step−2p)) shifts its own box but not the time.
    assert_eq!(axis.ticks[0].0, e1_left + 75.0);
    assert_eq!(axis.ticks[1].0, e2_left + 75.0);

    // The line runs under the last lane band.
    let last_lane = layout.groups.last().expect("lanes");
    assert!(
        axis.y > last_lane.bbox.y + last_lane.bbox.h - 0.01,
        "the axis is below the last lane"
    );
    assert_eq!(axis.x1, axis.ticks.first().expect("first tick").0);
    assert_eq!(axis.x2, axis.ticks.last().expect("last tick").0);

    // The axis band lives inside the bounding box: nothing clips.
    assert!(
        layout.bbox.max_y() >= axis.y + 0.01,
        "the axis fits the bbox"
    );
}

/// A sequence has no time axis: its columns are actors, and the lifelines
/// already say which column is which (`f22-dibujo.md`, L5d).
#[test]
fn a_sequence_layout_carries_no_axis() {
    let graph = IrGraph {
        direction: Direction::LR,
        layout_hint: LayoutHint::Lanes,
        nodes: vec![graph_node("u", 90.0, 40.0), graph_node("s", 90.0, 40.0)],
        edges: vec![],
        groups: Vec::new(),
    };

    let layout = engine().layout(&graph, &LayoutOpts::default());

    assert!(layout.axis.is_none(), "the actors are not time");
}
