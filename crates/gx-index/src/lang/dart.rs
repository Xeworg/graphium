//! The Dart chapter of the book (T3 Batch 3; S2, S4).
//!
//! The chapter keeps the declarations of a file — the classes as `class`, the
//! mixins as `trait` (the interface of the vocabulary), the enums, the
//! `extension`s and the `typedef`s as `type` — with their methods and
//! constructors as children, and the variables declared at the root of the file
//! as `const`. The module of the chapter is the path of the file
//! (`lib.store`): a `library` directive names a library, never a path, so the
//! file is the module the way it is in JavaScript and Ruby.
//!
//! **The mixin decision.** `class DiagramStore extends Base with Loggable` is
//! read as `inherits` for both clauses: `extends` names the superclass and
//! `with` embeds a mixin — the relation PHP gives `use TraitName` and Ruby gives
//! `include`. A `mixin Loggable on Base` declares a mixin (`trait`) and its
//! `on` clause is a **use-site constraint**, not inheritance: the chapter reads
//! it as a `uses`, because it names the type the mixin may be applied to.
//! Dart's grammar spells the members of a type one level down
//! (`member_declaration` → `method_declaration` → `method_signature` →
//! `function_signature`): the name lives in the signature and the span the book
//! needs is the whole declaration, because the calls of a body hang from the
//! method that owns them — so the declarations of a file and of a type are the
//! one place where Dart names its own symbols (the hooks of `dart.rs`).
//!
//! The relations of this grammar are the four kinds of the schema, read
//! syntactically (D3):
//!
//! - `calls` — `parseConfig(input)`, `other.load('x')`, `DiagramStore(…)`, a
//!   named constructor (`DiagramStore.named()`) and a `new` expression;
//! - `imports` — the alias of an `import` when it has one, else the stem of the
//!   URI it names (`package:graphium/render.dart` → `render`); an `export` is
//!   the same module reference and is read the same way;
//! - `inherits` — the `superclass`, every `with` mixin and every `implements`
//!   interface;
//! - `uses` — every type a declaration spells: a parameter, a return type, a
//!   field, a top-level variable, the type an `extension` extends.
//!
//! A `mixin C = Base with M;` (`mixin_application_class`) is not read as a
//! declaration: the book keeps the class the source declares with a body, and
//! that form is rare enough to stay out until a fixture needs it (it is noted
//! here rather than silently forgotten). A field is not a declaration of the
//! book and a function body is not walked.
//!
//! What is Dart's alone is here; the declarations the book keeps are the tables
//! of [`crate::lang::registry::DART`] and the pipeline is
//! [`crate::lang::engine`] (S3, T2).

use tree_sitter::Node;

use crate::lang::emit::{dotted_module, emit_named, text};
use crate::lang::engine::{Body, Nesting};
use crate::lang::model::ParsedSymbol;
use crate::relations::{push_edges, RawEdge, CALLS, IMPORTS, INHERITS, USES};

/// The language of the chapter, as the table `file` spells it.
pub const LANGUAGE: &str = "dart";

/// The tree-sitter grammar of Dart: one grammar answers every Dart path.
pub fn language(_path: &str) -> tree_sitter::Language {
    tree_sitter::Language::new(tree_sitter_dart::LANGUAGE)
}

/// The module path of a Dart file: its directories and its stem
/// (`lib/store.dart` → `lib.store`).
pub fn module_path(relative: &str) -> String {
    dotted_module(relative, &[])
}

/// A declaration of a file or of a type: the name is the one its signature
/// spells and the span is the whole declaration, body included — a `fn` at the
/// root of the file and a `method` inside a type. A `declaration` also wraps
/// the fields of a class, which are not declarations of the book, and a
/// constructor, which is a method named as the source spells it.
pub(crate) fn declaration(
    node: &Node,
    nesting: &Nesting,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) -> Option<Body> {
    let (kind, name) = if node.kind() == "declaration" {
        declared_member(node, source)?
    } else {
        let signature = node.child_by_field_name("signature")?;
        (member_kind(nesting), signature_name(&signature, source)?)
    };
    emit_named(node, kind, &name, nesting.prefix, nesting.parent, out);
    None
}

/// The kind of a member: a `method` inside a type and a `fn` at the root of the
/// file.
fn member_kind(nesting: &Nesting) -> &'static str {
    if nesting.container.is_some() {
        "method"
    } else {
        "fn"
    }
}

/// The declaration a `declaration` node wraps when it declares one: an abstract
/// method (a bare `function_signature`) or a constructor. A field is not a
/// declaration of the book and answers `None`.
fn declared_member(node: &Node, source: &[u8]) -> Option<(&'static str, String)> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "function_signature" | "getter_signature" | "setter_signature" => {
                return signature_name(&child, source).map(|name| ("method", name));
            }
            "constructor_signature"
            | "factory_constructor_signature"
            | "redirecting_factory_constructor_signature"
            | "constant_constructor_signature" => {
                return signature_name(&child, source).map(|name| ("method", name));
            }
            _ => {}
        }
    }
    None
}

/// The name a signature spells: the `name` field, or for a named constructor
/// the constructor's own name (`DiagramStore.named(…)` names `named`).
fn signature_name(signature: &Node, source: &[u8]) -> Option<String> {
    match signature.kind() {
        // A `method_signature` only wraps the signature of the method.
        "method_signature" => {
            let mut cursor = signature.walk();
            let inner = signature.named_children(&mut cursor).find(|child| {
                matches!(
                    child.kind(),
                    "function_signature"
                        | "getter_signature"
                        | "setter_signature"
                        | "constructor_signature"
                        | "factory_constructor_signature"
                        | "redirecting_factory_constructor_signature"
                        | "constant_constructor_signature"
                )
            })?;
            signature_name(&inner, source)
        }
        "factory_constructor_signature" if signature.child_by_field_name("name").is_none() => {
            // `factory Store()` declares no name of its own: the class names it.
            None
        }
        _ => {
            let mut cursor = signature.walk();
            let named: Vec<Node> = signature
                .children_by_field_name("name", &mut cursor)
                .collect();
            named
                .iter()
                .rev()
                .find(|name| name.kind() == "identifier")
                .and_then(|name| text(name, source))
                .or_else(|| {
                    named.last().and_then(|name| {
                        if name.kind() == "new" {
                            Some("new".to_string())
                        } else {
                            text(name, source)
                        }
                    })
                })
        }
    }
}

/// A `type_alias`: the grammar spells the alias name with no field, so the
/// first `type_identifier` of the node is its name.
pub(crate) fn alias(
    node: &Node,
    nesting: &Nesting,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) -> Option<Body> {
    let mut cursor = node.walk();
    let name = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == "type_identifier")
        .and_then(|name| text(&name, source))?;
    emit_named(node, "type", &name, nesting.prefix, nesting.parent, out);
    None
}

/// A `top_level_variable_declaration`: one `const` per declarator, the way
/// JavaScript and PHP read a declaration group.
pub(crate) fn constants(
    node: &Node,
    nesting: &Nesting,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) -> Option<Body> {
    for declarator in declarators(node) {
        if let Some(name) = declarator
            .child_by_field_name("name")
            .and_then(|name| text(&name, source))
        {
            emit_named(
                &declarator,
                "const",
                &name,
                nesting.prefix,
                nesting.parent,
                out,
            );
        }
    }
    None
}

/// The declarators of a top-level variable declaration: `const int LIMIT = 3`
/// and `late final store = …` spell them in two different lists.
fn declarators<'a>(node: &Node<'a>) -> Vec<Node<'a>> {
    let mut out = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "static_final_declaration_list" | "initialized_identifier_list" => {
                let mut inner = child.walk();
                out.extend(child.named_children(&mut inner));
            }
            _ => {}
        }
    }
    out
}

/// The relations of one Dart chapter, as the grammar sees them.
pub(crate) fn relations(root: &Node, source: &[u8]) -> Vec<RawEdge> {
    let mut out = Vec::new();
    visit(root, source, &mut out);
    out
}

/// Walks the tree once and records every relation it recognises.
fn visit(node: &Node, source: &[u8], out: &mut Vec<RawEdge>) {
    let byte = node.start_byte();
    match node.kind() {
        // `import 'package:graphium/render.dart'` and `export 'store.dart'`.
        "import_specification" | "library_export" => {
            if let Some(name) = imported(node, source) {
                push_edges(out, byte, IMPORTS, vec![name]);
            }
        }
        // `extends Base`: the superclass of a class.
        "superclass" => {
            if let Some(inherited) = node.child_by_field_name("type") {
                push_edges(out, byte, INHERITS, type_names(&inherited, source));
            }
        }
        // `with Loggable` and `implements Loader`: both embed a type, the
        // relation PHP gives a trait and Ruby an `include`.
        "mixins" | "interfaces" => {
            push_edges(out, byte, INHERITS, direct_types(node, source));
        }
        // `parseConfig(input)`, `other.load('x')`, `DiagramStore(…)`.
        "call_expression" => {
            if let Some(name) = called(node, source) {
                push_edges(out, byte, CALLS, vec![name]);
            }
        }
        // `new DiagramStore(…)` and the named constructor of a type.
        "new_expression" | "constructor_invocation" => {
            if let Some(name) = constructed(node, source) {
                push_edges(out, byte, CALLS, vec![name]);
            }
        }
        // The types a declaration spells: a parameter, a return type, a field,
        // a top-level variable.
        "formal_parameter"
        | "typed_identifier"
        | "function_signature"
        | "declaration"
        | "top_level_variable_declaration" => {
            push_edges(out, byte, USES, declared_types(node, source));
        }
        // `extension DiagramStoreExt on DiagramStore`: the type it extends.
        "extension_declaration" => {
            if let Some(extended) = node.child_by_field_name("class") {
                push_edges(out, byte, USES, type_names(&extended, source));
            }
        }
        // `mixin Loggable on Base`: a use-site constraint, not inheritance.
        "mixin_declaration" => {
            push_edges(out, byte, USES, direct_types(node, source));
        }
        _ => {}
    }

    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    for child in children {
        visit(&child, source, out);
    }
}

/// The name an `import` or an `export` binds: its alias, else the stem of the
/// URI it names.
fn imported(node: &Node, source: &[u8]) -> Option<String> {
    if let Some(alias) = node
        .child_by_field_name("alias")
        .and_then(|alias| text(&alias, source))
    {
        return Some(alias);
    }
    let uri = node
        .child_by_field_name("uri")
        .and_then(|uri| uri_name(&uri, source))?;
    let path = uri.trim_matches(|char| char == '\'' || char == '"');
    let file = path.rsplit('/').next().unwrap_or(path);
    let stem = file.split('.').next().unwrap_or(file).trim();
    if stem.is_empty() {
        None
    } else {
        Some(stem.to_string())
    }
}

/// The URI a `configurable_uri` wraps, as the source spells it.
fn uri_name(node: &Node, source: &[u8]) -> Option<String> {
    if node.kind() == "uri" {
        return text(node, source);
    }
    let mut cursor = node.walk();
    let inner = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == "uri")?;
    text(&inner, source)
}

/// The name a call expression targets, when the source spells one.
fn called(node: &Node, source: &[u8]) -> Option<String> {
    let function = node.child_by_field_name("function")?;
    match function.kind() {
        "identifier" => text(&function, source),
        // `other.load('x')`: the property the access names.
        "member_expression" => function
            .child_by_field_name("property")
            .and_then(|property| text(&property, source)),
        _ => None,
    }
}

/// The type a `new` expression or a named constructor instantiates.
fn constructed(node: &Node, source: &[u8]) -> Option<String> {
    let constructed = node.child_by_field_name("type")?;
    type_names(&constructed, source).into_iter().next()
}

/// Every type a declaration names in its type positions.
fn declared_types(node: &Node, source: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "type" {
            type_names_into(&child, source, &mut names);
        }
    }
    names.dedup();
    names
}

/// Every type a clause lists directly: the members of `with A, B`.
fn direct_types(node: &Node, source: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    for child in children {
        if child.kind() == "type" {
            type_names_into(&child, source, &mut names);
        }
    }
    names.dedup();
    names
}

/// Every name a type expression spells.
fn type_names(node: &Node, source: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    type_names_into(node, source, &mut names);
    names.dedup();
    names
}

/// Collects the names of one type expression: `List<Renderer>` names `List`
/// and `Renderer`, and `void`/`Function` name nothing.
fn type_names_into(node: &Node, source: &[u8], out: &mut Vec<String>) {
    match node.kind() {
        "type_identifier" => {
            if let Some(name) = text(node, source) {
                out.push(name);
            }
        }
        // `void` and `Function` are keywords of the language: they have no
        // declaration in the book (D3).
        "void_type" => {}
        _ => {
            let mut cursor = node.walk();
            let children: Vec<Node> = node.named_children(&mut cursor).collect();
            for child in children {
                type_names_into(&child, source, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::lang::registry::DART;

    /// The declarations of Dart map to the vocabulary of the book: a mixin is a
    /// `trait`, an enum and an extension a `type`, and the members of a class
    /// are its methods one wrapper at a time.
    #[test]
    fn the_declarations_of_dart_map_to_the_vocabulary_of_the_book() {
        let source = r#"
mixin Loggable {
  void log(String message) {}
}

enum Mode { fast, slow }

class DiagramStore with Loggable {
  DiagramStore(this.renderer);

  int load(String input) => 0;
}

extension DiagramStoreExt on DiagramStore {
  int describe() => 0;
}

typedef Handler = int Function(String input);

const int LIMIT = 3;
"#;
        let parsed = DART
            .parse("lib/store.dart", source.as_bytes(), 0)
            .expect("parses");
        let rows: Vec<(&str, &str, &str)> = parsed
            .symbols
            .iter()
            .map(|symbol| (symbol.kind, symbol.name.as_str(), symbol.qname.as_str()))
            .collect();
        for expected in [
            ("trait", "Loggable", "lib.store.Loggable"),
            ("method", "log", "lib.store.Loggable.log"),
            ("type", "Mode", "lib.store.Mode"),
            ("class", "DiagramStore", "lib.store.DiagramStore"),
            (
                "method",
                "DiagramStore",
                "lib.store.DiagramStore.DiagramStore",
            ),
            ("method", "load", "lib.store.DiagramStore.load"),
            ("type", "DiagramStoreExt", "lib.store.DiagramStoreExt"),
            ("method", "describe", "lib.store.DiagramStoreExt.describe"),
            ("type", "Handler", "lib.store.Handler"),
            ("const", "LIMIT", "lib.store.LIMIT"),
        ] {
            assert!(rows.contains(&expected), "missing {expected:?}: {rows:?}");
        }
    }

    /// `with` is an embedding — PHP's `use Trait` and Ruby's `include` — so it
    /// is `inherits`, and a mixin's `on` clause is a constraint, not a
    /// supertype.
    #[test]
    fn a_mixin_application_is_inherits_and_its_on_clause_is_uses() {
        let source = r#"
mixin Loggable on Base {
  void log(String message) {}
}

class Base {}

class DiagramStore extends Base with Loggable {}
"#;
        let parsed = DART
            .parse("lib/store.dart", source.as_bytes(), 0)
            .expect("parses");
        let inherits: Vec<&str> = parsed
            .edges
            .iter()
            .filter(|edge| edge.kind == "inherits")
            .map(|edge| edge.dst_name.as_str())
            .collect();
        let uses: Vec<&str> = parsed
            .edges
            .iter()
            .filter(|edge| edge.kind == "uses")
            .map(|edge| edge.dst_name.as_str())
            .collect();
        assert!(
            inherits.contains(&"Base"),
            "extends is inherits: {inherits:?}"
        );
        assert!(
            inherits.contains(&"Loggable"),
            "a mixin is inherits: {inherits:?}"
        );
        assert!(
            !inherits.contains(&"Loggable") || uses.contains(&"Base"),
            "the `on` clause of the mixin is uses: {uses:?}"
        );
    }
}
