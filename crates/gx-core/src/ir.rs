//! The intermediate representation (PRD §8.1-8.3).
//!
//! The IR does not grow per kind: 13 kinds share [`IrGraph`], 6 share
//! [`IrTimeline`] and the specialized ones are a [`LayoutHint`] apart. Kind
//! specific data travels in `kind_data`, ordered and never rendered directly.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{ErrorCode, ValidationError};
use crate::geom::Size;

/// Layout strategy requested by a kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LayoutHint {
    Layered,
    Tree,
    /// Actors as columns and messages in time order (PRD §6: `sequence`).
    Lanes,
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
    /// The dot: a frozen-diameter circle whose label sits beside it, not
    /// inside (`f19-formas.md`, S8). The commit of a `gitgraph`.
    Circle,
    /// The filled dot: the same frozen-diameter envelope as [`Shape::Circle`]
    /// but drawn solid — the UML glyph of a state machine's `initial`
    /// (`f22-dibujo.md`, S3).
    Dot,
    /// The dot inside a ring: the same frozen-diameter envelope, an unfilled
    /// outer ring with a solid inner dot — the UML glyph of a state machine's
    /// `final` (`f22-dibujo.md`, S3). The inner dot's radius is half the
    /// outer one.
    RingDot,
}

impl Shape {
    /// Every shape, in declaration order: the one list of the wire words
    /// (`f19-formas.md`, S3). A new shape enters here and the compiler walks
    /// the rest of the way.
    pub const ALL: [Shape; 9] = [
        Shape::Box,
        Shape::Rounded,
        Shape::Cylinder,
        Shape::Diamond,
        Shape::Ellipse,
        Shape::Note,
        Shape::Circle,
        Shape::Dot,
        Shape::RingDot,
    ];

    /// The wire word of the shape: the one spelling every painter, the scene
    /// payload and the schema read (`f19-formas.md`, S3).
    pub const fn as_str(self) -> &'static str {
        match self {
            Shape::Box => "box",
            Shape::Rounded => "rounded",
            Shape::Cylinder => "cylinder",
            Shape::Diamond => "diamond",
            Shape::Ellipse => "ellipse",
            Shape::Note => "note",
            Shape::Circle => "circle",
            Shape::Dot => "dot",
            Shape::RingDot => "ring_dot",
        }
    }
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
    /// Whether a compartment rule draws above this row (`f22-dibujo.md`,
    /// S7): the class kind marks the first method, so the attributes and the
    /// methods read as the two compartments of UML. The header rule is the
    /// first compartment's on its own, so a first row never divides.
    #[serde(default)]
    pub divider: bool,
}

impl IrRow {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            note: None,
            key: RowKey::None,
            nullable: false,
            divider: false,
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
    /// The stereotype word the kind gives the node's role, drawn above its
    /// label («interface», «abstract») — `f22-dibujo.md`, S4. `None` draws
    /// nothing: a plain node has no stereotype.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stereotype: Option<String>,
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
            stereotype: None,
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
    /// The adornment of the target end, when the kind gives it one
    /// (`f26-adornos.md`, S2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<Adornment>,
    /// The adornment of the source end, when the kind gives it one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tail: Option<Adornment>,
    /// Row of the source node the edge leaves from, when it anchors to one.
    pub from_row: Option<u32>,
    /// Row of the target node the edge arrives at, when it anchors to one.
    pub to_row: Option<u32>,
    /// Cardinality drawn at the target end, when the kind has one.
    pub cardinality: Option<Cardinality>,
    /// Kind-specific scalar data, the same channel [`IrNode`] and
    /// [`IrEvent`] carry: ordered, never rendered directly (PRD §8.3). The
    /// `sequence` kind marks its activating messages here (`f22-dibujo.md`,
    /// S5, L7b).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub kind_data: BTreeMap<String, IrValue>,
}

/// The glyph of one end of an edge (`f26-adornos.md`, S5).
///
/// The closed vocabulary of this cut is what the catalogue's Etapa 1 asks for
/// and nothing more: a new glyph is a new shape in **both** painters plus a
/// golden, so it does not enter for free.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Adornment {
    /// The filled triangle of a plain arrow: the only glyph before this cut.
    Arrow,
    /// The open V of a dependency: two strokes, no fill.
    OpenArrow,
    /// The hollow triangle of a generalization, kept for the decision that
    /// swaps `extends` away from the filled arrow.
    HollowTriangle,
    /// The filled diamond of a composition.
    Diamond,
    /// The hollow diamond of an aggregation.
    HollowDiamond,
}

/// What one end of an edge actually draws (`f26-adornos.md`, S2).
///
/// Two notational systems meet at the same tip: a marker from [`Adornment`] and
/// the crow's foot of a [`Cardinality`]. This type is the one place that
/// resolves the collision, so a painter reads the answer instead of deciding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EndAdornment {
    /// A marker of the shared vocabulary, from `<defs>`.
    Marker {
        /// Which glyph.
        adornment: Adornment,
    },
    /// The crow's foot of a relation: the fork and the circle of its end.
    CrowFoot {
        /// Whether zero rows are allowed at this end.
        min: CardinalityBound,
        /// Whether many rows are allowed at this end.
        max: CardinalityMax,
    },
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
            head: Some(Adornment::Arrow),
            tail: None,
            from_row: None,
            to_row: None,
            cardinality: None,
            kind_data: BTreeMap::new(),
        }
    }

    /// What the target end of the edge draws.
    ///
    /// A relation with a cardinality gives the end to the crow's foot: the two
    /// notations would otherwise land on the same tip, which is why `er` used
    /// to cancel the arrow by hand (`f26-adornos.md`, S4).
    pub fn head_end(&self) -> Option<EndAdornment> {
        match self.cardinality {
            Some(cardinality) => Some(EndAdornment::CrowFoot {
                min: cardinality.to_min,
                max: cardinality.to_max,
            }),
            None => self
                .head
                .map(|adornment| EndAdornment::Marker { adornment }),
        }
    }

    /// What the source end of the edge draws, resolved the same way.
    pub fn tail_end(&self) -> Option<EndAdornment> {
        match self.cardinality {
            Some(cardinality) => Some(EndAdornment::CrowFoot {
                min: cardinality.from_min,
                max: cardinality.from_max,
            }),
            None => self
                .tail
                .map(|adornment| EndAdornment::Marker { adornment }),
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
    /// The shape the event's role gives it (`f22-dibujo.md`, S6): the kinds
    /// resolve it with their role table (`shape_of`), and a kind without
    /// roles leaves the default. `Rounded` is the event box of always.
    #[serde(default = "crate::ir::default_event_shape")]
    pub shape: Shape,
    /// The time span of a `gantt` or a `timeline` bar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    pub kind_data: BTreeMap<String, IrValue>,
    /// Task ids this event depends on (`gantt`, feature doc `f18-gantt.md`):
    /// what has to finish before this event starts. Empty for every other
    /// timeline kind — `IrValue` has no list variant, so the dependencies
    /// travel in their own field instead of `kind_data`.
    #[serde(default)]
    pub depends_on: Vec<String>,
}

/// A timeline in the IR.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IrTimeline {
    pub lanes: Vec<IrLane>,
    pub events: Vec<IrEvent>,
    /// The connectors of a swimlane (`f22-dibujo.md`, S8): the arrows between
    /// steps, across lanes, with the ids resolved. Empty for every other
    /// timeline kind — the gantt arrows are the `depends_on` of the events.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connections: Vec<IrConnection>,
}

/// A connector between two steps of a swimlane (`f22-dibujo.md`, S8): the
/// arrow the lanes engine routes from the step that flows to the one that
/// follows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IrConnection {
    /// Id of the step the arrow leaves from.
    pub from: String,
    /// Id of the step the arrow arrives at.
    pub to: String,
}

/// The shape an event draws when its kind has no role for it: the rounded
/// box of always (`f22-dibujo.md`, S6).
fn default_event_shape() -> Shape {
    Shape::Rounded
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

    /// Narrows a graph to the neighbourhood of one node (PRD §7.3, S21).
    ///
    /// A timeline has events, not nodes: there is no neighbourhood to take,
    /// so the answer is the same actionable error as an unknown id.
    pub fn extracted(&self, around: &str, depth: usize) -> Result<Ir, ValidationError> {
        match self {
            Ir::Graph(graph) => extract_neighbourhood(graph, around, depth).map(Ir::Graph),
            Ir::Timeline(_) => Err(extract_unknown_node(around)),
        }
    }
}

/// The cap on the nodes of an extracted subgraph (PRD §7.3, S21).
///
/// The hard limit of a kind is not the limit of a view: a diagram over the
/// top still leaves a neighbourhood small enough to draw.
pub const MAX_EXTRACT_NODES: usize = 300;

/// The error of an `opts.extract.around` that names no node.
fn extract_unknown_node(around: &str) -> ValidationError {
    ValidationError::new(
        "opts.extract.around",
        ErrorCode::UnknownRef,
        format!("no hay ningún nodo con el `id` '{around}' en este diagrama"),
        Some("usá el `id` de un nodo de este diagrama".to_string()),
    )
}

/// The subgraph reachable from `around` in at most `depth` hops (PRD §7.3).
///
/// The walk ignores the direction of the edges: one hop is one edge, whichever
/// end of it `around` sits on. The subgraph keeps the order of the parent, the
/// edges whose two ends survived, the groups with at least one surviving
/// member, and the `direction` and `layout_hint` of the parent.
pub fn extract_neighbourhood(
    graph: &IrGraph,
    around: &str,
    depth: usize,
) -> Result<IrGraph, ValidationError> {
    if !graph.nodes.iter().any(|node| node.id == around) {
        return Err(extract_unknown_node(around));
    }

    // Both ends of every edge, so the walk can go against an arrow.
    let mut adjacency: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for edge in &graph.edges {
        adjacency
            .entry(edge.from.as_str())
            .or_default()
            .push(edge.to.as_str());
        adjacency
            .entry(edge.to.as_str())
            .or_default()
            .push(edge.from.as_str());
    }

    let mut reached: BTreeSet<&str> = BTreeSet::new();
    reached.insert(around);
    let mut frontier: Vec<&str> = vec![around];
    for _ in 0..depth {
        let mut next: Vec<&str> = Vec::new();
        for id in frontier {
            for &neighbour in adjacency.get(id).into_iter().flatten() {
                if reached.insert(neighbour) {
                    next.push(neighbour);
                }
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }

    let nodes: Vec<IrNode> = graph
        .nodes
        .iter()
        .filter(|node| reached.contains(node.id.as_str()))
        .cloned()
        .collect();
    if nodes.len() > MAX_EXTRACT_NODES {
        return Err(ValidationError::new(
            "opts.extract",
            ErrorCode::TooLarge,
            format!(
                "{} nodos en el subgrafo superan el tope de {MAX_EXTRACT_NODES}",
                nodes.len()
            ),
            Some("reducí `depth`".to_string()),
        ));
    }

    let edges = graph
        .edges
        .iter()
        .filter(|edge| reached.contains(edge.from.as_str()) && reached.contains(edge.to.as_str()))
        .cloned()
        .collect();
    let groups = graph
        .groups
        .iter()
        .filter_map(|group| {
            let members: Vec<String> = group
                .members
                .iter()
                .filter(|member| reached.contains(member.as_str()))
                .cloned()
                .collect();
            if members.is_empty() {
                return None;
            }
            Some(IrGroup {
                id: group.id.clone(),
                label: group.label.clone(),
                members,
            })
        })
        .collect();

    Ok(IrGraph {
        direction: graph.direction,
        layout_hint: graph.layout_hint,
        nodes,
        edges,
        groups,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        extract_neighbourhood, Adornment, Cardinality, CardinalityBound, CardinalityMax, Direction,
        EndAdornment, Ir, IrEdge, IrGraph, IrGroup, IrNode, IrTimeline, LayoutHint,
        MAX_EXTRACT_NODES,
    };
    use crate::error::ErrorCode;

    /// A chain of nodes: `a -> b -> c -> ...`.
    fn chain(ids: &[&str]) -> IrGraph {
        IrGraph {
            direction: Direction::LR,
            layout_hint: LayoutHint::Tree,
            nodes: ids
                .iter()
                .map(|id| IrNode::new(*id, id.to_uppercase()))
                .collect(),
            edges: ids
                .windows(2)
                .map(|pair| IrEdge::new(pair[0], pair[1]))
                .collect(),
            groups: Vec::new(),
        }
    }

    fn node_ids(graph: &IrGraph) -> Vec<&str> {
        graph.nodes.iter().map(|node| node.id.as_str()).collect()
    }

    fn edge_pairs(graph: &IrGraph) -> Vec<(&str, &str)> {
        graph
            .edges
            .iter()
            .map(|edge| (edge.from.as_str(), edge.to.as_str()))
            .collect()
    }

    /// S21: the subgraph is the neighbourhood of `around` up to `depth` hops.
    #[test]
    fn the_neighbourhood_stops_at_the_requested_depth() {
        let graph = chain(&["a", "b", "c", "d"]);

        let one = extract_neighbourhood(&graph, "a", 1).expect("a known node extracts");
        assert_eq!(node_ids(&one), vec!["a", "b"]);

        let two = extract_neighbourhood(&graph, "a", 2).expect("a known node extracts");
        assert_eq!(node_ids(&two), vec!["a", "b", "c"]);

        let three = extract_neighbourhood(&graph, "a", 3).expect("a known node extracts");
        assert_eq!(node_ids(&three), vec!["a", "b", "c", "d"]);

        let zero = extract_neighbourhood(&graph, "a", 0).expect("zero hops is the node");
        assert_eq!(node_ids(&zero), vec!["a"]);
    }

    /// S21: the walk does not follow the direction of the edges.
    #[test]
    fn the_neighbourhood_is_undirected() {
        let graph = chain(&["a", "b"]);

        let around_target = extract_neighbourhood(&graph, "b", 1).expect("a known node extracts");

        assert_eq!(node_ids(&around_target), vec!["a", "b"]);
        assert_eq!(edge_pairs(&around_target), vec![("a", "b")]);
    }

    /// S21: an id that is not a node of the diagram, and a timeline (which
    /// has events, not nodes), answer the same actionable error.
    #[test]
    fn extract_of_an_unknown_node_is_an_error() {
        let error = extract_neighbourhood(&chain(&["a", "b"]), "zzz", 1)
            .expect_err("an unknown id never extracts");

        assert_eq!(error.code, ErrorCode::UnknownRef);
        assert_eq!(error.path, "opts.extract.around");
        assert!(error.message.contains("'zzz'"), "{}", error.message);
        assert_eq!(
            error.hint.as_deref(),
            Some("usá el `id` de un nodo de este diagrama")
        );

        let timeline = Ir::Timeline(IrTimeline {
            lanes: Vec::new(),
            events: Vec::new(),
            connections: Vec::new(),
        });
        let timeline_error = timeline
            .extracted("a", 1)
            .expect_err("a timeline has no neighbourhood");
        assert_eq!(timeline_error.code, ErrorCode::UnknownRef);
        assert_eq!(timeline_error.path, "opts.extract.around");
        assert!(timeline_error.message.contains("'a'"), "{timeline_error}");
    }

    /// S21: the subgraph keeps the edges whose two ends survived and nothing
    /// else.
    #[test]
    fn extract_keeps_only_the_edges_inside() {
        let mut graph = chain(&["a", "b", "c", "d"]);
        graph.edges.push(IrEdge::new("a", "d"));

        let around = extract_neighbourhood(&graph, "b", 1).expect("a known node extracts");

        assert_eq!(node_ids(&around), vec!["a", "b", "c"]);
        assert_eq!(edge_pairs(&around), vec![("a", "b"), ("b", "c")]);
    }

    /// S21: a group travels with its surviving members and disappears when
    /// none of them survive.
    #[test]
    fn extract_drops_groups_without_members() {
        let mut graph = chain(&["a", "b", "c", "d"]);
        graph.groups = vec![
            IrGroup {
                id: "left".to_string(),
                label: Some("izquierda".to_string()),
                members: vec!["a".to_string(), "b".to_string(), "c".to_string()],
            },
            IrGroup {
                id: "right".to_string(),
                label: None,
                members: vec!["c".to_string(), "d".to_string()],
            },
            IrGroup {
                id: "empty".to_string(),
                label: None,
                members: Vec::new(),
            },
        ];

        let around = extract_neighbourhood(&graph, "a", 1).expect("a known node extracts");

        assert_eq!(node_ids(&around), vec!["a", "b"]);
        assert_eq!(around.groups.len(), 1, "{:?}", around.groups);
        assert_eq!(around.groups[0].id, "left");
        assert_eq!(around.groups[0].members, vec!["a", "b"]);
        assert_eq!(around.groups[0].label.as_deref(), Some("izquierda"));
        // The shape of the parent is copied, never recomputed.
        assert_eq!(around.direction, graph.direction);
        assert_eq!(around.layout_hint, graph.layout_hint);
    }

    /// S21: the subgraph has a cap of its own, below the hard limit of the
    /// kind, and the error says how to get under it.
    #[test]
    fn extract_over_the_cap_is_too_large() {
        let mut star = IrGraph {
            direction: Direction::TB,
            layout_hint: LayoutHint::Layered,
            nodes: vec![IrNode::new("hub", "hub")],
            edges: Vec::new(),
            groups: Vec::new(),
        };
        for i in 0..MAX_EXTRACT_NODES - 1 {
            let id = format!("n{i}");
            star.edges.push(IrEdge::new("hub", id.clone()));
            star.nodes.push(IrNode::new(id.clone(), id));
        }

        // Exactly the cap is accepted: 1 hub + 299 leaves.
        let at_the_cap = extract_neighbourhood(&star, "hub", 1).expect("300 nodes fit");
        assert_eq!(at_the_cap.nodes.len(), MAX_EXTRACT_NODES);

        let last = format!("n{}", MAX_EXTRACT_NODES - 1);
        star.edges.push(IrEdge::new("hub", last.clone()));
        star.nodes.push(IrNode::new(last.clone(), last));

        let error = extract_neighbourhood(&star, "hub", 1).expect_err("301 nodes do not fit");

        assert_eq!(error.code, ErrorCode::TooLarge);
        assert_eq!(error.path, "opts.extract");
        assert_eq!(
            error.message,
            format!(
                "{} nodos en el subgrafo superan el tope de {}",
                MAX_EXTRACT_NODES + 1,
                MAX_EXTRACT_NODES
            )
        );
        assert_eq!(error.hint.as_deref(), Some("reducí `depth`"));
    }

    /// `f26-adornos.md`, S5: a new edge keeps the drawing of always — the plain
    /// arrow at the target end and nothing at the source. This is what lets the
    /// goldens stay byte-identical across the cut.
    #[test]
    fn a_new_edge_carries_the_plain_arrow_at_its_target_end() {
        let edge = IrEdge::new("a", "b");

        assert_eq!(edge.head, Some(Adornment::Arrow));
        assert_eq!(edge.tail, None);
        assert_eq!(
            edge.head_end(),
            Some(EndAdornment::Marker {
                adornment: Adornment::Arrow
            })
        );
        assert_eq!(edge.tail_end(), None);
    }

    /// `f26-adornos.md`, S4: the crow's foot is the notation of a relation with
    /// a cardinality, so it owns **both** ends and no marker is drawn on top of
    /// it — the choice `er` used to make by hand, now the only resolution.
    #[test]
    fn a_relation_with_cardinality_gives_both_ends_to_the_crows_foot() {
        let mut edge = IrEdge::new("a", "b");
        edge.head = Some(Adornment::Diamond);
        edge.tail = Some(Adornment::OpenArrow);
        edge.cardinality = Cardinality::from_short("1:N", true);

        assert_eq!(
            edge.head_end(),
            Some(EndAdornment::CrowFoot {
                min: CardinalityBound::Optional,
                max: CardinalityMax::Many,
            })
        );
        assert_eq!(
            edge.tail_end(),
            Some(EndAdornment::CrowFoot {
                min: CardinalityBound::Required,
                max: CardinalityMax::One,
            })
        );
    }

    /// The two ends are independent, which is the whole point of `head`/`tail`:
    /// a composition says "this diamond is mine" while the other end keeps the
    /// arrow that shows which way to read the line.
    #[test]
    fn the_two_ends_are_independent_adornments() {
        let mut edge = IrEdge::new("part", "whole");
        edge.tail = Some(Adornment::OpenArrow);
        edge.head = Some(Adornment::Diamond);

        assert_eq!(
            edge.tail_end(),
            Some(EndAdornment::Marker {
                adornment: Adornment::OpenArrow
            })
        );
        assert_eq!(
            edge.head_end(),
            Some(EndAdornment::Marker {
                adornment: Adornment::Diamond
            })
        );
    }
}
