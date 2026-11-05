//! The settings of the plan: what the human chose and where it is kept
//! (PRD §7.1 RF25, §8.5.2 RF22, S19).
//!
//! The look of the plan belongs to the human. The agent never changes it, so
//! this module has no tool behind it: the window reads the file at start and
//! writes it when the panel changes something.
//!
//! Every colour of the palette is a setting (`f6-themes.md`, R1, R2): the
//! theme is the base and the fields below are the colours the human changed
//! on top of it. A broken field falls back to the value of its theme instead
//! of taking the window down: the file is edited by hand, and a typo in one
//! colour is not a reason to lose the plan (R4).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use gx_core::render::{Color, Palette, Theme};

/// Opacity of the grid of the `plano` preset (PRD §8.5.2: 14%).
const PLANO_GRID_OPACITY: f64 = 0.14;
/// Separations the panel lets the human choose, in plan units.
const DIAGONAL_RANGE: std::ops::RangeInclusive<f64> = 20.0..=200.0;
/// Angles the panel lets the human choose, in degrees.
///
/// The ends are open on purpose. The separation of the grid is the major
/// diagonal of the cell, so at 0 degrees the step collapses to zero (a solid
/// wash) and at 90 the two families of lines become one: outside 15..75 the
/// cell stops being a rhombus and the geometry stops meaning what the PRD
/// says it means.
const ANGLE_RANGE: std::ops::RangeInclusive<f64> = 15.0..=75.0;

/// The presets the panel offers, in the order it shows them: the three of F1
/// and the five themes of F6 plus `pitaya` (`f6-themes.md`, R5, R6).
pub const PRESET_NAMES: [&str; 8] = [
    "plano",
    "claro",
    "sin grilla",
    "brasa",
    "arcilla",
    "aurora",
    "neblina",
    "pitaya",
];

/// What the human chose for the plan (PRD §7.1 RF25).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Background of the plan: `#rrggbb`.
    pub background: String,
    /// Colour of the grid: `#rrggbb`.
    pub grid_color: String,
    /// Opacity of the grid, `0.0` to `1.0`.
    pub grid_opacity: f64,
    /// Separation of the grid: the diagonal of one cell, in plan units.
    pub grid_diagonal: f64,
    /// Deviation of the grid lines from the vertical, in degrees.
    pub grid_angle_degrees: f64,
    /// Whether the grid is painted at all.
    pub grid_visible: bool,
    /// The base of the colours: `dark`, `light`, `brasa`, `arcilla`,
    /// `aurora`, `neblina` or `pitaya` (PRD §8.5.2 RF22, `f6-themes.md` R5).
    pub theme: String,
    /// Fill of a node: `#rrggbb`.
    pub node_fill: String,
    /// Border of a node: `#rrggbb`.
    pub node_stroke: String,
    /// Label of a node: `#rrggbb`.
    pub node_text: String,
    /// Line of an edge: `#rrggbb`.
    pub edge_stroke: String,
    /// Label of an edge: `#rrggbb`.
    pub edge_text: String,
    /// Highlight of the drawing: markers, glyphs and the selected elements.
    pub accent: String,
}

impl Default for Settings {
    fn default() -> Self {
        presets::plano()
    }
}

/// The presets of the plan (PRD §8.5.2 RF24, RF25; `f6-themes.md`, R6).
pub mod presets {
    use super::{Settings, PLANO_GRID_OPACITY};
    use gx_core::render::{Palette, Theme};

    /// The settings of a theme: the palette of the theme in the fields of the
    /// panel, with the grid of the `plano` preset (`f6-themes.md`, R1, R6).
    ///
    /// The colours come from `Palette::for_theme`, so a theme has one look:
    /// the preset, the stylesheet and the export cannot drift apart.
    pub fn of_theme(theme: Theme) -> Settings {
        let palette = Palette::for_theme(theme);
        Settings {
            background: palette.bg.hex(),
            grid_color: palette.grid.hex(),
            grid_opacity: grid_opacity(theme),
            grid_diagonal: 59.0,
            grid_angle_degrees: 45.0,
            grid_visible: true,
            theme: theme.as_str().to_string(),
            node_fill: palette.node_fill.hex(),
            node_stroke: palette.node_stroke.hex(),
            node_text: palette.node_text.hex(),
            edge_stroke: palette.edge_stroke.hex(),
            edge_text: palette.edge_text.hex(),
            accent: palette.accent.hex(),
        }
    }

    /// The opacity of the grid of each theme: the 14% of the PRD for `plano`
    /// and a value per theme so the mesh reads without turning into noise.
    /// It is the suggestion of the theme, never a limit: RF25 lets the human
    /// change it.
    fn grid_opacity(theme: Theme) -> f64 {
        match theme {
            Theme::Dark => PLANO_GRID_OPACITY,
            Theme::Light => 0.10,
            Theme::Brasa => 0.16,
            Theme::Arcilla => 0.10,
            Theme::Aurora => 0.13,
            Theme::Neblina => 0.10,
            Theme::Pitaya => 0.13,
        }
    }

    /// The dark plan of the reference: dark background and a white grid.
    pub fn plano() -> Settings {
        of_theme(Theme::Dark)
    }

    /// The light theme: a real theme, not a white background. Nodes, labels
    /// and arrows keep their contrast, so the drawing is still readable.
    pub fn claro() -> Settings {
        of_theme(Theme::Light)
    }

    /// `brasa` (`f6-themes.md`, R5): a terminal dark with an amber accent.
    pub fn brasa() -> Settings {
        of_theme(Theme::Brasa)
    }

    /// `arcilla` (`f6-themes.md`, R5): warm cream with a terracotta accent.
    pub fn arcilla() -> Settings {
        of_theme(Theme::Arcilla)
    }

    /// `aurora` (`f6-themes.md`, R5): deep blue with a violet accent.
    pub fn aurora() -> Settings {
        of_theme(Theme::Aurora)
    }

    /// `neblina` (`f6-themes.md`, R5): soft grey with a teal accent.
    pub fn neblina() -> Settings {
        of_theme(Theme::Neblina)
    }

    /// `pitaya`: a near-black night with a rose accent and soft rose text,
    /// the look of the shell this plan lives beside.
    pub fn pitaya() -> Settings {
        of_theme(Theme::Pitaya)
    }

    /// The dark plan without the grid.
    pub fn sin_grilla() -> Settings {
        Settings {
            grid_visible: false,
            ..plano()
        }
    }
}

/// A preset by name, or `None` when the name is not one of the presets of
/// `PRESET_NAMES`.
pub fn preset(name: &str) -> Option<Settings> {
    match name.trim().to_lowercase().as_str() {
        "plano" => Some(presets::plano()),
        "claro" => Some(presets::claro()),
        "sin grilla" | "sin_grilla" | "singrilla" => Some(presets::sin_grilla()),
        "brasa" => Some(presets::brasa()),
        "arcilla" => Some(presets::arcilla()),
        "aurora" => Some(presets::aurora()),
        "neblina" => Some(presets::neblina()),
        "pitaya" => Some(presets::pitaya()),
        _ => None,
    }
}

impl Settings {
    /// `~/.graphium/settings.json`, when the home directory is known.
    pub fn path() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        Some(PathBuf::from(home).join(".graphium").join("settings.json"))
    }

    /// The settings of the human, or the default when there is no file.
    pub fn load() -> Settings {
        match Self::path() {
            Some(path) => Self::load_from(path),
            None => Settings::default(),
        }
    }

    /// The settings of a file, made sane.
    ///
    /// A missing file, a file that is not JSON and every field that makes no
    /// sense end in the default of that field: the plan opens with something
    /// readable even when the settings are broken.
    pub fn load_from(path: impl AsRef<Path>) -> Settings {
        let Ok(text) = fs::read_to_string(path) else {
            return Settings::default();
        };
        let parsed: Settings = match serde_json::from_str(&text) {
            Ok(settings) => settings,
            Err(_) => return Settings::default(),
        };
        parsed.sanitized()
    }

    /// Keeps the sane fields and replaces the rest with a value of the theme.
    ///
    /// The theme is decided first, because it is the base of every colour: a
    /// colour that cannot be read falls back to the palette of the theme the
    /// human chose, never to the default theme (`f6-themes.md`, R4). The
    /// fields that are not colours (the opacity of the grid, its geometry)
    /// keep their own default.
    pub fn sanitized(self) -> Settings {
        let theme = Theme::parse(&self.theme).unwrap_or_else(|| Settings::default().theme());
        let palette = Palette::for_theme(theme);
        let fallback = Settings::default();
        Settings {
            background: color_or(self.background, &palette.bg.hex()),
            grid_color: color_or(self.grid_color, &palette.grid.hex()),
            grid_opacity: if self.grid_opacity.is_finite() {
                self.grid_opacity.clamp(0.0, 1.0)
            } else {
                fallback.grid_opacity
            },
            grid_diagonal: clamp_or(self.grid_diagonal, &DIAGONAL_RANGE, fallback.grid_diagonal),
            grid_angle_degrees: clamp_or(
                self.grid_angle_degrees,
                &ANGLE_RANGE,
                fallback.grid_angle_degrees,
            ),
            grid_visible: self.grid_visible,
            theme: theme.as_str().to_string(),
            node_fill: color_or(self.node_fill, &palette.node_fill.hex()),
            node_stroke: color_or(self.node_stroke, &palette.node_stroke.hex()),
            node_text: color_or(self.node_text, &palette.node_text.hex()),
            edge_stroke: color_or(self.edge_stroke, &palette.edge_stroke.hex()),
            edge_text: color_or(self.edge_text, &palette.edge_text.hex()),
            accent: color_or(self.accent, &palette.accent.hex()),
        }
    }

    /// The theme of the plan as the renderer understands it.
    ///
    /// `sanitized()` already reduced the field to the name of a theme, so this
    /// is a mapping and never a validation (PRD §8.5.2 RF22).
    pub fn theme(&self) -> Theme {
        Theme::parse(&self.theme).unwrap_or(Theme::Dark)
    }

    /// The palette the plan paints with: the theme the human chose as the
    /// base, with every colour they changed on top (PRD §8.5.2, R1, R2).
    ///
    /// The opacity of the grid is the alpha of its colour, so the palette
    /// carries one colour per token and no opacity anywhere.
    pub fn palette(&self) -> Palette {
        let base = Palette::for_theme(self.theme());
        Palette {
            theme: base.theme,
            bg: parsed(&self.background, base.bg),
            grid: parsed(&self.grid_color, base.grid).with_opacity(self.grid_opacity),
            node_fill: parsed(&self.node_fill, base.node_fill),
            node_stroke: parsed(&self.node_stroke, base.node_stroke),
            node_text: parsed(&self.node_text, base.node_text),
            edge_stroke: parsed(&self.edge_stroke, base.edge_stroke),
            edge_text: parsed(&self.edge_text, base.edge_text),
            accent: parsed(&self.accent, base.accent),
        }
    }

    /// The settings with the colours of the theme they name (`f6-themes.md`,
    /// R3): choosing another theme makes it the base again, and the colours
    /// the human had picked for the previous one are replaced by its palette.
    ///
    /// The geometry of the grid and its opacity are not colours, so they stay
    /// exactly where the human left them.
    pub fn with_theme_palette(self) -> Settings {
        let palette = Palette::for_theme(self.theme());
        Settings {
            background: palette.bg.hex(),
            grid_color: palette.grid.hex(),
            node_fill: palette.node_fill.hex(),
            node_stroke: palette.node_stroke.hex(),
            node_text: palette.node_text.hex(),
            edge_stroke: palette.edge_stroke.hex(),
            edge_text: palette.edge_text.hex(),
            accent: palette.accent.hex(),
            ..self
        }
    }

    /// Writes the settings where the next start will read them.
    pub fn save(&self) -> io::Result<()> {
        let path = Self::path().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "the home directory is unknown: the settings have nowhere to live",
            )
        })?;
        self.save_to(path)
    }

    /// Writes the settings to a path, atomically.
    ///
    /// The file is written next to its destination and renamed over it: a
    /// window that dies in the middle of the write leaves the previous
    /// settings, never half a file.
    pub fn save_to(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, text)?;
        fs::rename(&temporary, path)?;
        Ok(())
    }
}

/// A `#rrggbb` colour, or the value of the theme.
fn color_or(value: String, fallback: &str) -> String {
    if is_color(&value) {
        value
    } else {
        fallback.to_string()
    }
}

/// A colour of the file, or the one of the theme when the text is not one.
fn parsed(value: &str, fallback: Color) -> Color {
    Color::from_hex(value).unwrap_or(fallback)
}

fn is_color(value: &str) -> bool {
    Color::from_hex(value).is_some()
}

fn clamp_or(value: f64, range: &std::ops::RangeInclusive<f64>, fallback: f64) -> f64 {
    if value.is_finite() {
        value.clamp(*range.start(), *range.end())
    } else {
        fallback
    }
}

#[cfg(test)]
mod tests {
    use super::{preset, presets, Settings, PRESET_NAMES};
    use gx_core::render::{Color, Palette, Theme};

    /// L7 (`f2-export.md`): el tema del plano es el que pinta el export. El
    /// campo ya está saneado, así que el mapeo es total: nada queda sin tema.
    #[test]
    fn the_settings_carry_the_theme_of_the_export() {
        assert_eq!(Settings::default().theme(), Theme::Dark);
        assert_eq!(preset("claro").expect("claro").theme(), Theme::Light);
        for theme in Theme::ALL {
            let settings = Settings {
                theme: theme.as_str().to_string(),
                ..Settings::default()
            };
            assert_eq!(settings.theme(), theme, "{theme:?} es un tema del panel");
        }
    }

    #[test]
    fn a_colour_is_six_hex_digits_after_the_hash() {
        assert!(super::is_color("#1b1b1b"));
        assert!(super::is_color("#FFFFFF"));
        assert!(!super::is_color("#1b1b"));
        assert!(!super::is_color("1b1b1b"));
        assert!(!super::is_color("#gggggg"));
    }

    #[test]
    fn the_settings_serialize_with_their_names() {
        let json = serde_json::to_value(Settings::default()).expect("serializes");
        assert_eq!(json["background"], "#1b1b1b");
        assert_eq!(json["grid_visible"], true);
        assert_eq!(json["theme"], "dark");
        // R2: todo color de la paleta viaja en `settings.json`, o el override
        // del humano no sobrevive al reinicio.
        for key in [
            "node_fill",
            "node_stroke",
            "node_text",
            "edge_stroke",
            "edge_text",
            "accent",
        ] {
            assert!(
                json[key].is_string(),
                "el color `{key}` tiene que estar en el archivo: {json}"
            );
        }
    }

    #[test]
    fn the_claro_preset_is_light_and_the_sin_grilla_preset_is_dark() {
        assert_eq!(preset("claro").expect("claro").theme, "light");
        assert!(!preset("sin grilla").expect("sin grilla").grid_visible);
        assert_eq!(preset("sin_grilla").expect("sin_grilla").theme, "dark");
    }

    /// R5, R6: los cuatro temas nuevos tienen preset, el preset pinta la
    /// paleta de su tema (un solo look, una sola fuente) y el panel los
    /// ofrece junto a los tres de F1.
    #[test]
    fn every_theme_has_a_preset_that_paints_its_palette() {
        for theme in Theme::ALL {
            let settings = presets::of_theme(theme);
            let palette = Palette::for_theme(theme);
            assert_eq!(settings.theme, theme.as_str());
            assert_eq!(settings.background, palette.bg.hex(), "{theme:?}");
            assert_eq!(settings.grid_color, palette.grid.hex(), "{theme:?}");
            assert_eq!(settings.node_fill, palette.node_fill.hex(), "{theme:?}");
            assert_eq!(settings.node_stroke, palette.node_stroke.hex(), "{theme:?}");
            assert_eq!(settings.node_text, palette.node_text.hex(), "{theme:?}");
            assert_eq!(settings.edge_stroke, palette.edge_stroke.hex(), "{theme:?}");
            assert_eq!(settings.edge_text, palette.edge_text.hex(), "{theme:?}");
            assert_eq!(settings.accent, palette.accent.hex(), "{theme:?}");
            assert!(settings.grid_visible, "{theme:?} muestra la grilla");
            assert_eq!(
                settings.clone().sanitized(),
                settings,
                "{theme:?} ya está sano"
            );
        }

        for name in PRESET_NAMES {
            let from_name = preset(name).unwrap_or_else(|| panic!("el preset {name}"));
            assert!(
                Theme::parse(&from_name.theme).is_some(),
                "el preset {name} nombra un tema"
            );
        }
        assert_eq!(preset(" brassa "), None);
    }

    /// R1, R2: la paleta del plano es el tema más los colores del humano, y
    /// la opacidad de la grilla se vuelve el alfa de su color.
    #[test]
    fn the_palette_is_the_theme_with_the_colours_of_the_human() {
        let settings = Settings {
            accent: "#ff0000".to_string(),
            node_text: "#00ff00".to_string(),
            grid_opacity: 0.5,
            ..presets::of_theme(Theme::Aurora)
        };
        let palette = settings.palette();
        assert_eq!(palette.theme, Theme::Aurora);
        assert_eq!(palette.accent, Color::rgb(0xff, 0x00, 0x00));
        assert_eq!(palette.node_text, Color::rgb(0x00, 0xff, 0x00));
        assert_eq!(
            palette.bg,
            Palette::aurora().bg,
            "el tema sigue siendo la base"
        );
        assert_eq!(palette.grid.a, 128, "la opacidad es el alfa de la grilla");
    }

    /// R4: un color que no se puede leer cae al valor del tema que el humano
    /// eligió, no al tema por defecto.
    #[test]
    fn a_broken_colour_falls_back_to_the_palette_of_the_theme() {
        let settings = Settings {
            theme: "arcilla".to_string(),
            background: "no-es-un-color".to_string(),
            node_fill: "#12".to_string(),
            accent: String::new(),
            ..Settings::default()
        }
        .sanitized();
        assert_eq!(settings.theme, "arcilla", "un typo no cambia el tema");
        assert_eq!(settings.background, Palette::arcilla().bg.hex());
        assert_eq!(settings.node_fill, Palette::arcilla().node_fill.hex());
        assert_eq!(settings.accent, Palette::arcilla().accent.hex());
        assert!(
            (settings.grid_opacity - Settings::default().grid_opacity).abs() < 1e-9,
            "la grilla sigue en su default"
        );
    }

    /// R3: elegir otro tema lo vuelve la base: los colores que el humano
    /// había elegido se reemplazan por la paleta del tema nuevo, y la
    /// geometría de la grilla (que no es un color) se queda como estaba.
    #[test]
    fn another_theme_resets_the_colours_but_not_the_grid() {
        let settings = Settings {
            theme: "brasa".to_string(),
            background: "#123456".to_string(),
            node_fill: "#123456".to_string(),
            accent: "#abcdef".to_string(),
            grid_visible: false,
            grid_diagonal: 120.0,
            grid_angle_degrees: 30.0,
            grid_opacity: 0.5,
            ..Settings::default()
        };
        let reset = settings.with_theme_palette();
        let palette = Palette::brasa();
        assert_eq!(reset.theme, "brasa");
        assert_eq!(reset.background, palette.bg.hex());
        assert_eq!(reset.node_fill, palette.node_fill.hex());
        assert_eq!(reset.accent, palette.accent.hex());
        assert_eq!(reset.grid_color, palette.grid.hex());
        assert!(!reset.grid_visible);
        assert!((reset.grid_diagonal - 120.0).abs() < 1e-9);
        assert!((reset.grid_angle_degrees - 30.0).abs() < 1e-9);
        assert!((reset.grid_opacity - 0.5).abs() < 1e-9);
    }

    /// R5: los nombres de los temas nuevos no son de marca y la UI los
    /// escribe en minúsculas, como `plano` y `claro`.
    #[test]
    fn the_new_theme_names_are_not_brand_names_and_are_lowercase() {
        for name in ["brasa", "arcilla", "aurora", "neblina", "pitaya"] {
            assert_eq!(name, name.to_lowercase());
            assert!(preset(name).is_some(), "el preset {name} existe");
            assert!(Settings::default().sanitized().theme != name);
        }
        for brand in ["opencode", "claude", "pi", "gentle"] {
            assert_eq!(preset(brand), None, "`{brand}` no es un nombre de tema");
            assert_eq!(Theme::parse(brand), None, "`{brand}` no es un tema");
        }
    }
}
