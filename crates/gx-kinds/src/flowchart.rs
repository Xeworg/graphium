//! El kind `flowchart`: un proceso con decisiones (PRD §6).
//!
//! Se dibuja con el motor `layered` y las formas que ya existen: el `role` de
//! cada nodo decide cuál —círculo para el arranque y el final, rombo para la
//! decisión, caja para un dato— y la etiqueta de la arista dice por qué camino
//! se va (`sí`, `no`). Agregar un kind no toca el motor ni el renderer (O7).

use gx_core::error::{ErrorCode, ValidationError};
use gx_core::ir::{Direction, Ir, LayoutHint, Shape};
use gx_core::kind::{DiagramKind, KindLimits, Role};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::flow::{to_graph, validate_flow, validate_header, FlowEdge, FlowNode};
use crate::groups::{to_ir_groups, validate_groups, Group};

/// Contract version supported by this build (PRD §10.1).
pub const SUPPORTED_VERSION: u32 = 1;

/// El rol de un nodo que no declara ninguno: un paso.
pub const DEFAULT_ROLE: &str = "process";

/// El rol de la única entrada del diagrama (`f20-reuso.md`, S8): exactamente
/// un nodo lo declara, como `initial` en `state`.
pub const START_ROLE: &str = "start";

/// Los roles de un flowchart y la forma con la que se dibuja cada uno.
pub const ROLES: [Role; 6] = [
    Role::new("start", Shape::Ellipse),
    Role::new("end", Shape::Ellipse),
    Role::new("process", Shape::Rounded),
    Role::new("decision", Shape::Diamond),
    Role::new("data", Shape::Box),
    Role::new("note", Shape::Note),
];

/// The JSON contract of the `flowchart` kind.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FlowchartInput {
    /// Contract version. Only `1` is supported.
    pub version: u32,
    /// Must be `flowchart`.
    pub kind: String,
    /// Human readable title of the diagram.
    #[serde(default)]
    pub title: Option<String>,
    /// Direction of the layout.
    #[serde(default)]
    pub direction: Option<Direction>,
    /// The steps and the decisions.
    pub nodes: Vec<FlowNode>,
    /// The arrows that join them, with the condition they carry.
    #[serde(default)]
    pub edges: Vec<FlowEdge>,
    /// The groups of nodes, which nest: a member is a node or another group.
    #[serde(default)]
    pub groups: Vec<Group>,
}

/// The `flowchart` kind.
#[derive(Debug, Default)]
pub struct FlowchartKind;

impl DiagramKind for FlowchartKind {
    const KIND: &'static str = "flowchart";
    const PHASE: &'static str = "F2";
    const LAYOUT_HINT: LayoutHint = LayoutHint::Layered;
    const LIMITS: KindLimits = KindLimits::GRAPH;

    type Input = FlowchartInput;

    fn schema() -> schemars::Schema {
        schemars::schema_for!(FlowchartInput)
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

        // Exactly one way in (`f20-reuso.md`, S8): the rule `state` enforces
        // for `initial`, now shared. A drawing without a start has no way in;
        // two starts are two ways into one drawing.
        let starts: Vec<&str> = input
            .nodes
            .iter()
            .filter(|node| node.role.as_deref().unwrap_or(DEFAULT_ROLE) == START_ROLE)
            .map(|node| node.id.as_str())
            .collect();
        match starts.len() {
            0 => errors.push(ValidationError::new(
                "nodes",
                ErrorCode::MissingField,
                "un diagrama de `flowchart` necesita exactamente un nodo con rol `start`",
                Some(format!(
                    "declará uno, por ejemplo `{{ \"id\": \"inicio\", \"role\": \"{START_ROLE}\" }}`"
                )),
            )),
            1 => {}
            _ => errors.push(ValidationError::new(
                "nodes",
                ErrorCode::InvalidValue,
                format!(
                    "hay {} nodos con rol `{START_ROLE}`: exactamente uno",
                    starts.len()
                ),
                Some("quitá todos menos uno, o bajá los demás a `process`".to_string()),
            )),
        }

        let node_ids: Vec<&str> = input.nodes.iter().map(|node| node.id.as_str()).collect();
        errors.extend(validate_groups(&node_ids, &input.groups));

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

    /// S8 de `f20-reuso.md`: la regla del arranque es contrato, no estilo, y el
    /// agente la lee antes de escribir el documento.
    fn rules(&self) -> Vec<String> {
        vec![
            "Un diagrama de `flowchart` declara **exactamente un** nodo con `role: \"start\"`: un dibujo sin entrada no tiene por dónde empezar y dos son dos flujos."
                .to_string(),
            "La etiqueta de una arista es la condición del camino, por ejemplo `sí` / `no`."
                .to_string(),
        ]
    }

    fn min_example() -> serde_json::Value {
        serde_json::json!({
            "version": 1,
            "kind": "flowchart",
            "title": "aprobación",
            "nodes": [
                { "id": "inicio", "label": "pedido", "role": "start" },
                { "id": "limite", "label": "¿supera el límite?", "role": "decision" },
                { "id": "fin", "label": "fin", "role": "end" }
            ],
            "edges": [
                { "from": "inicio", "to": "limite" },
                { "from": "limite", "to": "fin", "label": "sí" }
            ]
        })
    }
}

/// Registers the kind in the global registry.
pub fn register() {
    gx_core::kind::register_kind::<FlowchartKind>();
}

#[cfg(test)]
mod tests {
    use super::{FlowchartKind, DEFAULT_ROLE, ROLES};
    use crate::test_support::{an_unknown_group_member_is_rejected, groups_reach_the_ir, input};
    use gx_core::error::ErrorCode;
    use gx_core::ir::{Ir, Shape};
    use gx_core::kind::DiagramKind;

    /// RF15: `groups` existe en `flowchart` y anida: un miembro es un nodo o
    /// otro grupo, y el documento no cambia de version.
    #[test]
    fn the_groups_of_a_diagram_reach_the_ir() {
        groups_reach_the_ir::<FlowchartKind>(
            r#"{ "version": 1, "kind": "flowchart",
                 "nodes": [ { "id": "iniciar", "role": "start" }, { "id": "read" } ],
                 "edges": [ { "from": "iniciar", "to": "read" } ],
                 "groups": [
                   { "id": "io", "label": "I/O", "members": ["disk"] },
                   { "id": "disk", "members": ["read"] } ] }"#,
        );
    }

    /// Un miembro que no existe se reporta antes de dibujar.
    #[test]
    fn a_group_member_that_does_not_exist_is_rejected() {
        an_unknown_group_member_is_rejected::<FlowchartKind>(
            r#"{ "version": 1, "kind": "flowchart",
                 "nodes": [ { "id": "read" } ],
                 "groups": [ { "id": "io", "members": ["fantasma"] } ] }"#,
        );
    }

    const APPROVAL: &str = r#"{
      "version": 1, "kind": "flowchart", "title": "aprobación de un gasto",
      "nodes": [
        { "id": "inicio", "label": "pedido", "role": "start" },
        { "id": "monto", "role": "process" },
        { "id": "limite", "label": "¿supera el límite?", "role": "decision" },
        { "id": "jefe", "label": "aprueba el jefe", "role": "process" },
        { "id": "auto", "label": "aprobado", "role": "process" },
        { "id": "fin", "role": "end" }
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

    /// S1: un flowchart es un grafo con el motor `layered` y el rol de cada
    /// nodo decide su forma.
    #[test]
    fn a_flowchart_becomes_a_layered_graph_with_a_shape_per_role() {
        let parsed = input(APPROVAL);
        FlowchartKind
            .validate(&parsed)
            .expect("el flowchart es válido");

        let Ir::Graph(graph) = FlowchartKind.to_ir(&parsed) else {
            panic!("un flowchart es un grafo");
        };

        assert_eq!(graph.layout_hint, gx_core::ir::LayoutHint::Layered);
        assert_eq!(graph.nodes.len(), 6);
        assert_eq!(graph.edges.len(), 6);

        let shape_of = |id: &str| {
            graph
                .nodes
                .iter()
                .find(|node| node.id == id)
                .unwrap_or_else(|| panic!("falta `{id}`"))
                .shape
        };
        assert_eq!(shape_of("inicio"), Shape::Ellipse);
        assert_eq!(shape_of("limite"), Shape::Diamond);
        assert_eq!(shape_of("monto"), Shape::Rounded);
        assert_eq!(shape_of("fin"), Shape::Ellipse);

        // El camino de la decisión se lee en la etiqueta de la arista.
        let branch = graph
            .edges
            .iter()
            .find(|edge| edge.to == "jefe")
            .expect("la rama del sí");
        assert_eq!(branch.label.as_deref(), Some("sí"));
    }

    /// El rol es opcional: sin rol, el nodo es un paso.
    #[test]
    fn a_node_without_a_role_is_a_process() {
        let parsed = input(
            r#"{ "version": 1, "kind": "flowchart", "nodes": [ { "id": "inicio", "role": "start" }, { "id": "a" } ] }"#,
        );
        FlowchartKind.validate(&parsed).expect("es válido");

        let Ir::Graph(graph) = FlowchartKind.to_ir(&parsed) else {
            panic!("un flowchart es un grafo");
        };

        let expected = ROLES
            .iter()
            .find(|role| role.name == DEFAULT_ROLE)
            .expect("el rol por defecto está en la lista")
            .shape;
        let without_role = graph
            .nodes
            .iter()
            .find(|node| node.id == "a")
            .expect("el nodo sin rol está");
        assert_eq!(without_role.shape, expected);
        // Y la etiqueta cae al id cuando no hay ninguna.
        assert_eq!(without_role.label, "a");
    }

    #[test]
    fn an_unknown_role_is_reported_with_the_roles_of_the_kind() {
        let parsed = input(
            r#"{ "version": 1, "kind": "flowchart",
                 "nodes": [ { "id": "a", "role": "hexagon" } ] }"#,
        );

        let errors = FlowchartKind
            .validate(&parsed)
            .expect_err("el rol no existe");

        assert_eq!(errors[0].code, ErrorCode::InvalidValue);
        assert_eq!(errors[0].path, "nodes[0].role");
        let hint = errors[0].hint.as_deref().expect("dice cuáles hay");
        assert!(hint.contains("decision"), "{hint}");
    }

    #[test]
    fn a_flowchart_without_nodes_is_reported() {
        let parsed = input(r#"{ "version": 1, "kind": "flowchart", "nodes": [] }"#);

        let errors = FlowchartKind
            .validate(&parsed)
            .expect_err("sin nodos no hay diagrama");

        assert_eq!(errors[0].code, ErrorCode::MissingField);
        assert_eq!(errors[0].path, "nodes");
    }

    #[test]
    fn an_edge_to_an_unknown_node_is_reported() {
        let parsed = input(
            r#"{ "version": 1, "kind": "flowchart",
                 "nodes": [ { "id": "a" } ],
                 "edges": [ { "from": "a", "to": "fantasma" } ] }"#,
        );

        let errors = FlowchartKind
            .validate(&parsed)
            .expect_err("el destino no existe");

        assert_eq!(errors[0].code, ErrorCode::UnknownRef);
        assert_eq!(errors[0].path, "edges[0].to");
    }

    #[test]
    fn the_wrong_kind_and_version_are_reported() {
        let parsed = input(r#"{ "version": 2, "kind": "state", "nodes": [ { "id": "a" } ] }"#);

        let errors = FlowchartKind
            .validate(&parsed)
            .expect_err("no es un flowchart");

        assert_eq!(errors[0].code, ErrorCode::InvalidValue);
        assert_eq!(errors[0].path, "kind");
        assert_eq!(errors[1].path, "version");
    }
}

#[cfg(test)]
mod start_tests {
    use super::FlowchartKind;
    use crate::test_support::input;
    use gx_core::error::ErrorCode;
    use gx_core::kind::DiagramKind;

    /// S8 (`f20-reuso.md`): a flowchart declares exactly one `start` — the
    /// rule `state` already enforces for `initial`. A document without one is
    /// rejected, with the hint naming the rule.
    #[test]
    fn a_flowchart_without_a_start_is_rejected() {
        let document = input(
            r#"{ "version": 1, "kind": "flowchart",
                 "nodes": [ { "id": "paso", "role": "process" },
                            { "id": "fin", "role": "end" } ],
                 "edges": [ { "from": "paso", "to": "fin" } ] }"#,
        );

        let errors = FlowchartKind
            .validate(&document)
            .expect_err("no start: the drawing has no way in");
        assert!(
            errors
                .iter()
                .any(|error| error.code == ErrorCode::MissingField
                    && error.message.contains("start")),
            "{errors:?}"
        );
    }

    /// Two starts are two ways into one drawing: rejected.
    #[test]
    fn a_flowchart_with_two_starts_is_rejected() {
        let document = input(
            r#"{ "version": 1, "kind": "flowchart",
                 "nodes": [ { "id": "a", "role": "start" },
                            { "id": "b", "role": "start" } ],
                 "edges": [] }"#,
        );

        let errors = FlowchartKind
            .validate(&document)
            .expect_err("two starts: which one is the way in?");
        assert!(
            errors
                .iter()
                .any(|error| error.code == ErrorCode::InvalidValue),
            "{errors:?}"
        );
    }

    /// One start validates, and the approval document of the suite still does.
    #[test]
    fn a_flowchart_with_exactly_one_start_is_valid() {
        let document = input(
            r#"{ "version": 1, "kind": "flowchart",
                 "nodes": [ { "id": "inicio", "role": "start" },
                            { "id": "fin", "role": "end" } ],
                 "edges": [ { "from": "inicio", "to": "fin" } ] }"#,
        );

        FlowchartKind
            .validate(&document)
            .expect("one start is the rule");
    }
}
