//! Render contract and palette (PRD §8.5, §8.5.2).

use serde::{Deserialize, Serialize};

use crate::geom::Rect;
use crate::ir::{Direction, Ir};
use crate::layout::LayoutGraph;

/// An RGBA color. The palette never reads the filesystem, the environment or
/// the clock (PRD §8.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// `#rrggbb`, ignoring alpha.
    pub fn hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// CSS `rgba(r, g, b, a)` with the alpha as a 0..1 fraction.
    pub fn css(&self) -> String {
        let alpha = f64::from(self.a) / 255.0;
        format!(
            "rgba({}, {}, {}, {})",
            self.r,
            self.g,
            self.b,
            crate::fmt::fmt_num(alpha)
        )
    }
}

/// Theme of the plan (PRD §8.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    Dark,
    Light,
}

/// The resolved palette used by the renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Palette {
    pub theme: Theme,
    pub bg: Color,
    pub grid: Color,
    pub node_fill: Color,
    pub node_stroke: Color,
    pub node_text: Color,
    pub edge_stroke: Color,
    pub edge_text: Color,
    pub accent: Color,
}

impl Palette {
    /// The `plano` preset: base `#1b1b1b` with the white grid line at 14% of
    /// opacity (PRD §7.1, RF24).
    pub fn plano() -> Self {
        Self {
            theme: Theme::Dark,
            bg: Color::rgb(0x1b, 0x1b, 0x1b),
            grid: Color::rgba(0xff, 0xff, 0xff, 36),
            node_fill: Color::rgb(0x2a, 0x2a, 0x30),
            node_stroke: Color::rgb(0x4d, 0x4d, 0x57),
            node_text: Color::rgb(0xe9, 0xe9, 0xec),
            edge_stroke: Color::rgb(0x8a, 0x8a, 0x96),
            edge_text: Color::rgb(0xb9, 0xb9, 0xc2),
            accent: Color::rgb(0x6e, 0xa8, 0xfe),
        }
    }

    /// The `claro` preset: a real theme, not a white background (PRD §8.5.2).
    pub fn claro() -> Self {
        Self {
            theme: Theme::Light,
            bg: Color::rgb(0xf7, 0xf7, 0xf8),
            grid: Color::rgba(0x00, 0x00, 0x00, 24),
            node_fill: Color::rgb(0xff, 0xff, 0xff),
            node_stroke: Color::rgb(0xc7, 0xc7, 0xcf),
            node_text: Color::rgb(0x1b, 0x1b, 0x1f),
            edge_stroke: Color::rgb(0x6f, 0x6f, 0x7a),
            edge_text: Color::rgb(0x4a, 0x4a, 0x54),
            accent: Color::rgb(0x2f, 0x6f, 0xd0),
        }
    }

    /// The palette of a theme.
    pub fn for_theme(theme: Theme) -> Self {
        match theme {
            Theme::Dark => Self::plano(),
            Theme::Light => Self::claro(),
        }
    }
}

/// Options of a render run.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderOpts {
    pub theme: Theme,
    /// Overrides the direction declared by the diagram.
    pub direction: Option<Direction>,
    /// Deadline of the layout phase, in milliseconds (PRD §7.3).
    pub layout_timeout_ms: u64,
    /// How much of the drawing is degraded to enter the budget of
    /// PRD §7.3. The caller walks the ladder; the door only obeys.
    pub degradation: Degradation,
}

impl Default for RenderOpts {
    fn default() -> Self {
        Self {
            theme: Theme::Dark,
            direction: None,
            layout_timeout_ms: 2000,
            degradation: Degradation::NONE,
        }
    }
}

/// The spacing of the second step of the ladder: 70% of the default
/// (PRD §7.3).
pub const DEGRADED_SPACING_SCALE: f64 = 0.7;

/// The degradation ladder of PRD §7.3, in order and without asking.
///
/// Each level adds one step to the previous one: the caller picks the level
/// it needs, and the drawing is never degraded more than asked.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Degradation {
    /// Step 1: the labels of the edges are not drawn.
    pub omit_edge_labels: bool,
    /// Step 2: the separations between nodes are multiplied by this.
    pub spacing_scale: f64,
    /// Step 3: the text is measured without fine kerning.
    pub no_fine_kerning: bool,
}

impl Degradation {
    /// No degradation at all.
    pub const NONE: Self = Self {
        omit_edge_labels: false,
        spacing_scale: 1.0,
        no_fine_kerning: false,
    };

    /// Step 1 only.
    pub const fn step_one() -> Self {
        Self {
            omit_edge_labels: true,
            ..Self::NONE
        }
    }

    /// Steps 1 and 2.
    pub const fn step_two() -> Self {
        Self {
            omit_edge_labels: true,
            spacing_scale: DEGRADED_SPACING_SCALE,
            no_fine_kerning: false,
        }
    }

    /// Steps 1, 2 and 3.
    pub const fn step_three() -> Self {
        Self {
            omit_edge_labels: true,
            spacing_scale: DEGRADED_SPACING_SCALE,
            no_fine_kerning: true,
        }
    }

    /// The steps that are on, in order: `1`, `2`, `3`.
    pub fn steps(&self) -> Vec<u8> {
        let mut steps = Vec::new();
        if self.omit_edge_labels {
            steps.push(1);
        }
        if self.spacing_scale < 1.0 {
            steps.push(2);
        }
        if self.no_fine_kerning {
            steps.push(3);
        }
        steps
    }
}

/// Cost of a render, reported to the agent (PRD §7.3, RF20-bis).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderMetrics {
    pub nodes: usize,
    pub edges: usize,
    pub layout_ms: u64,
    pub render_ms: u64,
    pub svg_bytes: usize,
}

/// The result of the single engine door.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderOutcome {
    pub svg: String,
    pub layout: LayoutGraph,
    pub bbox: Rect,
    pub metrics: RenderMetrics,
}

/// The SVG renderer behind a trait (PRD §8.5).
pub trait SvgRenderer: Send + Sync {
    fn render(&self, ir: &Ir, layout: &LayoutGraph, palette: &Palette) -> String;
}
