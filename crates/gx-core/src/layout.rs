//! Layout contract (PRD §8.4).
//!
//! `gx-layout` implements [`LayoutEngine`]; `gx-render-svg` consumes the
//! resulting [`LayoutGraph`] without recomputing anything.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::geom::{Point, Rect, Size};
use crate::ir::IrGraph;

/// Position and size of a node in plan units.
///
/// The origin is the **center** of the node box (F0 decision L9); the graph
/// is normalized so the bounding box starts at `(0, 0)` before rendering.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NodeBox {
    pub center: Point,
    pub size: Size,
}

/// A routed edge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutEdge {
    pub from: String,
    pub to: String,
    /// Polyline of the route, in plan units.
    pub points: Vec<Point>,
    /// Where the edge label sits.
    pub label_at: Option<Point>,
}

/// A cluster placed by the layout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutGroup {
    pub id: String,
    pub label: Option<String>,
    pub bbox: Rect,
}

/// The result of a layout run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutGraph {
    /// Nodes by id: ordered, never a `HashMap` (PRD §8.5).
    pub nodes: BTreeMap<String, NodeBox>,
    pub edges: Vec<LayoutEdge>,
    pub groups: Vec<LayoutGroup>,
    pub bbox: Rect,
}

impl LayoutGraph {
    pub fn empty() -> Self {
        Self {
            nodes: BTreeMap::new(),
            edges: Vec::new(),
            groups: Vec::new(),
            bbox: Rect::default(),
        }
    }
}

/// A layout engine behind a trait, so the open source dependency stays
/// isolated (PRD §8.4).
pub trait LayoutEngine: Send + Sync {
    fn layout(&self, graph: &IrGraph) -> LayoutGraph;
}
