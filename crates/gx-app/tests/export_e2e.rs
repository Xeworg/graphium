//! End-to-end of the export (PRD §7 RF10, feature doc `f2-export.md`, T5).
//!
//! The unit tests of `gx-mcp` use a fake rasterizer, so they prove the tool's
//! contract but not that our own SVG survives `resvg`. This test runs the real
//! wiring — the app's engines and the real `ResvgRasterizer` — and exports one
//! diagram through the same two functions the MCP server calls.

use std::path::PathBuf;

use gx_mcp::export::{export_diagram, ExportRequest};
use gx_mcp::tools::{render_diagram, RenderRequest};

fn document() -> String {
    serde_json::json!({
        "version": 1,
        "kind": "graph",
        "title": "export e2e",
        "nodes": [
            { "id": "a", "label": "pedido" },
            { "id": "b", "label": "cocina" }
        ],
        "edges": [{ "from": "a", "to": "b", "label": "prepara" }]
    })
    .to_string()
}

/// An exports root of its own, so the test never touches the human's.
fn exports_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!("graphium-e2e-export-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("the exports root is created");
    std::env::set_var("GRAPHIUM_EXPORTS_ROOT", &root);
    root
}

fn request(format: &str, inline: bool, path: Option<&str>) -> ExportRequest {
    ExportRequest {
        session_id: "e2e".to_string(),
        diagram_id: None,
        format: format.to_string(),
        inline,
        path: path.map(str::to_string),
        overwrite: false,
        scale: None,
        max_px: None,
    }
}

#[test]
fn a_real_diagram_travels_inline_and_lands_as_a_png() {
    let root = exports_root();
    gx_app::install_engines().expect("the app engines are installed");
    gx_kinds::register_all();

    let rendered = render_diagram(&RenderRequest {
        document: document(),
        session_id: "e2e".to_string(),
    });
    assert!(rendered.ok, "the diagram renders: {:?}", rendered.errors);

    // Inline: the SVG comes back as text, and it is a closed document (S7) —
    // the canvas is painted, through the trait the engine calls.
    let inline = export_diagram(&request("svg", true, None));
    assert!(inline.ok, "the inline export works: {:?}", inline.errors);
    let svg = inline.content.expect("the SVG travels inline");
    assert!(svg.starts_with("<svg"), "the content is the SVG: {svg}");
    assert!(
        svg.contains("<rect class=\"bg\""),
        "the exported SVG paints its canvas: {svg}"
    );

    // PNG: the real rasterizer turns that same SVG into a file (S1, S2).
    let png = export_diagram(&request("png", false, Some("e2e/diagrama.png")));
    assert!(png.ok, "the PNG export works: {:?}", png.errors);
    assert_eq!(png.path.as_deref(), Some("e2e/diagrama.png"));

    let written = std::fs::read(root.join("e2e/diagrama.png")).expect("the PNG is written");
    assert_eq!(
        written.len(),
        png.bytes.expect("the response reports the size")
    );
    assert!(
        written.len() > 1_000,
        "the PNG has pixels, not just a header: {} bytes",
        written.len()
    );
    assert_eq!(&written[..8], b"\x89PNG\r\n\x1a\n");

    // The SVG of the same diagram lands as a file too, byte for byte.
    let to_file = export_diagram(&request("svg", false, Some("e2e/diagrama.svg")));
    assert!(
        to_file.ok,
        "the SVG file export works: {:?}",
        to_file.errors
    );
    let on_disk = std::fs::read_to_string(root.join("e2e/diagrama.svg")).expect("the SVG is read");
    assert_eq!(
        on_disk, svg,
        "the file and the inline text are the same bytes"
    );
}
