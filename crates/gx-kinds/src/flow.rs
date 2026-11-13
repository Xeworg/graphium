//! Lo que comparten los kinds de flujo: nodos con un rol que decide su forma,
//! aristas con su etiqueta —y, cuando el kind declara vocabulario, con la
//! relación tipada que escriben— y las reglas que JSON Schema no puede expresar
//! (PRD §6, `flowchart`, `state`, `dataflow`).
//!
//! Los tres kinds se dibujan con el motor `layered` y no necesitan formas
//! nuevas: lo que los distingue es el **vocabulario** —un paso, un estado, un
//! depósito de datos— y eso se dice con el `role` del nodo, que el kind valida
//! contra su propia lista. Agregar un rol es agregar una línea en el kind, no
//! tocar el motor ni el renderer (O7).

use gx_core::error::{ErrorCode, ValidationError};
use gx_core::ir::{
    Adornment, Direction, Emphasis, IrEdge, IrGraph, IrGroup, IrNode, LayoutHint, Shape, Style,
};
use gx_core::kind::{EdgeKind, Role};
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
    /// The word drawn above the name, between «» (`f27-etapa1.md`, S3): the
    /// stereotype is node vocabulary, not an edge slot, so any kind that draws
    /// a flow box can declare one. The engine already reserves the line, the
    /// renderer already writes it and the JS twin already paints it, which is
    /// why this field cost no drawing work.
    #[serde(default)]
    pub stereotype: Option<String>,
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
    /// The relation the line draws (`f29-componente.md`, S5): the name of a row
    /// of the kind's table. When written it supersedes `arrow` and `style` —the
    /// style, the two glyphs and the implied label come from the table and from
    /// nowhere else—; when absent the edge is the document's own line, which is
    /// the document of every drawing written before this field existed.
    #[serde(default)]
    pub kind: Option<String>,
}

/// The shape of a role, or the one of the default role. The kinds whose
/// events carry roles (`timeline`, `f22-dibujo.md` S6) resolve their shape
/// with this same table, so a role means one shape across the catalog.
pub fn shape_of(roles: &[Role], role: &str) -> Shape {
    roles
        .iter()
        .find(|candidate| candidate.name == role)
        .map(|candidate| candidate.shape)
        .unwrap_or(Shape::Rounded)
}

/// The shapes a role table draws, in table order, without repeats
/// (`f19-formas.md`, S4): the truth of the kind, derived once.
pub fn shapes_of_roles(roles: &[Role]) -> Vec<Shape> {
    let mut shapes: Vec<Shape> = Vec::new();
    for role in roles {
        if !shapes.contains(&role.shape) {
            shapes.push(role.shape);
        }
    }
    shapes
}

/// El vocabulario de un kind de flujo (`f29-componente.md`, D2): los roles que
/// admite, el rol por defecto y la tabla de relaciones. Los tres viajan juntos
/// porque el kind que adopta el campo `kind` también tiene roles, y separarlos
/// repetía la misma lista de parámetros en cada llamada.
#[derive(Debug, Clone, Copy)]
pub struct FlowVocabulary<'a> {
    /// The roles the kind admits, in the order its rules teach them.
    pub roles: &'a [Role],
    /// The role a node takes when the document declares none.
    pub default_role: &'a str,
    /// The relations the kind admits. Empty when the kind declares no
    /// vocabulary, which S6 turns into the rejection of any `kind` an edge
    /// writes.
    pub edge_kinds: &'a [EdgeKind],
}

/// Validates a flow document: the rules JSON Schema cannot express.
///
/// `vocabulary` is the kind's own: the roles it admits, the one used when a
/// node does not declare any and its relation table (`f29-componente.md`, D2).
/// Every flow kind walks this one path, and a kind without a relation table
/// passes an empty slice, which S6 turns into the rejection of any `kind` an
/// edge writes. The errors name the field the agent has to fix and, when the
/// vocabulary is the problem, list what the kind admits.
pub fn validate_flow(
    kind: &str,
    nodes: &[FlowNode],
    edges: &[FlowEdge],
    vocabulary: &FlowVocabulary,
) -> Vec<ValidationError> {
    let pairs: Vec<(&str, &str)> = edges
        .iter()
        .map(|edge| (edge.from.as_str(), edge.to.as_str()))
        .collect();
    let mut errors = validate_nodes(kind, nodes, vocabulary.roles, vocabulary.default_role);
    errors.extend(validate_refs(&pairs, nodes));
    errors.extend(validate_edge_kinds(kind, edges, vocabulary.edge_kinds));
    errors
}

/// Validates the relations the edges write (`f29-componente.md`, S5 and S6).
///
/// An edge that names a relation the kind's table does not declare is
/// `InvalidEnum` at `edges[i].kind`, with the names the table admits. A kind
/// whose table is empty declares no relation vocabulary at all, so writing
/// `kind` there is rejected instead of accepted and ignored: a field nobody can
/// use is a field that lies (S6).
pub fn validate_edge_kinds(
    kind: &str,
    edges: &[FlowEdge],
    edge_kinds: &[EdgeKind],
) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    for (index, edge) in edges.iter().enumerate() {
        let Some(written) = edge.kind.as_deref().map(str::trim) else {
            continue;
        };
        if edge_kinds.is_empty() {
            errors.push(ValidationError::new(
                format!("edges[{index}].kind"),
                ErrorCode::InvalidEnum,
                format!("`{kind}` no declara vocabulario de relación: `kind` no se puede escribir"),
                Some(
                    "quitá `kind` de la arista: el estilo, el glifo y la etiqueta son los del documento"
                        .to_string(),
                ),
            ));
            continue;
        }
        if EdgeKind::find(edge_kinds, written).is_none() {
            errors.push(ValidationError::new(
                format!("edges[{index}].kind"),
                ErrorCode::InvalidEnum,
                format!("relación desconocida en `{kind}`: `{written}`"),
                Some(format!(
                    "las relaciones de este kind son {}",
                    EdgeKind::names(edge_kinds).join(", ")
                )),
            ));
        }
    }
    errors
}

/// The longest stereotype a flow box draws above its name (`f27-etapa1.md`,
/// D3): the line is reserved at 0.85× the text size, so it holds a word and
/// not a sentence.
pub const MAX_STEREOTYPE_CHARS: usize = 32;

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
                Some(format!(
                    "escribí un id único, por ejemplo `nodo{}`",
                    if index == 0 { "" } else { "2" }
                )),
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

        // El estereotipo es una palabra sobre el nombre, no un párrafo
        // (`f27-etapa1.md`, D3): la línea que el motor le reserva mide
        // `text_size * 0.85` y se parte con el ancho de la caja.
        if let Some(stereotype) = &node.stereotype {
            let trimmed = stereotype.trim();
            if trimmed.is_empty() {
                errors.push(ValidationError::new(
                    format!("nodes[{index}].stereotype"),
                    ErrorCode::InvalidValue,
                    "el estereotipo del nodo no puede estar en blanco".to_string(),
                    Some("un estereotipo en blanco dibujaría «» sin nada adentro".to_string()),
                ));
            } else if trimmed.chars().count() > MAX_STEREOTYPE_CHARS {
                errors.push(ValidationError::new(
                    format!("nodes[{index}].stereotype"),
                    ErrorCode::InvalidValue,
                    format!(
                        "el estereotipo `{trimmed}` tiene {} caracteres y el máximo es {MAX_STEREOTYPE_CHARS}",
                        trimmed.chars().count()
                    ),
                    Some("el estereotipo va entre «» sobre el nombre: escribí una palabra, no una frase".to_string()),
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

/// The version every document means when it does not say one
/// (`f23-contrato.md`, S5): there is exactly one legal value today, so the
/// field stopped being boilerplate.
pub fn default_version() -> u32 {
    1
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
            ErrorCode::UnsupportedVersion,
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
            // El estereotipo viaja tal cual: el motor, el renderer, el CSS y el
            // gemelo JS ya lo dibujan desde `class` (`f27-etapa1.md`, D3).
            ir_node.stereotype = node
                .stereotype
                .as_deref()
                .map(str::trim)
                .filter(|word| !word.is_empty())
                .map(str::to_string);
            ir_node
        })
        .collect()
}

/// Translates a validated flow document to the layered IR.
///
/// `vocabulary` is the same one `validate_flow` reads (`f29-componente.md`,
/// D2), so an edge is validated and painted by the same rows: the relation
/// table decides the line or the document is rejected.
pub fn to_graph(
    nodes: &[FlowNode],
    edges: &[FlowEdge],
    vocabulary: &FlowVocabulary,
    direction: Option<Direction>,
    layout_hint: LayoutHint,
    groups: Vec<IrGroup>,
) -> IrGraph {
    let nodes = to_ir_nodes(nodes, vocabulary.roles, vocabulary.default_role);
    let edges: Vec<IrEdge> = edges
        .iter()
        .map(|edge| to_ir_edge(edge, vocabulary.edge_kinds))
        .collect();

    ir_graph(direction, layout_hint, nodes, edges, groups)
}

/// Translates one edge, resolving the relation it writes
/// (`f29-componente.md`, S5).
///
/// A written `kind` decides the style, the two glyphs and the implied label
/// from the kind's table and from nowhere else, so an explicit `arrow` or
/// `style` is not consulted: the table is the contract and the drawing follows
/// it. Without a relation the edge is the document's own line —the `arrow` of
/// always, its style and its label—, which is the document every drawing
/// written before this field existed means (S4).
pub fn to_ir_edge(edge: &FlowEdge, edge_kinds: &[EdgeKind]) -> IrEdge {
    let mut ir_edge = IrEdge::new(edge.from.clone(), edge.to.clone());
    ir_edge.label = edge.label.clone().filter(|label| !label.trim().is_empty());
    if let Some(emphasis) = edge.emphasis {
        ir_edge.emphasis = emphasis;
    }

    if let Some(relation) = edge
        .kind
        .as_deref()
        .map(str::trim)
        .and_then(|name| EdgeKind::find(edge_kinds, name))
    {
        // La palabra es el único hueco que el documento puede llenar: un
        // override de estilo o de glifo volvería a pintar una relación con una
        // notación que su fila no declara.
        ir_edge.label = ir_edge.label.or_else(|| relation.label.map(str::to_string));
        ir_edge.style = relation.style;
        ir_edge.head = relation.head;
        ir_edge.tail = relation.tail;
        return ir_edge;
    }

    if let Some(style) = edge.style {
        ir_edge.style = style;
    }
    if let Some(arrow) = edge.arrow {
        ir_edge.head = arrow.then_some(Adornment::Arrow);
    }
    ir_edge
}

/// The rule that teaches the relation vocabulary, derived from the table
/// (`f26-adornos.md`, S8): a relation can never be painted with a style or a
/// glyph the contract does not mention. `field` is the name of the document
/// field the vocabulary lives in, and `default` names the row a document that
/// writes nothing gets, when the kind has one.
pub fn relations_rule(field: &str, edge_kinds: &[EdgeKind], default: Option<&str>) -> String {
    if edge_kinds.is_empty() {
        return String::new();
    }
    let rows: Vec<String> = edge_kinds
        .iter()
        .map(|relation| {
            let mut words = relation.style_word().to_string();
            match relation.head_word() {
                Some(glyph) => {
                    words.push_str(" con ");
                    words.push_str(glyph);
                }
                None => words.push_str(" sin adorno"),
            }
            if let Some(glyph) = relation.tail_word() {
                words.push_str(" y ");
                words.push_str(glyph);
                words.push_str(" en el origen");
            }
            // Whether the relation names the line is a fact of the row, like
            // its style: the table says it, the rule repeats it.
            words.push_str(match relation.label {
                Some(_) => ", se nombra",
                None => ", no se nombra",
            });
            if Some(relation.name) == default {
                words.push_str(", por defecto");
            }
            format!("`{}` ({words})", relation.name)
        })
        .collect();
    format!("`{field}` decide la línea: {}.", rows.join(", "))
}

/// The one `IrGraph` assembly (`f20-reuso.md`, S6): the direction with its
/// default, the hint of the kind, and the pieces the kind mapped. Ten kinds
/// ended here — nine of them carried the same seven lines by hand.
pub fn ir_graph(
    direction: Option<Direction>,
    layout_hint: LayoutHint,
    nodes: Vec<IrNode>,
    edges: Vec<IrEdge>,
    groups: Vec<IrGroup>,
) -> IrGraph {
    IrGraph {
        direction: direction.unwrap_or(Direction::TB),
        layout_hint,
        nodes,
        edges,
        groups,
    }
}

#[cfg(test)]
mod tests {
    use super::{to_graph, validate_flow, FlowEdge, FlowNode, FlowVocabulary};
    use gx_core::error::ErrorCode;
    use gx_core::ir::{Adornment, LayoutHint, Shape, Style};
    use gx_core::kind::{EdgeKind, Role};

    /// La tabla de un kind de prueba (`f29-componente.md`, S5): una acción
    /// dirigida, un contacto simétrico, una observación y una fila con glifo en
    /// el origen, que es el único caso que prueba que el `tail` sale de la tabla.
    const TABLE: [EdgeKind; 4] = [
        EdgeKind::new(
            "calls",
            Style::Solid,
            Some(Adornment::Arrow),
            false,
            Some("calls"),
        ),
        EdgeKind::new("uses", Style::Solid, None, false, Some("uses")),
        EdgeKind::new(
            "subscribes",
            Style::Dashed,
            Some(Adornment::OpenArrow),
            false,
            Some("subscribes"),
        ),
        EdgeKind {
            name: "hosts",
            style: Style::Solid,
            head: Some(Adornment::Arrow),
            tail: Some(Adornment::Diamond),
            specialization: false,
            label: Some("hosts"),
        },
    ];

    const ROLES: [Role; 1] = [Role::new("component", Shape::Rounded)];

    fn node(id: &str) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            label: None,
            role: None,
            emphasis: None,
            style: None,
            stereotype: None,
        }
    }

    fn edge(from: &str, to: &str) -> FlowEdge {
        FlowEdge {
            from: from.to_string(),
            to: to.to_string(),
            label: None,
            emphasis: None,
            style: None,
            arrow: None,
            kind: None,
        }
    }

    fn vocabulary(edge_kinds: &[EdgeKind]) -> FlowVocabulary<'_> {
        FlowVocabulary {
            roles: &ROLES,
            default_role: "component",
            edge_kinds,
        }
    }

    fn graph_of(edges: &[FlowEdge], table: &[EdgeKind]) -> gx_core::ir::IrGraph {
        to_graph(
            &[node("a"), node("b")],
            edges,
            &vocabulary(table),
            None,
            LayoutHint::Layered,
            Vec::new(),
        )
    }

    /// S5: un `kind` que la tabla no declara se reporta en `edges[i].kind` con
    /// los nombres que la tabla admite, para que el agente arregle el campo sin
    /// adivinar.
    #[test]
    fn an_undeclared_relation_is_rejected_with_its_path_and_the_names() {
        let mut bad = edge("a", "b");
        bad.kind = Some("borrows".to_string());

        let errors = validate_flow(
            "component",
            &[node("a"), node("b")],
            &[bad],
            &vocabulary(&TABLE),
        );

        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code, ErrorCode::InvalidEnum);
        assert_eq!(errors[0].path, "edges[0].kind");
        let hint = errors[0].hint.as_deref().expect("la lista de los nombres");
        for name in EdgeKind::names(&TABLE) {
            assert!(hint.contains(name), "el hint no nombra `{name}`: {hint}");
        }
    }

    /// S5: la relación pinta lo que su fila declara —estilo, los dos glifos y
    /// la palabra— y no consulta nada más.
    #[test]
    fn a_relation_paints_the_glyph_it_declares() {
        let mut calls = edge("a", "b");
        calls.kind = Some("calls".to_string());
        let mut uses = edge("a", "b");
        uses.kind = Some("uses".to_string());
        let mut hosts = edge("a", "b");
        hosts.kind = Some("hosts".to_string());

        let graph = graph_of(&[calls, uses, hosts], &TABLE);

        assert_eq!(graph.edges[0].style, Style::Solid);
        assert_eq!(graph.edges[0].head, Some(Adornment::Arrow));
        assert_eq!(graph.edges[0].tail, None);
        assert_eq!(graph.edges[0].label.as_deref(), Some("calls"));

        // Una línea sólida sin adorno: el contacto simétrico no afirma
        // dirección, y la palabra es lo único que lo nombra.
        assert_eq!(graph.edges[1].style, Style::Solid);
        assert_eq!(graph.edges[1].head, None);
        assert_eq!(graph.edges[1].tail, None);
        assert_eq!(graph.edges[1].label.as_deref(), Some("uses"));

        // El glifo del origen también sale de la tabla.
        assert_eq!(graph.edges[2].head, Some(Adornment::Arrow));
        assert_eq!(graph.edges[2].tail, Some(Adornment::Diamond));
    }

    /// S5: con `kind` escrito, `arrow` y `style` no se consultan. La etiqueta
    /// del documento sigue ganando: la tabla solo implica la palabra.
    #[test]
    fn a_relation_supersedes_the_line_the_document_asked_for() {
        let mut subscribes = edge("a", "b");
        subscribes.kind = Some("subscribes".to_string());
        subscribes.style = Some(Style::Solid);
        subscribes.arrow = Some(false);
        subscribes.label = Some("eventos".to_string());

        let graph = graph_of(&[subscribes], &TABLE);

        assert_eq!(
            graph.edges[0].style,
            Style::Dashed,
            "el estilo es el de la tabla"
        );
        assert_eq!(
            graph.edges[0].head,
            Some(Adornment::OpenArrow),
            "la flecha abierta es la de la tabla, no `arrow: false`"
        );
        assert_eq!(
            graph.edges[0].label.as_deref(),
            Some("eventos"),
            "la etiqueta del documento gana sobre la implícita"
        );
    }

    /// D1: un documento que no escribe `kind` es el documento de hoy —el
    /// `arrow` de siempre y su estilo—, en todo kind, tenga tabla o no.
    #[test]
    fn an_edge_without_a_relation_keeps_the_document_line() {
        let mut plain = edge("a", "b");
        plain.label = Some("HTTPS".to_string());
        let mut no_arrow = edge("a", "b");
        no_arrow.arrow = Some(false);
        no_arrow.style = Some(Style::Dashed);

        for table in [&TABLE[..], &[][..]] {
            let graph = graph_of(&[plain.clone(), no_arrow.clone()], table);

            assert_eq!(graph.edges[0].style, Style::Solid);
            assert_eq!(
                graph.edges[0].head,
                Some(Adornment::Arrow),
                "sin `kind` la arista conserva la flecha de siempre"
            );
            assert_eq!(graph.edges[0].label.as_deref(), Some("HTTPS"));

            assert_eq!(graph.edges[1].style, Style::Dashed);
            assert_eq!(graph.edges[1].head, None);
            assert_eq!(graph.edges[1].label, None);
        }
    }

    /// S6: un kind cuya tabla está vacía no tiene vocabulario de relación, así
    /// que escribir `kind` se rechaza en vez de aceptarse y tirarse.
    #[test]
    fn a_kind_without_a_vocabulary_rejects_a_relation() {
        let mut bad = edge("a", "b");
        bad.kind = Some("calls".to_string());

        let errors = validate_flow(
            "flowchart",
            &[node("a"), node("b")],
            &[bad],
            &vocabulary(&[]),
        );

        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code, ErrorCode::InvalidEnum);
        assert_eq!(errors[0].path, "edges[0].kind");
        assert!(
            errors[0].message.contains("no declara"),
            "el mensaje dice que el kind no declara vocabulario: {}",
            errors[0].message
        );
        let hint = errors[0].hint.as_deref().expect("cómo se arregla");
        assert!(hint.contains("quitá `kind`"), "{hint}");
    }
}
