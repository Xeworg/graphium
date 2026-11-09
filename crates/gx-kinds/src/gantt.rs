//! El kind `gantt`: un cronograma de proyecto sobre el motor `lanes`
//! (PRD §6, fila 20).
//!
//! Los carriles son las filas —los frentes de un plan— y son opcionales: un
//! cronograma de una sola línea no necesita declarar ninguno. Cada tarea es un
//! tramo de tiempo (`start` → `end`); cuando ambos caen en el mismo día la
//! tarea es un hito y se dibuja como un punto. `depends_on` declara qué tiene
//! que terminar antes de que la tarea arranque: la escena lo dibuja como una
//! flecha de la tarea previa a la que la espera.
//!
//! Las dependencias no viajan en `kind_data`: `IrValue` no tiene variante de
//! lista, y codificar ids en un string colisiona con ids libres. Viajan en su
//! campo propio `IrEvent.depends_on`.

use gx_core::error::{ErrorCode, ValidationError};
use gx_core::ir::{Ir, IrEvent, IrLane, IrTimeline, LayoutHint, Span};
use gx_core::kind::{DiagramKind, KindLimits};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::flow::validate_header;
use crate::lanes::{duplicate_event, event_id, unknown_lane, validate_lanes, Lane, SeenIds};

/// Contract version supported by this build (PRD §10.1).
pub const SUPPORTED_VERSION: u32 = 1;

/// El carril que se usa cuando el documento no declara ninguno: un plan de una
/// sola línea.
pub const SINGLE_LANE: &str = "cronograma";

/// Una tarea del cronograma.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GanttTask {
    /// Id of the task. Defaults to its position in the array.
    #[serde(default)]
    pub id: Option<String>,
    /// Qué se hace, p. ej. `Demolición`.
    pub label: String,
    /// Lane the task sits on. Required when the document declares lanes.
    #[serde(default)]
    pub lane: Option<String>,
    /// Day (or unit of work) the task starts on. The column of the bar.
    pub start: i64,
    /// Day the task ends on. Equal to `start` makes the task a milestone.
    pub end: i64,
    /// Task ids that have to finish before this task starts. Each one must
    /// name a task declared in `tasks`, and no task depends on itself.
    #[serde(default)]
    pub depends_on: Vec<String>,
}

/// The JSON contract of the `gantt` kind.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GanttInput {
    /// Contract version. Only `1` is supported.
    pub version: u32,
    /// Must be `gantt`.
    pub kind: String,
    /// Human readable title of the diagram.
    #[serde(default)]
    pub title: Option<String>,
    /// The lanes, in the order they stack from top to bottom.
    #[serde(default)]
    pub lanes: Vec<Lane>,
    /// The tasks, in any order: `start` decides the time.
    pub tasks: Vec<GanttTask>,
}

/// The `gantt` kind.
#[derive(Debug, Default)]
pub struct GanttKind;

impl DiagramKind for GanttKind {
    const KIND: &'static str = "gantt";
    const LAYOUT_HINT: LayoutHint = LayoutHint::Lanes;
    const LIMITS: KindLimits = KindLimits {
        max_nodes: 200,
        max_edges: 0,
        max_density: 0.0,
        max_lanes: 40,
        max_events: 200,
    };

    type Input = GanttInput;

    fn schema() -> schemars::Schema {
        schemars::schema_for!(GanttInput)
    }

    fn validate(&self, input: &Self::Input) -> Result<(), Vec<ValidationError>> {
        let mut errors = validate_header(Self::KIND, &input.kind, SUPPORTED_VERSION, input.version);

        errors.extend(validate_lanes(&input.lanes, "lanes"));

        if input.tasks.is_empty() {
            errors.push(ValidationError::new(
                "tasks",
                ErrorCode::MissingField,
                "un cronograma necesita al menos una tarea",
                Some(
                    "agregá qué se hace, por ejemplo `[{ \"label\": \"Kickoff\", \"start\": 0, \"end\": 2 }]`"
                        .to_string(),
                ),
            ));
        }

        let lanes_declared = !input.lanes.is_empty();
        let mut ids = SeenIds::new();
        let mut resolved: Vec<String> = Vec::new();
        for (index, task) in input.tasks.iter().enumerate() {
            if task.label.trim().is_empty() {
                errors.push(ValidationError::new(
                    format!("tasks[{index}].label"),
                    ErrorCode::MissingField,
                    "una tarea necesita decir qué se hace",
                    Some("escribí el texto de la tarea, por ejemplo `Demolición`".to_string()),
                ));
            }

            // El carril: obligatorio cuando hay carriles, y siempre uno que el
            // documento haya declarado. La regla compartida de `timeline`.
            match (&task.lane, lanes_declared) {
                (Some(lane), _) if !input.lanes.iter().any(|known| &known.id == lane) => {
                    errors.push(unknown_lane(
                        &format!("tasks[{index}].lane"),
                        lane,
                        &input.lanes,
                    ));
                }
                (None, true) => errors.push(ValidationError::new(
                    format!("tasks[{index}].lane"),
                    ErrorCode::MissingField,
                    "la tarea no dice en qué carril va",
                    Some(format!(
                        "usá uno de los carriles declarados: {}",
                        input
                            .lanes
                            .iter()
                            .map(|lane| lane.id.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )),
                )),
                _ => {}
            }

            let id = event_id(&task.id, "task-", index);

            if task.end < task.start {
                errors.push(ValidationError::new(
                    format!("tasks[{index}].end"),
                    ErrorCode::InvalidValue,
                    format!(
                        "la tarea `{id}` termina en {} y empieza en {}: el final no puede ser anterior",
                        task.end, task.start
                    ),
                    Some(
                        "corré `end`, o poné `end` igual a `start` cuando la tarea es un hito"
                            .to_string(),
                    ),
                ));
            }

            if ids.insert(&id) {
                errors.push(duplicate_event(&format!("tasks[{index}].id"), &id));
            }
            resolved.push(id);
        }

        // La disciplina de las dependencias (S6): cada una nombra una tarea
        // declarada, ninguna se repite dentro de su tarea y nadie depende de
        // sí mismo. Los ciclos entre tareas se dibujan, no se validan: el
        // motor de carriles es columnar, no topológico.
        for (index, task) in input.tasks.iter().enumerate() {
            let id = &resolved[index];
            let mut seen: Vec<&str> = Vec::new();
            for (position, dep) in task.depends_on.iter().enumerate() {
                let field = format!("tasks[{index}].depends_on[{position}]");
                if !resolved.iter().any(|known| known == dep) {
                    errors.push(ValidationError::new(
                        field,
                        ErrorCode::UnknownRef,
                        format!("dependencia desconocida de `{id}`: `{dep}`"),
                        Some(
                            "las dependencias referencian tareas declaradas en `tasks`".to_string(),
                        ),
                    ));
                } else if dep == id {
                    errors.push(ValidationError::new(
                        field,
                        ErrorCode::InvalidValue,
                        format!("la tarea `{id}` depende de sí misma"),
                        Some(format!("quitá `{id}` de su propio `depends_on`")),
                    ));
                } else if seen.contains(&dep.as_str()) {
                    errors.push(ValidationError::new(
                        field,
                        ErrorCode::InvalidValue,
                        format!("`depends_on` de `{id}` repite la dependencia `{dep}`"),
                        Some("cada dependencia se declara una vez".to_string()),
                    ));
                }
                seen.push(dep);
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

        let tasks: Vec<String> = input
            .tasks
            .iter()
            .enumerate()
            .map(|(index, task)| event_id(&task.id, "task-", index))
            .collect();

        let events: Vec<IrEvent> = input
            .tasks
            .iter()
            .enumerate()
            .map(|(index, task)| {
                IrEvent {
                    id: tasks[index].clone(),
                    lane: match &task.lane {
                        Some(lane) => lane.clone(),
                        None => SINGLE_LANE.to_string(),
                    },
                    label: task.label.clone(),
                    // La columna de la tarea es su comienzo: el motor ordena
                    // por `order`, como el `order` de un evento de timeline.
                    order: task.start,
                    // Una tarea siempre ocupa un tramo; cuando `end` cae sobre
                    // `start` el tramo es de largo cero y el motor la dibuja
                    // como un punto: el hito.
                    span: Some(Span {
                        start: task.start as f64,
                        end: task.end as f64,
                    }),
                    kind_data: Default::default(),
                    depends_on: task.depends_on.clone(),
                }
            })
            .collect();

        Ir::Timeline(IrTimeline { lanes, events })
    }
}

/// Registers the `gantt` kind in the global registry.
pub fn register() {
    gx_core::kind::register_kind::<GanttKind>();
}
