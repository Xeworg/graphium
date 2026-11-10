//! El kind `swimlane`: un proceso repartido en carriles (PRD §6).
//!
//! Los carriles son las áreas que hacen el trabajo —soporte, negocio,
//! logística— y los pasos son las etapas del proceso: el orden del array de
//! pasos **es** el orden del proceso, así que el documento no numera nada y el
//! layout no adivina. Va sobre la IR de timeline y el motor `lanes`: el mismo
//! que dibuja una cronología, con carriles obligatorios porque un proceso sin
//! áreas no es un swimlane.

use gx_core::error::{ErrorCode, ValidationError};
use gx_core::ir::{Ir, IrEvent, IrLane, IrTimeline, LayoutHint, Shape};
use gx_core::kind::{DiagramKind, KindLimits};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::lanes::{duplicate_event, event_id, unknown_lane, validate_lanes, Lane, SeenIds};

/// Contract version supported by this build (PRD §10.1).
pub const SUPPORTED_VERSION: u32 = 1;

/// One step of the process.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Step {
    /// Id of the step. Defaults to its position in the array.
    #[serde(default)]
    pub id: Option<String>,
    /// What happens in this step, e.g. `abre el ticket`.
    pub label: String,
    /// Lane that takes the step. Must be one of the declared lanes.
    pub lane: String,
    /// Column of the step. Defaults to its position in the array.
    #[serde(default)]
    pub order: Option<i64>,
}

/// The JSON contract of the `swimlane` kind.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SwimlaneInput {
    /// Contract version. Only `1` is supported.
    pub version: u32,
    /// Must be `swimlane`.
    pub kind: String,
    /// Human readable title of the diagram.
    #[serde(default)]
    pub title: Option<String>,
    /// The lanes, in the order they stack from top to bottom.
    pub lanes: Vec<Lane>,
    /// The steps, in the order the process happens.
    pub steps: Vec<Step>,
}

/// The `swimlane` kind.
#[derive(Debug, Default)]
pub struct SwimlaneKind;

impl DiagramKind for SwimlaneKind {
    const KIND: &'static str = "swimlane";
    const PHASE: &'static str = "F2";
    const LAYOUT_HINT: LayoutHint = LayoutHint::Lanes;
    const LIMITS: KindLimits = KindLimits::TIMELINE;

    type Input = SwimlaneInput;

    fn schema() -> schemars::Schema {
        schemars::schema_for!(SwimlaneInput)
    }

    fn validate(&self, input: &Self::Input) -> Result<(), Vec<ValidationError>> {
        let mut errors =
            crate::flow::validate_header(Self::KIND, &input.kind, SUPPORTED_VERSION, input.version);

        if input.lanes.is_empty() {
            errors.push(ValidationError::new(
                "lanes",
                ErrorCode::MissingField,
                "un proceso con carriles necesita al menos un carril",
                Some(
                    "agregá quién hace el trabajo, por ejemplo `[{ \"id\": \"soporte\" }]`"
                        .to_string(),
                ),
            ));
        }
        errors.extend(validate_lanes(&input.lanes, "lanes"));

        if input.steps.is_empty() {
            errors.push(ValidationError::new(
                "steps",
                ErrorCode::MissingField,
                "un proceso necesita al menos un paso",
                Some(
                    "agregá qué pasa, por ejemplo `[{ \"label\": \"abre el ticket\", \"lane\": \"soporte\" }]`"
                        .to_string(),
                ),
            ));
        }

        let mut ids = SeenIds::new();
        for (index, step) in input.steps.iter().enumerate() {
            if step.label.trim().is_empty() {
                errors.push(ValidationError::new(
                    format!("steps[{index}].label"),
                    ErrorCode::MissingField,
                    "un paso necesita decir qué pasa",
                    Some("escribí el texto del paso, por ejemplo `abre el ticket`".to_string()),
                ));
            }
            if !input.lanes.iter().any(|lane| lane.id == step.lane) {
                errors.push(unknown_lane(
                    &format!("steps[{index}].lane"),
                    &step.lane,
                    &input.lanes,
                ));
            }
            let id = event_id(&step.id, "step-", index);
            if ids.insert(&id) {
                errors.push(duplicate_event(&format!("steps[{index}].id"), &id));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn to_ir(&self, input: &Self::Input) -> Ir {
        let lanes: Vec<IrLane> = input.lanes.iter().map(Lane::to_ir).collect();
        let events: Vec<IrEvent> = input
            .steps
            .iter()
            .enumerate()
            .map(|(index, step)| IrEvent {
                shape: Shape::Rounded,
                id: event_id(&step.id, "step-", index),
                lane: step.lane.clone(),
                label: step.label.clone(),
                order: step.order.unwrap_or(index as i64),
                // Un paso es un punto del proceso, no un tramo de tiempo: el
                // ancho de la caja es su texto.
                span: None,
                depends_on: Vec::new(),
                kind_data: Default::default(),
            })
            .collect();

        Ir::Timeline(IrTimeline { lanes, events })
    }

    /// El carril es la fila y el orden de los pasos es el orden del proceso:
    /// las dos cosas que un proceso declara y el schema no puede atar.
    fn rules(&self) -> Vec<String> {
        vec![
            "Un proceso necesita al menos un carril y al menos un paso.".to_string(),
            "Cada paso referencia con `lane` un carril declarado, y el orden de `steps` es el orden del proceso."
                .to_string(),
        ]
    }

    fn min_example() -> serde_json::Value {
        serde_json::json!({
            "version": 1,
            "kind": "swimlane",
            "title": "soporte",
            "lanes": [ { "id": "cliente" }, { "id": "soporte" } ],
            "steps": [
                { "label": "reclama", "lane": "cliente" },
                { "label": "responde", "lane": "soporte" }
            ]
        })
    }
}

/// Registers the kind in the global registry.
pub fn register() {
    gx_core::kind::register_kind::<SwimlaneKind>();
}

#[cfg(test)]
mod tests {
    use super::SwimlaneKind;
    use crate::test_support::input;
    use gx_core::error::ErrorCode;
    use gx_core::ir::Ir;
    use gx_core::kind::DiagramKind;

    const SOPORTE: &str = r#"{
      "version": 1, "kind": "swimlane", "title": "pedido trabado",
      "lanes": [ { "id": "cliente", "label": "Cliente" }, { "id": "soporte" } ],
      "steps": [
        { "label": "reclama", "lane": "cliente" },
        { "label": "abre el ticket", "lane": "soporte" },
        { "label": "responde", "lane": "soporte" }
      ]
    }"#;

    /// S1: el proceso se traduce a la IR de timeline, con cada paso en su
    /// carril y en el orden del documento.
    #[test]
    fn a_swimlane_becomes_a_timeline_ir() {
        let parsed = input(SOPORTE);
        SwimlaneKind
            .validate(&parsed)
            .expect("el proceso es válido");

        let Ir::Timeline(timeline) = SwimlaneKind.to_ir(&parsed) else {
            panic!("un swimlane va a la IR de timeline");
        };

        assert_eq!(timeline.lanes.len(), 2);
        assert_eq!(timeline.lanes[1].label.as_deref(), Some("soporte"));
        assert_eq!(timeline.events.len(), 3);
        assert_eq!(timeline.events[0].lane, "cliente");
        assert_eq!(timeline.events[1].lane, "soporte");
        // El orden es el del array, sin numerar nada en el documento.
        assert_eq!(timeline.events[2].order, 2);
        assert_eq!(timeline.events[2].id, "step-2");
        // Un paso no es un tramo de tiempo.
        assert!(timeline.events.iter().all(|event| event.span.is_none()));
    }

    #[test]
    fn a_process_without_lanes_is_reported() {
        let parsed = input(
            r#"{ "version": 1, "kind": "swimlane", "lanes": [],
                 "steps": [ { "label": "hola", "lane": "soporte" } ] }"#,
        );

        let errors = SwimlaneKind
            .validate(&parsed)
            .expect_err("sin carriles no hay proceso");

        assert_eq!(errors[0].code, ErrorCode::MissingField);
        assert_eq!(errors[0].path, "lanes");
    }

    #[test]
    fn a_step_in_an_unknown_lane_is_reported() {
        let parsed = input(
            r#"{ "version": 1, "kind": "swimlane",
                 "lanes": [ { "id": "soporte" } ],
                 "steps": [ { "label": "hola", "lane": "nadie" } ] }"#,
        );

        let errors = SwimlaneKind
            .validate(&parsed)
            .expect_err("el carril no existe");

        assert_eq!(errors[0].code, ErrorCode::UnknownRef);
        assert_eq!(errors[0].path, "steps[0].lane");
        assert!(errors[0]
            .hint
            .as_deref()
            .expect("dice cuáles hay")
            .contains("soporte"));
    }

    #[test]
    fn two_lanes_or_two_steps_cannot_share_an_id() {
        let lanes = input(
            r#"{ "version": 1, "kind": "swimlane",
                 "lanes": [ { "id": "a" }, { "id": "a" } ],
                 "steps": [ { "label": "uno", "lane": "a" } ] }"#,
        );
        let lane_errors = SwimlaneKind
            .validate(&lanes)
            .expect_err("los carriles chocan");
        assert_eq!(lane_errors[0].code, ErrorCode::DuplicateId);
        assert_eq!(lane_errors[0].path, "lanes[1].id");

        let steps = input(
            r#"{ "version": 1, "kind": "swimlane",
                 "lanes": [ { "id": "a" } ],
                 "steps": [
                   { "id": "x", "label": "uno", "lane": "a" },
                   { "id": "x", "label": "dos", "lane": "a" } ] }"#,
        );
        let step_errors = SwimlaneKind.validate(&steps).expect_err("los pasos chocan");
        assert_eq!(step_errors[0].code, ErrorCode::DuplicateId);
        assert_eq!(step_errors[0].path, "steps[1].id");
    }

    #[test]
    fn a_process_without_steps_is_reported() {
        let parsed = input(
            r#"{ "version": 1, "kind": "swimlane",
                 "lanes": [ { "id": "soporte" } ], "steps": [] }"#,
        );

        let errors = SwimlaneKind
            .validate(&parsed)
            .expect_err("sin pasos no hay proceso");

        assert_eq!(errors[0].code, ErrorCode::MissingField);
        assert_eq!(errors[0].path, "steps");
    }
}
