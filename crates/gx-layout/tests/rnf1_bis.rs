//! The layout matrix RNF1-bis (PRD §7.3).
//!
//! The thresholds of the PRD (< 500 ms for the first three cases, < 1.5 s for
//! the 300 node case) hold for the reference machine of the CI (Linux x86_64,
//! 4 vCPU, 8 GB, SSD) with `DagreEngine` in cold, without any layout cache.
//! The thresholds are **not** asserted in the ordinary test suite: a debug
//! build is an order of magnitude slower. The measurement lives in the
//! ignored test, which CI runs in release.

use std::sync::Arc;
use std::time::Instant;

use gx_core::layout::LayoutEngine;
use gx_core::text::{TextMetrics, TextStyle};
use gx_layout::{matrix::rnf1_bis_cases, DagreEngine};

/// A measurer with a fixed width per character: the layout timing does not
/// depend on the font, and the layout crate cannot depend on the renderer.
struct SyntheticMetrics;

impl TextMetrics for SyntheticMetrics {
    fn measure(&self, text: &str, style: &TextStyle) -> gx_core::geom::Size {
        let lines = text.lines().count().max(1) as f64;
        gx_core::geom::Size::new(
            text.chars().count() as f64 * style.size * 0.5,
            lines * style.size * style.line_height,
        )
    }
}

fn engine() -> DagreEngine {
    DagreEngine::new(Arc::new(SyntheticMetrics))
}

#[test]
fn the_four_cases_lay_out_without_failures() {
    for case in rnf1_bis_cases() {
        let layout = engine().layout(&case.graph);
        assert_eq!(
            layout.nodes.len(),
            case.graph.nodes.len(),
            "{}: every node must be placed",
            case.name
        );
        assert_eq!(
            layout.edges.len(),
            case.graph.edges.len(),
            "{}: every edge must be routed",
            case.name
        );
    }
}

#[test]
fn the_four_cases_are_deterministic() {
    for case in rnf1_bis_cases() {
        let first = engine().layout(&case.graph);
        let second = engine().layout(&case.graph);
        assert_eq!(first.nodes, second.nodes, "{}: nodes", case.name);
        assert_eq!(first.edges, second.edges, "{}: edges", case.name);
        assert_eq!(first.bbox, second.bbox, "{}: bbox", case.name);
    }
}

/// The measurement of the matrix. Run it with:
///
/// ```text
/// cargo test --release -p gx-layout -- --ignored --nocapture
/// ```
#[test]
#[ignore = "measurement: run it in release"]
fn the_matrix_is_measured() {
    println!("case                              nodes   edges      ms");
    for case in rnf1_bis_cases() {
        let started = Instant::now();
        let layout = engine().layout(&case.graph);
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        println!(
            "{:<32} {:>5} {:>7} {:>9.1}",
            case.name,
            layout.nodes.len(),
            layout.edges.len(),
            elapsed
        );
    }
}
