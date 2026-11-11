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

use std::collections::BTreeMap;

use gx_core::error::{ErrorCode, ValidationError};
use gx_core::ir::{Direction, Ir, IrEdge, IrNode, IrValue, LayoutHint};
use gx_core::kind::{DiagramKind, KindLimits};
use schemars::JsonSchema;
use serde::Deserialize;

/// Contract version supported by this build (PRD §10.1).
pub const SUPPORTED_VERSION: u32 = 1;

/// Depth a tree may reach: beyond this the drawing is a column of text.
pub const MAX_TREE_DEPTH: usize = 12;

/// The keys of `meta` the kind derives from the path (`f27-etapa1.md`, S8).
/// A written `level` or `number` would fight the derived one, and there is no
/// precedence between them: the document is rejected instead.
pub const RESERVED_META_KEYS: [&str; 2] = ["level", "number"];

/// A node of the tree.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TreeNode {
    /// Name of the file, the directory or the module.
    pub name: String,
    /// Optional second line, e.g. a size or a one-line description.
    #[serde(default)]
    pub note: Option<String>,
    /// The free channel of the node (`f27-etapa1.md`, S6): the scalars the
    /// document writes reach `IrNode.meta` next to the `level` and the
    /// `number` this kind derives. Those two keys are derived, so writing them
    /// here is rejected (`f27-etapa1.md`, S8).
    #[serde(default, deserialize_with = "meta_of")]
    #[schemars(with = "Option<BTreeMap<String, serde_json::Value>>")]
    pub meta: Option<BTreeMap<String, IrValue>>,
    #[serde(default)]
    pub children: Vec<TreeNode>,
}

/// Reads the free channel of a node: the JSON scalars a document writes, in
/// the IR's `IrValue` so the channel is one type on both sides.
///
/// `IrValue` has five scalar variants and no list or object one, so a document
/// that writes a structure there hears it here instead of losing it.
fn meta_of<'de, D>(deserializer: D) -> Result<Option<BTreeMap<String, IrValue>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error as _;

    let Some(raw) = Option::<BTreeMap<String, serde_json::Value>>::deserialize(deserializer)?
    else {
        return Ok(None);
    };
    let mut meta = BTreeMap::new();
    for (key, value) in raw {
        let value = match value {
            serde_json::Value::String(text) => IrValue::Str(text),
            serde_json::Value::Bool(flag) => IrValue::Bool(flag),
            serde_json::Value::Number(number) => number
                .as_i64()
                .map(IrValue::Int)
                .or_else(|| number.as_f64().map(IrValue::Float))
                .ok_or_else(|| D::Error::custom(format!("`meta.{key}` no es un número")))?,
            serde_json::Value::Null => IrValue::Null,
            other => {
                return Err(D::Error::custom(format!(
                    "`meta.{key}` tiene que ser un escalar (texto, número, booleano o nulo), no {other}"
                )))
            }
        };
        meta.insert(key, value);
    }
    Ok(Some(meta))
}

/// Where a node sits in the document: what the walk knows and what the drawing
/// derives from it (`f27-etapa1.md`, D7).
#[derive(Debug)]
struct Place {
    /// `src/lib/render.rs`: the id the drawing uses.
    id: String,
    /// `root.children[1].children[0]`: the path an error names.
    doc: String,
    /// The sibling indices from the root as a dotted number: `1.2.2`.
    number: String,
    /// The depth, from 0.
    level: usize,
}

/// The JSON contract of the `tree` kind.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TreeInput {
    /// Contract version. Only `1` is supported; absent means `1`.
    #[serde(default = "crate::flow::default_version")]
    pub version: u32,
    /// Must be `tree`.
    pub kind: String,
    /// Human readable title of the diagram.
    #[serde(default)]
    pub title: Option<String>,
    /// Whether the derived number precedes the label's first line
    /// (`1.1 · nombre`). Absent or `false` leaves the drawing as it is
    /// (`f27-etapa1.md`, D7).
    #[serde(default)]
    pub numbering: Option<bool>,
    /// The root of the tree.
    pub root: TreeNode,
}

/// The `tree` kind.
#[derive(Debug, Default)]
pub struct TreeKind;

impl TreeKind {
    /// Walks the tree and reports every node with its place: the name composes
    /// the id, the sibling indices compose the number and the depth is the
    /// level (`f27-etapa1.md`, D7). The walk computes all three once; nothing
    /// downstream re-derives them.
    fn walk<'a>(
        node: &'a TreeNode,
        parent: &str,
        doc: &str,
        number: &str,
        level: usize,
        visit: &mut impl FnMut(&'a TreeNode, Place),
    ) {
        let id = if parent.is_empty() {
            node.name.clone()
        } else {
            format!("{parent}/{}", node.name)
        };
        visit(
            node,
            Place {
                id: id.clone(),
                doc: doc.to_string(),
                number: number.to_string(),
                level,
            },
        );
        for (index, child) in node.children.iter().enumerate() {
            Self::walk(
                child,
                &id,
                &format!("{doc}.children[{index}]"),
                &format!("{number}.{}", index + 1),
                level + 1,
                visit,
            );
        }
    }
}

impl DiagramKind for TreeKind {
    const KIND: &'static str = "tree";
    const PHASE: &'static str = "F2";
    const LAYOUT_HINT: LayoutHint = LayoutHint::Tree;
    const LIMITS: KindLimits = KindLimits::TREE;
    const KIND_VERSION: u32 = SUPPORTED_VERSION;

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
        let mut reserved: Vec<(String, &'static str)> = Vec::new();

        let mut check = |node: &TreeNode, place: Place| {
            count += 1;
            if place.level >= MAX_TREE_DEPTH {
                too_deep = true;
            }
            if node.name.trim().is_empty() && empty.is_none() {
                empty = Some((place.id.clone(), node.name.clone()));
            }
            // El nivel y el número salen de la ruta: una clave escrita no gana
            // por precedencia, así que el documento se rechaza (S8).
            if let Some(meta) = &node.meta {
                for &key in &RESERVED_META_KEYS {
                    if meta.contains_key(key) {
                        reserved.push((format!("{}.meta.{key}", place.doc), key));
                    }
                }
            }
            // Two siblings with the same name would produce the same id: el
            // documento tiene que elegir un nombre, no repetirlo.
            let mut seen: Vec<&str> = Vec::new();
            for child in &node.children {
                if seen.contains(&child.name.as_str()) && duplicate.is_none() {
                    duplicate = Some((place.id.clone(), child.name.clone()));
                }
                seen.push(child.name.as_str());
            }
        };
        Self::walk(&input.root, "", "root", "1", 0, &mut check);

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
        for (path, key) in reserved {
            errors.push(ValidationError::new(
                path,
                ErrorCode::InvalidValue,
                format!("`{key}` se deriva de la ruta del nodo: no se escribe en `meta`"),
                Some(format!(
                    "sacá `{key}` de `meta`: el nivel sale de la profundidad y el número de la posición entre los hermanos, y el valor derivado no negocia con el escrito"
                )),
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
        let numbering = input.numbering.unwrap_or(false);

        Self::walk(&input.root, "", "root", "1", 0, &mut |node, place| {
            let label = match &node.note {
                Some(note) if !note.trim().is_empty() => format!("{}\n{}", node.name, note),
                _ => node.name.clone(),
            };
            // El número precede la primera línea de la etiqueta (el nombre); la
            // nota sigue en la segunda (`f27-etapa1.md`, D7).
            let label = if numbering {
                format!("{} · {label}", place.number)
            } else {
                label
            };
            let mut ir_node = IrNode::new(place.id.clone(), label);
            ir_node.size = gx_core::geom::Size::default();
            // La meta del documento viaja intacta; las claves derivadas se
            // escriben después porque son del kind, y un documento que las
            // declare no llega hasta acá (S8).
            if let Some(meta) = &node.meta {
                for (key, value) in meta {
                    ir_node.meta.insert(key.clone(), value.clone());
                }
            }
            ir_node
                .meta
                .insert("level".to_string(), IrValue::Int(place.level as i64));
            ir_node
                .meta
                .insert("number".to_string(), IrValue::Str(place.number));
            nodes.push(ir_node);
            parent_of.push((place.id, None));
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
                edge.head = None;
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

    /// La `tree` dibuja un árbol de nombres: el canal libre de cada nodo, el
    /// nivel y el número que el kind deriva, y cuándo el número se dibuja
    /// (`f27-etapa1.md`, S6/D7).
    fn rules(&self) -> Vec<String> {
        vec![
            "Cada nodo puede llevar su propia `meta` —un mapa de escalares—: viaja intacta al dibujo."
                .to_string(),
            "El nivel y el número de cada nodo se derivan de su ruta: `level` es la profundidad desde 0 y `number` sale de la posición entre los hermanos (`1`, `1.1`, `1.1.2`). Escribirlos en `meta` se rechaza: no hay precedencia entre el valor derivado y el escrito."
                .to_string(),
            "Con `numbering: true` el número precede a la primera línea de la etiqueta (`1.1 · nombre`); sin el campo, el árbol se dibuja como siempre."
                .to_string(),
        ]
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
    use gx_core::ir::{Ir, IrValue};
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
        assert!(graph.edges.iter().all(|edge| edge.head.is_none()));
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

    /// S8: `level` y `number` se derivan de la ruta, así que escribirlos en
    /// `meta` no gana por precedencia —el documento se rechaza nombrando la
    /// clave y la ruta exacta— (`f27-etapa1.md`, S8).
    #[test]
    fn the_derived_keys_cannot_be_written_in_meta() {
        let parsed = input(
            r#"{ "version": 1, "kind": "tree", "root": {
                 "name": "src", "meta": { "level": 1, "owner": "core" },
                 "children": [
                   { "name": "main.rs" },
                   { "name": "lib", "meta": { "number": "7" } } ] } }"#,
        );

        let errors = TreeKind
            .validate(&parsed)
            .expect_err("las claves derivadas no se escriben");

        assert_eq!(errors.len(), 2, "{errors:?}");
        assert_eq!(errors[0].code, gx_core::error::ErrorCode::InvalidValue);
        assert_eq!(errors[0].path, "root.meta.level");
        assert!(errors[0].message.contains("level"), "{:?}", errors[0]);
        let hint = errors[0].hint.as_deref().unwrap_or_default();
        assert!(hint.contains("deriva"), "el hint dice por qué: {hint}");
        assert_eq!(errors[1].code, gx_core::error::ErrorCode::InvalidValue);
        assert_eq!(errors[1].path, "root.children[1].meta.number");
        assert!(errors[1].message.contains("number"), "{:?}", errors[1]);
    }

    /// D7: el nivel es la profundidad desde 0 y el número sale de los índices
    /// de los hermanos; los dos llegan al `meta` de la IR sin que el documento
    /// los escriba (`f27-etapa1.md`, S6/D7).
    #[test]
    fn the_level_and_the_number_of_a_node_are_derived_from_its_path() {
        let parsed = input(SRC);

        let Ir::Graph(graph) = TreeKind.to_ir(&parsed) else {
            panic!("un árbol es un grafo");
        };
        let meta = |id: &str| {
            graph
                .nodes
                .iter()
                .find(|node| node.id == id)
                .unwrap_or_else(|| panic!("el nodo `{id}`"))
                .meta
                .clone()
        };

        assert_eq!(meta("src").get("level"), Some(&IrValue::Int(0)));
        assert_eq!(meta("src").get("number"), Some(&IrValue::Str("1".into())));
        assert_eq!(meta("src/main.rs").get("level"), Some(&IrValue::Int(1)));
        assert_eq!(
            meta("src/main.rs").get("number"),
            Some(&IrValue::Str("1.1".into()))
        );
        assert_eq!(meta("src/lib").get("level"), Some(&IrValue::Int(1)));
        assert_eq!(
            meta("src/lib").get("number"),
            Some(&IrValue::Str("1.2".into()))
        );
        assert_eq!(
            meta("src/lib/render.rs").get("level"),
            Some(&IrValue::Int(2))
        );
        assert_eq!(
            meta("src/lib/render.rs").get("number"),
            Some(&IrValue::Str("1.2.2".into()))
        );
    }

    /// D7: `numbering: true` antepone el número a la primera línea de la
    /// etiqueta; sin el campo (o con `false`) la etiqueta es la de siempre, que
    /// es lo que mantiene los bytes de los dibujos ya fijados.
    #[test]
    fn numbering_prefixes_the_label_and_its_absence_keeps_the_bare_name() {
        let document = r#"{ "version": 1, "kind": "tree"%NUMBERING%,
             "root": { "name": "src", "children": [
               { "name": "lib", "note": "312 líneas",
                 "children": [ { "name": "mod.rs" } ] } ] } }"#;
        let parse =
            |numbering: &str| input::<TreeInput>(&document.replace("%NUMBERING%", numbering));
        let label = |parsed: &TreeInput, id: &str| {
            let Ir::Graph(graph) = TreeKind.to_ir(parsed) else {
                panic!("un árbol es un grafo");
            };
            graph
                .nodes
                .into_iter()
                .find(|node| node.id == id)
                .unwrap_or_else(|| panic!("el nodo `{id}`"))
                .label
        };

        let numbered = parse(", \"numbering\": true");
        TreeKind
            .validate(&numbered)
            .expect("el documento es válido");
        // El número precede la primera línea: la nota sigue en la segunda.
        assert_eq!(label(&numbered, "src"), "1 · src");
        assert_eq!(label(&numbered, "src/lib"), "1.1 · lib\n312 líneas");
        assert_eq!(label(&numbered, "src/lib/mod.rs"), "1.1.1 · mod.rs");

        // Sin el campo y con `false` la etiqueta es la de hoy: el nombre, y la
        // nota en su segunda línea.
        for absent in ["", ", \"numbering\": false"] {
            let plain = parse(absent);
            assert_eq!(label(&plain, "src"), "src", "con `{absent}`");
            assert_eq!(
                label(&plain, "src/lib"),
                "lib\n312 líneas",
                "con `{absent}`"
            );
        }

        // El schema publica el campo, así que el schema es el contrato.
        let schema = TreeKind::schema().to_value().to_string();
        assert!(
            schema.contains("numbering"),
            "el esquema no nombra `numbering`"
        );
    }

    /// S6: el canal `meta` del nodo existe y llega intacto a la IR, junto a las
    /// claves derivadas (`f27-etapa1.md`, S6/D7).
    #[test]
    fn the_metadata_of_a_node_reaches_the_ir() {
        let parsed = input(
            r#"{ "version": 1, "kind": "tree", "root": {
                 "name": "src",
                 "meta": { "owner": "core", "size": 312, "public": true, "sin": null } } }"#,
        );

        TreeKind.validate(&parsed).expect("la meta libre pasa");
        let Ir::Graph(graph) = TreeKind.to_ir(&parsed) else {
            panic!("un árbol es un grafo");
        };
        let node = &graph.nodes[0];

        assert_eq!(node.meta.get("owner"), Some(&IrValue::Str("core".into())));
        assert_eq!(node.meta.get("size"), Some(&IrValue::Int(312)));
        assert_eq!(node.meta.get("public"), Some(&IrValue::Bool(true)));
        assert_eq!(node.meta.get("sin"), Some(&IrValue::Null));
        // Las claves derivadas conviven con las del documento.
        assert_eq!(node.meta.get("level"), Some(&IrValue::Int(0)));
        assert_eq!(node.meta.get("number"), Some(&IrValue::Str("1".into())));

        // El schema publica el canal, así que el schema es el contrato.
        let schema = TreeKind::schema().to_value().to_string();
        assert!(schema.contains("meta"), "el esquema no nombra la meta");
    }
}
