//! The `export_diagram` tool (PRD §7 RF10/RF10-bis, §11; feature doc
//! `f2-export.md`, T5).
//!
//! The tool decides **which** diagram is exported (S4) and **how** the answer
//! travels: the SVG inline or a file. It does not decide where the bytes may
//! land — the sandbox, the limits and the rasterizer seam live in
//! [`gx_core::export`] (decision L4) — and it never touches the plan: an
//! export reads the stored document, paints it and answers.
//!
//! The whole export (render, rasterize, write) runs on a worker thread under
//! the deadline of RF10-bis: a diagram that does not come out in five seconds
//! is an answer (`export_timeout`), not a hang.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use gx_core::error::ErrorCode;
use gx_core::export::{
    exports_root, plan_palette, rasterizer, resolve_in_root, used_bytes, ExportFormat, PathError,
    RasterError, RasterOptions, EXPORT_ROOT_QUOTA_BYTES, EXPORT_TIMEOUT_MS, MAX_INLINE_BYTES,
    PNG_DEFAULT_MAX_PX, PNG_DEFAULT_SCALE, PNG_MAX_MAX_PX, PNG_MAX_SCALE,
};
use gx_core::render::RenderOpts;
use serde::Serialize;

use crate::store::{diagram_store, DiagramSummary};
use crate::tools::ErrorPayload;

/// A request to export one diagram (PRD §11).
#[derive(Debug, Clone, PartialEq)]
pub struct ExportRequest {
    /// The MCP session that asks (PRD §11.1).
    pub session_id: String,
    /// The diagram to export. Without it the tool takes the only open one of
    /// the session (PRD §11.1, S4).
    pub diagram_id: Option<String>,
    /// `svg` or `png` (PRD §7 RF10; Mermaid is out of scope, decision L2).
    pub format: String,
    /// Returns the SVG as text by MCP instead of writing a file.
    pub inline: bool,
    /// The path **relative to the exports root** where the file is written.
    pub path: Option<String>,
    /// Whether an existing file may be replaced (PRD §7 RF10).
    pub overwrite: bool,
    /// Scale of the PNG: default 1.0, ceiling 3.0 (PRD §7 RF10-bis).
    pub scale: Option<f64>,
    /// Pixel ceiling of the PNG: default 16 Mpx, ceiling 64 Mpx.
    pub max_px: Option<u64>,
}

/// The response of `export_diagram` (PRD §11: `inline` → `{ ok, content }`,
/// archivo → `{ ok, path }`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExportResponse {
    pub ok: bool,
    /// The SVG, when the export travels inline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// The path the agent sent, when a file was written.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The format that was exported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// The diagram that was exported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagram_id: Option<String>,
    /// How many bytes the file has, when one was written.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<usize>,
    /// How big the content was, when it could not travel inline
    /// (PRD §7 RF10-bis: `too_large` reports `size_bytes`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<usize>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<ErrorPayload>,
}

/// Where the bytes of an export go.
#[derive(Debug, Clone, PartialEq)]
enum Destination {
    /// The SVG travels back by MCP, with zero disk access.
    Inline,
    /// The bytes land inside the exports root, and nowhere else.
    File {
        root: PathBuf,
        target: PathBuf,
        /// The path exactly as the agent sent it, for the response.
        path: String,
    },
}

/// Everything a decided export needs to run.
#[derive(Debug, Clone, PartialEq)]
struct ExportPlan {
    diagram_id: String,
    format: ExportFormat,
    document: String,
    destination: Destination,
    raster: RasterOptions,
}

/// Exports one diagram (PRD §11, S1-S5).
///
/// The request is validated and the diagram resolved before anything runs;
/// the render, the rasterization and the write happen on a worker thread
/// under the deadline of RF10-bis. A rejected export leaves the exports root
/// exactly as it was: the bytes are only written once everything succeeded.
pub fn export_diagram(request: &ExportRequest) -> ExportResponse {
    crate::bootstrap();

    let plan = match plan_export(request) {
        Ok(plan) => plan,
        Err(errors) => {
            return refused(
                ExportFormat::parse(&request.format),
                request.diagram_id.as_deref(),
                errors,
            )
        }
    };

    let (sender, receiver) = mpsc::channel();
    let worker_plan = plan.clone();
    let worker = std::thread::Builder::new()
        .name("gx-mcp-export".to_string())
        .spawn(move || {
            // A closed channel means the caller already answered with the
            // deadline: what a slow export produces past it is discarded.
            let _ = sender.send(write_export(&worker_plan));
        });

    match worker {
        // Without a thread the deadline cannot be enforced, so the export runs
        // here instead of failing a valid request.
        Err(_) => write_export(&plan),
        Ok(_) => match receiver.recv_timeout(Duration::from_millis(EXPORT_TIMEOUT_MS)) {
            Ok(response) => response,
            Err(RecvTimeoutError::Timeout) => failed(
                &plan,
                ErrorPayload {
                    path: "diagram".to_string(),
                    code: ErrorCode::ExportTimeout.as_str().to_string(),
                    message: format!("el export no terminó en {EXPORT_TIMEOUT_MS} ms"),
                    hint: Some(
                        "volvé a intentar, o achicá el diagrama con opts.extract en render_diagram"
                            .to_string(),
                    ),
                },
            ),
            Err(RecvTimeoutError::Disconnected) => failed(
                &plan,
                ErrorPayload {
                    path: "diagram".to_string(),
                    code: ErrorCode::InvalidValue.as_str().to_string(),
                    message: "el export terminó sin resultado".to_string(),
                    hint: Some("volvé a intentar".to_string()),
                },
            ),
        },
    }
}

/// Validates the request and resolves what the export will do.
///
/// Nothing is written here and nothing is rendered: the plan is the cheap
/// half of the export, so a bad request is answered before the deadline.
fn plan_export(request: &ExportRequest) -> Result<ExportPlan, Vec<ErrorPayload>> {
    let Some(format) = ExportFormat::parse(&request.format) else {
        return Err(vec![ErrorPayload {
            path: "format".to_string(),
            code: ErrorCode::InvalidValue.as_str().to_string(),
            message: format!("formato de export desconocido: `{}`", request.format),
            hint: Some(ExportFormat::accepted().to_string()),
        }]);
    };

    if request.inline && request.path.is_some() {
        return Err(vec![ErrorPayload {
            path: "opts".to_string(),
            code: ErrorCode::InvalidValue.as_str().to_string(),
            message: "`inline` y `path` son excluyentes: el export viaja por MCP o va a un archivo"
                .to_string(),
            hint: Some(
                "mandá `inline: true` para pegar el SVG, o `path` para escribir a archivo; no los dos"
                    .to_string(),
            ),
        }]);
    }

    // PRD §11.1: un PNG no tiene forma de viajar como texto.
    if format == ExportFormat::Png && request.path.is_none() {
        return Err(vec![ErrorPayload {
            path: "path".to_string(),
            code: ErrorCode::InvalidValue.as_str().to_string(),
            message: "el PNG no viaja inline: son bytes, no texto".to_string(),
            hint: Some("el PNG se escribe a archivo: mandá path".to_string()),
        }]);
    }

    let raster = plan_raster(request)?;
    let (diagram_id, document) =
        resolve_diagram(&request.session_id, request.diagram_id.as_deref())
            .map_err(|error| vec![error])?;

    // PRD §11.1: un PNG no tiene forma de viajar como texto.
    let destination = match request.path.as_deref() {
        None => Destination::Inline,
        Some(path) => {
            let Some(root) = exports_root() else {
                return Err(vec![ErrorPayload {
                    path: "path".to_string(),
                    code: ErrorCode::ExportPathOutsideRoot.as_str().to_string(),
                    message: "el proceso no tiene raíz de exportes configurada".to_string(),
                    hint: Some(
                        "configurá `GRAPHIUM_EXPORTS_ROOT` o `HOME`: el export escribe en `~/.graphium/exports/`"
                            .to_string(),
                    ),
                }]);
            };
            let target = resolve_in_root(&root, path).map_err(|error| vec![path_error(&error)])?;
            if target.exists() && !request.overwrite {
                return Err(vec![ErrorPayload {
                    path: "path".to_string(),
                    code: ErrorCode::ExportExists.as_str().to_string(),
                    message: format!("ya existe `{path}` en la raíz de exportes"),
                    hint: Some(
                        "mandá `overwrite: true` para reemplazarlo, o elegí otro nombre"
                            .to_string(),
                    ),
                }]);
            }
            Destination::File {
                root,
                target,
                path: path.to_string(),
            }
        }
    };

    Ok(ExportPlan {
        diagram_id,
        format,
        document,
        destination,
        raster,
    })
}

/// Reads `opts.scale` and `opts.max_px` into [`RasterOptions`] (S2).
///
/// The ceilings are checked here as well as inside the rasterizer: the tool
/// never sends a value it already knows is invalid, and the agent gets the
/// error with the path of the option that is wrong.
fn plan_raster(request: &ExportRequest) -> Result<RasterOptions, Vec<ErrorPayload>> {
    let scale = request.scale.unwrap_or(PNG_DEFAULT_SCALE);
    if !scale.is_finite() || scale <= 0.0 || scale > PNG_MAX_SCALE {
        return Err(vec![ErrorPayload {
            path: "opts.scale".to_string(),
            code: ErrorCode::InvalidValue.as_str().to_string(),
            message: format!(
                "`opts.scale` tiene que ser mayor que 0 y no pasar {PNG_MAX_SCALE}, no `{scale}`"
            ),
            hint: Some("mandá un `scale` entre 0 y 3; el default es 1".to_string()),
        }]);
    }

    let max_px = request.max_px.unwrap_or(PNG_DEFAULT_MAX_PX);
    if max_px == 0 || max_px > PNG_MAX_MAX_PX {
        return Err(vec![ErrorPayload {
            path: "opts.max_px".to_string(),
            code: ErrorCode::InvalidValue.as_str().to_string(),
            message: format!(
                "`opts.max_px` tiene que estar entre 1 y {PNG_MAX_MAX_PX}, no `{max_px}`"
            ),
            hint: Some("mandá un `max_px` hasta 64 Mpx; el default es 16 Mpx".to_string()),
        }]);
    }

    Ok(RasterOptions { scale, max_px })
}

/// Resolves the diagram to export (PRD §11.1, S4).
///
/// Without `diagram_id`: exactly one open diagram of the session is the one;
/// none is `unknown_diagram`; two or more is `ambiguous_diagram` with the
/// candidates. A diagram of another session is `not_owner`.
fn resolve_diagram(
    session_id: &str,
    diagram_id: Option<&str>,
) -> Result<(String, String), ErrorPayload> {
    let store = diagram_store();

    let Some(diagram_id) = diagram_id else {
        let open = store.list_for_session(session_id);
        return match open.as_slice() {
            [only] => match store.record(&only.diagram_id) {
                Some(record) => Ok((only.diagram_id.clone(), record.document)),
                None => Err(contract_error(crate::contract::unknown_diagram(
                    &only.diagram_id,
                    &store.known_ids(),
                ))),
            },
            [] => Err(no_open_diagram(&store.known_ids())),
            many => Err(ambiguous_diagram(many)),
        };
    };

    let Some(record) = store.record(diagram_id) else {
        return Err(contract_error(crate::contract::unknown_diagram(
            diagram_id,
            &store.known_ids(),
        )));
    };
    if record.summary.session_id != session_id {
        return Err(contract_error(crate::contract::not_owner(diagram_id)));
    }
    Ok((diagram_id.to_string(), record.document))
}

/// The lifecycle errors are built by `contract`, which owns the wording of
/// `unknown_diagram` and `not_owner` (with `known_ids[]`): the export answers
/// with the payload of `tools`, so the two copies of the type are joined in
/// one place ([`crate::tools::contract_payload`]) instead of duplicating the
/// messages.
fn contract_error(payload: crate::contract::ErrorPayload) -> ErrorPayload {
    let path = payload.path.clone();
    crate::tools::contract_payload(payload, &path)
}

/// The error of a session without open diagrams (PRD §11.1, S4).
fn no_open_diagram(known_ids: &[String]) -> ErrorPayload {
    ErrorPayload {
        path: "diagram_id".to_string(),
        code: ErrorCode::UnknownDiagram.as_str().to_string(),
        message: format!("esta sesión no tiene ningún diagrama abierto; conocidos: {known_ids:?}"),
        hint: Some("abrí un diagrama con render_diagram y volvé a intentar".to_string()),
    }
}

/// The error of a session with two open diagrams and no `diagram_id`
/// (PRD §11.1, S4: `ambiguous_diagram` with the candidates).
fn ambiguous_diagram(candidates: &[DiagramSummary]) -> ErrorPayload {
    let ids: Vec<&str> = candidates
        .iter()
        .map(|summary| summary.diagram_id.as_str())
        .collect();
    ErrorPayload {
        path: "diagram_id".to_string(),
        code: ErrorCode::AmbiguousDiagram.as_str().to_string(),
        message: format!(
            "hay {} diagramas abiertos en la sesión y falta `diagram_id`; candidatos: {ids:?}",
            ids.len()
        ),
        hint: Some(format!(
            "mandá `diagram_id` con uno de los candidatos: {ids:?}"
        )),
    }
}

/// The error of a path that does not name a file inside the root
/// (PRD §7 RF10, RNF4).
fn path_error(error: &PathError) -> ErrorPayload {
    ErrorPayload {
        path: "path".to_string(),
        code: error.code().to_string(),
        message: error.message().to_string(),
        hint: Some(error.hint().to_string()),
    }
}

/// The options of an export drawing.
///
/// PRD §8.5.2/S7: the exported SVG is a closed document, so it travels with
/// the canvas painted in the palette of the plan. The palette —the theme the
/// human chose plus the colours they changed— travels in `RenderOpts`, so the
/// file looks like the window (`f6-themes.md`, R7).
fn export_opts() -> RenderOpts {
    let palette = plan_palette();
    RenderOpts {
        theme: palette.theme,
        palette: Some(palette),
        export: true,
        ..RenderOpts::default()
    }
}

/// Runs the export: render, rasterize and write.
fn write_export(plan: &ExportPlan) -> ExportResponse {
    // It goes through the only engine door, and only reads the store.
    let opts = export_opts();
    let outcome = match gx_core::engine::render_json(&plan.document, &opts) {
        Ok(outcome) => outcome,
        Err(errors) => {
            return refused(
                Some(plan.format),
                Some(&plan.diagram_id),
                errors.iter().map(ErrorPayload::from).collect(),
            )
        }
    };

    match &plan.destination {
        Destination::Inline => export_inline(plan, &outcome.svg),
        Destination::File { root, target, path } => {
            export_file(plan, root, target, path, &outcome.svg)
        }
    }
}

/// Answers with the SVG as text (PRD §11.1, S2: at most 1 MB).
fn export_inline(plan: &ExportPlan, svg: &str) -> ExportResponse {
    let size = svg.len();
    if size > MAX_INLINE_BYTES {
        let mut response = failed(
            plan,
            ErrorPayload {
                path: "content".to_string(),
                code: ErrorCode::TooLarge.as_str().to_string(),
                message: format!(
                    "el SVG del export mide {size} bytes y el tope inline es {MAX_INLINE_BYTES}"
                ),
                hint: Some("usá path para escribir a archivo o opts.extract".to_string()),
            },
        );
        response.size_bytes = Some(size);
        return response;
    }

    ExportResponse {
        ok: true,
        content: Some(svg.to_string()),
        path: None,
        format: Some(plan.format.as_str().to_string()),
        diagram_id: Some(plan.diagram_id.clone()),
        bytes: None,
        size_bytes: None,
        errors: Vec::new(),
    }
}

/// Writes the bytes inside the exports root (PRD §7 RF10, RNF4).
///
/// The order matters: the PNG is rasterized, the quota is checked and only
/// then the file is written. Every rejection leaves the root untouched.
fn export_file(
    plan: &ExportPlan,
    root: &Path,
    target: &Path,
    path: &str,
    svg: &str,
) -> ExportResponse {
    let bytes: Vec<u8> = match plan.format {
        ExportFormat::Svg => svg.as_bytes().to_vec(),
        ExportFormat::Png => match rasterize(plan, svg) {
            Ok(png) => png,
            Err(payload) => return failed(plan, payload),
        },
    };

    // PRD §7 RF10-bis: the root has a quota of 200 MB. The check is on what is
    // already there plus what this export adds, and it happens before writing.
    let used = used_bytes(root);
    let added = bytes.len() as u64;
    if used.saturating_add(added) > EXPORT_ROOT_QUOTA_BYTES {
        return failed(
            plan,
            ErrorPayload {
                path: "path".to_string(),
                code: ErrorCode::DiskQuotaExceeded.as_str().to_string(),
                message: format!(
                    "la raíz de exportes usa {used} bytes y el export agrega {added}: el tope es {EXPORT_ROOT_QUOTA_BYTES}"
                ),
                hint: Some(
                    "liberá espacio en la raíz de exportes y volvé a intentar".to_string(),
                ),
            },
        );
    }

    if let Some(parent) = target.parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            return write_failed(plan, path, &error);
        }
    }
    if let Err(error) = std::fs::write(target, &bytes) {
        return write_failed(plan, path, &error);
    }

    ExportResponse {
        ok: true,
        content: None,
        path: Some(path.to_string()),
        format: Some(plan.format.as_str().to_string()),
        diagram_id: Some(plan.diagram_id.clone()),
        bytes: Some(bytes.len()),
        size_bytes: None,
        errors: Vec::new(),
    }
}

/// Rasterizes the exported SVG into PNG bytes (PRD §7 RF10, §8.5).
///
/// The rasterizer is the seam that `gx-app` installs: without it the answer is
/// `engine_unavailable` and says how to get one.
fn rasterize(plan: &ExportPlan, svg: &str) -> Result<Vec<u8>, ErrorPayload> {
    let Some(rasterizer) = rasterizer() else {
        return Err(ErrorPayload {
            path: "format".to_string(),
            code: ErrorCode::EngineUnavailable.as_str().to_string(),
            message: "este proceso no tiene rasterizador de PNG".to_string(),
            hint: Some(
                "el PNG lo rasteriza la app: abrí Graphium con la ventana y volvé a intentar"
                    .to_string(),
            ),
        });
    };

    match rasterizer.rasterize(svg, &plan.raster) {
        Ok(image) => Ok(image.png),
        Err(error) => Err(match &error {
            RasterError::TooLarge { .. } => ErrorPayload {
                path: "opts.max_px".to_string(),
                code: error.code().to_string(),
                message: error.message(),
                hint: Some("bajá opts.scale o subí opts.max_px".to_string()),
            },
            RasterError::Unavailable => ErrorPayload {
                path: "format".to_string(),
                code: error.code().to_string(),
                message: error.message(),
                hint: Some(
                    "el PNG lo rasteriza la app: abrí Graphium con la ventana y volvé a intentar"
                        .to_string(),
                ),
            },
            RasterError::Invalid(_) => ErrorPayload {
                path: "content".to_string(),
                code: error.code().to_string(),
                message: error.message(),
                hint: Some(
                    "el SVG se generó pero no se pudo rasterizar: revisá el diagrama y volvé a intentar"
                        .to_string(),
                ),
            },
        }),
    }
}

/// The answer to a file that could not be written.
fn write_failed(plan: &ExportPlan, path: &str, error: &std::io::Error) -> ExportResponse {
    failed(
        plan,
        ErrorPayload {
            path: "path".to_string(),
            code: ErrorCode::InvalidValue.as_str().to_string(),
            message: format!("no se pudo escribir `{path}`: {error}"),
            hint: Some("elegí otro nombre de archivo dentro de la raíz de exportes".to_string()),
        },
    )
}

/// A response that says no, keeping the context of the export.
fn failed(plan: &ExportPlan, error: ErrorPayload) -> ExportResponse {
    refused(Some(plan.format), Some(&plan.diagram_id), vec![error])
}

/// A refused export, with whatever context is already known.
fn refused(
    format: Option<ExportFormat>,
    diagram_id: Option<&str>,
    errors: Vec<ErrorPayload>,
) -> ExportResponse {
    ExportResponse {
        ok: false,
        content: None,
        path: None,
        format: format.map(|format| format.as_str().to_string()),
        diagram_id: diagram_id.map(str::to_string),
        bytes: None,
        size_bytes: None,
        errors,
    }
}

#[cfg(test)]
mod tests {
    use super::{export_diagram, ExportRequest};
    use crate::store::diagram_store;
    use crate::test_support::{
        install_fake_rasterizer, last_raster_options, lock_renders_blocking, FAKE_PNG,
    };
    use crate::tools::{render_diagram, RenderRequest};
    use gx_core::export::{
        EXPORT_ROOT_QUOTA_BYTES, MAX_INLINE_BYTES, PNG_DEFAULT_MAX_PX, PNG_MAX_MAX_PX,
        PNG_MAX_SCALE,
    };
    use gx_core::render::RenderOpts;
    use std::path::{Path, PathBuf};

    const SESSION: &str = "session-export";

    /// The exports root of one test: a fresh directory under the system temp
    /// directory, pointed at by `GRAPHIUM_EXPORTS_ROOT`, removed on drop.
    ///
    /// The tests share one process, so every test holds the render lock while
    /// it owns the variable.
    struct ExportRoot {
        path: PathBuf,
    }

    impl ExportRoot {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("graphium-mcp-export-{name}"));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("the temporary exports root is created");
            std::env::set_var("GRAPHIUM_EXPORTS_ROOT", &path);
            Self { path }
        }

        fn join(&self, relative: &str) -> PathBuf {
            self.path.join(relative)
        }

        /// How many files live under the root.
        fn files(&self) -> usize {
            fn walk(path: &Path) -> usize {
                let Ok(entries) = std::fs::read_dir(path) else {
                    return 0;
                };
                entries
                    .flatten()
                    .map(|entry| {
                        let path = entry.path();
                        if path.is_dir() {
                            walk(&path)
                        } else {
                            1
                        }
                    })
                    .sum()
            }
            walk(&self.path)
        }
    }

    impl Drop for ExportRoot {
        fn drop(&mut self) {
            std::env::remove_var("GRAPHIUM_EXPORTS_ROOT");
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    /// Renders the minimal example into the store and returns its id.
    fn render(session_id: &str) -> String {
        let response = render_diagram(&RenderRequest {
            document: crate::test_support::MINIMAL.to_string(),
            session_id: session_id.to_string(),
        });
        response.diagram_id.expect("the render stores the diagram")
    }

    /// An `svg` request with every option at its default.
    fn svg_request() -> ExportRequest {
        ExportRequest {
            session_id: SESSION.to_string(),
            diagram_id: None,
            format: "svg".to_string(),
            inline: false,
            path: None,
            overwrite: false,
            scale: None,
            max_px: None,
        }
    }

    /// S1, S4, S5: el camino primario del H4 es el SVG inline, sin tocar el
    /// disco y sin tocar el plano (ni un insert, ni una revisión nueva).
    #[test]
    fn the_svg_of_the_only_open_diagram_comes_inline_and_the_plan_is_untouched() {
        let _guard = lock_renders_blocking();
        let root = ExportRoot::new("inline");
        let diagram_id = render(SESSION);
        let before = diagram_store()
            .record(&diagram_id)
            .expect("the diagram is on the plan");

        let mut request = svg_request();
        request.inline = true;
        let response = export_diagram(&request);

        assert!(response.ok, "{response:?}");
        let content = response.content.as_deref().expect("the svg travels inline");
        assert!(content.starts_with("<svg"), "{content}");
        assert!(
            content.contains("class=\"bg\""),
            "el export pinta el lienzo: {content}"
        );
        assert_eq!(response.path, None, "inline no escribe ningún archivo");
        assert_eq!(response.format.as_deref(), Some("svg"));
        assert_eq!(response.diagram_id.as_deref(), Some(diagram_id.as_str()));
        assert_eq!(response.bytes, None);
        assert_eq!(response.errors, Vec::new());
        assert_eq!(root.files(), 0, "inline tiene cero acceso a disco");

        // Sin `inline` y sin `path`, el `svg` también viaja inline.
        let response = export_diagram(&svg_request());
        assert!(response.ok, "{response:?}");
        assert!(response
            .content
            .as_deref()
            .is_some_and(|content| content.starts_with("<svg")));

        let after = diagram_store()
            .record(&diagram_id)
            .expect("the diagram is still on the plan");
        assert_eq!(diagram_store().open_count(), 1, "el export no inserta nada");
        assert_eq!(
            after.summary.revision, before.summary.revision,
            "el export no bumpea la revisión"
        );
        assert_eq!(
            after.summary.updated_at, before.summary.updated_at,
            "el export no reescribe el plano"
        );
        assert_eq!(after.document, before.document, "el documento no cambia");
    }

    /// S1, S3: `path` escribe dentro de la raíz de exportes, y el directorio
    /// que falta se crea.
    #[test]
    fn the_svg_to_a_relative_path_lands_inside_the_root() {
        let _guard = lock_renders_blocking();
        let root = ExportRoot::new("path");
        render(SESSION);

        let mut request = svg_request();
        request.path = Some("arquitectura.svg".to_string());
        let response = export_diagram(&request);

        assert!(response.ok, "{response:?}");
        assert_eq!(response.path.as_deref(), Some("arquitectura.svg"));
        assert_eq!(response.format.as_deref(), Some("svg"));
        assert_eq!(response.content, None, "el archivo no viaja además inline");
        let written =
            std::fs::read(root.join("arquitectura.svg")).expect("the file is in the root");
        assert!(written.starts_with(b"<svg"), "the file is the exported SVG");
        assert_eq!(response.bytes, Some(written.len()));

        let mut nested = svg_request();
        nested.path = Some("docs/x.svg".to_string());
        let response = export_diagram(&nested);
        assert!(response.ok, "{response:?}");
        let nested_bytes =
            std::fs::read(root.join("docs/x.svg")).expect("the nested directory is created");
        assert!(nested_bytes.starts_with(b"<svg"));
        assert_eq!(response.bytes, Some(nested_bytes.len()));
        assert_eq!(root.files(), 2);
    }

    /// S1, S8: la raíz es el único lugar donde caen los bytes; un path
    /// absoluto o con `..` se rechaza y no se escribe nada.
    #[test]
    fn a_path_outside_the_root_is_refused_and_nothing_is_written() {
        let _guard = lock_renders_blocking();
        let root = ExportRoot::new("outside");
        render(SESSION);

        for path in [
            "/tmp/graphium-fuera.svg",
            "../fuera.svg",
            "docs/../..//fuera.svg",
        ] {
            let mut request = svg_request();
            request.path = Some(path.to_string());
            let response = export_diagram(&request);

            assert!(!response.ok, "{path} se rechaza");
            assert_eq!(response.errors.len(), 1, "{path}: {response:?}");
            assert_eq!(
                response.errors[0].code, "export_path_outside_root",
                "{path}"
            );
            assert_eq!(response.errors[0].path, "path");
            assert_eq!(response.path, None);
        }
        assert_eq!(root.files(), 0, "un export rechazado no escribe nada");
    }

    /// S1: sobrescribir exige `overwrite: true`; sin él el archivo conserva
    /// sus bytes viejos.
    #[test]
    fn an_existing_file_needs_overwrite_and_keeps_its_bytes_until_then() {
        let _guard = lock_renders_blocking();
        let root = ExportRoot::new("overwrite");
        render(SESSION);
        let target = root.join("arquitectura.svg");
        std::fs::write(&target, b"<svg>viejo</svg>").expect("the old file is written");

        let mut request = svg_request();
        request.path = Some("arquitectura.svg".to_string());
        let response = export_diagram(&request);

        assert!(!response.ok);
        assert_eq!(response.errors[0].code, "export_exists");
        assert!(
            response.errors[0]
                .hint
                .as_deref()
                .unwrap_or_default()
                .contains("overwrite: true"),
            "{:?}",
            response.errors[0]
        );
        assert_eq!(
            std::fs::read(&target).expect("the old file is there"),
            b"<svg>viejo</svg>",
            "sin overwrite el archivo no se toca"
        );

        request.overwrite = true;
        let response = export_diagram(&request);
        assert!(response.ok, "{response:?}");
        let replaced = std::fs::read(&target).expect("the file exists");
        assert!(replaced.starts_with(b"<svg"), "el SVG reemplazó al viejo");
        assert_ne!(replaced, b"<svg>viejo</svg>");
        assert_eq!(response.bytes, Some(replaced.len()));
    }

    /// S2: la raíz tiene tope de 200 MB; un export que lo pasaría se rechaza
    /// y no crea el archivo.
    #[test]
    fn a_root_at_the_quota_answers_disk_quota_exceeded() {
        let _guard = lock_renders_blocking();
        let root = ExportRoot::new("quota");
        render(SESSION);
        let filler = std::fs::File::create(root.join("lleno.bin")).expect("the filler is created");
        filler
            .set_len(EXPORT_ROOT_QUOTA_BYTES)
            .expect("the filler takes the whole quota");
        drop(filler);

        let mut request = svg_request();
        request.path = Some("no-entra.svg".to_string());
        let response = export_diagram(&request);

        assert!(!response.ok);
        assert_eq!(response.errors[0].code, "disk_quota_exceeded");
        let message = &response.errors[0].message;
        assert!(
            message.contains(&EXPORT_ROOT_QUOTA_BYTES.to_string()),
            "the message names the limit: {message}"
        );
        assert!(
            !root.join("no-entra.svg").exists(),
            "no se escribe nada cuando no entra"
        );
    }

    /// S2: el inline tiene tope de 1 MB. Un documento cuyo SVG lo pasa
    /// responde `too_large` con el tamaño y el hint que manda a archivo.
    #[test]
    fn an_inline_export_past_the_megabyte_is_too_large() {
        let _guard = lock_renders_blocking();
        let root = ExportRoot::new("too-large");
        let document = serde_json::json!({
            "version": 1,
            "kind": "graph",
            "nodes": [ { "id": "grande", "label": "x".repeat(MAX_INLINE_BYTES) } ]
        })
        .to_string();
        let rendered = render_diagram(&RenderRequest {
            document,
            session_id: SESSION.to_string(),
        });
        assert!(rendered.ok, "{rendered:?}");
        let diagram_id = rendered.diagram_id.expect("the diagram is stored");

        let mut request = svg_request();
        request.diagram_id = Some(diagram_id);
        request.inline = true;
        let response = export_diagram(&request);

        assert!(!response.ok);
        assert_eq!(response.errors[0].code, "too_large");
        assert_eq!(response.errors[0].path, "content");
        let size = response.size_bytes.expect("the size is reported");
        assert!(size > MAX_INLINE_BYTES, "{size}");
        assert!(
            response.errors[0].message.contains(&size.to_string()),
            "{:?}",
            response.errors[0].message
        );
        assert!(
            response.errors[0]
                .hint
                .as_deref()
                .unwrap_or_default()
                .contains("path"),
            "{:?}",
            response.errors[0]
        );
        assert_eq!(response.content, None, "nada viaja inline si no entra");
        assert_eq!(root.files(), 0, "un inline rechazado no escribe nada");
    }

    /// S1, S2: el PNG no viaja inline; sin `path` es un `invalid_value` que
    /// dice qué mandar.
    #[test]
    fn png_without_a_path_is_invalid_value() {
        let _guard = lock_renders_blocking();
        let root = ExportRoot::new("png-no-path");
        render(SESSION);

        let mut request = svg_request();
        request.format = "png".to_string();
        let response = export_diagram(&request);

        assert!(!response.ok);
        assert_eq!(response.errors[0].code, "invalid_value");
        assert_eq!(response.errors[0].path, "path");
        assert!(
            response.errors[0]
                .hint
                .as_deref()
                .unwrap_or_default()
                .contains("path"),
            "{:?}",
            response.errors[0]
        );
        assert_eq!(root.files(), 0);
    }

    /// S1, S2, S3: el PNG escribe los bytes del rasterizador, y `opts.scale`
    /// llega hasta él.
    #[test]
    fn the_png_written_to_a_path_carries_the_rasterizer_bytes() {
        let _guard = lock_renders_blocking();
        let root = ExportRoot::new("png");
        render(SESSION);
        install_fake_rasterizer();

        let mut request = svg_request();
        request.format = "png".to_string();
        request.path = Some("grafico.png".to_string());
        request.scale = Some(2.0);
        let response = export_diagram(&request);

        assert!(response.ok, "{response:?}");
        assert_eq!(response.format.as_deref(), Some("png"));
        assert_eq!(response.path.as_deref(), Some("grafico.png"));
        let bytes = std::fs::read(root.join("grafico.png")).expect("the png is in the root");
        assert_eq!(bytes, FAKE_PNG, "el archivo son los bytes del rasterizador");
        assert_eq!(response.bytes, Some(FAKE_PNG.len()));

        let options = last_raster_options().expect("the fake rasterizer ran");
        assert_eq!(options.scale, 2.0, "opts.scale llega al rasterizador");
        assert_eq!(options.max_px, PNG_DEFAULT_MAX_PX);
    }

    /// S2: la tool no manda al rasterizador un `scale` o un `max_px` que ya
    /// sabe inválidos: los rechaza con el path de la opción.
    #[test]
    fn a_png_past_the_ceilings_is_invalid_value() {
        let _guard = lock_renders_blocking();
        let root = ExportRoot::new("png-ceilings");
        render(SESSION);
        install_fake_rasterizer();

        let mut request = svg_request();
        request.format = "png".to_string();
        request.path = Some("grande.png".to_string());
        request.scale = Some(PNG_MAX_SCALE + 1.0);
        let response = export_diagram(&request);
        assert!(!response.ok);
        assert_eq!(response.errors[0].code, "invalid_value");
        assert_eq!(response.errors[0].path, "opts.scale");

        request.scale = None;
        request.max_px = Some(PNG_MAX_MAX_PX + 1);
        let response = export_diagram(&request);
        assert!(!response.ok);
        assert_eq!(response.errors[0].code, "invalid_value");
        assert_eq!(response.errors[0].path, "opts.max_px");

        assert_eq!(root.files(), 0, "no se escribe nada con opciones inválidas");
    }

    /// S1: los formatos son `svg` y `png`; otro es `invalid_value` con lo que
    /// se acepta.
    #[test]
    fn an_unknown_format_is_invalid_value() {
        let _guard = lock_renders_blocking();
        let root = ExportRoot::new("format");
        render(SESSION);

        let mut request = svg_request();
        request.format = "mermaid".to_string();
        let response = export_diagram(&request);

        assert!(!response.ok);
        assert_eq!(response.errors[0].code, "invalid_value");
        assert_eq!(response.errors[0].path, "format");
        assert_eq!(response.errors[0].hint.as_deref(), Some("svg, png"));
        assert_eq!(root.files(), 0);
    }

    /// S1, S3: `inline` y `path` son excluyentes y el error dice qué mandar.
    #[test]
    fn inline_and_path_together_are_invalid_value() {
        let _guard = lock_renders_blocking();
        let root = ExportRoot::new("exclusive");
        render(SESSION);

        let mut request = svg_request();
        request.inline = true;
        request.path = Some("arquitectura.svg".to_string());
        let response = export_diagram(&request);

        assert!(!response.ok);
        assert_eq!(response.errors[0].code, "invalid_value");
        assert!(
            response.errors[0]
                .hint
                .as_deref()
                .unwrap_or_default()
                .contains("no los dos"),
            "{:?}",
            response.errors[0]
        );
        assert_eq!(root.files(), 0);
    }

    /// S4: un `diagram_id` que no está en el plano es `unknown_diagram` con
    /// los ids conocidos.
    #[test]
    fn an_unknown_diagram_id_lists_the_known_ids() {
        let _guard = lock_renders_blocking();
        let _root = ExportRoot::new("unknown-id");
        let diagram_id = render(SESSION);

        let mut request = svg_request();
        request.diagram_id = Some("d-9999".to_string());
        request.inline = true;
        let response = export_diagram(&request);

        assert!(!response.ok);
        assert_eq!(response.errors[0].code, "unknown_diagram");
        assert!(
            response.errors[0].message.contains(&diagram_id),
            "{:?}",
            response.errors[0]
        );
    }

    /// S4, PRD §11.1: exportar el diagrama de otra sesión es `not_owner`.
    #[test]
    fn the_diagram_of_another_session_is_not_owner() {
        let _guard = lock_renders_blocking();
        let _root = ExportRoot::new("not-owner");
        let diagram_id = render("otra-sesion");

        let mut request = svg_request();
        request.diagram_id = Some(diagram_id);
        request.inline = true;
        let response = export_diagram(&request);

        assert!(!response.ok);
        assert_eq!(response.errors[0].code, "not_owner");
        assert!(
            response.errors[0]
                .hint
                .as_deref()
                .unwrap_or_default()
                .contains("otra sesión MCP"),
            "{:?}",
            response.errors[0]
        );
    }

    /// S4: sin `diagram_id` y con dos abiertos, el export no elige: responde
    /// `ambiguous_diagram` con los dos candidatos.
    #[test]
    fn no_id_with_two_open_diagrams_is_ambiguous() {
        let _guard = lock_renders_blocking();
        let root = ExportRoot::new("ambiguous");
        let first = render(SESSION);
        let second = render(SESSION);

        let mut request = svg_request();
        request.inline = true;
        let response = export_diagram(&request);

        assert!(!response.ok);
        assert_eq!(response.errors[0].code, "ambiguous_diagram");
        let message = &response.errors[0].message;
        assert!(
            message.contains(&first) && message.contains(&second),
            "los dos candidatos están en el mensaje: {message}"
        );
        assert!(response.errors[0].hint.is_some());
        assert_eq!(root.files(), 0);
    }

    /// S4: sin `diagram_id` y sin ningún diagrama abierto en la sesión.
    #[test]
    fn no_id_without_an_open_diagram_is_unknown_diagram() {
        let _guard = lock_renders_blocking();
        let _root = ExportRoot::new("no-open");

        let mut request = svg_request();
        request.inline = true;
        let response = export_diagram(&request);

        assert!(!response.ok);
        assert_eq!(response.errors[0].code, "unknown_diagram");
        assert_eq!(response.errors[0].path, "diagram_id");
        assert!(response.content.is_none());
    }

    /// S1: un documento que el render rechaza responde con esos errores y no
    /// escribe nada.
    #[test]
    fn a_document_rejected_by_the_render_writes_nothing() {
        let _guard = lock_renders_blocking();
        let root = ExportRoot::new("invalid-document");
        // Un documento que ya no valida (por ejemplo, restaurado del disco):
        // el export lo rechaza con los errores del render.
        let diagram_id = diagram_store().next_id();
        diagram_store().insert(
            diagram_id.clone(),
            "roto".to_string(),
            "graph".to_string(),
            SESSION.to_string(),
            r#"{"version":1,"kind":"graph","nodes":[{"id":"a"}],"edges":[{"from":"a","to":"b"}]}"#
                .to_string(),
        );

        let mut request = svg_request();
        request.diagram_id = Some(diagram_id.clone());
        request.path = Some("roto.svg".to_string());
        let response = export_diagram(&request);

        assert!(!response.ok);
        assert_eq!(response.errors[0].code, "unknown_ref");
        assert_eq!(response.errors[0].path, "edges[0].to");
        assert_eq!(response.diagram_id.as_deref(), Some(diagram_id.as_str()));
        assert_eq!(root.files(), 0, "no se escribe antes de validar");
    }

    /// R7 (`f6-themes.md`): la puerta del export pide la **paleta** del plano
    /// —el tema más los colores del humano—, no solo el tema: así el archivo
    /// sale con lo que el humano ve.
    #[test]
    fn the_export_asks_for_the_palette_of_the_plan() {
        let _guard = lock_renders_blocking();
        crate::bootstrap();

        gx_core::export::install_palette(gx_core::render::Palette::arcilla());
        let opts = super::export_opts();
        assert!(opts.export, "el export se dibuja como documento cerrado");
        assert_eq!(
            opts.palette,
            Some(gx_core::render::Palette::arcilla()),
            "la paleta del plano viaja en `RenderOpts`"
        );
        assert_eq!(
            opts.theme,
            gx_core::render::Theme::Arcilla,
            "el tema sigue siendo la base de la paleta"
        );

        gx_core::export::install_theme(gx_core::render::Theme::Dark);
    }

    /// S7: la puerta (`gx_core::engine::render_json`) pide el dibujo del export
    /// solo con `export: true`, y lo hace a través del trait object que guarda
    /// `Engines`. El renderer stub pinta el lienzo con la misma marca que el
    /// real (`class="bg"`), así que el cableado se prueba sin `gx-render-svg`
    /// (que `gx-mcp` no puede depender, PRD §9.1).
    #[test]
    fn the_engine_door_asks_for_the_export_drawing_only_when_asked() {
        let _guard = lock_renders_blocking();
        crate::bootstrap();

        let exported = gx_core::engine::render_json(
            crate::test_support::MINIMAL,
            &RenderOpts {
                export: true,
                ..RenderOpts::default()
            },
        )
        .expect("the minimal example renders");
        assert!(
            exported.svg.contains("class=\"bg\""),
            "el export pinta el lienzo: {}",
            exported.svg
        );

        let plan =
            gx_core::engine::render_json(crate::test_support::MINIMAL, &RenderOpts::default())
                .expect("the minimal example renders");
        assert!(
            !plan.svg.contains("class=\"bg\""),
            "el plano sigue transparente: {}",
            plan.svg
        );
    }
}
