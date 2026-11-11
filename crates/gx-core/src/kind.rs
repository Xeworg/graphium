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
use crate::ir::{Ir, LayoutHint, Shape};

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

impl KindLimits {
    /// The layered-graph profile: 300 nodes, 600 edges, density 3.0 — the
    /// ceiling eleven kinds share (`f20-reuso.md`, S4). This is also the
    /// `Default`: the graph was the first kind.
    pub const GRAPH: KindLimits = KindLimits {
        max_nodes: 300,
        max_edges: 600,
        max_density: 3.0,
        max_lanes: 0,
        max_events: 0,
    };

    /// The lanes profile: 200 events on 40 lanes, no edges — timeline,
    /// swimlane and gantt (`f20-reuso.md`, S4).
    pub const TIMELINE: KindLimits = KindLimits {
        max_nodes: 200,
        max_edges: 0,
        max_density: 0.0,
        max_lanes: 40,
        max_events: 200,
    };

    /// The sequence profile: 40 participants and 200 messages — the PRD's
    /// «40 carriles × 200 eventos» (`f21-roto.md`, S6). Sequence is a graph,
    /// so its participants are nodes and its messages edges; the density
    /// ceiling is the PRD maximum (200/40 = 5).
    pub const SEQUENCE: KindLimits = KindLimits {
        max_nodes: 40,
        max_edges: 200,
        max_density: 5.0,
        max_lanes: 0,
        max_events: 0,
    };

    /// The tree profile: 500 nodes, 499 edges — tree and mindmap, where the
    /// hierarchy is the whole drawing (PRD §7.3). The density ceiling stays
    /// the graph family's: a tree only ever reaches ~1 edge per node.
    pub const TREE: KindLimits = KindLimits {
        max_nodes: 500,
        max_edges: 499,
        max_density: 3.0,
        max_lanes: 0,
        max_events: 0,
    };
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

/// A role a node may take, and the shape it is drawn with.
///
/// The vocabulary of a kind lives with the kind, but its shape is the IR's
/// (`f20-reuso.md`, S11): a role table is a list of these, the contract spells
/// the line that teaches it and the renderer draws it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Role {
    /// The name the document writes, e.g. `decision`.
    pub name: &'static str,
    /// The shape of [`Shape`] that role is drawn with.
    pub shape: Shape,
}

impl Role {
    pub const fn new(name: &'static str, shape: Shape) -> Self {
        Self { name, shape }
    }
}

/// A diagram kind: its JSON contract, its semantic rules and its translation
/// to the IR.
pub trait DiagramKind: Send + Sync + Default {
    const KIND: &'static str;
    /// The phase of the roadmap the kind ships in (PRD §6). It is a const
    /// beside `KIND`, not a lookup with a fallback (`f20-reuso.md`, S10): a
    /// kind that forgets its phase does not compile, and no build ever
    /// answers for another one.
    const PHASE: &'static str;
    const LAYOUT_HINT: LayoutHint;
    const LIMITS: KindLimits;
    /// The contract version the kind serves (`f23-contrato.md`, S2): beside
    /// the other consts, so a kind that forgets it does not compile.
    const KIND_VERSION: u32;

    /// The JSON contract. `deny_unknown_fields` is part of the schema, so the
    /// contract the agent reads is the contract that is validated.
    type Input: JsonSchema + DeserializeOwned;

    /// Cross-field rules that JSON Schema cannot express (PRD §9.1).
    fn validate(&self, input: &Self::Input) -> Result<(), Vec<ValidationError>>;

    /// Translation to the IR.
    fn to_ir(&self, input: &Self::Input) -> Ir;

    /// The JSON schema of the kind.
    fn schema() -> schemars::Schema;

    /// The shapes the kind draws, as the schema reports them (PRD §11.1,
    /// `f19-formas.md` S4): the truth of the kind, never a hand list. Role
    /// tables derive it; kinds without one draw their default.
    fn shapes() -> Vec<Shape> {
        vec![Shape::Rounded]
    }

    /// The roles the kind admits, in the order its rules teach them
    /// (`f20-reuso.md`, S11). Empty when the kind has no role vocabulary.
    fn roles() -> &'static [Role] {
        &[]
    }

    /// The role a node takes when the document does not declare one.
    fn default_role() -> Option<&'static str> {
        None
    }

    /// The rules JSON Schema cannot express, in the words the agent reads
    /// (`f20-reuso.md`, S11): what is genuinely the kind's own teaching. The
    /// contract adds the generic rules and the line of the vocabulary.
    fn rules(&self) -> Vec<String> {
        Vec::new()
    }

    /// A minimal document of the kind that validates and paints (PRD §6.1,
    /// `f20-reuso.md` S9): the example lives with the kind, so a new kind
    /// never touches `gx-mcp`. The registry pin catches a kind that forgets
    /// it.
    fn min_example() -> Value {
        Value::Null
    }
}

/// Object safe view of a kind, used by the registry.
pub trait KindHandler: Send + Sync {
    fn kind(&self) -> &'static str;
    fn phase(&self) -> &'static str;
    fn layout_hint(&self) -> LayoutHint;
    fn limits(&self) -> KindLimits;
    fn schema(&self) -> schemars::Schema;

    /// The contract version the kind serves (`f23-contrato.md`, S2): the
    /// `kind_version` the schema response publishes, so an agent knows which
    /// generation of the contract it is reading.
    fn kind_version(&self) -> u32;

    /// The shapes the kind draws (`f19-formas.md`, S4).
    fn shapes(&self) -> Vec<Shape>;

    /// The roles the kind admits (`f20-reuso.md`, S11).
    fn roles(&self) -> &'static [Role];

    /// The role of a node that declares none.
    fn default_role(&self) -> Option<&'static str>;

    /// The kind's own cross rules, in the words the agent reads.
    fn rules(&self) -> Vec<String>;

    /// The minimal document of the kind (`f20-reuso.md`, S9).
    fn min_example(&self) -> Value;

    /// Validates a document against the kind contract.
    fn validate_json(&self, document: &Value) -> Result<(), Vec<ValidationError>>;

    /// The cross rules of the kind: empty when the document is valid.
    ///
    /// These are the rules JSON Schema cannot express (PRD §11.1, regla de
    /// oro): unknown references, duplicated ids and the limits of size.
    fn check(&self, document: &Value) -> Vec<ValidationError>;

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

    fn phase(&self) -> &'static str {
        K::PHASE
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

    fn kind_version(&self) -> u32 {
        K::KIND_VERSION
    }

    fn shapes(&self) -> Vec<Shape> {
        K::shapes()
    }

    fn roles(&self) -> &'static [Role] {
        K::roles()
    }

    fn default_role(&self) -> Option<&'static str> {
        K::default_role()
    }

    fn rules(&self) -> Vec<String> {
        self.kind.rules()
    }

    fn min_example(&self) -> Value {
        K::min_example()
    }

    fn validate_json(&self, document: &Value) -> Result<(), Vec<ValidationError>> {
        let input = parse_input::<K>(document)?;
        let mut errors = title_errors(document);
        match self.kind.validate(&input) {
            Ok(()) => {}
            Err(more) => errors.extend(more),
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn check(&self, document: &Value) -> Vec<ValidationError> {
        let input = match parse_input::<K>(document) {
            Ok(input) => input,
            Err(errors) => return errors,
        };
        let mut errors = title_errors(document);
        errors.extend(self.kind.validate(&input).err().unwrap_or_default());
        errors
    }

    fn process_json(&self, document: &Value) -> Result<Ir, Vec<ValidationError>> {
        let input = parse_input::<K>(document)?;
        self.kind.validate(&input)?;
        let errors = title_errors(document);
        if !errors.is_empty() {
            return Err(errors);
        }
        Ok(self.kind.to_ir(&input))
    }
}

/// The rule of the document `title` (`f23-contrato.md`, S5): when it is
/// written, it is what the side panel shows — 1 to 120 visible characters
/// (PRD §10.1). Absent is fine: the panel falls back to the kind and the id.
///
/// It lives here, beside the adapter, because it is a rule of every kind at
/// once and no kind input needs to repeat it.
fn title_errors(document: &Value) -> Vec<ValidationError> {
    /// The longest title the panel holds without cutting it (PRD §10.1).
    const TITLE_CHARS: usize = 120;

    let Some(title) = document.get("title") else {
        return Vec::new();
    };
    if title.is_null() {
        return Vec::new();
    }
    let Some(title) = title.as_str() else {
        return vec![ValidationError::new(
            "title",
            ErrorCode::InvalidSchema,
            "`title` tiene que ser un texto",
            Some("escribí el título del diagrama, por ejemplo `pedido`".to_string()),
        )];
    };
    let visible = title.trim();
    if visible.is_empty() {
        return vec![ValidationError::new(
            "title",
            ErrorCode::InvalidValue,
            "el título no puede estar vacío",
            Some("dejá el campo afuera y el panel usa el kind, o escribí un título".to_string()),
        )];
    }
    let length = visible.chars().count();
    if length > TITLE_CHARS {
        return vec![ValidationError::new(
            "title",
            ErrorCode::InvalidValue,
            format!("el título tiene {length} caracteres y entran {TITLE_CHARS}"),
            Some(format!(
                "acortalo a {TITLE_CHARS} caracteres o menos: el panel lateral lo muestra entero"
            )),
        )];
    }
    Vec::new()
}

/// Parses a document into the typed input of a kind, mapping the failure to
/// a stable error code.
///
/// An unknown field is reported with the path of the field and a did-you-mean
/// hint with the closest admitted name (PRD §7 RF5).
fn parse_input<K: DiagramKind>(document: &Value) -> Result<K::Input, Vec<ValidationError>> {
    serde_json::from_value(document.clone()).map_err(|error| {
        // Serde does not say WHERE it failed: its message carries no path
        // (`f23-contrato.md`, S3). The schema of the kind does, so the
        // document is walked against it to name the place, and the serde
        // message rides along as the reason.
        let schema = serde_json::to_value(K::schema()).unwrap_or(Value::Null);
        if let Some(found) = first_mismatch(&schema, document) {
            return vec![found.with_reason(&error.to_string(), K::KIND)];
        }
        // The walk covers the schema subset schemars emits; anything it does
        // not model still reports the reason, at the document root.
        let code = if error.to_string().contains("missing field") {
            ErrorCode::MissingField
        } else {
            ErrorCode::InvalidSchema
        };
        vec![ValidationError::new(
            "",
            code,
            format!("el JSON no cumple el contrato de `{}`: {error}", K::KIND),
            Some("consultá get_diagram_schema para ver los campos admitidos".to_string()),
        )]
    })
}

/// One segment of an error path: a field name or an array index.
#[derive(Clone)]
enum Segment {
    Key(String),
    Index(usize),
}

impl Segment {
    fn push_render(&self, out: &mut String) {
        match self {
            Segment::Key(key) => {
                if !out.is_empty() {
                    out.push('.');
                }
                out.push_str(key);
            }
            Segment::Index(index) => {
                out.push('[');
                out.push_str(&index.to_string());
                out.push(']');
            }
        }
    }
}

/// The mismatch the walk found: its place, its reason and its remedy.
struct Mismatch {
    path: String,
    code: ErrorCode,
    message: String,
    hint: Option<String>,
}

impl Mismatch {
    fn with_reason(self, reason: &str, kind: &str) -> ValidationError {
        ValidationError::new(
            self.path,
            self.code,
            format!("{} en el documento de `{kind}`: {reason}", self.message),
            self.hint,
        )
    }
}

/// Walks a document against the JSON Schema of its kind and names the first
/// mismatch (`f23-contrato.md`, S3).
///
/// The walk covers the subset schemars emits: `$ref` into `$defs`, `type`
/// (a name or a list of names), `properties`, `required`, `items` and
/// `additionalProperties: false`. It is a locator, not a validator: serde
/// already rejected the document, and this only says where.
fn first_mismatch(schema: &Value, document: &Value) -> Option<Mismatch> {
    let defs = schema.get("$defs").cloned().unwrap_or(Value::Null);
    let mut path = Vec::new();
    walk(schema, &defs, document, &mut path)
}

fn walk(schema: &Value, defs: &Value, value: &Value, path: &mut Vec<Segment>) -> Option<Mismatch> {
    // A reference into `$defs` (the structs of the document).
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let name = reference.rsplit('/').next().unwrap_or_default();
        if let Some(target) = defs.get(name) {
            return walk(target, defs, value, path);
        }
    }

    let render = |path: &[Segment]| {
        let mut out = String::new();
        for segment in path {
            segment.push_render(&mut out);
        }
        out
    };

    // `type`: one name or a list of them; `null` admits an absent value.
    if let Some(types) = schema.get("type") {
        if !admits(types, value) {
            return Some(Mismatch {
                path: render(path),
                code: ErrorCode::InvalidSchema,
                message: format!("el campo `{}` no es del tipo esperado", render(path)),
                hint: Some(
                    "consultá get_diagram_schema para ver el tipo de cada campo".to_string(),
                ),
            });
        }
    }

    match value {
        Value::Object(map) => {
            // `required`: the fields the document owes.
            if let Some(required) = schema.get("required").and_then(Value::as_array) {
                for field in required.iter().filter_map(Value::as_str) {
                    if !map.contains_key(field) {
                        path.push(Segment::Key(field.to_string()));
                        let at = render(path);
                        path.pop();
                        return Some(Mismatch {
                            path: at.clone(),
                            code: ErrorCode::MissingField,
                            message: format!("falta `{field}`"),
                            hint: Some(format!("agregá `{field}` a `{}`", render(path))),
                        });
                    }
                }
            }
            let properties = schema.get("properties").and_then(Value::as_object);
            if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
                if let Some(properties) = properties {
                    for key in map.keys() {
                        if !properties.contains_key(key) {
                            let mut at = path.clone();
                            at.push(Segment::Key(key.clone()));
                            let admitted: Vec<String> = properties.keys().cloned().collect();
                            let hint = closest_name(key, &admitted)
                                .map(|closest| format!("did you mean `{closest}`?"))
                                .unwrap_or_else(|| {
                                    format!("los campos de este objeto son {}", admitted.join(", "))
                                });
                            return Some(Mismatch {
                                path: render(&at),
                                code: ErrorCode::UnknownField,
                                message: format!("campo desconocido `{key}`"),
                                hint: Some(hint),
                            });
                        }
                    }
                }
            }
            if let Some(properties) = properties {
                for (key, child_schema) in properties {
                    if let Some(child) = map.get(key) {
                        path.push(Segment::Key(key.clone()));
                        if let Some(found) = walk(child_schema, defs, child, path) {
                            return Some(found);
                        }
                        path.pop();
                    }
                }
            }
            None
        }
        Value::Array(items) => {
            if let Some(item_schema) = schema.get("items") {
                for (index, item) in items.iter().enumerate() {
                    path.push(Segment::Index(index));
                    if let Some(found) = walk(item_schema, defs, item, path) {
                        return Some(found);
                    }
                    path.pop();
                }
            }
            None
        }
        _ => None,
    }
}

/// Whether a `type` clause admits the JSON value.
fn admits(types: &Value, value: &Value) -> bool {
    let names: Vec<&str> = match types {
        Value::String(name) => vec![name.as_str()],
        Value::Array(items) => items.iter().filter_map(Value::as_str).collect(),
        _ => return true,
    };
    names.iter().any(|name| match (*name, value) {
        ("string", Value::String(_)) => true,
        ("boolean", Value::Bool(_)) => true,
        ("integer", Value::Number(number)) => number.is_i64() || number.is_u64(),
        ("number", Value::Number(_)) => true,
        ("array", Value::Array(_)) => true,
        ("object", Value::Object(_)) => true,
        ("null", Value::Null) => true,
        _ => false,
    })
}

/// The admitted name closest to a written one, when the distance is small
/// enough to be a typo and not a different word.
fn closest_name(field: &str, admitted: &[String]) -> Option<String> {
    admitted
        .iter()
        .map(|candidate| (levenshtein(field, candidate), candidate))
        .min_by(|left, right| left.0.cmp(&right.0))
        .filter(|(distance, _)| *distance <= (field.len() / 3).max(2))
        .map(|(_, candidate)| candidate.clone())
}

/// The edit distance between two words.
fn levenshtein(left: &str, right: &str) -> usize {
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
    use crate::ir::{Direction, Ir, IrGraph, IrNode, LayoutHint, Shape};
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
        const PHASE: &'static str = "F0";
        const LAYOUT_HINT: LayoutHint = LayoutHint::Layered;
        const LIMITS: KindLimits = KindLimits {
            max_nodes: 10,
            max_edges: 10,
            max_density: 3.0,
            max_lanes: 1,
            max_events: 1,
        };
        const KIND_VERSION: u32 = 1;
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

        fn roles() -> &'static [super::Role] {
            const ROLES: [super::Role; 1] = [super::Role::new("in", Shape::Rounded)];
            &ROLES
        }

        fn default_role() -> Option<&'static str> {
            Some("in")
        }

        fn rules(&self) -> Vec<String> {
            vec!["El ejemplo es un título: el resto lo pone la base.".to_string()]
        }

        fn min_example() -> serde_json::Value {
            serde_json::json!({ "version": 1, "kind": "demo", "title": "demo" })
        }
    }

    #[test]
    fn registers_and_looks_up_kinds() {
        register_kind::<DemoKind>();
        assert!(kind_names().contains(&"demo"));
        assert!(for_kind("demo").is_some());
        assert!(for_kind("nope").is_none());
    }

    /// S9-S11 de `f20-reuso.md`: el registro sirve el contrato del kind tal
    /// como el kind lo declara —fase, ejemplo, roles y reglas—, que es lo
    /// que deja a `gx-mcp` fuera del camino de un kind nuevo (O7).
    #[test]
    fn the_registry_serves_the_contract_of_the_kind_itself() {
        register_kind::<DemoKind>();
        let handler = for_kind("demo").expect("registered");
        assert_eq!(handler.phase(), "F0", "the phase is the kind's const");
        assert_eq!(
            handler.min_example(),
            serde_json::json!({ "version": 1, "kind": "demo", "title": "demo" }),
            "the example is the kind's own document"
        );
        assert_eq!(handler.roles().len(), 1);
        assert_eq!(handler.roles()[0].name, "in");
        assert_eq!(handler.roles()[0].shape, Shape::Rounded);
        assert_eq!(handler.default_role(), Some("in"));
        assert_eq!(handler.rules().len(), 1);
    }

    /// S9: un kind que no declara ejemplo no hereda el de otro: el default es
    /// `null` y el pin del catálogo lo delata. Lo mismo con el vocabulario:
    /// un kind sin roles no enseña una línea de roles.
    #[test]
    fn the_defaults_of_a_bare_kind_are_empty() {
        #[derive(Default)]
        struct BareKind;

        impl DiagramKind for BareKind {
            const KIND: &'static str = "bare";
            const PHASE: &'static str = "F0";
            const LAYOUT_HINT: LayoutHint = LayoutHint::Layered;
            const LIMITS: KindLimits = KindLimits::GRAPH;
            const KIND_VERSION: u32 = 1;
            type Input = DemoInput;

            fn validate(&self, _input: &Self::Input) -> Result<(), Vec<ValidationError>> {
                Ok(())
            }

            fn to_ir(&self, _input: &Self::Input) -> Ir {
                Ir::Graph(IrGraph {
                    direction: Direction::TB,
                    layout_hint: LayoutHint::Layered,
                    nodes: Vec::new(),
                    edges: Vec::new(),
                    groups: Vec::new(),
                })
            }

            fn schema() -> schemars::Schema {
                schemars::schema_for!(DemoInput)
            }
        }

        assert!(BareKind::min_example().is_null());
        assert!(BareKind::roles().is_empty());
        assert_eq!(BareKind::default_role(), None);
        assert!(BareKind.rules().is_empty());
        assert_eq!(BareKind::shapes(), vec![Shape::Rounded]);
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
