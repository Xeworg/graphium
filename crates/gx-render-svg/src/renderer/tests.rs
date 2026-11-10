//! Behavioural tests of the deterministic SVG writer (PRD §8.5, §8.5.1, §9.1).

use std::sync::Arc;

use gx_core::{
    fmt_num, Cardinality, CardinalityBound, CardinalityMax, Direction, Ir, IrEdge, IrGraph, IrNode,
    IrRow, IrTimeline, LayoutEdge, LayoutGraph, LayoutHint, NodeBox, Palette, Point, Rect, RowKey,
    Shape, Size, SvgRenderer as SvgRendererContract, TOKENS,
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
            fixture::shaped("dot", "dot", Shape::Circle),
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
            ("dot", Point::new(200.0, 310.0), Size::new(120.0, 48.0)),
        ],
        Vec::new(),
        Rect::new(0.0, 0.0, 260.0, 340.0),
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
    assert!(of_id("g-dot").contains("shape-circle"));
    assert!(!of_id("g-cyl").contains("shape-rounded"));

    // The dot's label starts beside it, start-anchored — the only start
    // anchor in the drawing, and the tell of the beside placement
    // (`f19-formas.md`, S8).
    let beside = document
        .descendants()
        .find(|node| node.is_element() && node.attribute("text-anchor") == Some("start"))
        .expect("the dot's label sits beside it");
    assert!(
        beside.attribute("x").is_some(),
        "the beside label has its point"
    );

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

/// S13: a table whose rows have no key has nothing to put in the badge column,
/// so it reserves no column and writes no badge: the row names start at the
/// padding, and not one `row-badge` element reaches the drawing.
///
/// The header keeps its own place: semibold, centered on the box (S12 folds
/// the three writers, it does not change what the table reads like).
#[test]
fn a_table_without_keys_reserves_no_badge_column() {
    let mut node = fixture::shaped("roles", "Cliente", Shape::Box);
    node.rows = vec![IrRow::new("person"), IrRow::new("hace el pedido")];
    let ir = fixture::graph(vec![node], Vec::new());
    let layout = fixture::layout(
        &[("roles", Point::new(100.0, 50.0), Size::new(200.0, 90.0))],
        Vec::new(),
        Rect::new(0.0, 0.0, 200.0, 100.0),
    );

    let svg = render(&ir, &layout, &Palette::plano());
    let document = document(&svg);

    assert!(
        element(&document, "text")
            .iter()
            .all(|node| !classes(*node).contains("row-badge")),
        "a table without keys draws no badge: {svg}"
    );
    let texts = element(&document, "text");
    assert_eq!(texts.len(), 3, "header and two row names, nothing else");
    assert!(
        texts
            .iter()
            .all(|node| !node.text().unwrap_or_default().is_empty()),
        "no empty text element is written: {svg}"
    );

    let x_of = |text: &str| {
        element(&document, "text")
            .into_iter()
            .find(|node| node.text() == Some(text))
            .unwrap_or_else(|| panic!("`{text}` is drawn"))
            .attribute("x")
            .expect("a text carries its x")
            .to_string()
    };
    let name_x = fmt_num(TOKENS.node_padding);
    assert_eq!(x_of("person"), name_x, "the name starts at the padding");
    assert_eq!(x_of("hace el pedido"), name_x, "no badge column is paid");

    let header = element(&document, "text")
        .into_iter()
        .find(|node| node.text() == Some("Cliente"))
        .expect("the header is drawn");
    assert_eq!(header.attribute("x"), Some("100.00"), "centered on the box");
    assert_eq!(header.attribute("text-anchor"), Some("middle"));
    assert_eq!(
        header.attribute("font-weight"),
        Some(TOKENS.semibold_weight.to_string().as_str()),
        "the header stays semibold"
    );
}

/// S13: a key in one row reserves the badge column for the whole table, and
/// only the rows that carry a key write a badge. The keyless row keeps the
/// indent (the column exists) and gets no empty element.
#[test]
fn only_the_rows_with_a_key_draw_a_badge() {
    let mut id = IrRow::new("id");
    id.key = RowKey::Pk;
    let mut user = IrRow::new("user_id");
    user.key = RowKey::Fk;
    user.nullable = true;
    let title = IrRow::new("title");
    let mut node = fixture::shaped("posts", "posts", Shape::Box);
    node.rows = vec![id, user, title];
    let ir = fixture::graph(vec![node], Vec::new());
    let layout = fixture::layout(
        &[("posts", Point::new(150.0, 60.0), Size::new(300.0, 110.0))],
        Vec::new(),
        Rect::new(0.0, 0.0, 300.0, 120.0),
    );

    let svg = render(&ir, &layout, &Palette::plano());
    let document = document(&svg);

    let badges: Vec<_> = element(&document, "text")
        .into_iter()
        .filter(|node| classes(*node).contains("row-badge"))
        .collect();
    assert_eq!(badges.len(), 2, "one badge per key, and no other: {svg}");
    assert_eq!(badges[0].text(), Some("PK"));
    assert_eq!(badges[1].text(), Some("FK"));

    let texts = element(&document, "text");
    assert_eq!(
        texts.len(),
        6,
        "header, two badges and three row names, nothing else"
    );

    let x_of = |text: &str| {
        texts
            .iter()
            .find(|node| node.text() == Some(text))
            .unwrap_or_else(|| panic!("`{text}` is drawn"))
            .attribute("x")
            .expect("a text carries its x")
            .to_string()
    };
    let name_x = fmt_num(TOKENS.node_padding + crate::geometry::BADGE_WIDTH);
    assert_eq!(x_of("id"), name_x, "the badge column is reserved");
    assert_eq!(x_of("title"), name_x, "the keyless row keeps the indent");
}

/// S12: an event is a rounded box, and the emitter draws it with the same
/// shape writer a rounded node goes through, so the two cannot drift apart.
#[test]
fn an_event_is_drawn_as_the_same_rounded_box_as_a_node() {
    let ir = Ir::Timeline(IrTimeline {
        lanes: vec![gx_core::IrLane {
            id: "q1".to_string(),
            label: Some("Q1".to_string()),
        }],
        events: vec![event("event-0", "q1", "spike")],
    });
    let layout = LayoutGraph {
        nodes: [(
            "event-0".to_string(),
            NodeBox {
                center: Point::new(200.0, 70.0),
                size: Size::new(120.0, 44.0),
            },
        )]
        .into_iter()
        .collect(),
        edges: Vec::new(),
        groups: vec![group("q1", Some("Q1"), Rect::new(0.0, 0.0, 400.0, 150.0))],
        bbox: Rect::new(0.0, 0.0, 400.0, 150.0),
    };

    let svg = render(&ir, &layout, &Palette::plano());
    let document = document(&svg);
    let box_of_event = element(&document, "rect")
        .into_iter()
        .find(|node| node.attribute("id") == Some("g-event-0"))
        .expect("the event is drawn");

    assert_eq!(
        classes(box_of_event),
        "node-fill node-stroke shape-rounded",
        "an event is a rounded box"
    );
    assert_eq!(
        box_of_event.attribute("rx"),
        Some(fmt_num(TOKENS.corner_radius).as_str()),
        "with the corner radius of every rounded box"
    );
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

/// Un grafo con grupos dibuja cada grupo como un contenedor: el recuadro va
/// **detrás** de las cajas y las flechas, y su nombre arriba a la izquierda.
/// Sin el contenedor, un diagrama de componentes es una nube de cajas y no se
/// ve qué piezas forman el mismo sistema.
#[test]
fn a_graph_draws_each_group_as_a_container_with_its_name() {
    let ir = Ir::Graph(IrGraph {
        direction: Direction::TB,
        layout_hint: LayoutHint::Layered,
        nodes: vec![IrNode::new("api", "API"), IrNode::new("db", "Base")],
        edges: vec![IrEdge::new("api", "db")],
        groups: vec![gx_core::IrGroup {
            id: "backend".to_string(),
            label: Some("Backend".to_string()),
            members: vec!["api".to_string(), "db".to_string()],
        }],
    });
    let layout = LayoutGraph {
        nodes: [
            (
                "api".to_string(),
                NodeBox {
                    center: Point::new(120.0, 120.0),
                    size: Size::new(100.0, 44.0),
                },
            ),
            (
                "db".to_string(),
                NodeBox {
                    center: Point::new(120.0, 240.0),
                    size: Size::new(100.0, 44.0),
                },
            ),
        ]
        .into_iter()
        .collect(),
        edges: vec![fixture::routed(
            "api",
            "db",
            &[(120.0, 142.0), (120.0, 218.0)],
            None,
        )],
        groups: vec![group(
            "backend",
            Some("Backend"),
            Rect::new(20.0, 60.0, 200.0, 250.0),
        )],
        bbox: Rect::new(0.0, 0.0, 240.0, 330.0),
    };

    let svg = render(&ir, &layout, &Palette::plano());
    let document = document(&svg);

    let boxes: Vec<_> = element(&document, "rect")
        .into_iter()
        .filter(|node| classes(*node) == "group-box")
        .collect();
    assert_eq!(boxes.len(), 1, "un contenedor por grupo");
    assert_eq!(boxes[0].attribute("id"), Some("cluster-backend"));
    assert_eq!(boxes[0].attribute("x"), Some("20.00"));
    assert_eq!(boxes[0].attribute("width"), Some("200.00"));

    let labels: Vec<_> = element(&document, "text")
        .into_iter()
        .filter(|node| classes(*node).contains("group-label"))
        .collect();
    assert_eq!(labels.len(), 1);
    assert_eq!(labels[0].text(), Some("Backend"));
    assert_eq!(labels[0].attribute("text-anchor"), Some("start"));

    // Los contenedores van detrás: el SVG los escribe antes que las aristas y
    // que las cajas.
    let group_layer = svg.find("layer-groups").expect("capa de grupos");
    let edge_layer = svg.find("layer-edges").expect("capa de aristas");
    let node_layer = svg.find("layer-nodes").expect("capa de cajas");
    assert!(
        group_layer < edge_layer,
        "los grupos van debajo de las flechas"
    );
    assert!(
        group_layer < node_layer,
        "los grupos van debajo de las cajas"
    );

    assert_eq!(svg, render(&ir, &layout, &Palette::plano()));
}

/// RF15: un grupo dentro de otro. La caja externa se escribe primero —queda
/// detrás— y el nombre del interno se sangra por profundidad, para que los dos
/// nombres no se pisen en la misma esquina.
#[test]
fn nested_groups_draw_the_outer_box_first_and_indent_the_inner_name() {
    let ir = Ir::Graph(IrGraph {
        direction: Direction::TB,
        layout_hint: LayoutHint::Layered,
        nodes: vec![IrNode::new("api", "API")],
        edges: vec![],
        groups: vec![
            gx_core::IrGroup {
                id: "outer".to_string(),
                label: Some("Sistema".to_string()),
                members: vec!["inner".to_string()],
            },
            gx_core::IrGroup {
                id: "inner".to_string(),
                label: Some("Backend".to_string()),
                members: vec!["api".to_string()],
            },
        ],
    });
    let layout = LayoutGraph {
        nodes: [(
            "api".to_string(),
            NodeBox {
                center: Point::new(140.0, 180.0),
                size: Size::new(100.0, 44.0),
            },
        )]
        .into_iter()
        .collect(),
        edges: vec![],
        groups: vec![
            nested_group(
                "outer",
                Some("Sistema"),
                Rect::new(20.0, 60.0, 240.0, 220.0),
                0,
            ),
            nested_group(
                "inner",
                Some("Backend"),
                Rect::new(40.0, 100.0, 180.0, 140.0),
                1,
            ),
        ],
        bbox: Rect::new(0.0, 0.0, 280.0, 300.0),
    };

    let svg = render(&ir, &layout, &Palette::plano());
    let document = document(&svg);

    let outer = svg.find("cluster-outer").expect("la caja externa");
    let inner = svg.find("cluster-inner").expect("la caja interna");
    assert!(
        outer < inner,
        "la caja externa se dibuja primero, asi queda detras: {svg}"
    );

    let label_x = |title: &str| -> f64 {
        element(&document, "text")
            .into_iter()
            .find(|node| classes(*node).contains("group-label") && node.text() == Some(title))
            .unwrap_or_else(|| panic!("falta el nombre `{title}`"))
            .attribute("x")
            .expect("el nombre trae x")
            .parse()
            .expect("x es un numero")
    };
    assert!(
        label_x("Backend") > label_x("Sistema"),
        "el nombre del grupo interno se sangra por profundidad: {} vs {}",
        label_x("Backend"),
        label_x("Sistema")
    );
    assert_eq!(svg, render(&ir, &layout, &Palette::plano()));
}

/// Un timeline se lee por sus bandas: la banda dice en qué carril está el
/// evento y su nombre va en la canaleta de la izquierda. Sin banda, el dibujo
/// es una nube de cajas sueltas.
#[test]
fn a_timeline_draws_each_lane_as_a_band_with_its_name() {
    let ir = Ir::Timeline(IrTimeline {
        lanes: vec![
            gx_core::IrLane {
                id: "q1".to_string(),
                label: Some("Q1".to_string()),
            },
            gx_core::IrLane {
                id: "q2".to_string(),
                label: None,
            },
        ],
        events: vec![event("event-0", "q1", "spike")],
    });
    let layout = LayoutGraph {
        nodes: [(
            "event-0".to_string(),
            NodeBox {
                center: Point::new(200.0, 70.0),
                size: Size::new(120.0, 44.0),
            },
        )]
        .into_iter()
        .collect(),
        edges: Vec::new(),
        groups: vec![
            group("q1", Some("Q1"), Rect::new(0.0, 0.0, 400.0, 150.0)),
            group("q2", None, Rect::new(0.0, 150.0, 400.0, 150.0)),
        ],
        bbox: Rect::new(0.0, 0.0, 400.0, 300.0),
    };

    let svg = render(&ir, &layout, &Palette::plano());
    let document = document(&svg);

    let bands: Vec<_> = element(&document, "rect")
        .into_iter()
        .filter(|node| classes(*node) == "lane-band")
        .collect();
    assert_eq!(bands.len(), 2, "una banda por carril");
    assert_eq!(bands[0].attribute("id"), Some("lane-q1"));
    assert_eq!(bands[0].attribute("y"), Some("0.00"));
    assert_eq!(bands[1].attribute("y"), Some("150.00"));

    // El nombre del carril va sobre su banda, alineado a la izquierda: es lo
    // que el layout reservó en la canaleta.
    let labels: Vec<_> = element(&document, "text")
        .into_iter()
        .filter(|node| classes(*node).contains("lane-label"))
        .collect();
    assert_eq!(labels.len(), 1, "el carril sin nombre no inventa uno");
    assert_eq!(labels[0].text(), Some("Q1"));
    assert_eq!(labels[0].attribute("text-anchor"), Some("start"));
    assert_eq!(labels[0].attribute("y"), Some("75.00"));

    // El evento es una caja con su texto, como un nodo.
    assert!(element(&document, "rect")
        .into_iter()
        .any(|node| node.attribute("id") == Some("g-event-0")));
    assert!(element(&document, "text")
        .into_iter()
        .any(|node| node.text() == Some("spike")));
}

/// El orden de los elementos no depende del documento: los eventos y las
/// bandas salen por id, y las bandas van debajo de las cajas.
#[test]
fn the_timeline_is_written_in_id_order_with_the_bands_underneath() {
    let ir = Ir::Timeline(IrTimeline {
        lanes: vec![
            gx_core::IrLane {
                id: "b".to_string(),
                label: Some("B".to_string()),
            },
            gx_core::IrLane {
                id: "a".to_string(),
                label: Some("A".to_string()),
            },
        ],
        events: vec![event("event-1", "b", "uno"), event("event-0", "a", "dos")],
    });
    let layout = LayoutGraph {
        nodes: [
            (
                "event-1".to_string(),
                NodeBox {
                    center: Point::new(200.0, 220.0),
                    size: Size::new(120.0, 44.0),
                },
            ),
            (
                "event-0".to_string(),
                NodeBox {
                    center: Point::new(200.0, 70.0),
                    size: Size::new(120.0, 44.0),
                },
            ),
        ]
        .into_iter()
        .collect(),
        edges: Vec::new(),
        groups: vec![
            group("b", Some("B"), Rect::new(0.0, 150.0, 400.0, 150.0)),
            group("a", Some("A"), Rect::new(0.0, 0.0, 400.0, 150.0)),
        ],
        bbox: Rect::new(0.0, 0.0, 400.0, 300.0),
    };

    let svg = render(&ir, &layout, &Palette::plano());

    let a = svg.find("id=\"lane-a\"").expect("la banda a");
    let b = svg.find("id=\"lane-b\"").expect("la banda b");
    assert!(a < b, "las bandas salen por id");
    let first_event = svg.find("id=\"g-event-0\"").expect("el evento 0");
    let second_event = svg.find("id=\"g-event-1\"").expect("el evento 1");
    assert!(first_event < second_event, "los eventos salen por id");
    assert!(
        svg.find("layer-lanes") < Some(first_event) && svg.find("layer-lanes") < Some(second_event),
        "las bandas van debajo de las cajas"
    );

    // Y el dibujo es determinista.
    assert_eq!(svg, render(&ir, &layout, &Palette::plano()));
}

/// Un evento de un timeline.
fn event(id: &str, lane: &str, label: &str) -> gx_core::IrEvent {
    gx_core::IrEvent {
        id: id.to_string(),
        lane: lane.to_string(),
        label: label.to_string(),
        order: 0,
        span: None,
        depends_on: Vec::new(),
        kind_data: Default::default(),
    }
}

/// Un evento con una duración: `IrEvent.span` es un tramo de tiempo, no un
/// punto (`f11-elementos.md`, T4/D).
fn spanned(id: &str, lane: &str, label: &str, start: f64, end: f64) -> gx_core::IrEvent {
    gx_core::IrEvent {
        span: Some(gx_core::ir::Span { start, end }),
        ..event(id, lane, label)
    }
}

/// Un timeline con un punto en el tiempo y un evento que dura: el mismo
/// documento con el que se prueban las dos emisoras de la barra.
fn bar_timeline() -> (Ir, LayoutGraph) {
    let ir = Ir::Timeline(IrTimeline {
        lanes: vec![gx_core::IrLane {
            id: "q1".to_string(),
            label: Some("Q1".to_string()),
        }],
        events: vec![
            event("point", "q1", "punto"),
            spanned("bar", "q1", "dura", 1.0, 4.0),
        ],
    });
    let layout = LayoutGraph {
        nodes: [
            (
                "point".to_string(),
                NodeBox {
                    center: Point::new(100.0, 70.0),
                    size: Size::new(120.0, 44.0),
                },
            ),
            (
                "bar".to_string(),
                NodeBox {
                    center: Point::new(300.0, 70.0),
                    size: Size::new(250.0, 44.0),
                },
            ),
        ]
        .into_iter()
        .collect(),
        edges: Vec::new(),
        groups: vec![group("q1", Some("Q1"), Rect::new(0.0, 0.0, 500.0, 150.0))],
        bbox: Rect::new(0.0, 0.0, 500.0, 150.0),
    };
    (ir, layout)
}

/// `f11-elementos.md`, T4/D: un evento que dura se dibuja como una barra, y un
/// punto en el tiempo no.
///
/// El motor de carriles ya lee `IrEvent.span` al medir el evento (`gx-layout`,
/// `event_size`: un tramo de `n` columnas pide una caja de `n` columnas de
/// ancho), así que la caja medida **es** el largo de la barra. Lo que faltaba
/// era la marca: un punto en el tiempo y un tramo se dibujaban igual. La barra
/// es una banda al pie de la caja medida, con la clase `accent` que la paleta
/// ya resuelve — ninguna regla de estilo nueva, así que ningún golden de un
/// documento sin tramos cambia.
#[test]
fn a_spanned_event_is_drawn_as_a_bar_and_a_point_in_time_is_not() {
    let (ir, layout) = bar_timeline();
    let svg = render(&ir, &layout, &Palette::plano());
    let document = document(&svg);

    let bars: Vec<_> = element(&document, "rect")
        .into_iter()
        .filter(|node| classes(*node) == "accent")
        .collect();
    assert_eq!(bars.len(), 1, "one bar: the event with a span");
    let bar = bars[0];
    // La caja del evento `bar`: centro (300, 70), tamaño 250 × 44 →
    // rect (175, 48, 250, 44); la barra se mete el borde por cada lado y
    // mide las constantes congeladas de `geometry.rs`.
    assert_eq!(bar.attribute("x"), Some("176.00"));
    assert_eq!(bar.attribute("y"), Some("86.00"));
    assert_eq!(bar.attribute("width"), Some("248.00"));
    assert_eq!(bar.attribute("height"), Some("5.00"));

    // La barra va con su evento: después de su caja y antes de su texto.
    let chip = svg.find("id=\"g-bar\"").expect("la caja del evento");
    let at = svg.find("class=\"accent\"").expect("la barra");
    let label = svg.find("dura").expect("el texto del evento");
    assert!(chip < at && at < label, "la barra va sobre su caja");
}

/// T4/D, S5: la barra viaja como números y las dos emisoras llevan los mismos:
/// el payload no lleva un path y el painter no recalcula nada.
#[test]
fn the_bar_of_a_spanned_event_is_the_same_number_in_both_emitters() {
    let (ir, layout) = bar_timeline();
    let svg = render(&ir, &layout, &Palette::plano());
    let scene = crate::scene::scene(&ir, &layout, &Palette::plano());

    let document = document(&svg);
    let bar = element(&document, "rect")
        .into_iter()
        .find(|node| classes(*node) == "accent")
        .expect("la barra del SVG");
    let number = |name: &str| {
        bar.attribute(name)
            .unwrap_or_else(|| panic!("the bar carries {name}"))
            .parse::<f64>()
            .expect("a canonical number")
    };

    let spanned = scene
        .events
        .iter()
        .find(|event| event.id == "g-bar")
        .expect("el evento con tramo");
    let json = serde_json::to_value(spanned).expect("el evento serializa");
    let numbers = json
        .get("bar")
        .expect("la barra del evento viaja como números (S5)");
    assert_eq!(numbers["x"].as_f64(), Some(number("x")));
    assert_eq!(numbers["y"].as_f64(), Some(number("y")));
    assert_eq!(numbers["w"].as_f64(), Some(number("width")));
    assert_eq!(numbers["h"].as_f64(), Some(number("height")));
    assert!(
        !serde_json::to_string(&json)
            .expect("json")
            .contains("\"d\""),
        "the bar is numbers, never a path"
    );
}

/// Una banda de carril del layout.
fn group(id: &str, label: Option<&str>, bbox: Rect) -> gx_core::LayoutGroup {
    gx_core::LayoutGroup {
        id: id.to_string(),
        label: label.map(str::to_string),
        bbox,
        depth: 0,
        members: Vec::new(),
        gutter: Some(0.0),
    }
}

/// Un grupo del layout con su profundidad de anidamiento (PRD §7 RF15).
fn nested_group(id: &str, label: Option<&str>, bbox: Rect, depth: usize) -> gx_core::LayoutGroup {
    let mut placed = group(id, label, bbox);
    placed.depth = depth;
    placed
}

/// The crow's foot opens towards the table: the ankle is away from it and the
/// three toes touch its edge. Opening away from the table would draw an
/// arrowhead, which means the opposite.
#[test]
fn the_crow_foot_opens_towards_the_table() {
    let mut from = IrNode::new("users", "users");
    from.rows = vec![IrRow::new("id")];
    let mut to = IrNode::new("posts", "posts");
    to.rows = vec![IrRow::new("user_id")];

    let mut edge = IrEdge::new("users", "posts");
    edge.arrow = false;
    edge.cardinality = Some(Cardinality {
        from_min: CardinalityBound::Required,
        from_max: CardinalityMax::One,
        to_min: CardinalityBound::Optional,
        to_max: CardinalityMax::Many,
    });

    let layout = LayoutGraph {
        nodes: [
            (
                "users".to_string(),
                NodeBox {
                    center: gx_core::Point::new(50.0, 50.0),
                    size: gx_core::Size::new(100.0, 60.0),
                },
            ),
            (
                "posts".to_string(),
                NodeBox {
                    center: gx_core::Point::new(250.0, 50.0),
                    size: gx_core::Size::new(100.0, 60.0),
                },
            ),
        ]
        .into_iter()
        .collect(),
        edges: vec![LayoutEdge {
            from: "users".to_string(),
            to: "posts".to_string(),
            points: vec![
                gx_core::Point::new(100.0, 50.0),
                gx_core::Point::new(200.0, 50.0),
            ],
            label_at: None,
        }],
        groups: Vec::new(),
        bbox: Rect::new(0.0, 0.0, 300.0, 100.0),
    };

    let svg = engine().render(
        &Ir::Graph(IrGraph {
            direction: Direction::LR,
            layout_hint: LayoutHint::Layered,
            nodes: vec![from, to],
            edges: vec![edge],
            groups: Vec::new(),
        }),
        &layout,
        &Palette::plano(),
    );

    // The foot: three strokes from an ankle to the table edge at x = 200.
    let foot = svg
        .split("<path class=\"cardinality")
        .nth(1)
        .expect("the crow's foot is drawn");
    let foot = &foot[..foot.find("/>").expect("closed")];
    let toes_on_the_table = foot.matches("L 200.00").count();
    assert!(
        toes_on_the_table >= 1,
        "a toe has to touch the table edge: {foot}"
    );
    // The ankle is back from the table, never past it.
    assert!(foot.contains("M 178.00"), "the ankle sits away: {foot}");
}

/// Un diagrama de secuencia sin líneas de vida se lee desarmado: cada flecha
/// flota y no se ve a qué actor pertenece. Las líneas van del borde inferior
/// de cada actor al fondo del dibujo, y no aparecen en un grafo por capas.
#[test]
fn a_sequence_keeps_a_lifeline_under_every_actor() {
    use gx_core::ir::LayoutHint;

    let mut sequence = crate::test_support::graph(
        vec![
            crate::test_support::node("cliente", "Cliente"),
            crate::test_support::node("api", "API"),
        ],
        vec![crate::test_support::labeled_edge(
            "cliente",
            "api",
            "POST /orders",
        )],
    );
    if let gx_core::ir::Ir::Graph(graph) = &mut sequence {
        graph.layout_hint = LayoutHint::Lanes;
    }
    let layout = crate::test_support::layout(
        &[
            (
                "cliente",
                gx_core::geom::Point::new(60.0, 30.0),
                gx_core::geom::Size::new(90.0, 40.0),
            ),
            (
                "api",
                gx_core::geom::Point::new(240.0, 30.0),
                gx_core::geom::Size::new(90.0, 40.0),
            ),
        ],
        vec![crate::test_support::routed(
            "cliente",
            "api",
            &[(60.0, 120.0), (240.0, 120.0)],
            Some((150.0, 100.0)),
        )],
        gx_core::geom::Rect::new(0.0, 0.0, 300.0, 220.0),
    );

    let svg = render(&sequence, &layout, &Palette::plano());

    assert_eq!(
        svg.matches("<line class=\"lifeline\"").count(),
        2,
        "una línea de vida por actor: {svg}"
    );
    assert!(
        svg.contains(".lifeline{"),
        "la regla del tema viaja en el SVG"
    );

    // Un grafo por capas no lleva líneas de vida: no son parte de su dibujo.
    let layered = crate::test_support::graph(
        vec![
            crate::test_support::node("a", "A"),
            crate::test_support::node("b", "B"),
        ],
        vec![crate::test_support::labeled_edge("a", "b", "sigue")],
    );
    let svg = render(&layered, &layout, &Palette::plano());
    assert!(
        !svg.contains("<line class=\"lifeline\""),
        "capas no lleva líneas de vida: {svg}"
    );
}

/// S7 (PRD §8.5.2): el export es un documento **cerrado**, y el plano no. El
/// objeto del plano tiene que dejar ver la grilla detrás del dibujo, así que
/// solo el export pinta el rectángulo del lienzo; y ese rectángulo cubre todo
/// el `viewBox`, porque un fondo que no llega al borde deja una franja
/// transparente donde el texto claro desaparece.
#[test]
fn only_the_export_paints_the_canvas() {
    let plan = render_fixture();
    assert!(
        !plan.contains("<rect class=\"bg\""),
        "el plano deja el objeto transparente: {plan}"
    );

    let palette = Palette::plano();
    let engine = engine();
    let exported = SvgRendererContract::render_export(
        &engine,
        &fixture::fixture_ir(),
        &fixture::fixture_layout(),
        &palette,
    );

    let document = Document::parse(&exported).expect("the exported svg parses");
    let root = document.root_element();
    let view_box: Vec<&str> = root
        .attribute("viewBox")
        .expect("the canvas has a viewBox")
        .split(' ')
        .collect();
    let background = document
        .descendants()
        .find(|node| node.attribute("class") == Some("bg"))
        .expect("the export paints a background");
    assert_eq!(background.attribute("x"), Some(view_box[0]), "covers x");
    assert_eq!(background.attribute("y"), Some(view_box[1]), "covers y");
    assert_eq!(background.attribute("width"), Some(view_box[2]), "covers w");
    assert_eq!(
        background.attribute("height"),
        Some(view_box[3]),
        "covers h"
    );

    // Todo lo demás es el mismo dibujo: el export no reescribe nada.
    let without_background: String = exported
        .lines()
        .filter(|line| !line.contains("<rect class=\"bg\""))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(without_background, plan);
}

/// S7 (PRD §8.5.2): el door llega al renderer por un `dyn SvgRenderer` (es el
/// tipo que guarda `gx_core::engine::Engines`). Un método inherente con el
/// mismo nombre gana solo en la llamada concreta: por la puerta el export
/// saldría sin fondo. La prueba es la llamada a través del trait object.
#[test]
fn the_export_background_arrives_through_the_trait_object() {
    let engine: Arc<dyn SvgRendererContract> = Arc::new(engine());
    let palette = Palette::plano();

    let exported =
        engine.render_export(&fixture::fixture_ir(), &fixture::fixture_layout(), &palette);
    let plan = engine.render(&fixture::fixture_ir(), &fixture::fixture_layout(), &palette);

    assert!(
        exported.contains("<rect class=\"bg\""),
        "el export a través del trait pinta el lienzo: {exported}"
    );
    assert!(
        !plan.contains("<rect class=\"bg\""),
        "el plano sigue saliendo sin fondo: {plan}"
    );
}

/// The marks of an ER relation (the crow's-foot fork and the zero circle)
/// carry `class="cardinality"`, and the page styles them (`app.css`:
/// `fill:none; stroke: var(--edge-stroke); 1px`). A standalone export loads no
/// stylesheet, so the style block must carry the same rule — without it the
/// marks fall back to the default black (pre-existing before f11; found while
/// delivering T4/D).
#[test]
fn the_style_block_styles_the_cardinality_marks() {
    assert!(
        render_fixture().contains(".cardinality{fill:none;stroke:"),
        "the export's style block must style the cardinality marks"
    );
}
