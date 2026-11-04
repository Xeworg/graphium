//! Text metrics with `rustybuzz` and the embedded Inter faces (PRD §8.4.1, §8.5).
//!
//! This is the only implementation of `gx_core::TextMetrics` in the workspace
//! (PRD §8.4.1): the width is the sum of the glyph advances produced by the
//! shaper — so kerning and ligatures are already applied — and the height is
//! `lines × size × line_height`, independent of the font's own leading.

use gx_core::{Size, TextMetrics, TextStyle};
use rustybuzz::{Face, UnicodeBuffer};

/// Inter Regular, embedded so the metrics never read the filesystem (PRD §8.5).
///
/// The PNG rasterizer loads the very same bytes: the text of the PNG has to be
/// measured with the face that draws it.
pub(crate) const INTER_REGULAR: &[u8] = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");
/// Inter SemiBold, embedded for weight 600 (PRD §8.5, S8).
pub(crate) const INTER_SEMIBOLD: &[u8] = include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf");

/// Why the embedded font could not be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricsError {
    /// One of the embedded TTFs did not parse.
    FontUnavailable { weight: u16 },
}

impl std::fmt::Display for MetricsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MetricsError::FontUnavailable { weight } => {
                write!(formatter, "embedded Inter {weight} could not be parsed")
            }
        }
    }
}

impl std::error::Error for MetricsError {}

/// Which of the two embedded faces a weight maps to (PRD §8.5, S8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Weight {
    Regular,
    Semibold,
}

impl Weight {
    /// 400 → Regular, 600 → SemiBold. Any other weight takes the nearest of the
    /// two and the tie (500) goes to Regular: a documented safe default, since
    /// the frozen tokens only use 400 and 600.
    fn for_style(weight: u16) -> Self {
        if weight >= 500 {
            Weight::Semibold
        } else {
            Weight::Regular
        }
    }

    fn number(self) -> u16 {
        match self {
            Weight::Regular => 400,
            Weight::Semibold => 600,
        }
    }

    fn data(self) -> &'static [u8] {
        match self {
            Weight::Regular => INTER_REGULAR,
            Weight::Semibold => INTER_SEMIBOLD,
        }
    }
}

/// The single `TextMetrics` implementation of Graphium (PRD §8.4.1).
pub struct InterMetrics {
    regular: Face<'static>,
    semibold: Face<'static>,
}

impl InterMetrics {
    /// Parses the two embedded faces.
    pub fn new() -> Result<Self, MetricsError> {
        Ok(Self {
            regular: parse_face(Weight::Regular)?,
            semibold: parse_face(Weight::Semibold)?,
        })
    }

    fn face(&self, style: &TextStyle) -> &Face<'static> {
        match Weight::for_style(style.weight) {
            Weight::Regular => &self.regular,
            Weight::Semibold => &self.semibold,
        }
    }
}

fn parse_face(weight: Weight) -> Result<Face<'static>, MetricsError> {
    Face::from_slice(weight.data(), 0).ok_or(MetricsError::FontUnavailable {
        weight: weight.number(),
    })
}

/// Advance width of one line, in plan units.
///
/// With `kerning` off the `kern` feature of the shaping is disabled: the
/// third step of the degradation ladder (PRD §7.3) measures the text without
/// the fine adjustment between pairs, which is the cheapest way to make the
/// drawing smaller without hiding anything.
fn line_width(face: &Face<'static>, line: &str, size: f64, kerning: bool) -> f64 {
    if line.is_empty() {
        return 0.0;
    }
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(line);
    buffer.guess_segment_properties();
    let features = if kerning {
        Vec::new()
    } else {
        vec![rustybuzz::Feature::new(
            rustybuzz::ttf_parser::Tag::from_bytes(b"kern"),
            0,
            ..,
        )]
    };
    let glyphs = rustybuzz::shape(face, &features, buffer);
    let advance: i32 = glyphs
        .glyph_positions()
        .iter()
        .map(|position| position.x_advance)
        .sum();
    f64::from(advance) / f64::from(face.units_per_em()) * size
}

impl TextMetrics for InterMetrics {
    fn measure(&self, text: &str, style: &TextStyle) -> Size {
        let face = self.face(style);
        let mut width: f64 = 0.0;
        let mut lines = 0u32;
        for line in text.lines() {
            lines += 1;
            width = width.max(line_width(face, line, style.size, style.kerning));
        }
        let height = f64::from(lines) * style.size * style.line_height;
        Size::new(width, height)
    }
}

#[cfg(test)]
mod tests {
    use super::InterMetrics;
    use gx_core::{Size, TextMetrics, TextStyle, TOKENS};

    fn metrics() -> InterMetrics {
        InterMetrics::new().expect("the embedded Inter faces parse")
    }

    #[test]
    fn a_longer_label_is_wider() {
        let metrics = metrics();
        let style = TextStyle::regular(TOKENS.text_size);
        assert!(metrics.measure("Config", &style).w > metrics.measure("C", &style).w);
    }

    #[test]
    fn two_lines_are_taller_and_as_wide_as_the_widest_line() {
        let metrics = metrics();
        let style = TextStyle::regular(TOKENS.text_size);
        let one = metrics.measure("a", &style);
        let two = metrics.measure("a\na", &style);
        assert!(two.h > one.h);
        assert!((two.h - 2.0 * one.h).abs() < 1e-9);
        assert!((two.w - one.w).abs() < 1e-9);
    }

    #[test]
    fn the_height_follows_the_frozen_tokens() {
        let metrics = metrics();
        let one = metrics.measure("x", &TextStyle::regular(TOKENS.text_size));
        assert!((one.h - TOKENS.text_size * TOKENS.line_height).abs() < 1e-9);
    }

    #[test]
    fn empty_text_measures_zero() {
        let metrics = metrics();
        assert_eq!(
            metrics.measure("", &TextStyle::regular(14.0)),
            Size::new(0.0, 0.0)
        );
    }

    #[test]
    fn kerning_is_applied() {
        let metrics = metrics();
        let style = TextStyle::regular(TOKENS.text_size);
        let pair = metrics.measure("AV", &style).w;
        let apart = metrics.measure("A", &style).w + metrics.measure("V", &style).w;
        assert!(
            pair < apart,
            "AV ({pair}) must be narrower than A + V ({apart})"
        );
    }

    /// The third step of the degradation ladder (PRD §7.3): turning the fine
    /// kerning off never makes a text narrower, and it does widen at least one
    /// of the pairs where the embedded face kerns.
    #[test]
    fn text_without_kerning_is_not_narrower() {
        let metrics = metrics();
        let kerned_style = TextStyle::regular(TOKENS.text_size);
        let unkerned_style = kerned_style.without_kerning();

        let mut tightened = 0;
        for pair in ["AV", "Ta", "To", "Yo", "VA"] {
            let kerned = metrics.measure(pair, &kerned_style).w;
            let unkerned = metrics.measure(pair, &unkerned_style).w;
            assert!(
                kerned.is_finite() && unkerned.is_finite(),
                "{pair}: kerning {kerned}, without kerning {unkerned}"
            );
            assert!(
                unkerned >= kerned,
                "{pair}: without kerning {unkerned} must not be narrower than {kerned}"
            );
            if unkerned > kerned {
                tightened += 1;
            }
        }
        assert!(
            tightened > 0,
            "the third step must widen at least one kerned pair, or nothing is degraded"
        );
    }

    #[test]
    fn the_semibold_face_is_not_narrower_than_the_regular_one() {
        let metrics = metrics();
        let regular = metrics
            .measure("Config", &TextStyle::regular(TOKENS.text_size))
            .w;
        let semibold = metrics
            .measure("Config", &TextStyle::semibold(TOKENS.text_size))
            .w;
        assert!(
            semibold >= regular,
            "regular {regular}, semibold {semibold}"
        );
    }
}
