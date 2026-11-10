//! Fixtures shared by the unit tests.

use std::collections::BTreeMap;

use gx_core::{
    Direction, Emphasis, Ir, IrEdge, IrGraph, IrNode, LayoutEdge, LayoutGraph, LayoutHint, NodeBox,
    Point, Rect, Shape, Size, Style,
};

/// A plain rounded node sized like a real label.
pub(crate) fn node(id: &str, label: &str) -> IrNode {
    let mut node = IrNode::new(id, label);
    node.size = Size::new(104.0, 44.0);
    node
}

/// A node with a specific shape.
pub(crate) fn shaped(id: &str, label: &str, shape: Shape) -> IrNode {
    let mut node = node(id, label);
    node.shape = shape;
    node
}

/// An edge with a label.
pub(crate) fn labeled_edge(from: &str, to: &str, label: &str) -> IrEdge {
    let mut edge = IrEdge::new(from, to);
    edge.label = Some(label.to_string());
    edge
}

/// An `Ir::Graph` with no groups.
pub(crate) fn graph(nodes: Vec<IrNode>, edges: Vec<IrEdge>) -> Ir {
    Ir::Graph(IrGraph {
        direction: Direction::TB,
        layout_hint: LayoutHint::Layered,
        nodes,
        edges,
        groups: Vec::new(),
    })
}

/// A routed edge.
pub(crate) fn routed(
    from: &str,
    to: &str,
    points: &[(f64, f64)],
    label_at: Option<(f64, f64)>,
) -> LayoutEdge {
    LayoutEdge {
        from: from.to_string(),
        to: to.to_string(),
        points: points.iter().map(|(x, y)| Point::new(*x, *y)).collect(),
        label_at: label_at.map(|(x, y)| Point::new(x, y)),
    }
}

/// A `LayoutGraph` with an explicit bounding box.
pub(crate) fn layout(
    nodes: &[(&str, Point, Size)],
    edges: Vec<LayoutEdge>,
    bbox: Rect,
) -> LayoutGraph {
    let mut map = BTreeMap::new();
    for (id, center, size) in nodes {
        map.insert(
            (*id).to_string(),
            NodeBox {
                center: *center,
                size: *size,
            },
        );
    }
    LayoutGraph {
        activations: Vec::new(),
        axis: None,
        nodes: map,
        edges,
        groups: Vec::new(),
        bbox,
    }
}

/// Three nodes covering a rounded shape, a box and a cylinder.
pub(crate) fn fixture_ir() -> Ir {
    let mut ok = node("ok", "Config");
    ok.shape = Shape::Box;
    ok.emphasis = Emphasis::Highlight;

    let mut dashed = IrEdge::new("read", "cyl");
    dashed.emphasis = Emphasis::Muted;
    dashed.style = Style::Dashed;

    graph(
        vec![
            node("read", "read file"),
            ok,
            shaped("cyl", "a\nb", Shape::Cylinder),
        ],
        vec![labeled_edge("read", "ok", "utf-8"), dashed],
    )
}

/// The layout of [`fixture_ir`], with the bbox the layout normalizes to `(0, 0)`.
pub(crate) fn fixture_layout() -> LayoutGraph {
    layout(
        &[
            ("read", Point::new(60.0, 40.0), Size::new(104.0, 44.0)),
            ("ok", Point::new(60.0, 130.0), Size::new(78.0, 44.0)),
            ("cyl", Point::new(210.0, 85.0), Size::new(60.0, 60.0)),
        ],
        vec![
            routed(
                "read",
                "ok",
                &[(112.0, 40.0), (112.0, 90.0), (60.0, 108.0)],
                Some((90.0, 85.0)),
            ),
            routed("read", "cyl", &[(112.0, 46.0), (180.0, 70.0)], None),
        ],
        Rect::new(0.0, 0.0, 260.0, 170.0),
    )
}

/// [`fixture_ir`] with one routed edge per emphasis, so the marker set is full.
pub(crate) fn emphasized_ir() -> Ir {
    let emphasized = |emphasis: Emphasis| {
        let mut edge = IrEdge::new("read", "ok");
        edge.emphasis = emphasis;
        edge
    };
    graph(
        vec![node("read", "read file"), node("ok", "Config")],
        vec![
            emphasized(Emphasis::Normal),
            emphasized(Emphasis::Highlight),
            emphasized(Emphasis::Muted),
        ],
    )
}

/// The layout of [`emphasized_ir`].
pub(crate) fn emphasized_layout() -> LayoutGraph {
    layout(
        &[
            ("read", Point::new(60.0, 40.0), Size::new(104.0, 44.0)),
            ("ok", Point::new(60.0, 130.0), Size::new(78.0, 44.0)),
        ],
        vec![
            routed("read", "ok", &[(112.0, 40.0), (112.0, 108.0)], None),
            routed("read", "ok", &[(0.0, 40.0), (0.0, 108.0)], None),
            routed("read", "ok", &[(60.0, 40.0), (60.0, 108.0)], None),
        ],
        Rect::new(0.0, 0.0, 160.0, 170.0),
    )
}
