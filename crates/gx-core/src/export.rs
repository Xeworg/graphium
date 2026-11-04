//! The export contract and the sandbox of the exports (PRD §7 RF10/RF10-bis,
//! §11).
//!
//! Two things live here, both policy that the MCP layer must not invent:
//!
//! - the **limits** of an export (the size of an inline text, the deadline,
//!   the scale and the pixel ceiling of a PNG, the quota of the root);
//! - the **sandbox**: the only place a file may be written is the exports
//!   root, and the only way in is a *relative* path that stays inside it
//!   (PRD §7 RNF4: «la IA solo escribe dentro de `~/.graphium/exports/`»).
//!
//! The crate has no dependency on the MCP layer, the window or the renderer:
//! the rules are testable on their own.

use crate::render::Theme;

use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};

/// The maximum size of an inline export, in bytes (PRD §7 RF10-bis).
///
/// An inline export travels by MCP, so it goes through the agent's context:
/// past a megabyte it is not a text to paste in a README anymore.
pub const MAX_INLINE_BYTES: usize = 1024 * 1024;

/// The deadline of a whole export, in milliseconds (PRD §7 RF10-bis).
pub const EXPORT_TIMEOUT_MS: u64 = 5_000;

/// The quota of the exports root, in bytes (PRD §7 RF10-bis: 200 MB).
pub const EXPORT_ROOT_QUOTA_BYTES: u64 = 200 * 1024 * 1024;

/// The default scale of a PNG (PRD §7 RF10-bis).
pub const PNG_DEFAULT_SCALE: f64 = 1.0;

/// The ceiling of the scale of a PNG (PRD §7 RF10-bis).
pub const PNG_MAX_SCALE: f64 = 3.0;

/// The default pixel ceiling of a PNG: 16 Mpx (PRD §7 RF10-bis).
pub const PNG_DEFAULT_MAX_PX: u64 = 16_000_000;

/// The ceiling of the pixel ceiling of a PNG: 64 Mpx (PRD §7 RF10-bis).
pub const PNG_MAX_MAX_PX: u64 = 64_000_000;

/// The formats of an export (PRD §7 RF10, sin Mermaid).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// The SVG string, exactly as the renderer wrote it.
    Svg,
    /// The PNG rasterized from the SVG (PRD §8.5: `png` = `resvg`).
    Png,
}

impl ExportFormat {
    /// The name the agent writes.
    pub fn as_str(&self) -> &'static str {
        match self {
            ExportFormat::Svg => "svg",
            ExportFormat::Png => "png",
        }
    }

    /// Parses the name the agent writes, or `None` when it is not one.
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "svg" => Some(ExportFormat::Svg),
            "png" => Some(ExportFormat::Png),
            _ => None,
        }
    }

    /// The formats the tool accepts, for the error that says which are.
    pub fn accepted() -> &'static str {
        "svg, png"
    }
}

/// How a PNG is rasterized (PRD §7 RF10-bis).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RasterOptions {
    /// How much the drawing is scaled up (default 1.0, ceiling 3.0).
    pub scale: f64,
    /// The ceiling of pixels: `width × height × scale²` may not exceed it.
    pub max_px: u64,
}

impl Default for RasterOptions {
    fn default() -> Self {
        Self {
            scale: PNG_DEFAULT_SCALE,
            max_px: PNG_DEFAULT_MAX_PX,
        }
    }
}

/// A rasterized drawing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RasterImage {
    /// The bytes of the PNG.
    pub png: Vec<u8>,
    /// The width of the image, in pixels.
    pub width: u32,
    /// The height of the image, in pixels.
    pub height: u32,
}

/// Why a drawing could not be rasterized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RasterError {
    /// The drawing is bigger than the pixel ceiling.
    TooLarge {
        width: u32,
        height: u32,
        pixels: u64,
    },
    /// The SVG could not be parsed.
    Invalid(String),
    /// The process has no rasterizer installed.
    Unavailable,
}

impl RasterError {
    /// The stable code of the problem (PRD §11.1).
    pub fn code(&self) -> &'static str {
        match self {
            RasterError::TooLarge { .. } => "too_large",
            RasterError::Invalid(_) => "invalid_value",
            RasterError::Unavailable => "engine_unavailable",
        }
    }

    /// The message the agent reads.
    pub fn message(&self) -> String {
        match self {
            RasterError::TooLarge {
                width,
                height,
                pixels,
            } => format!("el PNG mide {width}×{height} px ({pixels} px), por encima del techo"),
            RasterError::Invalid(reason) => {
                format!("el SVG no se pudo rasterizar: {reason}")
            }
            RasterError::Unavailable => {
                "este proceso no tiene rasterizador: el PNG solo existe en la app".to_string()
            }
        }
    }
}

/// The seam that rasterizes an SVG into a PNG.
///
/// `gx-core` does not depend on `resvg`: the concrete rasterizer lives in
/// `gx-render-svg` behind its `png` feature and the app installs it, the same
/// way the layout engine and the renderer are installed (PRD §9.1, decisión L7).
pub trait Rasterizer: Send + Sync {
    /// Rasterizes an SVG into PNG bytes.
    fn rasterize(&self, svg: &str, options: &RasterOptions) -> Result<RasterImage, RasterError>;
}

/// Where the rasterizer of the process lives.
fn rasterizer_slot() -> &'static RwLock<Option<Arc<dyn Rasterizer>>> {
    static SLOT: OnceLock<RwLock<Option<Arc<dyn Rasterizer>>>> = OnceLock::new();
    SLOT.get_or_init(|| RwLock::new(None))
}

/// Installs the rasterizer of the process.
pub fn install_rasterizer(rasterizer: Arc<dyn Rasterizer>) {
    *rasterizer_slot()
        .write()
        .expect("the rasterizer slot is not poisoned") = Some(rasterizer);
}

/// The rasterizer of the process, when one is installed.
pub fn rasterizer() -> Option<Arc<dyn Rasterizer>> {
    rasterizer_slot()
        .read()
        .expect("the rasterizer slot is not poisoned")
        .clone()
}

/// Where the theme of the plan lives.
///
/// The theme that the human chose lives in `~/.graphium/settings.json` and
/// belongs to the window (`gx-app`), but the export is drawn by `gx-mcp`,
/// which does not depend on it (PRD §9.1). The window installs the theme it
/// is painting with, the same way it installs the engines, and the export
/// uses it: **el SVG exportado se parece a lo que el humano ve**.
fn theme_slot() -> &'static RwLock<Theme> {
    static SLOT: OnceLock<RwLock<Theme>> = OnceLock::new();
    SLOT.get_or_init(|| RwLock::new(Theme::Dark))
}

/// Installs the theme of the plan.
pub fn install_theme(theme: Theme) {
    *theme_slot()
        .write()
        .expect("the theme slot is not poisoned") = theme;
}

/// The theme of the plan, dark when nothing installed one.
pub fn plan_theme() -> Theme {
    *theme_slot().read().expect("the theme slot is not poisoned")
}

/// Why a relative path does not name a file inside the exports root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathError {
    /// The path is empty: there is no file to write.
    Empty,
    /// The path is absolute: the exports root is the only root.
    Absolute,
    /// The path climbs out of the exports root with `..`.
    Outside,
    /// A component of the path is a symbolic link: the link could point
    /// anywhere, and the sandbox is about *where* bytes land.
    Symlink,
}

impl PathError {
    /// The stable code of the problem (PRD §11.1).
    pub fn code(&self) -> &'static str {
        match self {
            PathError::Empty | PathError::Absolute | PathError::Outside | PathError::Symlink => {
                "export_path_outside_root"
            }
        }
    }

    /// The message the agent reads.
    pub fn message(&self) -> &'static str {
        match self {
            PathError::Empty => "el path del export está vacío",
            PathError::Absolute => {
                "el path del export tiene que ser relativo a la raíz de exportes"
            }
            PathError::Outside => {
                "el path del export sale de la raíz de exportes con `..`"
            }
            PathError::Symlink => {
                "el path del export pasa por un enlace simbólico: el sandbox es sobre dónde caen los bytes"
            }
        }
    }

    /// What the agent can do about it.
    pub fn hint(&self) -> &'static str {
        match self {
            PathError::Empty => "mandá un nombre de archivo, por ejemplo `diagrama.svg`",
            PathError::Absolute => "mandá un path relativo, por ejemplo `docs/diagrama.svg`",
            PathError::Outside => "quedate dentro de la raíz de exportes: no uses `..`",
            PathError::Symlink => "mandá un path que no pase por un enlace simbólico",
        }
    }
}

/// The exports root of the process (PRD §7 RNF4).
///
/// The default is `~/.graphium/exports/`. The environment variable
/// `GRAPHIUM_EXPORTS_ROOT` moves it, which is what the tests and a headless
/// run use; the agent never chooses it.
pub fn exports_root() -> Option<PathBuf> {
    if let Some(root) = std::env::var_os("GRAPHIUM_EXPORTS_ROOT") {
        if !root.is_empty() {
            return Some(PathBuf::from(root));
        }
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".graphium").join("exports"))
}

/// Resolves a relative path inside the exports root.
///
/// The rules, in order:
///
/// 1. the path is not empty and it is **relative**;
/// 2. no component is `..` (the path never climbs out);
/// 3. no component of the *existing* part of the path is a symbolic link
///    (a link could point anywhere);
/// 4. the result is inside the root.
pub fn resolve_in_root(root: &Path, relative: &str) -> Result<PathBuf, PathError> {
    if relative.trim().is_empty() {
        return Err(PathError::Empty);
    }

    let candidate = Path::new(relative);
    if candidate.is_absolute() {
        return Err(PathError::Absolute);
    }

    for component in candidate.components() {
        match component {
            Component::ParentDir => return Err(PathError::Outside),
            Component::Prefix(_) | Component::RootDir => return Err(PathError::Absolute),
            Component::CurDir | Component::Normal(_) => {}
        }
    }

    let target = root.join(candidate);

    // Every directory of the path that already exists has to be a real
    // directory and not a link: `exports/link/archivo.svg` with `link` a
    // symlink to `/etc` would write outside the sandbox.
    let mut walked = root.to_path_buf();
    for component in candidate.components() {
        let Component::Normal(name) = component else {
            continue;
        };
        walked.push(name);
        match std::fs::symlink_metadata(&walked) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(PathError::Symlink);
            }
            Ok(metadata) if !metadata.is_dir() && walked != target => {
                // A file in the middle of the path: the write cannot happen.
                return Err(PathError::Outside);
            }
            Ok(_) => {}
            Err(_) => {
                // The rest of the path does not exist yet: it will be created
                // inside the root, which is the sandbox.
                break;
            }
        }
    }

    // Last line of defense: the joined path is under the root.
    if !target.starts_with(root) {
        return Err(PathError::Outside);
    }

    Ok(target)
}

/// The bytes already used by the exports root (PRD §7 RF10-bis).
///
/// A missing root counts as zero: the quota is about what is there. Links are
/// not bytes of the root: following one would count what lives outside the
/// sandbox, and a link to itself never stops walking.
pub fn used_bytes(root: &Path) -> u64 {
    let mut total = 0;
    let Ok(entries) = std::fs::read_dir(root) else {
        return 0;
    };
    for entry in entries.flatten() {
        let Ok(metadata) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            total += used_bytes(&entry.path());
        } else {
            total += metadata.len();
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::{
        resolve_in_root, used_bytes, PathError, EXPORT_ROOT_QUOTA_BYTES, MAX_INLINE_BYTES,
    };
    use std::path::{Path, PathBuf};

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("graphium-export-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("the temporary root is created");
        root
    }

    #[test]
    fn a_relative_path_resolves_inside_the_root() {
        let root = temp_root("relative");
        let target = resolve_in_root(&root, "docs/diagrama.svg").expect("inside");
        assert_eq!(target, root.join("docs/diagrama.svg"));
        assert!(target.starts_with(&root));
    }

    #[test]
    fn an_absolute_path_is_refused() {
        let root = temp_root("absolute");
        let error = resolve_in_root(&root, "/etc/passwd").expect_err("absolute");
        assert_eq!(error, PathError::Absolute);
        assert_eq!(error.code(), "export_path_outside_root");
        assert!(error.hint().contains("relativo"));
    }

    #[test]
    fn a_path_that_climbs_out_is_refused() {
        let root = temp_root("climb");
        let error = resolve_in_root(&root, "../fuera.svg").expect_err("climbs");
        assert_eq!(error, PathError::Outside);

        let hidden = resolve_in_root(&root, "docs/../../fuera.svg").expect_err("climbs");
        assert_eq!(hidden, PathError::Outside);
    }

    #[test]
    fn an_empty_path_is_refused() {
        let root = temp_root("empty");
        assert_eq!(
            resolve_in_root(&root, "   ").expect_err("empty"),
            PathError::Empty
        );
    }

    /// Un link simbólico en medio del path puede apuntar afuera: el sandbox es
    /// sobre dónde caen los bytes, no sobre cómo se llama el camino.
    #[test]
    fn a_symlink_in_the_path_is_refused() {
        let root = temp_root("symlink");
        let outside = temp_root("symlink-target");
        std::os::unix::fs::symlink(&outside, root.join("link")).expect("the link is created");

        let error = resolve_in_root(&root, "link/diagrama.svg").expect_err("a link");
        assert_eq!(error, PathError::Symlink);
        assert_eq!(error.code(), "export_path_outside_root");
    }

    #[test]
    fn the_used_bytes_of_a_root_add_up() {
        let root = temp_root("quota");
        std::fs::write(root.join("a.svg"), vec![0_u8; 100]).expect("written");
        std::fs::create_dir_all(root.join("docs")).expect("the directory is created");
        std::fs::write(root.join("docs/b.svg"), vec![0_u8; 250]).expect("written");

        assert_eq!(used_bytes(&root), 350);
        assert_eq!(used_bytes(Path::new("/tmp/graphium-no-existe")), 0);
    }

    /// La cuota es sobre los bytes que están en la raíz: un link no suma los
    /// bytes de afuera, y un link a sí mismo no cuelga el proceso.
    #[test]
    fn a_symlink_does_not_add_bytes_to_the_quota() {
        let root = temp_root("quota-link");
        let outside = temp_root("quota-link-target");
        std::fs::write(outside.join("grande.svg"), vec![0_u8; 5_000]).expect("written");
        std::fs::write(root.join("a.svg"), vec![0_u8; 10]).expect("written");
        std::os::unix::fs::symlink(&outside, root.join("afuera")).expect("the link is created");
        std::os::unix::fs::symlink(&root, root.join("ciclo")).expect("the link is created");

        assert_eq!(used_bytes(&root), 10);
    }

    #[test]
    fn the_limits_are_the_ones_of_the_prd() {
        assert_eq!(MAX_INLINE_BYTES, 1_048_576);
        assert_eq!(EXPORT_ROOT_QUOTA_BYTES, 209_715_200);
    }
}
