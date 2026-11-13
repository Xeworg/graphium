//! El kind `component`: arquitectura de sistemas al estilo C4 (PRD §6).
//!
//! Contenedores, componentes, bases de datos y sistemas externos —y, desde
//! `f29-componente.md`, las aplicaciones, los servicios, la infraestructura, los
//! brokers, el almacenamiento y los runtimes—, agrupados en **fronteras**: el
//! grupo es el sistema o el subsistema al que pertenecen las piezas, y es lo que
//! convierte una nube de cajas en una arquitectura. Es el primer kind que usa
//! los grupos de la IR, así que el renderer dibuja el contenedor y su nombre
//! (PRD §6.1).
//!
//! Las relaciones son **tipadas**: la arista escribe un `kind` —`uses`, `calls`,
//! `reads`, `writes`, `publishes`, `subscribes`, `depends_on`, `hosts`,
//! `connects_to`— y el estilo, los glifos y la palabra salen de la tabla, no del
//! documento (D3). Cada fila nombra su línea: cuatro comparten familia de glifo
//! y la palabra es lo que quita la ambigüedad.
//!
//! La regla propia del kind: **una pieza pertenece a una sola frontera**. Si el
//! documento la pone en dos, son dos arquitecturas superpuestas y el dibujo no
//! puede decir cuál vale.

use gx_core::error::ValidationError;
use gx_core::ir::{Adornment, Direction, Ir, LayoutHint, Shape, Style};
use gx_core::kind::{DiagramKind, EdgeKind, KindLimits, Role};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::flow::{to_graph, validate_flow, validate_header, FlowEdge, FlowNode, FlowVocabulary};
use crate::groups::{to_ir_groups, validate_groups};

/// Contract version supported by this build (PRD §10.1).
pub const SUPPORTED_VERSION: u32 = 1;

/// El rol de un nodo que no declara ninguno: un componente.
pub const DEFAULT_ROLE: &str = "component";

/// Los roles de un diagrama de componentes y su forma.
pub const ROLES: [Role; 10] = [
    Role::new("container", Shape::Box),
    Role::new("component", Shape::Rounded),
    Role::new("database", Shape::Cylinder),
    Role::new("external", Shape::Box),
    // Los seis tipos de componente de `f29-componente.md` (D4): la caja
    // redondeada de siempre, porque el tipo se lee por su palabra.
    Role::new("application", Shape::Rounded),
    Role::new("service", Shape::Rounded),
    Role::new("infrastructure", Shape::Rounded),
    Role::new("broker", Shape::Rounded),
    Role::new("storage", Shape::Rounded),
    Role::new("runtime", Shape::Rounded),
];

/// El rol que se dibuja punteado: lo que no es de este sistema.
pub const EXTERNAL_ROLE: &str = "external";

/// Los seis roles que escriben su nombre arriba, en el hueco del estereotipo
/// (`f29-componente.md`, D4): el catálogo lo dice con todas las letras —«no hace
/// falta forma nueva: un broker se lee por su palabra, no por un glifo»—, así
/// que los cuatro roles de antes conservan su forma y siguen callados.
pub const STEREOTYPED_ROLES: [&str; 6] = [
    "application",
    "service",
    "infrastructure",
    "broker",
    "storage",
    "runtime",
];

/// Las nueve relaciones de un mapa de componentes (`f29-componente.md`, D3), en
/// el orden en que la fila 12 del catálogo las nombra.
///
/// La notación sigue una sola regla. Una flecha sólida es una **acción
/// dirigida**: `calls`, `writes`, `publishes`, `hosts`. Una línea sólida sin
/// adorno es un **contacto simétrico**: `uses`, `connects_to`. Una flecha
/// punteada es una **dependencia**: `depends_on`. Una flecha abierta punteada es
/// una **observación**: `reads`, `subscribes`. **Cada fila nombra su línea**:
/// cuatro comparten familia de glifo y la palabra es lo que las distingue, así
/// que ninguna relación es muda.
pub const RELATIONS: [EdgeKind; 9] = [
    EdgeKind::new("uses", Style::Solid, None, false, Some("uses")),
    EdgeKind::new(
        "calls",
        Style::Solid,
        Some(Adornment::Arrow),
        false,
        Some("calls"),
    ),
    EdgeKind::new(
        "reads",
        Style::Dashed,
        Some(Adornment::OpenArrow),
        false,
        Some("reads"),
    ),
    EdgeKind::new(
        "writes",
        Style::Solid,
        Some(Adornment::Arrow),
        false,
        Some("writes"),
    ),
    EdgeKind::new(
        "publishes",
        Style::Solid,
        Some(Adornment::Arrow),
        false,
        Some("publishes"),
    ),
    EdgeKind::new(
        "subscribes",
        Style::Dashed,
        Some(Adornment::OpenArrow),
        false,
        Some("subscribes"),
    ),
    EdgeKind::new(
        "depends_on",
        Style::Dashed,
        Some(Adornment::Arrow),
        false,
        Some("depends_on"),
    ),
    EdgeKind::new(
        "hosts",
        Style::Solid,
        Some(Adornment::Arrow),
        false,
        Some("hosts"),
    ),
    EdgeKind::new(
        "connects_to",
        Style::Solid,
        None,
        false,
        Some("connects_to"),
    ),
];

/// A boundary: the system or subsystem the pieces belong to. It is the shared
/// group of PRD §7 RF15, so a boundary nests inside another one like any other
/// group.
pub use crate::groups::Group as Boundary;

/// The JSON contract of the `component` kind.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComponentInput {
    /// Contract version. Only `1` is supported; absent means `1`.
    #[serde(default = "crate::flow::default_version")]
    pub version: u32,
    /// Must be `component`.
    pub kind: String,
    /// Human readable title of the diagram.
    #[serde(default)]
    pub title: Option<String>,
    /// Direction of the layout.
    #[serde(default)]
    pub direction: Option<Direction>,
    /// The pieces: containers, components, databases, external systems and the
    /// typed roles (`application`, `service`, `infrastructure`, `broker`,
    /// `storage`, `runtime`).
    pub nodes: Vec<FlowNode>,
    /// The boundaries, with the pieces that belong to each one.
    #[serde(default)]
    pub groups: Vec<Boundary>,
    /// The relations between pieces: the technology on the label when it
    /// matters, or the typed relation in `kind` (`calls`, `reads`, `hosts`).
    #[serde(default)]
    pub edges: Vec<FlowEdge>,
}

/// El vocabulario del kind para la maquinaria compartida (`f29-componente.md`,
/// D2): los roles, el de por defecto y la tabla de las nueve relaciones.
const VOCABULARY: FlowVocabulary<'static> = FlowVocabulary {
    roles: &ROLES,
    default_role: DEFAULT_ROLE,
    edge_kinds: &RELATIONS,
};

/// The `component` kind.
#[derive(Debug, Default)]
pub struct ComponentKind;

impl ComponentKind {
    /// La regla que enseña el vocabulario de relación (`f29-componente.md`,
    /// S5), derivada de [`RELATIONS`]: una relación no puede pintarse con una
    /// notación que el contrato no menciona. El campo es `kind` y no hay
    /// relación por defecto: un documento que no escribe ninguna es el
    /// documento de hoy.
    fn relations_rule() -> String {
        crate::flow::relations_rule("kind", &RELATIONS, None)
    }

    /// La regla que enseña los seis roles con estereotipo (`D4`), derivada de
    /// [`STEREOTYPED_ROLES`]: el tipo se lee por su palabra y no por un glifo.
    fn stereotypes_rule() -> String {
        let names: Vec<String> = STEREOTYPED_ROLES
            .iter()
            .map(|role| format!("`{role}`"))
            .collect();
        format!(
            "Los roles {} se dibujan con la caja redondeada y escriben su nombre arriba, entre «»: el tipo se lee por su palabra, no por un glifo.",
            names.join(", ")
        )
    }
}

impl DiagramKind for ComponentKind {
    const KIND: &'static str = "component";
    const PHASE: &'static str = "F2";
    const LAYOUT_HINT: LayoutHint = LayoutHint::Layered;
    const LIMITS: KindLimits = KindLimits::GRAPH;
    const KIND_VERSION: u32 = SUPPORTED_VERSION;

    type Input = ComponentInput;

    fn schema() -> schemars::Schema {
        schemars::schema_for!(ComponentInput)
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

    /// La tabla de relaciones del kind, publicada por la superficie compartida
    /// (`f29-componente.md`, T2): el contrato y la arista la leen desde acá.
    fn edge_kinds() -> &'static [EdgeKind] {
        &RELATIONS
    }

    fn validate(&self, input: &Self::Input) -> Result<(), Vec<ValidationError>> {
        let mut errors = validate_header(Self::KIND, &input.kind, SUPPORTED_VERSION, input.version);
        errors.extend(validate_flow(
            Self::KIND,
            &input.nodes,
            &input.edges,
            &VOCABULARY,
        ));

        let node_ids: Vec<&str> = input.nodes.iter().map(|node| node.id.as_str()).collect();
        errors.extend(validate_groups(&node_ids, &input.groups));

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn to_ir(&self, input: &Self::Input) -> Ir {
        let mut graph = to_graph(
            &input.nodes,
            &input.edges,
            &VOCABULARY,
            input.direction,
            Self::LAYOUT_HINT,
            to_ir_groups(&input.groups),
        );

        for node in &mut graph.nodes {
            let Some(source) = input.nodes.iter().find(|source| source.id == node.id) else {
                continue;
            };
            let role = source.role.as_deref().unwrap_or(DEFAULT_ROLE);
            // El tipo se lee por su palabra, no por un glifo (D4): los seis
            // roles nuevos escriben su nombre arriba, en el hueco que
            // `f27-etapa1` abrió. Lo que el documento escriba gana: el
            // estereotipo es vocabulario de nodo.
            if node.stereotype.is_none() && STEREOTYPED_ROLES.contains(&role) {
                node.stereotype = Some(role.to_string());
            }
            // Lo externo se dibuja punteado: no es de este sistema.
            if role == EXTERNAL_ROLE && source.style.is_none() {
                node.style = Style::Dashed;
            }
        }

        Ir::Graph(graph)
    }

    /// Lo propio de una arquitectura: la frontera es única, lo externo se
    /// dibuja punteado y el tipo de cada pieza se lee por su palabra.
    fn rules(&self) -> Vec<String> {
        vec![
            Self::relations_rule(),
            Self::stereotypes_rule(),
            "Un nodo con `role: \"external\"` se dibuja con borde punteado: no es de este sistema."
                .to_string(),
            "Cada `groups[].members` referencia piezas o fronteras declaradas, y **una pieza vive en una sola frontera**."
                .to_string(),
            "La etiqueta de la arista es el protocolo o la llamada (`HTTPS`, `SQL`); si la arista escribe `kind`, la relación la nombra salvo que el documento diga otra cosa."
                .to_string(),
        ]
    }

    fn min_example() -> serde_json::Value {
        serde_json::json!({
            "version": 1,
            "kind": "component",
            "title": "API de pedidos",
            "nodes": [
                { "id": "web", "label": "Web", "role": "container" },
                { "id": "api", "label": "API" },
                { "id": "db", "label": "Postgres", "role": "database" }
            ],
            "groups": [
                { "id": "backend", "label": "Backend", "members": ["api", "db"] }
            ],
            "edges": [
                { "from": "web", "to": "api", "label": "HTTPS" },
                { "from": "api", "to": "db", "label": "SQL" }
            ]
        })
    }
}

/// Registers the kind in the global registry.
pub fn register() {
    gx_core::kind::register_kind::<ComponentKind>();
}

#[cfg(test)]
mod tests {
    use super::{ComponentKind, EXTERNAL_ROLE, RELATIONS, ROLES, STEREOTYPED_ROLES};
    use crate::test_support::input;
    use gx_core::error::ErrorCode;
    use gx_core::ir::{Ir, Shape, Style};
    use gx_core::kind::DiagramKind;

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
            .find(|error| error.message.contains("backend"))
            .expect("el error del grupo doble");
        assert_eq!(error.code, ErrorCode::DuplicateId);
        assert_eq!(error.path, "groups[1].members[0]");
        assert!(error.message.contains("un solo grupo"), "{}", error.message);
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

    /// RF15: un grupo puede contener otro grupo, y el anidamiento llega a la IR
    /// con los miembros tal como el documento los declara.
    #[test]
    fn a_boundary_inside_a_boundary_is_accepted() {
        let parsed = input(
            r#"{ "version": 1, "kind": "component",
                 "nodes": [ { "id": "api" } ],
                 "groups": [
                   { "id": "sys", "label": "Sistema", "members": ["backend"] },
                   { "id": "backend", "label": "Backend", "members": ["api"] } ] }"#,
        );

        ComponentKind
            .validate(&parsed)
            .expect("un grupo puede contener otro grupo");

        let Ir::Graph(graph) = ComponentKind.to_ir(&parsed) else {
            panic!("un diagrama de componentes es un grafo");
        };
        assert_eq!(graph.groups.len(), 2);
        assert_eq!(graph.groups[0].members, vec!["backend"]);
        assert_eq!(graph.groups[1].members, vec!["api"]);
    }

    /// Un grupo no se contiene a sí mismo: el layout no puede cerrar el ciclo.
    #[test]
    fn a_boundary_that_contains_itself_is_reported() {
        let parsed = input(
            r#"{ "version": 1, "kind": "component",
                 "nodes": [ { "id": "api" } ],
                 "groups": [ { "id": "sys", "members": ["sys"] } ] }"#,
        );

        let errors = ComponentKind
            .validate(&parsed)
            .expect_err("un grupo no se contiene a sí mismo");

        assert!(
            errors
                .iter()
                .any(|error| error.code == ErrorCode::InvalidValue
                    && error.path == "groups[0].members[0]"),
            "{errors:?}"
        );
    }

    /// Dos grupos que se contienen mutuamente son el mismo ciclo, un paso más
    /// largo.
    #[test]
    fn a_boundary_cycle_is_reported() {
        let parsed = input(
            r#"{ "version": 1, "kind": "component",
                 "nodes": [ { "id": "api" } ],
                 "groups": [
                   { "id": "a", "members": ["b"] },
                   { "id": "b", "members": ["a"] } ] }"#,
        );

        let errors = ComponentKind
            .validate(&parsed)
            .expect_err("dos grupos no se contienen mutuamente");

        assert!(
            errors
                .iter()
                .any(|error| error.code == ErrorCode::InvalidValue),
            "{errors:?}"
        );
    }

    /// El id de un grupo y el de una pieza viven en el mismo espacio de nombres
    /// del motor, así que no pueden coincidir.
    #[test]
    fn a_boundary_with_the_id_of_a_piece_is_reported() {
        let parsed = input(
            r#"{ "version": 1, "kind": "component",
                 "nodes": [ { "id": "api" } ],
                 "groups": [ { "id": "api", "members": ["api"] } ] }"#,
        );

        let errors = ComponentKind
            .validate(&parsed)
            .expect_err("un grupo no toma el id de una pieza");

        assert!(
            errors
                .iter()
                .any(|error| error.code == ErrorCode::DuplicateId && error.path == "groups[0].id"),
            "{errors:?}"
        );
    }

    /// Un grupo tiene un solo padre: si dos lo reclaman, el dibujo no puede
    /// elegir cuál vale.
    #[test]
    fn a_boundary_with_two_parents_is_reported() {
        let parsed = input(
            r#"{ "version": 1, "kind": "component",
                 "nodes": [ { "id": "api" } ],
                 "groups": [
                   { "id": "inner", "members": ["api"] },
                   { "id": "left", "members": ["inner"] },
                   { "id": "right", "members": ["inner"] } ] }"#,
        );

        let errors = ComponentKind
            .validate(&parsed)
            .expect_err("un grupo no vive en dos padres");

        assert!(
            errors
                .iter()
                .any(|error| error.code == ErrorCode::DuplicateId
                    && error.path == "groups[2].members[0]"),
            "{errors:?}"
        );
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
        assert_eq!(ROLES.len(), 10, "los cuatro de siempre y los seis de `f29`");
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

    /// D3: las nueve relaciones son las que la fila 12 del catálogo nombra, en
    /// ese orden. La tabla es el contrato y no admite una décima inventada.
    #[test]
    fn the_nine_relations_are_the_ones_the_catalogue_names() {
        let names: Vec<&str> = RELATIONS.iter().map(|relation| relation.name).collect();
        assert_eq!(
            names,
            vec![
                "uses",
                "calls",
                "reads",
                "writes",
                "publishes",
                "subscribes",
                "depends_on",
                "hosts",
                "connects_to"
            ]
        );
    }

    /// D3: la notación sale de la fila —estilo, glifo y palabra—, y la regla de
    /// la tabla es una sola: flecha sólida para la acción dirigida, sólida sin
    /// adorno para el contacto simétrico, punteada con flecha para la
    /// dependencia y punteada con flecha abierta para la observación. Ninguna
    /// relación es muda: cuatro comparten familia de glifo y la palabra es lo
    /// que las distingue.
    #[test]
    fn every_relation_paints_the_notation_its_row_declares() {
        let edges: Vec<String> = RELATIONS
            .iter()
            .map(|relation| {
                format!(
                    r#"{{ "from": "a", "to": "b", "kind": "{}" }}"#,
                    relation.name
                )
            })
            .collect();
        let document = format!(
            r#"{{ "version": 1, "kind": "component",
                 "nodes": [ {{ "id": "a", "role": "service" }},
                            {{ "id": "b", "role": "storage" }} ],
                 "edges": [{}] }}"#,
            edges.join(", ")
        );
        let parsed = input(&document);
        ComponentKind
            .validate(&parsed)
            .expect("las nueve relaciones son del kind");

        let Ir::Graph(graph) = ComponentKind.to_ir(&parsed) else {
            panic!("un diagrama de componentes es un grafo");
        };

        for (edge, relation) in graph.edges.iter().zip(RELATIONS) {
            assert_eq!(
                edge.style, relation.style,
                "el estilo de `{}`",
                relation.name
            );
            assert_eq!(edge.head, relation.head, "el glifo de `{}`", relation.name);
            assert_eq!(edge.tail, relation.tail, "el origen de `{}`", relation.name);
            assert_eq!(
                edge.label.as_deref(),
                relation.label,
                "`{}` se nombra con su palabra",
                relation.name
            );
        }
    }

    /// D3: la regla del kind se **deriva** de la tabla, así que una relación no
    /// puede pintarse con una notación que el contrato no menciona. Las nueve
    /// filas se nombran, con su estilo, su glifo y su palabra.
    #[test]
    fn the_rule_line_names_the_nine_relations_it_derives() {
        let rules = ComponentKind.rules();
        let line = rules.first().expect("la primera regla es la de la línea");

        assert!(line.starts_with("`kind` decide la línea:"), "{line}");
        for relation in RELATIONS {
            let name = format!("`{}`", relation.name);
            assert!(line.contains(&name), "la regla no nombra {name}: {line}");
            assert!(
                line.contains(relation.style_word()),
                "la regla no dice el estilo de `{}`: {line}",
                relation.name
            );
            if let Some(glyph) = relation.head_word() {
                assert!(
                    line.contains(glyph),
                    "la regla no dice el glifo de `{}`: {line}",
                    relation.name
                );
            }
            if let Some(glyph) = relation.tail_word() {
                assert!(
                    line.contains(glyph),
                    "la regla no dice el glifo del origen de `{}`: {line}",
                    relation.name
                );
            }
        }
        // Ninguna relación es muda: la palabra es lo que separa las cuatro que
        // comparten familia de glifo.
        assert_eq!(
            line.matches("se nombra").count(),
            RELATIONS.len(),
            "cada fila nombra su línea: {line}"
        );
        assert!(!line.contains("no se nombra"), "{line}");
    }

    /// D4: los seis roles nuevos se dibujan con la caja redondeada de siempre y
    /// escriben su nombre en el hueco del estereotipo, porque el tipo se lee por
    /// su palabra y no por un glifo.
    #[test]
    fn the_six_new_roles_write_their_name_above_the_box() {
        let nodes: Vec<String> = STEREOTYPED_ROLES
            .iter()
            .map(|role| format!(r#"{{ "id": "{role}", "role": "{role}" }}"#))
            .collect();
        let document = format!(
            r#"{{ "version": 1, "kind": "component", "nodes": [{}] }}"#,
            nodes.join(", ")
        );
        let parsed = input(&document);
        ComponentKind
            .validate(&parsed)
            .expect("los seis roles son del kind");

        let Ir::Graph(graph) = ComponentKind.to_ir(&parsed) else {
            panic!("un diagrama de componentes es un grafo");
        };

        assert_eq!(graph.nodes.len(), STEREOTYPED_ROLES.len());
        for (node, role) in graph.nodes.iter().zip(STEREOTYPED_ROLES) {
            assert_eq!(node.shape, Shape::Rounded, "la caja de `{role}`");
            assert_eq!(
                node.stereotype.as_deref(),
                Some(role),
                "`{role}` escribe su nombre arriba"
            );
        }
    }

    /// S4: los cuatro roles que ya existían conservan su forma y no escriben
    /// nada, así que ningún dibujo de hoy se mueve.
    #[test]
    fn the_four_existing_roles_keep_their_shape_and_stay_silent() {
        let parsed = input(
            r#"{ "version": 1, "kind": "component",
                 "nodes": [
                   { "id": "web", "role": "container" },
                   { "id": "api", "role": "component" },
                   { "id": "db", "role": "database" },
                   { "id": "crm", "role": "external" } ] }"#,
        );
        ComponentKind
            .validate(&parsed)
            .expect("los cuatro son válidos");

        let Ir::Graph(graph) = ComponentKind.to_ir(&parsed) else {
            panic!("un diagrama de componentes es un grafo");
        };

        let shapes: Vec<Shape> = graph.nodes.iter().map(|node| node.shape).collect();
        assert_eq!(
            shapes,
            vec![Shape::Box, Shape::Rounded, Shape::Cylinder, Shape::Box]
        );
        for node in &graph.nodes {
            assert_eq!(node.stereotype, None, "`{}` no escribe nada", node.id);
        }
        assert_eq!(
            graph.nodes[3].style,
            Style::Dashed,
            "lo externo sigue punteado"
        );
    }

    /// S4/D1: un documento que no escribe `kind` es el documento de hoy —la
    /// etiqueta es el protocolo o la llamada y la línea es la flecha de
    /// siempre—, tenga el kind tabla o no.
    #[test]
    fn a_document_without_a_relation_keeps_todays_line() {
        let parsed = input(
            r#"{ "version": 1, "kind": "component",
                 "nodes": [ { "id": "web" }, { "id": "api" }, { "id": "db" } ],
                 "edges": [
                   { "from": "web", "to": "api", "label": "HTTPS" },
                   { "from": "api", "to": "db", "label": "SQL", "arrow": false } ] }"#,
        );
        ComponentKind
            .validate(&parsed)
            .expect("es el documento de hoy");

        let Ir::Graph(graph) = ComponentKind.to_ir(&parsed) else {
            panic!("un diagrama de componentes es un grafo");
        };

        assert_eq!(graph.edges[0].style, Style::Solid);
        assert_eq!(graph.edges[0].head, Some(gx_core::ir::Adornment::Arrow));
        assert_eq!(graph.edges[0].label.as_deref(), Some("HTTPS"));
        assert_eq!(graph.edges[1].head, None, "`arrow: false` sigue mandando");
        assert_eq!(graph.edges[1].label.as_deref(), Some("SQL"));
    }

    /// S5: una relación que la tabla no declara se reporta en `edges[i].kind`
    /// con las nueve que el kind admite.
    #[test]
    fn an_undeclared_relation_is_reported_with_the_nine_of_the_kind() {
        let parsed = input(
            r#"{ "version": 1, "kind": "component",
                 "nodes": [ { "id": "api" }, { "id": "db" } ],
                 "edges": [ { "from": "api", "to": "db", "kind": "borrows" } ] }"#,
        );

        let errors = ComponentKind
            .validate(&parsed)
            .expect_err("la relación no existe");

        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code, ErrorCode::InvalidEnum);
        assert_eq!(errors[0].path, "edges[0].kind");
        let hint = errors[0].hint.as_deref().expect("dice cuáles hay");
        for relation in RELATIONS {
            assert!(
                hint.contains(relation.name),
                "el hint no nombra `{}`: {hint}",
                relation.name
            );
        }
    }

    /// T1 de `f27-etapa1.md`: la tabla se publica por la superficie object-safe
    /// del kind, que es la que el contrato y un kind nuevo leen.
    #[test]
    fn the_kind_publishes_its_relation_table() {
        use gx_core::kind::{KindAdapter, KindHandler};

        let handler = KindAdapter::new(ComponentKind);
        let names: Vec<&str> = handler
            .edge_kinds()
            .iter()
            .map(|edge_kind| edge_kind.name)
            .collect();
        assert_eq!(
            names,
            RELATIONS.iter().map(|row| row.name).collect::<Vec<_>>()
        );
    }
}
