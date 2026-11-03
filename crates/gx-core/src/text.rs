//! Text measurement seam (PRD §8.4.1).
//!
//! Only `gx-render-svg` implements [`TextMetrics`]. Layout receives the
//! measurer as a parameter, so `gx-layout` never depends on the renderer and
//! never measures text on its own.

use crate::geom::Size;

/// Style of a text run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    /// Font size in plan units.
    pub size: f64,
    /// Font weight (400 regular, 600 semibold).
    pub weight: u16,
    /// Line height multiplier.
    pub line_height: f64,
}

impl TextStyle {
    pub const fn new(size: f64, weight: u16, line_height: f64) -> Self {
        Self {
            size,
            weight,
            line_height,
        }
    }

    pub const fn regular(size: f64) -> Self {
        Self::new(size, 400, 1.25)
    }

    pub const fn semibold(size: f64) -> Self {
        Self::new(size, 600, 1.25)
    }
}

/// The single source of text metrics.
pub trait TextMetrics: Send + Sync {
    /// Measures a text block (multi-line with `\n`).
    fn measure(&self, text: &str, style: &TextStyle) -> Size;
}
