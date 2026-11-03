//! The settings of the plan: what the human chose and where it is kept
//! (PRD §7.1 RF25, §8.5.2 RF22, S19).
//!
//! The look of the plan belongs to the human. The agent never changes it, so
//! this module has no tool behind it: the window reads the file at start and
//! writes it when the panel changes something.
//!
//! A broken field falls back to its default instead of taking the window
//! down: the file is edited by hand, and a typo in one colour is not a
//! reason to lose the plan.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Background of the `plano` preset: the dark of the reference capture.
const PLANO_BACKGROUND: &str = "#1b1b1b";
/// Colour of the grid of the `plano` preset.
const PLANO_GRID: &str = "#ffffff";
/// Opacity of the grid of the `plano` preset (PRD §8.5.2: 14%).
const PLANO_GRID_OPACITY: f64 = 0.14;
/// Separations the panel lets the human choose, in plan units.
const DIAGONAL_RANGE: std::ops::RangeInclusive<f64> = 20.0..=200.0;
/// Angles the panel lets the human choose, in degrees.
const ANGLE_RANGE: std::ops::RangeInclusive<f64> = 0.0..=90.0;

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
    /// `dark` or `light` (PRD §8.5.2 RF22).
    pub theme: String,
}

impl Default for Settings {
    fn default() -> Self {
        presets::plano()
    }
}

/// The three presets of F1 (PRD §8.5.2).
pub mod presets {
    use super::{
        Settings, ANGLE_RANGE, DIAGONAL_RANGE, PLANO_BACKGROUND, PLANO_GRID, PLANO_GRID_OPACITY,
    };

    /// The dark plan of the reference: dark background and a white grid.
    pub fn plano() -> Settings {
        Settings {
            background: PLANO_BACKGROUND.to_string(),
            grid_color: PLANO_GRID.to_string(),
            grid_opacity: PLANO_GRID_OPACITY,
            grid_diagonal: 59.0,
            grid_angle_degrees: 45.0,
            grid_visible: true,
            theme: "dark".to_string(),
        }
    }

    /// The light theme: a real theme, not a white background. Nodes, labels
    /// and arrows keep their contrast, so the drawing is still readable.
    pub fn claro() -> Settings {
        Settings {
            background: "#f4f4f5".to_string(),
            grid_color: "#1b1b1b".to_string(),
            grid_opacity: 0.10,
            grid_diagonal: *DIAGONAL_RANGE.start() + 39.0,
            grid_angle_degrees: *ANGLE_RANGE.start() + 45.0,
            grid_visible: true,
            theme: "light".to_string(),
        }
    }

    /// The dark plan without the grid.
    pub fn sin_grilla() -> Settings {
        Settings {
            grid_visible: false,
            ..plano()
        }
    }
}

/// A preset by name, or `None` when the name is not one of the three.
pub fn preset(name: &str) -> Option<Settings> {
    match name.trim().to_lowercase().as_str() {
        "plano" => Some(presets::plano()),
        "claro" => Some(presets::claro()),
        "sin grilla" | "sin_grilla" | "singrilla" => Some(presets::sin_grilla()),
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

    /// Keeps the sane fields and replaces the rest with their defaults.
    fn sanitized(self) -> Settings {
        let fallback = Settings::default();
        Settings {
            background: color_or(self.background, &fallback.background),
            grid_color: color_or(self.grid_color, &fallback.grid_color),
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
            theme: if self.theme == "light" || self.theme == "dark" {
                self.theme
            } else {
                fallback.theme
            },
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

/// A `#rrggbb` colour, or the fallback.
fn color_or(value: String, fallback: &str) -> String {
    if is_color(&value) {
        value
    } else {
        fallback.to_string()
    }
}

fn is_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value[1..].chars().all(|c| c.is_ascii_hexdigit())
}

fn clamp_or(
    value: f64,
    range: &std::ops::RangeInclusive<f64>,
    fallback: f64,
) -> f64 {
    if value.is_finite() {
        value.clamp(*range.start(), *range.end())
    } else {
        fallback
    }
}

#[cfg(test)]
mod tests {
    use super::{preset, Settings};

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
    }

    #[test]
    fn the_claro_preset_is_light_and_the_sin_grilla_preset_is_dark() {
        assert_eq!(preset("claro").expect("claro").theme, "light");
        assert!(!preset("sin grilla").expect("sin grilla").grid_visible);
        assert_eq!(preset("sin_grilla").expect("sin_grilla").theme, "dark");
    }
}
