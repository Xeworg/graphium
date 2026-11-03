//! Validation errors with stable codes (PRD §9.1, §11.1).

use serde::{Deserialize, Serialize};

/// Stable error codes. The string form is part of the MCP contract: agents
/// branch on it, so the enum variants never get renamed (PRD §11.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    // Contract and lifecycle codes listed in PRD §11.1.
    UnknownKind,
    UnknownDiagram,
    DiagramClosed,
    TooManyDiagrams,
    TooManySessions,
    NotOwner,
    AmbiguousDiagram,
    Busy,
    OwnerUnavailable,
    LayoutTimeout,
    ExportTimeout,
    ExportUnsupportedKind,
    ExportPathOutsideRoot,
    ExportExists,
    DiskQuotaExceeded,
    InvalidPatch,
    KindChangeNotAllowed,
    StaleRevision,
    DeprecatedKind,
    // Cross-field validation codes (PRD §9.1: `validate()` keeps the rules
    // that JSON Schema cannot express).
    InvalidJson,
    InvalidSchema,
    UnknownField,
    MissingField,
    DuplicateId,
    UnknownRef,
    InvalidValue,
    TooLarge,
    EngineUnavailable,
}

impl ErrorCode {
    /// The stable wire form of the code.
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorCode::UnknownKind => "unknown_kind",
            ErrorCode::UnknownDiagram => "unknown_diagram",
            ErrorCode::DiagramClosed => "diagram_closed",
            ErrorCode::TooManyDiagrams => "too_many_diagrams",
            ErrorCode::TooManySessions => "too_many_sessions",
            ErrorCode::NotOwner => "not_owner",
            ErrorCode::AmbiguousDiagram => "ambiguous_diagram",
            ErrorCode::Busy => "busy",
            ErrorCode::OwnerUnavailable => "owner_unavailable",
            ErrorCode::LayoutTimeout => "layout_timeout",
            ErrorCode::ExportTimeout => "export_timeout",
            ErrorCode::ExportUnsupportedKind => "export_unsupported_kind",
            ErrorCode::ExportPathOutsideRoot => "export_path_outside_root",
            ErrorCode::ExportExists => "export_exists",
            ErrorCode::DiskQuotaExceeded => "disk_quota_exceeded",
            ErrorCode::InvalidPatch => "invalid_patch",
            ErrorCode::KindChangeNotAllowed => "kind_change_not_allowed",
            ErrorCode::StaleRevision => "stale_revision",
            ErrorCode::DeprecatedKind => "deprecated_kind",
            ErrorCode::InvalidJson => "invalid_json",
            ErrorCode::InvalidSchema => "invalid_schema",
            ErrorCode::UnknownField => "unknown_field",
            ErrorCode::MissingField => "missing_field",
            ErrorCode::DuplicateId => "duplicate_id",
            ErrorCode::UnknownRef => "unknown_ref",
            ErrorCode::InvalidValue => "invalid_value",
            ErrorCode::TooLarge => "too_large",
            ErrorCode::EngineUnavailable => "engine_unavailable",
        }
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A single problem found while validating a diagram.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationError {
    /// JSON path of the offending value, e.g. `nodes[2].id`.
    pub path: String,
    pub code: ErrorCode,
    pub message: String,
    /// Optional actionable hint ("did you mean…").
    pub hint: Option<String>,
}

impl ValidationError {
    pub fn new(
        path: impl Into<String>,
        code: ErrorCode,
        message: impl Into<String>,
        hint: Option<String>,
    ) -> Self {
        Self {
            path: path.into(),
            code,
            message: message.into(),
            hint,
        }
    }

    pub fn without_hint(
        path: impl Into<String>,
        code: ErrorCode,
        message: impl Into<String>,
    ) -> Self {
        Self::new(path, code, message, None)
    }
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {} ({})", self.path, self.message, self.code)
    }
}

impl std::error::Error for ValidationError {}

#[cfg(test)]
mod tests {
    use super::{ErrorCode, ValidationError};

    #[test]
    fn codes_keep_their_wire_form() {
        assert_eq!(ErrorCode::UnknownKind.as_str(), "unknown_kind");
        assert_eq!(ErrorCode::TooManyDiagrams.as_str(), "too_many_diagrams");
        assert_eq!(
            ErrorCode::ExportPathOutsideRoot.as_str(),
            "export_path_outside_root"
        );
        assert_eq!(
            ErrorCode::KindChangeNotAllowed.as_str(),
            "kind_change_not_allowed"
        );
    }

    #[test]
    fn error_serializes_with_stable_code() {
        let error =
            ValidationError::new("nodes[0].id", ErrorCode::DuplicateId, "id repetido", None);
        let json = serde_json::to_string(&error).expect("serializable");
        assert!(json.contains("\"code\":\"duplicate_id\""), "{json}");
    }
}
