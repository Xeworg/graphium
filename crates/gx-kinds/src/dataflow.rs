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
use gx_core::ir::{Direction, Emphasis, Ir, IrValue, LayoutHint, Shape, Style};
use gx_core::kind::{DiagramKind, KindLimits, Role};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::flow::{to_graph, validate_flow, validate_header, FlowEdge, FlowNode};
use crate::groups::{to_ir_groups, validate_groups, Group};

/// Contract version supported by this build (PRD §10.1).
pub const SUPPORTED_VERSION: u32 = 1;

/// El rol de un nodo que no declara ninguno: un proceso.
pub const DEFAULT_ROLE: &str = "process";

/// El nivel más profundo que un DFD declara: del contexto (0) al 6
/// (`f27-etapa1.md`, D5).
pub const MAX_LEVEL: u32 = 6;

/// El prefijo del número de proceso: `P` y dígitos (`f27-etapa1.md`, S4).
pub const NUMBER_PREFIX: char = 'P';

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
    /// Contract version. Only `1` is supported; absent means `1`.
    #[serde(default = "crate::flow::default_version")]
    pub version: u32,
    /// Must be `dataflow`.
    pub kind: String,
    /// Human readable title of the diagram.
    #[serde(default)]
    pub title: Option<String>,
    /// Direction of the layout.
    #[serde(default)]
    pub direction: Option<Direction>,
    /// The level of the decomposition (`f27-etapa1.md`, S4): the context
    /// diagram is `0`, and each level below it is one drawing of its own.
    /// Absent means the document does not claim a level.
    #[serde(default)]
    pub level: Option<u32>,
    /// The processes, the stores and the external entities.
    pub nodes: Vec<DataflowNode>,
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

/// A node of a dataflow: the flow fields plus the process number of a DFD
/// (`f27-etapa1.md`, S4).
///
/// The number lives here and not in the shared `FlowNode` because it is a DFD
/// fact: a flowchart box has no number, and a field every other kind silently
/// ignores is a field that lies about the contract. This is the same shape
/// `class` took for its members (`ClassNode::flow`).
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DataflowNode {
    /// Unique identifier inside the diagram.
    pub id: String,
    /// Text of the node. Defaults to the id.
    #[serde(default)]
    pub label: Option<String>,
    /// Role of the node: `process`, `store` or `external`.
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub emphasis: Option<Emphasis>,
    #[serde(default)]
    pub style: Option<Style>,
    #[serde(default)]
    pub stereotype: Option<String>,
    /// El número de proceso de Gane-Sarson: `P1`, `P2`… Único en el diagrama y
    /// dibujado como prefijo de la etiqueta, que es donde hoy puede ir (la
    /// banda de la caja es un cambio de renderer, declarado fuera de este
    /// corte).
    #[serde(default)]
    pub number: Option<String>,
}

impl DataflowNode {
    /// The flow node the shared machinery reads.
    fn flow(&self) -> FlowNode {
        FlowNode {
            id: self.id.clone(),
            label: self.label.clone(),
            role: self.role.clone(),
            emphasis: self.emphasis,
            style: self.style,
            stereotype: self.stereotype.clone(),
        }
    }
}

impl DataflowKind {
    /// The role of a node, or the default one.
    fn role_of(node: &DataflowNode) -> &str {
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

    /// El número de un proceso, cuando lo declara y no está en blanco.
    fn number_of(node: &DataflowNode) -> Option<&str> {
        node.number
            .as_deref()
            .map(str::trim)
            .filter(|number| !number.is_empty())
    }

    /// Las dos reglas que llegan con los campos (`f27-etapa1.md`, S4): el
    /// número es `P` y dígitos, único en el diagrama, y el nivel del documento
    /// es coherente con la numeración.
    fn check_numbers(input: &DataflowInput) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        if let Some(level) = input.level {
            if level > MAX_LEVEL {
                errors.push(ValidationError::new(
                    "level",
                    ErrorCode::InvalidValue,
                    format!("nivel de DFD fuera de rango: {level} (del contexto 0 al {MAX_LEVEL})"),
                    Some(
                        "cada nivel es un documento aparte que descompone un proceso del anterior"
                            .to_string(),
                    ),
                ));
            }
        }

        let mut seen: Vec<&str> = Vec::new();
        let mut numbered = false;
        for (index, node) in input.nodes.iter().enumerate() {
            let Some(number) = Self::number_of(node) else {
                continue;
            };
            numbered = true;
            let digits = number.strip_prefix(NUMBER_PREFIX);
            let well_formed = digits.is_some_and(|digits| {
                !digits.is_empty() && digits.chars().all(|character| character.is_ascii_digit())
            });
            if !well_formed {
                errors.push(ValidationError::new(
                    format!("nodes[{index}].number"),
                    ErrorCode::InvalidValue,
                    format!("número de proceso inválido: `{number}`"),
                    Some(format!(
                        "el número de Gane-Sarson es `{NUMBER_PREFIX}` y dígitos, por ejemplo `{NUMBER_PREFIX}1`"
                    )),
                ));
                continue;
            }
            if seen.contains(&number) {
                errors.push(ValidationError::new(
                    format!("nodes[{index}].number"),
                    ErrorCode::DuplicateId,
                    format!("número de proceso repetido: `{number}`"),
                    Some("cada proceso se numera una vez: el DFD se lee por el número".to_string()),
                ));
                continue;
            }
            seen.push(number);
        }

        // El contexto es el sistema con sus entidades externas: no hay proceso
        // que numerar, y un `P1` ahí promete un nivel 0 que no existe.
        if input.level == Some(0) && numbered {
            errors.push(ValidationError::new(
                "level",
                ErrorCode::InvalidValue,
                "el diagrama de contexto (nivel 0) no numera procesos".to_string(),
                Some("quitá `level: 0` o quitá los números: el nivel 0 es el sistema con sus entidades externas".to_string()),
            ));
        }

        errors
    }
}

impl DiagramKind for DataflowKind {
    const KIND: &'static str = "dataflow";
    const PHASE: &'static str = "F2";
    const LAYOUT_HINT: LayoutHint = LayoutHint::Layered;
    const LIMITS: KindLimits = KindLimits::GRAPH;
    const KIND_VERSION: u32 = SUPPORTED_VERSION;

    type Input = DataflowInput;

    fn schema() -> schemars::Schema {
        schemars::schema_for!(DataflowInput)
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
        let flow_nodes: Vec<FlowNode> = input.nodes.iter().map(DataflowNode::flow).collect();
        errors.extend(validate_flow(
            Self::KIND,
            &flow_nodes,
            &input.edges,
            &ROLES,
            DEFAULT_ROLE,
        ));
        let node_ids: Vec<&str> = input.nodes.iter().map(|node| node.id.as_str()).collect();
        errors.extend(validate_groups(&node_ids, &input.groups));

        errors.extend(Self::check_flows(input));
        errors.extend(Self::check_numbers(input));

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn to_ir(&self, input: &Self::Input) -> Ir {
        let flow_nodes: Vec<FlowNode> = input.nodes.iter().map(DataflowNode::flow).collect();
        let mut graph = to_graph(
            &flow_nodes,
            &input.edges,
            &ROLES,
            DEFAULT_ROLE,
            input.direction,
            Self::LAYOUT_HINT,
            to_ir_groups(&input.groups),
        );
        for (node, input_node) in graph.nodes.iter_mut().zip(&input.nodes) {
            // El número va delante del nombre: el DFD se lee por el número, y
            // la etiqueta es lo único que hoy lleva texto libre sobre la caja.
            if let Some(number) = Self::number_of(input_node) {
                node.label = format!("{number} · {}", node.label);
            }
            // El nivel no tiene nodo propio: va en el canal `meta`, que viaja
            // intacto a la escena (`f27-etapa1.md`, D5).
            if let Some(level) = input.level {
                node.meta
                    .insert("level".to_string(), IrValue::Int(i64::from(level)));
            }
        }
        Ir::Graph(graph)
    }

    /// La regla del diagrama de flujo de datos: los datos se mueven por un
    /// proceso, nunca de depósito a depósito ni de externo a externo.
    fn rules(&self) -> Vec<String> {
        vec![
            "Un flujo no puede ir de un depósito a otro ni de una entidad externa a otra: en el medio va un proceso."
                .to_string(),
            "La etiqueta del flujo es el dato que viaja.".to_string(),
            "Un proceso puede llevar su número de Gane-Sarson (`number: \"P1\"`), único en el diagrama: se dibuja como prefijo de la etiqueta."
                .to_string(),
            format!(
                "El nivel (`level`) va del contexto, que es 0, al {MAX_LEVEL}: el diagrama de contexto no numera procesos y cada nivel es un documento aparte."
            ),
        ]
    }

    fn min_example() -> serde_json::Value {
        serde_json::json!({
            "version": 1,
            "kind": "dataflow",
            "title": "ETL",
            "nodes": [
                { "id": "crm", "label": "CRM", "role": "external" },
                { "id": "ingesta", "label": "ingesta", "role": "process" },
                { "id": "crudo", "label": "pedidos crudos", "role": "store" }
            ],
            "edges": [
                { "from": "crm", "to": "ingesta", "label": "pedidos" },
                { "from": "ingesta", "to": "crudo", "label": "pedidos crudos" }
            ]
        })
    }
}

/// Registers the kind in the global registry.
pub fn register() {
    gx_core::kind::register_kind::<DataflowKind>();
}

#[cfg(test)]
mod tests {
    use super::DataflowKind;
    use crate::test_support::{an_unknown_group_member_is_rejected, groups_reach_the_ir, input};
    use gx_core::error::ErrorCode;
    use gx_core::ir::{Ir, Shape};
    use gx_core::kind::DiagramKind;

    /// RF15: `groups` existe en `dataflow` y anida: un miembro es un nodo o
    /// otro grupo, y el documento no cambia de version.
    #[test]
    fn the_groups_of_a_diagram_reach_the_ir() {
        groups_reach_the_ir::<DataflowKind>(
            r#"{ "version": 1, "kind": "dataflow",
                 "nodes": [ { "id": "read" } ],
                 "groups": [
                   { "id": "io", "label": "I/O", "members": ["disk"] },
                   { "id": "disk", "members": ["read"] } ] }"#,
        );
    }

    /// Un miembro que no existe se reporta antes de dibujar.
    #[test]
    fn a_group_member_that_does_not_exist_is_rejected() {
        an_unknown_group_member_is_rejected::<DataflowKind>(
            r#"{ "version": 1, "kind": "dataflow",
                 "nodes": [ { "id": "read" } ],
                 "groups": [ { "id": "io", "members": ["fantasma"] } ] }"#,
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

    /// S4 de `f27-etapa1.md`: el número de proceso de Gane-Sarson se declara y
    /// llega al dibujo como prefijo de la etiqueta, que es el único lugar donde
    /// hoy puede ir (la banda de la caja es un cambio de renderer que este
    /// corte no compra).
    #[test]
    fn a_process_carries_its_number_into_the_label() {
        let document: crate::dataflow::DataflowInput = input(
            r#"{ "version": 1, "kind": "dataflow",
                 "nodes": [ { "id": "ingesta", "label": "ingesta",
                              "role": "process", "number": "P1" } ],
                 "edges": [] }"#,
        );

        DataflowKind
            .validate(&document)
            .expect("P1 es un número legal");
        let Ir::Graph(graph) = DataflowKind.to_ir(&document) else {
            panic!("un dataflow es un grafo")
        };
        assert_eq!(graph.nodes[0].label, "P1 · ingesta");
    }

    /// La numeración es única: dos procesos llamados P1 son dos procesos con el
    /// mismo nombre, y el DFD los lee por el número.
    #[test]
    fn a_duplicated_process_number_is_reported() {
        let document: crate::dataflow::DataflowInput = input(
            r#"{ "version": 1, "kind": "dataflow",
                 "nodes": [ { "id": "a", "role": "process", "number": "P1" },
                            { "id": "b", "role": "process", "number": "P1" } ],
                 "edges": [] }"#,
        );

        let errors = DataflowKind
            .validate(&document)
            .expect_err("P1 no puede ser dos procesos");
        assert_eq!(errors[0].path, "nodes[1].number");
        assert_eq!(errors[0].code, ErrorCode::DuplicateId);
    }

    /// El formato: `P` y dígitos. Nada de `1`, `p1` ni `P1a`.
    #[test]
    fn a_number_that_is_not_p_and_digits_is_reported() {
        for bad in ["1", "p1", "P1a", "P"] {
            let document: crate::dataflow::DataflowInput = input(&format!(
                r#"{{ "version": 1, "kind": "dataflow",
                      "nodes": [ {{ "id": "a", "number": "{bad}" }} ],
                      "edges": [] }}"#
            ));

            let errors = DataflowKind
                .validate(&document)
                .expect_err("el número de proceso es P y dígitos");
            assert_eq!(errors[0].path, "nodes[0].number", "para `{bad}`");
            assert_eq!(errors[0].code, ErrorCode::InvalidValue);
        }
    }

    /// El nivel es del documento y llega a cada nodo por el canal `meta`, que
    /// ya viaja intacto a la escena.
    #[test]
    fn the_level_reaches_the_nodes_as_metadata() {
        let document: crate::dataflow::DataflowInput = input(
            r#"{ "version": 1, "kind": "dataflow", "level": 2,
                 "nodes": [ { "id": "a", "role": "process", "number": "P1" } ],
                 "edges": [] }"#,
        );

        DataflowKind.validate(&document).expect("nivel 2 es legal");
        let Ir::Graph(graph) = DataflowKind.to_ir(&document) else {
            panic!("un dataflow es un grafo")
        };
        assert_eq!(
            graph.nodes[0].meta.get("level"),
            Some(&gx_core::ir::IrValue::Int(2))
        );
    }

    /// El nivel también tiene techo: más allá del 6 un DFD deja de ser legible
    /// y el documento no está diciendo un nivel, está inventando uno.
    #[test]
    fn a_level_beyond_the_ceiling_is_reported() {
        let document: crate::dataflow::DataflowInput = input(
            r#"{ "version": 1, "kind": "dataflow", "level": 7,
                 "nodes": [ { "id": "a", "role": "process" } ],
                 "edges": [] }"#,
        );

        let errors = DataflowKind
            .validate(&document)
            .expect_err("el nivel va del 0 al 6");
        assert_eq!(errors[0].path, "level");
        assert_eq!(errors[0].code, ErrorCode::InvalidValue);
    }

    /// El diagrama de contexto no numera procesos: en el nivel 0 no hay
    /// procesos que numerar, solo el sistema y sus entidades externas.
    #[test]
    fn the_context_level_has_no_numbered_process() {
        let document: crate::dataflow::DataflowInput = input(
            r#"{ "version": 1, "kind": "dataflow", "level": 0,
                 "nodes": [ { "id": "a", "role": "process", "number": "P1" } ],
                 "edges": [] }"#,
        );

        let errors = DataflowKind
            .validate(&document)
            .expect_err("el contexto no numera");
        assert_eq!(errors[0].path, "level");
        assert_eq!(errors[0].code, ErrorCode::InvalidValue);
    }
}
