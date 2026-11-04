//! El kind `class`: clases, interfaces y sus relaciones (PRD §6).
//!
//! Los cuatro roles de nodo se dibujan con las formas que ya existen y la
//! diferencia está en la **relación**: `extends` e `implements` son relaciones
//! *es un*, y `composition` y `dependency` no. El estilo de la línea lo dice
//! —sólida para lo que existe, punteada para lo que depende— y cuando el
//! documento no escribe etiqueta, la arista se nombra con su relación: sin eso
//! dos líneas iguales dirían cosas distintas y nadie sabría cuál es cuál.
//!
//! La regla propia del kind: **la herencia no puede tener ciclos**. Una clase
//! que se extiende a sí misma, directa o por un rodeo, no es un diagrama que
//! esté mal dibujado: es un modelo imposible.

use gx_core::error::{ErrorCode, ValidationError};
use gx_core::ir::{Direction, Emphasis, Ir, IrEdge, IrGraph, LayoutHint, Shape, Style};
use gx_core::kind::{DiagramKind, KindLimits};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::flow::{to_ir_nodes, validate_header, validate_nodes, validate_refs, FlowNode, Role};
use crate::groups::{to_ir_groups, validate_groups, Group};

/// Contract version supported by this build (PRD §10.1).
pub const SUPPORTED_VERSION: u32 = 1;

/// El rol de un nodo que no declara ninguno: una clase.
pub const DEFAULT_ROLE: &str = "class";

/// Los roles de un diagrama de clases y su forma.
pub const ROLES: [Role; 4] = [
    Role::new("class", Shape::Rounded),
    Role::new("abstract", Shape::Rounded),
    Role::new("interface", Shape::Rounded),
    Role::new("enum", Shape::Box),
];

/// Una relación admitida: cómo se dibuja la línea, si es *un* (is-a) y qué
/// palabra la nombra cuando el documento no escribe etiqueta.
#[derive(Debug, Clone, Copy)]
pub struct Relation {
    /// The name the document writes, e.g. `extends`.
    pub name: &'static str,
    /// Line style of the relation.
    pub style: Style,
    /// Whether the line carries an arrowhead.
    pub arrow: bool,
    /// Whether the relation is an "is a": those are the ones that cannot form
    /// a cycle.
    pub is_a: bool,
    /// The word the line shows when the document gives no label. `None` is for
    /// the relation that needs none: a plain arrow says what it says.
    pub label: Option<&'static str>,
}

impl Relation {
    const fn new(
        name: &'static str,
        style: Style,
        arrow: bool,
        is_a: bool,
        label: Option<&'static str>,
    ) -> Self {
        Self {
            name,
            style,
            arrow,
            is_a,
            label,
        }
    }
}

/// Las relaciones de un diagrama de clases.
pub const RELATIONS: [Relation; 5] = [
    // La relación que no dice nada más que "estos dos se conocen": es el
    // default, porque una línea sin palabras no puede afirmar una herencia.
    Relation::new("association", Style::Solid, true, false, None),
    Relation::new("extends", Style::Solid, true, true, Some("extends")),
    Relation::new("implements", Style::Dashed, true, true, Some("implements")),
    Relation::new(
        "composition",
        Style::Solid,
        false,
        false,
        Some("composition"),
    ),
    Relation::new("dependency", Style::Dashed, true, false, Some("dependency")),
];

/// The default relation: the one that claims the least.
pub const DEFAULT_RELATION: &str = "association";

/// A relation between two nodes of the class diagram.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassEdge {
    /// Id of the source node.
    pub from: String,
    /// Id of the target node.
    pub to: String,
    /// What the line means: `association`, `extends`, `implements`,
    /// `composition` or `dependency`. Defaults to `association`.
    #[serde(default)]
    pub relation: Option<String>,
    /// Text on the line. When absent, the relation names itself.
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub emphasis: Option<Emphasis>,
    /// Overrides the style the relation gives the line.
    #[serde(default)]
    pub style: Option<Style>,
    /// Overrides the arrowhead the relation gives the line.
    #[serde(default)]
    pub arrow: Option<bool>,
}

/// The JSON contract of the `class` kind.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassInput {
    /// Contract version. Only `1` is supported.
    pub version: u32,
    /// Must be `class`.
    pub kind: String,
    /// Human readable title of the diagram.
    #[serde(default)]
    pub title: Option<String>,
    /// Direction of the layout.
    #[serde(default)]
    pub direction: Option<Direction>,
    /// The classes, interfaces, abstract classes and enums.
    pub nodes: Vec<FlowNode>,
    /// The relations between them.
    #[serde(default)]
    pub edges: Vec<ClassEdge>,
    /// The groups of nodes, which nest: a member is a class or another group.
    #[serde(default)]
    pub groups: Vec<Group>,
}

/// The `class` kind.
#[derive(Debug, Default)]
pub struct ClassKind;

impl ClassKind {
    /// The relation of an edge, or the default one when it declares none.
    fn relation_of(edge: &ClassEdge) -> &str {
        edge.relation.as_deref().unwrap_or(DEFAULT_RELATION)
    }

    /// The `is a` relations that form a cycle, if any.
    fn inheritance_cycle(input: &ClassInput) -> Option<(usize, Vec<String>)> {
        // `from extends to`: from is the subclass, to is its parent.
        let mut parents: Vec<(&str, Vec<(usize, &str)>)> = Vec::new();
        for (index, edge) in input.edges.iter().enumerate() {
            let relation = Self::relation_of(edge);
            let is_a = RELATIONS
                .iter()
                .any(|known| known.name == relation && known.is_a);
            if !is_a {
                continue;
            }
            match parents.iter_mut().find(|(id, _)| *id == edge.from) {
                Some((_, list)) => list.push((index, edge.to.as_str())),
                None => parents.push((edge.from.as_str(), vec![(index, edge.to.as_str())])),
            }
        }

        // Depth first from every class, remembering the path that got there.
        for (start, _) in &parents {
            let mut stack: Vec<String> = Vec::new();
            if let Some(found) = Self::walk(start, start, &parents, &mut stack) {
                return Some(found);
            }
        }
        None
    }

    /// Walks the parents of `node` looking for `origin`.
    fn walk(
        node: &str,
        origin: &str,
        parents: &[(&str, Vec<(usize, &str)>)],
        path: &mut Vec<String>,
    ) -> Option<(usize, Vec<String>)> {
        let (_, list) = parents.iter().find(|(id, _)| *id == node)?;
        for (index, parent) in list {
            if *parent == origin {
                let mut cycle = vec![origin.to_string()];
                cycle.extend(path.iter().cloned());
                cycle.push(origin.to_string());
                return Some((*index, cycle));
            }
            if path.contains(&parent.to_string()) {
                continue;
            }
            path.push((*parent).to_string());
            if let Some(found) = Self::walk(parent, origin, parents, path) {
                return Some(found);
            }
            path.pop();
        }
        None
    }
}

impl DiagramKind for ClassKind {
    const KIND: &'static str = "class";
    const LAYOUT_HINT: LayoutHint = LayoutHint::Layered;
    const LIMITS: KindLimits = KindLimits {
        max_nodes: 300,
        max_edges: 600,
        max_density: 3.0,
        max_lanes: 0,
        max_events: 0,
    };

    type Input = ClassInput;

    fn schema() -> schemars::Schema {
        schemars::schema_for!(ClassInput)
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

        let admitted: Vec<&str> = RELATIONS.iter().map(|relation| relation.name).collect();
        for (index, edge) in input.edges.iter().enumerate() {
            if let Some(relation) = &edge.relation {
                if !admitted.contains(&relation.as_str()) {
                    errors.push(ValidationError::new(
                        format!("edges[{index}].relation"),
                        ErrorCode::InvalidValue,
                        format!("relación desconocida: `{relation}`"),
                        Some(format!(
                            "las relaciones de este kind son {} (por defecto `{DEFAULT_RELATION}`)",
                            admitted.join(", ")
                        )),
                    ));
                }
            }
        }

        if let Some((index, cycle)) = Self::inheritance_cycle(input) {
            errors.push(ValidationError::new(
                format!("edges[{index}].to"),
                ErrorCode::InvalidValue,
                format!("la herencia forma un ciclo: {}", cycle.join(" → ")),
                Some(
                    "una clase no puede ser su propio padre, ni directo ni por un rodeo: quitá la relación que cierra el ciclo"
                        .to_string(),
                ),
            ));
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
        let nodes = to_ir_nodes(&input.nodes, &ROLES, DEFAULT_ROLE);
        let edges: Vec<IrEdge> = input
            .edges
            .iter()
            .map(|edge| {
                let name = Self::relation_of(edge);
                let relation = RELATIONS
                    .iter()
                    .find(|known| known.name == name)
                    .copied()
                    .unwrap_or(RELATIONS[0]);

                let mut ir_edge = IrEdge::new(edge.from.clone(), edge.to.clone());
                // La relación nombra la línea cuando el documento no lo hace:
                // dos líneas iguales dirían cosas distintas. La asociación no
                // necesita palabra: una flecha simple no afirma nada más.
                ir_edge.label = edge
                    .label
                    .clone()
                    .filter(|label| !label.trim().is_empty())
                    .or_else(|| relation.label.map(str::to_string));
                ir_edge.style = edge.style.unwrap_or(relation.style);
                ir_edge.arrow = edge.arrow.unwrap_or(relation.arrow);
                if let Some(emphasis) = edge.emphasis {
                    ir_edge.emphasis = emphasis;
                }
                ir_edge
            })
            .collect();

        Ir::Graph(IrGraph {
            direction: input.direction.unwrap_or(Direction::TB),
            layout_hint: Self::LAYOUT_HINT,
            nodes,
            edges,
            groups: to_ir_groups(&input.groups),
        })
    }
}

/// Registers the kind in the global registry.
pub fn register() {
    gx_core::kind::register_kind::<ClassKind>();
}

#[cfg(test)]
mod tests {
    use super::{ClassInput, ClassKind};
    use gx_core::error::ErrorCode;
    use gx_core::ir::{Ir, Shape, Style};
    use gx_core::kind::DiagramKind;

    fn input(json: &str) -> ClassInput {
        serde_json::from_str(json).expect("the document parses")
    }

    /// RF15: `groups` existe en `class` y anida: un miembro es un nodo o
    /// otro grupo, y el documento no cambia de version.
    #[test]
    fn the_groups_of_a_diagram_reach_the_ir() {
        let parsed = input(
            r#"{ "version": 1, "kind": "class",
                 "nodes": [ { "id": "read" } ],
                 "groups": [
                   { "id": "io", "label": "I/O", "members": ["disk"] },
                   { "id": "disk", "members": ["read"] } ] }"#,
        );

        ClassKind
            .validate(&parsed)
            .expect("un grupo puede contener otro grupo");

        let Ir::Graph(graph) = ClassKind.to_ir(&parsed) else {
            panic!("un diagrama de `class` es un grafo");
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
            r#"{ "version": 1, "kind": "class",
                 "nodes": [ { "id": "read" } ],
                 "groups": [ { "id": "io", "members": ["fantasma"] } ] }"#,
        );

        let errors = ClassKind
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

    const SHOP: &str = r#"{
      "version": 1, "kind": "class", "title": "pedidos",
      "nodes": [
        { "id": "Base", "label": "Base", "role": "abstract" },
        { "id": "Order", "label": "Order" },
        { "id": "Refund", "label": "Refund" },
        { "id": "Payable", "label": "Payable", "role": "interface" },
        { "id": "Money", "label": "Money" }
      ],
      "edges": [
        { "from": "Order", "to": "Base", "relation": "extends" },
        { "from": "Refund", "to": "Order", "relation": "extends" },
        { "from": "Order", "to": "Payable", "relation": "implements" },
        { "from": "Order", "to": "Money", "relation": "composition", "label": "total" }
      ]
    }"#;

    /// S1: el diagrama de clases es un grafo `layered`; la relación decide la
    /// línea y, si el documento no escribe etiqueta, la nombra.
    #[test]
    fn a_class_diagram_becomes_a_layered_graph() {
        let parsed = input(SHOP);
        ClassKind.validate(&parsed).expect("el modelo es válido");

        let Ir::Graph(graph) = ClassKind.to_ir(&parsed) else {
            panic!("un diagrama de clases es un grafo");
        };

        assert_eq!(graph.layout_hint, gx_core::ir::LayoutHint::Layered);
        assert_eq!(graph.nodes.len(), 5);
        assert_eq!(graph.edges.len(), 4);
        assert_eq!(graph.nodes[0].shape, Shape::Rounded);

        let edge = |from: &str, to: &str| {
            graph
                .edges
                .iter()
                .find(|edge| edge.from == from && edge.to == to)
                .unwrap_or_else(|| panic!("falta {from} → {to}"))
        };
        // La herencia es sólida y con flecha, y se llama `extends`.
        let extends = edge("Order", "Base");
        assert_eq!(extends.style, Style::Solid);
        assert!(extends.arrow);
        assert_eq!(extends.label.as_deref(), Some("extends"));
        // La implementación es punteada.
        let implements = edge("Order", "Payable");
        assert_eq!(implements.style, Style::Dashed);
        // La composición no lleva flecha y conserva la etiqueta del documento.
        let composition = edge("Order", "Money");
        assert!(!composition.arrow);
        assert_eq!(composition.label.as_deref(), Some("total"));
    }

    /// Un vértice sin `relation` es una asociación: sólida, con flecha y **sin
    /// palabra**. Una línea que no dice nada no puede afirmar una herencia.
    #[test]
    fn an_edge_without_a_relation_is_a_plain_association() {
        let parsed = input(
            r#"{ "version": 1, "kind": "class",
                 "nodes": [ { "id": "Order" }, { "id": "Customer" } ],
                 "edges": [ { "from": "Order", "to": "Customer" } ] }"#,
        );
        ClassKind.validate(&parsed).expect("es válido");

        let Ir::Graph(graph) = ClassKind.to_ir(&parsed) else {
            panic!("un diagrama de clases es un grafo");
        };

        let edge = &graph.edges[0];
        assert_eq!(edge.style, Style::Solid);
        assert!(edge.arrow);
        assert_eq!(edge.label, None, "la asociación no se nombra");
    }

    /// La regla del kind: la herencia no puede tener ciclos.
    #[test]
    fn a_cycle_in_the_inheritance_is_reported() {
        let parsed = input(
            r#"{ "version": 1, "kind": "class",
                 "nodes": [
                   { "id": "a" }, { "id": "b" }, { "id": "c" } ],
                 "edges": [
                   { "from": "a", "to": "b", "relation": "extends" },
                   { "from": "b", "to": "c", "relation": "extends" },
                   { "from": "c", "to": "a", "relation": "extends" } ] }"#,
        );

        let errors = ClassKind
            .validate(&parsed)
            .expect_err("la herencia no puede dar la vuelta");

        let cycle = errors
            .iter()
            .find(|error| error.message.contains("ciclo"))
            .expect("el error del ciclo");
        assert_eq!(cycle.code, ErrorCode::InvalidValue);
        assert_eq!(cycle.path, "edges[2].to");
        assert!(cycle.message.contains("a → b → c → a"), "{}", cycle.message);
        assert!(cycle.hint.is_some());
    }

    /// Una clase que se extiende a sí misma es un ciclo de un paso.
    #[test]
    fn a_class_that_extends_itself_is_reported() {
        let parsed = input(
            r#"{ "version": 1, "kind": "class",
                 "nodes": [ { "id": "a" } ],
                 "edges": [ { "from": "a", "to": "a", "relation": "extends" } ] }"#,
        );

        let errors = ClassKind
            .validate(&parsed)
            .expect_err("no puede ser su propio padre");

        assert!(errors.iter().any(|error| error.message.contains("ciclo")));
    }

    /// Una composición puede dar la vuelta (un pedido tiene líneas y una línea
    /// sabe de su pedido): solo las relaciones *es un* se ordenan.
    #[test]
    fn a_cycle_in_a_composition_is_accepted() {
        let parsed = input(
            r#"{ "version": 1, "kind": "class",
                 "nodes": [ { "id": "a" }, { "id": "b" } ],
                 "edges": [
                   { "from": "a", "to": "b", "relation": "composition" },
                   { "from": "b", "to": "a", "relation": "composition" } ] }"#,
        );

        ClassKind
            .validate(&parsed)
            .expect("una composición de ida y vuelta es un modelo válido");
    }

    #[test]
    fn an_unknown_relation_is_reported_with_the_relations_of_the_kind() {
        let parsed = input(
            r#"{ "version": 1, "kind": "class",
                 "nodes": [ { "id": "a" }, { "id": "b" } ],
                 "edges": [ { "from": "a", "to": "b", "relation": "mixes-in" } ] }"#,
        );

        let errors = ClassKind
            .validate(&parsed)
            .expect_err("la relación no existe");

        assert_eq!(errors[0].code, ErrorCode::InvalidValue);
        assert_eq!(errors[0].path, "edges[0].relation");
        let hint = errors[0].hint.as_deref().expect("dice cuáles hay");
        assert!(hint.contains("implements"), "{hint}");
    }

    #[test]
    fn an_unknown_role_and_an_unknown_reference_are_reported() {
        let parsed = input(
            r#"{ "version": 1, "kind": "class",
                 "nodes": [ { "id": "a", "role": "trait" } ],
                 "edges": [ { "from": "a", "to": "fantasma" } ] }"#,
        );

        let errors = ClassKind
            .validate(&parsed)
            .expect_err("el rol y el destino no existen");

        assert!(errors
            .iter()
            .any(|error| error.path == "nodes[0].role" && error.code == ErrorCode::InvalidValue));
        assert!(errors
            .iter()
            .any(|error| error.path == "edges[0].to" && error.code == ErrorCode::UnknownRef));
    }
}
