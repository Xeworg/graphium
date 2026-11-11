//! Groups of nodes, shared by the kinds whose engine draws cluster boxes.
//!
//! PRD §7 RF15: groups nest, so a member is either a node or another group.
//! Every member lives in a single group and the parent chain never closes on
//! itself, which is what lets the layout turn the chain into compound nodes.

use gx_core::error::{ErrorCode, ValidationError};
use gx_core::ir::IrGroup;
use schemars::JsonSchema;
use serde::Deserialize;

/// A group: the nodes —or the other groups— that belong to it.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Group {
    /// Unique identifier of the group inside the diagram.
    pub id: String,
    /// Name the group shows. Defaults to the id.
    #[serde(default)]
    pub label: Option<String>,
    /// Ids of the nodes or of the other groups that belong to it.
    pub members: Vec<String>,
}

/// Validates the groups against the nodes of the diagram.
///
/// `node_ids` are the declared nodes: groups and nodes share one namespace
/// (the engine keeps both as nodes of the same graph), so a group never takes
/// the id of a node, and a member resolves to either of them.
pub fn validate_groups(node_ids: &[&str], groups: &[Group]) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let mut declared: Vec<&str> = node_ids.to_vec();

    for (index, group) in groups.iter().enumerate() {
        if group.id.trim().is_empty() {
            errors.push(ValidationError::new(
                format!("groups[{index}].id"),
                ErrorCode::InvalidValue,
                "el id del grupo no puede estar vacío",
                Some("escribí un id único para el grupo, por ejemplo `backend`".to_string()),
            ));
        }
        if declared.contains(&group.id.as_str()) {
            let hint = if node_ids.contains(&group.id.as_str()) {
                "un grupo no puede tomar el id de un nodo: el motor los guarda en el mismo espacio de nombres"
            } else {
                "los ids de grupo deben ser únicos dentro del diagrama"
            };
            errors.push(ValidationError::new(
                format!("groups[{index}].id"),
                ErrorCode::DuplicateId,
                format!("id de grupo repetido: `{}`", group.id),
                Some(hint.to_string()),
            ));
        }
        declared.push(group.id.as_str());
    }

    // Who owns each member: a node lives in one group and a group has a single
    // parent, otherwise the drawing cannot tell which nesting holds.
    let mut owner: Vec<(&str, &str)> = Vec::new();
    for (index, group) in groups.iter().enumerate() {
        if group.members.is_empty() {
            errors.push(ValidationError::new(
                format!("groups[{index}].members"),
                ErrorCode::MissingField,
                "un grupo necesita al menos un miembro",
                Some(
                    "agregá los `id` de los nodos o de los grupos que lo forman, o quitá el grupo"
                        .to_string(),
                ),
            ));
        }

        for (member_index, member) in group.members.iter().enumerate() {
            if !declared.contains(&member.as_str()) {
                errors.push(ValidationError::new(
                    format!("groups[{index}].members[{member_index}]"),
                    ErrorCode::UnknownRef,
                    format!("miembro desconocido: `{member}`"),
                    Some(
                        "los miembros referencian nodos declarados en `nodes` o grupos declarados en `groups`"
                            .to_string(),
                    ),
                ));
                continue;
            }
            match owner.iter().find(|(id, _)| id == member) {
                Some((_, previous)) => errors.push(ValidationError::new(
                    format!("groups[{index}].members[{member_index}]"),
                    ErrorCode::DuplicateId,
                    format!("`{member}` ya pertenece al grupo `{previous}`: un miembro vive en un solo grupo"),
                    Some(
                        "dejá el miembro en un grupo, o usá dos diagramas para las dos lecturas"
                            .to_string(),
                    ),
                )),
                None => owner.push((member.as_str(), group.id.as_str())),
            }
        }
    }

    // The parent chain never closes on itself: a group inside itself has no box
    // to draw (PRD §7 RF15). One error per cycle, on the member edge of the
    // group with the smallest id, so the same cycle is not reported once per
    // group that takes part in it.
    for (index, group) in groups.iter().enumerate() {
        let mut visited: Vec<&str> = vec![group.id.as_str()];
        let mut cursor = owner
            .iter()
            .find(|(id, _)| *id == group.id.as_str())
            .map(|(_, parent)| *parent);
        while let Some(current) = cursor {
            if current == group.id.as_str() {
                if visited.iter().all(|id| group.id.as_str() <= *id) {
                    let member_index = group
                        .members
                        .iter()
                        .position(|member| member == &group.id)
                        .unwrap_or(0);
                    errors.push(ValidationError::new(
                        format!("groups[{index}].members[{member_index}]"),
                        ErrorCode::InvalidValue,
                        format!("el grupo `{}` quedaría dentro de sí mismo", group.id),
                        Some(
                            "sacá el miembro que cierra el ciclo: un grupo no puede contenerse a sí mismo"
                                .to_string(),
                        ),
                    ));
                }
                break;
            }
            if visited.len() > groups.len() {
                break;
            }
            visited.push(current);
            cursor = owner
                .iter()
                .find(|(id, _)| *id == current)
                .map(|(_, parent)| *parent);
        }
    }

    errors
}

/// Translates the groups to the IR. The label falls back to the id, so every
/// group box carries a name.
pub fn to_ir_groups(groups: &[Group]) -> Vec<IrGroup> {
    groups
        .iter()
        .map(|group| IrGroup {
            id: group.id.clone(),
            label: group
                .label
                .clone()
                .filter(|label| !label.trim().is_empty())
                .or_else(|| Some(group.id.clone())),
            members: group.members.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{to_ir_groups, validate_groups, Group};
    use gx_core::error::ErrorCode;

    fn group(id: &str, members: &[&str]) -> Group {
        Group {
            id: id.to_string(),
            label: None,
            members: members.iter().map(|member| member.to_string()).collect(),
        }
    }

    /// A member resolves to a node or to another group, and the nesting reaches
    /// the IR unchanged (PRD §7 RF15).
    #[test]
    fn nested_groups_are_accepted() {
        let groups = [group("sys", &["backend"]), group("backend", &["api"])];

        let errors = validate_groups(&["api"], &groups);
        assert!(errors.is_empty(), "{errors:?}");

        let ir = to_ir_groups(&groups);
        assert_eq!(ir[0].members, vec!["backend"]);
        assert_eq!(ir[0].label.as_deref(), Some("sys"));
        assert_eq!(ir[1].members, vec!["api"]);
    }

    #[test]
    fn a_blank_label_falls_back_to_the_id() {
        let groups = [Group {
            id: "backend".to_string(),
            label: Some("   ".to_string()),
            members: vec!["api".to_string()],
        }];

        let ir = to_ir_groups(&groups);
        assert_eq!(ir[0].label.as_deref(), Some("backend"));
    }

    #[test]
    fn a_group_cannot_take_the_id_of_a_node() {
        let errors = validate_groups(&["api"], &[group("api", &["api"])]);

        assert!(
            errors
                .iter()
                .any(|error| error.code == ErrorCode::DuplicateId && error.path == "groups[0].id"),
            "{errors:?}"
        );
    }

    #[test]
    fn an_unknown_member_is_reported() {
        let errors = validate_groups(&["api"], &[group("backend", &["fantasma"])]);

        assert!(
            errors
                .iter()
                .any(|error| error.code == ErrorCode::UnknownRef
                    && error.path == "groups[0].members[0]"),
            "{errors:?}"
        );
    }

    #[test]
    fn a_group_inside_itself_is_reported() {
        let errors = validate_groups(&["api"], &[group("sys", &["sys"])]);

        assert!(
            errors
                .iter()
                .any(|error| error.code == ErrorCode::InvalidValue
                    && error.path == "groups[0].members[0]"),
            "{errors:?}"
        );
    }

    #[test]
    fn a_cycle_between_two_groups_is_reported() {
        let errors = validate_groups(&["api"], &[group("a", &["b"]), group("b", &["a"])]);

        assert!(
            errors
                .iter()
                .any(|error| error.code == ErrorCode::InvalidValue),
            "{errors:?}"
        );
    }

    #[test]
    fn a_group_with_two_parents_is_reported() {
        let errors = validate_groups(
            &["api"],
            &[
                group("inner", &["api"]),
                group("left", &["inner"]),
                group("right", &["inner"]),
            ],
        );

        assert!(
            errors
                .iter()
                .any(|error| error.code == ErrorCode::DuplicateId
                    && error.path == "groups[2].members[0]"),
            "{errors:?}"
        );
    }

    #[test]
    fn an_empty_group_is_reported() {
        let errors = validate_groups(&["api"], &[group("backend", &[])]);

        assert!(
            errors
                .iter()
                .any(|error| error.code == ErrorCode::MissingField
                    && error.path == "groups[0].members"),
            "{errors:?}"
        );
    }
}
