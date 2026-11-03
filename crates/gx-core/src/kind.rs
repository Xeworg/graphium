//! The kind trait and the registry (PRD §8.1-8.3, §9.1).
//!
//! `gx-core` does not depend on `gx-kinds`: kinds register themselves in the
//! global registry, so there is no cycle between the registry and the IR.
//!
//! [`DiagramKind`] is the typed contract of a kind (PRD §8.1-8.3). The
//! registry stores [`KindHandler`], the object safe bridge that lets a caller
//! resolve a kind by name without knowing its Rust type.

use std::collections::BTreeMap;
use std::sync::{OnceLock, RwLock};

use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::{ErrorCode, ValidationError};
use crate::ir::{Ir, LayoutHint};

/// Limits declared by a kind (PRD §7.3). They live in one place: the kind
/// module, never repeated in another crate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KindLimits {
    pub max_nodes: usize,
    pub max_edges: usize,
    /// Maximum edges per node.
    pub max_density: f64,
    pub max_lanes: usize,
    pub max_events: usize,
}

impl Default for KindLimits {
    fn default() -> Self {
        Self {
            max_nodes: 300,
            max_edges: 600,
            max_density: 3.0,
            max_lanes: 40,
            max_events: 200,
        }
    }
}

/// A diagram kind: its JSON contract, its semantic rules and its translation
/// to the IR.
pub trait DiagramKind: Send + Sync + Default {
    const KIND: &'static str;
    const LAYOUT_HINT: LayoutHint;
    const LIMITS: KindLimits;

    /// The JSON contract. `deny_unknown_fields` is part of the schema, so the
    /// contract the agent reads is the contract that is validated.
    type Input: JsonSchema + DeserializeOwned;

    /// Cross-field rules that JSON Schema cannot express (PRD §9.1).
    fn validate(&self, input: &Self::Input) -> Result<(), Vec<ValidationError>>;

    /// Translation to the IR.
    fn to_ir(&self, input: &Self::Input) -> Ir;

    /// The JSON schema of the kind.
    fn schema() -> schemars::Schema;
}

/// Object safe view of a kind, used by the registry.
pub trait KindHandler: Send + Sync {
    fn kind(&self) -> &'static str;
    fn layout_hint(&self) -> LayoutHint;
    fn limits(&self) -> KindLimits;
    fn schema(&self) -> schemars::Schema;

    /// Validates a document against the kind contract.
    fn validate_json(&self, document: &Value) -> Result<(), Vec<ValidationError>>;

    /// Validates and translates a document into the IR.
    fn process_json(&self, document: &Value) -> Result<Ir, Vec<ValidationError>>;
}

/// Adapts a typed [`DiagramKind`] to the object safe [`KindHandler`].
pub struct KindAdapter<K: DiagramKind> {
    kind: K,
}

impl<K: DiagramKind> KindAdapter<K> {
    pub fn new(kind: K) -> Self {
        Self { kind }
    }
}

impl<K: DiagramKind> KindHandler for KindAdapter<K> {
    fn kind(&self) -> &'static str {
        K::KIND
    }

    fn layout_hint(&self) -> LayoutHint {
        K::LAYOUT_HINT
    }

    fn limits(&self) -> KindLimits {
        K::LIMITS
    }

    fn schema(&self) -> schemars::Schema {
        K::schema()
    }

    fn validate_json(&self, document: &Value) -> Result<(), Vec<ValidationError>> {
        let input = parse_input::<K>(document)?;
        self.kind.validate(&input)
    }

    fn process_json(&self, document: &Value) -> Result<Ir, Vec<ValidationError>> {
        let input = parse_input::<K>(document)?;
        self.kind.validate(&input)?;
        Ok(self.kind.to_ir(&input))
    }
}

/// Parses a document into the typed input of a kind, mapping the failure to
/// a stable error code.
fn parse_input<K: DiagramKind>(document: &Value) -> Result<K::Input, Vec<ValidationError>> {
    serde_json::from_value(document.clone()).map_err(|error| {
        let message = error.to_string();
        let code = if message.contains("unknown field") {
            ErrorCode::UnknownField
        } else if message.contains("missing field") {
            ErrorCode::MissingField
        } else {
            ErrorCode::InvalidSchema
        };
        vec![ValidationError::new(
            "",
            code,
            format!("el JSON no cumple el contrato de `{}`: {message}", K::KIND),
            Some("consultá get_diagram_schema para ver los campos admitidos".to_string()),
        )]
    })
}

type KindMap = BTreeMap<&'static str, &'static dyn KindHandler>;

fn registry() -> &'static RwLock<KindMap> {
    static REGISTRY: OnceLock<RwLock<KindMap>> = OnceLock::new();
    REGISTRY.get_or_init(|| RwLock::new(BTreeMap::new()))
}

/// Registers a kind. Calling it twice with the same name is a no-op.
pub fn register(handler: &'static dyn KindHandler) {
    let mut map = registry().write().expect("registry is not poisoned");
    map.entry(handler.kind()).or_insert(handler);
}

/// Registers a typed kind, adapting it to the object safe handler.
pub fn register_kind<K: DiagramKind + 'static>() {
    let adapter: &'static dyn KindHandler = Box::leak(Box::new(KindAdapter::new(K::default())));
    register(adapter);
}

/// Looks a kind up by name (the only way `gx-mcp` reaches a kind, PRD §9.1).
pub fn for_kind(name: &str) -> Option<&'static dyn KindHandler> {
    let map = registry().read().expect("registry is not poisoned");
    map.get(name).copied()
}

/// Every registered kind, ordered by name.
pub fn handlers() -> Vec<&'static dyn KindHandler> {
    let map = registry().read().expect("registry is not poisoned");
    map.values().copied().collect()
}

/// The names of every registered kind.
pub fn kind_names() -> Vec<&'static str> {
    let map = registry().read().expect("registry is not poisoned");
    map.keys().copied().collect()
}

#[cfg(test)]
mod tests {
    use super::{for_kind, kind_names, register_kind, DiagramKind, KindLimits};
    use crate::error::ValidationError;
    use crate::ir::{Direction, Ir, IrGraph, IrNode, LayoutHint};
    use schemars::JsonSchema;
    use serde::Deserialize;

    #[derive(Debug, Default, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    struct DemoInput {
        title: String,
    }

    #[derive(Debug, Default)]
    struct DemoKind;

    impl DiagramKind for DemoKind {
        const KIND: &'static str = "demo";
        const LAYOUT_HINT: LayoutHint = LayoutHint::Layered;
        const LIMITS: KindLimits = KindLimits {
            max_nodes: 10,
            max_edges: 10,
            max_density: 3.0,
            max_lanes: 1,
            max_events: 1,
        };
        type Input = DemoInput;

        fn validate(&self, _input: &Self::Input) -> Result<(), Vec<ValidationError>> {
            Ok(())
        }

        fn to_ir(&self, input: &Self::Input) -> Ir {
            Ir::Graph(IrGraph {
                direction: Direction::TB,
                layout_hint: LayoutHint::Layered,
                nodes: vec![IrNode::new("a", input.title.clone())],
                edges: Vec::new(),
                groups: Vec::new(),
            })
        }

        fn schema() -> schemars::Schema {
            schemars::schema_for!(DemoInput)
        }
    }

    #[test]
    fn registers_and_looks_up_kinds() {
        register_kind::<DemoKind>();
        assert!(kind_names().contains(&"demo"));
        assert!(for_kind("demo").is_some());
        assert!(for_kind("nope").is_none());
    }

    #[test]
    fn unknown_field_is_reported_with_its_own_code() {
        register_kind::<DemoKind>();
        let handler = for_kind("demo").expect("registered");
        let document = serde_json::json!({ "title": "x", "extra": 1 });
        let errors = handler.validate_json(&document).expect_err("must fail");
        assert_eq!(errors[0].code, crate::error::ErrorCode::UnknownField);
    }
}
