//! `gx-kinds` — the diagram kinds of Graphium.
//!
//! A kind is a module with a JSON contract, its semantic rules and its
//! translation to the IR. Adding a kind never touches `gx-mcp` nor `gx-app`
//! (PRD §7, O7).

pub mod er;
pub mod flow;
pub mod flowchart;
pub mod graph;
pub mod lanes;
pub mod mindmap;
pub mod sequence;
pub mod state;
pub mod swimlane;
pub mod timeline;
pub mod tree;

use gx_core::kind::DiagramKind;

/// Registers every kind of this crate in the global registry.
pub fn register_all() {
    graph::register();
    er::register();
    tree::register();
    mindmap::register();
    sequence::register();
    flowchart::register();
    state::register();
    timeline::register();
    swimlane::register();
}

/// The names of the kinds registered by this crate.
pub fn kind_names() -> Vec<&'static str> {
    vec![
        graph::GraphKind::KIND,
        er::ErKind::KIND,
        tree::TreeKind::KIND,
        mindmap::MindmapKind::KIND,
        sequence::SequenceKind::KIND,
        flowchart::FlowchartKind::KIND,
        state::StateKind::KIND,
        timeline::TimelineKind::KIND,
        swimlane::SwimlaneKind::KIND,
    ]
}
