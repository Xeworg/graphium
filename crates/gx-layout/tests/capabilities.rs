//! The capability matrix of `dagre` (PRD §8.4).
//!
//! Every capability of the layout engine has its own test with a concrete
//! expectation: the directions, the clusters, the labels of the edges, the
//! self loops and the disconnected nodes.

use gx_core::geom::Point;
use gx_core::ir::{Direction, IrEdge, IrGraph, IrGroup, IrNode, IrRow};
use gx_core::layout::LayoutEngine;
use gx_core::layout::LayoutGraph;
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
    let layout = engine().layout(&graph(Direction::TB));
    assert!(layout.bbox.h > layout.bbox.w, "{:?}", layout.bbox);
    assert_normalized(&layout);
}

#[test]
fn left_to_right_is_wider_than_tall() {
    let layout = engine().layout(&graph(Direction::LR));
    assert!(layout.bbox.w > layout.bbox.h, "{:?}", layout.bbox);
}

#[test]
fn bottom_to_top_and_right_to_left_produce_a_layout() {
    for direction in [Direction::BT, Direction::RL] {
        let layout = engine().layout(&graph(direction));
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

    let layout = engine().layout(&graph);
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

#[test]
fn an_edge_with_a_label_gets_a_label_position() {
    let mut graph = graph(Direction::TB);
    graph.edges[0].label = Some("utf-8".to_string());

    let layout = engine().layout(&graph);
    let edge = layout
        .edges
        .iter()
        .find(|edge| edge.from == "a")
        .expect("edge");
    assert!(edge.label_at.is_some(), "the label needs a position");
}

#[test]
fn an_edge_without_a_label_has_no_label_position() {
    let layout = engine().layout(&graph(Direction::TB));
    assert!(layout.edges.iter().all(|edge| edge.label_at.is_none()));
}

#[test]
fn a_self_loop_routes_without_degenerating() {
    let mut graph = graph(Direction::TB);
    graph.edges.push(IrEdge::new("a", "a"));

    let layout = engine().layout(&graph);
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

    let layout = engine().layout(&graph);
    let island = layout.nodes.get("island").expect("placed");
    assert!(island.center.x >= layout.bbox.x);
    assert!(island.center.x <= layout.bbox.max_x());
    assert!(island.center.y >= layout.bbox.y);
    assert!(island.center.y <= layout.bbox.max_y());
}

#[test]
fn the_same_graph_produces_the_same_layout() {
    let first = engine().layout(&graph(Direction::TB));
    let second = engine().layout(&graph(Direction::TB));
    assert_eq!(first.nodes, second.nodes);
    assert_eq!(first.edges, second.edges);
    assert_eq!(first.bbox, second.bbox);
}

#[test]
fn an_empty_graph_has_an_empty_layout() {
    let layout = engine().layout(&IrGraph {
        direction: Direction::TB,
        layout_hint: gx_core::ir::LayoutHint::Layered,
        nodes: Vec::new(),
        edges: Vec::new(),
        groups: Vec::new(),
    });
    assert!(layout.nodes.is_empty());
    assert!(layout.edges.is_empty());
}

#[test]
fn a_node_reports_its_center_and_its_box() {
    let layout = engine().layout(&graph(Direction::TB));
    let placed = layout.nodes.get("a").expect("placed");
    assert!(placed.size.w > 0.0 && placed.size.h > 0.0);
    assert!(placed.center.x > 0.0 || placed.center.y > 0.0);
}

#[test]
fn the_origin_of_the_drawing_is_reachable() {
    let layout = engine().layout(&graph(Direction::TB));
    let closest = layout
        .nodes
        .values()
        .map(|placed| placed.center.x.min(placed.center.y))
        .fold(f64::INFINITY, f64::min);
    assert!(closest >= 0.0);
    assert!(closest < f64::INFINITY);
    let _ = Point::new(0.0, 0.0);
}

/// A relation of a schema goes straight between the two key rows when the
/// tables sit side by side: a bent line reads as another relation.
#[test]
fn a_relation_between_side_by_side_tables_is_a_straight_segment() {
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

    let layout = DagreEngine::new(Arc::new(FakeMetrics)).layout(&graph);
    let route = &layout.edges[0];
    assert_eq!(
        route.points.len(),
        2,
        "the route has to be a straight segment: {:?}",
        route.points
    );
    // The two ends are on different rows, so the segment is diagonal but it
    // has no bend: a middle point would mean a routed polyline.
    assert_ne!(
        route.points[0].y, route.points[1].y,
        "each end anchors to its own row"
    );
}
