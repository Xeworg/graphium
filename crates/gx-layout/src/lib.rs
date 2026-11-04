//! `gx-layout` — the layout engines of Graphium (PRD §8.4).
//!
//! `dagre` stays isolated behind the [`LayoutEngine`] trait of `gx-core`: the
//! engine receives the text measurer as a parameter and never measures text
//! on its own (PRD §8.4.1). The crate depends on `gx-core` and nothing else
//! (PRD §9.1).

pub mod matrix;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use dagre::graph::{Graph, GraphOptions};
use dagre::{layout, EdgeLabel, LayoutOptions, NodeLabel, RankDir, Ranker};
use gx_core::geom::{Point, Rect, Size};
use gx_core::ir::{Direction, IrEvent, IrGraph, IrLane, IrTimeline};
use gx_core::layout::{LayoutEdge, LayoutEngine, LayoutGraph, LayoutGroup, LayoutOpts, NodeBox};
use gx_core::text::{TextMetrics, TextStyle};
use gx_core::tokens::TOKENS;

/// Separation between nodes of the same rank, in plan units.
const NODE_SEPARATION: f64 = 60.0;
/// Separation between ranks, in plan units.
const RANK_SEPARATION: f64 = 80.0;
/// Separation between ranks when the nodes have rows.
///
/// A table is wide and its relations carry glyphs (the crow's foot and the
/// circle of the optional side), so the gap between the two tables has to be
/// bigger than the gap between two plain boxes.
const RANK_SEPARATION_WITH_ROWS: f64 = 130.0;
/// Separation between edges of the same rank.
const EDGE_SEPARATION: f64 = 20.0;
/// How far a detour runs from the tables it passes: it goes through the gap
/// between ranks, never along the edge of a box.
const DETOUR_CLEARANCE: f64 = 12.0;
/// Margin around the drawing.
const MARGIN: f64 = 10.0;

/// The layered layout engine, backed by `dagre` 0.1.2 (PRD §8.4).
pub struct DagreEngine {
    metrics: Arc<dyn TextMetrics>,
}

impl DagreEngine {
    /// Builds the engine around the single metrics source (PRD §8.4.1).
    pub fn new(metrics: Arc<dyn TextMetrics>) -> Self {
        Self { metrics }
    }
}

fn rank_dir(direction: Direction) -> RankDir {
    match direction {
        Direction::TB => RankDir::TB,
        Direction::BT => RankDir::BT,
        Direction::LR => RankDir::LR,
        Direction::RL => RankDir::RL,
    }
}

impl LayoutEngine for DagreEngine {
    fn layout(&self, graph: &IrGraph, opts: &LayoutOpts) -> LayoutGraph {
        let mut dagre: Graph<NodeLabel, EdgeLabel> = Graph::with_options(GraphOptions {
            directed: true,
            multigraph: true,
            compound: true,
        });

        for node in &graph.nodes {
            // The size is the one the door measured (PRD §8.4.1): measuring
            // again here gave two sources of truth for the same box, and
            // they drifted apart when the degradation changed the text.
            let size = node.size;
            dagre.set_node(
                node.id.clone(),
                Some(NodeLabel {
                    width: size.w,
                    height: size.h,
                    label: Some(node.label.clone()),
                    ..Default::default()
                }),
            );
        }

        // Clusters: a compound parent node with the members below it. Every
        // group is registered first, because a group can be a member of another
        // one (PRD §7 RF15) and the engine needs both ends to exist before it
        // links them.
        for group in &graph.groups {
            dagre.set_node(group.id.clone(), Some(NodeLabel::default()));
        }
        for (parent, member) in group_parents(graph) {
            dagre.set_parent(member, Some(parent));
        }

        for (index, edge) in graph.edges.iter().enumerate() {
            let mut label = EdgeLabel::default();
            if let Some(text) = &edge.label {
                let measured = self
                    .metrics
                    .measure(text, &TextStyle::regular(TOKENS.text_size));
                label.width = measured.w;
                label.height = measured.h;
            }
            // The name keeps the edges apart in the multigraph: two edges
            // with the same origin and destination are two different edges.
            let name = format!("e{index}");
            dagre.set_edge(
                edge.from.clone(),
                edge.to.clone(),
                Some(label),
                Some(name.as_str()),
            );
        }

        let has_rows = graph.nodes.iter().any(|node| !node.rows.is_empty());
        let scale = opts.spacing_scale;
        let options = LayoutOptions {
            rankdir: rank_dir(graph.direction),
            nodesep: NODE_SEPARATION * scale,
            ranksep: if has_rows {
                RANK_SEPARATION_WITH_ROWS * scale
            } else {
                RANK_SEPARATION * scale
            },
            edgesep: EDGE_SEPARATION * scale,
            marginx: MARGIN,
            marginy: MARGIN,
            ranker: Ranker::NetworkSimplex,
            ..Default::default()
        };
        layout(&mut dagre, Some(options));

        // `dagre` reports the center of every node, which is what
        // `NodeBox.center` holds (F0 decision L9).
        let mut nodes: BTreeMap<String, NodeBox> = BTreeMap::new();
        for node in &graph.nodes {
            let Some(label) = dagre.node(node.id.as_str()) else {
                continue;
            };
            let (Some(x), Some(y)) = (label.x, label.y) else {
                continue;
            };
            nodes.insert(
                node.id.clone(),
                NodeBox {
                    center: Point::new(x, y),
                    size: Size::new(label.width, label.height),
                },
            );
        }

        let mut edges = Vec::new();
        for (index, edge) in graph.edges.iter().enumerate() {
            let _ = index;
            let name = format!("e{index}");
            let Some(label) = dagre.edge(edge.from.as_str(), edge.to.as_str(), Some(name.as_str()))
            else {
                continue;
            };
            let points: Vec<Point> = label
                .points
                .iter()
                .map(|point| Point::new(point.x, point.y))
                .collect();
            let label_at = if edge.label.is_some() {
                label.x.zip(label.y).map(|(x, y)| Point::new(x, y))
            } else {
                None
            };
            let mut points = points;
            // The ports of a schema: the relation leaves and arrives at the
            // row of the key, and when the two tables sit side by side the
            // route is a straight segment between those two rows. A bent line
            // reads as a different relation (PRD §6.1).
            if let Some(orthogonal) = orthogonal_route(&nodes, edge) {
                points = orthogonal;
            }
            edges.push(LayoutEdge {
                from: edge.from.clone(),
                to: edge.to.clone(),
                points,
                label_at,
            });
        }

        let depths = group_depths(graph);
        let mut groups = Vec::new();
        for group in &graph.groups {
            let Some(label) = dagre.node(group.id.as_str()) else {
                continue;
            };
            let (Some(x), Some(y)) = (label.x, label.y) else {
                continue;
            };
            groups.push(LayoutGroup {
                id: group.id.clone(),
                label: group.label.clone(),
                bbox: Rect::new(
                    x - label.width / 2.0,
                    y - label.height / 2.0,
                    label.width,
                    label.height,
                ),
                depth: depths.get(group.id.as_str()).copied().unwrap_or(0),
            });
        }

        // Normalize so the drawing starts at (0, 0) (F0 decision L9).
        let offset = drawing_offset(&nodes, &edges, &groups);
        for placed in nodes.values_mut() {
            placed.center.x -= offset.0;
            placed.center.y -= offset.1;
        }
        for edge in &mut edges {
            for point in &mut edge.points {
                point.x -= offset.0;
                point.y -= offset.1;
            }
            if let Some(point) = &mut edge.label_at {
                point.x -= offset.0;
                point.y -= offset.1;
            }
        }
        for group in &mut groups {
            group.bbox.x -= offset.0;
            group.bbox.y -= offset.1;
        }

        let bbox = drawing_bbox(&nodes, &edges, &groups);

        LayoutGraph {
            nodes,
            edges,
            groups,
            bbox,
        }
    }
}

/// The parent of each group, read from the members that name another group.
///
/// A member that would close a cycle is skipped: the kind door rejects cycles,
/// and the engine still cannot hang or panic if one arrives anyway.
fn group_parents(graph: &IrGraph) -> Vec<(&str, &str)> {
    let node_ids: BTreeSet<&str> = graph.nodes.iter().map(|node| node.id.as_str()).collect();
    let group_ids: BTreeSet<&str> = graph.groups.iter().map(|group| group.id.as_str()).collect();
    let mut owner: BTreeMap<&str, &str> = BTreeMap::new();
    let mut parents: Vec<(&str, &str)> = Vec::new();

    for group in &graph.groups {
        for member in &group.members {
            let member = member.as_str();
            let is_group = group_ids.contains(member);
            // An unknown member is the door's business; the engine skips it.
            if !is_group && !node_ids.contains(member) {
                continue;
            }
            // One parent per member: the first group that claims it wins.
            if owner.contains_key(member) {
                continue;
            }
            if is_group
                && (member == group.id.as_str() || reaches(&owner, group.id.as_str(), member))
            {
                continue;
            }
            owner.insert(member, group.id.as_str());
            parents.push((group.id.as_str(), member));
        }
    }

    parents
}

/// How many groups contain each group (PRD §7 RF15): `0` is an outermost box.
fn group_depths(graph: &IrGraph) -> BTreeMap<&str, usize> {
    let parent_of: BTreeMap<&str, &str> = group_parents(graph)
        .into_iter()
        .map(|(parent, member)| (member, parent))
        .collect();

    let mut depths: BTreeMap<&str, usize> = BTreeMap::new();
    for group in &graph.groups {
        let mut depth = 0;
        let mut cursor = parent_of.get(group.id.as_str()).copied();
        while let Some(current) = cursor {
            depth += 1;
            if depth > graph.groups.len() {
                break;
            }
            cursor = parent_of.get(current).copied();
        }
        depths.insert(group.id.as_str(), depth);
    }

    depths
}

/// Whether walking up the parents of `start` arrives at `target`.
fn reaches(parents: &BTreeMap<&str, &str>, start: &str, target: &str) -> bool {
    let mut cursor = Some(start);
    let mut steps = 0;
    while let Some(current) = cursor {
        if current == target {
            return true;
        }
        if steps > parents.len() {
            return true;
        }
        cursor = parents.get(current).copied();
        steps += 1;
    }
    false
}

/// An orthogonal route between two rows, when the tables sit side by side.
///
/// The relation leaves the source horizontally, turns vertically at the
/// middle of the gap and arrives at the target row horizontally: every
/// segment is horizontal or vertical, which is what makes a schema easy to
/// follow. A diagonal segment leaves the reader guessing which row it
/// belongs to.
///
/// Returns `None` when the boxes are stacked or the edge has no ports: there
/// the routed polyline of the engine is the one that applies.
fn orthogonal_route(
    nodes: &BTreeMap<String, NodeBox>,
    edge: &gx_core::ir::IrEdge,
) -> Option<Vec<Point>> {
    let (from_row, to_row) = (edge.from_row?, edge.to_row?);
    let from = nodes.get(&edge.from)?;
    let to = nodes.get(&edge.to)?;

    let from_y = row_center(from, from_row);
    let to_y = row_center(to, to_row);

    // Side by side: the target starts after the source ends, or the reverse.
    let right_to_left = from.center.x + from.size.w / 2.0 <= to.center.x - to.size.w / 2.0;
    let left_to_right = to.center.x + to.size.w / 2.0 <= from.center.x - from.size.w / 2.0;
    if !right_to_left && !left_to_right {
        return None;
    }

    // A Z between two tables with a third one in the middle would cross that
    // box, and a relation that goes through a table is unreadable. There the
    // route goes **around** the tables in between, still with right angles:
    // the whole reason this function exists is that a diagonal leaves the
    // reader guessing which row it belongs to.
    let (low, high) = if from.center.x < to.center.x {
        (from.center.x, to.center.x)
    } else {
        (to.center.x, from.center.x)
    };
    let blocked = nodes.iter().any(|(id, other)| {
        id != &edge.from && id != &edge.to && other.center.x > low && other.center.x < high
    });

    let (start_x, end_x) = if right_to_left {
        (
            from.center.x + from.size.w / 2.0,
            to.center.x - to.size.w / 2.0,
        )
    } else {
        (
            from.center.x - from.size.w / 2.0,
            to.center.x + to.size.w / 2.0,
        )
    };

    // The rows are at the same height: a single horizontal segment.
    if (from_y - to_y).abs() < f64::EPSILON {
        return Some(vec![Point::new(start_x, from_y), Point::new(end_x, to_y)]);
    }

    if blocked {
        return detour_route(nodes, from, to, from_row, to_row, right_to_left);
    }

    // The Z: out, turn at the middle of the gap, and in.
    let middle_x = (start_x + end_x) / 2.0;
    Some(vec![
        Point::new(start_x, from_y),
        Point::new(middle_x, from_y),
        Point::new(middle_x, to_y),
        Point::new(end_x, to_y),
    ])
}

/// The route that goes around the tables between two ranks.
///
/// It leaves the source horizontally into the gap, runs along it to a band
/// above or below every table (whichever is shorter), crosses there, and
/// comes back down into the target row. Every segment is horizontal or
/// vertical and none of them touches a box, which is what a schema needs to
/// stay readable.
fn detour_route(
    nodes: &BTreeMap<String, NodeBox>,
    from: &NodeBox,
    to: &NodeBox,
    from_row: u32,
    to_row: u32,
    right_to_left: bool,
) -> Option<Vec<Point>> {
    let travel = if right_to_left { 1.0 } else { -1.0 };
    let from_y = row_center(from, from_row);
    let to_y = row_center(to, to_row);
    let start_x = from.center.x + travel * from.size.w / 2.0;
    let end_x = to.center.x - travel * to.size.w / 2.0;

    // The channel is measured on the rank, not on the box: a sibling table
    // can be wider than the one the relation leaves from.
    let source_rank = rank_extreme_x(nodes, from.center.x, travel)?;
    let target_rank = rank_extreme_x(nodes, to.center.x, -travel)?;
    let channel_out = source_rank + travel * DETOUR_CLEARANCE;
    let channel_in = target_rank - travel * DETOUR_CLEARANCE;

    let top = nodes
        .values()
        .map(|placed| placed.center.y - placed.size.h / 2.0)
        .fold(f64::INFINITY, f64::min)
        - DETOUR_CLEARANCE;
    let bottom = nodes
        .values()
        .map(|placed| placed.center.y + placed.size.h / 2.0)
        .fold(f64::NEG_INFINITY, f64::max)
        + DETOUR_CLEARANCE;

    let over = (from_y - top).abs() + (to_y - top).abs();
    let under = (bottom - from_y).abs() + (bottom - to_y).abs();
    let band_y = if over <= under { top } else { bottom };

    let points = vec![
        Point::new(start_x, from_y),
        Point::new(channel_out, from_y),
        Point::new(channel_out, band_y),
        Point::new(channel_in, band_y),
        Point::new(channel_in, to_y),
        Point::new(end_x, to_y),
    ];
    Some(without_repeats(points))
}

/// The extreme edge of the rank that owns `x`, in the direction of `sign`:
/// the right edge of the widest box when `sign` is positive, the left edge of
/// the leftmost one when it is negative.
fn rank_extreme_x(nodes: &BTreeMap<String, NodeBox>, x: f64, sign: f64) -> Option<f64> {
    let same_rank = |placed: &NodeBox| (placed.center.x - x).abs() < 0.5;
    let edges = nodes
        .values()
        .filter(|placed| same_rank(placed))
        .map(|placed| placed.center.x + sign * placed.size.w / 2.0);
    if sign > 0.0 {
        edges.fold(None, |acc: Option<f64>, edge| {
            Some(acc.map_or(edge, |acc| acc.max(edge)))
        })
    } else {
        edges.fold(None, |acc: Option<f64>, edge| {
            Some(acc.map_or(edge, |acc| acc.min(edge)))
        })
    }
}

/// Drops the turns of zero length: a duplicated point is noise in the path
/// data and in the golden files.
fn without_repeats(points: Vec<Point>) -> Vec<Point> {
    let mut kept: Vec<Point> = Vec::with_capacity(points.len());
    for point in points {
        let repeat = kept.last().is_some_and(|last: &Point| {
            (last.x - point.x).abs() < 0.001 && (last.y - point.y).abs() < 0.001
        });
        if !repeat {
            kept.push(point);
        }
    }
    kept
}

/// The vertical center of a row inside a node, in the same coordinates the
/// renderer uses for its detail rows.
fn row_center(placed: &NodeBox, row: u32) -> f64 {
    let top = placed.center.y - placed.size.h / 2.0;
    let line = TOKENS.text_size * TOKENS.line_height;
    top + TOKENS.node_padding + line + line * (f64::from(row) + 0.5)
}

/// The bounding box of the placed nodes, in plan units.
fn bounds_of(nodes: &BTreeMap<String, NodeBox>) -> Rect {
    nodes
        .values()
        .fold(None, |acc: Option<Rect>, placed| {
            let rect = Rect::new(
                placed.center.x - placed.size.w / 2.0,
                placed.center.y - placed.size.h / 2.0,
                placed.size.w,
                placed.size.h,
            );
            Some(match acc {
                Some(current) => current.union(&rect),
                None => rect,
            })
        })
        .unwrap_or_default()
}

/// The smallest translation that brings the drawing to the origin.
fn drawing_offset(
    nodes: &BTreeMap<String, NodeBox>,
    edges: &[LayoutEdge],
    groups: &[LayoutGroup],
) -> (f64, f64) {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;

    for placed in nodes.values() {
        min_x = min_x.min(placed.center.x - placed.size.w / 2.0);
        min_y = min_y.min(placed.center.y - placed.size.h / 2.0);
    }
    for edge in edges {
        for point in &edge.points {
            min_x = min_x.min(point.x);
            min_y = min_y.min(point.y);
        }
    }
    for group in groups {
        min_x = min_x.min(group.bbox.x);
        min_y = min_y.min(group.bbox.y);
    }

    if !min_x.is_finite() || !min_y.is_finite() {
        return (0.0, 0.0);
    }
    (min_x, min_y)
}

/// The bounding box of the drawing.
fn drawing_bbox(
    nodes: &BTreeMap<String, NodeBox>,
    edges: &[LayoutEdge],
    groups: &[LayoutGroup],
) -> Rect {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    let mut include = |x: f64, y: f64| {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    };

    for placed in nodes.values() {
        include(
            placed.center.x - placed.size.w / 2.0,
            placed.center.y - placed.size.h / 2.0,
        );
        include(
            placed.center.x + placed.size.w / 2.0,
            placed.center.y + placed.size.h / 2.0,
        );
    }
    for edge in edges {
        for point in &edge.points {
            include(point.x, point.y);
        }
    }
    for group in groups {
        include(group.bbox.x, group.bbox.y);
        include(group.bbox.max_x(), group.bbox.max_y());
    }

    if !min_x.is_finite() {
        return Rect::default();
    }
    Rect::new(min_x, min_y, max_x - min_x, max_y - min_y)
}

/// Separation between two levels of a tree, in plan units.
const TREE_LEVEL_SEPARATION: f64 = 60.0;
/// Separation between two sibling subtrees of a tree, in plan units.
const TREE_SIBLING_SEPARATION: f64 = 40.0;

/// The hierarchical layout of a tree (PRD §6: kinds `tree` y `mindmap`).
///
/// A tree is not a layered graph: the root sits on top, its children hang
/// below and every subtree owns its own horizontal band, so no two subtrees
/// overlap. The order of the children is the order of the document, which is
/// what makes the drawing deterministic without a cache.
pub struct TreeEngine;

impl TreeEngine {
    pub fn new() -> Self {
        Self
    }

    /// The width a subtree needs, in plan units.
    fn subtree_width(
        node: &gx_core::ir::IrNode,
        children: &BTreeMap<&str, Vec<&gx_core::ir::IrNode>>,
        separation: f64,
    ) -> f64 {
        let kids = children.get(node.id.as_str()).cloned().unwrap_or_default();
        if kids.is_empty() {
            return node.size.w;
        }
        let mut total = 0.0;
        for (index, kid) in kids.iter().enumerate() {
            if index > 0 {
                total += separation;
            }
            total += Self::subtree_width(kid, children, separation);
        }
        total.max(node.size.w)
    }

    /// Places the subtree of `node` with its top-left corner at `(left, top)`.
    fn place(
        node: &gx_core::ir::IrNode,
        children: &BTreeMap<&str, Vec<&gx_core::ir::IrNode>>,
        left: f64,
        top: f64,
        separation: f64,
        level_separation: f64,
        placed: &mut BTreeMap<String, NodeBox>,
    ) {
        let width = Self::subtree_width(node, children, separation);
        placed.insert(
            node.id.clone(),
            NodeBox {
                center: Point::new(left + width / 2.0, top + node.size.h / 2.0),
                size: node.size,
            },
        );

        let kids = children.get(node.id.as_str()).cloned().unwrap_or_default();
        let mut cursor = left + (width - Self::children_width(&kids, children, separation)) / 2.0;
        let child_top = top + node.size.h + level_separation;
        for kid in kids {
            let kid_width = Self::subtree_width(kid, children, separation);
            Self::place(
                kid,
                children,
                cursor,
                child_top,
                separation,
                level_separation,
                placed,
            );
            cursor += kid_width + separation;
        }
    }

    /// The width the children of a node occupy together.
    fn children_width(
        kids: &[&gx_core::ir::IrNode],
        children: &BTreeMap<&str, Vec<&gx_core::ir::IrNode>>,
        separation: f64,
    ) -> f64 {
        if kids.is_empty() {
            return 0.0;
        }
        let mut total = 0.0;
        for (index, kid) in kids.iter().enumerate() {
            if index > 0 {
                total += separation;
            }
            total += Self::subtree_width(kid, children, separation);
        }
        total
    }
}

impl Default for TreeEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutEngine for TreeEngine {
    fn layout(&self, graph: &IrGraph, opts: &LayoutOpts) -> LayoutGraph {
        let scale = opts.spacing_scale;
        let separation = TREE_SIBLING_SEPARATION * scale;
        let level_separation = TREE_LEVEL_SEPARATION * scale;

        if graph.nodes.is_empty() {
            return LayoutGraph::empty();
        }

        // The children of every node, in the order of the document.
        let mut children: BTreeMap<&str, Vec<&gx_core::ir::IrNode>> = BTreeMap::new();
        let mut has_parent: BTreeSet<&str> = BTreeSet::new();
        for edge in &graph.edges {
            let Some(parent) = graph.nodes.iter().find(|node| node.id == edge.from) else {
                continue;
            };
            let Some(child) = graph.nodes.iter().find(|node| node.id == edge.to) else {
                continue;
            };
            children.entry(parent.id.as_str()).or_default().push(child);
            has_parent.insert(child.id.as_str());
        }

        // The root is the first node of the document that nobody points at;
        // if every node has a parent (a cycle), the first one is the root, so
        // the drawing still happens and stays deterministic.
        let root = graph
            .nodes
            .iter()
            .find(|node| !has_parent.contains(node.id.as_str()))
            .or_else(|| graph.nodes.first())
            .expect("the graph has nodes");

        let mut placed: BTreeMap<String, NodeBox> = BTreeMap::new();
        Self::place(
            root,
            &children,
            0.0,
            0.0,
            separation,
            level_separation,
            &mut placed,
        );

        // A node the walk never reached (a second component) is placed in a
        // row of its own below the drawing: leaving it out would drop it from
        // the picture without saying so.
        let mut bottom = placed
            .values()
            .map(|box_| box_.center.y + box_.size.h / 2.0)
            .fold(0.0_f64, f64::max);
        for node in &graph.nodes {
            if placed.contains_key(&node.id) {
                continue;
            }
            bottom += level_separation;
            placed.insert(
                node.id.clone(),
                NodeBox {
                    center: Point::new(node.size.w / 2.0, bottom + node.size.h / 2.0),
                    size: node.size,
                },
            );
            bottom += node.size.h;
        }

        let edges: Vec<LayoutEdge> = graph
            .edges
            .iter()
            .filter_map(|edge| {
                let from = placed.get(&edge.from)?;
                let to = placed.get(&edge.to)?;
                Some(LayoutEdge {
                    from: edge.from.clone(),
                    to: edge.to.clone(),
                    points: vec![
                        Point::new(from.center.x, from.center.y + from.size.h / 2.0),
                        Point::new(to.center.x, to.center.y - to.size.h / 2.0),
                    ],
                    label_at: edge.label.as_ref().map(|_| {
                        Point::new(
                            (from.center.x + to.center.x) / 2.0,
                            (from.center.y + to.center.y) / 2.0,
                        )
                    }),
                })
            })
            .collect();

        let bbox = placed.values().fold(None, |acc: Option<Rect>, box_| {
            let rect = Rect::new(
                box_.center.x - box_.size.w / 2.0,
                box_.center.y - box_.size.h / 2.0,
                box_.size.w,
                box_.size.h,
            );
            Some(match acc {
                Some(current) => current.union(&rect),
                None => rect,
            })
        });
        let bbox = bbox.unwrap_or_default();

        LayoutGraph {
            nodes: placed,
            edges,
            groups: Vec::new(),
            bbox,
        }
    }
}

/// The engine of every kind, one per layout hint (PRD §6, O7).
///
/// The kinds do not choose an engine: their `layout_hint` does. This is the
/// only place that knows which engine answers which hint, so adding a motor
/// does not touch a single kind.
pub struct Layouts {
    layered: DagreEngine,
    tree: TreeEngine,
    lanes: LanesEngine,
}

impl Layouts {
    /// Builds every engine around the single metrics source (PRD §8.4.1).
    pub fn new(metrics: Arc<dyn TextMetrics>) -> Self {
        Self {
            layered: DagreEngine::new(metrics.clone()),
            tree: TreeEngine::new(),
            lanes: LanesEngine::new(metrics),
        }
    }
}

impl LayoutEngine for Layouts {
    fn layout(&self, graph: &IrGraph, opts: &LayoutOpts) -> LayoutGraph {
        match graph.layout_hint {
            gx_core::ir::LayoutHint::Tree => self.tree.layout(graph, opts),
            gx_core::ir::LayoutHint::Lanes => self.lanes.layout(graph, opts),
            _ => self.layered.layout(graph, opts),
        }
    }

    fn layout_timeline(&self, timeline: &IrTimeline, opts: &LayoutOpts) -> LayoutGraph {
        self.lanes.layout_timeline(timeline, opts)
    }
}

/// Height of a lane band, in plan units.
const LANE_HEIGHT: f64 = 110.0;
/// Width of one unit of time in a timeline, in plan units.
const TIME_STEP: f64 = 150.0;
/// Padding inside a lane, in plan units.
const LANE_PADDING: f64 = 16.0;
/// Height of the header row of a sequence diagram, in plan units.
const ACTOR_ROW_HEIGHT: f64 = 80.0;
/// Vertical distance between two messages, in plan units.
const MESSAGE_STEP: f64 = 90.0;
/// Horizontal air between two actors, in plan units.
const ACTOR_GAP: f64 = 70.0;
/// Narrowest actor box, in plan units.
const MIN_ACTOR_WIDTH: f64 = 90.0;
/// How far a self message sticks out, in plan units.
const LOOP_WIDTH: f64 = 70.0;

/// The layout of a timeline: lanes as rows, events as columns (PRD §6).
///
/// The horizontal axis is time: the `order` of an event decides its column,
/// not its position in the document, so a document that arrives unsorted is
/// still drawn in the order the timeline means. The events measure their own
/// label with the single metrics source, the same one the rest of the engine
/// uses.
pub struct LanesEngine {
    metrics: Arc<dyn TextMetrics>,
}

impl LanesEngine {
    pub fn new(metrics: Arc<dyn TextMetrics>) -> Self {
        Self { metrics }
    }

    /// The size of the box of an event: its label plus padding, and the width
    /// of the span it covers when it has one.
    fn event_size(&self, event: &IrEvent, step: f64, padding: f64) -> Size {
        let label = self
            .metrics
            .measure(&event.label, &TextStyle::regular(TOKENS.text_size));
        let columns = match &event.span {
            // A span is a stretch of time: its width in columns is the
            // distance between its ends, and a point in time is one column.
            Some(span) if span.end > span.start => (span.end - span.start).max(1.0),
            _ => 1.0,
        };
        let width = (label.w + 2.0 * padding).max(columns * step - 2.0 * padding);
        Size::new(width, label.h + 2.0 * padding)
    }

    /// The row of a lane, or the last one for an event without a lane.
    fn lane_index(lanes: &[IrLane], lane: &str) -> usize {
        lanes
            .iter()
            .position(|candidate| candidate.id == lane)
            .unwrap_or_else(|| lanes.len().saturating_sub(1))
    }
}

impl LayoutEngine for LanesEngine {
    /// The layout of a sequence diagram (PRD §6): the actors sit in a header
    /// row, one per column, and every message is a horizontal arrow that goes
    /// down the page in the order of the document.
    ///
    /// Es el mismo eje de tiempo que el de una timeline, con los actores en
    /// lugar de los carriles: por eso lo resuelve el mismo motor y el mismo
    /// despacho.
    fn layout(&self, graph: &IrGraph, opts: &LayoutOpts) -> LayoutGraph {
        let scale = opts.spacing_scale;
        let step = MESSAGE_STEP * scale;
        let header = ACTOR_ROW_HEIGHT * scale;

        if graph.nodes.is_empty() {
            return LayoutGraph::empty();
        }

        // The actors, in document order: they are the columns.
        let mut nodes: BTreeMap<String, NodeBox> = BTreeMap::new();
        let mut column_of: BTreeMap<&str, (f64, f64)> = BTreeMap::new();
        let mut cursor = 0.0;
        for actor in &graph.nodes {
            let label = self
                .metrics
                .measure(&actor.label, &TextStyle::semibold(TOKENS.text_size));
            let size = Size::new(
                (label.w + 2.0 * LANE_PADDING).max(MIN_ACTOR_WIDTH),
                label.h + 2.0 * LANE_PADDING,
            );
            let center_x = cursor + size.w / 2.0;
            nodes.insert(
                actor.id.clone(),
                NodeBox {
                    center: Point::new(center_x, header / 2.0),
                    size,
                },
            );
            column_of.insert(actor.id.as_str(), (center_x, size.w));
            cursor += size.w + ACTOR_GAP * scale;
        }

        // The messages: one row each, in the order of the document, as a
        // horizontal arrow between the two columns.
        let mut edges: Vec<LayoutEdge> = Vec::new();
        let mut row = 1.0;
        for edge in &graph.edges {
            let (Some(from), Some(to)) = (
                column_of.get(edge.from.as_str()),
                column_of.get(edge.to.as_str()),
            ) else {
                continue;
            };
            let y = header + row * step;
            let points = if edge.from == edge.to {
                // Un mensaje a sí mismo: un lazo que **sale y vuelve a la
                // línea de vida del actor**, no a su borde. Si arrancara en el
                // borde, el lazo quedaría flotando al costado de la vertical y
                // no se leería como algo que el actor se dice a sí mismo.
                let x = from.0;
                vec![
                    Point::new(x, y),
                    Point::new(x + LOOP_WIDTH, y),
                    Point::new(x + LOOP_WIDTH, y + step / 2.0),
                    Point::new(x, y + step / 2.0),
                ]
            } else {
                vec![Point::new(from.0, y), Point::new(to.0, y)]
            };
            edges.push(LayoutEdge {
                from: edge.from.clone(),
                to: edge.to.clone(),
                points,
                label_at: edge.label.as_ref().map(|_| {
                    if edge.from == edge.to {
                        // El nombre del lazo va sobre su tramo de arriba y
                        // centrado en el lazo, no encima de la línea de vida.
                        Point::new(from.0 + LOOP_WIDTH / 2.0, y - step * 0.15)
                    } else {
                        Point::new((from.0 + to.0) / 2.0, y - step * 0.2)
                    }
                }),
            });
            row += 1.0;
        }

        let bbox = bounds_of(&nodes);
        let bbox = Rect::new(
            bbox.x.min(0.0),
            bbox.y.min(0.0),
            bbox.w.max(cursor).max(1.0),
            bbox.h.max(header + row * step),
        );

        LayoutGraph {
            nodes,
            edges,
            groups: Vec::new(),
            bbox,
        }
    }

    fn layout_timeline(&self, timeline: &IrTimeline, opts: &LayoutOpts) -> LayoutGraph {
        let scale = opts.spacing_scale;
        let lane_height = LANE_HEIGHT * scale;
        let step = TIME_STEP * scale;
        let padding = LANE_PADDING;

        // The widest lane label decides the left gutter, so the label of a
        // lane never rides on top of its first event.
        let gutter = timeline
            .lanes
            .iter()
            .map(|lane| {
                let text = lane.label.clone().unwrap_or_else(|| lane.id.clone());
                self.metrics
                    .measure(&text, &TextStyle::semibold(TOKENS.text_size))
                    .w
                    + 3.0 * padding
            })
            .fold(TIME_STEP * scale, f64::max);

        let mut nodes: BTreeMap<String, NodeBox> = BTreeMap::new();
        let mut events: Vec<&IrEvent> = timeline.events.iter().collect();
        events.sort_by_key(|event| (event.order, event.id.clone()));

        let mut last_right = gutter;
        for event in events {
            let row = Self::lane_index(&timeline.lanes, &event.lane);
            let size = self.event_size(event, step, padding);
            let left = gutter + (event.order.max(0) as f64) * step;
            let right = left + size.w;
            last_right = last_right.max(right);
            nodes.insert(
                event.id.clone(),
                NodeBox {
                    center: Point::new(
                        left + size.w / 2.0,
                        row as f64 * lane_height + lane_height / 2.0,
                    ),
                    size,
                },
            );
        }

        let width = last_right + padding;
        let lane_count = timeline.lanes.len().max(1);
        let groups: Vec<LayoutGroup> = timeline
            .lanes
            .iter()
            .enumerate()
            .map(|(index, lane)| LayoutGroup {
                id: lane.id.clone(),
                label: lane.label.clone(),
                bbox: Rect::new(0.0, index as f64 * lane_height, width, lane_height),
                depth: 0,
            })
            .collect();
        let bbox = Rect::new(0.0, 0.0, width, lane_count as f64 * lane_height);

        LayoutGraph {
            nodes,
            edges: Vec::new(),
            groups,
            bbox,
        }
    }
}
