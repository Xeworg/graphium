//! The `graph` kind: boxes and arrows (PRD §6.1).
//!
//! Recipe of a kind (PRD §6.2): schema with `deny_unknown_fields`, semantic
//! rules for what JSON Schema cannot express, translation to the IR and one
//! line in the registry. `gx-mcp` and `gx-app` do not change when a kind is
//! added.

use gx_core::error::{ErrorCode, ValidationError};
use gx_core::ir::{Direction, Emphasis, Ir, IrEdge, IrGraph, IrNode, LayoutHint, Shape, Style};
use gx_core::kind::{DiagramKind, KindLimits};
use schemars::JsonSchema;
use serde::Deserialize;

/// Contract version supported by this build (PRD §10.1).
pub const SUPPORTED_VERSION: u32 = 1;

/// A node of the graph.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphNode {
    /// Unique identifier inside the diagram.
    pub id: String,
    /// Text of the node. Defaults to the id.
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub shape: Option<Shape>,
    #[serde(default)]
    pub emphasis: Option<Emphasis>,
    #[serde(default)]
    pub style: Option<Style>,
}

/// An edge of the graph.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphEdge {
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

/// The JSON contract of the `graph` kind.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphInput {
    /// Contract version. Only `1` is supported.
    pub version: u32,
    /// Must be `graph`.
    pub kind: String,
    /// Human readable title of the diagram.
    #[serde(default)]
    pub title: Option<String>,
    /// Direction of the layered layout.
    #[serde(default)]
    pub direction: Option<Direction>,
    #[serde(default)]
    pub nodes: Vec<GraphNode>,
    #[serde(default)]
    pub edges: Vec<GraphEdge>,
}

/// The `graph` kind.
#[derive(Debug, Default)]
pub struct GraphKind;

impl DiagramKind for GraphKind {
    const KIND: &'static str = "graph";
    const LAYOUT_HINT: LayoutHint = LayoutHint::Layered;
    const LIMITS: KindLimits = KindLimits {
        max_nodes: 300,
        max_edges: 600,
        max_density: 3.0,
        max_lanes: 0,
        max_events: 0,
    };

    type Input = GraphInput;

    fn validate(&self, input: &Self::Input) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();

        if input.kind != Self::KIND {
            errors.push(ValidationError::new(
                "kind",
                ErrorCode::InvalidValue,
                format!("`kind` debe ser `{}`, no `{}`", Self::KIND, input.kind),
                Some("usá get_diagram_schema para ver los kinds disponibles".to_string()),
            ));
        }

        if input.version != SUPPORTED_VERSION {
            errors.push(ValidationError::new(
                "version",
                ErrorCode::InvalidValue,
                format!(
                    "versión de contrato no soportada: {} (soportada: {SUPPORTED_VERSION})",
                    input.version
                ),
                Some("actualizá el documento a la versión 1 del contrato".to_string()),
            ));
        }

        let mut seen: Vec<&str> = Vec::new();
        for (index, node) in input.nodes.iter().enumerate() {
            if node.id.is_empty() {
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
        }

        for (index, edge) in input.edges.iter().enumerate() {
            if !seen.contains(&edge.from.as_str()) {
                errors.push(ValidationError::new(
                    format!("edges[{index}].from"),
                    ErrorCode::UnknownRef,
                    format!("nodo de origen desconocido: `{}`", edge.from),
                    Some("las aristas referencian nodos declarados en `nodes`".to_string()),
                ));
            }
            if !seen.contains(&edge.to.as_str()) {
                errors.push(ValidationError::new(
                    format!("edges[{index}].to"),
                    ErrorCode::UnknownRef,
                    format!("nodo de destino desconocido: `{}`", edge.to),
                    Some("las aristas referencian nodos declarados en `nodes`".to_string()),
                ));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn to_ir(&self, input: &Self::Input) -> Ir {
        let nodes = input
            .nodes
            .iter()
            .map(|node| {
                let mut ir_node = IrNode::new(
                    node.id.clone(),
                    node.label.clone().unwrap_or_else(|| node.id.clone()),
                );
                if let Some(shape) = node.shape {
                    ir_node.shape = shape;
                }
                if let Some(emphasis) = node.emphasis {
                    ir_node.emphasis = emphasis;
                }
                if let Some(style) = node.style {
                    ir_node.style = style;
                }
                ir_node
            })
            .collect();

        let edges = input
            .edges
            .iter()
            .map(|edge| {
                let mut ir_edge = IrEdge::new(edge.from.clone(), edge.to.clone());
                ir_edge.label = edge.label.clone();
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

        Ir::Graph(IrGraph {
            direction: input.direction.unwrap_or(Direction::TB),
            layout_hint: Self::LAYOUT_HINT,
            nodes,
            edges,
            groups: Vec::new(),
        })
    }

    fn schema() -> schemars::Schema {
        schemars::schema_for!(GraphInput)
    }
}

/// Registers the `graph` kind in the global registry.
pub fn register() {
    gx_core::kind::register_kind::<GraphKind>();
}

#[cfg(test)]
mod tests {
    use super::{GraphInput, GraphKind};
    use gx_core::error::ErrorCode;
    use gx_core::ir::Ir;
    use gx_core::kind::DiagramKind;

    const MINIMAL: &str = r#"{
      "version": 1,
      "kind": "graph",
      "title": "parse_config() — flujo",
      "direction": "TB",
      "nodes": [
        { "id": "read", "label": "read file" },
        { "id": "ok",   "label": "Config" }
      ],
      "edges": [
        { "from": "read", "to": "ok", "label": "utf-8" }
      ]
    }"#;

    #[test]
    fn minimal_example_validates() {
        let input: GraphInput = serde_json::from_str(MINIMAL).expect("valid document");
        assert!(GraphKind.validate(&input).is_ok());
    }

    #[test]
    fn minimal_example_becomes_the_expected_ir() {
        let input: GraphInput = serde_json::from_str(MINIMAL).expect("valid document");
        let ir = GraphKind.to_ir(&input);
        let graph = ir.graph().expect("graph ir");
        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.nodes[0].id, "read");
        assert_eq!(graph.nodes[0].label, "read file");
        assert_eq!(graph.edges[0].label.as_deref(), Some("utf-8"));
    }

    #[test]
    fn duplicate_ids_are_rejected() {
        let document = serde_json::json!({
            "version": 1,
            "kind": "graph",
            "nodes": [ { "id": "a" }, { "id": "a" } ]
        });
        let input: GraphInput = serde_json::from_value(document).expect("valid document");
        let errors = GraphKind.validate(&input).expect_err("must fail");
        assert_eq!(errors[0].code, ErrorCode::DuplicateId);
        assert_eq!(errors[0].path, "nodes[1].id");
    }

    #[test]
    fn unknown_references_are_rejected() {
        let document = serde_json::json!({
            "version": 1,
            "kind": "graph",
            "nodes": [ { "id": "a" } ],
            "edges": [ { "from": "a", "to": "ghost" } ]
        });
        let input: GraphInput = serde_json::from_value(document).expect("valid document");
        let errors = GraphKind.validate(&input).expect_err("must fail");
        assert_eq!(errors[0].code, ErrorCode::UnknownRef);
        assert_eq!(errors[0].path, "edges[0].to");
    }

    #[test]
    fn unknown_fields_are_denied_by_the_schema() {
        let document = serde_json::json!({
            "version": 1,
            "kind": "graph",
            "nodes": [ { "id": "a", "color": "red" } ]
        });
        let error = serde_json::from_value::<GraphInput>(document).expect_err("must fail");
        assert!(error.to_string().contains("unknown field"), "{error}");
    }

    #[test]
    fn wrong_kind_is_rejected() {
        let document = serde_json::json!({ "version": 1, "kind": "er", "nodes": [] });
        let input: GraphInput = serde_json::from_value(document).expect("valid document");
        let errors = GraphKind.validate(&input).expect_err("must fail");
        assert_eq!(errors[0].code, ErrorCode::InvalidValue);
        assert_eq!(errors[0].path, "kind");
    }

    #[test]
    fn the_ir_is_a_graph() {
        let input: GraphInput = serde_json::from_str(MINIMAL).expect("valid document");
        assert!(matches!(GraphKind.to_ir(&input), Ir::Graph(_)));
    }
}
