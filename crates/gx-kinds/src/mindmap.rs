//! El kind `mindmap`: descomposición de una idea en ramas (PRD §6).
//!
//! Se dibuja con el motor `tree` —un mapa mental **es** un árbol— pero su
//! contrato no es el de un árbol de archivos: acá hay `topic` y `branches`, y
//! una rama puede pedir énfasis. Cambiar los nombres no es capricho: un agente
//! que manda un documento de archivos a un mapa mental se equivoca de género,
//! y el schema tiene que hacerlo evidente.

use gx_core::error::{ErrorCode, ValidationError};
use gx_core::ir::{Direction, Emphasis, Ir, IrEdge, IrNode, LayoutHint};
use gx_core::kind::{DiagramKind, KindLimits};
use schemars::JsonSchema;
use serde::Deserialize;

/// Contract version supported by this build (PRD §10.1).
pub const SUPPORTED_VERSION: u32 = 1;

/// Depth a mind map may reach: beyond this the drawing is a column of text.
pub const MAX_MINDMAP_DEPTH: usize = 12;

/// A topic of the map, with the branches it opens.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MindmapBranch {
    /// What this branch is about.
    pub topic: String,
    /// `highlight` la destaca, `muted` la apaga (PRD §11, campo común).
    #[serde(default)]
    pub emphasis: Option<Emphasis>,
    /// The branches this topic opens.
    #[serde(default)]
    pub branches: Vec<MindmapBranch>,
}

/// The JSON contract of the `mindmap` kind.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MindmapInput {
    /// Contract version. Only `1` is supported.
    pub version: u32,
    /// Must be `mindmap`.
    pub kind: String,
    /// Human readable title of the diagram.
    #[serde(default)]
    pub title: Option<String>,
    /// The topic at the center of the map.
    pub root: MindmapBranch,
}

/// The `mindmap` kind.
#[derive(Debug, Default)]
pub struct MindmapKind;

impl MindmapKind {
    /// Walks the map and reports every topic with its path and its depth.
    fn walk<'a>(
        branch: &'a MindmapBranch,
        path: &str,
        depth: usize,
        visit: &mut impl FnMut(&'a MindmapBranch, String, usize),
    ) {
        let path = if path.is_empty() {
            branch.topic.clone()
        } else {
            format!("{path}/{0}", branch.topic)
        };
        visit(branch, path.clone(), depth);
        for child in &branch.branches {
            Self::walk(child, &path, depth + 1, visit);
        }
    }
}

impl DiagramKind for MindmapKind {
    const KIND: &'static str = "mindmap";
    const LAYOUT_HINT: LayoutHint = LayoutHint::Tree;
    const LIMITS: KindLimits = KindLimits::GRAPH;

    type Input = MindmapInput;

    fn schema() -> schemars::Schema {
        schemars::schema_for!(MindmapInput)
    }

    fn validate(&self, input: &Self::Input) -> Result<(), Vec<ValidationError>> {
        let mut errors =
            crate::flow::validate_header(Self::KIND, &input.kind, SUPPORTED_VERSION, input.version);

        let mut count = 0usize;
        let mut too_deep = false;
        let mut empty: Option<String> = None;
        let mut duplicate: Option<(String, String)> = None;

        let mut check = |branch: &MindmapBranch, path: String, depth: usize| {
            count += 1;
            if depth >= MAX_MINDMAP_DEPTH {
                too_deep = true;
            }
            if branch.topic.trim().is_empty() && empty.is_none() {
                empty = Some(path.clone());
            }
            let mut seen: Vec<&str> = Vec::new();
            for child in &branch.branches {
                if seen.contains(&child.topic.as_str()) && duplicate.is_none() {
                    duplicate = Some((path.clone(), child.topic.clone()));
                }
                seen.push(child.topic.as_str());
            }
        };
        Self::walk(&input.root, "", 0, &mut check);

        if let Some(path) = empty {
            errors.push(ValidationError::new(
                path,
                ErrorCode::InvalidValue,
                "el tema de una rama no puede estar vacío",
                Some("poné de qué se trata esa rama".to_string()),
            ));
        }
        if let Some((path, topic)) = duplicate {
            let where_ = if path.is_empty() {
                "la raíz".to_string()
            } else {
                format!("`{path}`")
            };
            errors.push(ValidationError::new(
                path,
                ErrorCode::DuplicateId,
                format!("dos ramas de {where_} se llaman `{topic}`"),
                Some(
                    "las ramas hermanas necesitan temas distintos: el id sale de la ruta"
                        .to_string(),
                ),
            ));
        }
        if count > Self::LIMITS.max_nodes {
            errors.push(ValidationError::new(
                "root",
                ErrorCode::TooLarge,
                format!(
                    "{count} ramas superan el límite de {}",
                    Self::LIMITS.max_nodes
                ),
                Some("partí el mapa en varios diagramas".to_string()),
            ));
        }
        if too_deep {
            errors.push(ValidationError::new(
                "root",
                ErrorCode::InvalidValue,
                format!("el mapa pasa la profundidad máxima de {MAX_MINDMAP_DEPTH} niveles"),
                Some("agrupá las ramas más profundas".to_string()),
            ));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn to_ir(&self, input: &Self::Input) -> Ir {
        let mut nodes: Vec<IrNode> = Vec::new();
        let mut edges: Vec<IrEdge> = Vec::new();

        Self::walk(&input.root, "", 0, &mut |branch, path, _| {
            let mut node = IrNode::new(path.clone(), branch.topic.clone());
            node.size = gx_core::geom::Size::default();
            if let Some(emphasis) = branch.emphasis {
                node.emphasis = emphasis;
            }
            nodes.push(node);
        });

        for node in &nodes {
            if let Some((head, _)) = node.id.rsplit_once('/') {
                let mut edge = IrEdge::new(head.to_string(), node.id.clone());
                edge.arrow = false;
                edges.push(edge);
            }
        }

        Ir::Graph(crate::flow::ir_graph(
            Some(Direction::TB),
            Self::LAYOUT_HINT,
            nodes,
            edges,
            Vec::new(),
        ))
    }
}

/// Registers the kind in the global registry.
pub fn register() {
    gx_core::kind::register_kind::<MindmapKind>();
}

#[cfg(test)]
mod tests {
    use super::{MindmapInput, MindmapKind, MAX_MINDMAP_DEPTH};
    use gx_core::ir::{Emphasis, Ir};
    use gx_core::kind::DiagramKind;

    fn input(json: &str) -> MindmapInput {
        serde_json::from_str(json).expect("the document parses")
    }

    const MAP: &str = r#"{
      "version": 1, "kind": "mindmap", "title": "Graphium",
      "root": { "topic": "Graphium", "branches": [
        { "topic": "MCP", "emphasis": "highlight", "branches": [
          { "topic": "tools" }, { "topic": "harness" } ] },
        { "topic": "Plano", "branches": [ { "topic": "cámara" } ] }
      ] }
    }"#;

    #[test]
    fn a_mindmap_becomes_a_tree_with_emphasis() {
        let parsed = input(MAP);
        MindmapKind.validate(&parsed).expect("el mapa es válido");

        let Ir::Graph(graph) = MindmapKind.to_ir(&parsed) else {
            panic!("un mapa mental es un grafo");
        };

        assert_eq!(graph.layout_hint, gx_core::ir::LayoutHint::Tree);
        assert_eq!(graph.nodes.len(), 6);
        assert_eq!(graph.edges.len(), 5);
        let mcp = graph
            .nodes
            .iter()
            .find(|node| node.id == "Graphium/MCP")
            .expect("la rama destacada");
        assert_eq!(mcp.emphasis, Emphasis::Highlight);
        assert!(graph.edges.iter().all(|edge| !edge.arrow));
        assert!(graph
            .edges
            .iter()
            .any(|edge| edge.from == "Graphium/MCP" && edge.to == "Graphium/MCP/tools"));
    }

    #[test]
    fn two_sibling_branches_cannot_share_a_topic() {
        let parsed = input(
            r#"{ "version": 1, "kind": "mindmap", "root": { "topic": "raíz", "branches": [
                 { "topic": "A" }, { "topic": "A" } ] } }"#,
        );

        let errors = MindmapKind
            .validate(&parsed)
            .expect_err("las ramas hermanas chocan");

        assert_eq!(errors[0].code, gx_core::error::ErrorCode::DuplicateId);
        assert!(errors[0].message.contains("A"), "{:?}", errors[0]);
    }

    #[test]
    fn an_empty_topic_is_reported_with_its_path() {
        let parsed = input(
            r#"{ "version": 1, "kind": "mindmap", "root": { "topic": "raíz", "branches": [
                 { "topic": "  " } ] } }"#,
        );

        let errors = MindmapKind
            .validate(&parsed)
            .expect_err("el tema vacío no pasa");

        assert_eq!(errors[0].code, gx_core::error::ErrorCode::InvalidValue);
        assert!(errors[0].message.contains("vacío"));
    }

    #[test]
    fn a_map_that_is_too_deep_is_reported() {
        let mut branch = serde_json::json!({ "topic": "n0" });
        for level in 1..=MAX_MINDMAP_DEPTH {
            branch = serde_json::json!({ "topic": format!("n{level}"), "branches": [branch] });
        }
        let document = serde_json::json!({ "version": 1, "kind": "mindmap", "root": branch });
        let parsed: MindmapInput = serde_json::from_value(document).expect("parses");

        let errors = MindmapKind
            .validate(&parsed)
            .expect_err("la profundidad máxima se respeta");

        assert!(errors
            .iter()
            .any(|error| error.message.contains("profundidad")));
    }

    #[test]
    fn the_tree_contract_is_not_the_mindmap_one() {
        // Un documento de `tree` no entra por acá: los nombres del contrato
        // son distintos a propósito.
        let error = serde_json::from_str::<MindmapInput>(
            r#"{ "version": 1, "kind": "mindmap", "root": { "name": "src" } }"#,
        )
        .expect_err("`name` no es `topic`");

        assert!(error.to_string().contains("topic") || error.to_string().contains("missing field"));
    }
}
