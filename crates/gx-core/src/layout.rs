//! Layout contract (PRD §8.4).
//!
//! `gx-layout` implements [`LayoutEngine`]; `gx-render-svg` consumes the
//! resulting [`LayoutGraph`] without recomputing anything.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::geom::{Point, Rect, Size};
use crate::ir::{IrGraph, IrTimeline};

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
    /// How many groups contain this one (PRD §7 RF15): `0` is an outermost
    /// box. The renderer draws the smaller depth first, so the outer box ends
    /// up behind the inner one.
    #[serde(default)]
    pub depth: usize,
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
    fn layout(&self, graph: &IrGraph, opts: &LayoutOpts) -> LayoutGraph;

    /// The layout of a timeline (PRD §6: six kinds use `lanes`).
    ///
    /// The default stacks the lanes as rows and puts the events in order, so
    /// an engine that only knows layered graphs still draws something honest
    /// instead of returning nothing. The lanes engine overrides it with the
    /// real one.
    fn layout_timeline(&self, timeline: &IrTimeline, opts: &LayoutOpts) -> LayoutGraph {
        let scale = opts.spacing_scale;
        let lane_height = 100.0 * scale;
        let event_gap = 40.0 * scale;
        let mut nodes: BTreeMap<String, NodeBox> = BTreeMap::new();
        let mut cursor = 0.0;
        for (index, lane) in timeline.lanes.iter().enumerate() {
            let y = index as f64 * lane_height + lane_height / 2.0;
            for event in timeline.events.iter().filter(|event| event.lane == lane.id) {
                let width = 120.0;
                let height = 40.0;
                nodes.insert(
                    event.id.clone(),
                    NodeBox {
                        center: Point::new(cursor + width / 2.0, y),
                        size: Size::new(width, height),
                    },
                );
                cursor += width + event_gap;
            }
        }
        let bbox = bounds_of(&nodes);
        LayoutGraph {
            nodes,
            edges: Vec::new(),
            groups: timeline
                .lanes
                .iter()
                .enumerate()
                .map(|(index, lane)| LayoutGroup {
                    id: lane.id.clone(),
                    label: lane.label.clone(),
                    bbox: Rect::new(
                        0.0,
                        index as f64 * lane_height,
                        bbox.max_x().max(1.0),
                        lane_height,
                    ),
                    depth: 0,
                })
                .collect(),
            bbox,
        }
    }
}

/// The bounding box of the placed nodes.
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

/// Options of one layout run (PRD §7.3).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutOpts {
    /// Multiplier of every separation between nodes. The degradation ladder
    /// of PRD §7.3 tightens the drawing with `0.7`.
    pub spacing_scale: f64,
}

impl Default for LayoutOpts {
    fn default() -> Self {
        Self { spacing_scale: 1.0 }
    }
}
