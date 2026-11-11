//! `gx-core` — the contracts of Graphium.
//!
//! This crate owns the IR, the validation errors, the kind trait and the
//! registry, the text metrics seam, the layout and render contracts and the
//! single engine door ([`engine::render_json`]). It depends on no other
//! `gx-*` crate (PRD §9.1).

pub mod engine;
pub mod error;
pub mod export;
pub mod fmt;
pub mod geom;
pub mod ir;
pub mod kind;
pub mod layout;
pub mod render;
pub mod scene;
pub mod text;
pub mod tokens;

pub use engine::{install, render_json, Engines};
pub use error::{ErrorCode, ValidationError};
pub use export::{
    install_rasterizer, rasterizer, ExportFormat, PathError, RasterError, RasterImage,
    RasterOptions, Rasterizer,
};
pub use fmt::fmt_num;
pub use geom::{Point, Rect, Size};
pub use ir::{
    Cardinality, CardinalityBound, CardinalityMax, Direction, Emphasis, Ir, IrConnection, IrEdge,
    IrEvent, IrGraph, IrGroup, IrLane, IrNode, IrRow, IrTimeline, IrValue, LayoutHint, RowKey,
    Shape, Style,
};
pub use kind::{for_kind, kind_names, register_kind, DiagramKind, KindHandler, KindLimits};
pub use layout::{
    connector_route, LayoutActivation, LayoutAxis, LayoutEdge, LayoutEngine, LayoutGraph,
    LayoutGroup, NodeBox,
};
pub use render::{Color, Palette, RenderMetrics, RenderOpts, RenderOutcome, SvgRenderer, Theme};
pub use scene::{
    BandKind, Scene, SceneActivation, SceneAxis, SceneBand, SceneEdge, SceneEvent, SceneNode,
    ScenePalette, SceneTokens,
};
pub use text::{TextMetrics, TextStyle};
pub use tokens::{Tokens, TOKENS};
