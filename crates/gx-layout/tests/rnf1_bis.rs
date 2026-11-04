//! The layout matrix RNF1-bis (PRD §7.3).
//!
//! The thresholds of the PRD (< 500 ms for the first three cases, < 1.5 s for
//! the 300 node case) hold for the reference machine of the CI (Linux x86_64,
//! 4 vCPU, 8 GB, SSD) with `DagreEngine` in cold, without any layout cache.
//! The thresholds are **not** asserted in the ordinary test suite: a debug
//! build is an order of magnitude slower. The measurement lives in the
//! ignored test, which CI runs in release.
//!
//! The matrix is four cases and the measurement stays on those four. The
//! nested reproduction of S3 (`100n/150e + 8 clusters + grupo externo`) is
//! checked here too, as the invariant of S7/S8, and it is not part of the
//! measurement. The same invariant is checked at scale on a graph of sixteen
//! leaf clusters with edges crossing them, which is the shape of the defect
//! that survives the leaf-only registration of S5.

use std::sync::Arc;
use std::time::Instant;

use gx_core::geom::{Rect, Size};
use gx_core::ir::{Direction, IrEdge, IrGraph, IrGroup, IrNode, LayoutHint};
use gx_core::layout::{LayoutEngine, LayoutOpts};
use gx_core::text::{TextMetrics, TextStyle};
use gx_layout::matrix::{rf15_nested_case, rnf1_bis_cases};
use gx_layout::DagreEngine;

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
        let layout = engine().layout(&case.graph, &LayoutOpts::default());
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
        let first = engine().layout(&case.graph, &LayoutOpts::default());
        let second = engine().layout(&case.graph, &LayoutOpts::default());
        assert_eq!(first.nodes, second.nodes, "{}: nodes", case.name);
        assert_eq!(first.edges, second.edges, "{}: edges", case.name);
        assert_eq!(first.bbox, second.bbox, "{}: bbox", case.name);
    }
}

/// The box of a placed node, as a rectangle.
fn box_of(layout: &gx_core::layout::LayoutGraph, id: &str) -> Rect {
    let placed = layout
        .nodes
        .get(id)
        .unwrap_or_else(|| panic!("el nodo `{id}` no se coloca"));
    Rect::new(
        placed.center.x - placed.size.w / 2.0,
        placed.center.y - placed.size.h / 2.0,
        placed.size.w,
        placed.size.h,
    )
}

/// `outer` contains `inner` on every side.
fn contains(outer: &Rect, inner: &Rect) -> bool {
    outer.union(inner) == *outer
}

/// S7/S8: el caso de reproducción de S3 —los ocho clusters hoja dentro de un
/// grupo externo, con aristas que cruzan grupos— coloca los 100 nodos y deja
/// una caja por grupo con sus miembros adentro. Antes de la red de seguridad
/// `dagre` devolvía 82 de 100.
#[test]
fn the_nested_case_places_every_node_of_the_ir() {
    let case = rf15_nested_case();
    let layout = engine().layout(&case.graph, &LayoutOpts::default());

    assert_eq!(
        layout.nodes.len(),
        case.graph.nodes.len(),
        "{}: el layout no puede devolver menos nodos que la IR",
        case.name
    );
    assert_eq!(
        layout.groups.len(),
        case.graph.groups.len(),
        "{}: una caja por grupo, también por los que contienen a otros",
        case.name
    );

    let outer = layout
        .groups
        .iter()
        .find(|group| group.id == "outer")
        .expect("el grupo externo se coloca");
    assert_eq!(outer.depth, 0);
    for group in &layout.groups {
        let ir_group = case
            .graph
            .groups
            .iter()
            .find(|candidate| candidate.id == group.id)
            .unwrap_or_else(|| panic!("`{}` no está en la IR", group.id));
        if group.id != "outer" {
            assert_eq!(group.depth, 1, "los clusters hoja cuelgan del externo");
            assert!(
                contains(&outer.bbox, &group.bbox),
                "{}: la caja externa no contiene a `{}`: {:?} {:?}",
                case.name,
                group.id,
                outer.bbox,
                group.bbox
            );
        }
        for member in &ir_group.members {
            let inside = layout
                .groups
                .iter()
                .find(|candidate| &candidate.id == member)
                .map(|inner| inner.bbox)
                .unwrap_or_else(|| box_of(&layout, member));
            assert!(
                contains(&group.bbox, &inside),
                "{}: `{}` no contiene a su miembro `{member}`: {:?} {:?}",
                case.name,
                group.id,
                group.bbox,
                inside
            );
        }
    }
}

/// S7/S8: la red de seguridad no puede romper la determinismo (O8): la misma
/// entrada, dos veces, da los mismos valores.
#[test]
fn the_nested_case_is_deterministic() {
    let case = rf15_nested_case();

    let first = engine().layout(&case.graph, &LayoutOpts::default());
    let second = engine().layout(&case.graph, &LayoutOpts::default());

    assert_eq!(
        first, second,
        "{}: la misma entrada tiene que dar los mismos valores",
        case.name
    );
}

/// The measurement of the matrix. Run it with:
///
/// ```text
/// cargo test --release -p gx-layout -- --ignored --nocapture
/// ```
#[test]
#[ignore = "measurement: run it in release"]
fn the_matrix_is_measured() {
    // The PRD asks for a p95, not a single run: every case is measured
    // several times and the percentile is reported.
    const RUNS: usize = 10;

    println!("case                              nodes   edges   p95 ms   max ms");
    for case in rnf1_bis_cases() {
        let mut samples = Vec::with_capacity(RUNS);
        for _ in 0..RUNS {
            let started = Instant::now();
            let layout = engine().layout(&case.graph, &LayoutOpts::default());
            let elapsed = started.elapsed().as_secs_f64() * 1000.0;
            assert_eq!(layout.nodes.len(), case.graph.nodes.len());
            samples.push(elapsed);
        }
        samples.sort_by(|left, right| left.partial_cmp(right).expect("no NaN"));
        let p95 = samples[((RUNS as f64 * 0.95).ceil() as usize).min(RUNS) - 1];
        println!(
            "{:<32} {:>5} {:>7} {:>7.1} {:>7.1}",
            case.name,
            case.graph.nodes.len(),
            case.graph.edges.len(),
            p95,
            samples[RUNS - 1]
        );
    }
}

/// Dieciséis clusters hoja de ocho nodos, con aristas que cruzan de un
/// cluster a otro: el defecto de S4 en su forma general, la que sobrevive al
/// arreglo de S5. Medido antes de la red de seguridad: `dagre` colocaba 52 de
/// 128 nodos y 9 de 16 cajas.
fn crowded_leaf_clusters() -> IrGraph {
    let nodes: Vec<IrNode> = (0..128)
        .map(|index| {
            let mut node = IrNode::new(format!("n{index}"), format!("nodo {index}"));
            node.size = Size::new(120.0, 40.0);
            node
        })
        .collect();
    let edges: Vec<IrEdge> = (0..240)
        .map(|index| {
            IrEdge::new(
                format!("n{}", index % 128),
                format!("n{}", (index * 13 + 7) % 128),
            )
        })
        .collect();
    let groups: Vec<IrGroup> = (0..16)
        .map(|cluster| IrGroup {
            id: format!("c{cluster}"),
            label: Some(format!("cluster {cluster}")),
            members: (cluster * 8..cluster * 8 + 8)
                .map(|index| format!("n{index}"))
                .collect(),
        })
        .collect();
    IrGraph {
        direction: Direction::TB,
        layout_hint: LayoutHint::Layered,
        nodes,
        edges,
        groups,
    }
}

/// S7: el motor no puede devolver menos nodos que la IR. Con muchos clusters
/// hoja y aristas que los cruzan, `dagre` 0.1.2 deja nodos sin coordenadas; la
/// red de seguridad reintenta sin nodos compuestos y calcula todas las cajas
/// acá, así que el dibujo sale completo y determinista.
#[test]
fn many_leaf_clusters_with_crossing_edges_never_lose_a_node() {
    let graph = crowded_leaf_clusters();

    let first = engine().layout(&graph, &LayoutOpts::default());

    assert_eq!(
        first.nodes.len(),
        graph.nodes.len(),
        "el layout no puede devolver menos nodos que la IR"
    );
    assert_eq!(
        first.groups.len(),
        graph.groups.len(),
        "una caja por grupo, también en el reintento"
    );
    for group in &first.groups {
        let ir_group = graph
            .groups
            .iter()
            .find(|candidate| candidate.id == group.id)
            .unwrap_or_else(|| panic!("`{}` no está en la IR", group.id));
        for member in &ir_group.members {
            assert!(
                contains(&group.bbox, &box_of(&first, member)),
                "`{}` no contiene a su miembro `{member}`",
                group.id
            );
        }
    }

    // El reintento también es determinista (O8): la misma entrada, los mismos
    // valores, corrida tras corrida.
    let second = engine().layout(&graph, &LayoutOpts::default());
    assert_eq!(
        first, second,
        "el reintento no puede romper el determinismo"
    );
}
