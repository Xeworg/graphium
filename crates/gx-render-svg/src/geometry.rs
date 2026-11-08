//! The math the two emitters of `gx-render-svg` share (`f11-elementos.md`, T1).
//!
//! The scene builder ([`crate::scene`]) and the SVG emitter
//! ([`crate::renderer`]) must draw the same picture: one mounts elements in
//! the window, the other writes the exported file. To keep them from drifting
//! apart, every decision that is not "how do I spell this in SVG" lives here —
//! the rectangle of a node, the order of the layers, the ids of the drawing,
//! the line breaks of a label, the geometry of a crow's foot — and the second
//! emitter reads the same numbers instead of recomputing them.
//!
//! The glyph constants are also the contract with the painter (S5): they travel
//! in [`SceneTokens`](gx_core::SceneTokens) as numbers, so the JS draws a crow's
//! foot with the same size the exporter uses and the payload never carries a
//! path.

use gx_core::ir::Span;
use gx_core::{
    fmt_num, Emphasis, IrEdge, IrEvent, IrGraph, IrNode, IrRow, LayoutEdge, LayoutGraph,
    LayoutGroup, LayoutHint, NodeBox, Point, Rect, RowKey, Shape, TOKENS,
};

/// Length of the arrowhead, in plan units (`markerUnits="userSpaceOnUse"`).
pub(crate) const ARROW_LENGTH: f64 = 10.0;
/// Opacity of a muted node or edge.
pub(crate) const MUTED_OPACITY: f64 = 0.55;
/// Dash pattern of a lifeline, in plan units: shorter than the one of an
/// arrow, so it never competes with the messages.
pub(crate) const LIFELINE_DASH: (f64, f64) = (4.0, 4.0);
/// Dash pattern of an edge declared dashed.
pub(crate) const DASH_PATTERN: (f64, f64) = (6.0, 4.0);
/// Dash pattern of the border of a container: it says "this is a boundary",
/// not one more box of the drawing.
pub(crate) const GROUP_DASH: (f64, f64) = (8.0, 5.0);
/// Size of the glyphs of the crow's foot, in plan units.
///
/// The legs of the foot are wide and short: a narrow fan reads as an arrowhead,
/// which is the one thing a crow's foot must not look like.
pub(crate) const CROW_FOOT: f64 = 22.0;
/// How wide the single stroke of the "one" side opens.
pub(crate) const CROW_FOOT_SPREAD: f64 = 0.85;
/// Horizontal space reserved for the `PK` / `FK` badge of a row.
pub(crate) const BADGE_WIDTH: f64 = 26.0;
/// Height of the bar of an event that lasts, in plan units (`f11-elementos.md`,
/// T4/D). It is a band, not a second box: it says "this event covers a stretch
/// of time" without competing with the label inside the box.
pub(crate) const EVENT_BAR_HEIGHT: f64 = 5.0;

/// The bar of an event that lasts, when it lasts (`f11-elementos.md`, T4/D).
///
/// The lanes engine already reads [`IrEvent::span`] when it measures the event
/// (`gx-layout`, `event_size`): a stretch of `n` columns asks for a box `n`
/// columns wide, so **the measured box is the length of the bar**. What the
/// drawing was missing was the mark — a point in time and a stretch of time
/// were drawn exactly the same — and it is a band along the bottom of that box,
/// inset by the border so it reads inside the shape.
///
/// The numbers are the numbers: the exported markup writes this rectangle and
/// the scene carries it (S5), so the payload never ships a path and the painter
/// never recomputes one.
///
/// A span whose ends are equal, or reversed, is a point in time: that is the
/// rule the layout applies when it sizes the event (`span.end > span.start`),
/// and the drawing repeats it instead of inventing a second one.
pub(crate) fn event_bar(rect: &Rect, span: Option<Span>) -> Option<Rect> {
    let span = span?;
    // The rule of the layout, repeated: `end > start` and nothing else.
    let lasts = span.end > span.start;
    if !lasts {
        return None;
    }
    let inset = TOKENS.border_width;
    Some(Rect::new(
        rect.x + inset,
        rect.max_y() - inset - EVENT_BAR_HEIGHT,
        (rect.w - 2.0 * inset).max(0.0),
        EVENT_BAR_HEIGHT,
    ))
}

/// The plan rectangle the root element covers.
pub(crate) fn canvas_rect(layout: &LayoutGraph) -> Rect {
    let bbox = layout.bbox;
    if bbox.w > 0.0 && bbox.h > 0.0 {
        return bbox;
    }
    // The layout normalizes the graph so the bounding box starts at `(0, 0)`
    // (F0 decision L9) and always reports it. If a layout does not, derive one
    // from the node boxes so the canvas is never degenerate. Safe default.
    let mut bounds: Option<Rect> = None;
    for placed in layout.nodes.values() {
        let rect = node_rect(placed);
        bounds = Some(match bounds {
            Some(union) => union.union(&rect),
            None => rect,
        });
    }
    bounds.unwrap_or(bbox)
}

/// The rectangle of a placed node, from the center of its measured box.
pub(crate) fn node_rect(placed: &NodeBox) -> Rect {
    Rect::new(
        placed.center.x - placed.size.w / 2.0,
        placed.center.y - placed.size.h / 2.0,
        placed.size.w,
        placed.size.h,
    )
}

/// The nodes of a graph, in id order (PRD §8.5.1: never insertion order).
pub(crate) fn ordered_nodes(graph: &IrGraph) -> Vec<&IrNode> {
    let mut nodes: Vec<&IrNode> = graph.nodes.iter().collect();
    nodes.sort_by(|left, right| left.id.cmp(&right.id));
    nodes
}

/// The events of a timeline, in id order (PRD §8.5.1: never insertion order).
pub(crate) fn ordered_events(timeline: &gx_core::IrTimeline) -> Vec<&IrEvent> {
    let mut events: Vec<&IrEvent> = timeline.events.iter().collect();
    events.sort_by(|left, right| left.id.cmp(&right.id));
    events
}

/// The clusters of a graph, outermost first: a box is drawn behind the ones it
/// contains, so the inner box never hides the outer border (PRD §7 RF15).
pub(crate) fn ordered_clusters(layout: &LayoutGraph) -> Vec<&LayoutGroup> {
    let mut groups: Vec<&LayoutGroup> = layout.groups.iter().collect();
    groups.sort_by(|left, right| {
        left.depth
            .cmp(&right.depth)
            .then_with(|| left.id.cmp(&right.id))
    });
    groups
}

/// The lane bands of a timeline, in id order.
pub(crate) fn ordered_groups(layout: &LayoutGraph) -> Vec<&LayoutGroup> {
    let mut groups: Vec<&LayoutGroup> = layout.groups.iter().collect();
    groups.sort_by(|left, right| left.id.cmp(&right.id));
    groups
}

/// The edges that can be drawn, in id order.
pub(crate) fn routed_edges<'a>(
    graph: &'a IrGraph,
    layout: &'a LayoutGraph,
) -> Vec<(&'a IrEdge, &'a LayoutEdge)> {
    let mut routed: Vec<(&IrEdge, &LayoutEdge)> = graph
        .edges
        .iter()
        .filter_map(|edge| {
            let route = layout
                .edges
                .iter()
                .find(|candidate| candidate.from == edge.from && candidate.to == edge.to)?;
            Some((edge, route))
        })
        .collect();
    // `sort_by` is stable, so parallel edges keep the order the layout gave
    // them. `m-<from>-<to>` cannot tell them apart and the canonical shape
    // forbids counters, so parallel edges share an id in F0 (known limitation).
    routed.sort_by(|left, right| {
        (left.0.from.as_str(), left.0.to.as_str())
            .cmp(&(right.0.from.as_str(), right.0.to.as_str()))
    });
    routed
}

/// `g-<id>`: the id of a node or an event in the drawing (S3).
pub(crate) fn node_id(id: &str) -> String {
    format!("g-{id}")
}

/// `m-<from>-<to>`: the id of an edge in the drawing (S3).
pub(crate) fn edge_id(from: &str, to: &str) -> String {
    format!("m-{from}-{to}")
}

/// `cluster-<id>`: the id of a container in the drawing (S3).
pub(crate) fn cluster_id(id: &str) -> String {
    format!("cluster-{id}")
}

/// `lane-<id>`: the id of a lane band in the drawing (S3).
pub(crate) fn lane_id(id: &str) -> String {
    format!("lane-{id}")
}

/// The marker of an emphasis: the arrowheads of `<defs>` (PRD §8.5.1).
pub(crate) fn marker_id(emphasis: Emphasis) -> &'static str {
    match emphasis {
        Emphasis::Normal => "arrow-normal",
        Emphasis::Highlight => "arrow-highlight",
        Emphasis::Muted => "arrow-muted",
    }
}

/// Adds the classes an emphasis needs. Highlighting a shape changes its stroke
/// and highlighting a text changes its fill, so the two need their own class.
pub(crate) fn push_emphasis(classes: &mut String, emphasis: Emphasis, for_text: bool) {
    match emphasis {
        Emphasis::Normal => {}
        Emphasis::Highlight => classes.push_str(if for_text {
            " emphasis-highlight-text"
        } else {
            " emphasis-highlight"
        }),
        Emphasis::Muted => classes.push_str(" emphasis-muted"),
    }
}

/// The class of a shape: the painter mounts the same names, so the CSS rules of
/// the page and the stylesheet of the export say the same thing.
pub(crate) fn shape_class(shape: Shape) -> &'static str {
    match shape {
        Shape::Box => "shape-box",
        Shape::Rounded => "shape-rounded",
        Shape::Cylinder => "shape-cylinder",
        Shape::Diamond => "shape-diamond",
        Shape::Ellipse => "shape-ellipse",
        Shape::Note => "shape-note",
    }
}

/// The lines a label breaks into.
///
/// Labels arrive **pre-broken with `\n`** (S4): the Rust side measures, the JS
/// side never wraps. A node label, an event label and a band label all break
/// here, so the emitter and the painter read the same lines.
pub(crate) fn label_lines(label: &str) -> Vec<&str> {
    label.lines().collect()
}

/// The baseline of each line of a label centered on `center_y`.
///
/// One formula for the whole drawing: a node label, an event label and an edge
/// label sit on the same grid, and a single line lands exactly on the center.
pub(crate) fn centered_baselines(center_y: f64, lines: usize) -> Vec<f64> {
    let line_height = TOKENS.text_size * TOKENS.line_height;
    let total = line_height * lines as f64;
    (0..lines)
        .map(|index| center_y - total / 2.0 + line_height * (index as f64 + 0.5))
        .collect()
}

/// Where the header, the rule and the rows of a table node sit (PRD §6.1: a
/// table is a node whose rows are its columns).
///
/// The painter draws the same table from [`SceneTokens`](gx_core::SceneTokens):
/// these are the numbers those tokens produce.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Table {
    /// Padding around the label.
    pub(crate) padding: f64,
    /// Height of one line, `text_size * line_height`.
    pub(crate) line: f64,
    /// Baseline of the label of the header.
    pub(crate) header_y: f64,
    /// The rule that separates the header from the rows.
    pub(crate) rule_y: f64,
    /// Left edge of the key badge column.
    pub(crate) badge_x: f64,
    /// Left edge of the name column.
    pub(crate) name_x: f64,
    /// Right edge of the note column, drawn against the border.
    pub(crate) note_x: f64,
}

impl Table {
    /// The table of a node box.
    pub(crate) fn of(rect: &Rect) -> Self {
        let padding = TOKENS.node_padding;
        let line = TOKENS.text_size * TOKENS.line_height;
        let header_y = rect.y + padding + line / 2.0;
        Self {
            padding,
            line,
            header_y,
            rule_y: header_y + line / 2.0,
            badge_x: rect.x + padding / 2.0,
            name_x: rect.x + padding + BADGE_WIDTH,
            note_x: rect.max_x() - padding / 2.0,
        }
    }

    /// The baseline of the row at `index`.
    pub(crate) fn row_y(&self, index: usize) -> f64 {
        self.rule_y + self.line * (index as f64 + 0.5)
    }

    /// The text of the key badge of a row: `PK`, `FK`, `PK, FK` or nothing.
    pub(crate) fn badge(key: RowKey) -> &'static str {
        match key {
            RowKey::Pk => "PK",
            RowKey::Fk => "FK",
            RowKey::PkFk => "PK, FK",
            RowKey::None => "",
        }
    }
}

/// The classes of the name of a row: the key and the nullability are painted.
pub(crate) fn row_name_classes(row: &IrRow) -> String {
    let mut classes = String::from("node-text text");
    if matches!(row.key, RowKey::Pk | RowKey::PkFk) {
        classes.push_str(" row-key");
    }
    if row.nullable {
        classes.push_str(" row-nullable");
    }
    classes
}

/// The unit direction of the line at one of its ends.
pub(crate) fn direction_at(route: &LayoutEdge, at_end: bool) -> (f64, f64) {
    let (from, to) = if at_end {
        let last = route.points.last().copied().unwrap_or_default();
        let before = route.points.iter().rev().nth(1).copied().unwrap_or(last);
        (last, before)
    } else {
        let first = route.points.first().copied().unwrap_or_default();
        let after = route.points.get(1).copied().unwrap_or(first);
        (first, after)
    };
    let dx = from.x - to.x;
    let dy = from.y - to.y;
    let length = (dx * dx + dy * dy).sqrt();
    if length <= f64::EPSILON {
        (1.0, 0.0)
    } else {
        (dx / length, dy / length)
    }
}

/// The glyphs of the crow's foot at one end of a relation, in plan units.
///
/// The notation is the crow's foot: each end says how many rows of the other
/// table can point here and whether zero is allowed (PRD §6.1). The geometry is
/// **not** shipped as a path (S5): [`CROW_FOOT`] and [`CROW_FOOT_SPREAD`] travel
/// in the tokens and the painter rebuilds these very points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum CrowFoot {
    /// Many rows: the ankle away from the table and the three toes on its edge.
    Many {
        /// Where the three legs meet.
        ankle: Point,
        /// The two outer toes; the third one is the end of the line itself.
        toes: [Point; 2],
    },
    /// Exactly one row: a single stroke across the relation.
    One {
        /// One end of the stroke.
        from: Point,
        /// The other end of the stroke.
        to: Point,
    },
}

/// One end of a relation: its glyph and, when zero rows are allowed, the
/// circle that says so.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CrowFootMark {
    pub(crate) glyph: CrowFoot,
    /// Center of the "zero rows allowed" circle.
    pub(crate) optional: Option<Point>,
    /// Radius of that circle.
    pub(crate) radius: f64,
}

/// The mark of one end of a relation.
pub(crate) fn crow_foot_mark(
    at: Point,
    direction: (f64, f64),
    optional: bool,
    many: bool,
) -> CrowFootMark {
    let (dx, dy) = direction;
    let back = |distance: f64| Point::new(at.x - dx * distance, at.y - dy * distance);
    let side = |point: Point, offset: f64| Point::new(point.x - dy * offset, point.y + dx * offset);
    let glyph = if many {
        // The crow's foot opens TOWARDS the table: the ankle sits away from it
        // and the three toes touch its edge. Opening away from the table draws
        // an arrowhead, which is the opposite meaning.
        let spread = CROW_FOOT * 0.62;
        CrowFoot::Many {
            ankle: back(CROW_FOOT),
            toes: [side(at, spread), side(at, -spread)],
        }
    } else {
        // Exactly one: a single stroke across the relation.
        let mark = back(CROW_FOOT * 0.6);
        let half = CROW_FOOT * CROW_FOOT_SPREAD;
        CrowFoot::One {
            from: side(mark, half),
            to: side(mark, -half),
        }
    };
    CrowFootMark {
        glyph,
        optional: optional.then(|| back(CROW_FOOT * 1.45)),
        radius: CROW_FOOT / 3.5,
    }
}

/// Whether the graph draws a lifeline under every actor (PRD §6, `sequence`).
///
/// A diagram of sequence without lifelines reads broken: every arrow floats and
/// the reader cannot tell which column it belongs to.
pub(crate) fn draws_lifelines(graph: &IrGraph) -> bool {
    graph.layout_hint == LayoutHint::Lanes
}

/// The label of a band, when it has one worth drawing.
pub(crate) fn band_label(group: &LayoutGroup) -> Option<&str> {
    group
        .label
        .as_deref()
        .filter(|label| !label.trim().is_empty())
}

/// The point of the canvas the lifeline of a node reaches: the bottom edge.
pub(crate) fn lifeline_end(canvas: &Rect) -> f64 {
    canvas.max_y()
}

/// The x of a lifeline: the center of the actor.
pub(crate) fn lifeline_x(placed: &NodeBox) -> f64 {
    placed.center.x
}

/// The y where a lifeline starts: the bottom border of the actor.
pub(crate) fn lifeline_start(placed: &NodeBox) -> f64 {
    placed.center.y + placed.size.h / 2.0
}

/// The two decimals of every numeric attribute, for a list of points.
pub(crate) fn points_text(points: &[Point]) -> Vec<String> {
    points
        .iter()
        .map(|point| format!("{},{}", fmt_num(point.x), fmt_num(point.y)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        centered_baselines, crow_foot_mark, direction_at, event_bar, label_lines, node_rect,
        CrowFoot, CROW_FOOT, CROW_FOOT_SPREAD, EVENT_BAR_HEIGHT,
    };
    use gx_core::ir::Span;
    use gx_core::{LayoutEdge, NodeBox, Point, Rect, Size, TOKENS};

    /// S4: the lines of a label are the ones the `\n` already marks; the JS
    /// never wraps and the emitter never invents a break.
    #[test]
    fn a_label_breaks_where_its_newlines_are() {
        assert_eq!(label_lines("uno"), vec!["uno"]);
        assert_eq!(label_lines("uno\ndos"), vec!["uno", "dos"]);
        assert_eq!(label_lines(""), Vec::<&str>::new());
        assert_eq!(label_lines("uno\n"), vec!["uno"]);
    }

    /// S4: one formula for the baselines of every label of the drawing, and a
    /// single line lands exactly on the center of its box.
    #[test]
    fn the_baselines_are_one_formula_and_one_line_is_centered() {
        let line = TOKENS.text_size * TOKENS.line_height;
        assert_eq!(centered_baselines(100.0, 1), vec![100.0]);
        assert_eq!(
            centered_baselines(100.0, 2),
            vec![100.0 - line / 2.0, 100.0 + line / 2.0]
        );
        let three = centered_baselines(100.0, 3);
        assert_eq!(three[1], 100.0, "the middle line of three is the center");
        assert_eq!(three[0], 100.0 - line, "the lines are one line apart");
        assert_eq!(three[2], 100.0 + line);
    }

    /// The rectangle of a node comes from the center of its measured box, never
    /// from the IR size (S4).
    #[test]
    fn the_rectangle_of_a_node_is_its_measured_box() {
        let placed = NodeBox {
            center: Point::new(50.0, 30.0),
            size: Size::new(100.0, 40.0),
        };
        assert_eq!(node_rect(&placed), Rect::new(0.0, 10.0, 100.0, 40.0));
    }

    /// T4/D: the bar of an event that lasts. The measured box is the length of
    /// the bar (the layout sized it from the span), the band sits at the bottom
    /// inside the border, and a point in time has no bar at all.
    #[test]
    fn the_bar_of_an_event_is_a_band_on_the_bottom_of_its_measured_box() {
        let rect = Rect::new(175.0, 48.0, 250.0, 44.0);
        let bar = event_bar(
            &rect,
            Some(Span {
                start: 1.0,
                end: 4.0,
            }),
        )
        .expect("a stretch of time is a bar");
        let inset = TOKENS.border_width;
        assert_eq!(bar.x, rect.x + inset, "inset by the border, left");
        assert_eq!(
            bar.max_y(),
            rect.max_y() - inset,
            "inset by the border, bottom"
        );
        assert_eq!(bar.h, EVENT_BAR_HEIGHT, "the frozen height of the band");
        assert_eq!(bar.w, rect.w - 2.0 * inset, "the length of the box");

        // A point in time: no span, the same ends, or ends in the wrong order.
        assert_eq!(event_bar(&rect, None), None);
        assert_eq!(
            event_bar(
                &rect,
                Some(Span {
                    start: 3.0,
                    end: 3.0
                })
            ),
            None,
            "a span of zero columns is a point"
        );
        assert_eq!(
            event_bar(
                &rect,
                Some(Span {
                    start: 4.0,
                    end: 1.0
                })
            ),
            None,
            "the layout reads a reversed span as a point too"
        );
    }

    /// S5: the crow's foot opens towards the table — the ankle away from it, the
    /// toes on its edge — and that is what the tokens say.
    #[test]
    fn the_crow_foot_opens_towards_the_table() {
        // A line arriving at `(100, 50)` from the left: the direction into the
        // box is +x.
        let mark = crow_foot_mark(Point::new(100.0, 50.0), (1.0, 0.0), false, true);
        let CrowFoot::Many { ankle, toes } = mark.glyph else {
            panic!("many rows draw a fork");
        };
        assert_eq!(ankle, Point::new(100.0 - CROW_FOOT, 50.0));
        let spread = CROW_FOOT * 0.62;
        assert_eq!(toes[0], Point::new(100.0, 50.0 + spread));
        assert_eq!(toes[1], Point::new(100.0, 50.0 - spread));
        assert_eq!(mark.optional, None, "a required end has no circle");

        let optional = crow_foot_mark(Point::new(100.0, 50.0), (1.0, 0.0), true, true);
        let circle = optional.optional.expect("zero rows allowed: a circle");
        assert_eq!(circle, Point::new(100.0 - CROW_FOOT * 1.45, 50.0));
        assert!(
            circle.x < ankle.x,
            "the circle sits behind the fork, away from the table"
        );
    }

    /// The "one" side of a relation is a single stroke across the line, and it
    /// opens with the frozen spread.
    #[test]
    fn the_one_side_is_a_single_stroke() {
        let mark = crow_foot_mark(Point::new(0.0, 0.0), (1.0, 0.0), false, false);
        let CrowFoot::One { from, to } = mark.glyph else {
            panic!("one row draws a stroke");
        };
        let half = CROW_FOOT * CROW_FOOT_SPREAD;
        assert_eq!(from, Point::new(-CROW_FOOT * 0.6, half));
        assert_eq!(to, Point::new(-CROW_FOOT * 0.6, -half));
    }

    /// The unit direction at an end of a route, and the safe default of a route
    /// with a single point.
    #[test]
    fn the_direction_of_a_route_points_into_its_ends() {
        let route = LayoutEdge {
            from: "a".to_string(),
            to: "b".to_string(),
            points: vec![Point::new(0.0, 0.0), Point::new(10.0, 0.0)],
            label_at: None,
        };
        assert_eq!(direction_at(&route, true), (1.0, 0.0));
        assert_eq!(direction_at(&route, false), (-1.0, 0.0));

        let single = LayoutEdge {
            points: vec![Point::new(4.0, 4.0)],
            ..route
        };
        assert_eq!(direction_at(&single, true), (1.0, 0.0));
    }
}
