//! El kind `state`: una máquina de estados (PRD §6).
//!
//! Los estados son nodos y las transiciones son aristas: la etiqueta de la
//! arista es el evento que dispara el cambio (`pagar`, `timeout`, `retry`), que
//! es lo que hace legible una máquina. Se dibuja con el motor `layered` y las
//! formas que ya existen: elipse para el estado inicial y el final, caja
//! redondeada para un estado.
//!
//! La regla propia del kind es la de una máquina: **un solo estado inicial**.
//! Dos entradas son dos máquinas dibujadas juntas y el documento tiene que
//! decirlo, no dejar que el dibujo lo esconda.

use gx_core::error::{ErrorCode, ValidationError};
use gx_core::ir::{Direction, Ir, LayoutHint, Shape};
use gx_core::kind::{DiagramKind, KindLimits, Role};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::flow::{to_graph, validate_flow, validate_header, FlowEdge, FlowNode};
use crate::groups::{to_ir_groups, validate_groups, Group};

/// Contract version supported by this build (PRD §10.1).
pub const SUPPORTED_VERSION: u32 = 1;

/// El rol de un nodo que no declara ninguno: un estado.
pub const DEFAULT_ROLE: &str = "state";

/// Los roles de una máquina de estados y su forma.
pub const ROLES: [Role; 3] = [
    Role::new("initial", Shape::Dot),
    Role::new("state", Shape::Rounded),
    Role::new("final", Shape::RingDot),
];

/// The JSON contract of the `state` kind.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StateInput {
    /// Contract version. Only `1` is supported.
    pub version: u32,
    /// Must be `state`.
    pub kind: String,
    /// Human readable title of the diagram.
    #[serde(default)]
    pub title: Option<String>,
    /// Direction of the layout.
    #[serde(default)]
    pub direction: Option<Direction>,
    /// The states, including the initial one and the final ones.
    pub nodes: Vec<FlowNode>,
    /// The transitions, with the event that triggers each one.
    #[serde(default)]
    pub edges: Vec<FlowEdge>,
    /// The groups of nodes, which nest: a member is a node or another group.
    #[serde(default)]
    pub groups: Vec<Group>,
}

/// The `state` kind.
#[derive(Debug, Default)]
pub struct StateKind;

impl DiagramKind for StateKind {
    const KIND: &'static str = "state";
    const PHASE: &'static str = "F2";
    const LAYOUT_HINT: LayoutHint = LayoutHint::Layered;
    const LIMITS: KindLimits = KindLimits::GRAPH;

    type Input = StateInput;

    fn schema() -> schemars::Schema {
        schemars::schema_for!(StateInput)
    }

    fn shapes() -> Vec<Shape> {
        crate::flow::shapes_of_roles(&ROLES)
    }

    fn roles() -> &'static [Role] {
        &ROLES
    }

    fn default_role() -> Option<&'static str> {
        Some(DEFAULT_ROLE)
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

        // Una máquina tiene una sola entrada: si el documento declara dos, son
        // dos máquinas y el dibujo no puede insinuar lo contrario.
        let initials: Vec<usize> = input
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.role.as_deref() == Some("initial"))
            .map(|(index, _)| index)
            .collect();
        if initials.len() > 1 {
            let second = initials[1];
            errors.push(ValidationError::new(
                format!("nodes[{second}].role"),
                ErrorCode::DuplicateId,
                format!(
                    "la máquina declara {} estados iniciales: solo puede haber uno",
                    initials.len()
                ),
                Some(
                    "dejá un `initial` y modelá la otra entrada como un estado más, o partí el diagrama en dos máquinas"
                        .to_string(),
                ),
            ));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn to_ir(&self, input: &Self::Input) -> Ir {
        let graph = to_graph(
            &input.nodes,
            &input.edges,
            &ROLES,
            DEFAULT_ROLE,
            input.direction,
            Self::LAYOUT_HINT,
            to_ir_groups(&input.groups),
        );
        Ir::Graph(graph)
    }

    /// La regla propia de una máquina, en las palabras que el agente lee: dos
    /// estados iniciales son dos máquinas.
    fn rules(&self) -> Vec<String> {
        vec![
            "Una máquina tiene **un solo** estado `initial`: dos entradas son dos máquinas."
                .to_string(),
            "La etiqueta de una transición es el evento que la dispara, por ejemplo `pagar`."
                .to_string(),
        ]
    }

    fn min_example() -> serde_json::Value {
        serde_json::json!({
            "version": 1,
            "kind": "state",
            "title": "pedido",
            "nodes": [
                { "id": "nuevo", "label": "nuevo", "role": "initial" },
                { "id": "pago", "label": "pago pendiente" },
                { "id": "cerrado", "label": "cerrado", "role": "final" }
            ],
            "edges": [
                { "from": "nuevo", "to": "pago", "label": "crear" },
                { "from": "pago", "to": "cerrado", "label": "cancelar" }
            ]
        })
    }
}

/// Registers the kind in the global registry.
pub fn register() {
    gx_core::kind::register_kind::<StateKind>();
}

#[cfg(test)]
mod tests {
    use super::{StateKind, DEFAULT_ROLE, ROLES};
    use crate::test_support::{an_unknown_group_member_is_rejected, groups_reach_the_ir, input};
    use gx_core::error::ErrorCode;
    use gx_core::ir::{Ir, Shape};
    use gx_core::kind::DiagramKind;

    /// RF15: `groups` existe en `state` y anida: un miembro es un nodo o
    /// otro grupo, y el documento no cambia de version.
    #[test]
    fn the_groups_of_a_diagram_reach_the_ir() {
        groups_reach_the_ir::<StateKind>(
            r#"{ "version": 1, "kind": "state",
                 "nodes": [ { "id": "read" } ],
                 "groups": [
                   { "id": "io", "label": "I/O", "members": ["disk"] },
                   { "id": "disk", "members": ["read"] } ] }"#,
        );
    }

    /// Un miembro que no existe se reporta antes de dibujar.
    #[test]
    fn a_group_member_that_does_not_exist_is_rejected() {
        an_unknown_group_member_is_rejected::<StateKind>(
            r#"{ "version": 1, "kind": "state",
                 "nodes": [ { "id": "read" } ],
                 "groups": [ { "id": "io", "members": ["fantasma"] } ] }"#,
        );
    }

    const ORDER: &str = r#"{
      "version": 1, "kind": "state", "title": "pedido",
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

    /// S1: una máquina de estados es un grafo `layered`; el rol decide la
    /// forma y la etiqueta de la transición es el evento que la dispara.
    #[test]
    fn a_state_machine_becomes_a_layered_graph() {
        let parsed = input(ORDER);
        StateKind.validate(&parsed).expect("la máquina es válida");

        let Ir::Graph(graph) = StateKind.to_ir(&parsed) else {
            panic!("una máquina es un grafo");
        };

        assert_eq!(graph.layout_hint, gx_core::ir::LayoutHint::Layered);
        assert_eq!(graph.nodes.len(), 4);
        assert_eq!(graph.edges.len(), 4);

        let shape_of = |id: &str| {
            graph
                .nodes
                .iter()
                .find(|node| node.id == id)
                .unwrap_or_else(|| panic!("falta `{id}`"))
                .shape
        };
        assert_eq!(shape_of("nuevo"), Shape::Dot);
        assert_eq!(shape_of("pago"), Shape::Rounded);
        assert_eq!(shape_of("cerrado"), Shape::RingDot);

        let transition = graph
            .edges
            .iter()
            .find(|edge| edge.from == "pago" && edge.to == "enviado")
            .expect("la transición de pago");
        assert_eq!(transition.label.as_deref(), Some("pagar"));
    }

    /// La regla propia del kind: una máquina tiene una sola entrada.
    #[test]
    fn two_initial_states_are_reported() {
        let parsed = input(
            r#"{ "version": 1, "kind": "state",
                 "nodes": [
                   { "id": "a", "role": "initial" },
                   { "id": "b", "role": "initial" },
                   { "id": "c" } ] }"#,
        );

        let errors = StateKind
            .validate(&parsed)
            .expect_err("dos entradas son dos máquinas");

        assert_eq!(errors[0].code, ErrorCode::DuplicateId);
        assert_eq!(errors[0].path, "nodes[1].role");
        assert!(errors[0].message.contains("2 estados iniciales"));
        assert!(errors[0].hint.is_some());
    }

    #[test]
    fn a_node_without_a_role_is_a_state() {
        let parsed = input(
            r#"{ "version": 1, "kind": "state", "nodes": [ { "id": "a", "label": "idle" } ] }"#,
        );
        StateKind.validate(&parsed).expect("es válida");

        let Ir::Graph(graph) = StateKind.to_ir(&parsed) else {
            panic!("una máquina es un grafo");
        };

        let expected = ROLES
            .iter()
            .find(|role| role.name == DEFAULT_ROLE)
            .expect("el rol por defecto está en la lista")
            .shape;
        assert_eq!(graph.nodes[0].shape, expected);
    }

    #[test]
    fn an_unknown_role_is_reported_with_the_roles_of_the_kind() {
        let parsed = input(
            r#"{ "version": 1, "kind": "state", "nodes": [ { "id": "a", "role": "substate" } ] }"#,
        );

        let errors = StateKind.validate(&parsed).expect_err("el rol no existe");

        assert_eq!(errors[0].code, ErrorCode::InvalidValue);
        assert_eq!(errors[0].path, "nodes[0].role");
        let hint = errors[0].hint.as_deref().expect("dice cuáles hay");
        assert!(hint.contains("initial"), "{hint}");
    }

    #[test]
    fn a_transition_to_an_unknown_state_is_reported() {
        let parsed = input(
            r#"{ "version": 1, "kind": "state",
                 "nodes": [ { "id": "a" } ],
                 "edges": [ { "from": "a", "to": "fantasma", "label": "ir" } ] }"#,
        );

        let errors = StateKind
            .validate(&parsed)
            .expect_err("el destino no existe");

        assert_eq!(errors[0].code, ErrorCode::UnknownRef);
        assert_eq!(errors[0].path, "edges[0].to");
    }

    #[test]
    fn a_machine_without_states_is_reported() {
        let parsed = input(r#"{ "version": 1, "kind": "state", "nodes": [] }"#);

        let errors = StateKind
            .validate(&parsed)
            .expect_err("sin estados no hay máquina");

        assert_eq!(errors[0].code, ErrorCode::MissingField);
        assert_eq!(errors[0].path, "nodes");
    }
}
