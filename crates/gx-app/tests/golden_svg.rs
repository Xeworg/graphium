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
const CASES: [(&str, &str); 23] = [
    ("graph-minimal", MINIMAL),
    ("graph-left-to-right", LEFT_TO_RIGHT),
    ("graph-labeled", LABELED),
    ("er-posts", ER_POSTS),
    ("tree-src", TREE_SRC),
    ("tree-numbered", TREE_NUMBERED),
    ("mindmap-feature", MINDMAP),
    ("sequence-order", SEQUENCE),
    ("sequence-activation", SEQUENCE_ACTIVATION),
    ("timeline-roadmap", TIMELINE_ROADMAP),
    ("timeline-milestone", TIMELINE_MILESTONE),
    ("swimlane-soporte", SWIMLANE_SOPORTE),
    ("flowchart-approval", FLOWCHART_APPROVAL),
    ("state-order", STATE_ORDER),
    ("dataflow-etl", DATAFLOW_ETL),
    ("dataflow-levels", DATAFLOW_LEVELS),
    ("class-shop", CLASS_SHOP),
    ("component-api", COMPONENT_API),
    ("component-nested", COMPONENT_NESTED),
    ("c4-context", C4_CONTEXT),
    ("gantt-plan", GANTT_PLAN),
    ("graph-note", GRAPH_NOTE),
    ("shape-gallery", SHAPE_GALLERY),
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

const TREE_NUMBERED: &str = r#"{
  "version": 1,
  "kind": "tree",
  "title": "graphium",
  "numbering": true,
  "root": { "name": "graphium", "meta": { "owner": "core" }, "children": [
    { "name": "crates", "children": [
      { "name": "gx-core", "note": "IR y motor" },
      { "name": "gx-kinds", "meta": { "kinds": 16 } }
    ] },
    { "name": "docs", "children": [ { "name": "PRD.md" } ] }
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

/// S5 (`f22-dibujo.md`): the activation — the control the `to` actor holds
/// from the message that hands it over until its next outgoing message. Two
/// bars of different spans: API holds rows 1-2, Base rows 2-3.
const SEQUENCE_ACTIVATION: &str = r#"{
  "version": 1,
  "kind": "sequence",
  "title": "pedido con control",
  "participants": [
    { "id": "cliente", "label": "Cliente" },
    { "id": "api", "label": "API" },
    { "id": "db", "label": "Base" }
  ],
  "messages": [
    { "from": "cliente", "to": "api", "label": "POST /orders", "activation": true },
    { "from": "api", "to": "db", "label": "INSERT", "activation": true },
    { "from": "db", "to": "api", "label": "ok", "reply": true },
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

const TIMELINE_MILESTONE: &str = r#"{
  "version": 1,
  "kind": "timeline",
  "title": "Lanzamiento — los puntos que mandan",
  "lanes": [
    { "id": "pre", "label": "Preparación" },
    { "id": "ship", "label": "Entrega" }
  ],
  "events": [
    { "label": "código congelado", "lane": "pre", "role": "milestone" },
    { "label": "beta", "lane": "pre", "end": 2 },
    { "label": "correcciones", "lane": "pre", "order": 1, "end": 3 },
    { "label": "v1.0", "lane": "ship", "order": 3, "role": "milestone" }
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
    { "id": "reclama", "label": "reclama el pedido", "lane": "cliente", "connects": ["ticket"] },
    { "id": "ticket", "label": "abre el ticket", "lane": "soporte", "connects": ["consulta"] },
    { "id": "consulta", "label": "consulta el envío", "lane": "logistica", "connects": ["responde"] },
    { "id": "responde", "label": "responde", "lane": "soporte", "connects": ["reclama"] }
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

/// S7 de `f27-etapa1.md`: un DFD de nivel 2, el documento que escribe los dos
/// campos nuevos —el número de Gane-Sarson de cada proceso (D4) y el nivel
/// declarado (D5)—. Se dibuja como el `dataflow-etl` de arriba más dos cosas
/// que solo existen aquí: la etiqueta de cada proceso empieza por su número
/// (`P1 · valida el pedido`) y el nivel viaja en el canal `meta` de cada nodo.
/// Dos externos, dos procesos numerados y dos depósitos: el nivel 2 descompone
/// un proceso del nivel 1, así que numera.
const DATAFLOW_LEVELS: &str = r#"{
  "version": 1,
  "kind": "dataflow",
  "title": "Cobro — nivel 2",
  "level": 2,
  "nodes": [
    { "id": "cliente", "label": "Cliente", "role": "external" },
    { "id": "valida", "label": "valida el pedido", "role": "process", "number": "P1" },
    { "id": "pedidos", "label": "pedidos", "role": "store" },
    { "id": "cobra", "label": "cobra el total", "role": "process", "number": "P2" },
    { "id": "pagos", "label": "pagos", "role": "store" },
    { "id": "pasarela", "label": "pasarela", "role": "external" }
  ],
  "edges": [
    { "from": "cliente", "to": "valida", "label": "pedido" },
    { "from": "valida", "to": "pedidos", "label": "pedido válido" },
    { "from": "pedidos", "to": "cobra", "label": "total" },
    { "from": "cobra", "to": "pasarela", "label": "cargo" },
    { "from": "pasarela", "to": "cobra", "label": "aprobación" },
    { "from": "cobra", "to": "pagos", "label": "pago" }
  ]
}"#;

const CLASS_SHOP: &str = r#"{
  "version": 1,
  "kind": "class",
  "title": "pedidos",
  "nodes": [
    { "id": "Base", "label": "Base", "role": "abstract" },
    { "id": "Order", "label": "Order", "attributes": ["id: uuid", "total: Money"], "methods": ["cobrar(pago)"] },
    { "id": "Refund", "label": "Refund" },
    { "id": "Payable", "label": "Payable", "role": "interface", "methods": ["total(): Money"] },
    { "id": "Money", "label": "Money" }
  ],
  "edges": [
    { "from": "Order", "to": "Base", "relation": "extends" },
    { "from": "Refund", "to": "Order", "relation": "extends" },
    { "from": "Order", "to": "Payable", "relation": "implements" },
    { "from": "Order", "to": "Money", "relation": "composition", "label": "total" }
  ]
}"#;

const COMPONENT_API: &str = r#"{
  "version": 1,
  "kind": "component",
  "title": "API de pedidos",
  "nodes": [
    { "id": "web", "label": "Web", "role": "container" },
    { "id": "api", "label": "API" },
    { "id": "db", "label": "Postgres", "role": "database" },
    { "id": "pagos", "label": "Pagos", "role": "external" },
    { "id": "jobs", "label": "worker de envíos" }
  ],
  "groups": [
    { "id": "backend", "label": "Backend", "members": ["api", "db", "jobs"] }
  ],
  "edges": [
    { "from": "web", "to": "api", "label": "HTTPS" },
    { "from": "api", "to": "db", "label": "SQL" },
    { "from": "api", "to": "pagos", "label": "cobro" },
    { "from": "api", "to": "jobs", "label": "encola" }
  ]
}"#;

/// RF15: a group inside another group. The outer group owns the inner one as
/// a member and the inner one owns the nodes, so the drawing has two boxes,
/// the outer behind the inner.
const COMPONENT_NESTED: &str = r#"{
  "version": 1,
  "kind": "component",
  "title": "Pedidos",
  "nodes": [
    { "id": "web", "label": "Web", "role": "container" },
    { "id": "api", "label": "API" },
    { "id": "db", "label": "Postgres", "role": "database" },
    { "id": "pagos", "label": "Pagos", "role": "external" }
  ],
  "groups": [
    { "id": "plataforma", "label": "Plataforma", "members": ["web", "servicio"] },
    { "id": "servicio", "label": "Servicio", "members": ["api", "db"] }
  ],
  "edges": [
    { "from": "web", "to": "api", "label": "HTTPS" },
    { "from": "api", "to": "db", "label": "SQL" },
    { "from": "api", "to": "pagos", "label": "cobro" }
  ]
}"#;

/// S5 de `f17-c4-kind.md`: un contexto C4 —dos personas, un sistema y las dos
/// relaciones que las conectan—. Cada elemento se dibuja como una caja con su
/// etiqueta en el encabezado y su tipo en el compartimento de detalle, con la
/// tecnología a la derecha; cada relación lleva su verbo.
const C4_CONTEXT: &str = r#"{
  "version": 1,
  "kind": "c4",
  "title": "Pedidos — contexto",
  "level": "context",
  "elements": [
    { "id": "cliente", "type": "person", "label": "Cliente",
      "description": "hace el pedido" },
    { "id": "soporte", "type": "person", "label": "Soporte" },
    { "id": "pedidos", "type": "system", "label": "Sistema de pedidos",
      "technology": "Rust + Svelte" }
  ],
  "relations": [
    { "from": "cliente", "to": "pedidos", "label": "usa" },
    { "from": "soporte", "to": "pedidos", "label": "consulta" }
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

/// Where the scenes the equivalence probe mounts live (`f11-elementos.md` S8).
fn scenes_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden-scenes")
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

/// The scenes the equivalence probe of `f11-elementos.md` S8 mounts.
///
/// `frontend/tools/equivalence.mjs` dispatches each of these scenes through
/// the real bundle and compares the mounted DOM with the golden SVG of the
/// same document, so the scene the window paints and the file the export
/// writes are the same drawing. The fixtures are committed (one JSON per
/// document, the `elements` of the render) and this test is their freshness
/// gate: a drawing that changed without regenerating them fails here, not in
/// the probe.
///
/// Regenerate with `UPDATE_SCENES=1 cargo test -p gx-app --test golden_svg`.
#[test]
fn the_committed_scenes_match_the_drawing() {
    install_engines();
    let update = std::env::var_os("UPDATE_SCENES").is_some();
    let dir = scenes_dir();
    std::fs::create_dir_all(&dir).expect("the scenes directory exists");

    for (name, document) in CASES {
        let outcome = render_json(document, &RenderOpts::default())
            .unwrap_or_else(|errors| panic!("{name} does not render: {errors:?}"));
        let scene = outcome
            .elements
            .as_ref()
            .unwrap_or_else(|| panic!("{name}: the paint path builds a scene"));
        let json = format!(
            "{}\n",
            serde_json::to_string_pretty(scene).expect("the scene serialises")
        );
        let path = dir.join(format!("{name}.json"));

        if update {
            std::fs::write(&path, &json).expect("the scene fixture is written");
            continue;
        }

        let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!(
                "{name}: the scene fixture is missing ({error}); \
                 regenerate it with UPDATE_SCENES=1"
            )
        });
        assert_eq!(
            json, expected,
            "{name}: the scene changed; if the change is intended, regenerate with \
             UPDATE_SCENES=1 and review the diff"
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

const GANTT_PLAN: &str = r#"{
  "version": 1,
  "kind": "gantt",
  "title": "Restauración — plan",
  "lanes": [
    { "id": "diseno", "label": "Diseño" },
    { "id": "obra", "label": "Obra" }
  ],
  "tasks": [
    { "id": "planos", "label": "Planos", "lane": "diseno",
      "start": 0, "end": 3 },
    { "id": "permiso", "label": "Permiso municipal", "lane": "diseno",
      "start": 2, "end": 5 },
    { "id": "demolicion", "label": "Demolición", "lane": "obra",
      "start": 5, "end": 9, "depends_on": ["permiso"] },
    { "id": "apertura", "label": "Apertura", "lane": "obra",
      "start": 9, "end": 9, "depends_on": ["demolicion"] }
  ]
}"#;

const GRAPH_NOTE: &str = r#"{
  "version": 1,
  "kind": "graph",
  "title": "Nota al margen",
  "nodes": [
    { "id": "idea", "label": "La idea" },
    { "id": "margen", "label": "el margen la anota", "shape": "note" }
  ],
  "edges": [ { "from": "idea", "to": "margen", "label": "anota" } ]
}"#;

const SHAPE_GALLERY: &str = r#"{
  "version": 1,
  "kind": "graph",
  "title": "Las siete formas",
  "nodes": [
    { "id": "caja", "label": "Caja", "shape": "box" },
    { "id": "redondeada", "label": "Redondeada", "shape": "rounded" },
    { "id": "cilindro", "label": "Cilindro", "shape": "cylinder" },
    { "id": "rombo", "label": "Rombo", "shape": "diamond" },
    { "id": "elipse", "label": "Elipse", "shape": "ellipse" },
    { "id": "nota", "label": "Nota", "shape": "note" },
    { "id": "punto", "label": "el punto, su etiqueta al lado", "shape": "circle" }
  ],
  "edges": [
    { "from": "caja", "to": "rombo", "label": "decide" },
    { "from": "rombo", "to": "punto", "label": "commit" }
  ]
}"#;
