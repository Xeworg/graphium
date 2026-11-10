//! The `er` kind: a database schema as tables with their columns (PRD §6.1).
//!
//! A table is a node whose rows are its columns, and a relation is an edge
//! that anchors to the foreign key column. The cardinality is the crow's
//! foot: each end says whether it is one or many, required or optional, so
//! the diagram answers the question a schema diagram exists for.

use gx_core::error::{ErrorCode, ValidationError};
use gx_core::ir::{Cardinality, Direction, Ir, IrEdge, IrNode, IrRow, LayoutHint, RowKey, Shape};
use gx_core::kind::{DiagramKind, KindLimits};
use schemars::JsonSchema;
use serde::Deserialize;

/// Contract version supported by this build (PRD §10.1).
pub const SUPPORTED_VERSION: u32 = 1;

/// A column of a table.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ErColumn {
    /// Name of the column.
    pub name: String,
    /// Database type, free text: `uuid`, `text`, `int4`, `timestamptz`.
    #[serde(default)]
    pub r#type: Option<String>,
    /// Role of the column in the keys: `pk`, `fk`, `pk_fk`.
    #[serde(default)]
    pub key: Option<String>,
    /// Whether the column admits `NULL`. A nullable foreign key makes its
    /// relation optional on that side.
    #[serde(default)]
    pub nullable: Option<bool>,
    /// Free note shown after the type.
    #[serde(default)]
    pub note: Option<String>,
}

/// A table of the schema.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ErTable {
    /// Name of the table; unique inside the diagram.
    pub name: String,
    /// Optional human label; defaults to the name.
    #[serde(default)]
    pub label: Option<String>,
    /// The columns, in the order they are drawn.
    #[serde(default)]
    pub columns: Vec<ErColumn>,
}

/// One side of a relation.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ErSide {
    /// Name of the table.
    pub table: String,
    /// Name of the column.
    pub column: String,
}

/// A relation between two columns.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ErRelation {
    /// The referenced side: usually the primary key.
    pub from: ErSide,
    /// The referencing side: usually the foreign key.
    pub to: ErSide,
    /// One of `1:1`, `1:N`, `N:1`, `N:M`.
    #[serde(default)]
    pub cardinality: Option<String>,
}

/// The JSON contract of the `er` kind.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ErInput {
    /// Contract version. Only `1` is supported.
    pub version: u32,
    /// Must be `er`.
    pub kind: String,
    /// Human readable title of the diagram.
    #[serde(default)]
    pub title: Option<String>,
    /// Direction of the layered layout.
    #[serde(default)]
    pub direction: Option<Direction>,
    #[serde(default)]
    pub tables: Vec<ErTable>,
    #[serde(default)]
    pub relations: Vec<ErRelation>,
}

/// The `er` kind.
#[derive(Debug, Default)]
pub struct ErKind;

fn key_of(column: &ErColumn) -> RowKey {
    match column
        .key
        .as_deref()
        .map(str::trim)
        .map(str::to_ascii_lowercase)
    {
        Some(value) if value == "pk" => RowKey::Pk,
        Some(value) if value == "fk" => RowKey::Fk,
        Some(value) if value == "pk_fk" || value == "pkfk" => RowKey::PkFk,
        _ => RowKey::None,
    }
}

impl DiagramKind for ErKind {
    const KIND: &'static str = "er";
    const PHASE: &'static str = "F1";
    const LAYOUT_HINT: LayoutHint = LayoutHint::Layered;
    const LIMITS: KindLimits = KindLimits::GRAPH;

    type Input = ErInput;

    fn validate(&self, input: &Self::Input) -> Result<(), Vec<ValidationError>> {
        let mut errors =
            crate::flow::validate_header(Self::KIND, &input.kind, SUPPORTED_VERSION, input.version);

        // Names of the tables, in order, for the references and uniqueness.
        let mut tables: Vec<&str> = Vec::new();
        for (index, table) in input.tables.iter().enumerate() {
            if table.name.is_empty() {
                errors.push(ValidationError::new(
                    format!("tables[{index}].name"),
                    ErrorCode::InvalidValue,
                    "el nombre de la tabla no puede estar vacío",
                    None,
                ));
            }
            if tables.contains(&table.name.as_str()) {
                errors.push(ValidationError::new(
                    format!("tables[{index}].name"),
                    ErrorCode::DuplicateId,
                    format!("tabla repetida: `{}`", table.name),
                    Some("los nombres de tabla deben ser únicos dentro del diagrama".to_string()),
                ));
            }
            tables.push(table.name.as_str());

            let mut columns: Vec<&str> = Vec::new();
            for (column_index, column) in table.columns.iter().enumerate() {
                if column.name.is_empty() {
                    errors.push(ValidationError::new(
                        format!("tables[{index}].columns[{column_index}].name"),
                        ErrorCode::InvalidValue,
                        "el nombre de la columna no puede estar vacío",
                        Some("poné el nombre que la fila muestra, por ejemplo `total`".to_string()),
                    ));
                }
                if columns.contains(&column.name.as_str()) {
                    errors.push(ValidationError::new(
                        format!("tables[{index}].columns[{column_index}].name"),
                        ErrorCode::DuplicateId,
                        format!("columna repetida en `{}`: `{}`", table.name, column.name),
                        Some("los nombres de columna son únicos dentro de la tabla".to_string()),
                    ));
                }
                columns.push(column.name.as_str());

                if let Some(key) = column.key.as_deref() {
                    let normal = key.trim().to_ascii_lowercase();
                    if !matches!(normal.as_str(), "pk" | "fk" | "pk_fk" | "pkfk") {
                        errors.push(ValidationError::new(
                            format!("tables[{index}].columns[{column_index}].key"),
                            ErrorCode::InvalidEnum,
                            format!("`key` inválida: `{key}`"),
                            Some("los valores válidos son `pk`, `fk` y `pk_fk`".to_string()),
                        ));
                    }
                }
            }
        }

        for (index, relation) in input.relations.iter().enumerate() {
            let from = find_column(input, &relation.from.table, &relation.from.column);
            match from {
                None => errors.push(ValidationError::new(
                    format!("relations[{index}].from"),
                    ErrorCode::UnknownRef,
                    format!(
                        "la tabla o columna `{}`.`{}` no existe",
                        relation.from.table, relation.from.column
                    ),
                    Some(did_you_mean(&tables, &relation.from.table)),
                )),
                Some(column) => {
                    if !matches!(key_of(column), RowKey::Pk | RowKey::PkFk) {
                        errors.push(ValidationError::new(
                            format!("relations[{index}].from.column"),
                            ErrorCode::InvalidValue,
                            format!(
                                "`{}`.`{}` no es clave primaria",
                                relation.from.table, relation.from.column
                            ),
                            Some(
                                "el lado `from` de una relación es la clave referenciada"
                                    .to_string(),
                            ),
                        ));
                    }
                }
            }

            match find_column(input, &relation.to.table, &relation.to.column) {
                None => errors.push(ValidationError::new(
                    format!("relations[{index}].to"),
                    ErrorCode::UnknownRef,
                    format!(
                        "la tabla o columna `{}`.`{}` no existe",
                        relation.to.table, relation.to.column
                    ),
                    Some(did_you_mean(&tables, &relation.to.table)),
                )),
                Some(column) => {
                    if !matches!(key_of(column), RowKey::Fk | RowKey::PkFk) {
                        errors.push(ValidationError::new(
                            format!("relations[{index}].to.column"),
                            ErrorCode::InvalidValue,
                            format!(
                                "`{}`.`{}` no es clave foránea",
                                relation.to.table, relation.to.column
                            ),
                            Some("marcá la columna con `key: \"fk\"`".to_string()),
                        ));
                    }
                }
            }

            if let Some(short) = relation.cardinality.as_deref() {
                if Cardinality::from_short(short, false).is_none() {
                    errors.push(ValidationError::new(
                        format!("relations[{index}].cardinality"),
                        ErrorCode::InvalidEnum,
                        format!("cardinalidad inválida: `{short}`"),
                        Some(format!(
                            "los valores válidos son {}",
                            Cardinality::SHORT_FORMS.join(", ")
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
        let nodes = input
            .tables
            .iter()
            .map(|table| {
                let mut node = IrNode::new(
                    table.name.clone(),
                    table.label.clone().unwrap_or_else(|| table.name.clone()),
                );
                node.shape = Shape::Box;
                for column in &table.columns {
                    let mut row = IrRow::new(&column.name);
                    row.key = key_of(column);
                    row.nullable = column.nullable.unwrap_or(false);
                    row.note = match (&column.r#type, &column.note) {
                        (Some(kind), Some(note)) => Some(format!("{kind} {note}")),
                        (Some(kind), None) => Some(kind.clone()),
                        (None, Some(note)) => Some(note.clone()),
                        (None, None) => None,
                    };
                    node.rows.push(row);
                }
                node
            })
            .collect();

        let edges = input
            .relations
            .iter()
            .map(|relation| {
                let mut edge = IrEdge::new(relation.from.table.clone(), relation.to.table.clone());
                // The crow's foot is the notation of a relation: an arrowhead
                // on top of it would only add noise.
                edge.arrow = false;
                // The edge anchors to the foreign key row, which is where a
                // schema reader looks for the relation.
                edge.to_row = row_index(input, &relation.to.table, &relation.to.column);
                edge.from_row = row_index(input, &relation.from.table, &relation.from.column);
                let nullable = find_column(input, &relation.to.table, &relation.to.column)
                    .map(|column| column.nullable.unwrap_or(false))
                    .unwrap_or(false);
                edge.cardinality = Some(
                    relation
                        .cardinality
                        .as_deref()
                        .and_then(|short| Cardinality::from_short(short, nullable))
                        .unwrap_or(Cardinality {
                            from_min: gx_core::ir::CardinalityBound::Required,
                            from_max: gx_core::ir::CardinalityMax::One,
                            to_min: if nullable {
                                gx_core::ir::CardinalityBound::Optional
                            } else {
                                gx_core::ir::CardinalityBound::Required
                            },
                            to_max: gx_core::ir::CardinalityMax::Many,
                        }),
                );
                // The crow's foot already says the cardinality: a label on
                // top of it repeats the same data and adds noise.
                edge.label = None;
                edge
            })
            .collect();

        Ir::Graph(crate::flow::ir_graph(
            // El contrato de `er` defaultea LR: el esquema se lee de izquierda
            // a derecha. `ir_graph` defaultea TB (el default de la familia
            // graph), así que el kind fija el suyo acá.
            Some(input.direction.unwrap_or(Direction::LR)),
            Self::LAYOUT_HINT,
            nodes,
            edges,
            Vec::new(),
        ))
    }

    fn schema() -> schemars::Schema {
        schemars::schema_for!(ErInput)
    }

    fn shapes() -> Vec<Shape> {
        vec![Shape::Box]
    }

    fn rules(&self) -> Vec<String> {
        vec![
            "Cada tabla tiene un `name` único dentro del diagrama.".to_string(),
            "Cada relación referencia tablas y columnas declaradas.".to_string(),
        ]
    }

    fn min_example() -> serde_json::Value {
        serde_json::json!({
            "version": 1,
            "kind": "er",
            "title": "Schema — posts",
            "tables": [
                { "name": "users", "columns": [ { "name": "id", "type": "uuid", "key": "pk" } ] },
                { "name": "posts", "columns": [
                    { "name": "id", "type": "uuid", "key": "pk" },
                    { "name": "user_id", "type": "uuid", "key": "fk" }
                ] }
            ],
            "relations": [
                {
                    "from": { "table": "users", "column": "id" },
                    "to": { "table": "posts", "column": "user_id" },
                    "cardinality": "1:N"
                }
            ]
        })
    }
}

/// The column of a table, if both exist.
fn find_column<'a>(input: &'a ErInput, table: &str, column: &str) -> Option<&'a ErColumn> {
    input
        .tables
        .iter()
        .find(|candidate| candidate.name == table)?
        .columns
        .iter()
        .find(|candidate| candidate.name == column)
}

/// The index of a column inside its table.
fn row_index(input: &ErInput, table: &str, column: &str) -> Option<u32> {
    let found = input
        .tables
        .iter()
        .find(|candidate| candidate.name == table)?;
    let index = found
        .columns
        .iter()
        .position(|candidate| candidate.name == column)?;
    Some(index as u32)
}

/// A did-you-mean hint over the tables of the document.
fn did_you_mean(tables: &[&str], name: &str) -> String {
    let closest = tables
        .iter()
        .map(|candidate| (distance(name, candidate), *candidate))
        .min_by_key(|(distance, _)| *distance);
    match closest {
        Some((_, candidate)) if candidate != name => format!("did you mean `{candidate}`?"),
        _ => "declarala en `tables`".to_string(),
    }
}

/// The edit distance between two words.
fn distance(left: &str, right: &str) -> usize {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0usize; right.len() + 1];
    for (i, left_char) in left.iter().enumerate() {
        current[0] = i + 1;
        for (j, right_char) in right.iter().enumerate() {
            let cost = usize::from(left_char != right_char);
            current[j + 1] = (previous[j] + cost)
                .min(previous[j + 1] + 1)
                .min(current[j] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

/// Registers the `er` kind in the global registry.
pub fn register() {
    gx_core::kind::register_kind::<ErKind>();
}

#[cfg(test)]
mod tests {
    use super::{ErInput, ErKind};
    use gx_core::kind::DiagramKind;

    const EXAMPLE: &str = r#"{
      "version": 1,
      "kind": "er",
      "title": "Schema — posts",
      "tables": [
        { "name": "users", "columns": [ { "name": "id", "type": "uuid", "key": "pk" } ] },
        { "name": "posts", "columns": [
            { "name": "id", "type": "uuid", "key": "pk" },
            { "name": "user_id", "type": "uuid", "key": "fk" }
        ] }
      ],
      "relations": [
        {
          "from": { "table": "users", "column": "id" },
          "to": { "table": "posts", "column": "user_id" },
          "cardinality": "1:N"
        }
      ]
    }"#;

    #[test]
    fn the_minimal_example_of_the_prd_validates() {
        let input: ErInput = serde_json::from_str(EXAMPLE).expect("valid");
        assert!(ErKind.validate(&input).is_ok());
    }

    #[test]
    fn a_table_becomes_a_node_with_one_row_per_column() {
        let input: ErInput = serde_json::from_str(EXAMPLE).expect("valid");
        let ir = ErKind.to_ir(&input);
        let graph = ir.graph().expect("graph");
        assert_eq!(graph.nodes.len(), 2);
        let posts = graph
            .nodes
            .iter()
            .find(|node| node.id == "posts")
            .expect("posts");
        assert_eq!(posts.rows.len(), 2);
        assert_eq!(posts.rows[1].text, "user_id");
        assert_eq!(posts.rows[1].note.as_deref(), Some("uuid"));
        assert_eq!(posts.rows[1].key, gx_core::ir::RowKey::Fk);
    }

    #[test]
    fn a_relation_becomes_a_crow_foot_edge() {
        let input: ErInput = serde_json::from_str(EXAMPLE).expect("valid");
        let ir = ErKind.to_ir(&input);
        let edge = &ir.graph().expect("graph").edges[0];
        assert_eq!(edge.from, "users");
        assert_eq!(edge.to, "posts");
        let cardinality = edge.cardinality.expect("cardinality");
        assert_eq!(cardinality.from_max, gx_core::ir::CardinalityMax::One);
        assert_eq!(cardinality.to_max, gx_core::ir::CardinalityMax::Many);
    }

    #[test]
    fn a_nullable_foreign_key_makes_the_relation_optional() {
        let document = EXAMPLE.replace(
            r#"{ "name": "user_id", "type": "uuid", "key": "fk" }"#,
            r#"{ "name": "user_id", "type": "uuid", "key": "fk", "nullable": true }"#,
        );
        let input: ErInput = serde_json::from_str(&document).expect("valid");
        let ir = ErKind.to_ir(&input);
        let edge = &ir.graph().expect("graph").edges[0];
        assert_eq!(
            edge.cardinality.expect("cardinality").to_min,
            gx_core::ir::CardinalityBound::Optional
        );
    }

    #[test]
    fn a_duplicated_table_is_rejected() {
        let document = serde_json::json!({
            "version": 1, "kind": "er",
            "tables": [ { "name": "t" }, { "name": "t" } ]
        });
        let input: ErInput = serde_json::from_value(document).expect("valid");
        let errors = ErKind.validate(&input).expect_err("must fail");
        assert_eq!(errors[0].code, gx_core::error::ErrorCode::DuplicateId);
        assert_eq!(errors[0].path, "tables[1].name");
    }

    #[test]
    fn an_unknown_table_is_rejected_with_a_hint() {
        let document = serde_json::json!({
            "version": 1, "kind": "er",
            "tables": [ { "name": "users", "columns": [ { "name": "id", "key": "pk" } ] } ],
            "relations": [ {
                "from": { "table": "users", "column": "id" },
                "to": { "table": "user", "column": "id" }
            } ]
        });
        let input: ErInput = serde_json::from_value(document).expect("valid");
        let errors = ErKind.validate(&input).expect_err("must fail");
        assert_eq!(errors[0].code, gx_core::error::ErrorCode::UnknownRef);
        assert!(errors[0].hint.clone().unwrap().contains("users"));
    }

    #[test]
    fn a_bad_cardinality_is_rejected_with_the_valid_ones() {
        let document = EXAMPLE.replace(
            r#""cardinality": "1:N""#,
            r#""cardinality": "uno a muchos""#,
        );
        let input: ErInput = serde_json::from_str(&document).expect("valid");
        let errors = ErKind.validate(&input).expect_err("must fail");
        assert_eq!(errors[0].code, gx_core::error::ErrorCode::InvalidEnum);
        assert!(errors[0].hint.clone().unwrap().contains("1:N"));
    }

    #[test]
    fn a_relation_must_point_to_a_foreign_key() {
        let document = EXAMPLE.replace(
            r#""to": { "table": "posts", "column": "user_id" }"#,
            r#""to": { "table": "posts", "column": "id" }"#,
        );
        let input: ErInput = serde_json::from_str(&document).expect("valid");
        let errors = ErKind.validate(&input).expect_err("must fail");
        assert_eq!(errors[0].code, gx_core::error::ErrorCode::InvalidValue);
    }
}

#[cfg(test)]
mod direction_tests {
    use super::ErKind;
    use gx_core::ir::{Direction, Ir};
    use gx_core::kind::DiagramKind;

    /// El default de `er` es LR (el esquema se lee de izquierda a derecha),
    /// aunque `flow::ir_graph` defaultee TB: el kind fija el suyo.
    #[test]
    fn a_schema_without_direction_maps_to_lr() {
        let parsed: super::ErInput = serde_json::from_str(
            r#"{ "version": 1, "kind": "er",
                 "tables": [ { "name": "posts" } ] }"#,
        )
        .expect("the document parses");

        let Ir::Graph(graph) = ErKind.to_ir(&parsed) else {
            panic!("un diagrama de `er` es un grafo");
        };
        assert_eq!(graph.direction, Direction::LR);
    }
}
