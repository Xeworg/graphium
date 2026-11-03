//! `gx-layout` — the layout engines of Graphium (PRD §8.4).
//!
//! `dagre` stays isolated behind the [`LayoutEngine`] trait of `gx-core`: the
//! engine receives the text measurer as a parameter and never measures text
//! on its own (PRD §8.4.1). The crate depends on `gx-core` and nothing else
//! (PRD §9.1).

pub mod matrix;

use std::collections::BTreeMap;
use std::sync::Arc;

use dagre::graph::{Graph, GraphOptions};
use dagre::{layout, EdgeLabel, LayoutOptions, NodeLabel, RankDir, Ranker};
use gx_core::geom::{Point, Rect, Size};
use gx_core::ir::{Direction, IrGraph};
use gx_core::layout::{LayoutEdge, LayoutEngine, LayoutGraph, LayoutGroup, NodeBox};
use gx_core::text::{TextMetrics, TextStyle};
use gx_core::tokens::TOKENS;

/// Separation between nodes of the same rank, in plan units.
const NODE_SEPARATION: f64 = 60.0;
/// Separation between ranks, in plan units.
const RANK_SEPARATION: f64 = 80.0;
/// Separation between edges of the same rank.
const EDGE_SEPARATION: f64 = 20.0;
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

    /// The size of the box of a node: the label, padded, plus its rows.
    ///
    /// The rows are the columns of a table (PRD §6.1): the box has to be tall
    /// enough to hold them, or the drawing would overlap the text.
    fn node_size(&self, node: &gx_core::ir::IrNode) -> Size {
        let label = self
            .metrics
            .measure(&node.label, &TextStyle::semibold(TOKENS.text_size));
        let mut width = label.w;
        let mut height = label.h + 2.0 * TOKENS.node_padding;

        if !node.rows.is_empty() {
            // The badge of the key occupies its own column and the note is
            // drawn against the right edge: badge + name + note have to fit.
            let badge = self
                .metrics
                .measure("PK, FK", &TextStyle::regular(TOKENS.text_size));
            let gap = TOKENS.node_padding;
            height += TOKENS.border_width;
            for row in &node.rows {
                let name = self
                    .metrics
                    .measure(&row.text, &TextStyle::regular(TOKENS.text_size));
                let mut row_width = badge.w + gap + name.w;
                if let Some(note) = &row.note {
                    let measured_note = self
                        .metrics
                        .measure(note, &TextStyle::regular(TOKENS.text_size));
                    row_width += gap + measured_note.w;
                }
                width = width.max(row_width);
                height += name.h;
            }
            height += TOKENS.node_padding;
        }

        Size::new(width + 2.0 * TOKENS.node_padding, height)
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
    fn layout(&self, graph: &IrGraph) -> LayoutGraph {
        let mut dagre: Graph<NodeLabel, EdgeLabel> = Graph::with_options(GraphOptions {
            directed: true,
            multigraph: true,
            compound: true,
        });

        for node in &graph.nodes {
            let size = self.node_size(node);
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

        // Clusters: a compound parent node with the members below it.
        for group in &graph.groups {
            dagre.set_node(group.id.clone(), Some(NodeLabel::default()));
            for member in &group.members {
                dagre.set_parent(member, Some(group.id.as_str()));
            }
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

        let options = LayoutOptions {
            rankdir: rank_dir(graph.direction),
            nodesep: NODE_SEPARATION,
            ranksep: RANK_SEPARATION,
            edgesep: EDGE_SEPARATION,
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
            // The ports: a relation of a schema anchors to the row of the
            // key, not to the middle of the table (PRD §6.1).
            if let Some(row) = edge.to_row {
                if let (Some(point), Some(placed)) = (points.last_mut(), nodes.get(&edge.to)) {
                    point.y = row_center(placed, row);
                }
            }
            if let Some(row) = edge.from_row {
                if let (Some(point), Some(placed)) = (points.first_mut(), nodes.get(&edge.from)) {
                    point.y = row_center(placed, row);
                }
            }
            edges.push(LayoutEdge {
                from: edge.from.clone(),
                to: edge.to.clone(),
                points,
                label_at,
            });
        }

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

/// The vertical center of a row inside a node, in the same coordinates the
/// renderer uses for its detail rows.
fn row_center(placed: &NodeBox, row: u32) -> f64 {
    let top = placed.center.y - placed.size.h / 2.0;
    let line = TOKENS.text_size * TOKENS.line_height;
    top + TOKENS.node_padding + line + line * (f64::from(row) + 0.5)
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
