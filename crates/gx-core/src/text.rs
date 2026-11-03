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
    /// Whether the fine kerning of the pair is applied. The third step of
    /// the degradation ladder turns it off (PRD §7.3).
    pub kerning: bool,
}

impl TextStyle {
    pub const fn new(size: f64, weight: u16, line_height: f64) -> Self {
        Self {
            size,
            weight,
            line_height,
            kerning: true,
        }
    }

    pub const fn regular(size: f64) -> Self {
        Self::new(size, 400, 1.25)
    }

    pub const fn semibold(size: f64) -> Self {
        Self::new(size, 600, 1.25)
    }

    /// The same style, measured without fine kerning.
    pub const fn without_kerning(self) -> Self {
        Self {
            kerning: false,
            ..self
        }
    }
}

/// The single source of text metrics.
pub trait TextMetrics: Send + Sync {
    /// Measures a text block (multi-line with `\n`).
    fn measure(&self, text: &str, style: &TextStyle) -> Size;
}
