//! Behavioural tests of the deterministic SVG writer (PRD §8.5, §8.5.1, §9.1).

use std::sync::Arc;

use gx_core::{
    Ir, IrTimeline, LayoutGraph, Palette, Point, Rect, Shape, Size,
    SvgRenderer as SvgRendererContract,
};
use roxmltree::Document;

use super::SvgRenderer;
use crate::metrics::InterMetrics;
use crate::test_support as fixture;

/// Attribute names whose values must be canonical (PRD §8.5.1).
const NUMERIC_ATTRIBUTES: [&str; 24] = [
    "x",
    "y",
    "x1",
    "y1",
    "x2",
    "y2",
    "cx",
    "cy",
    "r",
    "rx",
    "ry",
    "width",
    "height",
    "font-size",
    "stroke-width",
    "dx",
    "dy",
    "points",
    "d",
    "refX",
    "refY",
    "markerWidth",
    "markerHeight",
    "viewBox",
];

fn engine() -> SvgRenderer {
    let metrics = InterMetrics::new().expect("the embedded Inter faces parse");
    SvgRenderer::new(Arc::new(metrics))
}

fn render(ir: &Ir, layout: &LayoutGraph, palette: &Palette) -> String {
    engine().render(ir, layout, palette)
}

fn render_fixture() -> String {
    render(
        &fixture::fixture_ir(),
        &fixture::fixture_layout(),
        &Palette::plano(),
    )
}

fn document(svg: &str) -> Document<'_> {
    Document::parse(svg).expect("the rendered SVG parses")
}

fn element<'a>(document: &'a Document<'a>, name: &str) -> Vec<roxmltree::Node<'a, 'a>> {
    document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == name)
        .collect()
}

fn classes(node: roxmltree::Node<'_, '_>) -> String {
    node.attribute("class").unwrap_or_default().to_string()
}

/// Mirrors `^-?\d+\.\d{2}$` without pulling a regex dependency (S7).
fn is_canonical_number(token: &str) -> bool {
    let digits = token.strip_prefix('-').unwrap_or(token);
    let mut parts = digits.split('.');
    let (integer, fraction) = match (parts.next(), parts.next(), parts.next()) {
        (Some(integer), Some(fraction), None) => (integer, fraction),
        _ => return false,
    };
    !integer.is_empty()
        && integer.chars().all(|c| c.is_ascii_digit())
        && fraction.len() == 2
        && fraction.chars().all(|c| c.is_ascii_digit())
}

#[test]
fn a_hostile_label_is_escaped_and_the_svg_stays_valid() {
    let hostile = "</text><script>alert(1)</script>";
    let ir = fixture::graph(vec![fixture::node("hostile", hostile)], Vec::new());
    let layout = fixture::layout(
        &[("hostile", Point::new(80.0, 40.0), Size::new(160.0, 44.0))],
        Vec::new(),
        Rect::new(0.0, 0.0, 160.0, 80.0),
    );

    let svg = render(&ir, &layout, &Palette::plano());

    document(&svg);
    assert!(
        element(&document(&svg), "script").is_empty(),
        "no <script> node may reach the DOM"
    );
    assert!(!svg.contains("<script"));
    assert!(svg.contains("&lt;/text&gt;&lt;script&gt;alert(1)&lt;/script&gt;"));
}

#[test]
fn two_renders_of_the_same_ir_are_byte_identical() {
    let ir = fixture::fixture_ir();
    let layout = fixture::fixture_layout();

    let first = render(&ir, &layout, &Palette::plano());
    let second = render(&ir, &layout, &Palette::plano());

    assert_eq!(first, second);
}

#[test]
fn has_no_xml_declaration_and_no_background_rect() {
    let svg = render_fixture();
    let document = document(&svg);
    let root = document.root_element();

    assert!(!svg.contains("<?xml"));
    assert_eq!(root.tag_name().name(), "svg");
    assert_eq!(
        root.tag_name().namespace(),
        Some("http://www.w3.org/2000/svg")
    );
    assert_eq!(root.attribute("class"), Some("graphium"));
    assert!(
        root.children()
            .filter(|node| node.is_element())
            .all(|node| node.tag_name().name() != "rect"),
        "the root must not paint a background rect"
    );
    assert!(!svg.contains("class=\"bg"), "no element uses the bg token");
}

#[test]
fn every_numeric_attribute_has_exactly_two_decimals() {
    let svg = render_fixture();
    let document = document(&svg);
    let mut checked = 0;

    for node in document.descendants().filter(|node| node.is_element()) {
        for attribute in node.attributes() {
            if !NUMERIC_ATTRIBUTES.contains(&attribute.name()) {
                continue;
            }
            for token in attribute.value().split([' ', ',']) {
                // Path data carries command letters (`M`, `C`, `L`, `Z`).
                let number = token.trim_start_matches(|c: char| c.is_ascii_alphabetic());
                if number.is_empty() {
                    continue;
                }
                assert!(
                    is_canonical_number(number),
                    "{}=\"{}\" carries `{}`, which is not a two-decimal number",
                    attribute.name(),
                    attribute.value(),
                    number
                );
                checked += 1;
            }
        }
    }

    assert!(
        checked >= 20,
        "the scan must visit real numeric attributes, it visited {checked}"
    );
}

#[test]
fn ids_are_derived_from_ir_ids_and_never_from_counters() {
    let svg = render_fixture();
    let document = document(&svg);
    let mut ids: Vec<String> = document
        .descendants()
        .filter(|node| node.is_element())
        .filter_map(|node| node.attribute("id"))
        .map(str::to_string)
        .collect();
    ids.sort();

    assert!(ids.contains(&"g-read".to_string()));
    assert!(ids.contains(&"g-ok".to_string()));
    assert!(ids.contains(&"m-read-ok".to_string()));

    for id in ids.iter().filter(|id| id.starts_with("g-")) {
        let ir_id = id.trim_start_matches("g-");
        assert!(
            ["read", "ok", "cyl"].contains(&ir_id),
            "`{id}` is not derived from an IR node id"
        );
    }
    for id in ids.iter().filter(|id| id.starts_with("m-")) {
        let ends_with_ir_id = ["read", "ok", "cyl"]
            .iter()
            .any(|ir_id| id.ends_with(&format!("-{ir_id}")));
        assert!(ends_with_ir_id, "`{id}` is not derived from an IR edge id");
    }
}

#[test]
fn markers_are_emitted_in_id_order() {
    let svg = render(
        &fixture::emphasized_ir(),
        &fixture::emphasized_layout(),
        &Palette::plano(),
    );
    let document = document(&svg);
    let defs = document
        .descendants()
        .find(|node| node.tag_name().name() == "defs")
        .expect("<defs> is always emitted");
    let ids: Vec<&str> = defs
        .children()
        .filter(|node| node.is_element())
        .filter_map(|node| node.attribute("id"))
        .collect();

    assert_eq!(ids, vec!["arrow-highlight", "arrow-muted", "arrow-normal"]);
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted, "markers must be emitted in id order");
    assert!(svg.contains("marker-end=\"url(#arrow-normal)\""));
    assert!(svg.contains("marker-end=\"url(#arrow-highlight)\""));
    assert!(svg.contains("marker-end=\"url(#arrow-muted)\""));
}

#[test]
fn arrowheads_and_dashes_follow_the_edge_style() {
    let svg = render_fixture();

    assert!(svg.contains("marker-end=\"url(#arrow-normal)\""));
    assert!(svg.contains("marker-end=\"url(#arrow-muted)\""));
    assert!(svg.contains("stroke-dashed"));
    assert!(svg.contains("id=\"m-read-cyl\""));
}

#[test]
fn every_marker_class_is_styled() {
    let svg = render(
        &fixture::emphasized_ir(),
        &fixture::emphasized_layout(),
        &Palette::plano(),
    );
    let document = document(&svg);
    let markers: Vec<roxmltree::Node<'_, '_>> = document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "marker")
        .collect();

    assert_eq!(markers.len(), 3, "one marker per emphasis");
    for marker in markers {
        let path = marker
            .children()
            .find(|node| node.is_element() && node.tag_name().name() == "path")
            .expect("the marker has an arrowhead path");
        let class = classes(path);
        assert!(
            svg.contains(&format!(".{class}{{")),
            "marker class `{class}` must resolve in the style block"
        );
    }
}

#[test]
fn shapes_are_drawn_with_their_own_class() {
    let ir = fixture::graph(
        vec![
            fixture::shaped("rnd", "rounded", Shape::Rounded),
            fixture::shaped("cyl", "cyl", Shape::Cylinder),
            fixture::shaped("dia", "diamond", Shape::Diamond),
            fixture::shaped("ell", "ellipse", Shape::Ellipse),
            fixture::shaped("nte", "note", Shape::Note),
        ],
        Vec::new(),
    );
    let layout = fixture::layout(
        &[
            ("rnd", Point::new(60.0, 40.0), Size::new(104.0, 44.0)),
            ("cyl", Point::new(60.0, 130.0), Size::new(80.0, 60.0)),
            ("dia", Point::new(200.0, 40.0), Size::new(90.0, 60.0)),
            ("ell", Point::new(200.0, 130.0), Size::new(90.0, 60.0)),
            ("nte", Point::new(200.0, 220.0), Size::new(90.0, 60.0)),
        ],
        Vec::new(),
        Rect::new(0.0, 0.0, 260.0, 260.0),
    );
    let svg = render(&ir, &layout, &Palette::plano());
    let document = document(&svg);
    let of_id = |id: &str| {
        classes(
            document
                .descendants()
                .find(|node| node.is_element() && node.attribute("id") == Some(id))
                .unwrap_or_else(|| panic!("{id} is drawn")),
        )
    };

    assert!(of_id("g-rnd").contains("shape-rounded"));
    assert!(of_id("g-cyl").contains("shape-cylinder"));
    assert!(of_id("g-dia").contains("shape-diamond"));
    assert!(of_id("g-ell").contains("shape-ellipse"));
    assert!(of_id("g-nte").contains("shape-note"));
    assert!(!of_id("g-cyl").contains("shape-rounded"));

    let cylinder_geometry = document
        .descendants()
        .filter(|node| node.is_element())
        .find(|node| {
            matches!(node.tag_name().name(), "path" | "ellipse")
                && classes(*node).contains("shape-cylinder")
        });
    assert!(
        cylinder_geometry.is_some(),
        "a cylinder is drawn with its own path class"
    );
    assert!(
        element(&document, "path").len() >= 2,
        "cylinder and note contribute paths"
    );
}

#[test]
fn the_style_block_resolves_every_palette_token() {
    let dark = render_fixture();
    let light = render(
        &fixture::fixture_ir(),
        &fixture::fixture_layout(),
        &Palette::claro(),
    );

    for token in [
        "node-fill",
        "node-stroke",
        "node-text",
        "edge-stroke",
        "edge-text",
        "accent",
        "bg",
    ] {
        assert!(dark.contains(&format!(".{token}{{")), "missing .{token}");
        assert!(light.contains(&format!(".{token}{{")), "missing .{token}");
    }
    assert_ne!(dark, light, "the resolved palette must change the output");
    assert!(dark.contains(&Palette::plano().node_fill.css()));
    assert!(light.contains(&Palette::claro().node_fill.css()));
}

#[test]
fn node_labels_are_centered_inside_their_box() {
    let svg = render_fixture();
    let document = document(&svg);
    let text = document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "text")
        .find(|node| node.text() == Some("read file"))
        .expect("the node label is emitted");

    assert!(classes(text).contains("node-text"));
    assert!(classes(text).contains("text"));
    assert_eq!(text.attribute("x"), Some("60.00"));
    assert_eq!(text.attribute("y"), Some("40.00"));
    assert_eq!(text.attribute("font-size"), Some("14.00"));
    assert_eq!(text.attribute("text-anchor"), Some("middle"));
    assert_eq!(text.attribute("dominant-baseline"), Some("central"));
}

#[test]
fn a_multi_line_label_is_split_into_tspans() {
    let svg = render_fixture();
    let document = document(&svg);
    let tspans = element(&document, "tspan");

    assert_eq!(tspans.len(), 2);
    assert_eq!(tspans[0].text(), Some("a"));
    assert_eq!(tspans[1].text(), Some("b"));
    assert!(tspans[0].attribute("y").unwrap() < tspans[1].attribute("y").unwrap());
}

#[test]
fn the_edge_label_is_centered_on_label_at() {
    let svg = render_fixture();
    let document = document(&svg);
    let label = document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "text")
        .find(|node| node.text() == Some("utf-8"))
        .expect("the edge label is emitted");

    assert!(classes(label).contains("edge-text"));
    assert_eq!(label.attribute("x"), Some("90.00"));
    assert_eq!(label.attribute("y"), Some("85.00"));
    assert_eq!(label.attribute("text-anchor"), Some("middle"));
}

#[test]
fn the_root_uses_plan_units_and_the_document_has_no_trailing_whitespace() {
    let svg = render_fixture();
    let document = document(&svg);
    let root = document.root_element();

    assert_eq!(root.attribute("viewBox"), Some("0.00 0.00 260.00 170.00"));
    assert_eq!(root.attribute("width"), Some("260.00"));
    assert_eq!(root.attribute("height"), Some("170.00"));
    assert!(!svg.ends_with('\n'));
    for line in svg.lines() {
        assert_eq!(line.trim_end(), line, "trailing space in `{line}`");
    }
}

#[test]
fn a_timeline_ir_still_renders_a_valid_canvas() {
    let ir = Ir::Timeline(IrTimeline {
        lanes: Vec::new(),
        events: Vec::new(),
    });
    let svg = render(&ir, &LayoutGraph::empty(), &Palette::plano());
    let document = document(&svg);

    assert_eq!(document.root_element().tag_name().name(), "svg");
    assert!(element(&document, "script").is_empty());
}
