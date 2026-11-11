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
use gx_core::ir::{Direction, Emphasis, Ir, IrEdge, IrRow, LayoutHint, Shape, Style};
use gx_core::kind::{DiagramKind, KindLimits, Role};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::flow::{to_ir_nodes, validate_header, validate_nodes, validate_refs, FlowNode};
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

/// A node of the class diagram: the flow fields plus the members
/// (`f22-dibujo.md`, S7) — the attributes and the methods a class carries,
/// each one a line of its own compartment. Opt-in: a node without members
/// is the plain box of always.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassNode {
    /// Unique identifier inside the diagram.
    pub id: String,
    /// Text of the node. Defaults to the id.
    #[serde(default)]
    pub label: Option<String>,
    /// Role of the node: it decides the shape and the stereotype.
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub emphasis: Option<Emphasis>,
    #[serde(default)]
    pub style: Option<Style>,
    /// The attributes, the lines of the first compartment.
    #[serde(default)]
    pub attributes: Vec<String>,
    /// The methods, the lines of the second compartment.
    #[serde(default)]
    pub methods: Vec<String>,
}

impl ClassNode {
    /// The flow node the shared machinery reads.
    fn flow(&self) -> FlowNode {
        FlowNode {
            id: self.id.clone(),
            label: self.label.clone(),
            role: self.role.clone(),
            emphasis: self.emphasis,
            style: self.style,
        }
    }
}

/// The JSON contract of the `class` kind.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassInput {
    /// Contract version. Only `1` is supported; absent means `1`.
    #[serde(default = "crate::flow::default_version")]
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
    pub nodes: Vec<ClassNode>,
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
    const PHASE: &'static str = "F2";
    const LAYOUT_HINT: LayoutHint = LayoutHint::Layered;
    const LIMITS: KindLimits = KindLimits::GRAPH;
    const KIND_VERSION: u32 = SUPPORTED_VERSION;

    type Input = ClassInput;

    fn schema() -> schemars::Schema {
        schemars::schema_for!(ClassInput)
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
        let flow_nodes: Vec<FlowNode> = input.nodes.iter().map(|node| node.flow()).collect();
        errors.extend(validate_nodes(
            Self::KIND,
            &flow_nodes,
            &ROLES,
            DEFAULT_ROLE,
        ));
        // The members are the lines of the compartments (S7): a blank one
        // would draw an empty line, so it says nothing.
        for (index, node) in input.nodes.iter().enumerate() {
            for (field, members) in [("attributes", &node.attributes), ("methods", &node.methods)] {
                for (member_index, member) in members.iter().enumerate() {
                    if member.trim().is_empty() {
                        errors.push(ValidationError::new(
                            format!("nodes[{index}].{field}[{member_index}]"),
                            ErrorCode::InvalidValue,
                            "un miembro no puede estar vacío",
                            Some("escribí la línea tal como se lee en la clase".to_string()),
                        ));
                    }
                }
            }
        }
        let pairs: Vec<(&str, &str)> = input
            .edges
            .iter()
            .map(|edge| (edge.from.as_str(), edge.to.as_str()))
            .collect();
        errors.extend(validate_refs(&pairs, &flow_nodes));

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
        let flow_nodes: Vec<FlowNode> = input.nodes.iter().map(|node| node.flow()).collect();
        let mut nodes = to_ir_nodes(&flow_nodes, &ROLES, DEFAULT_ROLE);
        // El rol que se dibuja con palabra la lleva sobre el nombre
        // (`f22-dibujo.md`, S4): UML marca el estereotipo con angulillos. El
        // `class` y el `enum` no llevan ninguno: el primero es el default y el
        // segundo ya se distingue por su forma.
        for (node, input_node) in nodes.iter_mut().zip(&input.nodes) {
            match input_node.role.as_deref() {
                Some("abstract") | Some("interface") => {
                    node.stereotype = input_node.role.clone();
                }
                _ => {}
            }
            // The members are the rows the table machinery already draws
            // (S7, L9): the attributes are the first compartment, the methods
            // the second, and the rule above the first method is what makes
            // them two. The header rule is the first compartment's on its
            // own, so a node without attributes divides nothing.
            let mut rows: Vec<IrRow> = input_node
                .attributes
                .iter()
                .map(|attribute| IrRow::new(attribute.clone()))
                .collect();
            for (index, method) in input_node.methods.iter().enumerate() {
                let mut row = IrRow::new(method.clone());
                row.divider = index == 0 && !rows.is_empty();
                rows.push(row);
            }
            node.rows = rows;
        }
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

        Ir::Graph(crate::flow::ir_graph(
            input.direction,
            Self::LAYOUT_HINT,
            nodes,
            edges,
            to_ir_groups(&input.groups),
        ))
    }

    /// Lo que el kind enseña y el schema no puede: la relación decide la línea
    /// y la herencia no puede cerrar un ciclo.
    fn rules(&self) -> Vec<String> {
        vec![
            "`relation` decide la línea: `association` (sólida con flecha, por defecto), `extends` (sólida, se nombra), `implements` (punteada, se nombra), `composition` (sólida sin flecha), `dependency` (punteada)."
                .to_string(),
            "La herencia no puede tener ciclos: una clase no es su propio padre.".to_string(),
            "Si la arista no lleva `label`, la línea se nombra con su relación; la asociación no, porque no afirma nada."
                .to_string(),
        ]
    }

    fn min_example() -> serde_json::Value {
        serde_json::json!({
            "version": 1,
            "kind": "class",
            "title": "pedidos",
            "nodes": [
                { "id": "Base", "label": "Base", "role": "abstract" },
                { "id": "Order", "label": "Order", "attributes": ["id: uuid"], "methods": ["cobrar()"] },
                { "id": "Payable", "label": "Payable", "role": "interface" }
            ],
            "edges": [
                { "from": "Order", "to": "Base", "relation": "extends" },
                { "from": "Order", "to": "Payable", "relation": "implements" }
            ]
        })
    }
}

/// Registers the kind in the global registry.
pub fn register() {
    gx_core::kind::register_kind::<ClassKind>();
}

#[cfg(test)]
mod tests {
    use super::ClassKind;
    use crate::test_support::{an_unknown_group_member_is_rejected, groups_reach_the_ir, input};
    use gx_core::error::ErrorCode;
    use gx_core::ir::{Ir, Shape, Style};
    use gx_core::kind::DiagramKind;

    /// RF15: `groups` existe en `class` y anida: un miembro es un nodo o
    /// otro grupo, y el documento no cambia de version.
    #[test]
    fn the_groups_of_a_diagram_reach_the_ir() {
        groups_reach_the_ir::<ClassKind>(
            r#"{ "version": 1, "kind": "class",
                 "nodes": [ { "id": "read" } ],
                 "groups": [
                   { "id": "io", "label": "I/O", "members": ["disk"] },
                   { "id": "disk", "members": ["read"] } ] }"#,
        );
    }

    /// Un miembro que no existe se reporta antes de dibujar.
    #[test]
    fn a_group_member_that_does_not_exist_is_rejected() {
        an_unknown_group_member_is_rejected::<ClassKind>(
            r#"{ "version": 1, "kind": "class",
                 "nodes": [ { "id": "read" } ],
                 "groups": [ { "id": "io", "members": ["fantasma"] } ] }"#,
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

    #[test]
    fn class_members_fill_the_rows_and_the_schema_admits_them() {
        let parsed = input(
            r#"{ "version": 1, "kind": "class",
                 "nodes": [ { "id": "pedido", "label": "Pedido", "attributes": ["id: uuid"], "methods": ["cobrar()", "total(): din"] } ],
                 "edges": [] }"#,
        );

        // S7 (`f22-dibujo.md`): the members are opt-in and additive — a node
        // that declares them validates, and the plain node of always stays.
        ClassKind
            .validate(&parsed)
            .expect("los miembros son parte del contrato");

        let Ir::Graph(graph) = ClassKind.to_ir(&parsed) else {
            panic!("una clase es un grafo");
        };
        let rows = &graph.nodes[0].rows;
        assert_eq!(rows.len(), 3, "una línea por miembro");
        assert_eq!(rows[0].text, "id: uuid");
        assert_eq!(rows[1].text, "cobrar()");
        assert_eq!(rows[2].text, "total(): din");
        // The compartment rule sits above the first method: attributes and
        // methods read as the two compartments of UML. The first row never
        // divides — the header rule is its compartment's on its own.
        assert!(!rows[0].divider, "la primera fila no divide");
        assert!(rows[1].divider, "la regla separa atributos de métodos");
        assert!(!rows[2].divider, "solo la primera de su compartimento");

        // Methods without attributes divide nothing: the header rule is the
        // compartment's rule already.
        let parsed = input(
            r#"{ "version": 1, "kind": "class",
                 "nodes": [ { "id": "p", "methods": ["cobrar()"] } ],
                 "edges": [] }"#,
        );
        let Ir::Graph(graph) = ClassKind.to_ir(&parsed) else {
            panic!("una clase es un grafo");
        };
        assert!(!graph.nodes[0].rows[0].divider);

        // The schema publishes the members, so the schema is the contract.
        let schema = ClassKind::schema().to_value().to_string();
        assert!(
            schema.contains("attributes"),
            "el esquema no nombra atributos"
        );
        assert!(schema.contains("methods"), "el esquema no nombra métodos");
    }

    #[test]
    fn an_empty_member_is_reported() {
        let parsed = input(
            r#"{ "version": 1, "kind": "class",
                 "nodes": [ { "id": "p", "attributes": ["   "] } ],
                 "edges": [] }"#,
        );

        let errors = ClassKind
            .validate(&parsed)
            .expect_err("una línea en blanco no dice nada");

        assert_eq!(errors[0].path, "nodes[0].attributes[0]");
        assert_eq!(errors[0].code, ErrorCode::InvalidValue);
    }

    #[test]
    fn a_class_node_rejects_unknown_fields_still() {
        use crate::class::ClassInput;

        // The rejection is serde's `deny_unknown_fields`, at parse time —
        // before any validation runs.
        let error = serde_json::from_str::<ClassInput>(
            r#"{ "version": 1, "kind": "class",
                 "nodes": [ { "id": "p", "atributos": ["x"] } ],
                 "edges": [] }"#,
        )
        .expect_err("deny_unknown_fields sigue guardando el contrato");
        assert!(error.to_string().contains("atributos"));
    }
}
