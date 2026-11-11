//! El kind `sequence`: interacciones en el tiempo entre actores (PRD §6).
//!
//! Los actores son las columnas y los mensajes son las flechas: el orden del
//! array de mensajes **es** el orden del tiempo, así que el documento no
//! necesita numerar nada y el layout no tiene que adivinar.

use gx_core::error::{ErrorCode, ValidationError};
use gx_core::ir::{Direction, Ir, IrEdge, IrNode, LayoutHint};
use gx_core::kind::{DiagramKind, KindLimits};
use schemars::JsonSchema;
use serde::Deserialize;

/// Contract version supported by this build (PRD §10.1).
pub const SUPPORTED_VERSION: u32 = 1;

/// A participant of the interaction.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Actor {
    /// Unique identifier of the actor inside the diagram.
    pub id: String,
    /// Name the actor shows. Defaults to the id.
    #[serde(default)]
    pub label: Option<String>,
}

/// One message of the interaction.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Message {
    /// Id of the actor that sends.
    pub from: String,
    /// Id of the actor that receives.
    pub to: String,
    /// What the message says, e.g. `POST /orders`.
    pub label: String,
    /// Whether the arrow is dashed, for a reply that does not start an action.
    #[serde(default)]
    pub reply: bool,
    /// Whether the receiving actor takes control here (`f22-dibujo.md`, S5):
    /// opt-in, it draws a thin rectangle over the `to` actor's lifeline while
    /// the actor holds the control — until its next outgoing message.
    #[serde(default)]
    pub activation: bool,
}

/// The JSON contract of the `sequence` kind.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SequenceInput {
    /// Contract version. Only `1` is supported.
    pub version: u32,
    /// Must be `sequence`.
    pub kind: String,
    /// Human readable title of the diagram.
    #[serde(default)]
    pub title: Option<String>,
    /// The actors, in the order they appear across the top.
    pub participants: Vec<Actor>,
    /// The messages, in the order they happen.
    #[serde(default)]
    pub messages: Vec<Message>,
}

/// The `sequence` kind.
#[derive(Debug, Default)]
pub struct SequenceKind;

impl DiagramKind for SequenceKind {
    const KIND: &'static str = "sequence";
    const PHASE: &'static str = "F2";
    const LAYOUT_HINT: LayoutHint = LayoutHint::Lanes;
    const LIMITS: KindLimits = KindLimits::SEQUENCE;
    const KIND_VERSION: u32 = SUPPORTED_VERSION;

    type Input = SequenceInput;

    fn schema() -> schemars::Schema {
        schemars::schema_for!(SequenceInput)
    }

    fn validate(&self, input: &Self::Input) -> Result<(), Vec<ValidationError>> {
        let mut errors =
            crate::flow::validate_header(Self::KIND, &input.kind, SUPPORTED_VERSION, input.version);

        if input.participants.is_empty() {
            errors.push(ValidationError::new(
                "participants",
                ErrorCode::MissingField,
                "una secuencia necesita al menos un participante",
                Some(
                    "agregá quién interactúa, por ejemplo `[{ \"id\": \"cliente\" }]`".to_string(),
                ),
            ));
        }

        let mut seen: Vec<&str> = Vec::new();
        for (index, actor) in input.participants.iter().enumerate() {
            if actor.id.trim().is_empty() {
                errors.push(ValidationError::new(
                    format!("participants[{index}].id"),
                    ErrorCode::InvalidValue,
                    "el id del participante no puede estar vacío",
                    Some("escribí un id único, por ejemplo `cliente`".to_string()),
                ));
            }
            if seen.contains(&actor.id.as_str()) {
                errors.push(ValidationError::new(
                    format!("participants[{index}].id"),
                    ErrorCode::DuplicateId,
                    format!("participante repetido: `{}`", actor.id),
                    Some("los ids de participante deben ser únicos".to_string()),
                ));
            }
            seen.push(actor.id.as_str());
        }

        for (index, message) in input.messages.iter().enumerate() {
            if message.label.trim().is_empty() {
                errors.push(ValidationError::new(
                    format!("messages[{index}].label"),
                    ErrorCode::MissingField,
                    "un mensaje necesita decir qué dice",
                    Some("escribí el texto del mensaje, por ejemplo `POST /orders`".to_string()),
                ));
            }
            for (field, actor) in [("from", &message.from), ("to", &message.to)] {
                if !seen.contains(&actor.as_str()) {
                    errors.push(ValidationError::new(
                        format!("messages[{index}].{field}"),
                        ErrorCode::UnknownRef,
                        format!("participante desconocido: `{actor}`"),
                        Some(format!(
                            "usá uno de los ids declarados: {}",
                            seen.join(", ")
                        )),
                    ));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn to_ir(&self, input: &Self::Input) -> Ir {
        let nodes: Vec<IrNode> = input
            .participants
            .iter()
            .map(|actor| {
                let label = actor
                    .label
                    .clone()
                    .filter(|label| !label.trim().is_empty())
                    .unwrap_or_else(|| actor.id.clone());
                let mut node = IrNode::new(actor.id.clone(), label);
                node.size = gx_core::geom::Size::default();
                node
            })
            .collect();

        // The order of the messages is the order of time: el array no se
        // reordena, y el layout lo respeta tal cual llegó.
        let edges: Vec<IrEdge> = input
            .messages
            .iter()
            .map(|message| {
                let mut edge = IrEdge::new(message.from.clone(), message.to.clone());
                edge.label = Some(message.label.clone());
                edge.arrow = true;
                if message.reply {
                    edge.style = gx_core::ir::Style::Dashed;
                }
                if message.activation {
                    edge.kind_data
                        .insert("activation".to_string(), gx_core::ir::IrValue::Bool(true));
                }
                edge
            })
            .collect();

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
            "kind": "sequence",
            "title": "pedido",
            "participants": [
                { "id": "cliente", "label": "Cliente" },
                { "id": "api", "label": "API" }
            ],
            "messages": [
                { "from": "cliente", "to": "api", "label": "POST /orders", "activation": true },
                { "from": "api", "to": "cliente", "label": "201", "reply": true }
            ]
        })
    }
}

/// Registers the kind in the global registry.
pub fn register() {
    gx_core::kind::register_kind::<SequenceKind>();
}

#[cfg(test)]
mod tests {
    use super::SequenceKind;
    use crate::test_support::input;
    use gx_core::ir::Ir;
    use gx_core::kind::DiagramKind;

    const FLOW: &str = r#"{
      "version": 1, "kind": "sequence", "title": "pedido",
      "participants": [
        { "id": "cliente", "label": "Cliente" },
        { "id": "api", "label": "API" },
        { "id": "db", "label": "Base" }
      ],
      "messages": [
        { "from": "cliente", "to": "api", "label": "POST /orders" },
        { "from": "api", "to": "db", "label": "INSERT" },
        { "from": "db", "to": "api", "label": "ok", "reply": true },
        { "from": "api", "to": "cliente", "label": "201", "reply": true }
      ]
    }"#;

    #[test]
    fn a_sequence_becomes_a_lanes_graph_in_time_order() {
        let parsed = input(FLOW);
        SequenceKind
            .validate(&parsed)
            .expect("la secuencia es válida");

        let Ir::Graph(graph) = SequenceKind.to_ir(&parsed) else {
            panic!("una secuencia es un grafo");
        };

        assert_eq!(graph.layout_hint, gx_core::ir::LayoutHint::Lanes);
        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.nodes[0].label, "Cliente");
        assert_eq!(graph.edges.len(), 4);
        // El orden del array es el orden del tiempo.
        assert_eq!(graph.edges[0].label.as_deref(), Some("POST /orders"));
        assert_eq!(graph.edges[3].label.as_deref(), Some("201"));
        // Las respuestas van punteadas.
        assert_eq!(graph.edges[0].style, gx_core::ir::Style::Solid);
        assert_eq!(graph.edges[2].style, gx_core::ir::Style::Dashed);
    }

    #[test]
    fn an_activation_travels_on_the_edge_and_the_schema_admits_it() {
        let parsed = input(
            r#"{ "version": 1, "kind": "sequence",
                 "participants": [ { "id": "cliente" }, { "id": "api" } ],
                 "messages": [
                   { "from": "cliente", "to": "api", "label": "POST /orders", "activation": true },
                   { "from": "api", "to": "cliente", "label": "201", "reply": true }
                 ] }"#,
        );

        // S5 (`f22-dibujo.md`): the field is opt-in and additive — a document
        // that names it validates, and `deny_unknown_fields` keeps guarding
        // everything else.
        SequenceKind
            .validate(&parsed)
            .expect("la activación es parte del contrato");

        let Ir::Graph(graph) = SequenceKind.to_ir(&parsed) else {
            panic!("una secuencia es un grafo");
        };
        // The flag rides the edge's kind_data: the scalar channel the IR
        // reserves for kind-specific data (L7b).
        assert_eq!(
            graph.edges[0].kind_data.get("activation"),
            Some(&gx_core::ir::IrValue::Bool(true)),
        );
        assert!(
            !graph.edges[1].kind_data.contains_key("activation"),
            "un mensaje sin activación no marca nada"
        );

        // The schema publishes the field, so the JSON schema is the contract.
        let schema = SequenceKind::schema().to_value().to_string();
        assert!(
            schema.contains("activation"),
            "el esquema no nombra la activación"
        );
    }

    #[test]
    fn a_message_to_an_unknown_participant_is_reported() {
        let parsed = input(
            r#"{ "version": 1, "kind": "sequence",
                 "participants": [ { "id": "api" } ],
                 "messages": [ { "from": "api", "to": "nadie", "label": "hola" } ] }"#,
        );

        let errors = SequenceKind
            .validate(&parsed)
            .expect_err("el destino no existe");

        assert_eq!(errors[0].code, gx_core::error::ErrorCode::UnknownRef);
        assert_eq!(errors[0].path, "messages[0].to");
        assert!(errors[0].hint.is_some());
    }

    #[test]
    fn a_sequence_without_participants_is_reported() {
        let parsed = input(r#"{ "version": 1, "kind": "sequence", "participants": [] }"#);

        let errors = SequenceKind
            .validate(&parsed)
            .expect_err("sin actores no hay secuencia");

        assert_eq!(errors[0].code, gx_core::error::ErrorCode::MissingField);
        assert_eq!(errors[0].path, "participants");
    }

    #[test]
    fn a_duplicate_participant_is_reported() {
        let parsed = input(
            r#"{ "version": 1, "kind": "sequence",
                 "participants": [ { "id": "api" }, { "id": "api" } ] }"#,
        );

        let errors = SequenceKind
            .validate(&parsed)
            .expect_err("los actores se repiten");

        assert_eq!(errors[0].code, gx_core::error::ErrorCode::DuplicateId);
    }

    #[test]
    fn a_message_without_a_label_is_reported() {
        let parsed = input(
            r#"{ "version": 1, "kind": "sequence",
                 "participants": [ { "id": "api" } ],
                 "messages": [ { "from": "api", "to": "api", "label": "  " } ] }"#,
        );

        let errors = SequenceKind
            .validate(&parsed)
            .expect_err("un mensaje sin texto no dice nada");

        assert_eq!(errors[0].path, "messages[0].label");
    }
}
