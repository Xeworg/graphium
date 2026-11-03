//! The `tree` layout engine (PRD §6: kinds `tree` y `mindmap`).
//!
//! A tree is not a layered graph: el padre va arriba, los hijos cuelgan de él
//! y ningún subárbol se pisa con su vecino. El orden de los hijos es el del
//! documento, así el dibujo es determinista.

use gx_core::geom::Size;
use gx_core::ir::{Direction, IrEdge, IrGraph, IrNode, LayoutHint};
use gx_core::layout::{LayoutEngine, LayoutOpts};
use gx_layout::TreeEngine;

fn node(id: &str, label: &str, w: f64, h: f64) -> IrNode {
    let mut node = IrNode::new(id, label);
    node.size = Size::new(w, h);
    node
}

fn tree(nodes: Vec<IrNode>, edges: Vec<IrEdge>) -> IrGraph {
    IrGraph {
        direction: Direction::TB,
        layout_hint: LayoutHint::Tree,
        nodes,
        edges,
        groups: Vec::new(),
    }
}

fn engine() -> TreeEngine {
    TreeEngine::new()
}

/// The box of a node as a rectangle.
fn box_of(layout: &gx_core::layout::LayoutGraph, id: &str) -> gx_core::geom::Rect {
    let placed = layout
        .nodes
        .get(id)
        .unwrap_or_else(|| panic!("{id} placed"));
    gx_core::geom::Rect::new(
        placed.center.x - placed.size.w / 2.0,
        placed.center.y - placed.size.h / 2.0,
        placed.size.w,
        placed.size.h,
    )
}

#[test]
fn a_single_node_is_placed_and_normalized() {
    let layout = engine().layout(
        &tree(vec![node("root", "root", 100.0, 40.0)], Vec::new()),
        &LayoutOpts::default(),
    );

    assert_eq!(layout.nodes.len(), 1);
    assert!(layout.edges.is_empty());
    let placed = box_of(&layout, "root");
    assert!(placed.x >= 0.0 && placed.y >= 0.0, "{placed:?}");
    assert_eq!(placed.w, 100.0);
    assert_eq!(placed.h, 40.0);
}

#[test]
fn the_parent_sits_above_its_children() {
    let layout = engine().layout(
        &tree(
            vec![
                node("root", "root", 100.0, 40.0),
                node("a", "a", 80.0, 30.0),
                node("b", "b", 80.0, 30.0),
            ],
            vec![IrEdge::new("root", "a"), IrEdge::new("root", "b")],
        ),
        &LayoutOpts::default(),
    );

    let root = box_of(&layout, "root");
    let a = box_of(&layout, "a");
    let b = box_of(&layout, "b");
    assert!(root.max_y() <= a.y, "el padre va arriba de sus hijos");
    assert!(root.max_y() <= b.y);
    assert_eq!(a.y, b.y, "los hermanos comparten nivel");
    assert!(a.max_x() <= b.x, "el orden del documento manda");
}

#[test]
fn no_two_subtrees_overlap() {
    // Un árbol desparejo: un hijo con tres nietos y otro con uno.
    let layout = engine().layout(
        &tree(
            vec![
                node("root", "root", 120.0, 40.0),
                node("left", "left", 100.0, 30.0),
                node("right", "right", 100.0, 30.0),
                node("l1", "l1", 90.0, 30.0),
                node("l2", "l2", 90.0, 30.0),
                node("l3", "l3", 90.0, 30.0),
                node("r1", "r1", 90.0, 30.0),
            ],
            vec![
                IrEdge::new("root", "left"),
                IrEdge::new("root", "right"),
                IrEdge::new("left", "l1"),
                IrEdge::new("left", "l2"),
                IrEdge::new("left", "l3"),
                IrEdge::new("right", "r1"),
            ],
        ),
        &LayoutOpts::default(),
    );

    let boxes: Vec<(String, gx_core::geom::Rect)> = layout
        .nodes
        .keys()
        .map(|id| (id.clone(), box_of(&layout, id)))
        .collect();
    for (index, (id, rect)) in boxes.iter().enumerate() {
        for (other_id, other) in boxes.iter().skip(index + 1) {
            assert!(
                !rect.overlaps(other),
                "`{id}` y `{other_id}` se pisan: {rect:?} {other:?}"
            );
        }
    }
    // El subárbol de la izquierda queda entero a la izquierda del de la derecha.
    let left = box_of(&layout, "left");
    let right = box_of(&layout, "right");
    let left_leaves = layout
        .nodes
        .iter()
        .filter(|(id, _)| ["l1", "l2", "l3"].contains(&id.as_str()))
        .map(|(_, placed)| placed.center.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let right_leaves = layout
        .nodes
        .iter()
        .filter(|(id, _)| id.as_str() == "r1")
        .map(|(_, placed)| placed.center.x)
        .fold(f64::INFINITY, f64::min);
    assert!(
        left_leaves < right_leaves,
        "los subárboles se ordenan como el documento"
    );
    assert!(left.center().x < right.center().x);
}

#[test]
fn a_deep_chain_goes_down_level_by_level() {
    let mut nodes = vec![node("n0", "n0", 80.0, 30.0)];
    let mut edges = Vec::new();
    for index in 1..6 {
        nodes.push(node(&format!("n{index}"), "n", 80.0, 30.0));
        edges.push(IrEdge::new(format!("n{}", index - 1), format!("n{index}")));
    }

    let layout = engine().layout(&tree(nodes, edges), &LayoutOpts::default());
    let mut previous = -1.0;
    for index in 0..6 {
        let placed = layout.nodes.get(&format!("n{index}")).expect("placed");
        assert!(
            placed.center.y > previous,
            "cada nivel baja: n{index} en {}",
            placed.center.y
        );
        previous = placed.center.y;
    }
}

#[test]
fn the_same_tree_is_laid_out_the_same_way() {
    let graph = tree(
        vec![
            node("root", "root", 100.0, 40.0),
            node("a", "a", 80.0, 30.0),
            node("b", "b", 80.0, 30.0),
        ],
        vec![IrEdge::new("root", "a"), IrEdge::new("root", "b")],
    );

    let first = engine().layout(&graph, &LayoutOpts::default());
    let second = engine().layout(&graph, &LayoutOpts::default());

    assert_eq!(first.nodes, second.nodes);
    assert_eq!(first.bbox, second.bbox);
}

#[test]
fn the_degraded_spacing_also_tightens_a_tree() {
    let graph = tree(
        vec![
            node("root", "root", 100.0, 40.0),
            node("a", "a", 80.0, 30.0),
            node("b", "b", 80.0, 30.0),
            node("c", "c", 80.0, 30.0),
        ],
        vec![
            IrEdge::new("root", "a"),
            IrEdge::new("root", "b"),
            IrEdge::new("root", "c"),
        ],
    );

    let normal = engine().layout(&graph, &LayoutOpts::default());
    let tight = engine().layout(
        &graph,
        &LayoutOpts {
            spacing_scale: gx_core::render::DEGRADED_SPACING_SCALE,
        },
    );

    assert!(
        tight.bbox.h < normal.bbox.h,
        "la escalera también aprieta el árbol"
    );
    assert!(tight.bbox.w < normal.bbox.w);
}

#[test]
fn the_edges_connect_the_boxes() {
    let layout = engine().layout(
        &tree(
            vec![
                node("root", "root", 100.0, 40.0),
                node("a", "a", 80.0, 30.0),
            ],
            vec![IrEdge::new("root", "a")],
        ),
        &LayoutOpts::default(),
    );

    assert_eq!(layout.edges.len(), 1);
    let points = &layout.edges[0].points;
    assert!(
        points.len() >= 2,
        "una arista necesita dos extremos: {points:?}"
    );
    let root = box_of(&layout, "root");
    let a = box_of(&layout, "a");
    assert!((points[0].y - root.max_y()).abs() < 80.0);
    assert!((points[points.len() - 1].y - a.y).abs() < 80.0);
}

/// El despachador elige el motor por `layout_hint`: un kind no nombra nunca un
/// motor, y agregar uno nuevo no toca ningún kind (PRD §6, O7).
#[test]
fn the_dispatcher_follows_the_layout_hint() {
    use gx_layout::Layouts;

    struct NoMetrics;
    impl gx_core::text::TextMetrics for NoMetrics {
        fn measure(&self, text: &str, style: &gx_core::text::TextStyle) -> Size {
            Size::new(text.chars().count() as f64 * style.size * 0.5, style.size)
        }
    }

    let layouts = Layouts::new(std::sync::Arc::new(NoMetrics));

    // Un árbol: el padre arriba de sus hijos.
    let as_tree = layouts.layout(
        &tree(
            vec![
                node("root", "root", 100.0, 40.0),
                node("a", "a", 80.0, 30.0),
            ],
            vec![IrEdge::new("root", "a")],
        ),
        &LayoutOpts::default(),
    );
    let root = box_of(&as_tree, "root");
    let child = box_of(&as_tree, "a");
    assert!(
        root.max_y() <= child.y,
        "el árbol no lo dibujó el motor de árboles"
    );

    // El despacho se compara contra cada motor por separado: el árbol tiene
    // que salir exactamente del motor de árboles, y el mismo grafo como
    // `layered`, exactamente de `dagre`.
    let graph = tree(
        vec![
            node("root", "root", 100.0, 40.0),
            node("a", "a", 80.0, 30.0),
            node("b", "b", 80.0, 30.0),
        ],
        vec![IrEdge::new("root", "a"), IrEdge::new("root", "b")],
    );
    let direct_tree = TreeEngine::new().layout(&graph, &LayoutOpts::default());
    assert_eq!(layouts.layout(&graph, &LayoutOpts::default()), direct_tree);

    let mut layered = graph.clone();
    layered.layout_hint = LayoutHint::Layered;
    let engine = gx_layout::DagreEngine::new(std::sync::Arc::new(NoMetrics));
    let direct_layered = engine.layout(&layered, &LayoutOpts::default());
    assert_eq!(
        layouts.layout(&layered, &LayoutOpts::default()),
        direct_layered
    );
}
