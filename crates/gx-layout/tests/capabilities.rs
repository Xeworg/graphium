//! The capability matrix of `dagre` (PRD §8.4).
//!
//! Every capability of the layout engine has its own test with a concrete
//! expectation: the directions, the clusters, the labels of the edges, the
//! self loops and the disconnected nodes.

use gx_core::geom::Point;
use gx_core::ir::{Direction, IrEdge, IrGraph, IrGroup, IrNode, IrRow};
use gx_core::layout::{LayoutEngine, LayoutGraph, LayoutGroup, LayoutOpts};
use gx_core::render::DEGRADED_SPACING_SCALE;
use gx_layout::DagreEngine;
use std::sync::Arc;

/// A measurer with a fixed width per character, so the tests do not
/// depend on the font.
struct FakeMetrics;

impl gx_core::text::TextMetrics for FakeMetrics {
    fn measure(&self, text: &str, style: &gx_core::text::TextStyle) -> gx_core::geom::Size {
        let lines = text.lines().count().max(1) as f64;
        gx_core::geom::Size::new(
            text.chars().count() as f64 * style.size * 0.5,
            lines * style.size * style.line_height,
        )
    }
}

fn engine() -> DagreEngine {
    DagreEngine::new(Arc::new(FakeMetrics))
}

fn node(id: &str, label: &str) -> IrNode {
    let mut node = IrNode::new(id, label);
    node.size = gx_core::geom::Size::new(60.0, 30.0);
    node
}

fn graph(direction: Direction) -> IrGraph {
    IrGraph {
        direction,
        layout_hint: gx_core::ir::LayoutHint::Layered,
        nodes: vec![
            node("a", "A"),
            node("b", "B"),
            node("c", "C"),
            node("d", "D"),
        ],
        edges: vec![
            IrEdge::new("a", "b"),
            IrEdge::new("b", "c"),
            IrEdge::new("c", "d"),
        ],
        groups: Vec::new(),
    }
}

/// Every point of the drawing is at or after the origin.
fn assert_normalized(layout: &LayoutGraph) {
    for placed in layout.nodes.values() {
        assert!(
            placed.center.x >= 0.0 && placed.center.y >= 0.0,
            "the drawing must start at (0, 0): {placed:?}"
        );
    }
    assert!(layout.bbox.x >= 0.0 && layout.bbox.y >= 0.0);
}

#[test]
fn top_to_bottom_is_taller_than_wide() {
    let layout = engine().layout(&graph(Direction::TB), &LayoutOpts::default());
    assert!(layout.bbox.h > layout.bbox.w, "{:?}", layout.bbox);
    assert_normalized(&layout);
}

#[test]
fn left_to_right_is_wider_than_tall() {
    let layout = engine().layout(&graph(Direction::LR), &LayoutOpts::default());
    assert!(layout.bbox.w > layout.bbox.h, "{:?}", layout.bbox);
}

#[test]
fn bottom_to_top_and_right_to_left_produce_a_layout() {
    for direction in [Direction::BT, Direction::RL] {
        let layout = engine().layout(&graph(direction), &LayoutOpts::default());
        assert_eq!(layout.nodes.len(), 4, "{direction:?}");
        assert_eq!(layout.edges.len(), 3, "{direction:?}");
        assert!(layout.bbox.w > 0.0 && layout.bbox.h > 0.0, "{direction:?}");
    }
}

#[test]
fn clusters_appear_as_groups_that_contain_their_members() {
    let mut graph = graph(Direction::TB);
    graph.groups.push(IrGroup {
        id: "cluster".to_string(),
        label: Some("cluster".to_string()),
        members: vec!["a".to_string(), "b".to_string()],
    });

    let layout = engine().layout(&graph, &LayoutOpts::default());
    assert_eq!(layout.groups.len(), 1);
    let group = &layout.groups[0];
    assert_eq!(group.id, "cluster");

    for member in ["a", "b"] {
        let placed = layout.nodes.get(member).expect("member placed");
        assert!(
            group.bbox.x <= placed.center.x && placed.center.x <= group.bbox.max_x(),
            "the cluster must contain {member}"
        );
        assert!(
            group.bbox.y <= placed.center.y && placed.center.y <= group.bbox.max_y(),
            "the cluster must contain {member}"
        );
    }
}

/// RF15: los grupos anidan. El layout dice la profundidad de cada uno (la caja
/// externa se dibuja primero) y la caja externa contiene a la interna.
#[test]
fn nested_clusters_carry_their_depth_and_contain_each_other() {
    let mut graph = graph(Direction::TB);
    graph.groups.push(IrGroup {
        id: "outer".to_string(),
        label: Some("outer".to_string()),
        members: vec!["inner".to_string()],
    });
    graph.groups.push(IrGroup {
        id: "inner".to_string(),
        label: Some("inner".to_string()),
        members: vec!["a".to_string(), "b".to_string()],
    });

    let layout = engine().layout(&graph, &LayoutOpts::default());
    let outer = layout
        .groups
        .iter()
        .find(|group| group.id == "outer")
        .expect("el grupo externo se coloca");
    let inner = layout
        .groups
        .iter()
        .find(|group| group.id == "inner")
        .expect("el grupo interno se coloca");

    assert_eq!(outer.depth, 0);
    assert_eq!(inner.depth, 1);

    assert!(
        outer.bbox.x <= inner.bbox.x && outer.bbox.y <= inner.bbox.y,
        "la caja externa arranca antes: {outer:?} {inner:?}"
    );
    assert!(
        outer.bbox.max_x() >= inner.bbox.max_x() && outer.bbox.max_y() >= inner.bbox.max_y(),
        "la caja externa termina después: {outer:?} {inner:?}"
    );

    let placed = layout.nodes.get("a").expect("el miembro se coloca");
    assert!(
        inner.bbox.x <= placed.center.x && placed.center.x <= inner.bbox.max_x(),
        "el miembro vive dentro del grupo interno"
    );
}

/// Tres niveles de anidamiento: `outer` contiene al nodo `top` y al grupo
/// `middle`, `middle` contiene a `inner` y `inner` contiene a los nodos `a` y
/// `b`. `c` y `d` quedan afuera de todo grupo.
///
/// S5: a `dagre` sólo llegan los clusters hoja. S6: la caja de un grupo que
/// contiene a otro se calcula en el layout como la unión de las cajas de sus
/// miembros más el padding, de adentro hacia afuera.
fn three_level_nest() -> IrGraph {
    let mut graph = graph(Direction::TB);
    graph.nodes.push(node("top", "Top"));
    graph.groups.push(IrGroup {
        id: "outer".to_string(),
        label: Some("outer".to_string()),
        members: vec!["top".to_string(), "middle".to_string()],
    });
    graph.groups.push(IrGroup {
        id: "middle".to_string(),
        label: Some("middle".to_string()),
        members: vec!["inner".to_string()],
    });
    graph.groups.push(IrGroup {
        id: "inner".to_string(),
        label: Some("inner".to_string()),
        members: vec!["a".to_string(), "b".to_string()],
    });
    graph
}

fn group_of<'a>(layout: &'a LayoutGraph, id: &str) -> &'a LayoutGroup {
    layout
        .groups
        .iter()
        .find(|group| group.id == id)
        .unwrap_or_else(|| panic!("el grupo `{id}` no se coloca"))
}

fn node_box(layout: &LayoutGraph, id: &str) -> gx_core::geom::Rect {
    let placed = layout
        .nodes
        .get(id)
        .unwrap_or_else(|| panic!("el nodo `{id}` no se coloca"));
    gx_core::geom::Rect::new(
        placed.center.x - placed.size.w / 2.0,
        placed.center.y - placed.size.h / 2.0,
        placed.size.w,
        placed.size.h,
    )
}

fn assert_contains(outer: &gx_core::geom::Rect, inner: &gx_core::geom::Rect, message: &str) {
    assert!(
        outer.min_x() <= inner.min_x()
            && outer.min_y() <= inner.min_y()
            && outer.max_x() >= inner.max_x()
            && outer.max_y() >= inner.max_y(),
        "{message}: {outer:?} no contiene a {inner:?}"
    );
}

/// S5/S6: con tres niveles todos los nodos se colocan, la profundidad de cada
/// grupo sale de la IR y cada caja contiene la de adentro.
#[test]
fn a_three_level_nest_places_every_node_and_nests_its_boxes() {
    let graph = three_level_nest();
    let layout = engine().layout(&graph, &LayoutOpts::default());

    assert_eq!(
        layout.nodes.len(),
        graph.nodes.len(),
        "el anidamiento no puede perder nodos"
    );

    let outer = group_of(&layout, "outer");
    let middle = group_of(&layout, "middle");
    let inner = group_of(&layout, "inner");
    assert_eq!(outer.depth, 0);
    assert_eq!(middle.depth, 1);
    assert_eq!(inner.depth, 2);

    assert_contains(
        &outer.bbox,
        &middle.bbox,
        "la caja externa no contiene a la del medio",
    );
    assert_contains(
        &middle.bbox,
        &inner.bbox,
        "la del medio no contiene a la interna",
    );

    for member in ["a", "b"] {
        assert_contains(
            &inner.bbox,
            &node_box(&layout, member),
            "el grupo interno no contiene a su miembro",
        );
    }
    let top = node_box(&layout, "top");
    assert_contains(
        &outer.bbox,
        &top,
        "la caja externa no contiene al nodo que cuelga de ella",
    );

    // El nombre de la caja se escribe arriba, sobre `y = bbox.y + text_size`,
    // así que el borde de arriba se expande más que el de abajo: la línea del
    // nombre tiene que entrar en la caja sin pisar al primer miembro.
    let members = middle.bbox.union(&top);
    let above = members.min_y() - outer.bbox.min_y();
    let below = outer.bbox.max_y() - members.max_y();
    assert!(
        above > below,
        "el borde de arriba deja lugar al nombre: {above} vs {below}"
    );
}

/// S6: la caja local es determinista. La misma entrada, dos veces, da los
/// mismos valores: es lo que hace comparables los bytes del SVG.
#[test]
fn the_same_nest_lays_out_the_same_way_twice() {
    let graph = three_level_nest();

    let first = engine().layout(&graph, &LayoutOpts::default());
    let second = engine().layout(&graph, &LayoutOpts::default());

    assert_eq!(first.nodes, second.nodes);
    assert_eq!(first.edges, second.edges);
    assert_eq!(first.groups, second.groups);
    assert_eq!(first.bbox, second.bbox);
}

/// Una arista que no une dos nodos declarados no puede voltear el motor:
/// `dagre` hace panick en su network simplex cuando un extremo es un grupo o un
/// id que no existe. La puerta de los kinds ya rechaza ese documento; el motor
/// igual no se cae, ignora la arista y coloca todo.
#[test]
fn an_edge_that_names_a_group_or_an_unknown_id_is_ignored() {
    let mut graph = graph(Direction::TB);
    graph.groups.push(IrGroup {
        id: "cluster".to_string(),
        label: Some("cluster".to_string()),
        members: vec!["a".to_string(), "b".to_string()],
    });
    graph.edges.push(IrEdge::new("a", "cluster"));
    graph.edges.push(IrEdge::new("cluster", "b"));
    graph.edges.push(IrEdge::new("c", "fantasma"));

    let layout = engine().layout(&graph, &LayoutOpts::default());

    assert_eq!(layout.nodes.len(), graph.nodes.len());
    for routed in &layout.edges {
        assert!(
            graph.nodes.iter().any(|node| node.id == routed.from)
                && graph.nodes.iter().any(|node| node.id == routed.to),
            "la arista envenenada no se rutea: {routed:?}"
        );
    }
    // The three original edges are routed; the three poisoned ones are not.
    assert_eq!(layout.edges.len(), 3);
}

#[test]
fn an_edge_with_a_label_gets_a_label_position() {
    let mut graph = graph(Direction::TB);
    graph.edges[0].label = Some("utf-8".to_string());

    let layout = engine().layout(&graph, &LayoutOpts::default());
    let edge = layout
        .edges
        .iter()
        .find(|edge| edge.from == "a")
        .expect("edge");
    assert!(edge.label_at.is_some(), "the label needs a position");
}

#[test]
fn an_edge_without_a_label_has_no_label_position() {
    let layout = engine().layout(&graph(Direction::TB), &LayoutOpts::default());
    assert!(layout.edges.iter().all(|edge| edge.label_at.is_none()));
}

#[test]
fn a_self_loop_routes_without_degenerating() {
    let mut graph = graph(Direction::TB);
    graph.edges.push(IrEdge::new("a", "a"));

    let layout = engine().layout(&graph, &LayoutOpts::default());
    let edge = layout
        .edges
        .iter()
        .find(|edge| edge.from == "a" && edge.to == "a")
        .expect("self loop");
    assert!(
        edge.points.len() >= 3,
        "a self loop needs a real route: {:?}",
        edge.points
    );
}

#[test]
fn disconnected_nodes_are_placed_inside_the_drawing() {
    let mut graph = graph(Direction::TB);
    graph.nodes.push(node("island", "island"));

    let layout = engine().layout(&graph, &LayoutOpts::default());
    let island = layout.nodes.get("island").expect("placed");
    assert!(island.center.x >= layout.bbox.x);
    assert!(island.center.x <= layout.bbox.max_x());
    assert!(island.center.y >= layout.bbox.y);
    assert!(island.center.y <= layout.bbox.max_y());
}

#[test]
fn the_same_graph_produces_the_same_layout() {
    let first = engine().layout(&graph(Direction::TB), &LayoutOpts::default());
    let second = engine().layout(&graph(Direction::TB), &LayoutOpts::default());
    assert_eq!(first.nodes, second.nodes);
    assert_eq!(first.edges, second.edges);
    assert_eq!(first.bbox, second.bbox);
}

/// The second step of the degradation ladder (PRD §7.3): the same graph laid
/// out with the degraded spacing is strictly tighter on both axes, and every
/// box keeps the size the graph declared.
#[test]
fn the_degraded_spacing_is_tighter() {
    // Two nodes share a rank and the drawing has three ranks, so the width
    // depends on `nodesep` and the height on `ranksep`: both shrink.
    let graph = IrGraph {
        direction: Direction::TB,
        layout_hint: gx_core::ir::LayoutHint::Layered,
        nodes: vec![
            node("a", "A"),
            node("b", "B"),
            node("c", "C"),
            node("d", "D"),
            node("e", "E"),
        ],
        edges: vec![
            IrEdge::new("a", "b"),
            IrEdge::new("a", "c"),
            IrEdge::new("b", "d"),
            IrEdge::new("c", "e"),
        ],
        groups: Vec::new(),
    };

    let engine = engine();
    let default = engine.layout(&graph, &LayoutOpts::default());
    let degraded = engine.layout(
        &graph,
        &LayoutOpts {
            spacing_scale: DEGRADED_SPACING_SCALE,
        },
    );

    assert!(
        degraded.bbox.w < default.bbox.w,
        "the degraded drawing must be narrower: {:?} vs {:?}",
        degraded.bbox,
        default.bbox
    );
    assert!(
        degraded.bbox.h < default.bbox.h,
        "the degraded drawing must be shorter: {:?} vs {:?}",
        degraded.bbox,
        default.bbox
    );

    // The same graph yields the same nodes, placed in both runs.
    let default_ids: Vec<&String> = default.nodes.keys().collect();
    let degraded_ids: Vec<&String> = degraded.nodes.keys().collect();
    assert_eq!(default_ids, degraded_ids);
    assert_eq!(degraded.nodes.len(), graph.nodes.len());

    // The box is the size the graph declared (`node.size`): the layout does
    // not measure the text again, so the door's measurement is the only one.
    for node in &graph.nodes {
        let placed = default.nodes.get(&node.id).expect("placed");
        assert_eq!(placed.size, node.size, "{} measured twice", node.id);
        let placed = degraded.nodes.get(&node.id).expect("placed");
        assert_eq!(placed.size, node.size, "{} measured twice", node.id);
    }
}

/// A relation that skips a table cannot cross it: it goes around, and it
/// keeps the right angles. A diagonal there leaves the reader guessing which
/// table the line belongs to and, worse, looks like it touches the one in the
/// middle (PRD §7.3, L10).
#[test]
fn a_relation_that_skips_a_table_goes_around_it() {
    fn table(id: &str) -> IrNode {
        let mut node = IrNode::new(id, id);
        node.size = gx_core::geom::Size::new(60.0, 30.0);
        node.rows = vec![IrRow::new("id"), IrRow::new("user_id")];
        node
    }
    fn relation(from: &str, to: &str, to_row: u32) -> IrEdge {
        let mut edge = IrEdge::new(from, to);
        edge.from_row = Some(0);
        edge.to_row = Some(to_row);
        edge.arrow = false;
        edge
    }

    let graph = IrGraph {
        direction: Direction::LR,
        layout_hint: gx_core::ir::LayoutHint::Layered,
        nodes: vec![table("users"), table("posts"), table("comments")],
        edges: vec![
            relation("users", "posts", 1),
            relation("posts", "comments", 1),
            // The relation of the schema that skips the table in the middle.
            relation("users", "comments", 1),
        ],
        groups: Vec::new(),
    };

    let layout = DagreEngine::new(Arc::new(FakeMetrics)).layout(&graph, &LayoutOpts::default());
    let route = layout
        .edges
        .iter()
        .find(|edge| edge.from == "users" && edge.to == "comments")
        .expect("the skipping relation is routed");

    // Every segment is horizontal or vertical.
    for pair in route.points.windows(2) {
        let horizontal = (pair[0].y - pair[1].y).abs() < 0.001;
        let vertical = (pair[0].x - pair[1].x).abs() < 0.001;
        assert!(
            horizontal || vertical,
            "the skip route keeps the right angles: {:?}",
            pair
        );
    }

    // No segment crosses a table: the line does not go through a box.
    for pair in route.points.windows(2) {
        let (x0, x1) = (
            pair[0].x.min(pair[1].x) - 0.5,
            pair[0].x.max(pair[1].x) + 0.5,
        );
        let (y0, y1) = (
            pair[0].y.min(pair[1].y) - 0.5,
            pair[0].y.max(pair[1].y) + 0.5,
        );
        for (id, placed) in &layout.nodes {
            let left = placed.center.x - placed.size.w / 2.0;
            let right = placed.center.x + placed.size.w / 2.0;
            let top = placed.center.y - placed.size.h / 2.0;
            let bottom = placed.center.y + placed.size.h / 2.0;
            let crosses = x0 < right && left < x1 && y0 < bottom && top < y1;
            assert!(!crosses, "the route crosses the table `{id}`: {:?}", pair);
        }
    }

    // The route leaves the band of the tables in the middle: it passes above
    // or below it instead of through it.
    let middle = &layout.nodes["posts"];
    let middle_top = middle.center.y - middle.size.h / 2.0;
    let middle_bottom = middle.center.y + middle.size.h / 2.0;
    let passes_outside = route
        .points
        .iter()
        .any(|point| point.y < middle_top || point.y > middle_bottom);
    assert!(
        passes_outside,
        "the route has to leave the band of the middle table: {:?}",
        route.points
    );
}

#[test]
fn an_empty_graph_has_an_empty_layout() {
    let layout = engine().layout(
        &IrGraph {
            direction: Direction::TB,
            layout_hint: gx_core::ir::LayoutHint::Layered,
            nodes: Vec::new(),
            edges: Vec::new(),
            groups: Vec::new(),
        },
        &LayoutOpts::default(),
    );
    assert!(layout.nodes.is_empty());
    assert!(layout.edges.is_empty());
}

#[test]
fn a_node_reports_its_center_and_its_box() {
    let layout = engine().layout(&graph(Direction::TB), &LayoutOpts::default());
    let placed = layout.nodes.get("a").expect("placed");
    assert!(placed.size.w > 0.0 && placed.size.h > 0.0);
    assert!(placed.center.x > 0.0 || placed.center.y > 0.0);
}

#[test]
fn the_origin_of_the_drawing_is_reachable() {
    let layout = engine().layout(&graph(Direction::TB), &LayoutOpts::default());
    let closest = layout
        .nodes
        .values()
        .map(|placed| placed.center.x.min(placed.center.y))
        .fold(f64::INFINITY, f64::min);
    assert!(closest >= 0.0);
    assert!(closest < f64::INFINITY);
    let _ = Point::new(0.0, 0.0);
}

/// A relation of a schema is routed with right angles when the tables sit
/// side by side: every segment is horizontal or vertical, so the reader
/// follows it without guessing which row it belongs to.
#[test]
fn a_relation_between_side_by_side_tables_is_orthogonal() {
    let mut source = IrNode::new("users", "users");
    source.rows = vec![IrRow::new("id"), IrRow::new("email")];
    let mut target = IrNode::new("posts", "posts");
    target.rows = vec![IrRow::new("id"), IrRow::new("user_id")];

    let mut edge = IrEdge::new("users", "posts");
    edge.from_row = Some(0);
    edge.to_row = Some(1);
    edge.arrow = false;

    let graph = IrGraph {
        direction: Direction::LR,
        layout_hint: gx_core::ir::LayoutHint::Layered,
        nodes: vec![source, target],
        edges: vec![edge],
        groups: Vec::new(),
    };

    let layout = DagreEngine::new(Arc::new(FakeMetrics)).layout(&graph, &LayoutOpts::default());
    let route = &layout.edges[0];

    // The Z: out, turn, in.
    assert_eq!(
        route.points.len(),
        4,
        "a routed relation needs the two turns: {:?}",
        route.points
    );
    for pair in route.points.windows(2) {
        let horizontal = (pair[0].y - pair[1].y).abs() < 0.001;
        let vertical = (pair[0].x - pair[1].x).abs() < 0.001;
        assert!(
            horizontal || vertical,
            "every segment is horizontal or vertical: {:?}",
            pair
        );
    }
    // The turns share the x of the middle of the gap.
    assert!((route.points[1].x - route.points[2].x).abs() < 0.001);
    // Each end anchors to its own row.
    assert_ne!(route.points[0].y, route.points[3].y);
}

/// When the two rows are at the same height the relation is one horizontal
/// segment: a turn with zero length would be noise.
#[test]
fn a_relation_between_rows_at_the_same_height_is_one_segment() {
    let mut source = IrNode::new("users", "users");
    source.rows = vec![IrRow::new("id")];
    let mut target = IrNode::new("posts", "posts");
    target.rows = vec![IrRow::new("user_id")];

    let mut edge = IrEdge::new("users", "posts");
    edge.from_row = Some(0);
    edge.to_row = Some(0);
    edge.arrow = false;

    let graph = IrGraph {
        direction: Direction::LR,
        layout_hint: gx_core::ir::LayoutHint::Layered,
        nodes: vec![source, target],
        edges: vec![edge],
        groups: Vec::new(),
    };

    let layout = DagreEngine::new(Arc::new(FakeMetrics)).layout(&graph, &LayoutOpts::default());
    assert_eq!(layout.edges[0].points.len(), 2);
}

// T6b (`f22-dibujo.md`, S10/L3b): the flowchart stubs — a branch edge of a
// diamond anchors at the vertex the flow direction names (the bottom one for
// `TB`), and an edge that arrives anchors at the opposite vertex. No diagonal
// stub hangs off a slanted edge; a box keeps its border anchoring.
#[test]
fn a_diamond_branches_from_its_bottom_vertex_and_is_entered_at_its_top() {
    let mut decision = node("d", "¿supera el límite?");
    decision.shape = gx_core::ir::Shape::Diamond;
    let graph = IrGraph {
        direction: Direction::TB,
        layout_hint: gx_core::ir::LayoutHint::Layered,
        nodes: vec![
            node("a", "monto"),
            decision,
            node("f", "fin"),
            node("g", "acto"),
        ],
        edges: vec![
            IrEdge::new("a", "d"),
            IrEdge::new("d", "f"),
            IrEdge::new("d", "g"),
        ],
        groups: Vec::new(),
    };

    let layout = engine().layout(&graph, &LayoutOpts::default());
    let d = &layout.nodes["d"];

    let outgoing = layout
        .edges
        .iter()
        .find(|edge| edge.from == "d")
        .expect("the branch leaves the decision");
    let start = outgoing.points[0];
    assert!(
        (start.x - d.center.x).abs() < 1e-6
            && (start.y - (d.center.y + d.size.h / 2.0)).abs() < 1e-6,
        "the branch leaves the bottom vertex: {start:?} vs {:?}",
        (d.center.x, d.center.y + d.size.h / 2.0)
    );

    let incoming = layout
        .edges
        .iter()
        .find(|edge| edge.to == "d")
        .expect("the edge arrives at the decision");
    let end = incoming.points.last().copied().unwrap();
    assert!(
        (end.x - d.center.x).abs() < 1e-6 && (end.y - (d.center.y - d.size.h / 2.0)).abs() < 1e-6,
        "the edge enters at the top vertex: {end:?}"
    );

    // A box keeps its border anchoring: the rule is the shape's, not every
    // edge's.
    let plain = &layout.nodes["a"];
    let from_box = layout
        .edges
        .iter()
        .find(|edge| edge.from == "a")
        .expect("the edge leaves the box");
    let start = from_box.points[0];
    assert!(
        start.y <= plain.center.y + plain.size.h / 2.0 + 1e-6
            && start.y >= plain.center.y - plain.size.h / 2.0 - 1e-6,
        "the box anchor rides its border: {start:?}"
    );
}
