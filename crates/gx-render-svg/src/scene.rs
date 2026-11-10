//! The scene builder: the drawing as data (`f11-elementos.md`, T1, S3-S5).
//!
//! This module is the second emitter of `gx-render-svg`. It walks the same
//! [`Drawing`] plan the SVG emitter walks — same order, same ids, same frozen
//! numbers ([`crate::geometry`]) — and produces the [`Scene`] the window
//! mounts, instead of a string.
//!
//! What it does **not** do:
//!
//! - It ships no glyph as a path (S5): a crow's foot, a cylinder rim or a
//!   marker travels as numbers in [`SceneTokens`], and the painter draws them.
//! - It measures nothing (S4): the boxes are the [`NodeBox`]es the layout
//!   produced, and the labels are the IR labels, already broken with `\n`.
//! - It carries no colour (S3): the palette names the CSS custom properties of
//!   the page.

use std::collections::BTreeMap;

use gx_core::{
    Ir, IrEdge, IrEvent, IrGraph, IrNode, IrTimeline, LayoutAxis, LayoutEdge, LayoutGraph,
    LayoutGroup, NodeBox, Palette, Rect, Scene, SceneAxis, SceneBand, SceneEdge, SceneEvent,
    SceneNode, ScenePalette, SceneTokens, TOKENS,
};

use crate::geometry::{
    self, canvas_rect, cluster_id, edge_id, lane_id, node_id, node_rect, ARROW_LENGTH, BADGE_WIDTH,
    CROW_FOOT, CROW_FOOT_SPREAD, DASH_PATTERN, GROUP_DASH, LIFELINE_DASH, MUTED_OPACITY,
};

/// The drawing plan both emitters walk (`f11-elementos.md`, T1).
///
/// It is built once from the IR and the layout: the layers in the order they
/// are painted, with every element already paired with its measured box. The
/// SVG emitter spells it as markup, the element serializer spells it as data.
pub(crate) struct Drawing<'a> {
    /// The rectangle the drawing covers.
    pub(crate) canvas: Rect,
    graph: Option<&'a IrGraph>,
    timeline: Option<&'a IrTimeline>,
    /// The nodes of a graph, in id order and paired with their box.
    pub(crate) nodes: Vec<(&'a IrNode, &'a NodeBox)>,
    /// The events of a timeline, in id order and paired with their box.
    pub(crate) events: Vec<(&'a IrEvent, &'a NodeBox)>,
    /// The routed edges, in id order. A graph routes its own edges; a
    /// timeline routes the dependency arrows its tasks declare.
    pub(crate) edges: Vec<(IrEdge, LayoutEdge)>,
    /// The containers of a graph, outermost first.
    pub(crate) clusters: Vec<&'a LayoutGroup>,
    /// The lane bands of a timeline, in id order.
    pub(crate) lanes: Vec<&'a LayoutGroup>,
    /// The time axis of a timeline (`f22-dibujo.md`, S2).
    pub(crate) axis: Option<LayoutAxis>,
}

/// The dependency arrows of a timeline (`f18-gantt.md`, S7): one edge per
/// declared `depends_on` entry, from the dependency's event box to the
/// waiting task's box. Events missing from the layout draw nothing — the
/// layout is the authority, as everywhere else.
fn timeline_dependency_edges(
    timeline: &IrTimeline,
    layout: &LayoutGraph,
) -> Vec<(IrEdge, LayoutEdge)> {
    let placed: BTreeMap<&str, &NodeBox> = timeline
        .events
        .iter()
        .filter_map(|event| {
            layout
                .nodes
                .get(&event.id)
                .map(|placed| (event.id.as_str(), placed))
        })
        .collect();
    let mut edges = Vec::new();
    for event in geometry::ordered_events(timeline) {
        let target = match placed.get(event.id.as_str()) {
            Some(placed) => *placed,
            None => continue,
        };
        for dep in &event.depends_on {
            let source = match placed.get(dep.as_str()) {
                Some(placed) => *placed,
                None => continue,
            };
            edges.push((
                IrEdge::new(dep.clone(), event.id.clone()),
                LayoutEdge {
                    from: dep.clone(),
                    to: event.id.clone(),
                    points: geometry::dependency_route(
                        &geometry::node_rect(source),
                        &geometry::node_rect(target),
                    ),
                    label_at: None,
                },
            ));
        }
    }
    edges
}

impl<'a> Drawing<'a> {
    /// Reads the plan of a render.
    pub(crate) fn of(ir: &'a Ir, layout: &'a LayoutGraph) -> Self {
        let graph = ir.graph();
        let timeline = ir.timeline();
        let place_nodes = |graph: Option<&'a IrGraph>| match graph {
            Some(graph) => geometry::ordered_nodes(graph)
                .into_iter()
                .filter_map(|node| Some((node, layout.nodes.get(&node.id)?)))
                .collect(),
            None => Vec::new(),
        };
        let events = match timeline {
            Some(timeline) => geometry::ordered_events(timeline)
                .into_iter()
                .filter_map(|event| Some((event, layout.nodes.get(&event.id)?)))
                .collect(),
            None => Vec::new(),
        };
        let edges = match graph {
            Some(graph) => geometry::routed_edges(graph, layout)
                .into_iter()
                .map(|(edge, route)| (edge.clone(), route.clone()))
                .collect(),
            // A timeline draws only the arrows its tasks declare: one
            // dependency arrow per `depends_on` entry (`f18-gantt.md`,
            // S7), tasks in id order, dependencies in declared order.
            None => timeline_dependency_edges(timeline.expect("graph or timeline"), layout),
        };
        // The label points the canvas has to contain (`f21-roto.md`, S3):
        // every routed edge that carries a label, at the point the layout
        // chose for it. A timeline's dependency arrows carry no labels.
        let labels: Vec<(gx_core::Point, &str)> = edges
            .iter()
            .filter_map(|(edge, route)| {
                let label = edge.label.as_deref()?;
                let at = route.label_at?;
                Some((at, label))
            })
            .collect();
        Self {
            canvas: canvas_rect(layout, &labels),
            graph,
            timeline,
            nodes: place_nodes(graph),
            events,
            edges,
            clusters: match graph {
                Some(_) => geometry::ordered_clusters(layout),
                None => Vec::new(),
            },
            lanes: match timeline {
                Some(_) => geometry::ordered_groups(layout),
                None => Vec::new(),
            },
            axis: layout.axis.clone(),
        }
    }

    /// Whether the drawing has the layer of the containers of a graph.
    pub(crate) fn draws_clusters(&self) -> bool {
        self.graph.is_some()
    }

    /// Whether the drawing has the layer of the lane bands of a timeline.
    pub(crate) fn draws_lanes(&self) -> bool {
        self.timeline.is_some()
    }

    /// Whether the graph draws a lifeline under every actor (PRD §6,
    /// `sequence`).
    pub(crate) fn draws_lifelines(&self) -> bool {
        self.graph.map(geometry::draws_lifelines).unwrap_or(false)
    }

    /// The drawing as the scene the window mounts (S3).
    pub(crate) fn to_scene(&self, palette: &Palette) -> Scene {
        // The painter keys the edges by `edge.id`, so the ids of the scene must
        // be unique. `m-<from>-<to>` cannot tell two parallel edges apart, so
        // the first edge of a pair keeps the exact id of the drawing and the
        // later ones are suffixed `-2`, `-3`, … in paint order. Only the scene
        // disambiguates: the exported markup keeps the plain id.
        let mut parallel: BTreeMap<(String, String), usize> = BTreeMap::new();
        Scene {
            canvas: self.canvas,
            tokens: scene_tokens(),
            direction: self.graph.map(|graph| graph.direction),
            layout_hint: self.graph.map(|graph| graph.layout_hint),
            palette: ScenePalette::css_vars(palette.theme),
            nodes: self
                .nodes
                .iter()
                .map(|(node, placed)| {
                    let rect = node_rect(placed);
                    SceneNode {
                        id: node_id(&node.id),
                        // The label travels exactly as the IR has it: the `\n` is
                        // the wrap (S4), there is nothing to compute here.
                        label: node.label.clone(),
                        shape: node.shape,
                        emphasis: node.emphasis,
                        style: node.style,
                        // The size is the measured one, never `IrNode.size` (S4).
                        center: placed.center,
                        size: placed.size,
                        // The beside placement of the circle; `None` keeps the
                        // centered default of every other shape (S8).
                        label_at: match geometry::label_placement(node.shape, &rect) {
                            geometry::LabelPlacement::Inside => None,
                            geometry::LabelPlacement::Beside { at } => Some(at),
                        },
                        rows: node.rows.clone(),
                        stereotype: node.stereotype.clone(),
                        meta: node.meta.clone(),
                    }
                })
                .collect(),
            edges: self
                .edges
                .iter()
                .filter(|(_, route)| !route.points.is_empty())
                .map(|(edge, route)| SceneEdge {
                    id: scene_edge_id(&mut parallel, &edge.from, &edge.to),
                    from: edge.from.clone(),
                    to: edge.to.clone(),
                    label: edge.label.clone(),
                    emphasis: edge.emphasis,
                    style: edge.style,
                    arrow: edge.arrow,
                    from_row: edge.from_row,
                    to_row: edge.to_row,
                    cardinality: edge.cardinality,
                    points: route.points.clone(),
                    label_at: route.label_at,
                })
                .collect(),
            // The containers come first, then the lanes: the paint order of
            // the emitter, so a mounted scene stacks the same way.
            bands: self
                .clusters
                .iter()
                .map(|group| SceneBand {
                    id: cluster_id(&group.id),
                    label: group.label.clone(),
                    bbox: group.bbox,
                    depth: group.depth,
                    members: group.members.clone(),
                    gutter: group.gutter,
                    kind: gx_core::BandKind::Cluster,
                })
                .chain(self.lanes.iter().map(|group| SceneBand {
                    id: lane_id(&group.id),
                    label: group.label.clone(),
                    bbox: group.bbox,
                    depth: group.depth,
                    members: group.members.clone(),
                    gutter: group.gutter,
                    kind: gx_core::BandKind::Lane,
                }))
                .collect(),
            events: self
                .events
                .iter()
                .map(|(event, placed)| SceneEvent {
                    id: node_id(&event.id),
                    lane: event.lane.clone(),
                    label: event.label.clone(),
                    order: event.order,
                    shape: event.shape,
                    span: event.span,
                    center: placed.center,
                    size: placed.size,
                    // The bar of an event that lasts, as numbers: the rectangle
                    // `event_bar` decides is the one the exported markup draws
                    // (S5, T4/D).
                    bar: geometry::event_bar(&geometry::node_rect(placed), event.span),
                })
                .collect(),
            // The axis travels verbatim (`f22-dibujo.md`, L5): the layout
            // computed it, the scene carries the numbers, the painter spells
            // them — no path ships in the payload (S5).
            axis: self.axis.as_ref().map(|axis| SceneAxis {
                y: axis.y,
                x1: axis.x1,
                x2: axis.x2,
                ticks: axis.ticks.clone(),
            }),
        }
    }
}

/// The scene of a drawing (`f11-elementos.md`, T1).
pub fn scene(ir: &Ir, layout: &LayoutGraph, palette: &Palette) -> Scene {
    Drawing::of(ir, layout).to_scene(palette)
}

/// The id of one edge of the scene: unique inside a `Scene`.
///
/// The first edge between a pair of nodes carries exactly the id of the
/// drawing, [`edge_id`]; the ones that follow carry `-2`, `-3`, … in paint
/// order. The counter is per pair and deterministic, so the same document
/// always yields the same ids.
fn scene_edge_id(seen: &mut BTreeMap<(String, String), usize>, from: &str, to: &str) -> String {
    let count = seen.entry((from.to_string(), to.to_string())).or_insert(0);
    *count += 1;
    let id = edge_id(from, to);
    if *count == 1 {
        id
    } else {
        format!("{id}-{count}")
    }
}

/// The frozen numbers of the drawing (S5): the tokens of F0 plus the glyph
/// constants of the emitter.
///
/// They travel as numbers on purpose. The painter draws the crow's foot, the
/// cylinder rim, the fold of a note, the arrowheads and the lifelines from
/// these, so the payload is never "SVG again".
pub fn scene_tokens() -> SceneTokens {
    SceneTokens {
        text_size: TOKENS.text_size,
        node_padding: TOKENS.node_padding,
        corner_radius: TOKENS.corner_radius,
        border_width: TOKENS.border_width,
        line_height: TOKENS.line_height,
        regular_weight: TOKENS.regular_weight,
        semibold_weight: TOKENS.semibold_weight,
        arrow_length: ARROW_LENGTH,
        crow_foot: CROW_FOOT,
        crow_foot_spread: CROW_FOOT_SPREAD,
        badge_width: BADGE_WIDTH,
        muted_opacity: MUTED_OPACITY,
        lifeline_dash: [LIFELINE_DASH.0, LIFELINE_DASH.1],
        dash_pattern: [DASH_PATTERN.0, DASH_PATTERN.1],
        group_dash: [GROUP_DASH.0, GROUP_DASH.1],
        circle_diameter: TOKENS.circle_diameter,
    }
}

#[cfg(test)]
mod tests;
