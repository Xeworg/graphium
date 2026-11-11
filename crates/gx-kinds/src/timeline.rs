//! El kind `timeline`: una cronología de eventos sobre el motor `lanes`
//! (PRD §6).
//!
//! El eje horizontal es el tiempo: el `order` de un evento decide su columna,
//! no su lugar en el array, así que un documento escrito en desorden se dibuja
//! en orden. Los carriles son las filas —las fases de un roadmap, los frentes
//! de un plan— y son opcionales: una cronología de una sola línea no necesita
//! declarar ninguno y el documento no repite el carril en cada evento.

use crate::flow::shape_of;
use gx_core::error::{ErrorCode, ValidationError};
use gx_core::ir::{Ir, IrEvent, IrLane, IrTimeline, LayoutHint, Shape, Span};
use gx_core::kind::{DiagramKind, KindLimits, Role};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::lanes::{duplicate_event, event_id, validate_lanes, Lane, SeenIds};

/// Contract version supported by this build (PRD §10.1).
pub const SUPPORTED_VERSION: u32 = 1;

/// El carril que se usa cuando el documento no declara ninguno: una cronología
/// de una sola línea.
pub const SINGLE_LANE: &str = "cronologia";

/// Los roles que un evento puede declarar (`f22-dibujo.md`, S6). Un evento sin
/// rol es un evento: su caja redondeada de siempre. El `milestone` es el punto
/// del tiempo que se lee distinto: su rombo lo dice sin leer el texto.
pub const ROLES: [Role; 1] = [Role::new("milestone", Shape::Diamond)];

/// One event of the chronology.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimelineEvent {
    /// Id of the event. Defaults to its position in the array.
    #[serde(default)]
    pub id: Option<String>,
    /// What happened, e.g. `v1.0`.
    pub label: String,
    /// Lane the event sits on. Required when the document declares lanes.
    #[serde(default)]
    pub lane: Option<String>,
    /// Column of the event. Defaults to its position in the array.
    #[serde(default)]
    pub order: Option<i64>,
    /// Column where the event ends, when it is a stretch of time and not a
    /// point. Draws the event as a bar from `order` to `end`.
    #[serde(default)]
    pub end: Option<i64>,
    /// Role of the event: it decides the shape (`f22-dibujo.md`, S6). The
    /// kind lists the roles it admits; the default event declares none and
    /// draws as its rounded box.
    #[serde(default)]
    pub role: Option<String>,
}

impl TimelineEvent {
    /// The column of the event: the `order` it declares or its position.
    fn column(&self, index: usize) -> i64 {
        self.order.unwrap_or(index as i64)
    }
}

/// The JSON contract of the `timeline` kind.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimelineInput {
    /// Contract version. Only `1` is supported.
    pub version: u32,
    /// Must be `timeline`.
    pub kind: String,
    /// Human readable title of the diagram.
    #[serde(default)]
    pub title: Option<String>,
    /// The lanes, in the order they stack from top to bottom.
    #[serde(default)]
    pub lanes: Vec<Lane>,
    /// The events, in any order: `order` decides the time.
    pub events: Vec<TimelineEvent>,
}

/// The `timeline` kind.
#[derive(Debug, Default)]
pub struct TimelineKind;

impl DiagramKind for TimelineKind {
    const KIND: &'static str = "timeline";
    const PHASE: &'static str = "F2";
    const LAYOUT_HINT: LayoutHint = LayoutHint::Lanes;
    const LIMITS: KindLimits = KindLimits::TIMELINE;
    const KIND_VERSION: u32 = SUPPORTED_VERSION;

    type Input = TimelineInput;

    fn schema() -> schemars::Schema {
        schemars::schema_for!(TimelineInput)
    }

    fn validate(&self, input: &Self::Input) -> Result<(), Vec<ValidationError>> {
        let mut errors =
            crate::flow::validate_header(Self::KIND, &input.kind, SUPPORTED_VERSION, input.version);

        errors.extend(validate_lanes(&input.lanes, "lanes"));

        if input.events.is_empty() {
            errors.push(ValidationError::new(
                "events",
                ErrorCode::MissingField,
                "una línea de tiempo necesita al menos un evento",
                Some("agregá qué pasó, por ejemplo `[{ \"label\": \"v1.0\" }]`".to_string()),
            ));
        }

        let _lanes_declared = !input.lanes.is_empty();
        let mut ids = SeenIds::new();
        for (index, event) in input.events.iter().enumerate() {
            if event.label.trim().is_empty() {
                errors.push(ValidationError::new(
                    format!("events[{index}].label"),
                    ErrorCode::MissingField,
                    "un evento necesita decir qué pasó",
                    Some("escribí el texto del evento, por ejemplo `v1.0`".to_string()),
                ));
            }

            errors.extend(crate::lanes::validate_row_lane(
                &format!("events[{index}].lane"),
                "evento",
                &event.lane,
                &input.lanes,
            ));

            let column = event.column(index);
            if let Some(role) = &event.role {
                if !ROLES.iter().any(|known| known.name == role) {
                    let known = ROLES
                        .iter()
                        .map(|known| format!("`{}`", known.name))
                        .collect::<Vec<_>>()
                        .join(", ");
                    errors.push(ValidationError::new(
                        format!("events[{index}].role"),
                        ErrorCode::InvalidValue,
                        format!("el rol `{role}` no es un rol de un evento"),
                        Some(format!("los roles que un evento puede llevar son: {known}")),
                    ));
                }
            }
            // Un hito es un punto (`f22-dibujo.md`, S6): un hito con tramo es
            // dos cosas a la vez, y el documento que lo escribe se confundió.
            if event.role.as_deref() == Some("milestone") && event.end.is_some() {
                errors.push(ValidationError::new(
                    format!("events[{index}].end"),
                    ErrorCode::InvalidValue,
                    "un `milestone` es un punto del tiempo: no puede llevar `end`",
                    Some("quitá el `end`, o quitá el `role` si el evento es un tramo".to_string()),
                ));
            }
            if let Some(end) = event.end {
                if end <= column {
                    errors.push(ValidationError::new(
                        format!("events[{index}].end"),
                        ErrorCode::InvalidValue,
                        format!(
                            "el tramo del evento termina en {end} y empieza en {column}: el final tiene que ser posterior"
                        ),
                        Some("subí `end`, o quitá el campo si el evento es un punto en el tiempo".to_string()),
                    ));
                }
            }

            let id = event_id(&event.id, "event-", index);
            if ids.insert(&id) {
                errors.push(duplicate_event(&format!("events[{index}].id"), &id));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn to_ir(&self, input: &Self::Input) -> Ir {
        let lanes: Vec<IrLane> = if input.lanes.is_empty() {
            vec![IrLane {
                id: SINGLE_LANE.to_string(),
                label: None,
            }]
        } else {
            input.lanes.iter().map(Lane::to_ir).collect()
        };

        let events: Vec<IrEvent> = input
            .events
            .iter()
            .enumerate()
            .map(|(index, event)| {
                let column = event.column(index);
                IrEvent {
                    // El rol decide la forma (`f22-dibujo.md`, S6): un evento
                    // sin rol es la caja redondeada de siempre; un `milestone`
                    // dibuja su rombo.
                    shape: shape_of(&ROLES, event.role.as_deref().unwrap_or("")),
                    id: event_id(&event.id, "event-", index),
                    lane: match &event.lane {
                        Some(lane) => lane.clone(),
                        None => SINGLE_LANE.to_string(),
                    },
                    label: event.label.clone(),
                    order: column,
                    // Un evento sin `end` es un punto: el motor lo dibuja en su
                    // columna y no le inventa un ancho.
                    span: event.end.map(|end| Span {
                        start: column as f64,
                        end: end as f64,
                    }),
                    depends_on: Vec::new(),
                    kind_data: Default::default(),
                }
            })
            .collect();

        Ir::Timeline(IrTimeline {
            lanes,
            events,
            connections: Vec::new(),
        })
    }

    /// La línea de tiempo enseña su orden, sus carriles y sus tramos, que es lo
    /// que el schema no puede decir.
    fn rules(&self) -> Vec<String> {
        vec![
            "Cada `id` de carril es único, y `order` decide la columna: el array de eventos puede venir desordenado."
                .to_string(),
            "Si el documento declara `lanes`, cada evento dice su `lane` y referencia uno declarado."
                .to_string(),
            "Un evento con `end` es un tramo y `end` tiene que ser posterior a `order`."
                .to_string(),
        ]
    }

    fn min_example() -> serde_json::Value {
        serde_json::json!({
            "version": 1,
            "kind": "timeline",
            "title": "Roadmap",
            "lanes": [ { "id": "q1", "label": "Q1" } ],
            "events": [
                { "label": "v1.0", "lane": "q1", "role": "milestone" },
                { "label": "v1.1", "lane": "q1" }
            ]
        })
    }
}

/// Registers the kind in the global registry.
pub fn register() {
    gx_core::kind::register_kind::<TimelineKind>();
}

#[cfg(test)]
mod tests {
    use super::TimelineKind;
    use crate::test_support::input;
    use gx_core::error::ErrorCode;
    use gx_core::ir::Ir;
    use gx_core::kind::DiagramKind;

    const ROADMAP: &str = r#"{
      "version": 1, "kind": "timeline", "title": "Roadmap",
      "lanes": [ { "id": "q1", "label": "Q1" }, { "id": "q2", "label": "Q2" } ],
      "events": [
        { "label": "F0", "lane": "q1", "order": 0 },
        { "label": "F1", "lane": "q1", "order": 1, "end": 2 },
        { "label": "F2", "lane": "q2", "order": 2 }
      ]
    }"#;

    /// S1: la timeline se traduce a la IR de timeline, con sus carriles y sus
    /// eventos en las columnas que declaran.
    #[test]
    fn a_timeline_becomes_a_timeline_ir() {
        let parsed = input(ROADMAP);
        TimelineKind
            .validate(&parsed)
            .expect("la timeline es válida");

        let Ir::Timeline(timeline) = TimelineKind.to_ir(&parsed) else {
            panic!("una timeline va a la IR de timeline");
        };

        assert_eq!(timeline.lanes.len(), 2);
        assert_eq!(timeline.lanes[0].id, "q1");
        assert_eq!(timeline.lanes[0].label.as_deref(), Some("Q1"));
        assert_eq!(timeline.events.len(), 3);
        assert_eq!(timeline.events[1].order, 1);
        assert_eq!(timeline.events[1].lane, "q1");
        // Un `end` es un tramo; sin `end` el evento es un punto.
        let span = timeline.events[1].span.expect("F1 es un tramo");
        assert_eq!(span.start, 1.0);
        assert_eq!(span.end, 2.0);
        assert!(timeline.events[0].span.is_none());
    }

    /// S6 (`f22-dibujo.md`): el evento que declara el rol `milestone` se
    /// traduce con su rombo, y el que no declara rol queda como siempre.
    #[test]
    fn a_milestone_draws_its_diamond() {
        let parsed = input(
            r#"{ "version": 1, "kind": "timeline",
              "lanes": [ { "id": "q1", "label": "Q1" } ],
              "events": [
                { "label": "v1.0", "lane": "q1", "order": 0, "role": "milestone" },
                { "label": "spike", "lane": "q1", "order": 1 }
              ] }"#,
        );
        TimelineKind
            .validate(&parsed)
            .expect("la timeline es válida");

        let Ir::Timeline(timeline) = TimelineKind.to_ir(&parsed) else {
            panic!("una timeline va a la IR de timeline");
        };

        use gx_core::ir::Shape;
        assert_eq!(timeline.events[0].shape, Shape::Diamond);
        assert_eq!(timeline.events[1].shape, Shape::Rounded);
    }

    /// S6: un hito es un punto del tiempo — un `milestone` con `end` es dos
    /// cosas a la vez y el kind lo rechaza.
    #[test]
    fn a_milestone_with_an_end_is_rejected() {
        let parsed = input(
            r#"{ "version": 1, "kind": "timeline",
              "lanes": [ { "id": "q1", "label": "Q1" } ],
              "events": [
                { "label": "v1.0", "lane": "q1", "order": 0, "role": "milestone", "end": 2 }
              ] }"#,
        );

        let errors = TimelineKind.validate(&parsed).expect_err("rechazado");
        assert!(errors.iter().any(|error| error.path == "events[0].end"));
    }

    /// S6: un rol que el kind no conoce se rechaza con la lista completa —
    /// la misma disciplina que los roles de los nodos.
    #[test]
    fn an_unknown_event_role_is_rejected_with_the_list() {
        let parsed = input(
            r#"{ "version": 1, "kind": "timeline",
              "lanes": [ { "id": "q1", "label": "Q1" } ],
              "events": [
                { "label": "v1.0", "lane": "q1", "order": 0, "role": "crown" }
              ] }"#,
        );

        let errors = TimelineKind.validate(&parsed).expect_err("rechazado");
        let role_error = errors
            .iter()
            .find(|error| error.path == "events[0].role")
            .expect("el error del rol");
        let hint = role_error.hint.as_deref().expect("el error lleva su hint");
        assert!(
            hint.contains("milestone"),
            "el hint nombra los roles conocidos"
        );
    }

    /// S1: sin carriles declarados, el documento no repite el carril en cada
    /// evento y la IR le da uno solo.
    #[test]
    fn a_timeline_without_lanes_gets_a_single_row() {
        let parsed = input(
            r#"{ "version": 1, "kind": "timeline",
                 "events": [ { "label": "v1.0" }, { "label": "v1.1" } ] }"#,
        );
        TimelineKind
            .validate(&parsed)
            .expect("la cronología es válida");

        let Ir::Timeline(timeline) = TimelineKind.to_ir(&parsed) else {
            panic!("una timeline va a la IR de timeline");
        };

        assert_eq!(timeline.lanes.len(), 1);
        assert_eq!(timeline.events.len(), 2);
        assert_eq!(timeline.events[0].lane, timeline.lanes[0].id);
        // El orden sale de la posición cuando el documento no lo dice.
        assert_eq!(timeline.events[1].order, 1);
        // Los ids también, y son deterministas.
        assert_eq!(timeline.events[0].id, "event-0");
    }

    #[test]
    fn an_event_in_an_unknown_lane_is_reported() {
        let parsed = input(
            r#"{ "version": 1, "kind": "timeline",
                 "lanes": [ { "id": "q1" } ],
                 "events": [ { "label": "hola", "lane": "q9" } ] }"#,
        );

        let errors = TimelineKind
            .validate(&parsed)
            .expect_err("el carril no existe");

        assert_eq!(errors[0].code, ErrorCode::UnknownRef);
        assert_eq!(errors[0].path, "events[0].lane");
        assert!(errors[0].hint.is_some());
    }

    #[test]
    fn an_event_without_a_lane_is_reported_when_lanes_are_declared() {
        let parsed = input(
            r#"{ "version": 1, "kind": "timeline",
                 "lanes": [ { "id": "q1", "label": "Q1" } ],
                 "events": [ { "label": "hola" } ] }"#,
        );

        let errors = TimelineKind
            .validate(&parsed)
            .expect_err("el evento no dice su carril");

        assert_eq!(errors[0].code, ErrorCode::MissingField);
        assert_eq!(errors[0].path, "events[0].lane");
        assert!(errors[0]
            .hint
            .as_deref()
            .expect("dice cuáles hay")
            .contains("q1"));
    }

    #[test]
    fn a_span_that_ends_before_it_starts_is_reported() {
        let parsed = input(
            r#"{ "version": 1, "kind": "timeline",
                 "events": [ { "label": "tramo", "order": 3, "end": 2 } ] }"#,
        );

        let errors = TimelineKind
            .validate(&parsed)
            .expect_err("el tramo va al revés");

        assert_eq!(errors[0].code, ErrorCode::InvalidValue);
        assert_eq!(errors[0].path, "events[0].end");
    }

    #[test]
    fn two_events_cannot_share_an_id() {
        let parsed = input(
            r#"{ "version": 1, "kind": "timeline",
                 "events": [ { "id": "x", "label": "uno" }, { "id": "x", "label": "dos" } ] }"#,
        );

        let errors = TimelineKind.validate(&parsed).expect_err("los ids chocan");

        assert_eq!(errors[0].code, ErrorCode::DuplicateId);
        assert_eq!(errors[0].path, "events[1].id");
    }

    #[test]
    fn a_timeline_without_events_is_reported() {
        let parsed = input(r#"{ "version": 1, "kind": "timeline", "events": [] }"#);

        let errors = TimelineKind
            .validate(&parsed)
            .expect_err("sin eventos no hay cronología");

        assert_eq!(errors[0].code, ErrorCode::MissingField);
        assert_eq!(errors[0].path, "events");
    }
}
