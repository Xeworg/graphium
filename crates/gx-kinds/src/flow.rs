//! Lo que comparten los kinds de flujo: nodos con un rol que decide su forma,
//! aristas con su etiqueta y las reglas que JSON Schema no puede expresar
//! (PRD §6, `flowchart`, `state`, `dataflow`).
//!
//! Los tres kinds se dibujan con el motor `layered` y no necesitan formas
//! nuevas: lo que los distingue es el **vocabulario** —un paso, un estado, un
//! depósito de datos— y eso se dice con el `role` del nodo, que el kind valida
//! contra su propia lista. Agregar un rol es agregar una línea en el kind, no
//! tocar el motor ni el renderer (O7).

use gx_core::error::{ErrorCode, ValidationError};
use gx_core::ir::{Direction, Emphasis, IrEdge, IrGraph, IrNode, LayoutHint, Shape, Style};
use schemars::JsonSchema;
use serde::Deserialize;

/// A node of a flow.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FlowNode {
    /// Unique identifier inside the diagram.
    pub id: String,
    /// Text of the node. Defaults to the id.
    #[serde(default)]
    pub label: Option<String>,
    /// Role of the node: it decides the shape. The kind lists the roles it
    /// admits, and the default when the field is absent.
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub emphasis: Option<Emphasis>,
    #[serde(default)]
    pub style: Option<Style>,
}

/// An edge of a flow: the arrow and what it says.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FlowEdge {
    /// Id of the source node.
    pub from: String,
    /// Id of the target node.
    pub to: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub emphasis: Option<Emphasis>,
    #[serde(default)]
    pub style: Option<Style>,
    /// Whether the edge draws an arrowhead. Defaults to true.
    #[serde(default)]
    pub arrow: Option<bool>,
}

/// A role a node may take, and the shape it is drawn with.
#[derive(Debug, Clone, Copy)]
pub struct Role {
    /// The name the document writes, e.g. `decision`.
    pub name: &'static str,
    /// The shape of [`Shape`] that role is drawn with.
    pub shape: Shape,
}

impl Role {
    pub const fn new(name: &'static str, shape: Shape) -> Self {
        Self { name, shape }
    }
}

/// The shape of a role, or the one of the default role.
fn shape_of(roles: &[Role], role: &str) -> Shape {
    roles
        .iter()
        .find(|candidate| candidate.name == role)
        .map(|candidate| candidate.shape)
        .unwrap_or(Shape::Rounded)
}

/// Validates a flow document: the rules JSON Schema cannot express.
///
/// `roles` are the roles the kind admits and `default_role` the one used when
/// a node does not declare any. The errors name the field the agent has to fix
/// and, when the vocabulary is the problem, list what the kind admits.
pub fn validate_flow(
    kind: &str,
    nodes: &[FlowNode],
    edges: &[FlowEdge],
    roles: &[Role],
    default_role: &str,
) -> Vec<ValidationError> {
    let pairs: Vec<(&str, &str)> = edges
        .iter()
        .map(|edge| (edge.from.as_str(), edge.to.as_str()))
        .collect();
    let mut errors = validate_nodes(kind, nodes, roles, default_role);
    errors.extend(validate_refs(&pairs, nodes));
    errors
}

/// Validates the nodes: unique, non empty ids and a role the kind admits.
pub fn validate_nodes(
    kind: &str,
    nodes: &[FlowNode],
    roles: &[Role],
    default_role: &str,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    if nodes.is_empty() {
        errors.push(ValidationError::new(
            "nodes",
            ErrorCode::MissingField,
            format!("un diagrama de `{kind}` necesita al menos un nodo"),
            Some("agregá el primer nodo, por ejemplo `{ \"id\": \"a\" }`".to_string()),
        ));
    }

    let admitted: Vec<&str> = roles.iter().map(|role| role.name).collect();
    let mut seen: Vec<&str> = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        if node.id.trim().is_empty() {
            errors.push(ValidationError::new(
                format!("nodes[{index}].id"),
                ErrorCode::InvalidValue,
                "el id del nodo no puede estar vacío",
                None,
            ));
        }
        if seen.contains(&node.id.as_str()) {
            errors.push(ValidationError::new(
                format!("nodes[{index}].id"),
                ErrorCode::DuplicateId,
                format!("id de nodo repetido: `{}`", node.id),
                Some("los ids de nodo deben ser únicos dentro del diagrama".to_string()),
            ));
        }
        seen.push(node.id.as_str());

        if let Some(role) = &node.role {
            if !admitted.contains(&role.as_str()) {
                errors.push(ValidationError::new(
                    format!("nodes[{index}].role"),
                    ErrorCode::InvalidValue,
                    format!("rol desconocido en `{kind}`: `{role}`"),
                    Some(format!(
                        "los roles de este kind son {} (por defecto `{default_role}`)",
                        admitted.join(", ")
                    )),
                ));
            }
        }
    }

    errors
}

/// Validates that every edge joins two declared nodes.
pub fn validate_refs(pairs: &[(&str, &str)], nodes: &[FlowNode]) -> Vec<ValidationError> {
    let ids: Vec<&str> = nodes.iter().map(|node| node.id.as_str()).collect();
    let mut errors = Vec::new();
    for (index, (from, to)) in pairs.iter().enumerate() {
        if !ids.contains(from) {
            errors.push(ValidationError::new(
                format!("edges[{index}].from"),
                ErrorCode::UnknownRef,
                format!("nodo de origen desconocido: `{from}`"),
                Some("las aristas referencian nodos declarados en `nodes`".to_string()),
            ));
        }
        if !ids.contains(to) {
            errors.push(ValidationError::new(
                format!("edges[{index}].to"),
                ErrorCode::UnknownRef,
                format!("nodo de destino desconocido: `{to}`"),
                Some("las aristas referencian nodos declarados en `nodes`".to_string()),
            ));
        }
    }
    errors
}

/// The `kind` and the `version` checks every kind shares.
pub fn validate_header(
    kind: &str,
    document_kind: &str,
    supported_version: u32,
    version: u32,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    if document_kind != kind {
        errors.push(ValidationError::new(
            "kind",
            ErrorCode::InvalidValue,
            format!("`kind` debe ser `{kind}`, no `{document_kind}`"),
            Some("usá get_diagram_schema para ver los kinds disponibles".to_string()),
        ));
    }
    if version != supported_version {
        errors.push(ValidationError::new(
            "version",
            ErrorCode::InvalidValue,
            format!("versión de contrato no soportada: {version} (soportada: {supported_version})"),
            Some("actualizá el documento a la versión 1 del contrato".to_string()),
        ));
    }
    errors
}

/// Translates the nodes of a validated flow document to IR nodes.
pub fn to_ir_nodes(nodes: &[FlowNode], roles: &[Role], default_role: &str) -> Vec<IrNode> {
    nodes
        .iter()
        .map(|node| {
            let mut ir_node = IrNode::new(
                node.id.clone(),
                node.label
                    .clone()
                    .filter(|label| !label.trim().is_empty())
                    .unwrap_or_else(|| node.id.clone()),
            );
            let role = node.role.as_deref().unwrap_or(default_role);
            ir_node.shape = shape_of(roles, role);
            if let Some(emphasis) = node.emphasis {
                ir_node.emphasis = emphasis;
            }
            if let Some(style) = node.style {
                ir_node.style = style;
            }
            ir_node
        })
        .collect()
}

/// Translates a validated flow document to the layered IR.
pub fn to_graph(
    nodes: &[FlowNode],
    edges: &[FlowEdge],
    roles: &[Role],
    default_role: &str,
    direction: Option<Direction>,
    layout_hint: LayoutHint,
) -> IrGraph {
    let nodes = to_ir_nodes(nodes, roles, default_role);

    let edges: Vec<IrEdge> = edges
        .iter()
        .map(|edge| {
            let mut ir_edge = IrEdge::new(edge.from.clone(), edge.to.clone());
            ir_edge.label = edge.label.clone().filter(|label| !label.trim().is_empty());
            if let Some(emphasis) = edge.emphasis {
                ir_edge.emphasis = emphasis;
            }
            if let Some(style) = edge.style {
                ir_edge.style = style;
            }
            if let Some(arrow) = edge.arrow {
                ir_edge.arrow = arrow;
            }
            ir_edge
        })
        .collect();

    IrGraph {
        direction: direction.unwrap_or(Direction::TB),
        layout_hint,
        nodes,
        edges,
        groups: Vec::new(),
    }
}
