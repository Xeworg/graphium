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

use gx_core::error::{ErrorCode, ValidationError};
use gx_core::ir::{Direction, Ir, IrGraph, IrGroup, LayoutHint, Shape, Style};
use gx_core::kind::{DiagramKind, KindLimits};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::flow::{
    to_graph, validate_header, validate_nodes, validate_refs, FlowEdge, FlowNode, Role,
};

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

/// A boundary: the system or subsystem the pieces belong to.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Boundary {
    /// Unique identifier of the boundary inside the diagram.
    pub id: String,
    /// Name the boundary shows. Defaults to the id.
    #[serde(default)]
    pub label: Option<String>,
    /// Ids of the nodes that belong to it.
    pub members: Vec<String>,
}

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
    const LIMITS: KindLimits = KindLimits {
        max_nodes: 300,
        max_edges: 600,
        max_density: 3.0,
        max_lanes: 0,
        max_events: 0,
    };

    type Input = ComponentInput;

    fn schema() -> schemars::Schema {
        schemars::schema_for!(ComponentInput)
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

        let mut seen_groups: Vec<&str> = Vec::new();
        // The boundary each node already belongs to, so a second claim is
        // reported with the name of both.
        let mut owner: Vec<(&str, &str)> = Vec::new();
        for (index, group) in input.groups.iter().enumerate() {
            if group.id.trim().is_empty() {
                errors.push(ValidationError::new(
                    format!("groups[{index}].id"),
                    ErrorCode::InvalidValue,
                    "el id del grupo no puede estar vacío",
                    None,
                ));
            }
            if seen_groups.contains(&group.id.as_str()) {
                errors.push(ValidationError::new(
                    format!("groups[{index}].id"),
                    ErrorCode::DuplicateId,
                    format!("grupo repetido: `{}`", group.id),
                    Some("los ids de grupo deben ser únicos".to_string()),
                ));
            }
            seen_groups.push(group.id.as_str());

            if group.members.is_empty() {
                errors.push(ValidationError::new(
                    format!("groups[{index}].members"),
                    ErrorCode::MissingField,
                    "un grupo necesita al menos un miembro",
                    Some(
                        "agregá los `id` de las piezas que forman la frontera, o quitá el grupo"
                            .to_string(),
                    ),
                ));
            }

            for (member_index, member) in group.members.iter().enumerate() {
                if !input.nodes.iter().any(|node| &node.id == member) {
                    errors.push(ValidationError::new(
                        format!("groups[{index}].members[{member_index}]"),
                        ErrorCode::UnknownRef,
                        format!("pieza desconocida: `{member}`"),
                        Some("los miembros referencian nodos declarados en `nodes`".to_string()),
                    ));
                    continue;
                }
                match owner.iter().find(|(id, _)| id == member) {
                    Some((_, previous)) => errors.push(ValidationError::new(
                        format!("groups[{index}].members[{member_index}]"),
                        ErrorCode::DuplicateId,
                        format!(
                            "`{member}` ya pertenece al grupo `{previous}`: una pieza vive en una sola frontera"
                        ),
                        Some(
                            "dejá la pieza en un grupo, o usá dos diagramas para las dos arquitecturas"
                                .to_string(),
                        ),
                    )),
                    None => owner.push((member.as_str(), group.id.as_str())),
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn to_ir(&self, input: &Self::Input) -> Ir {
        let mut graph: IrGraph = to_graph(
            &input.nodes,
            &input.edges,
            &ROLES,
            DEFAULT_ROLE,
            input.direction,
            Self::LAYOUT_HINT,
        );

        // Lo externo se dibuja punteado: no es de este sistema.
        for node in &mut graph.nodes {
            if let Some(source) = input.nodes.iter().find(|source| source.id == node.id) {
                if source.role.as_deref() == Some(EXTERNAL_ROLE) && source.style.is_none() {
                    node.style = Style::Dashed;
                }
            }
        }

        graph.groups = input
            .groups
            .iter()
            .map(|group| IrGroup {
                id: group.id.clone(),
                label: group
                    .label
                    .clone()
                    .filter(|label| !label.trim().is_empty())
                    .or_else(|| Some(group.id.clone())),
                members: group.members.clone(),
            })
            .collect();

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
            .find(|error| error.message.contains("frontera"))
            .expect("el error de la frontera doble");
        assert_eq!(error.code, ErrorCode::DuplicateId);
        assert_eq!(error.path, "groups[1].members[0]");
        assert!(error.message.contains("backend"), "{}", error.message);
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
