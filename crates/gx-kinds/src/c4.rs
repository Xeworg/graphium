//! El kind `c4`: arquitectura en el modelo Context → Containers → Components.
//!
//! Un documento de `c4` es **plano**: no hay anidamiento en v1. La lectura sale
//! de tres lugares —el `level` declarado, el `type` de cada elemento y las
//! relaciones entre ellos—, así que el nivel es lo que disciplina el
//! vocabulario: en `context` se habla de personas y sistemas, en `containers` de
//! sistemas y contenedores, y en `components` de contenedores y componentes. Un
//! tipo que el nivel no admite es un error, no un estilo.
//!
//! Cada elemento se dibuja como una **caja** con su etiqueta en el encabezado y
//! su tipo en el compartimento de detalle —la forma que `er` usa para sus
//! filas—, con la tecnología a la derecha del tipo y la descripción debajo. Cada
//! relación es una arista con su etiqueta: el dibujo dice quién usa qué.

use gx_core::error::{ErrorCode, ValidationError};
use gx_core::ir::{Direction, Ir, IrEdge, IrGraph, IrNode, IrRow, LayoutHint, Shape};
use gx_core::kind::{DiagramKind, KindLimits};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::flow::validate_header;

/// Contract version supported by this build (PRD §10.1).
pub const SUPPORTED_VERSION: u32 = 1;

/// Los niveles del modelo C4 (S3).
pub const LEVELS: [&str; 3] = ["context", "containers", "components"];

/// Los tipos de elemento que existen en el modelo (S2).
pub const TYPES: [&str; 4] = ["person", "system", "container", "component"];

/// Los tipos que admite cada nivel (S3). Un nivel desconocido no admite
/// ninguno: el error del `level` ya nombra los tres que hay.
pub fn types_of(level: &str) -> &'static [&'static str] {
    match level {
        "context" => &["person", "system"],
        "containers" => &["system", "container"],
        "components" => &["container", "component"],
        _ => &[],
    }
}

/// Un elemento del diagrama: una persona, un sistema, un contenedor o un
/// componente.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct C4Element {
    /// Unique identifier inside the diagram.
    pub id: String,
    /// What the element is: `person`, `system`, `container` or `component`.
    /// The `level` decides which of them the document admits.
    pub r#type: String,
    /// El nombre que la caja muestra.
    pub label: String,
    /// Qué hace, en una línea. Se dibuja debajo del tipo.
    #[serde(default)]
    pub description: Option<String>,
    /// Con qué está hecho: `Rust`, `Postgres`, `React + Vite`. Se dibuja a la
    /// derecha del tipo.
    #[serde(default)]
    pub technology: Option<String>,
}

/// Una relación entre dos elementos: quién usa a quién y cómo se llama eso.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct C4Relation {
    /// Id of the source element.
    pub from: String,
    /// Id of the target element.
    pub to: String,
    /// El verbo de la relación: `usa`, `lee`, `cobra`.
    #[serde(default)]
    pub label: Option<String>,
}

/// The JSON contract of the `c4` kind.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct C4Input {
    /// Contract version. Only `1` is supported.
    pub version: u32,
    /// Must be `c4`.
    pub kind: String,
    /// Human readable title of the diagram.
    #[serde(default)]
    pub title: Option<String>,
    /// El nivel que el documento declara: `context`, `containers` o
    /// `components`. Decide qué tipos admite.
    pub level: String,
    /// Los elementos del diagrama.
    pub elements: Vec<C4Element>,
    /// Las relaciones entre elementos.
    #[serde(default)]
    pub relations: Vec<C4Relation>,
}

/// The `c4` kind.
#[derive(Debug, Default)]
pub struct C4Kind;

impl DiagramKind for C4Kind {
    const KIND: &'static str = "c4";
    const LAYOUT_HINT: LayoutHint = LayoutHint::Layered;
    const LIMITS: KindLimits = KindLimits {
        max_nodes: 300,
        max_edges: 600,
        max_density: 3.0,
        max_lanes: 0,
        max_events: 0,
    };

    type Input = C4Input;

    fn schema() -> schemars::Schema {
        schemars::schema_for!(C4Input)
    }

    fn shapes() -> Vec<Shape> {
        vec![Shape::Box]
    }

    fn validate(&self, input: &Self::Input) -> Result<(), Vec<ValidationError>> {
        let mut errors = validate_header(Self::KIND, &input.kind, SUPPORTED_VERSION, input.version);

        // El nivel decide el vocabulario del documento entero (S3). Un nivel
        // que no existe se reporta una vez, con los tres que hay, y no se
        // arrastra a los elementos: el agente arregla el nivel primero.
        let known_level = LEVELS.contains(&input.level.as_str());
        let admits = types_of(&input.level);
        if !known_level {
            errors.push(ValidationError::new(
                "level",
                ErrorCode::InvalidEnum,
                format!("nivel desconocido en `c4`: `{}`", input.level),
                Some(format!(
                    "los niveles son {}: `context` admite persona y sistema, `containers` sistema y contenedor, `components` contenedor y componente",
                    LEVELS.join(", ")
                )),
            ));
        }

        if input.elements.is_empty() {
            errors.push(ValidationError::new(
                "elements",
                ErrorCode::MissingField,
                "un diagrama de `c4` necesita al menos un elemento",
                Some(
                    "agregá el primero, por ejemplo `{ \"id\": \"cliente\", \"type\": \"person\", \"label\": \"Cliente\" }`"
                        .to_string(),
                ),
            ));
        }

        let mut ids: Vec<&str> = Vec::new();
        for (index, element) in input.elements.iter().enumerate() {
            if element.id.trim().is_empty() {
                errors.push(ValidationError::new(
                    format!("elements[{index}].id"),
                    ErrorCode::InvalidValue,
                    "el id del elemento no puede estar vacío",
                    None,
                ));
            }
            if ids.contains(&element.id.as_str()) {
                errors.push(ValidationError::new(
                    format!("elements[{index}].id"),
                    ErrorCode::DuplicateId,
                    format!("id de elemento repetido: `{}`", element.id),
                    Some("los ids de elemento deben ser únicos dentro del diagrama".to_string()),
                ));
            }
            ids.push(element.id.as_str());

            if element.label.trim().is_empty() {
                errors.push(ValidationError::new(
                    format!("elements[{index}].label"),
                    ErrorCode::InvalidValue,
                    "la etiqueta del elemento no puede estar vacía",
                    Some("la etiqueta es el nombre que el dibujo muestra en la caja".to_string()),
                ));
            }

            if !TYPES.contains(&element.r#type.as_str()) {
                errors.push(ValidationError::new(
                    format!("elements[{index}].type"),
                    ErrorCode::InvalidEnum,
                    format!("tipo desconocido en `c4`: `{}`", element.r#type),
                    Some(format!("los tipos son {}", TYPES.join(", "))),
                ));
            } else if known_level && !admits.contains(&element.r#type.as_str()) {
                errors.push(ValidationError::new(
                    format!("elements[{index}].type"),
                    ErrorCode::InvalidValue,
                    format!(
                        "el nivel `{}` no admite el tipo `{}`",
                        input.level, element.r#type
                    ),
                    Some(format!(
                        "en el nivel `{}` los tipos legales son {}",
                        input.level,
                        quoted(admits)
                    )),
                ));
            }
        }

        for (index, relation) in input.relations.iter().enumerate() {
            if !ids.contains(&relation.from.as_str()) {
                errors.push(ValidationError::new(
                    format!("relations[{index}].from"),
                    ErrorCode::UnknownRef,
                    format!("elemento de origen desconocido: `{}`", relation.from),
                    Some(
                        "las relaciones referencian elementos declarados en `elements`".to_string(),
                    ),
                ));
            }
            if !ids.contains(&relation.to.as_str()) {
                errors.push(ValidationError::new(
                    format!("relations[{index}].to"),
                    ErrorCode::UnknownRef,
                    format!("elemento de destino desconocido: `{}`", relation.to),
                    Some(
                        "las relaciones referencian elementos declarados en `elements`".to_string(),
                    ),
                ));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn to_ir(&self, input: &Self::Input) -> Ir {
        let nodes = input
            .elements
            .iter()
            .map(|element| {
                let mut node = IrNode::new(element.id.clone(), element.label.clone());
                node.shape = Shape::Box;
                // El tipo abre el compartimento de detalle y la tecnología va
                // en la nota, a la derecha: la forma de las filas de `er`,
                // donde la fila dice el hecho y la nota el calificador. La caja
                // muestra así el tipo y la etiqueta (S4).
                let mut typed = IrRow::new(element.r#type.clone());
                typed.note = filled(&element.technology);
                node.rows.push(typed);
                if let Some(description) = filled(&element.description) {
                    node.rows.push(IrRow::new(description));
                }
                node
            })
            .collect();

        let edges = input
            .relations
            .iter()
            .map(|relation| {
                let mut edge = IrEdge::new(relation.from.clone(), relation.to.clone());
                edge.label = filled(&relation.label);
                edge
            })
            .collect();

        Ir::Graph(IrGraph {
            // El contrato no lleva `direction` (S2): la lectura la fija el
            // kind. Un contexto se lee de izquierda a derecha, quien pide a la
            // izquierda y el sistema a la derecha.
            direction: Direction::LR,
            layout_hint: Self::LAYOUT_HINT,
            nodes,
            edges,
            groups: Vec::new(),
        })
    }
}

/// El texto del campo, o nada cuando está vacío: un campo opcional vacío no
/// dibuja una fila vacía.
fn filled(value: &Option<String>) -> Option<String> {
    value
        .as_ref()
        .filter(|text| !text.trim().is_empty())
        .cloned()
}

/// Una lista de nombres entre comillas invertidas, para el hint del error.
fn quoted(names: &[&str]) -> String {
    let quoted: Vec<String> = names.iter().map(|name| format!("`{name}`")).collect();
    match quoted.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} y {last}", rest.join(", ")),
        _ => quoted.join(""),
    }
}

/// Registers the `c4` kind in the global registry.
pub fn register() {
    gx_core::kind::register_kind::<C4Kind>();
}
