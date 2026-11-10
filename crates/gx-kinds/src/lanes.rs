//! Lo que comparten los kinds de carriles: el contrato de un carril y las
//! reglas de id de los eventos (PRD §6, `timeline` y `swimlane`).
//!
//! Un carril es una fila del dibujo y un evento es una caja sobre su fila. Las
//! dos cosas son parte de la IR (`IrLane`, `IrEvent`), así que los dos kinds
//! solo declaran su documento y traducen: no inventan una segunda forma de
//! decir lo mismo.

use std::collections::BTreeSet;

use gx_core::error::{ErrorCode, ValidationError};
use gx_core::ir::IrLane;
use schemars::JsonSchema;
use serde::Deserialize;

/// Un carril declarado por el documento.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Lane {
    /// Unique identifier of the lane inside the diagram.
    pub id: String,
    /// Name the lane shows. Defaults to the id.
    #[serde(default)]
    pub label: Option<String>,
}

impl Lane {
    /// The lane of the IR: the label falls back to the id.
    pub fn to_ir(&self) -> IrLane {
        IrLane {
            id: self.id.clone(),
            label: self
                .label
                .clone()
                .filter(|label| !label.trim().is_empty())
                .or_else(|| Some(self.id.clone())),
        }
    }
}

/// Validates the declared lanes: an id is never empty and never repeated.
///
/// `field` is the path the errors name, so `timeline` and `swimlane` report
/// their own document shape.
pub fn validate_lanes(lanes: &[Lane], field: &str) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for (index, lane) in lanes.iter().enumerate() {
        if lane.id.trim().is_empty() {
            errors.push(ValidationError::new(
                format!("{field}[{index}].id"),
                ErrorCode::InvalidValue,
                "el id del carril no puede estar vacío",
                Some("poné el nombre del carril, por ejemplo `soporte`".to_string()),
            ));
        }
        if seen.contains(&lane.id.as_str()) {
            errors.push(ValidationError::new(
                format!("{field}[{index}].id"),
                ErrorCode::DuplicateId,
                format!("carril repetido: `{}`", lane.id),
                Some("los ids de carril deben ser únicos".to_string()),
            ));
        }
        seen.push(lane.id.as_str());
    }
    errors
}

/// The error of an event that names a lane the document does not declare.
pub fn unknown_lane(field: &str, lane: &str, declared: &[Lane]) -> ValidationError {
    let known: Vec<&str> = declared.iter().map(|lane| lane.id.as_str()).collect();
    let hint = if known.is_empty() {
        "declará el carril en `lanes`".to_string()
    } else {
        format!("usá uno de los carriles declarados: {}", known.join(", "))
    };
    ValidationError::new(
        field,
        ErrorCode::UnknownRef,
        format!("carril desconocido: `{lane}`"),
        Some(hint),
    )
}

/// The id of an event: the one the document gives, or one derived from its
/// position so the drawing is deterministic and no id is invented twice.
pub fn event_id(explicit: &Option<String>, prefix: &str, index: usize) -> String {
    match explicit {
        Some(id) if !id.trim().is_empty() => id.clone(),
        _ => format!("{prefix}{index}"),
    }
}

/// The error of two events that would answer to the same id.
pub fn duplicate_event(field: &str, id: &str) -> ValidationError {
    ValidationError::new(
        field,
        ErrorCode::DuplicateId,
        format!("evento repetido: `{id}`"),
        Some(
            "poné un `id` distinto en cada evento o quitá los `id` para que salgan del orden"
                .to_string(),
        ),
    )
}

/// The lane check every lanes kind repeats per row (`f20-reuso.md`, S7): the
/// lane is required when the document declares lanes, and always one the
/// document declared. `noun` is what the row is — `evento`, `tarea` — so the
/// error speaks the document's language, not the helper's.
pub fn validate_row_lane(
    field: &str,
    noun: &str,
    lane: &Option<String>,
    declared: &[Lane],
) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    match (lane, declared.is_empty()) {
        (Some(lane), _) if !declared.iter().any(|known| &known.id == lane) => {
            errors.push(unknown_lane(field, lane, declared));
        }
        (None, false) => errors.push(ValidationError::new(
            field,
            ErrorCode::MissingField,
            format!("el {noun} no dice en qué carril va"),
            Some(format!(
                "usá uno de los carriles declarados: {}",
                declared
                    .iter()
                    .map(|lane| lane.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        )),
        _ => {}
    }
    errors
}

/// The ids that already exist, so the caller reports the first repeated one.
pub struct SeenIds(BTreeSet<String>);

impl SeenIds {
    pub fn new() -> Self {
        Self(BTreeSet::new())
    }

    /// Records an id and says whether it was already there.
    pub fn insert(&mut self, id: &str) -> bool {
        !self.0.insert(id.to_string())
    }
}

impl Default for SeenIds {
    fn default() -> Self {
        Self::new()
    }
}
