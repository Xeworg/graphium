//! The `tree` kind: un árbol de archivos o de módulos (PRD §6).
//!
//! Receta de un kind (PRD §6.2): schema con `deny_unknown_fields`, reglas
//! semánticas para lo que JSON Schema no puede expresar, traducción a la IR y
//! una línea en el registro. `gx-mcp` y `gx-app` no cambian.
//!
//! El documento es un **árbol anidado**, no una lista de nodos y aristas: es
//! lo que un agente escribe sin repetir rutas y lo que un humano lee. La
//! traducción inventa los ids a partir de la ruta (`src/main.rs`), así el
//! dibujo es determinista y una misma rama siempre tiene el mismo id.

use gx_core::error::{ErrorCode, ValidationError};
use gx_core::ir::{Direction, Ir, IrEdge, IrNode, LayoutHint};
use gx_core::kind::{DiagramKind, KindLimits};
use schemars::JsonSchema;
use serde::Deserialize;

/// Contract version supported by this build (PRD §10.1).
pub const SUPPORTED_VERSION: u32 = 1;

/// Depth a tree may reach: beyond this the drawing is a column of text.
pub const MAX_TREE_DEPTH: usize = 12;

/// A node of the tree.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TreeNode {
    /// Name of the file, the directory or the module.
    pub name: String,
    /// Optional second line, e.g. a size or a one-line description.
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub children: Vec<TreeNode>,
}

/// The JSON contract of the `tree` kind.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TreeInput {
    /// Contract version. Only `1` is supported.
    pub version: u32,
    /// Must be `tree`.
    pub kind: String,
    /// Human readable title of the diagram.
    #[serde(default)]
    pub title: Option<String>,
    /// The root of the tree.
    pub root: TreeNode,
}

/// The `tree` kind.
#[derive(Debug, Default)]
pub struct TreeKind;

impl TreeKind {
    /// Walks the tree and reports every node with its path and its depth.
    fn walk<'a>(
        node: &'a TreeNode,
        path: &str,
        depth: usize,
        visit: &mut impl FnMut(&'a TreeNode, String, usize),
    ) {
        let path = if path.is_empty() {
            node.name.clone()
        } else {
            format!("{path}/{}", node.name)
        };
        visit(node, path.clone(), depth);
        for child in &node.children {
            Self::walk(child, &path, depth + 1, visit);
        }
    }
}

impl DiagramKind for TreeKind {
    const KIND: &'static str = "tree";
    const PHASE: &'static str = "F2";
    const LAYOUT_HINT: LayoutHint = LayoutHint::Tree;
    const LIMITS: KindLimits = KindLimits::TREE;

    type Input = TreeInput;

    fn schema() -> schemars::Schema {
        schemars::schema_for!(TreeInput)
    }

    fn validate(&self, input: &Self::Input) -> Result<(), Vec<ValidationError>> {
        let mut errors =
            crate::flow::validate_header(Self::KIND, &input.kind, SUPPORTED_VERSION, input.version);

        let mut count = 0usize;
        let mut too_deep = false;
        let mut empty: Option<(String, String)> = None;
        let mut duplicate: Option<(String, String)> = None;

        let mut check = |node: &TreeNode, path: String, depth: usize| {
            count += 1;
            if depth >= MAX_TREE_DEPTH {
                too_deep = true;
            }
            if node.name.trim().is_empty() && empty.is_none() {
                empty = Some((path.clone(), node.name.clone()));
            }
            // Two siblings with the same name would produce the same id: el
            // documento tiene que elegir un nombre, no repetirlo.
            let mut seen: Vec<&str> = Vec::new();
            for child in &node.children {
                if seen.contains(&child.name.as_str()) && duplicate.is_none() {
                    duplicate = Some((path.clone(), child.name.clone()));
                }
                seen.push(child.name.as_str());
            }
        };
        Self::walk(&input.root, "", 0, &mut check);

        if let Some((path, _)) = empty {
            errors.push(ValidationError::new(
                path,
                ErrorCode::InvalidValue,
                "el nombre de un nodo del árbol no puede estar vacío",
                Some("poné el nombre del archivo, del directorio o del módulo".to_string()),
            ));
        }
        if let Some((path, name)) = duplicate {
            let where_ = if path.is_empty() {
                "la raíz".to_string()
            } else {
                format!("`{path}`")
            };
            errors.push(ValidationError::new(
                path,
                ErrorCode::DuplicateId,
                format!("dos hijos de {where_} se llaman `{name}`"),
                Some("los hermanos necesitan nombres distintos: el id sale de la ruta".to_string()),
            ));
        }
        if count > Self::LIMITS.max_nodes {
            errors.push(ValidationError::new(
                "root",
                ErrorCode::TooLarge,
                format!(
                    "{count} nodos superan el límite de {}",
                    Self::LIMITS.max_nodes
                ),
                Some("partí el árbol en varios diagramas".to_string()),
            ));
        }
        if too_deep {
            errors.push(ValidationError::new(
                "root",
                ErrorCode::InvalidValue,
                format!("el árbol pasa la profundidad máxima de {MAX_TREE_DEPTH} niveles"),
                Some("aplaná las ramas más profundas".to_string()),
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
        let mut parent_of: Vec<(String, Option<String>)> = Vec::new();

        Self::walk(&input.root, "", 0, &mut |node, path, _| {
            let label = match &node.note {
                Some(note) if !note.trim().is_empty() => format!("{}\n{}", node.name, note),
                _ => node.name.clone(),
            };
            let mut ir_node = IrNode::new(path.clone(), label);
            ir_node.size = gx_core::geom::Size::default();
            nodes.push(ir_node);
            parent_of.push((path, None));
        });

        // The parent of a node is its path without the last segment: the walk
        // already produced the paths, so the tree is rebuilt from them instead
        // of being walked twice.
        for (path, parent) in parent_of.iter_mut() {
            if let Some((head, _)) = path.rsplit_once('/') {
                *parent = Some(head.to_string());
            }
        }
        for (path, parent) in &parent_of {
            if let Some(parent) = parent {
                let mut edge = IrEdge::new(parent.clone(), path.clone());
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

    fn min_example() -> serde_json::Value {
        serde_json::json!({
            "version": 1,
            "kind": "tree",
            "title": "src",
            "root": {
                "name": "src",
                "children": [
                    { "name": "main.rs" },
                    { "name": "lib", "children": [ { "name": "mod.rs" } ] }
                ]
            }
        })
    }
}

/// Registers the kind in the global registry.
pub fn register() {
    gx_core::kind::register_kind::<TreeKind>();
}

#[cfg(test)]
mod tests {
    use super::{TreeInput, TreeKind, MAX_TREE_DEPTH};
    use crate::test_support::input;
    use gx_core::ir::Ir;
    use gx_core::kind::DiagramKind;

    const SRC: &str = r#"{
      "version": 1, "kind": "tree", "title": "src",
      "root": { "name": "src", "children": [
        { "name": "main.rs" },
        { "name": "lib", "children": [ { "name": "mod.rs" }, { "name": "render.rs", "note": "312 líneas" } ] }
      ] }
    }"#;

    /// Un árbol válido se traduce a un grafo con el hint de árbol y una arista
    /// por padre e hijo.
    #[test]
    fn a_tree_becomes_a_graph_with_parent_child_edges() {
        let parsed = input(SRC);
        TreeKind.validate(&parsed).expect("el árbol es válido");

        let Ir::Graph(graph) = TreeKind.to_ir(&parsed) else {
            panic!("un árbol es un grafo");
        };

        assert_eq!(graph.layout_hint, gx_core::ir::LayoutHint::Tree);
        assert_eq!(graph.nodes.len(), 5);
        assert_eq!(graph.edges.len(), 4);
        let ids: Vec<&str> = graph.nodes.iter().map(|node| node.id.as_str()).collect();
        assert!(ids.contains(&"src"), "{ids:?}");
        assert!(ids.contains(&"src/lib/render.rs"), "{ids:?}");
        // La nota viaja como segunda línea del label.
        let render = graph
            .nodes
            .iter()
            .find(|node| node.id == "src/lib/render.rs")
            .expect("la hoja");
        assert!(render.label.contains("render.rs"));
        assert!(render.label.contains("312"));
        // Las aristas unen padre e hijo y no llevan flecha.
        assert!(graph
            .edges
            .iter()
            .any(|edge| edge.from == "src/lib" && edge.to == "src/lib/render.rs"));
        assert!(graph.edges.iter().all(|edge| !edge.arrow));
    }

    #[test]
    fn two_siblings_cannot_share_a_name() {
        let parsed = input(
            r#"{ "version": 1, "kind": "tree", "root": { "name": "src", "children": [
                 { "name": "mod.rs" }, { "name": "mod.rs" } ] } }"#,
        );

        let errors = TreeKind.validate(&parsed).expect_err("los hermanos chocan");

        assert_eq!(errors[0].code, gx_core::error::ErrorCode::DuplicateId);
        assert!(errors[0].message.contains("mod.rs"), "{:?}", errors[0]);
        assert!(errors[0].hint.is_some());
    }

    #[test]
    fn an_empty_name_is_reported_with_its_path() {
        let parsed = input(
            r#"{ "version": 1, "kind": "tree", "root": { "name": "src", "children": [
                 { "name": " " } ] } }"#,
        );

        let errors = TreeKind
            .validate(&parsed)
            .expect_err("el nombre vacío no pasa");

        assert_eq!(errors[0].code, gx_core::error::ErrorCode::InvalidValue);
        assert!(errors[0].message.contains("vacío"));
    }

    #[test]
    fn a_tree_that_is_too_deep_is_reported() {
        // Una cadena de MAX_TREE_DEPTH + 1 niveles: la raíz más un hijo por
        // nivel, que es el caso que el límite tiene que cortar.
        let mut node = serde_json::json!({ "name": "n0" });
        for level in 1..=MAX_TREE_DEPTH {
            node = serde_json::json!({ "name": format!("n{level}"), "children": [node] });
        }
        let document = serde_json::json!({ "version": 1, "kind": "tree", "root": node });
        let parsed: TreeInput = serde_json::from_value(document).expect("the document parses");

        let errors = TreeKind
            .validate(&parsed)
            .expect_err("la profundidad máxima se respeta");

        assert!(errors
            .iter()
            .any(|error| error.message.contains("profundidad")));
    }

    #[test]
    fn an_unknown_field_is_rejected() {
        let error = serde_json::from_str::<TreeInput>(
            r#"{ "version": 1, "kind": "tree", "root": { "name": "a", "colour": "red" } }"#,
        )
        .expect_err("el schema cierra los campos");

        assert!(error.to_string().contains("colour"), "{error}");
    }
}
