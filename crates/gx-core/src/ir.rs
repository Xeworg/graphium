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

/// The role of a row, when the node has rows (PRD §6.1, kind `er`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowKey {
    /// Not a key.
    None,
    /// Primary key.
    Pk,
    /// Foreign key.
    Fk,
    /// Both: a foreign key that is also part of the primary key.
    PkFk,
}

/// One row inside a node: a column of a table, an attribute of a class.
///
/// The IR does not grow per kind: a row is a generic detail line, and a kind
/// that has none simply leaves the list empty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IrRow {
    pub text: String,
    /// The right hand side of the row: the type, the modifier.
    pub note: Option<String>,
    pub key: RowKey,
    /// Whether the row admits `NULL` (PRD §6.1, kind `er`).
    pub nullable: bool,
}

impl IrRow {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            note: None,
            key: RowKey::None,
            nullable: false,
        }
    }
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
    /// The detail rows of the node, drawn under its label.
    pub rows: Vec<IrRow>,
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
            rows: Vec::new(),
            size: Size::default(),
        }
    }

    /// Adds a detail row.
    pub fn with_row(mut self, row: IrRow) -> Self {
        self.rows.push(row);
        self
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
    /// Row of the source node the edge leaves from, when it anchors to one.
    pub from_row: Option<u32>,
    /// Row of the target node the edge arrives at, when it anchors to one.
    pub to_row: Option<u32>,
    /// Cardinality drawn at the target end, when the kind has one.
    pub cardinality: Option<Cardinality>,
}

/// How many rows of each side take part in a relation (PRD §6.1, kind `er`).
///
/// The notation is the crow's foot: each end says the minimum and the maximum
/// number of rows on the other side, which is what makes a schema readable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Cardinality {
    /// Minimum at the source end: `0` (circle) or `1` (a stroke).
    pub from_min: CardinalityBound,
    /// Maximum at the source end.
    pub from_max: CardinalityMax,
    /// Minimum at the target end.
    pub to_min: CardinalityBound,
    /// Maximum at the target end.
    pub to_max: CardinalityMax,
}

/// Whether a side is mandatory or optional.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardinalityBound {
    /// Zero rows are allowed: the circle of the crow's foot.
    Optional,
    /// At least one row: the stroke.
    Required,
}

/// Whether a side is single or multiple.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardinalityMax {
    /// At most one row.
    One,
    /// Many rows: the crow's foot.
    Many,
}

impl Cardinality {
    /// Builds a cardinality from its short form (`1:1`, `1:N`, `N:1`, `N:M`).
    ///
    /// `to_nullable` turns the target end into an optional one: a foreign key
    /// that admits `NULL` allows zero rows on that side.
    pub fn from_short(short: &str, to_nullable: bool) -> Option<Self> {
        let (left, right) = short.split_once(':')?;
        let (from_max, to_max) = match (parse_side(left)?, parse_side(right)?) {
            (CardinalityMax::One, CardinalityMax::One) => {
                (CardinalityMax::One, CardinalityMax::One)
            }
            (CardinalityMax::One, CardinalityMax::Many) => {
                (CardinalityMax::One, CardinalityMax::Many)
            }
            (CardinalityMax::Many, CardinalityMax::One) => {
                (CardinalityMax::Many, CardinalityMax::One)
            }
            (CardinalityMax::Many, CardinalityMax::Many) => {
                (CardinalityMax::Many, CardinalityMax::Many)
            }
        };
        Some(Self {
            // The source end of a relation is the key that is referenced, so
            // it always has a row: at least one.
            from_min: CardinalityBound::Required,
            from_max,
            to_min: if to_nullable {
                CardinalityBound::Optional
            } else {
                CardinalityBound::Required
            },
            to_max,
        })
    }

    /// The short form the agent writes: `1:1`, `1:N`, `N:1`, `N:M`.
    pub const SHORT_FORMS: [&'static str; 4] = ["1:1", "1:N", "N:1", "N:M"];
}

/// Parses one side of a short form.
fn parse_side(text: &str) -> Option<CardinalityMax> {
    match text.trim().to_ascii_uppercase().as_str() {
        "1" => Some(CardinalityMax::One),
        "N" => Some(CardinalityMax::Many),
        _ => None,
    }
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
            from_row: None,
            to_row: None,
            cardinality: None,
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
