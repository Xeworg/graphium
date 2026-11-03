//! Memory with the full plan (PRD §12, RNF10).
//!
//! «RSS < 250 MB con 12 diagramas de 300 nodos» es el techo de F2. Se mide
//! con los motores reales y sin ventana: doce dibujos de 300 nodos viven a la
//! vez, que es lo que el plano tiene que aguantar con el tope de doce.
//!
//! La medición no corre en la suite ordinaria porque en debug los tiempos y
//! el uso de memoria no significan nada: es un test `#[ignore]` que el
//! workflow corre con `--release` al publicar una etiqueta.
//!
//! ```text
//! cargo test --release -p gx-app --test rnf10 -- --ignored --nocapture
//! ```

use std::sync::Arc;

use gx_core::engine::render_json;
use gx_core::layout::LayoutEngine;
use gx_core::render::{RenderOpts, SvgRenderer};
use gx_core::text::TextMetrics;

/// The ceiling of the PRD, in bytes.
const MAX_RSS_BYTES: usize = 250 * 1024 * 1024;
/// Diagrams the plan holds at once (PRD §11.1).
const DIAGRAMS: usize = 12;
/// Nodes per diagram, the worst case the PRD names.
const NODES: usize = 300;

fn install_engines() {
    let metrics: Arc<dyn TextMetrics> =
        Arc::new(gx_render_svg::InterMetrics::new().expect("fonts"));
    let layout: Arc<dyn LayoutEngine> = Arc::new(gx_layout::DagreEngine::new(metrics.clone()));
    let renderer: Arc<dyn SvgRenderer> = Arc::new(gx_render_svg::SvgRenderer::new(metrics.clone()));
    gx_core::engine::install(gx_core::engine::Engines {
        layout,
        metrics,
        renderer,
    });
    gx_kinds::register_all();
}

/// A chain of `nodes` nodes: the worst case for the layout and for the SVG.
fn chain(count: usize) -> String {
    let ids: Vec<String> = (0..count)
        .map(|i| format!(r#"{{ "id": "n{i}", "label": "nodo {i} del pipeline" }}"#))
        .collect();
    let edges: Vec<String> = (0..count.saturating_sub(1))
        .map(|i| format!(r#"{{ "from": "n{i}", "to": "n{}" }}"#, i + 1))
        .collect();
    format!(
        r#"{{ "version": 1, "kind": "graph", "title": "grande", "direction": "LR",
             "nodes": [{}], "edges": [{}] }}"#,
        ids.join(", "),
        edges.join(", ")
    )
}

/// The resident set size of this process, in bytes.
///
/// Linux publishes it in `/proc/self/statm` as pages; the page size of the
/// reference machine of the CI (x86_64) is 4 KiB.
fn resident_bytes() -> usize {
    let statm = std::fs::read_to_string("/proc/self/statm").expect("/proc/self/statm");
    let pages: usize = statm
        .split_whitespace()
        .nth(1)
        .expect("the resident field")
        .parse()
        .expect("a number");
    pages * 4096
}

#[test]
#[ignore = "medición de RNF10: corre en release desde el workflow de la etiqueta"]
fn twelve_diagrams_of_three_hundred_nodes_stay_under_the_ceiling() {
    install_engines();
    let document = chain(NODES);

    // The plan keeps twelve drawings alive at the same time: their SVGs are
    // what the window holds in memory.
    let mut paintings: Vec<String> = Vec::with_capacity(DIAGRAMS);
    for index in 0..DIAGRAMS {
        let outcome = render_json(&document, &RenderOpts::default())
            .unwrap_or_else(|errors| panic!("diagram {index} does not render: {errors:?}"));
        assert_eq!(outcome.metrics.nodes, NODES);
        paintings.push(outcome.svg);
    }

    let rss = resident_bytes();
    let megabytes = rss as f64 / (1024.0 * 1024.0);
    println!(
        "RNF10: {} diagramas de {} nodos, {} KB de SVG, RSS {:.1} MB (techo {} MB)",
        DIAGRAMS,
        NODES,
        paintings.iter().map(String::len).sum::<usize>() / 1024,
        megabytes,
        MAX_RSS_BYTES / (1024 * 1024)
    );
    assert!(
        rss < MAX_RSS_BYTES,
        "RSS {megabytes:.1} MB sobre el techo de {} MB de RNF10",
        MAX_RSS_BYTES / (1024 * 1024)
    );
}
