//! The `gantt` contract: tasks as time spans with dependency discipline
//! (feature doc `f18-gantt.md`, S5, S6).
//!
//! A gantt document is a schedule: lanes are the rows, `start`/`end` put every
//! task on the time axis, and `depends_on` declares what has to finish first.
//! These tests pin the contract the agent writes and the IR it gets.

use gx_core::error::ErrorCode;
use gx_core::ir::{Ir, Span};
use gx_core::kind::DiagramKind;
use gx_kinds::gantt::{GanttInput, GanttKind};

/// The plan of the feature doc: four tasks on two lanes, two dependencies and
/// one milestone (`start == end`).
const PLAN: &str = r#"{
  "version": 1,
  "kind": "gantt",
  "title": "Restauración — plan",
  "lanes": [
    { "id": "diseno", "label": "Diseño" },
    { "id": "obra", "label": "Obra" }
  ],
  "tasks": [
    { "id": "planos", "label": "Planos", "lane": "diseno",
      "start": 0, "end": 3 },
    { "id": "permiso", "label": "Permiso municipal", "lane": "diseno",
      "start": 2, "end": 5 },
    { "id": "demolicion", "label": "Demolición", "lane": "obra",
      "start": 5, "end": 9, "depends_on": ["planos", "permiso"] },
    { "id": "apertura", "label": "Apertura", "lane": "obra",
      "start": 9, "end": 9, "depends_on": ["demolicion"] }
  ]
}"#;

fn input(json: &str) -> GanttInput {
    serde_json::from_str(json).expect("the document parses")
}

/// The errors of a document, panicking when it is valid.
fn errors_of(input: &GanttInput) -> Vec<gx_core::error::ValidationError> {
    match GanttKind.validate(input) {
        Ok(()) => panic!("the document has to be rejected"),
        Err(errors) => errors,
    }
}

/// S5: a plan with lanes, spans and dependencies is valid.
#[test]
fn a_plan_document_validates() {
    let parsed = input(PLAN);

    if let Err(errors) = GanttKind.validate(&parsed) {
        panic!("the plan document is valid: {errors:?}");
    }
}

/// S5: a schedule without lanes lands on the single implicit lane.
#[test]
fn a_schedule_without_lanes_lands_on_the_single_lane() {
    let document = serde_json::json!({
        "version": 1,
        "kind": "gantt",
        "tasks": [ { "label": "Kickoff", "start": 0, "end": 2 } ],
    });
    let parsed: GanttInput = serde_json::from_value(document).expect("the document parses");

    if let Err(errors) = GanttKind.validate(&parsed) {
        panic!("a one-lane schedule is valid: {errors:?}");
    }

    let ir = GanttKind.to_ir(&parsed);
    let timeline = ir.timeline().expect("gantt maps to Ir::Timeline");
    assert_eq!(timeline.lanes.len(), 1, "one implicit lane");
}

/// S6: a task ending before it starts names the task.
#[test]
fn a_task_ending_before_it_starts_names_the_task() {
    let document = PLAN.replace(
        r#"{ "id": "demolicion", "label": "Demolición", "lane": "obra",
      "start": 5, "end": 9, "depends_on": ["planos", "permiso"] }"#,
        r#"{ "id": "demolicion", "label": "Demolición", "lane": "obra",
      "start": 9, "end": 5, "depends_on": ["planos", "permiso"] }"#,
    );
    let parsed = input(&document);
    let errors = errors_of(&parsed);

    assert!(
        errors.iter().any(|error| {
            error.code == ErrorCode::InvalidValue && error.message.contains("demolicion")
        }),
        "the error names the task: {errors:?}"
    );
}

/// S6: a milestone — `start == end` — is a point, not an error.
#[test]
fn a_milestone_starting_and_ending_on_the_same_day_is_a_point() {
    let parsed = input(PLAN);
    let ir = GanttKind.to_ir(&parsed);
    let timeline = ir.timeline().expect("gantt maps to Ir::Timeline");

    let milestone = timeline
        .events
        .iter()
        .find(|event| event.id == "apertura")
        .expect("the milestone is on the timeline");
    let span = milestone.span.expect("the milestone carries its span");
    assert_eq!(
        span,
        Span {
            start: 9.0,
            end: 9.0
        }
    );
}

/// S6: a dependency naming an unknown task names the missing id.
#[test]
fn an_unknown_dependency_names_the_missing_id() {
    let document = PLAN.replace(
        "\"depends_on\": [\"demolicion\"]",
        "\"depends_on\": [\"demolicion\", \"pintura\"]",
    );
    let parsed = input(&document);
    let errors = errors_of(&parsed);

    assert!(
        errors
            .iter()
            .any(|error| error.code == ErrorCode::UnknownRef && error.message.contains("pintura")),
        "the error names the missing id: {errors:?}"
    );
}

/// S6: a task cannot depend on itself.
#[test]
fn a_task_that_depends_on_itself_is_rejected() {
    let document = PLAN.replace(
        "\"depends_on\": [\"planos\", \"permiso\"]",
        "\"depends_on\": [\"planos\", \"permiso\", \"demolicion\"]",
    );
    let parsed = input(&document);
    let errors = errors_of(&parsed);

    assert!(
        errors
            .iter()
            .any(|error| error.code == ErrorCode::InvalidValue
                && error.message.contains("demolicion")),
        "the self dependency is an error: {errors:?}"
    );
}

/// S6: the same dependency twice in one task is an error.
#[test]
fn a_duplicated_dependency_is_rejected() {
    let document = PLAN.replace(
        "\"depends_on\": [\"planos\", \"permiso\"]",
        "\"depends_on\": [\"planos\", \"planos\"]",
    );
    let parsed = input(&document);
    let errors = errors_of(&parsed);

    assert!(
        errors
            .iter()
            .any(|error| error.code == ErrorCode::InvalidValue),
        "the duplicated dependency is an error: {errors:?}"
    );
}

/// S5: two tasks with the same id are rejected.
#[test]
fn two_tasks_with_the_same_id_are_rejected() {
    let document = PLAN.replace(
        r#"{ "id": "apertura", "label": "Apertura", "lane": "obra",
      "start": 9, "end": 9, "depends_on": ["demolicion"] }"#,
        r#"{ "id": "planos", "label": "Apertura", "lane": "obra",
      "start": 9, "end": 9, "depends_on": ["demolicion"] }"#,
    );
    let parsed = input(&document);
    let errors = errors_of(&parsed);

    assert!(
        errors
            .iter()
            .any(|error| error.code == ErrorCode::DuplicateId),
        "the duplicated id is an error: {errors:?}"
    );
}

/// S5: a task on a lane the document never declared is rejected.
#[test]
fn a_task_on_an_unknown_lane_is_rejected() {
    let document = PLAN.replace("\"lane\": \"obra\",", "\"lane\": \"limpieza\",");
    let parsed = input(&document);
    let errors = errors_of(&parsed);

    assert!(
        errors.iter().any(|error| error.path.contains("lane")),
        "the unknown lane is an error: {errors:?}"
    );
}

/// S5: when the document declares lanes, every task says which one it is on.
#[test]
fn a_task_without_lane_when_lanes_are_declared_is_rejected() {
    let document = PLAN.replace(
        r#"{ "id": "permiso", "label": "Permiso municipal", "lane": "diseno",
      "start": 2, "end": 5 },"#,
        r#"{ "id": "permiso", "label": "Permiso municipal",
      "start": 2, "end": 5 },"#,
    );
    let parsed = input(&document);
    let errors = errors_of(&parsed);

    assert!(
        errors
            .iter()
            .any(|error| error.code == ErrorCode::MissingField && error.path.contains("lane")),
        "the missing lane is an error: {errors:?}"
    );
}

/// S5: a schedule without tasks has nothing to draw.
#[test]
fn a_schedule_without_tasks_is_rejected() {
    let document = serde_json::json!({
        "version": 1,
        "kind": "gantt",
        "tasks": [],
    });
    let parsed: GanttInput = serde_json::from_value(document).expect("the document parses");
    let errors = errors_of(&parsed);

    assert!(
        errors
            .iter()
            .any(|error| error.code == ErrorCode::MissingField),
        "the empty schedule is an error: {errors:?}"
    );
}

/// S4: the IR is a timeline whose events carry the spans and the dependencies.
#[test]
fn the_plan_maps_to_a_timeline_of_spans_and_dependencies() {
    let parsed = input(PLAN);
    let ir = GanttKind.to_ir(&parsed);

    let timeline = match &ir {
        Ir::Timeline(timeline) => timeline,
        other => panic!("gantt maps to Ir::Timeline, got {other:?}"),
    };

    assert_eq!(timeline.lanes.len(), 2, "the declared lanes");
    assert_eq!(timeline.events.len(), 4, "the declared tasks");

    let demolicion = timeline
        .events
        .iter()
        .find(|event| event.id == "demolicion")
        .expect("the task is on the timeline");
    assert_eq!(demolicion.lane, "obra");
    assert_eq!(
        demolicion.span,
        Some(Span {
            start: 5.0,
            end: 9.0
        }),
        "the span is the task's stretch of time"
    );
    assert_eq!(
        demolicion.depends_on,
        vec!["planos".to_string(), "permiso".to_string()],
        "the dependencies travel with the event"
    );

    let planos = timeline
        .events
        .iter()
        .find(|event| event.id == "planos")
        .expect("the task is on the timeline");
    assert_eq!(planos.order, 0, "the column is the start");
    assert!(
        planos.depends_on.is_empty(),
        "a task without depends_on has no dependencies"
    );
}
