//! Golden of the export with nested clusters and emphasis (feature doc
//! `f2-cierre.md`, R1/T1; PRD §7 RF10, RF15, RF16, §8.5.2).
//!
//! The export re-renders the stored document through the same renderer the
//! plan paints with, so the drawing has to arrive complete: every group box
//! with its depth and its indented name, every emphasis class. The SVG of the
//! export is pinned byte by byte — the drawing is deterministic (O8) — and the
//! PNG is checked through its header, because its bytes belong to `resvg` and
//! not to us (PRD §8.5.1, S10).
//!
//! To accept a change on purpose, regenerate with `UPDATE_GOLDEN=1`:
//!
//! ```text
//! UPDATE_GOLDEN=1 cargo test -p gx-app --test export_golden
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gx_core::engine::{install, Engines};
use gx_core::export::{install_rasterizer, install_theme};
use gx_core::layout::LayoutEngine;
use gx_core::render::{SvgRenderer, Theme};
use gx_core::text::TextMetrics;
use gx_mcp::export::{export_diagram, ExportRequest};
use gx_mcp::tools::{render_diagram, render_diagram_from_json, RenderRequest};

/// A `component` document with a group inside another group and emphasis on a
/// node and on two edges: the three things the export has to keep (RF15,
/// RF16).
const NESTED_WITH_EMPHASIS: &str = r#"{
  "version": 1,
  "kind": "component",
  "title": "Pedidos — fronteras",
  "nodes": [
    { "id": "web", "label": "Web", "role": "container" },
    { "id": "api", "label": "API", "role": "component", "emphasis": "highlight" },
    { "id": "db", "label": "Postgres", "role": "database" },
    { "id": "pagos", "label": "Pagos", "role": "external", "emphasis": "muted" },
    { "id": "jobs", "label": "worker de envíos" }
  ],
  "groups": [
    { "id": "plataforma", "label": "Plataforma", "members": ["web", "servicio"] },
    { "id": "servicio", "label": "Servicio", "members": ["api", "db", "jobs"] }
  ],
  "edges": [
    { "from": "web", "to": "api", "label": "HTTPS" },
    { "from": "api", "to": "db", "label": "SQL", "emphasis": "highlight" },
    { "from": "api", "to": "pagos", "label": "cobro" },
    { "from": "api", "to": "jobs", "label": "encola", "emphasis": "muted" }
  ]
}"#;

/// The exports root of this test binary: a fresh directory under the system
/// temp directory, removed when the test ends. The human's root is never
/// touched.
///
/// `GRAPHIUM_EXPORTS_ROOT` is process-wide, so the tests of this binary take
/// turns owning it instead of racing over it.
static EXPORTS_ROOT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn take_exports_root(name: &str) -> (std::sync::MutexGuard<'static, ()>, ExportsRoot) {
    // A test that panics poisons the lock: the other tests still run.
    let guard = EXPORTS_ROOT_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    (guard, ExportsRoot::new(name))
}

struct ExportsRoot {
    path: PathBuf,
}

impl ExportsRoot {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "graphium-export-golden-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("the temporary exports root is created");
        std::env::set_var("GRAPHIUM_EXPORTS_ROOT", &path);
        Self { path }
    }

    fn join(&self, relative: &str) -> PathBuf {
        self.path.join(relative)
    }
}

impl Drop for ExportsRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
        std::env::remove_var("GRAPHIUM_EXPORTS_ROOT");
    }
}

/// The real engines and the real rasterizer, without reading the settings of
/// the human: the golden has to be the same bytes in every machine.
fn install_engines() {
    let metrics: Arc<dyn TextMetrics> =
        Arc::new(gx_render_svg::InterMetrics::new().expect("fonts"));
    let layout: Arc<dyn LayoutEngine> = Arc::new(gx_layout::Layouts::new(metrics.clone()));
    let renderer: Arc<dyn SvgRenderer> = Arc::new(gx_render_svg::SvgRenderer::new(metrics.clone()));
    install(Engines {
        layout,
        metrics,
        renderer,
    });
    install_rasterizer(Arc::new(gx_render_svg::ResvgRasterizer::new()));
    install_theme(Theme::Dark);
    gx_kinds::register_all();
}

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn request(session_id: &str, diagram_id: &str, format: &str) -> ExportRequest {
    ExportRequest {
        session_id: session_id.to_string(),
        diagram_id: Some(diagram_id.to_string()),
        format: format.to_string(),
        inline: format == "svg",
        path: None,
        overwrite: false,
        scale: None,
        max_px: None,
    }
}

/// The `(width, height)` the exported SVG declares, in plan units.
fn svg_size(svg: &str) -> (f64, f64) {
    let attribute = |name: &str| -> f64 {
        let needle = format!("{name}=\"");
        let start = svg
            .find(&needle)
            .unwrap_or_else(|| panic!("the SVG declares `{name}`: {svg}"))
            + needle.len();
        let end = start + svg[start..].find('"').expect("the attribute is closed");
        svg[start..end]
            .parse()
            .unwrap_or_else(|error| panic!("`{name}` is a number: {error}"))
    };
    (attribute("width"), attribute("height"))
}

/// The size the PNG declares in its `IHDR` chunk.
fn png_size(png: &[u8]) -> (u32, u32) {
    assert_eq!(
        &png[..8],
        b"\x89PNG\r\n\x1a\n",
        "a PNG starts with its magic"
    );
    assert_eq!(&png[12..16], b"IHDR", "the first chunk is IHDR");
    let width = u32::from_be_bytes(png[16..20].try_into().expect("four bytes"));
    let height = u32::from_be_bytes(png[20..24].try_into().expect("four bytes"));
    (width, height)
}

/// R1: the exported SVG keeps the nested group boxes —outer behind inner,
/// inner name indented— and the emphasis classes of the nodes and edges.
#[test]
fn the_export_of_nested_clusters_keeps_every_group_and_emphasis() {
    install_engines();
    let (_env, _root) = take_exports_root("nested");
    let session = format!("export-golden-nested-{}", std::process::id());

    let rendered = render_diagram(&RenderRequest {
        document: NESTED_WITH_EMPHASIS.to_string(),
        session_id: session.clone(),
    });
    assert!(rendered.ok, "the diagram renders: {:?}", rendered.errors);
    let diagram_id = rendered.diagram_id.expect("the render stores the diagram");

    let exported = export_diagram(&request(&session, &diagram_id, "svg"));
    assert!(exported.ok, "the export works: {:?}", exported.errors);
    let svg = exported.content.expect("the SVG travels inline");

    // The canvas is painted and both containers are there (RF15).
    assert!(
        svg.contains("<rect class=\"bg\""),
        "the export is closed: {svg}"
    );
    let outer = svg
        .find("id=\"cluster-plataforma\"")
        .expect("the outer group box is drawn");
    let inner = svg
        .find("id=\"cluster-servicio\"")
        .expect("the inner group box is drawn");
    assert!(
        outer < inner,
        "the outer box is drawn behind the inner one: {svg}"
    );
    assert!(
        svg.contains(">Plataforma<"),
        "the outer group is named: {svg}"
    );
    assert!(
        svg.contains(">Servicio<"),
        "the inner group is named: {svg}"
    );

    // The names are indented by depth, so nesting is visible (LayoutGroup.depth).
    let label_x = |label: &str| -> f64 {
        let end = svg
            .find(&format!(">{label}<"))
            .unwrap_or_else(|| panic!("the label `{label}` is drawn: {svg}"));
        let start = svg[..end]
            .rfind("<text class=\"group-label text\" x=\"")
            .expect("the label is a group label")
            + "<text class=\"group-label text\" x=\"".len();
        svg[start..]
            .split('"')
            .next()
            .expect("the x attribute is closed")
            .parse()
            .expect("x is a number")
    };
    assert!(
        label_x("Servicio") > label_x("Plataforma"),
        "the inner name is indented past the outer one: {svg}"
    );

    // The emphasis of the document arrives as classes (RF16).
    for class in [
        "emphasis-highlight",
        "emphasis-highlight-text",
        "emphasis-muted",
    ] {
        assert!(svg.contains(class), "the export keeps `{class}`: {svg}");
    }

    // The SVG of the export is the deterministic drawing (O8): pinned bytes.
    let path = golden_dir().join("export-clusters-emphasis.svg");
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(golden_dir()).expect("the golden directory exists");
        std::fs::write(&path, &svg).expect("the golden is written");
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("the golden is missing ({error}); regenerate it with UPDATE_GOLDEN=1")
    });
    assert_eq!(
        svg, expected,
        "the exported drawing changed. If the change is intended, regenerate with \
         UPDATE_GOLDEN=1 and review the diff"
    );
}

/// R1: the PNG of the same export rasterizes that SVG at its declared size, so
/// the groups and the emphasis reach the pixels too.
#[test]
fn the_png_of_the_nested_export_covers_the_same_canvas() {
    install_engines();
    let (_env, root) = take_exports_root("png-nested");
    let session = format!("export-golden-png-{}", std::process::id());

    let rendered = render_diagram(&RenderRequest {
        document: NESTED_WITH_EMPHASIS.to_string(),
        session_id: session.clone(),
    });
    assert!(rendered.ok, "the diagram renders: {:?}", rendered.errors);
    let diagram_id = rendered.diagram_id.expect("the render stores the diagram");

    let inline = export_diagram(&request(&session, &diagram_id, "svg"));
    let svg = inline.content.expect("the SVG travels inline");

    let mut to_file = request(&session, &diagram_id, "png");
    to_file.inline = false;
    to_file.path = Some("golden/export-clusters-emphasis.png".to_string());
    let png = export_diagram(&to_file);
    assert!(png.ok, "the PNG export works: {:?}", png.errors);

    let written = std::fs::read(root.join("golden/export-clusters-emphasis.png"))
        .expect("the PNG lands inside the root");
    assert_eq!(
        written.len(),
        png.bytes.expect("the response reports the size")
    );

    let (width, height) = svg_size(&svg);
    let (png_width, png_height) = png_size(&written);
    assert_eq!(
        png_width,
        width.ceil() as u32,
        "the PNG covers the SVG width"
    );
    assert_eq!(
        png_height,
        height.ceil() as u32,
        "the PNG covers the SVG height"
    );
}

/// R1: an export is the whole document even when the plan drew a view of it.
/// `opts.extract` is a view, never an edit (PRD §7.3), so the file keeps the
/// nodes the extract left out and the containers they live in.
#[test]
fn the_export_is_the_whole_document_after_an_extract_view() {
    install_engines();
    let (_env, _root) = take_exports_root("extract");
    let session = format!("export-golden-extract-{}", std::process::id());

    let narrowed = render_diagram_from_json(
        &RenderRequest {
            document: NESTED_WITH_EMPHASIS.to_string(),
            session_id: session.clone(),
        },
        Some(&serde_json::json!({ "extract": { "around": "web", "depth": 0 } })),
    );
    assert!(narrowed.ok, "the extract renders: {:?}", narrowed.errors);
    assert_eq!(
        narrowed.metrics.expect("metrics").nodes,
        1,
        "the view is the node alone"
    );
    let diagram_id = narrowed.diagram_id.expect("the render stores the diagram");
    assert_eq!(
        narrowed.extracted_from.as_deref(),
        Some(format!("{diagram_id}@0").as_str())
    );

    let exported = export_diagram(&request(&session, &diagram_id, "svg"));
    assert!(exported.ok, "the export works: {:?}", exported.errors);
    let svg = exported.content.expect("the SVG travels inline");

    for outside in ["g-api", "g-db", "g-pagos", "g-jobs"] {
        assert!(
            svg.contains(&format!("id=\"{outside}\"")),
            "the export is not the extract view: `{outside}` is missing from {svg}"
        );
    }
    for group in ["cluster-plataforma", "cluster-servicio"] {
        assert!(
            svg.contains(&format!("id=\"{group}\"")),
            "the containers of the whole document survive: `{group}`"
        );
    }
}
