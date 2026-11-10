//! The deterministic SVG writer (PRD §8.5, §8.5.1, §8.5.2).
//!
//! This module is **one of the two emitters** of the drawing
//! (`f11-elementos.md`, T1). It walks the same [`Drawing`] plan the scene
//! builder walks ([`crate::scene`]) and spells it as markup; the scene builder
//! spells the very same plan as data for the window. Everything that is not
//! "how do I write this in SVG" — the box of a node, the order of the layers,
//! the ids of the drawing, the lines of a label, the geometry of a crow's foot,
//! the frozen tokens — lives in [`crate::geometry`], so the two emitters cannot
//! drift apart.
//!
//! The SVG of this emitter is the **export artifact** (S2): the goldens of
//! `crates/gx-app/tests/golden_svg.rs` compare it byte by byte and the window
//! no longer depends on it.
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

use gx_core::scene::BandKind;
use gx_core::{
    fmt_num, Cardinality, CardinalityBound, CardinalityMax, Ir, IrEdge, IrEvent, IrNode,
    LayoutEdge, LayoutGraph, LayoutGroup, NodeBox, Palette, Scene, Shape, Style,
    SvgRenderer as SvgRendererContract, TextMetrics, TOKENS,
};

use crate::escape::escape_text;
use crate::geometry::{
    self, band_label, centered_baselines, cluster_id, crow_foot_mark, edge_id, label_lines,
    lane_id, marker_id, node_id, node_rect, points_text, push_emphasis, shape_class, CrowFoot,
    Table, ARROW_LENGTH, DASH_PATTERN, GROUP_DASH, LIFELINE_DASH, MUTED_OPACITY,
};
use crate::scene::Drawing;

/// Namespace of the emitted document, so the SVG parses on its own (S12).
const SVG_NAMESPACE: &str = "http://www.w3.org/2000/svg";

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

    /// The drawing as data, beside the SVG string (`f11-elementos.md`, T1).
    ///
    /// It is the second emitter of the same [`Drawing`] plan: the ids, the
    /// order of the layers and the numbers are the ones the string carries.
    pub fn scene(&self, ir: &Ir, layout: &LayoutGraph, palette: &Palette) -> Scene {
        crate::scene::scene(ir, layout, palette)
    }

    fn render_with(
        &self,
        ir: &Ir,
        layout: &LayoutGraph,
        palette: &Palette,
        background: bool,
    ) -> String {
        // Los dos emisores del dibujo leen el mismo plan: éste lo escribe como
        // markup, el serializador de elementos como datos.
        let drawing = Drawing::of(ir, layout);
        let canvas = drawing.canvas;
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

        write_defs(&mut out, &drawing.edges);

        // Los contenedores de un grafo van primero: son el fondo sobre el que
        // se leen las cajas. Sin ellos un diagrama de componentes es una nube
        // de cajas y no se ve qué piezas forman el mismo sistema.
        if drawing.draws_clusters() {
            out.push_str("<g class=\"layer-groups\">\n");
            for group in &drawing.clusters {
                write_band(&mut out, BandKind::Cluster, group);
            }
            out.push_str("</g>\n");
        }

        // Las bandas de carril van primero: son el fondo sobre el que se leen
        // los eventos. Sin ellas un swimlane es una nube de cajas y no se ve
        // quién hace qué (PRD §6, `timeline` y `swimlane`).
        if drawing.draws_lanes() {
            out.push_str("<g class=\"layer-lanes\">\n");
            for group in &drawing.lanes {
                write_band(&mut out, BandKind::Lane, group);
            }
            out.push_str("</g>\n");
        }

        // Un diagrama de secuencia sin líneas de vida se lee desarmado: cada
        // flecha flota y no se ve a qué columna pertenece. Se dibujan del
        // borde inferior del actor hasta el fondo del dibujo, y van antes de
        // las aristas para que las flechas queden encima (PRD §6, `sequence`).
        if drawing.draws_lifelines() {
            out.push_str("<g class=\"layer-lifelines\">\n");
            for (_, placed) in &drawing.nodes {
                out.push_str(&format!(
                    "<line class=\"lifeline\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"/>\n",
                    fmt_num(geometry::lifeline_x(placed)),
                    fmt_num(geometry::lifeline_start(placed)),
                    fmt_num(geometry::lifeline_x(placed)),
                    fmt_num(geometry::lifeline_end(&canvas)),
                ));
            }
            out.push_str("</g>\n");
        }

        out.push_str("<g class=\"layer-edges\">\n");
        for (edge, route) in &drawing.edges {
            write_edge(&mut out, edge, route);
        }
        out.push_str("</g>\n");

        out.push_str("<g class=\"layer-nodes\">\n");
        for (node, placed) in &drawing.nodes {
            write_node(&mut out, node, placed);
        }
        for (event, placed) in &drawing.events {
            write_event(&mut out, event, placed);
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

    /// The drawing as **self-contained** document: the same SVG the plan
    /// paints, plus the canvas rectangle in the palette colour (PRD §8.5.2,
    /// S7).
    ///
    /// The plan leaves the object transparent on purpose — the diagram floats
    /// over the grid — but an exported file is pasted somewhere else, and a
    /// light text over a transparent background is invisible when the reader
    /// has a white page. The rectangle uses the `.bg` rule the style block
    /// already carries, so the export shares the palette instead of inventing
    /// one.
    ///
    /// It lives in the trait impl on purpose: the door reaches the renderer
    /// through a `dyn SvgRenderer`, so an inherent method of the same name
    /// would shadow nothing and never run.
    fn render_export(&self, ir: &Ir, layout: &LayoutGraph, palette: &Palette) -> String {
        self.render_with(ir, layout, palette, true)
    }

    /// The structured drawing beside the string (`f11-elementos.md`, T1/S3).
    fn scene(&self, ir: &Ir, layout: &LayoutGraph, palette: &Palette) -> Option<Scene> {
        Some(SvgRenderer::scene(self, ir, layout, palette))
    }
}

fn write_style(out: &mut String, palette: &Palette) {
    let border = fmt_num(TOKENS.border_width);
    let rules: [(&str, String); 25] = [
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
            ".cardinality",
            format!(
                "fill:none;stroke:{};stroke-width:{border}",
                palette.edge_stroke.css()
            ),
        ),
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
        // The rows of a table (`f21-roto.md`, S2): the same rules the window
        // CSS carries (app.css, PRD §6.1), so the exported artifact paints
        // badges, notes, keys and nullable rows like the window does. Without
        // them the export drew row text black on the dark background.
        (".row-badge", format!("fill:{}", palette.edge_text.css())),
        (".row-note", format!("fill:{}", palette.edge_text.css())),
        (".row-key", String::from("font-weight:600")),
        (".row-nullable", String::from("opacity:0.75")),
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

fn write_defs(out: &mut String, routed: &[(IrEdge, LayoutEdge)]) {
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
    let points = points_text(&route.points);
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
    let id = escape_text(&edge_id(&edge.from, &edge.to));
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
    let id = escape_text(&node_id(&node.id));
    let mut classes = String::from("node-fill node-stroke ");
    classes.push_str(&shape_class(node.shape));
    if node.style == Style::Dashed {
        classes.push_str(" stroke-dashed");
    }
    push_emphasis(&mut classes, node.emphasis, false);

    write_shape(out, &id, &classes, node.shape, &rect);
    if node.rows.is_empty() || node.shape == Shape::Circle {
        write_node_text(out, node, placed);
    } else {
        write_node_table(out, node, placed);
    }
}

/// Draws the glyph of one shape inside its rectangle.
///
/// The box of a node and the box of an event are the same drawing — a
/// rectangle the reader fills with a name — so the dispatch lives here once and
/// both go through it (S12).
fn write_shape(out: &mut String, id: &str, classes: &str, shape: Shape, rect: &gx_core::Rect) {
    match shape {
        Shape::Box => write_rect(out, id, classes, rect, None),
        Shape::Rounded => write_rect(out, id, classes, rect, Some(TOKENS.corner_radius)),
        Shape::Cylinder => write_cylinder(out, id, classes, rect),
        Shape::Diamond => write_diamond(out, id, classes, rect),
        Shape::Ellipse => write_ellipse(out, id, classes, rect),
        Shape::Note => write_note(out, id, classes, rect),
        Shape::Circle => write_circle(out, id, classes, &geometry::glyph_rect(shape, rect)),
    }
}

/// Draws the two ends of a relation with the crow's foot notation.
///
/// Each end says how many rows of the other table can point here and whether
/// zero is allowed, which is what makes a schema diagram readable (PRD §6.1).
fn write_cardinality(out: &mut String, cardinality: Cardinality, route: &LayoutEdge) {
    let (Some(first), Some(last)) = (route.points.first(), route.points.last()) else {
        return;
    };

    // The direction of the line at each end, pointing into the box.
    let end_direction = geometry::direction_at(route, true);
    let start_direction = geometry::direction_at(route, false);

    let mut classes = String::from("cardinality");
    if cardinality.to_min == CardinalityBound::Optional {
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

/// Draws the glyphs of one end: the circle when zero rows are allowed and the
/// fork when many rows are. The points come from [`crate::geometry`], the same
/// numbers the painter gets in the tokens (S5).
fn write_crow_foot(
    out: &mut String,
    at: gx_core::Point,
    direction: (f64, f64),
    bound: CardinalityBound,
    max: CardinalityMax,
    classes: &str,
) {
    let mark = crow_foot_mark(
        at,
        direction,
        bound == CardinalityBound::Optional,
        max == CardinalityMax::Many,
    );
    match mark.glyph {
        CrowFoot::Many { ankle, toes } => {
            let stroke = |to: gx_core::Point| {
                format!(
                    "M {} {} L {} {} ",
                    fmt_num(ankle.x),
                    fmt_num(ankle.y),
                    fmt_num(to.x),
                    fmt_num(to.y)
                )
            };
            let mut path = String::new();
            path.push_str(&stroke(at));
            for toe in toes {
                path.push_str(&stroke(toe));
            }
            out.push_str(&format!(
                "<path class=\"{classes}\" d=\"{}\"/>\n",
                path.trim_end()
            ));
        }
        CrowFoot::One { from, to } => {
            out.push_str(&format!(
                "<line class=\"{classes}\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"/>\n",
                fmt_num(from.x),
                fmt_num(from.y),
                fmt_num(to.x),
                fmt_num(to.y)
            ));
        }
    }

    if let Some(center) = mark.optional {
        out.push_str(&format!(
            "<circle class=\"{classes}\" cx=\"{}\" cy=\"{}\" r=\"{}\"/>\n",
            fmt_num(center.x),
            fmt_num(center.y),
            fmt_num(mark.radius)
        ));
    }
}

/// Draws a node that has rows: a header with the label, a rule, and one line
/// per row, with the key role on the left (PRD §6.1: a table is a node whose
/// rows are its columns).
fn write_node_table(out: &mut String, node: &IrNode, placed: &NodeBox) {
    let rect = node_rect(placed);
    // The badge column exists only when some row has a key to put in it (S13).
    let keys = node.rows.iter().any(|row| Table::badge(row.key).is_some());
    let table = Table::of(&rect, keys);

    // The header: the label, centered, in the first line of the box.
    let mut label_classes = String::from("node-text text");
    push_emphasis(&mut label_classes, node.emphasis, true);
    write_lines(
        out,
        &label_classes,
        &[node.label.as_str()],
        TextAt::header(rect.center().x, table.header_y),
    );

    // The rule that separates the header from the rows.
    out.push_str(&format!(
        "<line class=\"node-stroke\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke-width=\"{}\"/>\n",
        fmt_num(rect.x),
        fmt_num(table.rule_y),
        fmt_num(rect.max_x()),
        fmt_num(table.rule_y),
        fmt_num(TOKENS.border_width),
    ));

    // One line per row: the key role on the left, the name, the note right.
    for (index, row) in node.rows.iter().enumerate() {
        let y = table.row_y(index);
        // A row without a key draws no badge: no empty element is written, and
        // a table without keys has no column to write it in (S13).
        if let (Some(badge), Some(badge_x)) = (Table::badge(row.key), table.badge_x) {
            out.push_str(&format!(
                "<text class=\"row-badge text\" x=\"{}\" y=\"{}\" font-size=\"{}\" dominant-baseline=\"central\">{}</text>\n",
                fmt_num(badge_x),
                fmt_num(y),
                fmt_num(TOKENS.text_size * 0.85),
                escape_text(badge),
            ));
        }
        out.push_str(&format!(
            "<text class=\"{}\" x=\"{}\" y=\"{}\" font-size=\"{}\" dominant-baseline=\"central\">{}</text>\n",
            geometry::row_name_classes(row),
            fmt_num(table.name_x),
            fmt_num(y),
            fmt_num(TOKENS.text_size),
            escape_text(&row.text),
        ));
        if let Some(note) = &row.note {
            out.push_str(&format!(
                "<text class=\"row-note text\" x=\"{}\" y=\"{}\" font-size=\"{}\" text-anchor=\"end\" dominant-baseline=\"central\">{}</text>\n",
                fmt_num(table.note_x),
                fmt_num(y),
                fmt_num(TOKENS.text_size * 0.9),
                escape_text(note),
            ));
        }
    }
}

fn write_rect(
    out: &mut String,
    id: &str,
    classes: &str,
    rect: &gx_core::Rect,
    radius: Option<f64>,
) {
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

fn write_cylinder(out: &mut String, id: &str, classes: &str, rect: &gx_core::Rect) {
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

fn write_diamond(out: &mut String, id: &str, classes: &str, rect: &gx_core::Rect) {
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

fn write_ellipse(out: &mut String, id: &str, classes: &str, rect: &gx_core::Rect) {
    out.push_str(&format!(
        "<ellipse id=\"{id}\" class=\"{classes}\" cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\"/>\n",
        fmt_num(rect.x + rect.w / 2.0),
        fmt_num(rect.y + rect.h / 2.0),
        fmt_num(rect.w / 2.0),
        fmt_num(rect.h / 2.0),
    ));
}

/// Draws the circle dot: one `<circle>` in the glyph rect
/// (`f19-formas.md`, S8).
fn write_circle(out: &mut String, id: &str, classes: &str, rect: &gx_core::Rect) {
    out.push_str(&format!(
        "<circle id=\"{id}\" class=\"{classes}\" cx=\"{}\" cy=\"{}\" r=\"{}\"/>\n",
        fmt_num(rect.center().x),
        fmt_num(rect.center().y),
        fmt_num(rect.w / 2.0),
    ));
}

fn write_note(out: &mut String, id: &str, classes: &str, rect: &gx_core::Rect) {
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
    let lines = label_lines(&node.label);
    match geometry::label_placement(node.shape, &node_rect(placed)) {
        geometry::LabelPlacement::Inside => write_lines(
            out,
            &classes,
            &lines,
            TextAt::centered(placed.center.x, placed.center.y),
        ),
        geometry::LabelPlacement::Beside { at } => {
            write_lines(out, &classes, &lines, TextAt::beside(at));
        }
    }
}

/// Where a block of text sits before it is spelled as SVG (S12).
///
/// The three writers this replaced differed only in these four numbers: the
/// column of the lines, the center of the block, which way the lines grow from
/// that column and, for the header of a table, a weight other than the regular
/// one.
#[derive(Clone, Copy)]
struct TextAt {
    x: f64,
    center_y: f64,
    anchor: Anchor,
    weight: Option<u16>,
}

impl TextAt {
    /// Centered on a box: the label of a node, of an event, of a table.
    fn centered(x: f64, center_y: f64) -> Self {
        Self {
            x,
            center_y,
            anchor: Anchor::Middle,
            weight: None,
        }
    }

    /// Starting at a point: the label beside a dot (`f19-formas.md`, S8).
    fn beside(at: gx_core::Point) -> Self {
        Self {
            x: at.x,
            center_y: at.y,
            anchor: Anchor::Start,
            weight: None,
        }
    }

    /// The header of a table: centered and semibold (PRD §6.1).
    fn header(x: f64, center_y: f64) -> Self {
        Self {
            weight: Some(TOKENS.semibold_weight),
            ..Self::centered(x, center_y)
        }
    }
}

/// Which way the lines of a text grow from its column.
#[derive(Clone, Copy)]
enum Anchor {
    Start,
    Middle,
}

impl Anchor {
    fn name(self) -> &'static str {
        match self {
            Anchor::Start => "start",
            Anchor::Middle => "middle",
        }
    }
}

/// Draws one text of the drawing: one `<text>` when it is a single line and one
/// `<tspan>` per line when it is more (PRD §8.5, S4).
///
/// A centered label, a label beside a dot and the header of a table are the
/// same thing for the reader — text the box holds — so they are written here
/// once instead of drifting apart (S12).
fn write_lines(out: &mut String, classes: &str, lines: &[&str], at: TextAt) {
    if lines.is_empty() {
        return;
    }
    let size = fmt_num(TOKENS.text_size);
    let x = fmt_num(at.x);
    let anchor = at.anchor.name();
    let weight = match at.weight {
        Some(weight) => format!(" font-weight=\"{weight}\""),
        None => String::new(),
    };

    if lines.len() == 1 {
        out.push_str(&format!(
            "<text class=\"{classes}\" x=\"{x}\" y=\"{}\" font-size=\"{size}\" text-anchor=\"{anchor}\" dominant-baseline=\"central\"{weight}>{}</text>\n",
            fmt_num(at.center_y),
            escape_text(lines[0]),
        ));
        return;
    }

    out.push_str(&format!(
        "<text class=\"{classes}\" font-size=\"{size}\" text-anchor=\"{anchor}\" dominant-baseline=\"central\"{weight}>\n"
    ));
    for (line, baseline) in lines
        .iter()
        .zip(centered_baselines(at.center_y, lines.len()))
    {
        out.push_str(&format!(
            "<tspan x=\"{x}\" y=\"{}\">{}</tspan>\n",
            fmt_num(baseline),
            escape_text(line)
        ));
    }
    out.push_str("</text>\n");
}

/// Draws one band of a drawing: a container of a graph or a lane of a timeline.
///
/// Both are a box behind elements plus a name in a corner. They differ in three
/// things — the classes, the corner radius and the point the name sits at — and
/// those three are all this writer needs, so the two hand-written copies became
/// one (S12).
fn write_band(out: &mut String, kind: BandKind, group: &LayoutGroup) {
    let (id, classes, radius, label_classes) = match kind {
        BandKind::Cluster => (
            cluster_id(&group.id),
            "group-box",
            Some(TOKENS.corner_radius),
            "group-label text",
        ),
        BandKind::Lane => (lane_id(&group.id), "lane-band", None, "lane-label text"),
    };
    let id = escape_text(&id);
    write_rect(out, &id, classes, &group.bbox, radius);

    let Some(label) = band_label(group) else {
        return;
    };
    let at = geometry::band_label_at(group, kind);
    write_lines(out, label_classes, &[label], TextAt::beside(at));
}

/// Draws one event of a timeline: a box with its label, inside its lane.
///
/// An event that lasts is also a bar (`f11-elementos.md`, T4/D): the lanes
/// engine already sizes its box from `IrEvent.span`, so the box is the length
/// of the bar and the drawing only had to say it. The numbers come from
/// [`geometry::event_bar`], the same rectangle the scene carries (S5).
fn write_event(out: &mut String, event: &IrEvent, placed: &NodeBox) {
    let rect = node_rect(placed);
    let id = escape_text(&node_id(&event.id));
    // An event is a rounded box: the glyph a rounded node goes through, so the
    // two cannot drift apart (S12).
    write_shape(
        out,
        &id,
        "node-fill node-stroke shape-rounded",
        Shape::Rounded,
        &rect,
    );
    if let Some(bar) = geometry::event_bar(&rect, event.span) {
        out.push_str(&format!(
            "<rect class=\"accent\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>\n",
            fmt_num(bar.x),
            fmt_num(bar.y),
            fmt_num(bar.w),
            fmt_num(bar.h),
        ));
    }
    write_lines(
        out,
        "node-text text",
        &label_lines(&event.label),
        TextAt::centered(placed.center.x, placed.center.y),
    );
}

#[cfg(test)]
mod tests;
