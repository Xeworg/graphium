//! The structured scene the painter mounts (`f11-elementos`, S3-S5).
//!
//! The scene **mirrors** `Ir` ∪ [`LayoutGraph`](crate::layout::LayoutGraph): it
//! invents no third vocabulary (`f11-elementos.md`, S3). Every field is either
//! a field of the IR, a field of the layout, or a frozen token of the drawing
//! ([`TOKENS`](crate::tokens::TOKENS) and the glyph constants of the emitter),
//! so the two sides of the paint — the SVG string of the export and the
//! elements the window mounts — cannot drift apart.
//!
//! Three rules decide the shape of this schema:
//!
//! - **Ids are the ids of the drawing** (S3): `g-<id>` for a node or an event,
//!   `m-<from>-<to>` for an edge, `cluster-<id>` / `lane-<id>` for a band. The
//!   scene writes the same strings the SVG emitter writes, so the equivalence
//!   probe of task C compares element by element and not by name.
//! - **The JS never measures** (S4): `center` and `size` come from
//!   [`NodeBox`](crate::layout::NodeBox), the measured box, and the labels
//!   arrive pre-broken with `\n`. There is no wrap feature on either side.
//! - **No geometry as paths** (S5): a crow's foot, a cylinder rim, a diamond
//!   or a marker travels as numbers in [`SceneTokens`], never as a `d` string.
//!   A path in the payload would be "SVG again".
//!
//! The paint order of the scene is the paint order of the SVG emitter: bands,
//! then the lifelines of a lanes graph, then edges, then nodes and events.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::geom::{Point, Rect, Size};
use crate::ir::{Cardinality, Direction, Emphasis, IrRow, IrValue, LayoutHint, Shape, Span, Style};
use crate::render::Theme;

/// The whole drawing, as data (`f11-elementos.md`, S3).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Scene {
    /// The plan rectangle the drawing covers: the bounding box of the layout.
    pub canvas: Rect,
    /// The frozen numbers of the drawing, so the painter never guesses one.
    pub tokens: SceneTokens,
    /// The direction of a layered graph; `None` for a timeline, which has no
    /// direction (`IrTimeline` has no such field).
    pub direction: Option<Direction>,
    /// The layout strategy of the document; `Lanes` is what tells the painter
    /// to draw a lifeline under every actor, as the SVG emitter does.
    pub layout_hint: Option<LayoutHint>,
    /// Where each paint slot takes its colour from (S3): the CSS custom
    /// properties the page publishes from the settings, never a literal.
    pub palette: ScenePalette,
    /// The nodes of a graph, in id order.
    pub nodes: Vec<SceneNode>,
    /// The routed edges of a graph, in id order.
    pub edges: Vec<SceneEdge>,
    /// The containers of a graph and the lanes of a timeline, background
    /// first and then in the order the SVG emitter draws them.
    pub bands: Vec<SceneBand>,
    /// The events of a timeline, in id order.
    pub events: Vec<SceneEvent>,
}

/// The frozen numbers of the drawing (`f11-elementos.md`, S5).
///
/// [`TOKENS`](crate::tokens::TOKENS) plus the glyph constants of the emitter:
/// the length of an arrowhead, the size of a crow's foot, the width the key
/// badge reserves, the dash patterns. They travel as numbers on purpose: the
/// painter draws the glyphs, the payload does not carry their paths.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SceneTokens {
    /// Font size of the labels, in plan units.
    pub text_size: f64,
    /// Padding around the label inside a node box.
    pub node_padding: f64,
    /// Corner radius of rounded nodes; the rim of a cylinder and the fold of a
    /// note are capped from it.
    pub corner_radius: f64,
    /// Border width of nodes and edges.
    pub border_width: f64,
    /// Line height multiplier of a label.
    pub line_height: f64,
    /// Font weight of the regular text.
    pub regular_weight: u16,
    /// Font weight of an emphasized text.
    pub semibold_weight: u16,
    /// Length of the arrowhead, in plan units (`markerUnits="userSpaceOnUse"`).
    pub arrow_length: f64,
    /// Size of the glyphs of the crow's foot, in plan units.
    pub crow_foot: f64,
    /// How wide the single stroke of the "one" side of a relation opens.
    pub crow_foot_spread: f64,
    /// Horizontal space reserved for the `PK` / `FK` badge of a row.
    pub badge_width: f64,
    /// Opacity of a muted node or edge.
    pub muted_opacity: f64,
    /// Dash pattern of a lifeline, in plan units.
    pub lifeline_dash: [f64; 2],
    /// Dash pattern of a dashed edge, in plan units.
    pub dash_pattern: [f64; 2],
    /// Dash pattern of the border of a container, in plan units.
    pub group_dash: [f64; 2],
}

/// Where the paint takes its colours from (`f11-elementos.md`, S3).
///
/// The scene carries **no colour literal**: every slot names the CSS custom
/// property the page publishes from the settings (`palettePaint`), so a theme
/// change repaints the mounted scene without a new render, exactly as it
/// repaints the `{@html}` SVG today.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ScenePalette {
    /// The channel the colours travel through: always `"css-vars"`.
    pub source: &'static str,
    /// The theme of the settings, which the page applies as `data-theme`.
    pub theme: Theme,
    /// The canvas of an export.
    pub bg: &'static str,
    /// The fill of a node box.
    pub node_fill: &'static str,
    /// The border of a node box and the line of a lifeline.
    pub node_stroke: &'static str,
    /// The label of a node.
    pub node_text: &'static str,
    /// The line of an edge.
    pub edge_stroke: &'static str,
    /// The label of an edge, of a container and of a lane.
    pub edge_text: &'static str,
    /// The highlight of a node or an edge.
    pub accent: &'static str,
}

impl ScenePalette {
    /// The channel of the palette: there is one.
    pub const CSS_VARS: &'static str = "css-vars";

    /// The palette of a theme, expressed as the custom properties of the page.
    ///
    /// The names are the ones `palettePaint` writes on the root element, so the
    /// painter and the stylesheet cannot disagree about a colour.
    pub fn css_vars(theme: Theme) -> Self {
        Self {
            source: Self::CSS_VARS,
            theme,
            bg: "var(--bg)",
            node_fill: "var(--node-fill)",
            node_stroke: "var(--node-stroke)",
            node_text: "var(--node-text)",
            edge_stroke: "var(--edge-stroke)",
            edge_text: "var(--edge-text)",
            accent: "var(--accent)",
        }
    }

    /// Every slot and the custom property it names.
    pub fn slots(&self) -> [(&'static str, &'static str); 7] {
        [
            ("bg", self.bg),
            ("node_fill", self.node_fill),
            ("node_stroke", self.node_stroke),
            ("node_text", self.node_text),
            ("edge_stroke", self.edge_stroke),
            ("edge_text", self.edge_text),
            ("accent", self.accent),
        ]
    }
}

/// A node of the graph: `IrNode` plus the box the layout gave it (S3, S4).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SceneNode {
    /// `g-<id>`, the id of the element in the drawing.
    pub id: String,
    /// The label, pre-broken with `\n`; the painter never wraps (S4).
    pub label: String,
    pub shape: Shape,
    pub emphasis: Emphasis,
    pub style: Style,
    /// Center of the measured box, in plan units.
    pub center: Point,
    /// Size of the measured box, never `IrNode.size` recomputed in the JS (S4).
    pub size: Size,
    /// The detail rows of the node, drawn under its label. Absent when the
    /// node has none: an empty list and no list mean the same thing.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<IrRow>,
    /// The free channel of the kind, untouched (S3).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub meta: BTreeMap<String, IrValue>,
}

/// A routed edge: `IrEdge` plus the polyline of the layout (S3).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SceneEdge {
    /// `m-<from>-<to>`, the id of the element in the drawing.
    pub id: String,
    /// `id` of the source node, as the IR names it (the node is `g-<from>`).
    pub from: String,
    /// `id` of the target node, as the IR names it (the node is `g-<to>`).
    pub to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub emphasis: Emphasis,
    pub style: Style,
    /// Whether the target end carries a marker: `arrow-<emphasis>`.
    pub arrow: bool,
    /// Row of the source node the edge leaves from, when it anchors to one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_row: Option<u32>,
    /// Row of the target node the edge arrives at, when it anchors to one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_row: Option<u32>,
    /// The crow's foot of the relation, when the kind has one: the painter
    /// draws it from [`SceneTokens::crow_foot`] (S5).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cardinality: Option<Cardinality>,
    /// The route, in plan units.
    pub points: Vec<Point>,
    /// Where the label sits, when it has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label_at: Option<Point>,
}

/// A container of a graph or a lane of a timeline: `LayoutGroup` (S3).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SceneBand {
    /// `cluster-<id>` for a container, `lane-<id>` for a lane: the id the
    /// emitter writes, so the two drawings name the same box.
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The box of the band, in plan units.
    pub bbox: Rect,
    /// How many bands contain this one; `0` is outermost (`IrGroup` depth).
    pub depth: usize,
    /// The ids the band owns, in the order of the document: nodes or other
    /// bands for a container, events of the lane for a lane.
    pub members: Vec<String>,
    /// The left gutter the layout reserved for the names of the lanes, in
    /// plan units; absent for a container, which has no gutter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gutter: Option<f64>,
}

/// An event of a timeline: `IrEvent` plus the measured box (S3, S4).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SceneEvent {
    /// `g-<id>`, the id of the element in the drawing.
    pub id: String,
    /// `id` of the lane the event belongs to (the band is `lane-<lane>`).
    pub lane: String,
    /// The label, pre-broken with `\n` (S4).
    pub label: String,
    pub order: i64,
    /// The time span of a `gantt` or a `timeline` bar.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    /// Center of the measured box, in plan units.
    pub center: Point,
    /// Size of the measured box: the lanes engine measures it, the JS never
    /// does (S4).
    pub size: Size,
    /// The bar of an event that lasts, as numbers (`f11-elementos.md`, T4/D).
    ///
    /// It is a band along the bottom of [`size`](Self::size): the layout
    /// already sized the box from the span, so the box is the length of the bar
    /// and the painter only has to draw the rectangle it is given. Absent for a
    /// point in time — a span with equal or reversed ends is one, the rule the
    /// layout applies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bar: Option<Rect>,
}

#[cfg(test)]
mod tests {
    use super::{ScenePalette, SceneTokens};
    use crate::render::Theme;
    use crate::tokens::TOKENS;

    /// S3, S5: the palette of the scene names the CSS custom properties of the
    /// page and never a colour literal, so a theme change repaints the mounted
    /// scene without a new render.
    #[test]
    fn the_palette_of_the_scene_is_the_css_vars_of_the_page() {
        for theme in Theme::ALL {
            let palette = ScenePalette::css_vars(theme);
            assert_eq!(palette.source, "css-vars");
            assert_eq!(palette.theme, theme, "{theme:?} keeps its name");
            for (slot, value) in palette.slots() {
                assert!(
                    value.starts_with("var(--") && value.ends_with(')'),
                    "{slot} is a custom property, not a colour: {value}"
                );
                assert!(
                    !value.contains('#') && !value.contains("rgb"),
                    "{slot} carries a colour literal: {value}"
                );
            }
            // The names are the ones `palettePaint` writes on the root: a new
            // name here would paint nothing at all.
            let names: Vec<&str> = palette.slots().iter().map(|(_, value)| *value).collect();
            assert_eq!(
                names,
                vec![
                    "var(--bg)",
                    "var(--node-fill)",
                    "var(--node-stroke)",
                    "var(--node-text)",
                    "var(--edge-stroke)",
                    "var(--edge-text)",
                    "var(--accent)",
                ]
            );
            assert_eq!(palette, ScenePalette::css_vars(theme), "deterministic");
        }
    }

    /// S5: the numbers of the drawing travel as tokens, so a painter that draws
    /// a crow's foot and the emitter that writes it agree without a path.
    #[test]
    fn the_tokens_of_the_scene_are_the_frozen_ones() {
        let tokens = SceneTokens {
            text_size: TOKENS.text_size,
            node_padding: TOKENS.node_padding,
            corner_radius: TOKENS.corner_radius,
            border_width: TOKENS.border_width,
            line_height: TOKENS.line_height,
            regular_weight: TOKENS.regular_weight,
            semibold_weight: TOKENS.semibold_weight,
            arrow_length: 10.0,
            crow_foot: 22.0,
            crow_foot_spread: 0.85,
            badge_width: 26.0,
            muted_opacity: 0.55,
            lifeline_dash: [4.0, 4.0],
            dash_pattern: [6.0, 4.0],
            group_dash: [8.0, 5.0],
        };
        assert_eq!(tokens.text_size, TOKENS.text_size);
        let json = serde_json::to_value(tokens).expect("the tokens serialise");
        for key in [
            "text_size",
            "node_padding",
            "corner_radius",
            "border_width",
            "line_height",
            "regular_weight",
            "semibold_weight",
            "arrow_length",
            "crow_foot",
            "crow_foot_spread",
            "badge_width",
            "muted_opacity",
            "lifeline_dash",
            "dash_pattern",
            "group_dash",
        ] {
            assert!(json.get(key).is_some(), "the token {key} travels");
        }
        assert_eq!(json["lifeline_dash"], serde_json::json!([4.0, 4.0]));
    }
}
