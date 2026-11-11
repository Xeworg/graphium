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
    IrTimeline {
        lanes,
        events,
        connections: Vec::new(),
    }
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
        connections: Vec::new(),
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
        connections: Vec::new(),
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

// S5 (`f22-dibujo.md`): the activation of a sequence — an opt-in flag on the
// message contract that draws a thin rectangle over the `to` actor's
// lifeline while the actor holds the control. The layout computes the bars
// (`f22-dibujo.md`, L7c): from the activating row to the next message the
// actor **sends**, so control is held until the answer leaves.
mod activation {
    use super::*;

    /// A sequence graph: three actors, four rows, built row by row so every
    /// test declares its own messages.
    struct Rows {
        graph: IrGraph,
    }

    impl Rows {
        fn new() -> Self {
            let nodes: Vec<IrNode> = ["cliente", "api", "db"]
                .iter()
                .map(|id| graph_node(id, 90.0, 40.0))
                .collect();
            Self {
                graph: IrGraph {
                    direction: Direction::TB,
                    layout_hint: LayoutHint::Lanes,
                    nodes,
                    edges: Vec::new(),
                    groups: Vec::new(),
                },
            }
        }

        fn row(mut self, from: &str, to: &str, activation: bool) -> Self {
            let mut edge = IrEdge::new(from, to);
            if activation {
                edge.kind_data
                    .insert("activation".to_string(), gx_core::ir::IrValue::Bool(true));
            }
            self.graph.edges.push(edge);
            self
        }

        fn layout(&self) -> gx_core::layout::LayoutGraph {
            engine().layout(&self.graph, &LayoutOpts::default())
        }
    }

    use gx_core::ir::{IrEdge, IrGraph};

    #[test]
    fn an_activation_holds_control_until_the_actor_answers() {
        let rows = Rows::new()
            .row("cliente", "api", false)
            .row("db", "api", true)
            .row("api", "cliente", false)
            .row("api", "db", false);
        let layout = rows.layout();

        // The activating message is row 2; the actor's next send is row 4:
        // the bar brackets rows 2..4.
        assert_eq!(layout.activations.len(), 1, "una barra por activación");
        let bar = &layout.activations[0];
        let step = layout.edges[1].points[0].y - layout.edges[0].points[0].y;
        let row_y = |row: usize| layout.edges[row - 1].points[0].y;
        assert!(
            (bar.x - layout.nodes["api"].center.x).abs() < 1e-6,
            "la barra vive en la línea de vida del actor: {}",
            bar.x
        );
        assert!(
            (bar.y1 - (row_y(2) - 0.35 * step)).abs() < 1e-6,
            "la barra arranca sobre su flecha: {} vs {}",
            bar.y1,
            row_y(2) - 0.35 * step
        );
        // El primer envío del actor es la fila 3 (la respuesta): ahí se
        // sostiene hasta, no en la fila 4.
        assert!(
            (bar.y2 - (row_y(3) + 0.35 * step)).abs() < 1e-6,
            "y se sostiene hasta que el actor contesta: {} vs {}",
            bar.y2,
            row_y(3) + 0.35 * step
        );
    }

    #[test]
    fn an_activation_without_an_answer_covers_its_own_row() {
        let rows = Rows::new()
            .row("cliente", "api", true)
            .row("cliente", "db", false);
        let layout = rows.layout();

        // The api actor never sends again: the bar covers its own row only.
        let bar = &layout.activations[0];
        let step = layout.edges[1].points[0].y - layout.edges[0].points[0].y;
        let row_y = |row: usize| layout.edges[row - 1].points[0].y;
        assert!(
            (bar.y2 - (row_y(1) + 0.35 * step)).abs() < 1e-6,
            "sin respuesta la barra no baja: {} vs {}",
            bar.y2,
            row_y(1) + 0.35 * step
        );
    }

    #[test]
    fn a_second_activation_inside_a_held_control_extends_the_bar() {
        let rows = Rows::new()
            .row("db", "api", true)
            .row("db", "api", true)
            .row("api", "cliente", false);
        let layout = rows.layout();

        // Two activating rows, one answer: one merged bar over both, never
        // two rectangles painted on the same lifeline.
        assert_eq!(
            layout.activations.len(),
            1,
            "las barras del mismo actor se funden: {:?}",
            layout.activations
        );
    }

    #[test]
    fn a_sequence_without_activations_carries_no_bar() {
        let rows = Rows::new().row("cliente", "api", false);
        let layout = rows.layout();

        assert!(
            layout.activations.is_empty(),
            "nadie pidió control: {:?}",
            layout.activations
        );
    }
}

// S8 (`f22-dibujo.md`): the connectors of a swimlane — the lanes engine
// routes the arrows between steps, across lanes. Forward flows exit the
// source's right edge and enter the target's left edge with one bend in the
// middle; backward flows mirror it so the line never crosses a box; a
// connector in the same column goes straight down.
mod connectors {
    use super::*;
    use gx_core::ir::IrConnection;

    fn connected(from: &str, to: &str) -> IrTimeline {
        let mut timeline = timeline(
            vec![lane("soporte", "Soporte"), lane("negocio", "Negocio")],
            vec![
                event("a", "soporte", "abre", 0),
                event("b", "negocio", "escala", 1),
            ],
        );
        timeline.connections = vec![IrConnection {
            from: from.to_string(),
            to: to.to_string(),
        }];
        timeline
    }

    #[test]
    fn a_connector_routes_forward_across_lanes() {
        let layout = engine().layout_timeline(&connected("a", "b"), &LayoutOpts::default());

        assert_eq!(layout.edges.len(), 1, "una conexión, una ruta");
        let route = &layout.edges[0];
        assert_eq!(route.from, "a");
        assert_eq!(route.to, "b");
        let a = &layout.nodes["a"];
        let b = &layout.nodes["b"];
        // The arrow leaves the source's right edge and lands on the target's
        // left edge, with the bend between the two boxes.
        assert!(
            (route.points[0].x - (a.center.x + a.size.w / 2.0)).abs() < 1e-6,
            "sale del borde derecho: {:?} vs {}",
            route.points[0],
            a.center.x + a.size.w / 2.0
        );
        assert!((route.points[0].y - a.center.y).abs() < 1e-6);
        assert!(
            (route.points.last().unwrap().x - b.center.x + b.size.w / 2.0).abs() < 1e-6
                || (route.points.last().unwrap().x - (b.center.x - b.size.w / 2.0)).abs() < 1e-6,
            "llega a un borde del destino"
        );
        // Four points: exit, the bend down, the entry.
        assert_eq!(route.points.len(), 4, "un solo quiebre: {:?}", route.points);
        assert!(
            route.points[1].x > a.center.x + a.size.w / 2.0
                && route.points[1].x < b.center.x - b.size.w / 2.0,
            "el quiebre vive entre las cajas: {:?}",
            route.points[1]
        );
    }

    #[test]
    fn a_connector_routes_backwards_without_crossing_a_box() {
        // `b` sits to the right of `a` in column order; a connector that goes
        // back leaves the source's LEFT edge and enters the target's right.
        let mut timeline = connected("a", "b");
        timeline.events[0].order = 1;
        timeline.events[1].order = 0;
        let layout = engine().layout_timeline(&timeline, &LayoutOpts::default());

        let route = &layout.edges[0];
        let a = &layout.nodes["a"];
        let b = &layout.nodes["b"];
        assert!(
            (route.points[0].x - (a.center.x - a.size.w / 2.0)).abs() < 1e-6,
            "sale del borde izquierdo: {:?}",
            route.points[0]
        );
        assert!(
            (route.points.last().unwrap().x - (b.center.x + b.size.w / 2.0)).abs() < 1e-6,
            "entra por el borde derecho: {:?}",
            route.points.last().unwrap()
        );
        // The bend lives between the two boxes, never over one.
        assert!(
            route.points[1].x < a.center.x - a.size.w / 2.0
                && route.points[1].x > b.center.x + b.size.w / 2.0,
            "el quiebre vive entre las cajas: {:?}",
            route.points[1]
        );
    }

    #[test]
    fn a_connector_in_the_same_column_goes_straight_down() {
        let mut timeline = connected("a", "b");
        timeline.events[1].order = 0;
        let layout = engine().layout_timeline(&timeline, &LayoutOpts::default());

        let route = &layout.edges[0];
        let a = &layout.nodes["a"];
        let b = &layout.nodes["b"];
        assert_eq!(route.points.len(), 2, "vertical: {:?}", route.points);
        assert!((route.points[0].x - a.center.x).abs() < 1e-6);
        assert!(
            (route.points[0].y - (a.center.y + a.size.h / 2.0)).abs() < 1e-6,
            "sale del borde inferior: {:?}",
            route.points[0]
        );
        assert!(
            (route.points[1].y - (b.center.y - b.size.h / 2.0)).abs() < 1e-6,
            "entra por el borde superior: {:?}",
            route.points[1]
        );
    }

    #[test]
    fn a_connector_with_an_unplaced_end_draws_nothing() {
        let mut timeline = connected("a", "fantasma");
        timeline.connections[0].to = "fantasma".to_string();
        let layout = engine().layout_timeline(&timeline, &LayoutOpts::default());

        assert!(
            layout.edges.is_empty(),
            "el layout es la autoridad: {:?}",
            layout.edges
        );
    }
}

// S9 (`f22-dibujo.md`): the label fits the drawn shape — a milestone's
// diamond is measured against the usable width at the label's extreme rows,
// so its own column widens until the label fits the slanted edges.
#[test]
fn a_milestones_diamond_is_wide_enough_for_its_label() {
    let mut milestone = event("corte", "hits", "código congelado", 0);
    milestone.shape = gx_core::ir::Shape::Diamond;
    let timeline = timeline(vec![lane("hits", "Hits")], vec![milestone]);
    let layout = engine().layout_timeline(&timeline, &LayoutOpts::default());

    let box_ = &layout.nodes["corte"];
    let line = 14.0 * 1.25; // TOKENS at the fixed measurer of the engine
    let usable = box_.size.w * (1.0 - line / box_.size.h);
    let label_w = "código congelado".chars().count() as f64 * 14.0 * 0.5;
    assert!(
        usable >= label_w - 1e-6,
        "la etiqueta cabe en el rombo: usable {usable} ≥ texto {label_w} (caja {:?})",
        box_.size
    );
}
