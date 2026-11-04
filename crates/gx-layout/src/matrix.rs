//! The fixed cases of the layout matrix RNF1-bis (PRD §7.3).
//!
//! The matrix is measured with four fixed cases, never with a loose number:
//! `100n/150e` without groups, `100n/400e` without groups, `100n/150e` with
//! 8 clusters and `300n/600e` without groups.
//!
//! Next to those four there is a fifth case, [`rf15_nested_case`], which is
//! **not** part of the matrix the PRD fixes: it is the reproduction of the
//! defect of S3 (`100n/150e + 8 clusters + grupo externo`) and the evidence
//! that nested groups never drop a node (S7, S8 of
//! `odd/tasks/f2-cajas-anidadas.md`).
//!
//! The cases are built with a fixed formula, never with randomness: the same
//! case has to be the same graph on every machine and every run.

use gx_core::ir::{Direction, IrEdge, IrGraph, IrGroup, IrNode, LayoutHint};

/// One fixed case of the matrix.
#[derive(Debug, Clone)]
pub struct Case {
    /// The name of the case, as the PRD writes it.
    pub name: &'static str,
    /// The graph to lay out.
    pub graph: IrGraph,
}

/// The four cases of RNF1-bis, in the order of the PRD.
pub fn rnf1_bis_cases() -> Vec<Case> {
    vec![
        Case {
            name: "100n/150e",
            graph: build(100, 150, 0),
        },
        Case {
            name: "100n/400e",
            graph: build(100, 400, 0),
        },
        Case {
            name: "100n/150e + 8 clusters",
            graph: build(100, 150, 8),
        },
        Case {
            name: "300n/600e",
            graph: build(300, 600, 0),
        },
    ]
}

/// The extra case for RF15: the reproduction of the defect of S3, the eight
/// leaf clusters wrapped in one outer group, with edges that cross groups.
///
/// It is not one of the four cases RNF1-bis measures (the PRD fixes four and
/// the thresholds are asserted for those); it is the nested reproduction that
/// proves the engine never returns fewer nodes than the IR (S8).
pub fn rf15_nested_case() -> Case {
    let mut graph = build(100, 150, 8);
    let members: Vec<String> = graph.groups.iter().map(|group| group.id.clone()).collect();
    graph.groups.push(IrGroup {
        id: "outer".to_string(),
        label: Some("grupo externo".to_string()),
        members,
    });
    Case {
        name: "100n/150e + 8 clusters + grupo externo",
        graph,
    }
}

/// Builds a deterministic graph with `nodes` nodes, `edges` edges and
/// `clusters` clusters.
fn build(nodes: usize, edges: usize, clusters: usize) -> IrGraph {
    let mut ir_nodes = Vec::with_capacity(nodes);
    for index in 0..nodes {
        let mut node = IrNode::new(format!("n{index}"), format!("node {index}"));
        node.size = gx_core::geom::Size::new(120.0, 40.0);
        ir_nodes.push(node);
    }

    let mut ir_edges = Vec::with_capacity(edges);
    for index in 0..edges {
        let from = index % nodes;
        // A fixed stride with a coprime step: the edges spread over the graph
        // without ever repeating the same pair twice.
        let mut to = (from + 1 + (index * 7) % nodes) % nodes;
        if to == from {
            to = (to + 1) % nodes;
        }
        ir_edges.push(IrEdge::new(format!("n{from}"), format!("n{to}")));
    }

    let mut groups = Vec::with_capacity(clusters);
    if let Some(per_cluster) = nodes.checked_div(clusters).filter(|_| clusters > 0) {
        for cluster in 0..clusters {
            let members: Vec<String> = (cluster * per_cluster
                ..((cluster + 1) * per_cluster).min(nodes))
                .map(|index| format!("n{index}"))
                .collect();
            if members.is_empty() {
                continue;
            }
            groups.push(IrGroup {
                id: format!("cluster{cluster}"),
                label: Some(format!("cluster {cluster}")),
                members,
            });
        }
    }

    IrGraph {
        direction: Direction::TB,
        layout_hint: LayoutHint::Layered,
        nodes: ir_nodes,
        edges: ir_edges,
        groups,
    }
}

#[cfg(test)]
mod tests {
    use super::{rf15_nested_case, rnf1_bis_cases};

    #[test]
    fn the_matrix_has_the_four_cases_of_the_prd() {
        let cases = rnf1_bis_cases();
        let names: Vec<&str> = cases.iter().map(|case| case.name).collect();
        assert_eq!(
            names,
            vec![
                "100n/150e",
                "100n/400e",
                "100n/150e + 8 clusters",
                "300n/600e",
            ]
        );
    }

    #[test]
    fn the_cases_have_the_sizes_the_prd_asks_for() {
        let cases = rnf1_bis_cases();
        assert_eq!(
            (cases[0].graph.nodes.len(), cases[0].graph.edges.len()),
            (100, 150)
        );
        assert_eq!(
            (cases[1].graph.nodes.len(), cases[1].graph.edges.len()),
            (100, 400)
        );
        assert_eq!(
            (cases[2].graph.nodes.len(), cases[2].graph.edges.len()),
            (100, 150)
        );
        assert_eq!(cases[2].graph.groups.len(), 8);
        assert_eq!(
            (cases[3].graph.nodes.len(), cases[3].graph.edges.len()),
            (300, 600)
        );
    }

    #[test]
    fn the_cases_are_built_the_same_way_every_time() {
        assert_eq!(
            rnf1_bis_cases()[0].graph.nodes[7].id,
            rnf1_bis_cases()[0].graph.nodes[7].id
        );
    }

    /// El quinto caso es el de S8: los ocho clusters hoja dentro de un grupo
    /// externo, sin tocar los cuatro índices que fija el PRD.
    #[test]
    fn the_extra_case_wraps_the_eight_clusters_in_one_outer_group() {
        let case = rf15_nested_case();
        assert_eq!(
            (
                case.graph.nodes.len(),
                case.graph.edges.len(),
                case.graph.groups.len(),
            ),
            (100, 150, 9)
        );
        let outer = case.graph.groups.last().expect("el grupo externo");
        assert_eq!(outer.id, "outer");
        assert_eq!(outer.members.len(), 8);
        assert!(outer
            .members
            .iter()
            .all(|member| member.starts_with("cluster")));
        // El PRD sigue midiendo cuatro casos, en el mismo orden.
        assert_eq!(
            rnf1_bis_cases()
                .iter()
                .map(|case| case.name)
                .collect::<Vec<_>>(),
            vec![
                "100n/150e",
                "100n/400e",
                "100n/150e + 8 clusters",
                "300n/600e",
            ]
        );
    }
}
