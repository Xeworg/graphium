//! Visual tokens frozen in F0 (PRD §8.5).
//!
//! The golden SVG hashes compare byte by byte, so these values only change in
//! a commit that updates every hash.

/// The frozen visual tokens of the renderer and the layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tokens {
    /// Font size of node labels, in plan units.
    pub text_size: f64,
    /// Padding around the label inside a node box.
    pub node_padding: f64,
    /// Corner radius of rounded nodes.
    pub corner_radius: f64,
    /// Border width of nodes and edges.
    pub border_width: f64,
    /// Line height multiplier.
    pub line_height: f64,
    /// Font weight of the regular text.
    pub regular_weight: u16,
    /// Font weight of emphasized text.
    pub semibold_weight: u16,
}

impl Default for Tokens {
    fn default() -> Self {
        Self {
            text_size: 14.0,
            node_padding: 12.0,
            corner_radius: 6.0,
            border_width: 1.0,
            line_height: 1.25,
            regular_weight: 400,
            semibold_weight: 600,
        }
    }
}

/// The frozen tokens of Graphium.
pub const TOKENS: Tokens = Tokens {
    text_size: 14.0,
    node_padding: 12.0,
    corner_radius: 6.0,
    border_width: 1.0,
    line_height: 1.25,
    regular_weight: 400,
    semibold_weight: 600,
};

#[cfg(test)]
mod tests {
    use super::TOKENS;

    #[test]
    fn tokens_are_the_frozen_ones() {
        assert_eq!(TOKENS.text_size, 14.0);
        assert_eq!(TOKENS.node_padding, 12.0);
        assert_eq!(TOKENS.corner_radius, 6.0);
        assert_eq!(TOKENS.border_width, 1.0);
        assert_eq!(TOKENS.line_height, 1.25);
        assert_eq!(TOKENS.regular_weight, 400);
        assert_eq!(TOKENS.semibold_weight, 600);
    }
}
