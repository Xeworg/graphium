//! El kind `dataflow`: un pipeline de datos, un ETL, un DFD (PRD §6).
//!
//! Procesos, depósitos y entidades externas, con flujos que llevan el nombre
//! del dato que viaja. Se dibuja con el motor `layered` y las formas que ya
//! existen: caja redondeada para el proceso, cilindro para el depósito, caja
//! para la entidad externa.
//!
//! Las dos reglas propias son las del diagrama de flujo de datos, y las dos
//! existen porque el dibujo, sin ellas, diría algo falso: **un depósito no
//! alimenta a otro depósito** (los datos solo se mueven a través de un
//! proceso) y **una entidad externa no alimenta a otra** (el sistema no genera
//! sus propios insumos).

use gx_core::error::{ErrorCode, ValidationError};
use gx_core::ir::{Direction, Ir, LayoutHint, Shape};
use gx_core::kind::{DiagramKind, KindLimits};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::flow::{to_graph, validate_flow, validate_header, FlowEdge, FlowNode, Role};
use crate::groups::{to_ir_groups, validate_groups, Group};

/// Contract version supported by this build (PRD §10.1).
pub const SUPPORTED_VERSION: u32 = 1;

/// El rol de un nodo que no declara ninguno: un proceso.
pub const DEFAULT_ROLE: &str = "process";

/// Los roles de un dataflow y su forma.
pub const ROLES: [Role; 3] = [
    Role::new("process", Shape::Rounded),
    Role::new("store", Shape::Cylinder),
    Role::new("external", Shape::Box),
];

/// The JSON contract of the `dataflow` kind.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DataflowInput {
    /// Contract version. Only `1` is supported.
    pub version: u32,
    /// Must be `dataflow`.
    pub kind: String,
    /// Human readable title of the diagram.
    #[serde(default)]
    pub title: Option<String>,
    /// Direction of the layout.
    #[serde(default)]
    pub direction: Option<Direction>,
    /// The processes, the stores and the external entities.
    pub nodes: Vec<FlowNode>,
    /// The flows, with the data they carry on the label.
    #[serde(default)]
    pub edges: Vec<FlowEdge>,
    /// The groups of nodes, which nest: a member is a node or another group.
    #[serde(default)]
    pub groups: Vec<Group>,
}

/// The `dataflow` kind.
#[derive(Debug, Default)]
pub struct DataflowKind;

impl DataflowKind {
    /// The role of a node, or the default one.
    fn role_of(node: &FlowNode) -> &str {
        node.role.as_deref().unwrap_or(DEFAULT_ROLE)
    }

    /// The two DFD rules the drawing cannot express on its own.
    fn check_flows(input: &DataflowInput) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let role_of = |id: &str| {
            input
                .nodes
                .iter()
                .find(|node| node.id == id)
                .map(Self::role_of)
        };

        for (index, edge) in input.edges.iter().enumerate() {
            let (Some(from), Some(to)) = (role_of(&edge.from), role_of(&edge.to)) else {
                // An unknown reference is the other check's business.
                continue;
            };
            match (from, to) {
                ("store", "store") => errors.push(ValidationError::new(
                    format!("edges[{index}].from"),
                    ErrorCode::InvalidValue,
                    format!(
                        "un depósito no puede alimentar a otro depósito (`{}` → `{}`)",
                        edge.from, edge.to
                    ),
                    Some(
                        "los datos se mueven a través de un proceso: agregá el proceso que copia de uno al otro"
                            .to_string(),
                    ),
                )),
                ("external", "external") => errors.push(ValidationError::new(
                    format!("edges[{index}].from"),
                    ErrorCode::InvalidValue,
                    format!(
                        "una entidad externa no puede alimentar a otra (`{}` → `{}`)",
                        edge.from, edge.to
                    ),
                    Some(
                        "un flujo entre dos externos pasa por el sistema: agregá el proceso que lo recibe"
                            .to_string(),
                    ),
                )),
                _ => {}
            }
        }

        errors
    }
}

impl DiagramKind for DataflowKind {
    const KIND: &'static str = "dataflow";
    const LAYOUT_HINT: LayoutHint = LayoutHint::Layered;
    const LIMITS: KindLimits = KindLimits {
        max_nodes: 300,
        max_edges: 600,
        max_density: 3.0,
        max_lanes: 0,
        max_events: 0,
    };

    type Input = DataflowInput;

    fn schema() -> schemars::Schema {
        schemars::schema_for!(DataflowInput)
    }

    fn validate(&self, input: &Self::Input) -> Result<(), Vec<ValidationError>> {
        let mut errors = validate_header(Self::KIND, &input.kind, SUPPORTED_VERSION, input.version);
        errors.extend(validate_flow(
            Self::KIND,
            &input.nodes,
            &input.edges,
            &ROLES,
            DEFAULT_ROLE,
        ));
        let node_ids: Vec<&str> = input.nodes.iter().map(|node| node.id.as_str()).collect();
        errors.extend(validate_groups(&node_ids, &input.groups));

        errors.extend(Self::check_flows(input));

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn to_ir(&self, input: &Self::Input) -> Ir {
        let mut graph = to_graph(
            &input.nodes,
            &input.edges,
            &ROLES,
            DEFAULT_ROLE,
            input.direction,
            Self::LAYOUT_HINT,
        );
        graph.groups = to_ir_groups(&input.groups);
        Ir::Graph(graph)
    }
}

/// Registers the kind in the global registry.
pub fn register() {
    gx_core::kind::register_kind::<DataflowKind>();
}

#[cfg(test)]
mod tests {
    use super::{DataflowInput, DataflowKind};
    use gx_core::error::ErrorCode;
    use gx_core::ir::{Ir, Shape};
    use gx_core::kind::DiagramKind;

    fn input(json: &str) -> DataflowInput {
        serde_json::from_str(json).expect("the document parses")
    }

    /// RF15: `groups` existe en `dataflow` y anida: un miembro es un nodo o
    /// otro grupo, y el documento no cambia de version.
    #[test]
    fn the_groups_of_a_diagram_reach_the_ir() {
        let parsed = input(
            r#"{ "version": 1, "kind": "dataflow",
                 "nodes": [ { "id": "read" } ],
                 "groups": [
                   { "id": "io", "label": "I/O", "members": ["disk"] },
                   { "id": "disk", "members": ["read"] } ] }"#,
        );

        DataflowKind
            .validate(&parsed)
            .expect("un grupo puede contener otro grupo");

        let Ir::Graph(graph) = DataflowKind.to_ir(&parsed) else {
            panic!("un diagrama de `dataflow` es un grafo");
        };
        assert_eq!(graph.groups.len(), 2);
        assert_eq!(graph.groups[0].id, "io");
        assert_eq!(graph.groups[0].label.as_deref(), Some("I/O"));
        assert_eq!(graph.groups[0].members, vec!["disk"]);
        assert_eq!(graph.groups[1].members, vec!["read"]);
    }

    /// Un miembro que no existe se reporta antes de dibujar.
    #[test]
    fn a_group_member_that_does_not_exist_is_rejected() {
        let parsed = input(
            r#"{ "version": 1, "kind": "dataflow",
                 "nodes": [ { "id": "read" } ],
                 "groups": [ { "id": "io", "members": ["fantasma"] } ] }"#,
        );

        let errors = DataflowKind
            .validate(&parsed)
            .expect_err("el miembro no existe");
        assert!(
            errors
                .iter()
                .any(|error| error.code == ErrorCode::UnknownRef
                    && error.path == "groups[0].members[0]"),
            "{errors:?}"
        );
    }

    const ETL: &str = r#"{
      "version": 1, "kind": "dataflow", "title": "ETL de pedidos",
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

    /// S1: un dataflow es un grafo `layered`; el rol decide la forma y la
    /// etiqueta del flujo es el dato que viaja.
    #[test]
    fn a_dataflow_becomes_a_layered_graph_with_its_three_shapes() {
        let parsed = input(ETL);
        DataflowKind
            .validate(&parsed)
            .expect("el pipeline es válido");

        let Ir::Graph(graph) = DataflowKind.to_ir(&parsed) else {
            panic!("un dataflow es un grafo");
        };

        assert_eq!(graph.layout_hint, gx_core::ir::LayoutHint::Layered);
        assert_eq!(graph.nodes.len(), 6);
        assert_eq!(graph.edges.len(), 5);

        let shape_of = |id: &str| {
            graph
                .nodes
                .iter()
                .find(|node| node.id == id)
                .unwrap_or_else(|| panic!("falta `{id}`"))
                .shape
        };
        assert_eq!(shape_of("crm"), Shape::Box);
        assert_eq!(shape_of("ingesta"), Shape::Rounded);
        assert_eq!(shape_of("crudo"), Shape::Cylinder);

        let flow = graph
            .edges
            .iter()
            .find(|edge| edge.from == "limpieza")
            .expect("el flujo de salida de la limpieza");
        assert_eq!(flow.label.as_deref(), Some("pedidos curados"));
    }

    /// La primera regla del DFD: los datos no pasan de un depósito a otro sin
    /// un proceso en el medio.
    #[test]
    fn a_flow_between_two_stores_is_reported() {
        let parsed = input(
            r#"{ "version": 1, "kind": "dataflow",
                 "nodes": [
                   { "id": "a", "role": "store" },
                   { "id": "b", "role": "store" } ],
                 "edges": [ { "from": "a", "to": "b", "label": "filas" } ] }"#,
        );

        let errors = DataflowKind
            .validate(&parsed)
            .expect_err("dos depósitos no se hablan");

        assert_eq!(errors[0].code, ErrorCode::InvalidValue);
        assert_eq!(errors[0].path, "edges[0].from");
        assert!(errors[0].message.contains("depósito"), "{:?}", errors[0]);
        assert!(errors[0].hint.as_deref().unwrap_or("").contains("proceso"));
    }

    /// La segunda regla: una entidad externa no alimenta a otra.
    #[test]
    fn a_flow_between_two_externals_is_reported() {
        let parsed = input(
            r#"{ "version": 1, "kind": "dataflow",
                 "nodes": [
                   { "id": "a", "role": "external" },
                   { "id": "b", "role": "external" } ],
                 "edges": [ { "from": "a", "to": "b" } ] }"#,
        );

        let errors = DataflowKind
            .validate(&parsed)
            .expect_err("dos externos no se hablan");

        assert_eq!(errors[0].code, ErrorCode::InvalidValue);
        assert_eq!(errors[0].path, "edges[0].from");
        assert!(
            errors[0].message.contains("entidad externa"),
            "{:?}",
            errors[0]
        );
    }

    /// Un flujo que pasa por un proceso es legal, aunque el depósito y el
    /// externo estén en los extremos.
    #[test]
    fn a_flow_through_a_process_is_accepted() {
        let parsed = input(
            r#"{ "version": 1, "kind": "dataflow",
                 "nodes": [
                   { "id": "a", "role": "store" },
                   { "id": "p" },
                   { "id": "b", "role": "store" } ],
                 "edges": [
                   { "from": "a", "to": "p", "label": "filas" },
                   { "from": "p", "to": "b", "label": "filas" } ] }"#,
        );

        DataflowKind
            .validate(&parsed)
            .expect("el proceso copia de un depósito al otro");
    }

    #[test]
    fn an_unknown_role_is_reported_with_the_roles_of_the_kind() {
        let parsed = input(
            r#"{ "version": 1, "kind": "dataflow",
                 "nodes": [ { "id": "a", "role": "queue" } ] }"#,
        );

        let errors = DataflowKind
            .validate(&parsed)
            .expect_err("el rol no existe");

        assert_eq!(errors[0].code, ErrorCode::InvalidValue);
        assert_eq!(errors[0].path, "nodes[0].role");
        let hint = errors[0].hint.as_deref().expect("dice cuáles hay");
        assert!(hint.contains("store"), "{hint}");
    }

    #[test]
    fn a_pipeline_without_nodes_is_reported() {
        let parsed = input(r#"{ "version": 1, "kind": "dataflow", "nodes": [] }"#);

        let errors = DataflowKind
            .validate(&parsed)
            .expect_err("sin nodos no hay pipeline");

        assert_eq!(errors[0].code, ErrorCode::MissingField);
        assert_eq!(errors[0].path, "nodes");
    }
}
