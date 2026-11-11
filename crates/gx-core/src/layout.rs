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
    /// The ids the group owns, in the order of the document: nodes or other
    /// groups for a container, the events of the lane for a lane
    /// (`IrGroup.members` and `IrLane`, `f11-elementos.md` T1). The layout used
    /// to drop them after computing the box; the scene ships them so the
    /// painter knows what belongs to what.
    #[serde(default)]
    pub members: Vec<String>,
    /// The left gutter the layout reserved for the names of the lanes, in plan
    /// units (`f11-elementos.md` T1). `None` for a container, which has no
    /// gutter: the space is a decision of the lanes layout, never of the JS
    /// (S4).
    #[serde(default)]
    pub gutter: Option<f64>,
}

/// A time axis under the last lane of a timeline (`f22-dibujo.md`, S2): the
/// line, its x span and one tick per known column. The layout computes it
/// and the painter spells it; `None` on [`LayoutGraph.axis`] draws nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutAxis {
    /// The y of the axis line, below the last lane band.
    pub y: f64,
    /// The x where the line starts and ends: the center of the first and of
    /// the last known column.
    pub x1: f64,
    pub x2: f64,
    /// One tick per known column, in order: the center x of the column and
    /// the column value it names.
    pub ticks: Vec<(f64, i64)>,
}

/// The orthogonal route of a connector between two placed nodes
/// (`f22-dibujo.md`, S8): a forward flow leaves the source's right edge and
/// enters the target's left edge with one bend in the middle; a backward
/// flow mirrors it, so the line never crosses a box; a connector in the same
/// column goes straight through the vertical.
pub fn connector_route(from: &NodeBox, to: &NodeBox) -> Vec<Point> {
    let source = Rect::new(
        from.center.x - from.size.w / 2.0,
        from.center.y - from.size.h / 2.0,
        from.size.w,
        from.size.h,
    );
    let target = Rect::new(
        to.center.x - to.size.w / 2.0,
        to.center.y - to.size.h / 2.0,
        to.size.w,
        to.size.h,
    );
    let dx = to.center.x - from.center.x;
    if dx.abs() < 0.01 {
        // Same column: the vertical carries the flow, top to bottom.
        let (exit, entry) = if to.center.y >= from.center.y {
            (
                Point::new(from.center.x, source.max_y()),
                Point::new(to.center.x, target.min_y()),
            )
        } else {
            (
                Point::new(from.center.x, source.min_y()),
                Point::new(to.center.x, target.max_y()),
            )
        };
        return vec![exit, entry];
    }
    let (exit, entry, mid_x) = if dx > 0.0 {
        (
            Point::new(source.max_x(), source.center().y),
            Point::new(target.min_x(), target.center().y),
            (source.max_x() + target.min_x()) / 2.0,
        )
    } else {
        (
            Point::new(source.min_x(), source.center().y),
            Point::new(target.max_x(), target.center().y),
            (source.min_x() + target.max_x()) / 2.0,
        )
    };
    if (exit.y - entry.y).abs() < 0.01 {
        // Same row: a straight line needs no bend.
        return vec![exit, entry];
    }
    vec![
        exit,
        Point::new(mid_x, exit.y),
        Point::new(mid_x, entry.y),
        entry,
    ]
}

/// An activation bar of a sequence (`f22-dibujo.md`, S5): the thin rectangle
/// over an actor's lifeline while the actor holds the control. The layout
/// computes it and the painters spell it; an empty [`LayoutGraph
/// ::activations`] draws nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutActivation {
    /// The x of the lifeline: the center of the actor.
    pub x: f64,
    /// Where the bar starts and ends, bracketing the rows it spans.
    pub y1: f64,
    pub y2: f64,
}

/// The result of a layout run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutGraph {
    /// Nodes by id: ordered, never a `HashMap` (PRD §8.5).
    pub nodes: BTreeMap<String, NodeBox>,
    pub edges: Vec<LayoutEdge>,
    pub groups: Vec<LayoutGroup>,
    /// The time axis of a timeline layout (`f22-dibujo.md`, S2): `None` for
    /// every graph layout and for the sequence lanes, which have no time.
    pub axis: Option<LayoutAxis>,
    /// The activation bars of a sequence (`f22-dibujo.md`, S5), in actor
    /// order: empty for every other layout.
    pub activations: Vec<LayoutActivation>,
    pub bbox: Rect,
}

impl LayoutGraph {
    pub fn empty() -> Self {
        Self {
            nodes: BTreeMap::new(),
            edges: Vec::new(),
            groups: Vec::new(),
            axis: None,
            activations: Vec::new(),
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
            activations: Vec::new(),
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
                    members: timeline
                        .events
                        .iter()
                        .filter(|event| event.lane == lane.id)
                        .map(|event| event.id.clone())
                        .collect(),
                    // No gutter here: this default stacks the lanes from the
                    // left edge, so it reserves no space for the names.
                    gutter: Some(0.0),
                })
                .collect(),
            // The default timeline layout has no axis yet: the lanes engine
            // is the one that computes it (`f22-dibujo.md`, S2).
            axis: None,
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
