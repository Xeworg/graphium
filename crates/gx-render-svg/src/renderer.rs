//! The deterministic SVG writer (PRD §8.5, §8.5.1, §8.5.2).
//!
//! Rules this module enforces mechanically:
//!
//! - Every numeric attribute goes through [`gx_core::fmt_num`] (S7).
//! - Every string that comes from the JSON goes through
//!   [`crate::escape::escape_text`] (S12).
//! - Nodes and edges are emitted in id order, ids come from the IR (`g-<id>`,
//!   `m-<from>-<to>`) and never from counters, and the arrow markers of
//!   `<defs>` are sorted by id (S7).
//! - No `HashMap`, no clock and no randomness: the same IR always produces the
//!   same bytes (PRD §8.5).
//!
//! The renderer draws the [`LayoutGraph`] it receives and never measures or
//! resizes anything (PRD §8.4.1); [`InterMetrics`](crate::InterMetrics) is kept
//! only so the wiring layer can build the layout engine from the same instance.

use std::collections::BTreeSet;
use std::sync::Arc;

use gx_core::{
    fmt_num, Emphasis, Ir, IrEdge, IrEvent, IrGraph, IrNode, IrTimeline, LayoutEdge, LayoutGraph,
    LayoutGroup, LayoutHint, NodeBox, Palette, Rect, Shape, Style,
    SvgRenderer as SvgRendererContract, TextMetrics, TOKENS,
};

use crate::escape::escape_text;

/// Namespace of the emitted document, so the SVG parses on its own (S12).
const SVG_NAMESPACE: &str = "http://www.w3.org/2000/svg";
/// Length of the arrowhead, in plan units (`markerUnits="userSpaceOnUse"`).
const ARROW_LENGTH: f64 = 10.0;
/// Opacity of a muted node or edge.
const MUTED_OPACITY: f64 = 0.55;
/// Dash pattern of a dashed node border or edge, in plan units.
/// El punteado de una línea de vida: más corto que el de una flecha, para que
/// no compita con los mensajes.
const LIFELINE_DASH: (f64, f64) = (4.0, 4.0);
/// El punteado de una flecha declarada punteada.
const DASH_PATTERN: (f64, f64) = (6.0, 4.0);
/// El punteado del borde de un contenedor: dice "esto es una frontera", no una
/// caja más del dibujo.
const GROUP_DASH: (f64, f64) = (8.0, 5.0);

/// The only `SvgRenderer` of Graphium (PRD §8.5).
pub struct SvgRenderer {
    metrics: Arc<dyn TextMetrics>,
}

impl SvgRenderer {
    /// Builds a renderer around the single metrics source (PRD §8.4.1).
    pub fn new(metrics: Arc<dyn TextMetrics>) -> Self {
        Self { metrics }
    }

    /// The measurer behind this renderer, so the wiring layer can build the
    /// layout engine from the very same instance.
    pub fn metrics(&self) -> Arc<dyn TextMetrics> {
        Arc::clone(&self.metrics)
    }

    /// The drawing as a **self-contained** document: the same SVG the plan
    /// paints, plus the canvas rectangle in the palette colour (PRD §8.5.2,
    /// S7).
    ///
    /// The plan leaves the object transparent on purpose — the diagram floats
    /// over the grid — but an exported file is pasted somewhere else, and a
    /// light text over a transparent background is invisible when the reader
    /// has a white page. The rectangle uses the `.bg` rule the style block
    /// already carries, so the export shares the palette instead of inventing
    /// one.
    pub fn render_export(&self, ir: &Ir, layout: &LayoutGraph, palette: &Palette) -> String {
        self.render_with(ir, layout, palette, true)
    }

    fn render_with(
        &self,
        ir: &Ir,
        layout: &LayoutGraph,
        palette: &Palette,
        background: bool,
    ) -> String {
        let canvas = canvas_rect(layout);
        let mut out = String::new();

        out.push_str(&format!(
            "<svg xmlns=\"{SVG_NAMESPACE}\" class=\"graphium\" viewBox=\"{} {} {} {}\" width=\"{}\" height=\"{}\">\n",
            fmt_num(canvas.x),
            fmt_num(canvas.y),
            fmt_num(canvas.w),
            fmt_num(canvas.h),
            fmt_num(canvas.w),
            fmt_num(canvas.h),
        ));
        write_style(&mut out, palette);

        // The background goes right after the style block: under every layer,
        // so nothing it covers is lost.
        if background {
            out.push_str(&format!(
                "<rect class=\"bg\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>\n",
                fmt_num(canvas.x),
                fmt_num(canvas.y),
                fmt_num(canvas.w),
                fmt_num(canvas.h),
            ));
        }

        let graph = ir.graph();
        let timeline = ir.timeline();
        let routed = match graph {
            Some(graph) => routed_edges(graph, layout),
            // A timeline has no edges: every order of events is the order of
            // the columns, never a line between two boxes.
            None => Vec::new(),
        };
        write_defs(&mut out, &routed);

        // Los contenedores de un grafo van primero: son el fondo sobre el que
        // se leen las cajas. Sin ellos un diagrama de componentes es una nube
        // de cajas y no se ve qué piezas forman el mismo sistema.
        if graph.is_some() {
            out.push_str("<g class=\"layer-groups\">\n");
            for group in ordered_groups(layout) {
                write_group_box(&mut out, group);
            }
            out.push_str("</g>\n");
        }

        // Las bandas de carril van primero: son el fondo sobre el que se leen
        // los eventos. Sin ellas un swimlane es una nube de cajas y no se ve
        // quién hace qué (PRD §6, `timeline` y `swimlane`).
        if timeline.is_some() {
            out.push_str("<g class=\"layer-lanes\">\n");
            for group in ordered_groups(layout) {
                write_lane_band(&mut out, group);
            }
            out.push_str("</g>\n");
        }

        // Un diagrama de secuencia sin líneas de vida se lee desarmado: cada
        // flecha flota y no se ve a qué columna pertenece. Se dibujan del
        // borde inferior del actor hasta el fondo del dibujo, y van antes de
        // las aristas para que las flechas queden encima (PRD §6, `sequence`).
        if let Some(graph) = graph {
            if graph.layout_hint == LayoutHint::Lanes {
                out.push_str("<g class=\"layer-lifelines\">\n");
                for node in ordered_nodes(graph) {
                    if let Some(placed) = layout.nodes.get(&node.id) {
                        out.push_str(&format!(
                            "<line class=\"lifeline\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"/>\n",
                            fmt_num(placed.center.x),
                            fmt_num(placed.center.y + placed.size.h / 2.0),
                            fmt_num(placed.center.x),
                            fmt_num(canvas.max_y()),
                        ));
                    }
                }
                out.push_str("</g>\n");
            }
        }

        out.push_str("<g class=\"layer-edges\">\n");
        for (edge, route) in &routed {
            write_edge(&mut out, edge, route);
        }
        out.push_str("</g>\n");

        out.push_str("<g class=\"layer-nodes\">\n");
        if let Some(graph) = graph {
            for node in ordered_nodes(graph) {
                if let Some(placed) = layout.nodes.get(&node.id) {
                    write_node(&mut out, node, placed);
                }
            }
        }
        if let Some(timeline) = timeline {
            for event in ordered_events(timeline) {
                if let Some(placed) = layout.nodes.get(&event.id) {
                    write_event(&mut out, event, placed);
                }
            }
        }
        out.push_str("</g>\n");

        out.push_str("</svg>");
        out
    }
}

impl SvgRendererContract for SvgRenderer {
    fn render(&self, ir: &Ir, layout: &LayoutGraph, palette: &Palette) -> String {
        self.render_with(ir, layout, palette, false)
    }
}

/// The plan rectangle the root element covers.
fn canvas_rect(layout: &LayoutGraph) -> Rect {
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

fn node_rect(placed: &NodeBox) -> Rect {
    Rect::new(
        placed.center.x - placed.size.w / 2.0,
        placed.center.y - placed.size.h / 2.0,
        placed.size.w,
        placed.size.h,
    )
}

fn ordered_nodes(graph: &IrGraph) -> Vec<&IrNode> {
    let mut nodes: Vec<&IrNode> = graph.nodes.iter().collect();
    nodes.sort_by(|left, right| left.id.cmp(&right.id));
    nodes
}

/// The events of a timeline, in id order (PRD §8.5.1: never insertion order).
fn ordered_events(timeline: &IrTimeline) -> Vec<&IrEvent> {
    let mut events: Vec<&IrEvent> = timeline.events.iter().collect();
    events.sort_by(|left, right| left.id.cmp(&right.id));
    events
}

/// The lane bands of a timeline, in id order.
fn ordered_groups(layout: &LayoutGraph) -> Vec<&LayoutGroup> {
    let mut groups: Vec<&LayoutGroup> = layout.groups.iter().collect();
    groups.sort_by(|left, right| left.id.cmp(&right.id));
    groups
}

/// The edges that can be drawn, in id order.
fn routed_edges<'a>(
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

fn write_style(out: &mut String, palette: &Palette) {
    let border = fmt_num(TOKENS.border_width);
    let rules: [(&str, String); 20] = [
        (".bg", format!("fill:{}", palette.bg.css())),
        (".node-fill", format!("fill:{}", palette.node_fill.css())),
        (
            ".node-stroke",
            format!("stroke:{};stroke-width:{border}", palette.node_stroke.css()),
        ),
        (".node-text", format!("fill:{}", palette.node_text.css())),
        (
            ".edge-stroke",
            format!(
                "stroke:{};stroke-width:{border};fill:none",
                palette.edge_stroke.css()
            ),
        ),
        (".edge-text", format!("fill:{}", palette.edge_text.css())),
        (
            ".accent",
            format!(
                "fill:{};stroke:{}",
                palette.accent.css(),
                palette.accent.css()
            ),
        ),
        (
            ".emphasis-highlight",
            format!("stroke:{}", palette.accent.css()),
        ),
        (
            ".emphasis-highlight-text",
            format!("fill:{}", palette.accent.css()),
        ),
        (
            ".emphasis-muted",
            format!("opacity:{}", fmt_num(MUTED_OPACITY)),
        ),
        (
            ".group-box",
            format!(
                "fill:{};fill-opacity:0.12;stroke:{};stroke-width:{};stroke-dasharray:{} {}",
                palette.node_fill.css(),
                palette.node_stroke.css(),
                border,
                fmt_num(GROUP_DASH.0),
                fmt_num(GROUP_DASH.1),
            ),
        ),
        (".group-label", format!("fill:{}", palette.edge_text.css())),
        (
            ".lane-band",
            format!(
                "fill:{};fill-opacity:0.35;stroke:{};stroke-width:{border}",
                palette.node_fill.css(),
                palette.node_stroke.css(),
            ),
        ),
        (".lane-label", format!("fill:{}", palette.edge_text.css())),
        (
            ".lifeline",
            format!(
                "stroke:{};stroke-width:{border};stroke-dasharray:{} {};fill:none",
                palette.node_stroke.css(),
                fmt_num(LIFELINE_DASH.0),
                fmt_num(LIFELINE_DASH.1),
            ),
        ),
        (
            ".stroke-dashed",
            format!(
                "stroke-dasharray:{} {}",
                fmt_num(DASH_PATTERN.0),
                fmt_num(DASH_PATTERN.1)
            ),
        ),
        (".text", String::from("font-family:Inter, sans-serif")),
        (
            ".arrow-normal",
            format!("fill:{}", palette.edge_stroke.css()),
        ),
        (".arrow-highlight", format!("fill:{}", palette.accent.css())),
        (
            ".arrow-muted",
            format!(
                "fill:{};opacity:{}",
                palette.edge_stroke.css(),
                fmt_num(MUTED_OPACITY)
            ),
        ),
    ];
    out.push_str("<style>\n");
    for (selector, body) in rules {
        out.push_str(selector);
        out.push('{');
        out.push_str(&body);
        out.push_str("}\n");
    }
    out.push_str("</style>\n");
}

fn marker_id(emphasis: Emphasis) -> &'static str {
    match emphasis {
        Emphasis::Normal => "arrow-normal",
        Emphasis::Highlight => "arrow-highlight",
        Emphasis::Muted => "arrow-muted",
    }
}

fn write_defs(out: &mut String, routed: &[(&IrEdge, &LayoutEdge)]) {
    // Only the markers the document references, sorted by id and never by
    // insertion order (PRD §8.5.1).
    let mut used: BTreeSet<&'static str> = BTreeSet::new();
    for (edge, _) in routed {
        if edge.arrow {
            used.insert(marker_id(edge.emphasis));
        }
    }
    out.push_str("<defs>\n");
    for id in used {
        write_marker(out, id);
    }
    out.push_str("</defs>\n");
}

fn write_marker(out: &mut String, id: &str) {
    let zero = fmt_num(0.0);
    let length = fmt_num(ARROW_LENGTH);
    let half = fmt_num(ARROW_LENGTH / 2.0);
    out.push_str(&format!(
        "<marker id=\"{id}\" viewBox=\"{zero} {zero} {length} {length}\" refX=\"{length}\" refY=\"{half}\" markerWidth=\"{length}\" markerHeight=\"{length}\" markerUnits=\"userSpaceOnUse\" orient=\"auto\">\n\
         <path class=\"{id}\" d=\"M {zero} {zero} L {length} {half} L {zero} {length} Z\"/>\n\
         </marker>\n"
    ));
}

fn write_edge(out: &mut String, edge: &IrEdge, route: &LayoutEdge) {
    let points: Vec<String> = route
        .points
        .iter()
        .map(|point| format!("{},{}", fmt_num(point.x), fmt_num(point.y)))
        .collect();
    if points.is_empty() {
        return;
    }

    let mut classes = String::from("edge-stroke");
    if edge.style == Style::Dashed {
        classes.push_str(" stroke-dashed");
    }
    push_emphasis(&mut classes, edge.emphasis, false);

    let marker = if edge.arrow {
        format!(" marker-end=\"url(#{})\"", marker_id(edge.emphasis))
    } else {
        String::new()
    };
    let id = escape_text(&format!("m-{}-{}", edge.from, edge.to));
    out.push_str(&format!(
        "<polyline id=\"{id}\" class=\"{classes}\" points=\"{}\"{marker}/>\n",
        points.join(" ")
    ));

    // The crow's foot: the end of the line says how many rows of the other
    // table can point here, and whether zero is allowed (PRD §6.1).
    if let Some(cardinality) = edge.cardinality {
        write_cardinality(out, cardinality, route);
    }

    if let (Some(label), Some(anchor)) = (edge.label.as_deref(), route.label_at) {
        let mut text_classes = String::from("edge-text text");
        push_emphasis(&mut text_classes, edge.emphasis, true);
        out.push_str(&format!(
            "<text class=\"{text_classes}\" x=\"{}\" y=\"{}\" font-size=\"{}\" text-anchor=\"middle\" dominant-baseline=\"central\">{}</text>\n",
            fmt_num(anchor.x),
            fmt_num(anchor.y),
            fmt_num(TOKENS.text_size),
            escape_text(label),
        ));
    }
}

fn write_node(out: &mut String, node: &IrNode, placed: &NodeBox) {
    let rect = node_rect(placed);
    let id = escape_text(&format!("g-{}", node.id));
    let mut classes = String::from("node-fill node-stroke ");
    classes.push_str(shape_class(node.shape));
    if node.style == Style::Dashed {
        classes.push_str(" stroke-dashed");
    }
    push_emphasis(&mut classes, node.emphasis, false);

    match node.shape {
        Shape::Box => write_rect(out, &id, &classes, &rect, None),
        Shape::Rounded => write_rect(out, &id, &classes, &rect, Some(TOKENS.corner_radius)),
        Shape::Cylinder => write_cylinder(out, &id, &classes, &rect),
        Shape::Diamond => write_diamond(out, &id, &classes, &rect),
        Shape::Ellipse => write_ellipse(out, &id, &classes, &rect),
        Shape::Note => write_note(out, &id, &classes, &rect),
    }
    if node.rows.is_empty() {
        write_node_text(out, node, placed);
    } else {
        write_node_table(out, node, placed);
    }
}

/// Size of the glyphs of the crow's foot, in plan units.///
/// The legs of the foot are wide and short: a narrow fan reads as an
/// arrowhead, which is the one thing a crow's foot must not look like.
const CROW_FOOT: f64 = 22.0;

/// How wide the single stroke of the "one" side opens.
const CROW_FOOT_SPREAD: f64 = 0.85;

/// Horizontal space reserved for the `PK` / `FK` badge of a row.
const BADGE_WIDTH: f64 = 26.0;

/// Draws the two ends of a relation with the crow's foot notation.
///
/// Each end says how many rows of the other table can point here and whether
/// zero is allowed, which is what makes a schema diagram readable (PRD §6.1).
fn write_cardinality(out: &mut String, cardinality: gx_core::ir::Cardinality, route: &LayoutEdge) {
    let (Some(first), Some(last)) = (route.points.first(), route.points.last()) else {
        return;
    };

    // The direction of the line at each end, pointing into the box.
    let end_direction = direction_at(route, true);
    let start_direction = direction_at(route, false);

    let mut classes = String::from("cardinality");
    if cardinality.to_min == gx_core::ir::CardinalityBound::Optional {
        classes.push_str(" cardinality-optional");
    }

    write_crow_foot(
        out,
        *last,
        end_direction,
        cardinality.to_min,
        cardinality.to_max,
        &classes,
    );
    write_crow_foot(
        out,
        *first,
        start_direction,
        cardinality.from_min,
        cardinality.from_max,
        &classes,
    );
}

/// The unit direction of the line at one of its ends.
fn direction_at(route: &LayoutEdge, at_end: bool) -> (f64, f64) {
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

/// Draws the glyphs of one end: the circle when zero rows are allowed and the
/// fork when many rows are.
fn write_crow_foot(
    out: &mut String,
    at: gx_core::Point,
    direction: (f64, f64),
    bound: gx_core::ir::CardinalityBound,
    max: gx_core::ir::CardinalityMax,
    classes: &str,
) {
    let (dx, dy) = direction;
    let back = |distance: f64| gx_core::Point::new(at.x - dx * distance, at.y - dy * distance);
    let side = |point: gx_core::Point, offset: f64| {
        gx_core::Point::new(point.x - dy * offset, point.y + dx * offset)
    };
    match max {
        gx_core::ir::CardinalityMax::Many => {
            // The crow's foot opens TOWARDS the table: the ankle sits away
            // from it and the three toes touch its edge. Opening away from
            // the table draws an arrowhead, which is the opposite meaning.
            let ankle = back(CROW_FOOT);
            let toe = |spread: f64| side(at, spread);
            let stroke = |to: gx_core::Point| {
                format!(
                    "M {} {} L {} {} ",
                    fmt_num(ankle.x),
                    fmt_num(ankle.y),
                    fmt_num(to.x),
                    fmt_num(to.y)
                )
            };
            let spread = CROW_FOOT * 0.62;
            let mut path = String::new();
            path.push_str(&stroke(at));
            path.push_str(&stroke(toe(spread)));
            path.push_str(&stroke(toe(-spread)));
            out.push_str(&format!(
                "<path class=\"{classes}\" d=\"{}\"/>\n",
                path.trim_end()
            ));
        }
        gx_core::ir::CardinalityMax::One => {
            // Exactly one: a single stroke across the relation.
            let mark = back(CROW_FOOT * 0.6);
            let half = CROW_FOOT * CROW_FOOT_SPREAD;
            let from = side(mark, half);
            let to = side(mark, -half);
            out.push_str(&format!(
                "<line class=\"{classes}\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"/>\n",
                fmt_num(from.x),
                fmt_num(from.y),
                fmt_num(to.x),
                fmt_num(to.y)
            ));
        }
    }

    if bound == gx_core::ir::CardinalityBound::Optional {
        let center = back(CROW_FOOT * 1.45);
        out.push_str(&format!(
            "<circle class=\"{classes}\" cx=\"{}\" cy=\"{}\" r=\"{}\"/>\n",
            fmt_num(center.x),
            fmt_num(center.y),
            fmt_num(CROW_FOOT / 3.5)
        ));
    }
}

/// Draws a node that has rows: a header with the label, a rule, and one line
/// per row, with the key role on the left (PRD §6.1: a table is a node whose
/// rows are its columns).
fn write_node_table(out: &mut String, node: &IrNode, placed: &NodeBox) {
    let rect = node_rect(placed);
    let padding = TOKENS.node_padding;
    let line = TOKENS.text_size * TOKENS.line_height;

    // The header: the label, centered, in the first line of the box.
    let header_y = rect.y + padding + line / 2.0;
    let mut label_classes = String::from("node-text text");
    push_emphasis(&mut label_classes, node.emphasis, true);
    out.push_str(&format!(
        "<text class=\"{label_classes}\" x=\"{}\" y=\"{}\" font-size=\"{}\" text-anchor=\"middle\" dominant-baseline=\"central\" font-weight=\"{}\">{}</text>\n",
        fmt_num(rect.center().x),
        fmt_num(header_y),
        fmt_num(TOKENS.text_size),
        TOKENS.semibold_weight,
        escape_text(&node.label),
    ));

    // The rule that separates the header from the rows.
    let rule_y = header_y + line / 2.0;
    out.push_str(&format!(
        "<line class=\"node-stroke\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke-width=\"{}\"/>\n",
        fmt_num(rect.x),
        fmt_num(rule_y),
        fmt_num(rect.max_x()),
        fmt_num(rule_y),
        fmt_num(TOKENS.border_width),
    ));

    // One line per row: the key role on the left, the name, the note right.
    for (index, row) in node.rows.iter().enumerate() {
        let y = rule_y + line * (index as f64 + 0.5);
        let badge = match row.key {
            gx_core::ir::RowKey::Pk => "PK".to_string(),
            gx_core::ir::RowKey::Fk => "FK".to_string(),
            gx_core::ir::RowKey::PkFk => "PK, FK".to_string(),
            gx_core::ir::RowKey::None => String::new(),
        };
        let mut name_classes = String::from("node-text text");
        if matches!(row.key, gx_core::ir::RowKey::Pk | gx_core::ir::RowKey::PkFk) {
            name_classes.push_str(" row-key");
        }
        if row.nullable {
            name_classes.push_str(" row-nullable");
        }
        out.push_str(&format!(
            "<text class=\"row-badge text\" x=\"{}\" y=\"{}\" font-size=\"{}\" dominant-baseline=\"central\">{}</text>\n",
            fmt_num(rect.x + padding / 2.0),
            fmt_num(y),
            fmt_num(TOKENS.text_size * 0.85),
            escape_text(&badge),
        ));
        out.push_str(&format!(
            "<text class=\"{name_classes}\" x=\"{}\" y=\"{}\" font-size=\"{}\" dominant-baseline=\"central\">{}</text>\n",
            fmt_num(rect.x + padding + BADGE_WIDTH),
            fmt_num(y),
            fmt_num(TOKENS.text_size),
            escape_text(&row.text),
        ));
        if let Some(note) = &row.note {
            out.push_str(&format!(
                "<text class=\"row-note text\" x=\"{}\" y=\"{}\" font-size=\"{}\" text-anchor=\"end\" dominant-baseline=\"central\">{}</text>\n",
                fmt_num(rect.max_x() - padding / 2.0),
                fmt_num(y),
                fmt_num(TOKENS.text_size * 0.9),
                escape_text(note),
            ));
        }
    }
}

/// Adds the classes an emphasis needs. Highlighting a shape changes its stroke
/// and highlighting a text changes its fill, so the two need their own class.
fn push_emphasis(classes: &mut String, emphasis: Emphasis, for_text: bool) {
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

fn shape_class(shape: Shape) -> &'static str {
    match shape {
        Shape::Box => "shape-box",
        Shape::Rounded => "shape-rounded",
        Shape::Cylinder => "shape-cylinder",
        Shape::Diamond => "shape-diamond",
        Shape::Ellipse => "shape-ellipse",
        Shape::Note => "shape-note",
    }
}

fn write_rect(out: &mut String, id: &str, classes: &str, rect: &Rect, radius: Option<f64>) {
    let (x, y) = (fmt_num(rect.x), fmt_num(rect.y));
    let (width, height) = (fmt_num(rect.w), fmt_num(rect.h));
    match radius {
        Some(radius) => out.push_str(&format!(
            "<rect id=\"{id}\" class=\"{classes}\" x=\"{x}\" y=\"{y}\" width=\"{width}\" height=\"{height}\" rx=\"{}\" ry=\"{}\"/>\n",
            fmt_num(radius),
            fmt_num(radius),
        )),
        None => out.push_str(&format!(
            "<rect id=\"{id}\" class=\"{classes}\" x=\"{x}\" y=\"{y}\" width=\"{width}\" height=\"{height}\"/>\n"
        )),
    }
}

fn write_cylinder(out: &mut String, id: &str, classes: &str, rect: &Rect) {
    // The rim height is the corner radius, capped to a quarter of the box so a
    // short node still has a readable body. Safe default for T4.
    let rim = TOKENS.corner_radius.min(rect.h / 4.0);
    // Cubics instead of elliptical arcs: an arc flag is a bare `0`/`1`, which
    // would break the two-decimal rule of PRD §8.5.1 for path data.
    let bulge = rim * 4.0 / 3.0;
    let left = rect.x;
    let right = rect.x + rect.w;
    let top = rect.y + rim;
    let bottom = rect.y + rect.h - rim;
    out.push_str(&format!(
        "<path id=\"{id}\" class=\"{classes}\" d=\"M {} {} C {} {} {} {} {} {} L {} {} C {} {} {} {} {} {} Z\"/>\n",
        fmt_num(left),
        fmt_num(top),
        fmt_num(left),
        fmt_num(top - bulge),
        fmt_num(right),
        fmt_num(top - bulge),
        fmt_num(right),
        fmt_num(top),
        fmt_num(right),
        fmt_num(bottom),
        fmt_num(right),
        fmt_num(bottom + bulge),
        fmt_num(left),
        fmt_num(bottom + bulge),
        fmt_num(left),
        fmt_num(bottom),
    ));
    out.push_str(&format!(
        "<path class=\"node-stroke shape-cylinder\" fill=\"none\" d=\"M {} {} C {} {} {} {} {} {}\"/>\n",
        fmt_num(left),
        fmt_num(top),
        fmt_num(left),
        fmt_num(top + bulge),
        fmt_num(right),
        fmt_num(top + bulge),
        fmt_num(right),
        fmt_num(top),
    ));
}

fn write_diamond(out: &mut String, id: &str, classes: &str, rect: &Rect) {
    let left = fmt_num(rect.x);
    let right = fmt_num(rect.x + rect.w);
    let top = fmt_num(rect.y);
    let bottom = fmt_num(rect.y + rect.h);
    let center_x = fmt_num(rect.x + rect.w / 2.0);
    let center_y = fmt_num(rect.y + rect.h / 2.0);
    out.push_str(&format!(
        "<polygon id=\"{id}\" class=\"{classes}\" points=\"{center_x},{top} {right},{center_y} {center_x},{bottom} {left},{center_y}\"/>\n"
    ));
}

fn write_ellipse(out: &mut String, id: &str, classes: &str, rect: &Rect) {
    out.push_str(&format!(
        "<ellipse id=\"{id}\" class=\"{classes}\" cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\"/>\n",
        fmt_num(rect.x + rect.w / 2.0),
        fmt_num(rect.y + rect.h / 2.0),
        fmt_num(rect.w / 2.0),
        fmt_num(rect.h / 2.0),
    ));
}

fn write_note(out: &mut String, id: &str, classes: &str, rect: &Rect) {
    let fold = TOKENS.corner_radius.min(rect.w / 4.0).min(rect.h / 4.0);
    let left = fmt_num(rect.x);
    let right = fmt_num(rect.x + rect.w);
    let top = fmt_num(rect.y);
    let bottom = fmt_num(rect.y + rect.h);
    let fold_x = fmt_num(rect.x + rect.w - fold);
    let fold_y = fmt_num(rect.y + fold);
    out.push_str(&format!(
        "<path id=\"{id}\" class=\"{classes}\" d=\"M {left} {top} L {fold_x} {top} L {right} {fold_y} L {right} {bottom} L {left} {bottom} Z\"/>\n"
    ));
    out.push_str(&format!(
        "<path class=\"node-stroke shape-note\" fill=\"none\" d=\"M {fold_x} {top} L {fold_x} {fold_y} L {right} {fold_y}\"/>\n"
    ));
}

fn write_node_text(out: &mut String, node: &IrNode, placed: &NodeBox) {
    let mut classes = String::from("node-text text");
    push_emphasis(&mut classes, node.emphasis, true);
    write_centered_text(out, &classes, &node.label, placed);
}

/// Draws a label centered on its box, one `<text>` per line.
///
/// A node and a timeline event are the same thing for the reader —a box with
/// its name— so they share this writer instead of drifting apart.
fn write_centered_text(out: &mut String, classes: &str, label: &str, placed: &NodeBox) {
    let lines: Vec<&str> = label.lines().collect();
    if lines.is_empty() {
        return;
    }
    let size = fmt_num(TOKENS.text_size);
    let center_x = fmt_num(placed.center.x);

    if lines.len() == 1 {
        out.push_str(&format!(
            "<text class=\"{classes}\" x=\"{center_x}\" y=\"{}\" font-size=\"{size}\" text-anchor=\"middle\" dominant-baseline=\"central\">{}</text>\n",
            fmt_num(placed.center.y),
            escape_text(lines[0]),
        ));
        return;
    }

    out.push_str(&format!(
        "<text class=\"{classes}\" font-size=\"{size}\" text-anchor=\"middle\" dominant-baseline=\"central\">\n"
    ));
    let line_height = TOKENS.text_size * TOKENS.line_height;
    let total = line_height * lines.len() as f64;
    for (index, line) in lines.iter().enumerate() {
        let baseline = placed.center.y - total / 2.0 + line_height * (index as f64 + 0.5);
        out.push_str(&format!(
            "<tspan x=\"{center_x}\" y=\"{}\">{}</tspan>\n",
            fmt_num(baseline),
            escape_text(line)
        ));
    }
    out.push_str("</text>\n");
}

/// Draws one group of a graph: its container and its name, top left.
fn write_group_box(out: &mut String, group: &LayoutGroup) {
    let id = escape_text(&format!("cluster-{}", group.id));
    write_rect(
        out,
        &id,
        "group-box",
        &group.bbox,
        Some(TOKENS.corner_radius),
    );

    let Some(label) = group
        .label
        .as_deref()
        .filter(|label| !label.trim().is_empty())
    else {
        return;
    };
    out.push_str(&format!(
        "<text class=\"group-label text\" x=\"{}\" y=\"{}\" font-size=\"{}\" text-anchor=\"start\" dominant-baseline=\"central\">{}</text>\n",
        fmt_num(group.bbox.x + TOKENS.node_padding),
        fmt_num(group.bbox.y + TOKENS.text_size),
        fmt_num(TOKENS.text_size),
        escape_text(label),
    ));
}

/// Draws one lane of a timeline: its band and its name in the left gutter.
///
/// The band is what tells the reader which row an event belongs to; the name
/// sits in the gutter the layout reserved for it, so it never rides on top of
/// the first event of the row.
fn write_lane_band(out: &mut String, group: &LayoutGroup) {
    let id = escape_text(&format!("lane-{}", group.id));
    write_rect(out, &id, "lane-band", &group.bbox, None);

    let Some(label) = group
        .label
        .as_deref()
        .filter(|label| !label.trim().is_empty())
    else {
        return;
    };
    out.push_str(&format!(
        "<text class=\"lane-label text\" x=\"{}\" y=\"{}\" font-size=\"{}\" text-anchor=\"start\" dominant-baseline=\"central\">{}</text>\n",
        fmt_num(group.bbox.x + TOKENS.node_padding),
        fmt_num(group.bbox.y + group.bbox.h / 2.0),
        fmt_num(TOKENS.text_size),
        escape_text(label),
    ));
}

/// Draws one event of a timeline: a box with its label, inside its lane.
fn write_event(out: &mut String, event: &IrEvent, placed: &NodeBox) {
    let rect = node_rect(placed);
    let id = escape_text(&format!("g-{}", event.id));
    write_rect(
        out,
        &id,
        "node-fill node-stroke shape-rounded",
        &rect,
        Some(TOKENS.corner_radius),
    );
    write_centered_text(out, "node-text text", &event.label, placed);
}

#[cfg(test)]
mod tests;
