//! Test scaffolding shared by the kind modules (`f20-reuso.md`, S5 tail).
//!
//! Every kind parses its documents the same way, and every kind whose document
//! has `groups` asserts the same thing about them. The copies lived one per
//! module —ten of the first, five of the second— and a drift between them was
//! a matter of time; here they are written once.

use gx_core::error::ErrorCode;
use gx_core::ir::{Ir, IrGraph};
use gx_core::kind::DiagramKind;
use serde::de::DeserializeOwned;

/// Parses a document into the input of a kind.
pub fn input<T: DeserializeOwned>(json: &str) -> T {
    serde_json::from_str(json).expect("the document parses")
}

/// RF15: the `groups` of a diagram nest —a member is a node or another
/// group— and reach the IR in the order the document declares them.
pub fn groups_reach_the_ir<K: DiagramKind>(document: &str) -> IrGraph {
    let parsed = input::<K::Input>(document);
    let kind = K::default();
    kind.validate(&parsed)
        .unwrap_or_else(|errors| panic!("un grupo puede contener otro grupo: {errors:?}"));

    let Ir::Graph(graph) = kind.to_ir(&parsed) else {
        panic!("un diagrama de `{}` es un grafo", K::KIND);
    };
    assert_eq!(graph.groups.len(), 2);
    assert_eq!(graph.groups[0].id, "io");
    assert_eq!(graph.groups[0].label.as_deref(), Some("I/O"));
    assert_eq!(graph.groups[0].members, vec!["disk"]);
    assert_eq!(graph.groups[1].id, "disk");
    assert_eq!(graph.groups[1].members, vec!["read"]);
    graph
}

/// A member the document never declares is `unknown_ref` at its own path, and
/// the document does not pass.
pub fn an_unknown_group_member_is_rejected<K: DiagramKind>(document: &str) {
    let parsed = input::<K::Input>(document);
    let errors = K::default()
        .validate(&parsed)
        .expect_err("el miembro no existe");
    assert!(
        errors.iter().any(
            |error| error.code == ErrorCode::UnknownRef && error.path == "groups[0].members[0]"
        ),
        "{errors:?}"
    );
}
