//! Behavioural tests of the scene builder (`f11-elementos.md`, T1, S3-S5).

use std::collections::BTreeSet;

use gx_core::ir::Span;
use gx_core::{
    Direction, Emphasis, Ir, IrEdge, IrEvent, IrGraph, IrLane, IrNode, IrRow, IrTimeline,
    LayoutGraph, LayoutGroup, LayoutHint, Palette, Point, Rect, Shape, Size, Style, TOKENS,
};

use super::{scene as build_scene, scene_tokens, Drawing};
use crate::geometry::{
    ARROW_LENGTH, BADGE_WIDTH, CROW_FOOT, CROW_FOOT_SPREAD, DASH_PATTERN, GROUP_DASH,
    LIFELINE_DASH, MUTED_OPACITY,
};
use crate::test_support as fixture;

fn palette() -> Palette {
    Palette::plano()
}

/// S3: the scene mirrors `Ir ∪ LayoutGraph`. The nodes come in id order, with
/// the id of the element in the drawing and the box the layout measured.
#[test]
fn the_scene_mirrors_the_nodes_of_the_drawing() {
    let ir = fixture::fixture_ir();
    let layout = fixture::fixture_layout();

    let scene = build_scene(&ir, &layout, &palette());

    let ids: Vec<&str> = scene.nodes.iter().map(|node| node.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["g-cyl", "g-ok", "g-read"],
        "the nodes come in id order, with the id of the drawing"
    );
    let cyl = &scene.nodes[0];
    assert_eq!(cyl.label, "a\nb", "the label travels pre-broken (S4)");
    assert_eq!(cyl.shape, Shape::Cylinder);
    assert_eq!(cyl.center, Point::new(210.0, 85.0), "the center of the box");
    assert_eq!(
        cyl.size,
        Size::new(60.0, 60.0),
        "the measured size, from the layout (S4)"
    );
    assert_eq!(cyl.style, Style::Solid);
    assert_eq!(cyl.emphasis, Emphasis::Normal);
    assert!(cyl.rows.is_empty());
    assert!(cyl.meta.is_empty(), "meta is the free channel of the kind");

    let ok = &scene.nodes[1];
    assert_eq!(ok.shape, Shape::Box);
    assert_eq!(ok.emphasis, Emphasis::Highlight, "the emphasis travels");
}

/// S4: the size of the scene is the one `NodeBox` carries. `IrNode.size` is
/// what the engine measured before the layout; the layout may place a box of
/// another size (a lane measures its own), and that is the authoritative one.
#[test]
fn the_size_of_a_node_is_the_measured_box_and_not_the_ir_size() {
    let mut ir = fixture::fixture_ir();
    if let Ir::Graph(graph) = &mut ir {
        graph.nodes[0].size = Size::new(999.0, 999.0);
    }
    let layout = fixture::fixture_layout();

    let scene = build_scene(&ir, &layout, &palette());

    let read = scene
        .nodes
        .iter()
        .find(|node| node.id == "g-read")
        .expect("the node is in the scene");
    assert_eq!(read.size, Size::new(104.0, 44.0));
    assert_eq!(read.center, Point::new(60.0, 40.0));
}

/// S3: the edges keep the id of the drawing, the ends as the IR names them, and
/// the route of the layout. An edge the layout did not route is not in the
/// scene, because it is not in the drawing either.
#[test]
fn the_scene_mirrors_the_routed_edges() {
    let ir = fixture::fixture_ir();
    let layout = fixture::fixture_layout();

    let scene = build_scene(&ir, &layout, &palette());

    let ids: Vec<&str> = scene.edges.iter().map(|edge| edge.id.as_str()).collect();
    assert_eq!(ids, vec!["m-read-cyl", "m-read-ok"], "in id order");
    let ok = &scene.edges[1];
    assert_eq!(ok.from, "read");
    assert_eq!(ok.to, "ok");
    assert_eq!(ok.label.as_deref(), Some("utf-8"));
    assert_eq!(
        ok.points,
        vec![
            Point::new(112.0, 40.0),
            Point::new(112.0, 90.0),
            Point::new(60.0, 108.0)
        ]
    );
    assert_eq!(ok.label_at, Some(Point::new(90.0, 85.0)));
    assert!(ok.arrow, "a plain edge carries a marker");
    assert_eq!(ok.cardinality, None, "a plain edge has no crow's foot");
    assert_eq!(ok.from_row, None);
    assert_eq!(ok.to_row, None);

    let dashed = &scene.edges[0];
    assert_eq!(dashed.style, Style::Dashed, "the dash travels");
    assert_eq!(dashed.emphasis, Emphasis::Muted);

    // An edge with no route in the layout is not drawn and not shipped.
    let mut unplaced = layout.clone();
    unplaced.edges.retain(|edge| edge.to != "cyl");
    let rerouted = build_scene(&ir, &unplaced, &palette());
    assert_eq!(
        rerouted
            .edges
            .iter()
            .map(|edge| edge.id.as_str())
            .collect::<Vec<_>>(),
        vec!["m-read-ok"]
    );
}

/// S3: a document without parallel edges keeps the exact ids of the drawing.
/// The disambiguation counter is per pair and per direction: `a->b` and `b->a`
/// are two different lines and neither is suffixed.
#[test]
fn an_edge_without_a_parallel_twin_keeps_its_exact_id() {
    let ir = fixture::graph(
        vec![
            fixture::node("a", "A"),
            fixture::node("b", "B"),
            fixture::node("c", "C"),
        ],
        vec![
            fixture::labeled_edge("a", "b", "ida"),
            fixture::labeled_edge("b", "a", "vuelta"),
            fixture::labeled_edge("a", "c", "otra"),
        ],
    );
    let layout = fixture::layout(
        &[
            ("a", Point::new(60.0, 40.0), Size::new(104.0, 44.0)),
            ("b", Point::new(60.0, 130.0), Size::new(104.0, 44.0)),
            ("c", Point::new(180.0, 40.0), Size::new(104.0, 44.0)),
        ],
        vec![
            fixture::routed("a", "b", &[(60.0, 40.0), (60.0, 108.0)], None),
            fixture::routed("b", "a", &[(80.0, 108.0), (80.0, 40.0)], None),
            fixture::routed("a", "c", &[(112.0, 40.0), (128.0, 40.0)], None),
        ],
        Rect::new(0.0, 0.0, 240.0, 170.0),
    );

    let scene = build_scene(&ir, &layout, &palette());

    let ids: Vec<&str> = scene.edges.iter().map(|edge| edge.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["m-a-b", "m-a-c", "m-b-a"],
        "one edge per pair keeps the id of the drawing"
    );
}

/// S3: two edges between the same pair of nodes are two lines, so the scene
/// gives them two ids. The scene is the payload of a `keyed each` on `edge.id`
/// and a duplicated key makes the window refuse to mount, so the id of the
/// later twins is suffixed in paint order. The first edge of the pair keeps
/// exactly the id of the drawing.
#[test]
fn parallel_edges_get_distinct_ids() {
    let ir = fixture::emphasized_ir();
    let layout = fixture::emphasized_layout();

    let scene = build_scene(&ir, &layout, &palette());

    let ids: Vec<&str> = scene.edges.iter().map(|edge| edge.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["m-read-ok", "m-read-ok-2", "m-read-ok-3"],
        "the first twin keeps the id of the drawing, the rest are suffixed"
    );
    let distinct: BTreeSet<&str> = ids.iter().copied().collect();
    assert_eq!(
        distinct.len(),
        ids.len(),
        "the painter keys by id, so no twin may repeat one"
    );
    assert_eq!(scene.edges.len(), 3, "every routed edge is drawn");
    let ends: Vec<(&str, &str)> = scene
        .edges
        .iter()
        .map(|edge| (edge.from.as_str(), edge.to.as_str()))
        .collect();
    assert_eq!(
        ends,
        vec![("read", "ok"), ("read", "ok"), ("read", "ok")],
        "every twin keeps its ends"
    );
    assert_eq!(
        build_scene(&ir, &layout, &palette()).edges[1].id,
        "m-read-ok-2",
        "the suffix is deterministic"
    );
}

/// S3: the canvas is the box of the layout and the direction of the document
/// travels so a painter can order the layers the way the emitter does.
#[test]
fn the_scene_carries_the_canvas_and_the_direction() {
    let ir = fixture::fixture_ir();
    let layout = fixture::fixture_layout();

    let scene = build_scene(&ir, &layout, &palette());

    assert_eq!(scene.canvas, Rect::new(0.0, 0.0, 260.0, 170.0));
    assert_eq!(scene.direction, Some(Direction::TB));
    assert_eq!(scene.layout_hint, Some(LayoutHint::Layered));
}

/// S3: a container travels with its box, its depth and its members — the list
/// the layout used to drop.
#[test]
fn the_scene_carries_the_containers_of_a_graph() {
    let mut ir = fixture::fixture_ir();
    if let Ir::Graph(graph) = &mut ir {
        graph.groups = vec![gx_core::IrGroup {
            id: "backend".to_string(),
            label: Some("Backend".to_string()),
            members: vec!["ok".to_string(), "cyl".to_string()],
        }];
    }
    let mut layout = fixture::fixture_layout();
    layout.groups = vec![LayoutGroup {
        id: "backend".to_string(),
        label: Some("Backend".to_string()),
        bbox: Rect::new(0.0, 20.0, 250.0, 160.0),
        depth: 1,
        members: vec!["ok".to_string(), "cyl".to_string()],
        gutter: None,
    }];

    let scene = build_scene(&ir, &layout, &palette());

    assert_eq!(scene.bands.len(), 1);
    let band = &scene.bands[0];
    assert_eq!(band.id, "cluster-backend", "the id of the drawing (S3)");
    assert_eq!(band.label.as_deref(), Some("Backend"));
    assert_eq!(band.bbox, Rect::new(0.0, 20.0, 250.0, 160.0));
    assert_eq!(band.depth, 1);
    assert_eq!(band.members, vec!["ok", "cyl"]);
    assert_eq!(band.gutter, None, "a container has no gutter");
    assert!(scene.events.is_empty(), "a graph has no events");
}

/// S3, S4: a timeline has no direction and no edges; its lanes are the bands,
/// its events carry the box the lanes engine measured and the span of a bar.
#[test]
fn the_scene_carries_the_lanes_and_events_of_a_timeline() {
    let ir = Ir::Timeline(IrTimeline {
        lanes: vec![
            IrLane {
                id: "a".to_string(),
                label: Some("Ana".to_string()),
            },
            IrLane {
                id: "b".to_string(),
                label: None,
            },
        ],
        events: vec![
            IrEvent {
                id: "one".to_string(),
                lane: "a".to_string(),
                label: "arranca".to_string(),
                order: 0,
                span: None,
                kind_data: Default::default(),
            },
            IrEvent {
                id: "two".to_string(),
                lane: "b".to_string(),
                label: "barra\nde dos".to_string(),
                order: 3,
                span: Some(Span {
                    start: 2.0,
                    end: 4.0,
                }),
                kind_data: Default::default(),
            },
        ],
    });
    let layout = fixture::layout(
        &[
            ("one", Point::new(80.0, 40.0), Size::new(120.0, 40.0)),
            ("two", Point::new(220.0, 120.0), Size::new(160.0, 40.0)),
        ],
        Vec::new(),
        Rect::new(0.0, 0.0, 400.0, 160.0),
    );
    let layout = LayoutGraph {
        groups: vec![
            LayoutGroup {
                id: "a".to_string(),
                label: Some("Ana".to_string()),
                bbox: Rect::new(0.0, 0.0, 400.0, 80.0),
                depth: 0,
                members: vec!["one".to_string()],
                gutter: Some(90.0),
            },
            LayoutGroup {
                id: "b".to_string(),
                label: None,
                bbox: Rect::new(0.0, 80.0, 400.0, 80.0),
                depth: 0,
                members: vec!["two".to_string()],
                gutter: Some(90.0),
            },
        ],
        ..layout
    };

    let scene = build_scene(&ir, &layout, &palette());

    assert_eq!(scene.direction, None, "a timeline has no direction");
    assert_eq!(scene.layout_hint, None);
    assert!(scene.edges.is_empty(), "a timeline has no edges");
    assert!(scene.nodes.is_empty(), "a timeline has events");

    let bands: Vec<&str> = scene.bands.iter().map(|band| band.id.as_str()).collect();
    assert_eq!(bands, vec!["lane-a", "lane-b"], "the id of a lane band");
    assert_eq!(scene.bands[0].members, vec!["one"]);
    assert_eq!(scene.bands[1].members, vec!["two"]);
    assert_eq!(
        scene.bands[0].gutter,
        Some(90.0),
        "the gutter the layout reserved travels with the band (S4)"
    );

    let events: Vec<&str> = scene.events.iter().map(|event| event.id.as_str()).collect();
    assert_eq!(events, vec!["g-one", "g-two"]);
    assert_eq!(scene.events[1].lane, "b");
    assert_eq!(scene.events[1].label, "barra\nde dos");
    assert_eq!(scene.events[1].order, 3);
    assert_eq!(
        scene.events[1].span,
        Some(Span {
            start: 2.0,
            end: 4.0
        })
    );
    assert_eq!(
        scene.events[1].size,
        Size::new(160.0, 40.0),
        "the lanes engine measured it"
    );
}

/// S5: the scene carries numbers, never a path. There is no `d` of a glyph and
/// no markup in the payload: the painter draws the glyphs from the tokens.
#[test]
fn the_scene_never_carries_a_path() {
    let mut ir = fixture::fixture_ir();
    if let Ir::Graph(graph) = &mut ir {
        graph.nodes[1].rows = vec![
            IrRow {
                text: "id".to_string(),
                note: Some("uuid".to_string()),
                key: gx_core::RowKey::Pk,
                nullable: false,
            },
            IrRow::new("email"),
        ];
        graph.edges[0].cardinality = Some(gx_core::Cardinality {
            from_min: gx_core::CardinalityBound::Required,
            from_max: gx_core::CardinalityMax::One,
            to_min: gx_core::CardinalityBound::Optional,
            to_max: gx_core::CardinalityMax::Many,
        });
    }
    let layout = fixture::fixture_layout();

    let scene = build_scene(&ir, &layout, &palette());
    let json = serde_json::to_string(&scene).expect("the scene serialises");

    assert!(!json.contains('<'), "no markup in the payload: {json}");
    assert!(!json.contains("\"d\":"), "no path data in the payload");
    assert!(!json.contains("marker"), "no marker of the exporter");
    assert!(
        json.contains("\"cardinality\""),
        "the relation travels as data: {json}"
    );
    assert!(
        json.contains("\"rows\""),
        "the rows of a table travel as data"
    );
}

/// S5: the tokens of the scene are the frozen numbers of the drawing, so the
/// painter and the exporter cannot disagree about a glyph.
#[test]
fn the_tokens_of_the_scene_are_the_frozen_numbers() {
    let tokens = scene_tokens();

    assert_eq!(tokens.text_size, TOKENS.text_size);
    assert_eq!(tokens.node_padding, TOKENS.node_padding);
    assert_eq!(tokens.corner_radius, TOKENS.corner_radius);
    assert_eq!(tokens.border_width, TOKENS.border_width);
    assert_eq!(tokens.line_height, TOKENS.line_height);
    assert_eq!(tokens.regular_weight, TOKENS.regular_weight);
    assert_eq!(tokens.semibold_weight, TOKENS.semibold_weight);
    assert_eq!(tokens.arrow_length, ARROW_LENGTH);
    assert_eq!(tokens.crow_foot, CROW_FOOT);
    assert_eq!(tokens.crow_foot_spread, CROW_FOOT_SPREAD);
    assert_eq!(tokens.badge_width, BADGE_WIDTH);
    assert_eq!(tokens.muted_opacity, MUTED_OPACITY);
    assert_eq!(tokens.lifeline_dash, [LIFELINE_DASH.0, LIFELINE_DASH.1]);
    assert_eq!(tokens.dash_pattern, [DASH_PATTERN.0, DASH_PATTERN.1]);
    assert_eq!(tokens.group_dash, [GROUP_DASH.0, GROUP_DASH.1]);
    assert_eq!(tokens, scene_tokens(), "deterministic");
}

/// S3: the palette of the scene names the CSS variables of the page and the
/// theme of the render; there is no colour literal in the payload.
#[test]
fn the_palette_of_the_scene_is_the_theme_as_css_vars() {
    for theme in gx_core::Theme::ALL {
        let scene = build_scene(
            &fixture::fixture_ir(),
            &fixture::fixture_layout(),
            &Palette::for_theme(theme),
        );
        assert_eq!(scene.palette.theme, theme);
        for (slot, value) in scene.palette.slots() {
            assert!(
                value.starts_with("var(--"),
                "{slot} of {theme:?} is a custom property: {value}"
            );
        }
    }
}

/// The drawing plan is the same one the emitter walks: the flags of the layers
/// are the ones that decide which `<g>` groups exist.
#[test]
fn the_drawing_plan_knows_which_layers_exist() {
    let graph = fixture::fixture_ir();
    let layout = fixture::fixture_layout();
    let drawing = Drawing::of(&graph, &layout);
    assert!(drawing.draws_clusters());
    assert!(!drawing.draws_lanes());
    assert!(!drawing.draws_lifelines());
    assert_eq!(drawing.nodes.len(), 3);
    assert_eq!(drawing.edges.len(), 2);
    assert_eq!(drawing.canvas, Rect::new(0.0, 0.0, 260.0, 170.0));

    let mut lanes_ir = Ir::Graph(IrGraph {
        direction: Direction::LR,
        layout_hint: LayoutHint::Lanes,
        nodes: vec![IrNode::new("actor", "Actor")],
        edges: vec![IrEdge::new("actor", "actor")],
        groups: Vec::new(),
    });
    if let Ir::Graph(graph) = &mut lanes_ir {
        graph.nodes[0].size = Size::new(80.0, 40.0);
    }
    let lanes_layout = fixture::layout(
        &[("actor", Point::new(40.0, 20.0), Size::new(80.0, 40.0))],
        vec![fixture::routed(
            "actor",
            "actor",
            &[(40.0, 40.0), (60.0, 40.0)],
            None,
        )],
        Rect::new(0.0, 0.0, 120.0, 100.0),
    );
    let drawing = Drawing::of(&lanes_ir, &lanes_layout);
    assert!(
        drawing.draws_lifelines(),
        "a lanes graph draws a lifeline under every actor"
    );
}
