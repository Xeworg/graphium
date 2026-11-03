//! The golden SVGs and the byte budget of the drawing (PRD §7.3, RNF1-ter,
//! S24, S25, S28, S29).
//!
//! Determinism (O8) is a promise with a witness: the same JSON has to produce
//! the same bytes, in every process and every machine. The golden files are
//! that witness. They are compared byte to byte, and the byte budget of
//! RNF1-ter is checked on the same drawings, so a change that makes the
//! diagrams heavier fails here and not in somebody's window.
//!
//! To accept a change on purpose, regenerate with `UPDATE_GOLDEN=1`:
//!
//! ```text
//! UPDATE_GOLDEN=1 cargo test -p gx-app --test golden_svg
//! ```
//!
//! The SVG of a diagram whose bytes are not compared is still pinned: the
//! golden set is the contract, so a new case is a new file in `golden/`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gx_core::engine::render_json;
use gx_core::layout::LayoutEngine;
use gx_core::render::{RenderOpts, SvgRenderer};
use gx_core::text::TextMetrics;

/// Header the byte budget of RNF1-ter allows to every drawing.
const HEADER_BYTES: usize = 40 * 1024;
/// Bytes per node the budget of RNF1-ter allows.
const BYTES_PER_NODE: usize = 900;
/// Bytes per edge the budget of RNF1-ter allows.
const BYTES_PER_EDGE: usize = 350;

/// The drawings that are pinned, by file name.
const CASES: [(&str, &str); 13] = [
    ("graph-minimal", MINIMAL),
    ("graph-left-to-right", LEFT_TO_RIGHT),
    ("graph-labeled", LABELED),
    ("er-posts", ER_POSTS),
    ("tree-src", TREE_SRC),
    ("mindmap-feature", MINDMAP),
    ("sequence-order", SEQUENCE),
    ("timeline-roadmap", TIMELINE_ROADMAP),
    ("swimlane-soporte", SWIMLANE_SOPORTE),
    ("flowchart-approval", FLOWCHART_APPROVAL),
    ("state-order", STATE_ORDER),
    ("dataflow-etl", DATAFLOW_ETL),
    ("class-shop", CLASS_SHOP),
];

const MINIMAL: &str = r#"{
  "version": 1,
  "kind": "graph",
  "title": "parse_config() — flujo",
  "direction": "TB",
  "nodes": [
    { "id": "read", "label": "read file" },
    { "id": "parse", "label": "parse" },
    { "id": "ok", "label": "Config" },
    { "id": "fail", "label": "error" }
  ],
  "edges": [
    { "from": "read", "to": "parse" },
    { "from": "parse", "to": "ok" },
    { "from": "parse", "to": "fail" }
  ]
}"#;

const LEFT_TO_RIGHT: &str = r#"{
  "version": 1,
  "kind": "graph",
  "title": "pipeline",
  "direction": "LR",
  "nodes": [
    { "id": "in", "label": "entrada" },
    { "id": "core", "label": "gx-core" },
    { "id": "out", "label": "salida" }
  ],
  "edges": [
    { "from": "in", "to": "core" },
    { "from": "core", "to": "out" }
  ]
}"#;

const LABELED: &str = r#"{
  "version": 1,
  "kind": "graph",
  "title": "decisiones",
  "nodes": [
    { "id": "ask", "label": "¿hay hueco?" },
    { "id": "place", "label": "colocar" },
    { "id": "cascade", "label": "cascada" }
  ],
  "edges": [
    { "from": "ask", "to": "place", "label": "sí" },
    { "from": "ask", "to": "cascade", "label": "no" }
  ]
}"#;

const ER_POSTS: &str = r#"{
  "version": 1,
  "kind": "er",
  "title": "Schema — posts",
  "tables": [
    { "name": "users", "columns": [
        { "name": "id", "type": "uuid", "key": "pk" },
        { "name": "email", "type": "text" }
    ] },
    { "name": "posts", "columns": [
        { "name": "id", "type": "uuid", "key": "pk" },
        { "name": "user_id", "type": "uuid", "key": "fk", "nullable": true },
        { "name": "title", "type": "text" }
    ] }
  ],
  "relations": [
    { "from": { "table": "users", "column": "id" },
      "to": { "table": "posts", "column": "user_id" },
      "cardinality": "1:N" }
  ]
}"#;

const TREE_SRC: &str = r#"{
  "version": 1,
  "kind": "tree",
  "title": "src",
  "root": { "name": "src", "children": [
    { "name": "main.rs", "note": "48 líneas" },
    { "name": "lib", "children": [
      { "name": "mod.rs" },
      { "name": "render.rs", "note": "312 líneas" },
      { "name": "layout.rs" }
    ] }
  ] }
}"#;

const MINDMAP: &str = r#"{
  "version": 1,
  "kind": "mindmap",
  "title": "Graphium",
  "root": { "topic": "Graphium", "branches": [
    { "topic": "MCP", "emphasis": "highlight", "branches": [
      { "topic": "tools" }, { "topic": "harness" } ] },
    { "topic": "Plano", "branches": [ { "topic": "cámara" }, { "topic": "culling" } ] }
  ] }
}"#;

const SEQUENCE: &str = r#"{
  "version": 1,
  "kind": "sequence",
  "title": "pedido",
  "participants": [
    { "id": "cliente", "label": "Cliente" },
    { "id": "api", "label": "API" },
    { "id": "db", "label": "Base" }
  ],
  "messages": [
    { "from": "cliente", "to": "api", "label": "POST /orders" },
    { "from": "api", "to": "db", "label": "INSERT" },
    { "from": "db", "to": "api", "label": "ok", "reply": true },
    { "from": "api", "to": "api", "label": "reintento", "reply": true },
    { "from": "api", "to": "cliente", "label": "201", "reply": true }
  ]
}"#;

const TIMELINE_ROADMAP: &str = r#"{
  "version": 1,
  "kind": "timeline",
  "title": "Roadmap 2026",
  "lanes": [
    { "id": "q1", "label": "Q1" },
    { "id": "q2", "label": "Q2" }
  ],
  "events": [
    { "label": "spike F0", "lane": "q1" },
    { "label": "MVP", "lane": "q1", "end": 2 },
    { "label": "los 10 kinds", "lane": "q2" },
    { "label": "export", "lane": "q2", "end": 4 }
  ]
}"#;

const SWIMLANE_SOPORTE: &str = r#"{
  "version": 1,
  "kind": "swimlane",
  "title": "Soporte — pedido trabado",
  "lanes": [
    { "id": "cliente", "label": "Cliente" },
    { "id": "soporte", "label": "Soporte" },
    { "id": "logistica", "label": "Logística" }
  ],
  "steps": [
    { "label": "reclama el pedido", "lane": "cliente" },
    { "label": "abre el ticket", "lane": "soporte" },
    { "label": "consulta el envío", "lane": "logistica" },
    { "label": "responde", "lane": "soporte" }
  ]
}"#;

const FLOWCHART_APPROVAL: &str = r#"{
  "version": 1,
  "kind": "flowchart",
  "title": "aprobación de un gasto",
  "nodes": [
    { "id": "inicio", "label": "pedido", "role": "start" },
    { "id": "monto", "label": "monto del gasto" },
    { "id": "limite", "label": "¿supera el límite?", "role": "decision" },
    { "id": "jefe", "label": "aprueba el jefe" },
    { "id": "auto", "label": "aprobado en el acto" },
    { "id": "fin", "label": "fin", "role": "end" }
  ],
  "edges": [
    { "from": "inicio", "to": "monto" },
    { "from": "monto", "to": "limite" },
    { "from": "limite", "to": "jefe", "label": "sí" },
    { "from": "limite", "to": "auto", "label": "no" },
    { "from": "jefe", "to": "fin" },
    { "from": "auto", "to": "fin" }
  ]
}"#;

const STATE_ORDER: &str = r#"{
  "version": 1,
  "kind": "state",
  "title": "pedido",
  "nodes": [
    { "id": "nuevo", "label": "nuevo", "role": "initial" },
    { "id": "pago", "label": "pago pendiente" },
    { "id": "enviado", "label": "enviado" },
    { "id": "cerrado", "label": "cerrado", "role": "final" }
  ],
  "edges": [
    { "from": "nuevo", "to": "pago", "label": "crear" },
    { "from": "pago", "to": "enviado", "label": "pagar" },
    { "from": "pago", "to": "nuevo", "label": "timeout" },
    { "from": "enviado", "to": "cerrado", "label": "entregar" }
  ]
}"#;

const DATAFLOW_ETL: &str = r#"{
  "version": 1,
  "kind": "dataflow",
  "title": "ETL de pedidos",
  "nodes": [
    { "id": "crm", "label": "CRM", "role": "external" },
    { "id": "ingesta", "label": "ingesta", "role": "process" },
    { "id": "crudo", "label": "pedidos crudos", "role": "store" },
    { "id": "limpieza", "label": "limpieza", "role": "process" },
    { "id": "curado", "label": "pedidos curados", "role": "store" },
    { "id": "bi", "label": "tablero", "role": "external" }
  ],
  "edges": [
    { "from": "crm", "to": "ingesta", "label": "pedidos" },
    { "from": "ingesta", "to": "crudo", "label": "pedidos crudos" },
    { "from": "crudo", "to": "limpieza", "label": "pedidos crudos" },
    { "from": "limpieza", "to": "curado", "label": "pedidos curados" },
    { "from": "curado", "to": "bi", "label": "pedidos curados" }
  ]
}"#;

const CLASS_SHOP: &str = r#"{
  "version": 1,
  "kind": "class",
  "title": "pedidos",
  "nodes": [
    { "id": "Base", "label": "Base", "role": "abstract" },
    { "id": "Order", "label": "Order" },
    { "id": "Refund", "label": "Refund" },
    { "id": "Payable", "label": "Payable", "role": "interface" },
    { "id": "Money", "label": "Money" }
  ],
  "edges": [
    { "from": "Order", "to": "Base", "relation": "extends" },
    { "from": "Refund", "to": "Order", "relation": "extends" },
    { "from": "Order", "to": "Payable", "relation": "implements" },
    { "from": "Order", "to": "Money", "relation": "composition", "label": "total" }
  ]
}"#;

fn install_engines() {
    let metrics: Arc<dyn TextMetrics> =
        Arc::new(gx_render_svg::InterMetrics::new().expect("fonts"));
    // El despachador, no un motor suelto: el golden tiene que ser el dibujo
    // que el producto produce, y el producto elige el motor por `layout_hint`.
    let layout: Arc<dyn LayoutEngine> = Arc::new(gx_layout::Layouts::new(metrics.clone()));
    let renderer: Arc<dyn SvgRenderer> = Arc::new(gx_render_svg::SvgRenderer::new(metrics.clone()));
    gx_core::engine::install(gx_core::engine::Engines {
        layout,
        metrics,
        renderer,
    });
    gx_kinds::register_all();
}

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

/// A stable digest of the SVG, so the CI log shows what changed without
/// needing the diff: FNV-1a 64, no dependency.
fn digest(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[test]
fn the_golden_drawings_do_not_change() {
    install_engines();
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let dir = golden_dir();
    std::fs::create_dir_all(&dir).expect("the golden directory exists");

    for (name, document) in CASES {
        let outcome = render_json(document, &RenderOpts::default())
            .unwrap_or_else(|errors| panic!("{name} does not render: {errors:?}"));
        let path = dir.join(format!("{name}.svg"));
        let hash = digest(&outcome.svg);
        println!(
            "{name}: {} nodes, {} edges, {} bytes, fnv1a64 {hash}",
            outcome.metrics.nodes, outcome.metrics.edges, outcome.metrics.svg_bytes
        );

        if update {
            std::fs::write(&path, &outcome.svg).expect("the golden is written");
            continue;
        }

        let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!("{name}: the golden is missing ({error}); regenerate it with UPDATE_GOLDEN=1")
        });
        assert!(!expected.is_empty(), "{name}: the golden file is empty");
        assert_eq!(
            hash,
            digest(&expected),
            "{name}: the drawing changed ({} bytes now, {} in the golden). If the change is \
             intended, regenerate with UPDATE_GOLDEN=1 and review the diff",
            outcome.svg.len(),
            expected.len()
        );
        assert_eq!(
            outcome.svg, expected,
            "{name}: the drawing changed byte by byte"
        );
    }
}

/// PRD §7.3, RNF1-ter: the drawing of a `graph` weighs at most
/// `900 B × nodos + 350 B × aristas + 40 KB`.
#[test]
fn the_drawings_stay_inside_the_byte_budget() {
    install_engines();

    for (name, document) in CASES {
        let outcome = render_json(document, &RenderOpts::default())
            .unwrap_or_else(|errors| panic!("{name} does not render: {errors:?}"));
        let budget = BYTES_PER_NODE * outcome.metrics.nodes
            + BYTES_PER_EDGE * outcome.metrics.edges
            + HEADER_BYTES;
        assert!(
            outcome.metrics.svg_bytes <= budget,
            "{name}: {} bytes over a budget of {budget} ({} nodes, {} edges)",
            outcome.metrics.svg_bytes,
            outcome.metrics.nodes,
            outcome.metrics.edges
        );
    }
}

/// The same document twice in the same process: the promise of O8 that the
/// golden files cannot make on their own, because they are read once.
#[test]
fn the_same_document_renders_the_same_bytes_twice() {
    install_engines();

    for (name, document) in CASES {
        let first = render_json(document, &RenderOpts::default()).expect("renders");
        let second = render_json(document, &RenderOpts::default()).expect("renders");
        assert_eq!(first.svg, second.svg, "{name} is not deterministic");
    }
}
