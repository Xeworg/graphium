//! The intermediate representation (PRD §8.1-8.3).
//!
//! The IR does not grow per kind: 13 kinds share [`IrGraph`], 6 share
//! [`IrTimeline`] and the specialized ones are a [`LayoutHint`] apart. Kind
//! specific data travels in `kind_data`, ordered and never rendered directly.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::geom::Size;

/// Layout strategy requested by a kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LayoutHint {
    Layered,
    Tree,
    Sankey,
    Git,
}

/// Direction of a layered graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum Direction {
    TB,
    BT,
    LR,
    RL,
}

impl Direction {
    /// The four directions a layered layout supports (PRD §8.4).
    pub const ALL: [Direction; 4] = [Direction::TB, Direction::BT, Direction::LR, Direction::RL];
}

/// Shape of a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    Box,
    Rounded,
    Cylinder,
    Diamond,
    Ellipse,
    Note,
}

/// Visual emphasis of a node or an edge (PRD §11.1, RF16).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Emphasis {
    Normal,
    Highlight,
    Muted,
}

/// Line style of a node border or an edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Style {
    Solid,
    Dashed,
}

/// A kind specific value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IrValue {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Null,
}

/// A node of the graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IrNode {
    pub id: String,
    pub label: String,
    pub shape: Shape,
    pub emphasis: Emphasis,
    pub style: Style,
    pub meta: BTreeMap<String, IrValue>,
    /// Filled by the engine with the single `TextMetrics` implementation
    /// before the layout runs (PRD §8.4.1).
    pub size: Size,
}

impl IrNode {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            shape: Shape::Rounded,
            emphasis: Emphasis::Normal,
            style: Style::Solid,
            meta: BTreeMap::new(),
            size: Size::default(),
        }
    }
}

/// An edge of the graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IrEdge {
    pub from: String,
    pub to: String,
    pub label: Option<String>,
    pub emphasis: Emphasis,
    pub style: Style,
    pub arrow: bool,
}

impl IrEdge {
    pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            label: None,
            emphasis: Emphasis::Normal,
            style: Style::Solid,
            arrow: true,
        }
    }
}

/// A cluster of nodes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IrGroup {
    pub id: String,
    pub label: Option<String>,
    pub members: Vec<String>,
}

/// A graph in the IR.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IrGraph {
    pub direction: Direction,
    pub layout_hint: LayoutHint,
    pub nodes: Vec<IrNode>,
    pub edges: Vec<IrEdge>,
    pub groups: Vec<IrGroup>,
}

/// A lane of a timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IrLane {
    pub id: String,
    pub label: Option<String>,
}

/// A time span, used by `gantt` and `timeline`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Span {
    pub start: f64,
    pub end: f64,
}

/// An event of a timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IrEvent {
    pub id: String,
    pub lane: String,
    pub label: String,
    pub order: i64,
    pub span: Option<Span>,
    pub kind_data: BTreeMap<String, IrValue>,
}

/// A timeline in the IR.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IrTimeline {
    pub lanes: Vec<IrLane>,
    pub events: Vec<IrEvent>,
}

/// The two IRs of Graphium.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ir {
    Graph(IrGraph),
    Timeline(IrTimeline),
}

impl Ir {
    pub fn graph(&self) -> Option<&IrGraph> {
        match self {
            Ir::Graph(graph) => Some(graph),
            Ir::Timeline(_) => None,
        }
    }

    pub fn timeline(&self) -> Option<&IrTimeline> {
        match self {
            Ir::Graph(_) => None,
            Ir::Timeline(timeline) => Some(timeline),
        }
    }
}
