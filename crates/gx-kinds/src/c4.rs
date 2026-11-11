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
use gx_core::ir::{Direction, Ir, IrEdge, IrNode, IrRow, LayoutHint, Shape};
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

/// La línea que ata cada nivel a los tipos que admite (`f20-reuso.md`, S11):
/// se escribe una vez, desde [`types_of`], así que un nivel o un tipo nuevo
/// entra en la tabla y el texto del agente lo sigue sin otra copia.
pub fn level_types_rule() -> String {
    let levels: Vec<String> = LEVELS
        .iter()
        .map(|level| {
            let types: Vec<String> = types_of(level)
                .iter()
                .map(|name| format!("`{name}`"))
                .collect();
            let types = match types.split_last() {
                Some((last, rest)) if !rest.is_empty() => {
                    format!("{} y {last}", rest.join(", "))
                }
                _ => types.join(""),
            };
            format!("`{level}` → {types}")
        })
        .collect();
    format!(
        "El `level` decide qué `type` admite cada elemento: {}.",
        levels.join("; ")
    )
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
    /// Contract version. Only `1` is supported; absent means `1`.
    #[serde(default = "crate::flow::default_version")]
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
    const PHASE: &'static str = "F3";
    const LAYOUT_HINT: LayoutHint = LayoutHint::Layered;
    const LIMITS: KindLimits = KindLimits::GRAPH;
    const KIND_VERSION: u32 = SUPPORTED_VERSION;

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
                    Some("escribí un id único, por ejemplo `api`".to_string()),
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

        Ir::Graph(crate::flow::ir_graph(
            Some(Direction::LR),
            Self::LAYOUT_HINT,
            nodes,
            edges,
            Vec::new(),
        ))
    }

    /// El nivel manda sobre el vocabulario y v1 no anida: lo que el agente
    /// tiene que saber antes de escribir un `c4`.
    fn rules(&self) -> Vec<String> {
        vec![
            level_types_rule(),
            "Cada elemento tiene un `id` único y una `label`; `description` y `technology` son opcionales."
                .to_string(),
            "Cada relación referencia con `from` y `to` elementos declarados en `elements`, y su `label` es el verbo (`usa`, `lee`, `cobra`)."
                .to_string(),
            "No hay anidamiento en v1: la lectura sale de los tipos, del `level` y de las relaciones."
                .to_string(),
        ]
    }

    fn min_example() -> serde_json::Value {
        serde_json::json!({
            "version": 1,
            "kind": "c4",
            "title": "Pedidos — contexto",
            "level": "context",
            "elements": [
                { "id": "cliente", "type": "person", "label": "Cliente",
                  "description": "hace el pedido" },
                { "id": "operador", "type": "person", "label": "Operador" },
                { "id": "pedidos", "type": "system", "label": "Sistema de pedidos",
                  "technology": "Rust" }
            ],
            "relations": [
                { "from": "cliente", "to": "pedidos", "label": "hace un pedido" },
                { "from": "operador", "to": "pedidos", "label": "despacha" }
            ]
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

#[cfg(test)]
mod tests {
    use super::{level_types_rule, types_of, LEVELS};

    /// S11 (`f20-reuso.md`): la frase level→types se escribe una vez, desde
    /// `types_of`, así que cada nivel enseña sus tipos y un nivel nuevo entra
    /// en la tabla y en el texto sin una segunda copia.
    #[test]
    fn the_level_types_rule_is_spelled_from_the_table() {
        let rule = level_types_rule();
        assert!(
            rule.contains("`context` → `person` y `system`"),
            "la frase no es la del contrato: {rule}"
        );
        for level in LEVELS {
            assert!(rule.contains(&format!("`{level}`")), "{level}: {rule}");
            for name in types_of(level) {
                assert!(
                    rule.contains(&format!("`{name}`")),
                    "{level}/{name}: {rule}"
                );
            }
        }
    }
}
