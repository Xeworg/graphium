//! The single engine door (PRD §9.1, RNF6).
//!
//! `render_json` is the only path from JSON to SVG. `gx-mcp` and the tests
//! call exactly this function: no test of RNF6 needs Tauri, webkit or a
//! webview.
//!
//! The concrete engines are injected globally (F0 decision L7) because
//! `gx-core` cannot depend on `dagre` nor on `rustybuzz`.

use std::sync::{Arc, OnceLock, RwLock};
use std::time::Instant;

use serde_json::Value;

use crate::error::{ErrorCode, ValidationError};
use crate::ir::{Ir, IrGraph};
use crate::kind::{self, KindLimits};
use crate::layout::{LayoutEngine, LayoutOpts};
use crate::render::{RenderMetrics, RenderOpts, RenderOutcome, SvgRenderer};
use crate::text::{TextMetrics, TextStyle};
use crate::tokens::TOKENS;

/// The engines used by the door.
#[derive(Clone)]
pub struct Engines {
    pub layout: Arc<dyn LayoutEngine>,
    pub metrics: Arc<dyn TextMetrics>,
    pub renderer: Arc<dyn SvgRenderer>,
}

static ENGINES: OnceLock<RwLock<Option<Engines>>> = OnceLock::new();

fn engines_slot() -> &'static RwLock<Option<Engines>> {
    ENGINES.get_or_init(|| RwLock::new(None))
}

/// Installs the engines used by [`render_json`].
pub fn install(engines: Engines) {
    let mut slot = engines_slot().write().expect("engine slot is not poisoned");
    *slot = Some(engines);
}

/// Whether the engines are installed.
pub fn is_installed() -> bool {
    engines_slot()
        .read()
        .expect("engine slot is not poisoned")
        .is_some()
}

/// Renders a diagram document into SVG.
///
/// The pipeline is: parse → resolve kind → validate → IR → measure → check
/// limits → layout → render. Every caller gets the same behaviour.
pub fn render_json(src: &str, opts: &RenderOpts) -> Result<RenderOutcome, Vec<ValidationError>> {
    let engines = engines_slot()
        .read()
        .expect("engine slot is not poisoned")
        .clone()
        .ok_or_else(|| {
            vec![ValidationError::without_hint(
                "",
                ErrorCode::EngineUnavailable,
                "no hay motores instalados: el binario debe llamar gx_core::engine::install",
            )]
        })?;

    let document: Value = serde_json::from_str(src).map_err(|error| {
        vec![ValidationError::new(
            "",
            ErrorCode::InvalidJson,
            format!("JSON inválido: {error}"),
            Some("el documento debe ser un objeto JSON con el campo `kind`".to_string()),
        )]
    })?;

    let kind_name = document
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let kind = kind::for_kind(&kind_name).ok_or_else(|| {
        let available = kind::kind_names().join(", ");
        vec![ValidationError::new(
            "kind",
            ErrorCode::UnknownKind,
            format!("kind desconocido: `{kind_name}`"),
            Some(format!("kinds disponibles: {available}")),
        )]
    })?;

    let mut ir = kind.process_json(&document)?;

    // PRD §7.3, S21: `opts.extract` narrows the graph to one neighbourhood
    // before anything is measured, so the subgraph is what gets laid out and
    // the limits are checked against it.
    if let Some(extract) = &opts.extract {
        let narrowed = ir
            .extracted(&extract.around, extract.depth)
            .map_err(|error| vec![error])?;
        ir = narrowed;
    }

    if let Ir::Graph(graph) = &mut ir {
        // Step 1 of the ladder: without edge labels the drawing is smaller
        // and tighter, so the labels leave before the layout, not after
        // (PRD §7.3).
        if opts.degradation.omit_edge_labels {
            for edge in &mut graph.edges {
                edge.label = None;
            }
        }
        for node in &mut graph.nodes {
            // The node is its label plus its detail rows, when it has any
            // (PRD §6.1: a table is a node whose rows are its columns).
            node.size = measure_node(&*engines.metrics, node, opts.degradation.no_fine_kerning);
        }
    }

    let layout_opts = LayoutOpts {
        spacing_scale: opts.degradation.spacing_scale,
    };

    // The two IRs of the PRD: a graph goes to the graph layout, a timeline to
    // the lanes one (PRD §6: 13 kinds share `IrGraph`, 6 use `IrTimeline`).
    let layout_started = Instant::now();
    let (layout, node_count, edge_count) = match &ir {
        Ir::Graph(graph) => {
            check_limits(kind.limits(), graph)?;
            (
                engines.layout.layout(graph, &layout_opts),
                graph.nodes.len(),
                graph.edges.len(),
            )
        }
        Ir::Timeline(timeline) => {
            check_timeline_limits(kind.limits(), timeline)?;
            (
                engines.layout.layout_timeline(timeline, &layout_opts),
                timeline.events.len(),
                0,
            )
        }
    };
    let layout_ms = layout_started.elapsed().as_millis() as u64;

    let palette = opts.effective_palette();
    let render_started = Instant::now();
    let svg = if opts.export {
        engines.renderer.render_export(&ir, &layout, &palette)
    } else {
        engines.renderer.render(&ir, &layout, &palette)
    };
    // The scene belongs to the paint path, never to the export (S2): an
    // exported file is bytes on disk and has no window to mount into.
    let elements = if opts.export {
        None
    } else {
        engines.renderer.scene(&ir, &layout, &palette)
    };
    let render_ms = render_started.elapsed().as_millis() as u64;
    let svg_bytes = svg.len();

    Ok(RenderOutcome {
        svg,
        elements,
        bbox: layout.bbox,
        layout,
        metrics: RenderMetrics {
            nodes: node_count,
            edges: edge_count,
            layout_ms,
            render_ms,
            svg_bytes,
        },
    })
}

/// Measures a node: the label, padded, plus the detail rows.
fn measure_node(
    metrics: &dyn TextMetrics,
    node: &crate::ir::IrNode,
    no_fine_kerning: bool,
) -> crate::geom::Size {
    use crate::geom::Size;

    let style = |style: TextStyle| {
        if no_fine_kerning {
            style.without_kerning()
        } else {
            style
        }
    };
    let label = metrics.measure(&node.label, &style(TextStyle::semibold(TOKENS.text_size)));
    let mut width = label.w;
    let mut height = label.h + 2.0 * TOKENS.node_padding;

    if !node.rows.is_empty() {
        // The badge of the key occupies its own column, and the note is drawn
        // against the right edge: the box has to fit badge + name + note.
        let badge = metrics.measure("PK, FK", &style(TextStyle::regular(TOKENS.text_size)));
        let gap = TOKENS.node_padding;
        height += TOKENS.border_width;
        for row in &node.rows {
            let name = metrics.measure(&row.text, &style(TextStyle::regular(TOKENS.text_size)));
            let mut row_width = badge.w + gap + name.w;
            if let Some(note) = &row.note {
                let measured_note =
                    metrics.measure(note, &style(TextStyle::regular(TOKENS.text_size)));
                row_width += gap + measured_note.w;
            }
            width = width.max(row_width);
            height += name.h;
        }
        height += TOKENS.node_padding;
    }

    Size::new(width + 2.0 * TOKENS.node_padding, height)
}

/// Enforces the limits of a timeline: lanes and events (PRD §7.3).
fn check_timeline_limits(
    limits: KindLimits,
    timeline: &crate::ir::IrTimeline,
) -> Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();
    if limits.max_lanes > 0 && timeline.lanes.len() > limits.max_lanes {
        errors.push(ValidationError::new(
            "lanes",
            ErrorCode::TooLarge,
            format!(
                "{} carriles superan el límite de {}",
                timeline.lanes.len(),
                limits.max_lanes
            ),
            Some("agrupá los eventos en menos carriles".to_string()),
        ));
    }
    if limits.max_events > 0 && timeline.events.len() > limits.max_events {
        errors.push(ValidationError::new(
            "events",
            ErrorCode::TooLarge,
            format!(
                "{} eventos superan el límite de {}",
                timeline.events.len(),
                limits.max_events
            ),
            Some("partí el diagrama en varios".to_string()),
        ));
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Enforces the limits declared by the kind (PRD §7.3).
fn check_limits(limits: KindLimits, graph: &IrGraph) -> Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();
    if graph.nodes.len() > limits.max_nodes {
        errors.push(ValidationError::new(
            "nodes",
            ErrorCode::TooLarge,
            format!(
                "{} nodos superan el límite de {}",
                graph.nodes.len(),
                limits.max_nodes
            ),
            Some("partí el diagrama con opts.extract".to_string()),
        ));
    }
    if graph.edges.len() > limits.max_edges {
        errors.push(ValidationError::new(
            "edges",
            ErrorCode::TooLarge,
            format!(
                "{} aristas superan el límite de {}",
                graph.edges.len(),
                limits.max_edges
            ),
            Some("partí el diagrama con opts.extract".to_string()),
        ));
    }
    if !graph.nodes.is_empty() {
        let density = graph.edges.len() as f64 / graph.nodes.len() as f64;
        if density > limits.max_density {
            errors.push(ValidationError::new(
                "edges",
                ErrorCode::TooLarge,
                format!(
                    "densidad {:.2} supera el límite de {:.2}",
                    density, limits.max_density
                ),
                Some("redují las aristas o partí el diagrama".to_string()),
            ));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
