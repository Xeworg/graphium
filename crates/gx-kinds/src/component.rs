//! El kind `component`: arquitectura de sistemas al estilo C4 (PRD §6).
//!
//! Contenedores, componentes, bases de datos y sistemas externos, agrupados en
//! **fronteras**: el grupo es el sistema o el subsistema al que pertenecen las
//! piezas, y es lo que convierte una nube de cajas en una arquitectura. Es el
//! primer kind que usa los grupos de la IR, así que el renderer dibuja el
//! contenedor y su nombre (PRD §6.1).
//!
//! La regla propia del kind: **una pieza pertenece a una sola frontera**. Si el
//! documento la pone en dos, son dos arquitecturas superpuestas y el dibujo no
//! puede decir cuál vale.

use gx_core::error::ValidationError;
use gx_core::ir::{Direction, Ir, LayoutHint, Shape, Style};
use gx_core::kind::{DiagramKind, KindLimits};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::flow::{
    to_graph, validate_header, validate_nodes, validate_refs, FlowEdge, FlowNode, Role,
};
use crate::groups::{to_ir_groups, validate_groups};

/// Contract version supported by this build (PRD §10.1).
pub const SUPPORTED_VERSION: u32 = 1;

/// El rol de un nodo que no declara ninguno: un componente.
pub const DEFAULT_ROLE: &str = "component";

/// Los roles de un diagrama de componentes y su forma.
pub const ROLES: [Role; 4] = [
    Role::new("container", Shape::Box),
    Role::new("component", Shape::Rounded),
    Role::new("database", Shape::Cylinder),
    Role::new("external", Shape::Box),
];

/// El rol que se dibuja punteado: lo que no es de este sistema.
pub const EXTERNAL_ROLE: &str = "external";

/// A boundary: the system or subsystem the pieces belong to. It is the shared
/// group of PRD §7 RF15, so a boundary nests inside another one like any other
/// group.
pub use crate::groups::Group as Boundary;

/// The JSON contract of the `component` kind.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComponentInput {
    /// Contract version. Only `1` is supported.
    pub version: u32,
    /// Must be `component`.
    pub kind: String,
    /// Human readable title of the diagram.
    #[serde(default)]
    pub title: Option<String>,
    /// Direction of the layout.
    #[serde(default)]
    pub direction: Option<Direction>,
    /// The containers, components, databases and external systems.
    pub nodes: Vec<FlowNode>,
    /// The boundaries, with the pieces that belong to each one.
    #[serde(default)]
    pub groups: Vec<Boundary>,
    /// The calls between pieces, with the technology on the label when it
    /// matters.
    #[serde(default)]
    pub edges: Vec<FlowEdge>,
}

/// The `component` kind.
#[derive(Debug, Default)]
pub struct ComponentKind;

impl DiagramKind for ComponentKind {
    const KIND: &'static str = "component";
    const LAYOUT_HINT: LayoutHint = LayoutHint::Layered;
    const LIMITS: KindLimits = KindLimits::GRAPH;

    type Input = ComponentInput;

    fn schema() -> schemars::Schema {
        schemars::schema_for!(ComponentInput)
    }

    fn shapes() -> Vec<Shape> {
        crate::flow::shapes_of_roles(&ROLES)
    }

    fn validate(&self, input: &Self::Input) -> Result<(), Vec<ValidationError>> {
        let mut errors = validate_header(Self::KIND, &input.kind, SUPPORTED_VERSION, input.version);
        errors.extend(validate_nodes(
            Self::KIND,
            &input.nodes,
            &ROLES,
            DEFAULT_ROLE,
        ));
        let pairs: Vec<(&str, &str)> = input
            .edges
            .iter()
            .map(|edge| (edge.from.as_str(), edge.to.as_str()))
            .collect();
        errors.extend(validate_refs(&pairs, &input.nodes));

        let node_ids: Vec<&str> = input.nodes.iter().map(|node| node.id.as_str()).collect();
        errors.extend(validate_groups(&node_ids, &input.groups));

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
            to_ir_groups(&input.groups),
        );

        // Lo externo se dibuja punteado: no es de este sistema.
        for node in &mut graph.nodes {
            if let Some(source) = input.nodes.iter().find(|source| source.id == node.id) {
                if source.role.as_deref() == Some(EXTERNAL_ROLE) && source.style.is_none() {
                    node.style = Style::Dashed;
                }
            }
        }

        Ir::Graph(graph)
    }
}

/// Registers the kind in the global registry.
pub fn register() {
    gx_core::kind::register_kind::<ComponentKind>();
}

#[cfg(test)]
mod tests {
    use super::{ComponentInput, ComponentKind, EXTERNAL_ROLE, ROLES};
    use gx_core::error::ErrorCode;
    use gx_core::ir::{Ir, Shape, Style};
    use gx_core::kind::DiagramKind;

    fn input(json: &str) -> ComponentInput {
        serde_json::from_str(json).expect("the document parses")
    }

    const API: &str = r#"{
      "version": 1, "kind": "component", "title": "API de pedidos",
      "nodes": [
        { "id": "web", "label": "Web", "role": "container" },
        { "id": "api", "label": "API" },
        { "id": "db", "label": "Postgres", "role": "database" },
        { "id": "pagos", "label": "Pagos", "role": "external" }
      ],
      "groups": [
        { "id": "backend", "label": "Backend", "members": ["api", "db"] }
      ],
      "edges": [
        { "from": "web", "to": "api", "label": "HTTPS" },
        { "from": "api", "to": "db", "label": "SQL" },
        { "from": "api", "to": "pagos", "label": "cobro" }
      ]
    }"#;

    /// S1: el diagrama de componentes es un grafo `layered` con sus fronteras
    /// como grupos de la IR, y lo externo se dibuja punteado.
    #[test]
    fn a_component_diagram_becomes_a_layered_graph_with_its_boundaries() {
        let parsed = input(API);
        ComponentKind
            .validate(&parsed)
            .expect("la arquitectura es válida");

        let Ir::Graph(graph) = ComponentKind.to_ir(&parsed) else {
            panic!("un diagrama de componentes es un grafo");
        };

        assert_eq!(graph.layout_hint, gx_core::ir::LayoutHint::Layered);
        assert_eq!(graph.nodes.len(), 4);
        assert_eq!(graph.edges.len(), 3);
        assert_eq!(graph.groups.len(), 1);
        assert_eq!(graph.groups[0].id, "backend");
        assert_eq!(graph.groups[0].label.as_deref(), Some("Backend"));
        assert_eq!(graph.groups[0].members, vec!["api", "db"]);

        let shape_of = |id: &str| {
            graph
                .nodes
                .iter()
                .find(|node| node.id == id)
                .unwrap_or_else(|| panic!("falta `{id}`"))
                .shape
        };
        assert_eq!(shape_of("web"), Shape::Box);
        assert_eq!(shape_of("api"), Shape::Rounded);
        assert_eq!(shape_of("db"), Shape::Cylinder);

        let external = graph
            .nodes
            .iter()
            .find(|node| node.id == "pagos")
            .expect("el externo");
        assert_eq!(external.style, Style::Dashed);
        assert_eq!(EXTERNAL_ROLE, "external");
    }

    /// La regla del kind: una pieza vive en una sola frontera.
    #[test]
    fn a_piece_in_two_boundaries_is_reported() {
        let parsed = input(
            r#"{ "version": 1, "kind": "component",
                 "nodes": [ { "id": "api" } ],
                 "groups": [
                   { "id": "backend", "label": "Backend", "members": ["api"] },
                   { "id": "frontend", "label": "Frontend", "members": ["api"] } ] }"#,
        );

        let errors = ComponentKind
            .validate(&parsed)
            .expect_err("una pieza no vive en dos fronteras");

        let error = errors
            .iter()
            .find(|error| error.message.contains("backend"))
            .expect("el error del grupo doble");
        assert_eq!(error.code, ErrorCode::DuplicateId);
        assert_eq!(error.path, "groups[1].members[0]");
        assert!(error.message.contains("un solo grupo"), "{}", error.message);
    }

    #[test]
    fn a_member_that_is_not_a_declared_node_is_reported() {
        let parsed = input(
            r#"{ "version": 1, "kind": "component",
                 "nodes": [ { "id": "api" } ],
                 "groups": [ { "id": "backend", "members": ["api", "fantasma"] } ] }"#,
        );

        let errors = ComponentKind
            .validate(&parsed)
            .expect_err("el miembro no existe");

        assert!(errors.iter().any(
            |error| error.path == "groups[0].members[1]" && error.code == ErrorCode::UnknownRef
        ));
    }

    #[test]
    fn an_empty_boundary_or_a_repeated_one_is_reported() {
        let parsed = input(
            r#"{ "version": 1, "kind": "component",
                 "nodes": [ { "id": "api" } ],
                 "groups": [
                   { "id": "backend", "members": [] },
                   { "id": "backend", "members": ["api"] } ] }"#,
        );

        let errors = ComponentKind
            .validate(&parsed)
            .expect_err("el grupo vacío y el repetido no pasan");

        assert!(errors.iter().any(
            |error| error.code == ErrorCode::MissingField && error.path == "groups[0].members"
        ));
        assert!(errors
            .iter()
            .any(|error| error.code == ErrorCode::DuplicateId && error.path == "groups[1].id"));
    }

    /// RF15: un grupo puede contener otro grupo, y el anidamiento llega a la IR
    /// con los miembros tal como el documento los declara.
    #[test]
    fn a_boundary_inside_a_boundary_is_accepted() {
        let parsed = input(
            r#"{ "version": 1, "kind": "component",
                 "nodes": [ { "id": "api" } ],
                 "groups": [
                   { "id": "sys", "label": "Sistema", "members": ["backend"] },
                   { "id": "backend", "label": "Backend", "members": ["api"] } ] }"#,
        );

        ComponentKind
            .validate(&parsed)
            .expect("un grupo puede contener otro grupo");

        let Ir::Graph(graph) = ComponentKind.to_ir(&parsed) else {
            panic!("un diagrama de componentes es un grafo");
        };
        assert_eq!(graph.groups.len(), 2);
        assert_eq!(graph.groups[0].members, vec!["backend"]);
        assert_eq!(graph.groups[1].members, vec!["api"]);
    }

    /// Un grupo no se contiene a sí mismo: el layout no puede cerrar el ciclo.
    #[test]
    fn a_boundary_that_contains_itself_is_reported() {
        let parsed = input(
            r#"{ "version": 1, "kind": "component",
                 "nodes": [ { "id": "api" } ],
                 "groups": [ { "id": "sys", "members": ["sys"] } ] }"#,
        );

        let errors = ComponentKind
            .validate(&parsed)
            .expect_err("un grupo no se contiene a sí mismo");

        assert!(
            errors
                .iter()
                .any(|error| error.code == ErrorCode::InvalidValue
                    && error.path == "groups[0].members[0]"),
            "{errors:?}"
        );
    }

    /// Dos grupos que se contienen mutuamente son el mismo ciclo, un paso más
    /// largo.
    #[test]
    fn a_boundary_cycle_is_reported() {
        let parsed = input(
            r#"{ "version": 1, "kind": "component",
                 "nodes": [ { "id": "api" } ],
                 "groups": [
                   { "id": "a", "members": ["b"] },
                   { "id": "b", "members": ["a"] } ] }"#,
        );

        let errors = ComponentKind
            .validate(&parsed)
            .expect_err("dos grupos no se contienen mutuamente");

        assert!(
            errors
                .iter()
                .any(|error| error.code == ErrorCode::InvalidValue),
            "{errors:?}"
        );
    }

    /// El id de un grupo y el de una pieza viven en el mismo espacio de nombres
    /// del motor, así que no pueden coincidir.
    #[test]
    fn a_boundary_with_the_id_of_a_piece_is_reported() {
        let parsed = input(
            r#"{ "version": 1, "kind": "component",
                 "nodes": [ { "id": "api" } ],
                 "groups": [ { "id": "api", "members": ["api"] } ] }"#,
        );

        let errors = ComponentKind
            .validate(&parsed)
            .expect_err("un grupo no toma el id de una pieza");

        assert!(
            errors
                .iter()
                .any(|error| error.code == ErrorCode::DuplicateId && error.path == "groups[0].id"),
            "{errors:?}"
        );
    }

    /// Un grupo tiene un solo padre: si dos lo reclaman, el dibujo no puede
    /// elegir cuál vale.
    #[test]
    fn a_boundary_with_two_parents_is_reported() {
        let parsed = input(
            r#"{ "version": 1, "kind": "component",
                 "nodes": [ { "id": "api" } ],
                 "groups": [
                   { "id": "inner", "members": ["api"] },
                   { "id": "left", "members": ["inner"] },
                   { "id": "right", "members": ["inner"] } ] }"#,
        );

        let errors = ComponentKind
            .validate(&parsed)
            .expect_err("un grupo no vive en dos padres");

        assert!(
            errors
                .iter()
                .any(|error| error.code == ErrorCode::DuplicateId
                    && error.path == "groups[2].members[0]"),
            "{errors:?}"
        );
    }

    #[test]
    fn an_unknown_role_is_reported_with_the_roles_of_the_kind() {
        let parsed = input(
            r#"{ "version": 1, "kind": "component",
                 "nodes": [ { "id": "api", "role": "microservice" } ] }"#,
        );

        let errors = ComponentKind
            .validate(&parsed)
            .expect_err("el rol no existe");

        assert_eq!(errors[0].code, ErrorCode::InvalidValue);
        assert_eq!(errors[0].path, "nodes[0].role");
        let hint = errors[0].hint.as_deref().expect("dice cuáles hay");
        assert!(hint.contains("database"), "{hint}");
        assert_eq!(ROLES.len(), 4);
    }

    #[test]
    fn an_edge_to_an_unknown_piece_is_reported() {
        let parsed = input(
            r#"{ "version": 1, "kind": "component",
                 "nodes": [ { "id": "api" } ],
                 "edges": [ { "from": "api", "to": "fantasma" } ] }"#,
        );

        let errors = ComponentKind
            .validate(&parsed)
            .expect_err("el destino no existe");

        assert_eq!(errors[0].code, ErrorCode::UnknownRef);
        assert_eq!(errors[0].path, "edges[0].to");
    }
}
