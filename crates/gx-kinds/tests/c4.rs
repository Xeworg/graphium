//! The `c4` contract: the declared level disciplines the types
//! (feature doc `f17-c4-kind.md`, S2, S3, S4).
//!
//! A c4 document is flat — no nesting in v1 — so the reading comes from three
//! places: the `level`, the `type` of each element and the relations between
//! them. These tests pin the contract the agent writes and the IR it gets.

use gx_core::error::ErrorCode;
use gx_core::ir::Shape;
use gx_core::kind::DiagramKind;
use gx_kinds::c4::{C4Input, C4Kind};

/// The small context document of the feature doc: two people, one system and
/// the two relations that connect them.
const CONTEXT: &str = r#"{
  "version": 1,
  "kind": "c4",
  "title": "Pedidos — contexto",
  "level": "context",
  "elements": [
    { "id": "cliente", "type": "person", "label": "Cliente",
      "description": "hace el pedido" },
    { "id": "operador", "type": "person", "label": "Operador" },
    { "id": "pedidos", "type": "system", "label": "Sistema de pedidos",
      "technology": "Rust + Svelte" }
  ],
  "relations": [
    { "from": "cliente", "to": "pedidos", "label": "hace un pedido" },
    { "from": "operador", "to": "pedidos", "label": "despacha" }
  ]
}"#;

fn input(json: &str) -> C4Input {
    serde_json::from_str(json).expect("the document parses")
}

/// The errors of a document, panicking when it is valid.
fn errors_of(input: &C4Input) -> Vec<gx_core::error::ValidationError> {
    match C4Kind.validate(input) {
        Ok(()) => panic!("the document has to be rejected"),
        Err(errors) => errors,
    }
}

/// A one element document at `level` whose element declares `element_type`.
fn one_element(level: &str, element_type: &str) -> C4Input {
    let document = serde_json::json!({
        "version": 1,
        "kind": "c4",
        "level": level,
        "elements": [ { "id": "a", "type": element_type, "label": "A" } ],
    });
    serde_json::from_value(document).expect("the document parses")
}

/// S2: a context document with its people, its system and its relations is
/// valid.
#[test]
fn a_context_document_validates() {
    let parsed = input(CONTEXT);

    if let Err(errors) = C4Kind.validate(&parsed) {
        panic!("the context document is valid: {errors:?}");
    }
}

/// S3: the level disciplines the types, and the error names the level and the
/// types it admits.
#[test]
fn a_type_the_level_does_not_admit_names_the_level_and_its_types() {
    let document = CONTEXT.replace(
        r#"{ "id": "operador", "type": "person", "label": "Operador" }"#,
        r#"{ "id": "operador", "type": "container", "label": "Operador" }"#,
    );
    let parsed = input(&document);

    let errors = errors_of(&parsed);
    let error = errors
        .iter()
        .find(|error| error.path == "elements[1].type")
        .unwrap_or_else(|| panic!("the error names the element: {errors:?}"));
    assert_eq!(error.code, ErrorCode::InvalidValue);
    assert!(
        error.message.contains("context"),
        "the message names the level: {}",
        error.message
    );
    assert!(
        error.message.contains("container"),
        "the message names the rejected type: {}",
        error.message
    );
    let hint = error.hint.as_deref().unwrap_or_default();
    assert!(hint.contains("person"), "the hint lists the types: {hint}");
    assert!(hint.contains("system"), "the hint lists the types: {hint}");
}

/// S2: every relation references elements present in the document.
#[test]
fn a_relation_to_an_absent_element_is_reported() {
    let document = serde_json::json!({
        "version": 1,
        "kind": "c4",
        "level": "context",
        "elements": [ { "id": "cliente", "type": "person", "label": "Cliente" } ],
        "relations": [ { "from": "cliente", "to": "fantasma" } ],
    });
    let parsed: C4Input = serde_json::from_value(document).expect("the document parses");

    let errors = errors_of(&parsed);
    assert_eq!(errors[0].code, ErrorCode::UnknownRef);
    assert_eq!(errors[0].path, "relations[0].to");
}

/// S2: ids are unique inside the document.
#[test]
fn a_duplicated_element_id_is_reported() {
    let document = serde_json::json!({
        "version": 1,
        "kind": "c4",
        "level": "context",
        "elements": [
            { "id": "pedidos", "type": "system", "label": "Pedidos" },
            { "id": "pedidos", "type": "system", "label": "Pedidos otra vez" }
        ],
    });
    let parsed: C4Input = serde_json::from_value(document).expect("the document parses");

    let errors = errors_of(&parsed);
    assert_eq!(errors[0].code, ErrorCode::DuplicateId);
    assert_eq!(errors[0].path, "elements[1].id");
}

/// S3: the components level admits containers and components, and a person is
/// not one of them.
#[test]
fn the_components_level_admits_containers_and_components_and_rejects_a_person() {
    let legal = serde_json::json!({
        "version": 1,
        "kind": "c4",
        "level": "components",
        "elements": [
            { "id": "api", "type": "container", "label": "API" },
            { "id": "auth", "type": "component", "label": "Auth" }
        ],
        "relations": [ { "from": "api", "to": "auth", "label": "usa" } ],
    });
    let parsed: C4Input = serde_json::from_value(legal).expect("the document parses");
    if let Err(errors) = C4Kind.validate(&parsed) {
        panic!("container + component is a components document: {errors:?}");
    }

    let errors = errors_of(&one_element("components", "person"));
    let error = &errors[0];
    assert_eq!(error.code, ErrorCode::InvalidValue);
    assert_eq!(error.path, "elements[0].type");
    assert!(error.message.contains("components"), "{}", error.message);
    assert!(
        error
            .hint
            .as_deref()
            .unwrap_or_default()
            .contains("container"),
        "{:?}",
        error.hint
    );
}

/// S3: the three levels and the types each one admits, as one table.
#[test]
fn every_level_admits_its_own_types() {
    // (level, admitted, rejected)
    let cases: [(&str, [&str; 2], [&str; 2]); 3] = [
        ("context", ["person", "system"], ["container", "component"]),
        (
            "containers",
            ["system", "container"],
            ["person", "component"],
        ),
        (
            "components",
            ["container", "component"],
            ["person", "system"],
        ),
    ];

    for (level, admitted, rejected) in cases {
        for element_type in admitted {
            if let Err(errors) = C4Kind.validate(&one_element(level, element_type)) {
                panic!("`{element_type}` belongs in `{level}`: {errors:?}");
            }
        }
        for element_type in rejected {
            let errors = errors_of(&one_element(level, element_type));
            assert!(
                errors
                    .iter()
                    .any(|error| error.code == ErrorCode::InvalidValue
                        && error.path == "elements[0].type"),
                "`{element_type}` does not belong in `{level}`: {errors:?}"
            );
        }
    }
}

/// S2: `label` is part of the contract, not an optional decoration.
#[test]
fn an_element_without_a_label_does_not_parse() {
    let document = serde_json::json!({
        "version": 1,
        "kind": "c4",
        "level": "context",
        "elements": [ { "id": "cliente", "type": "person" } ],
    });

    serde_json::from_value::<C4Input>(document).expect_err("the label is required");
}

/// S2: unknown fields are rejected by the same mechanism as the rest of the
/// family: the registry reports `unknown_field` with the path of the field.
#[test]
fn an_unknown_field_is_reported_like_the_rest_of_the_family() {
    gx_kinds::c4::register();
    let handler = gx_core::kind::for_kind("c4").expect("c4 is registered");
    let document = serde_json::json!({
        "version": 1,
        "kind": "c4",
        "level": "context",
        "elements": [ { "id": "a", "type": "person", "label": "A" } ],
        "parents": [],
    });

    let errors = handler
        .validate_json(&document)
        .expect_err("an unknown field is rejected");
    assert_eq!(errors[0].code, ErrorCode::UnknownField);
    assert_eq!(errors[0].path, "parents");
}

/// S4: every element is a box whose text carries the type and the label, in
/// the compartment shape `er` uses for its rows.
#[test]
fn every_element_becomes_a_box_with_its_type_and_its_label() {
    let parsed = input(CONTEXT);

    let graph = C4Kind
        .to_ir(&parsed)
        .graph()
        .expect("a c4 graph")
        .to_owned();
    assert_eq!(graph.nodes.len(), 3);

    let cliente = graph
        .nodes
        .iter()
        .find(|node| node.id == "cliente")
        .expect("cliente");
    assert_eq!(cliente.label, "Cliente");
    assert_eq!(cliente.shape, Shape::Box);
    // The type is the first row: the box shows both the label and the type.
    assert_eq!(cliente.rows[0].text, "person");
    assert_eq!(cliente.rows[1].text, "hace el pedido");

    let pedidos = graph
        .nodes
        .iter()
        .find(|node| node.id == "pedidos")
        .expect("pedidos");
    assert_eq!(pedidos.rows[0].text, "system");
    assert_eq!(pedidos.rows[0].note.as_deref(), Some("Rust + Svelte"));
}

/// S4: every relation is a labelled edge between the two elements.
#[test]
fn every_relation_becomes_a_labelled_edge() {
    let parsed = input(CONTEXT);

    let graph = C4Kind
        .to_ir(&parsed)
        .graph()
        .expect("a c4 graph")
        .to_owned();
    assert_eq!(graph.edges.len(), 2);
    assert_eq!(graph.edges[0].from, "cliente");
    assert_eq!(graph.edges[0].to, "pedidos");
    assert_eq!(graph.edges[0].label.as_deref(), Some("hace un pedido"));
    assert_eq!(graph.edges[1].label.as_deref(), Some("despacha"));
}

/// S5: the kind declares its ceilings like the rest of the family.
#[test]
fn the_kind_declares_the_limits_of_the_family() {
    assert_eq!(C4Kind::KIND, "c4");
    assert_eq!(C4Kind::LIMITS.max_nodes, 300);
    assert_eq!(C4Kind::LIMITS.max_edges, 600);
}
