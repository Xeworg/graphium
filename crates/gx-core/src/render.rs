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

    /// The colour of `#rrggbb`, or `None` when the text is not one.
    ///
    /// This is the format the settings file and the colour pickers of the
    /// panel use (PRD §7.1 RF25): a typo is `None`, never a panic.
    pub fn from_hex(value: &str) -> Option<Self> {
        let digits = value.strip_prefix('#')?;
        if digits.len() != 6 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let channel = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).ok();
        Some(Self {
            r: channel(0)?,
            g: channel(2)?,
            b: channel(4)?,
            a: 255,
        })
    }

    /// The same colour with the alpha of an opacity in `0.0..=1.0`.
    ///
    /// The panel keeps the opacity of the grid apart from its colour
    /// (PRD §7.1 RF25), so the palette has to rebuild one from the other.
    pub fn with_opacity(&self, opacity: f64) -> Self {
        let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round();
        Self {
            a: if alpha.is_finite() { alpha as u8 } else { 0 },
            ..*self
        }
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
///
/// The first two are the themes of F1; the four of F6 are the ones inspired
/// by the tools the human named, with names of their own (`f6-themes.md`,
/// S3, R5). The name is the one the settings file and `[data-theme]` use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    Dark,
    Light,
    Brasa,
    Arcilla,
    Aurora,
    Neblina,
    Pitaya,
}

impl Theme {
    /// Every theme, in the order the panel lists them.
    pub const ALL: [Theme; 7] = [
        Theme::Dark,
        Theme::Light,
        Theme::Brasa,
        Theme::Arcilla,
        Theme::Aurora,
        Theme::Neblina,
        Theme::Pitaya,
    ];

    /// The name of the theme: the value of `settings.json`, the radio of the
    /// panel and the `[data-theme]` of the stylesheet (PRD §8.5.2 RF22).
    pub fn as_str(&self) -> &'static str {
        match self {
            Theme::Dark => "dark",
            Theme::Light => "light",
            Theme::Brasa => "brasa",
            Theme::Arcilla => "arcilla",
            Theme::Aurora => "aurora",
            Theme::Neblina => "neblina",
            Theme::Pitaya => "pitaya",
        }
    }

    /// The theme of a name, or `None` when the name is not one.
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "dark" => Some(Theme::Dark),
            "light" => Some(Theme::Light),
            "brasa" => Some(Theme::Brasa),
            "arcilla" => Some(Theme::Arcilla),
            "aurora" => Some(Theme::Aurora),
            "neblina" => Some(Theme::Neblina),
            "pitaya" => Some(Theme::Pitaya),
            _ => None,
        }
    }
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
            Theme::Brasa => Self::brasa(),
            Theme::Arcilla => Self::arcilla(),
            Theme::Aurora => Self::aurora(),
            Theme::Neblina => Self::neblina(),
            Theme::Pitaya => Self::pitaya(),
        }
    }

    /// `brasa`: a dark terminal with an amber accent (`f6-themes.md`, S3).
    pub fn brasa() -> Self {
        Self {
            theme: Theme::Brasa,
            bg: Color::rgb(0x13, 0x13, 0x16),
            grid: Color::rgba(0xff, 0xff, 0xff, 40),
            node_fill: Color::rgb(0x1e, 0x1e, 0x24),
            node_stroke: Color::rgb(0x3d, 0x3d, 0x47),
            node_text: Color::rgb(0xf2, 0xe9, 0xe1),
            edge_stroke: Color::rgb(0x8f, 0x86, 0x7c),
            edge_text: Color::rgb(0xcb, 0xbc, 0xab),
            accent: Color::rgb(0xf5, 0xa6, 0x23),
        }
    }

    /// `arcilla`: warm cream with a terracotta accent (`f6-themes.md`, S3).
    pub fn arcilla() -> Self {
        Self {
            theme: Theme::Arcilla,
            bg: Color::rgb(0xfa, 0xf6, 0xf0),
            grid: Color::rgba(0x00, 0x00, 0x00, 22),
            node_fill: Color::rgb(0xff, 0xfd, 0xf9),
            node_stroke: Color::rgb(0xd9, 0xcb, 0xb7),
            node_text: Color::rgb(0x3a, 0x2f, 0x27),
            edge_stroke: Color::rgb(0x8a, 0x7a, 0x68),
            edge_text: Color::rgb(0x5c, 0x4d, 0x40),
            accent: Color::rgb(0xc0, 0x5a, 0x3a),
        }
    }

    /// `aurora`: a deep blue background with a violet accent
    /// (`f6-themes.md`, S3).
    pub fn aurora() -> Self {
        Self {
            theme: Theme::Aurora,
            bg: Color::rgb(0x0e, 0x16, 0x26),
            grid: Color::rgba(0xc8, 0xdc, 0xff, 32),
            node_fill: Color::rgb(0x16, 0x21, 0x3a),
            node_stroke: Color::rgb(0x2c, 0x3d, 0x5e),
            node_text: Color::rgb(0xe6, 0xec, 0xf7),
            edge_stroke: Color::rgb(0x72, 0x88, 0xb0),
            edge_text: Color::rgb(0xa9, 0xbc, 0xd9),
            accent: Color::rgb(0x9a, 0x8c, 0xf0),
        }
    }

    /// `neblina`: a soft, low-saturation grey with a teal accent
    /// (`f6-themes.md`, S3).
    pub fn neblina() -> Self {
        Self {
            theme: Theme::Neblina,
            bg: Color::rgb(0xee, 0xf1, 0xf2),
            grid: Color::rgba(0x00, 0x00, 0x00, 20),
            node_fill: Color::rgb(0xfa, 0xfb, 0xfb),
            node_stroke: Color::rgb(0xcb, 0xd3, 0xd5),
            node_text: Color::rgb(0x22, 0x30, 0x2f),
            edge_stroke: Color::rgb(0x6f, 0x80, 0x80),
            edge_text: Color::rgb(0x4c, 0x5c, 0x5d),
            accent: Color::rgb(0x2f, 0x8f, 0x86),
        }
    }

    /// `pitaya`: the night of a terminal, a rose accent and soft rose text —
    /// the look of the shell this plan lives beside.
    pub fn pitaya() -> Self {
        Self {
            theme: Theme::Pitaya,
            bg: Color::rgb(0x10, 0x0e, 0x13),
            grid: Color::rgba(0xff, 0xff, 0xff, 34),
            node_fill: Color::rgb(0x1b, 0x16, 0x1c),
            node_stroke: Color::rgb(0x4d, 0x3a, 0x45),
            node_text: Color::rgb(0xf0, 0xdc, 0xe4),
            edge_stroke: Color::rgb(0x94, 0x70, 0x7f),
            edge_text: Color::rgb(0xc9, 0x9f, 0xb0),
            accent: Color::rgb(0xe0, 0x6a, 0x8d),
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
    /// Draws only the neighbourhood of one node instead of the whole graph
    /// (PRD §7.3, S21).
    pub extract: Option<Extract>,
    /// How much of the drawing is degraded to enter the budget of
    /// PRD §7.3. The caller walks the ladder; the door only obeys.
    pub degradation: Degradation,
    /// Draws the document as a **self-contained file** (PRD §7 RF10, §8.5.2):
    /// the canvas is painted in the palette colour. The plan leaves the object
    /// transparent so the grid shows through, so this is off by default.
    pub export: bool,
    /// The palette the drawing is painted with, when the caller already
    /// resolved it: the theme plus the colours the human changed
    /// (`f6-themes.md`, R7). `None` paints with the theme alone, so the field
    /// is additive for every existing caller.
    pub palette: Option<Palette>,
}

impl RenderOpts {
    /// The palette the renderer paints with: the one the caller resolved, or
    /// the theme's own when nobody resolved one (PRD §8.5.2, R7).
    pub fn effective_palette(&self) -> Palette {
        self.palette
            .unwrap_or_else(|| Palette::for_theme(self.theme))
    }
}

/// The neighbourhood to draw instead of the whole graph (PRD §7.3, S21).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extract {
    /// `id` of the node the subgraph is built around.
    pub around: String,
    /// Hops of the walk: `1` is the parent and its direct neighbours.
    pub depth: usize,
}

/// Deadline of the layout when the caller asks for nothing (PRD §7.3, S21).
pub const DEFAULT_LAYOUT_TIMEOUT_MS: u64 = 2000;

/// Ceiling of the layout deadline, whatever the caller sends (PRD §7.3, S21).
pub const MAX_LAYOUT_TIMEOUT_MS: u64 = 10_000;

impl Default for RenderOpts {
    fn default() -> Self {
        Self {
            theme: Theme::Dark,
            direction: None,
            layout_timeout_ms: DEFAULT_LAYOUT_TIMEOUT_MS,
            extract: None,
            degradation: Degradation::NONE,
            export: false,
            palette: None,
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

    /// The drawing as a self-contained document: the canvas painted in the
    /// palette colour (PRD §8.5.2, S7).
    ///
    /// The default is the plain drawing, which is right for anything that
    /// paints its own background; the real renderer overrides it, because an
    /// exported file has nobody behind it.
    fn render_export(&self, ir: &Ir, layout: &LayoutGraph, palette: &Palette) -> String {
        self.render(ir, layout, palette)
    }
}

#[cfg(test)]
mod tests {
    use super::{Color, Palette, RenderOpts, Theme};

    /// The relative luminance of WCAG 2.1.
    fn luminance(color: Color) -> f64 {
        let channel = |value: u8| {
            let value = f64::from(value) / 255.0;
            if value <= 0.039_28 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(color.r) + 0.7152 * channel(color.g) + 0.0722 * channel(color.b)
    }

    /// The contrast ratio of two colours (WCAG 2.1): from 1.0 up to 21.
    fn contrast(a: Color, b: Color) -> f64 {
        let (la, lb) = (luminance(a), luminance(b));
        let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// R5, R6: the five new themes are ordinary themes, named in lowercase
    /// like `plano` and `claro`, and they travel as snake_case in JSON: the
    /// name is the one `settings.json` and `[data-theme]` use.
    #[test]
    fn the_seven_themes_are_named_in_lowercase() {
        assert_eq!(
            Theme::ALL.len(),
            7,
            "F6 adds four themes and the selector adds pitaya"
        );
        for theme in Theme::ALL {
            let name = theme.as_str();
            assert_eq!(
                name,
                name.to_lowercase(),
                "the UI writes the themes in lowercase: {name}"
            );
            assert_eq!(Theme::parse(name), Some(theme), "{name} round-trips");
            assert_eq!(
                serde_json::to_string(&theme).expect("the theme serialises"),
                format!("\"{name}\""),
                "the settings file and the stylesheet share one name per theme"
            );
        }
        for name in ["brasa", "arcilla", "aurora", "neblina", "pitaya"] {
            assert!(Theme::parse(name).is_some(), "the new theme {name} exists");
        }
        // `claro` is the preset, `light` is the theme (F1, S18).
        assert_eq!(Theme::parse("claro"), None);
        assert_eq!(Theme::parse(" BRASA "), Some(Theme::Brasa));
    }

    /// R5, R8: every theme is its own palette, and every palette keeps the
    /// drawings readable: the label of a node over its own fill, the edge and
    /// the accent over the background.
    #[test]
    fn every_theme_owns_a_palette_with_readable_contrast() {
        let palettes: Vec<Palette> = Theme::ALL.into_iter().map(Palette::for_theme).collect();
        for (index, palette) in palettes.iter().enumerate() {
            assert_eq!(
                palette.theme,
                Theme::ALL[index],
                "the palette is of its theme"
            );
            for other in palettes.iter().skip(index + 1) {
                assert_ne!(
                    palette.bg, other.bg,
                    "{:?} shares its background",
                    palette.theme
                );
                assert_ne!(
                    palette.node_fill, other.node_fill,
                    "the fill of {:?} is the fill of {:?}",
                    palette.theme, other.theme
                );
            }
            assert!(
                contrast(palette.node_text, palette.node_fill) >= 4.5,
                "{:?}: the node label ({}) does not read over its fill ({})",
                palette.theme,
                palette.node_text.hex(),
                palette.node_fill.hex()
            );
            assert!(
                contrast(palette.edge_stroke, palette.bg) >= 3.0,
                "{:?}: the edges ({}) vanish in the background ({})",
                palette.theme,
                palette.edge_stroke.hex(),
                palette.bg.hex()
            );
            assert!(
                contrast(palette.edge_text, palette.bg) >= 4.5,
                "{:?}: the edge label ({}) does not read over the background",
                palette.theme,
                palette.edge_text.hex()
            );
            assert!(
                contrast(palette.accent, palette.bg) >= 3.0,
                "{:?}: the accent ({}) vanishes in the background",
                palette.theme,
                palette.accent.hex()
            );
            assert_ne!(
                palette.bg, palette.node_fill,
                "{:?}: no flat box",
                palette.theme
            );
        }
    }

    /// R7: the render paints with the palette the caller resolved (theme plus
    /// the colours the human changed), or with the theme's own when nobody
    /// resolved one. The field is additive: the default keeps working.
    #[test]
    fn the_render_options_carry_the_effective_palette() {
        let default = RenderOpts::default();
        assert_eq!(
            default.effective_palette(),
            Palette::for_theme(default.theme)
        );
        assert_eq!(default.palette, None);

        let custom = Palette {
            accent: Color::rgb(0x01, 0x02, 0x03),
            ..Palette::aurora()
        };
        let opts = RenderOpts {
            palette: Some(custom),
            ..RenderOpts::default()
        };
        assert_eq!(opts.effective_palette(), custom);
    }

    /// R4: a colour of the settings file is `#rrggbb`; the grid takes its
    /// alpha from the opacity of the panel.
    #[test]
    fn a_colour_is_read_from_its_hex_and_the_alpha_from_the_opacity() {
        assert_eq!(
            Color::from_hex("#1b1b1b"),
            Some(Color::rgb(0x1b, 0x1b, 0x1b))
        );
        assert_eq!(
            Color::from_hex("#FFFFFF"),
            Some(Color::rgb(0xff, 0xff, 0xff))
        );
        assert_eq!(Color::from_hex("#1b1b"), None);
        assert_eq!(Color::from_hex("1b1b1b"), None);
        assert_eq!(Color::from_hex("#gggggg"), None);
        assert_eq!(Color::rgb(1, 2, 3).hex(), "#010203");
        assert_eq!(Color::rgb(1, 2, 3).with_opacity(1.0).a, 255);
        assert_eq!(Color::rgb(1, 2, 3).with_opacity(0.0).a, 0);
        assert_eq!(Color::rgb(1, 2, 3).with_opacity(0.14).a, 36);
        assert_eq!(Color::rgb(1, 2, 3).with_opacity(2.0).a, 255);
        assert_eq!(Color::rgb(1, 2, 3).with_opacity(f64::NAN).a, 0);
    }
}
