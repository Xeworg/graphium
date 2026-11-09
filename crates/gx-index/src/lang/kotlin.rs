//! The Kotlin chapter of the book (T3 Batch 3; S2, S4).
//!
//! The chapter keeps the declarations of a file — the classes and objects as
//! `class`, the interfaces as `trait` (the interface of the vocabulary), the
//! enum classes and the `typealias`es as `type` — with their functions as
//! children, nested under the `package` the file declares, which is what names
//! it in the language (`graphium.store.DiagramStore.Load`). A file with no
//! `package` keeps its directory, the way a Java file with no `package` does.
//! Swift's rule of a single node kind applies here too: Kotlin spells a class
//! and an interface with one `class_declaration` node and marks an enum with a
//! `class_modifier`, which is the one hook the tables cannot cover.
//!
//! **The companion decision.** A companion object is a declaration of Kotlin:
//! it has a name — the one it declares, or `Companion`, which is the name the
//! language gives the unnamed one and the name the source uses to reach it —
//! and its members are the members of that object, so the chapter keeps it as
//! a `class` with its members nested under it. **A data class is a class**: the
//! `data` modifier generates `copy`/`componentN` and changes no declaration, so
//! the book keeps the class the source spells and never a synthesised member
//! (Ruby's metaprogramming rule). Its primary-constructor `val`s are properties
//! of the class, not declarations of the book.
//!
//! The relations of this grammar are the four kinds of the schema, read
//! syntactically (D3):
//!
//! - `calls` — `parseConfig(input)`, `other.load("x")`, `DiagramStore(…)`;
//! - `imports` — the last segment of an `import`
//!   (`graphium.render.Renderer` → `Renderer`), or the alias it renames it to;
//! - `inherits` — every supertype of the `:` list: the superclass constructor,
//!   the interfaces and a delegation (`Loader by delegate`);
//! - `uses` — every type a declaration spells: a parameter (a constructor
//!   parameter included), a return type, a property type, the type alias.
//!
//! A secondary constructor and an `init` block declare no name of their own, so
//! they are not symbols of the book; an enum entry is not a declaration either
//! (Java's and C#'s enum constants are kept the same way), a property inside a
//! type is a field and not a declaration (Java, C# and C++ keep theirs the same
//! way), and a function body is not walked.
//!
//! What is Kotlin's alone is here; the declarations the book keeps are the
//! tables of [`crate::lang::registry::KOTLIN`] and the pipeline is
//! [`crate::lang::engine`] (S3, T2).

use tree_sitter::Node;

use crate::lang::emit::{dotted_directories, emit_named, text};
use crate::lang::engine::{Body, Nesting};
use crate::lang::model::ParsedSymbol;
use crate::relations::{push_edges, RawEdge, CALLS, IMPORTS, INHERITS, USES};

/// The language of the chapter, as the table `file` spells it.
pub const LANGUAGE: &str = "kotlin";

/// The tree-sitter grammar of Kotlin. The crate called `tree-sitter-kotlin` on
/// crates.io takes tree-sitter `<0.23` as a normal dependency and cannot feed
/// this engine, so the language comes from `tree-sitter-kotlin-ng` — the
/// tree-sitter-grammars grammar at the ABI of the engine. The feature and the
/// `lang` of the table are the language, never the crate (the rule of L8).
pub fn language(_path: &str) -> tree_sitter::Language {
    tree_sitter::Language::new(tree_sitter_kotlin_ng::LANGUAGE)
}

/// The module path of a Kotlin file: the `package` it declares, or its
/// directory when the file declares none. The first package of the file wins:
/// a file with two of them is not Kotlin.
pub fn module_path(root: &Node, path: &str, source: &[u8]) -> String {
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if child.kind() != "package_header" {
            continue;
        }
        if let Some(name) = package_name(&child, source) {
            return name;
        }
    }
    dotted_directories(path)
}

/// The dotted name of a `package_header`.
fn package_name(node: &Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    let name = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == "qualified_identifier")
        .and_then(|name| text(&name, source))?;
    let name = name.trim().to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// A `class_declaration`: a class, an interface or an enum class. The grammar
/// spells the two first with one node and the keyword is not a field, so the
/// language reads the unnamed keyword it left (`interface`) and the `enum`
/// modifier of the declaration.
pub(crate) fn type_declaration(
    node: &Node,
    nesting: &Nesting,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) -> Option<Body> {
    let kind = declared_kind(node);
    let name = node.child_by_field_name("name")?;
    let name = text(&name, source)?;
    let index = emit_named(node, kind, &name, nesting.prefix, nesting.parent, out)?;
    let prefix = format!("{}{}.", nesting.prefix, out[index].name);
    Some(Body {
        index,
        prefix,
        container: kind,
    })
}

/// The kind of a `class_declaration` in the vocabulary of the book.
fn declared_kind(node: &Node) -> &'static str {
    let mut cursor = node.walk();
    let interface = node
        .children(&mut cursor)
        .any(|child| child.kind() == "interface");
    if interface {
        return "trait";
    }
    // `enum class Mode` is a class with an `enum` class modifier: the keyword
    // lives inside the modifier, not on the declaration.
    let mut cursor = node.walk();
    let modifiers: Vec<Node> = node
        .children(&mut cursor)
        .filter(|child| child.kind() == "modifiers")
        .collect();
    let enumeration = modifiers.iter().any(|modifiers| {
        let mut inner = modifiers.walk();
        let list: Vec<Node> = modifiers.named_children(&mut inner).collect();
        list.iter().any(|modifier| {
            modifier.kind() == "class_modifier"
                && modifier
                    .children(&mut modifier.walk())
                    .any(|keyword| keyword.kind() == "enum")
        })
    });
    if enumeration {
        return "type";
    }
    "class"
}

/// A `companion object`: a class named `Companion` when it declares no name of
/// its own, which is the name the language gives it and the one the source uses
/// to reach its members.
pub(crate) fn companion(
    node: &Node,
    nesting: &Nesting,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) -> Option<Body> {
    let name = node
        .child_by_field_name("name")
        .and_then(|name| text(&name, source))
        .unwrap_or_else(|| "Companion".to_string());
    let index = emit_named(node, "class", &name, nesting.prefix, nesting.parent, out)?;
    let prefix = format!("{}{}.", nesting.prefix, out[index].name);
    Some(Body {
        index,
        prefix,
        container: "class",
    })
}

/// A `property_declaration`: a `const` at the root of the file, a field
/// everywhere else. `val LIMIT = 3` at the root is a declaration of the
/// chapter; `private val renderer` of a class is a field of the book's
/// vocabulary — none.
pub(crate) fn property(
    node: &Node,
    nesting: &Nesting,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) -> Option<Body> {
    if nesting.container.is_some() {
        return None;
    }
    let mut declarations: Vec<Node> = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "variable_declaration" => declarations.push(child),
            // `val (a, b) = pair` declares one name per variable.
            "multi_variable_declaration" => {
                let mut inner = child.walk();
                declarations.extend(child.named_children(&mut inner));
            }
            _ => {}
        }
    }
    for declaration in declarations {
        let mut inner = declaration.walk();
        let name = declaration
            .named_children(&mut inner)
            .find(|child| child.kind() == "identifier")
            .and_then(|name| text(&name, source));
        if let Some(name) = name {
            emit_named(
                &declaration,
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

/// A `type_alias`: the grammar names the alias in the field the table `type`
/// uses for the aliased type, so its name is the one hook the table cannot
/// cover.
pub(crate) fn alias(
    node: &Node,
    nesting: &Nesting,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) -> Option<Body> {
    let name = node
        .child_by_field_name("type")
        .and_then(|name| text(&name, source))?;
    emit_named(node, "type", &name, nesting.prefix, nesting.parent, out);
    None
}

/// The relations of one Kotlin chapter, as the grammar sees them.
pub(crate) fn relations(root: &Node, source: &[u8]) -> Vec<RawEdge> {
    let mut out = Vec::new();
    visit(root, source, &mut out);
    out
}

/// Walks the tree once and records every relation it recognises.
fn visit(node: &Node, source: &[u8], out: &mut Vec<RawEdge>) {
    let byte = node.start_byte();
    match node.kind() {
        // `import graphium.render.Renderer` and `import dart.async as a`: the
        // name the source binds is the last segment, or the alias.
        "import" => {
            if let Some(name) = imported(node, source) {
                push_edges(out, byte, IMPORTS, vec![name]);
            }
        }
        // `: Base(), Loader`, `: Loader by delegate`: every supertype.
        "delegation_specifier" => {
            push_edges(out, byte, INHERITS, delegated(node, source));
        }
        // `parseConfig(input)`, `other.load("x")`, `DiagramStore(…)`.
        "call_expression" => {
            if let Some(name) = called(node, source) {
                push_edges(out, byte, CALLS, vec![name]);
            }
        }
        // The types a declaration spells: a parameter, a return type, a
        // property type, the underlying type of a `typealias`.
        "parameter"
        | "class_parameter"
        | "function_declaration"
        | "property_declaration"
        | "type_alias" => {
            push_edges(out, byte, USES, declared_types(node, source));
        }
        _ => {}
    }

    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    for child in children {
        visit(&child, source, out);
    }
}

/// The name an `import` binds: its alias, or the last segment of the path.
fn imported(node: &Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    // `import a.b.C as D` binds `D`; the alias is a direct identifier child.
    if let Some(alias) = children
        .iter()
        .find(|child| child.kind() == "identifier")
        .and_then(|alias| text(alias, source))
    {
        return Some(alias);
    }
    let path = children
        .iter()
        .find(|child| child.kind() == "qualified_identifier")
        .and_then(|path| text(path, source))?;
    let segment = path.rsplit('.').next().unwrap_or(&path).trim();
    if segment.is_empty() {
        None
    } else {
        Some(segment.to_string())
    }
}

/// The supertype a delegating specifier names.
fn delegated(node: &Node, source: &[u8]) -> Vec<String> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "annotation" => continue,
            // `Loader by delegate`: the supertype is the `type` it delegates.
            "explicit_delegation" => {
                let mut inner = child.walk();
                let names = child
                    .named_children(&mut inner)
                    .find(|inner| is_type(inner.kind()))
                    .map(|inner| type_names(&inner, source))
                    .unwrap_or_default();
                return names;
            }
            // `Base()`: the supertype is the type the constructor invocation
            // names.
            "constructor_invocation" => {
                let mut inner = child.walk();
                let names = child
                    .named_children(&mut inner)
                    .find(|inner| is_type(inner.kind()))
                    .map(|inner| type_names(&inner, source))
                    .unwrap_or_default();
                return names;
            }
            // `Loader`: the type of the supertype.
            _ => return type_names(&child, source),
        }
    }
    Vec::new()
}

/// The name a call expression targets, when the source spells one.
fn called(node: &Node, source: &[u8]) -> Option<String> {
    let target = node.named_child(0)?;
    match target.kind() {
        "identifier" => text(&target, source),
        // `other.load(…)`: the member the navigation names.
        "navigation_expression" => {
            let mut cursor = target.walk();
            let names: Vec<Node> = target
                .named_children(&mut cursor)
                .filter(|child| child.kind() == "identifier")
                .collect();
            names.last().and_then(|name| text(name, source))
        }
        _ => None,
    }
}

/// Whether a node kind is a type expression of the Kotlin grammar.
fn is_type(kind: &str) -> bool {
    matches!(
        kind,
        "type"
            | "user_type"
            | "nullable_type"
            | "parenthesized_type"
            | "function_type"
            | "type_projection"
            | "type_arguments"
            | "type_parameters"
            | "type_parameter"
    )
}

/// Every type a declaration names in its type positions.
fn declared_types(node: &Node, source: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if is_type(child.kind()) {
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

/// Collects the names of one type expression.
fn type_names_into(node: &Node, source: &[u8], out: &mut Vec<String>) {
    match node.kind() {
        // `List<Renderer>` names `List` and `Renderer`.
        "user_type" => {
            let mut cursor = node.walk();
            let children: Vec<Node> = node.named_children(&mut cursor).collect();
            for child in children {
                match child.kind() {
                    "identifier" => {
                        if let Some(name) = text(&child, source) {
                            out.push(name);
                        }
                    }
                    "type_arguments" => type_names_into(&child, source, out),
                    _ => {}
                }
            }
        }
        kind if is_type(kind) => {
            let mut cursor = node.walk();
            let children: Vec<Node> = node.named_children(&mut cursor).collect();
            for child in children {
                type_names_into(&child, source, out);
            }
        }
        // A bare identifier in a type position (`Unit`, `Int`) is the name the
        // source spells.
        "identifier" => {
            if let Some(name) = text(node, source) {
                out.push(name);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use crate::lang::registry::KOTLIN;

    /// The `package` of the file names the chapter, and the declarations map to
    /// the vocabulary of the book: an interface is a `trait`, an enum class a
    /// `type` and a data class a `class`.
    #[test]
    fn the_package_of_a_kotlin_file_names_its_chapter() {
        let source = r#"
package graphium.store

interface Loader {
    fun load(input: String): Int
}

data class Limits(val max: Int)

enum class Mode { Fast, Slow }

class DiagramStore : Loader {
    override fun load(input: String): Int = 0
}

val LIMIT = 3
"#;
        let parsed = KOTLIN
            .parse("src/store.kt", source.as_bytes(), 0)
            .expect("parses");
        let rows: Vec<(&str, &str, &str)> = parsed
            .symbols
            .iter()
            .map(|symbol| (symbol.kind, symbol.name.as_str(), symbol.qname.as_str()))
            .collect();
        for expected in [
            ("trait", "Loader", "graphium.store.Loader"),
            ("method", "load", "graphium.store.Loader.load"),
            ("class", "Limits", "graphium.store.Limits"),
            ("type", "Mode", "graphium.store.Mode"),
            ("class", "DiagramStore", "graphium.store.DiagramStore"),
            ("method", "load", "graphium.store.DiagramStore.load"),
            ("const", "LIMIT", "graphium.store.LIMIT"),
        ] {
            assert!(rows.contains(&expected), "missing {expected:?}: {rows:?}");
        }
        // A constructor `val` is a property of the class, not a declaration.
        assert!(
            !parsed.symbols.iter().any(|symbol| symbol.name == "max"),
            "a constructor property is not a declaration: {rows:?}"
        );
    }

    /// A file with no `package` keeps its directory, the way a Java file does.
    #[test]
    fn a_kotlin_file_without_a_package_keeps_its_directory() {
        let parsed = KOTLIN
            .parse(
                "src/graphium/store/One.kt",
                b"class One {\n    fun open(): Int = 0\n}\n",
                0,
            )
            .expect("parses");
        assert!(
            parsed
                .symbols
                .iter()
                .any(|symbol| symbol.qname == "src.graphium.store.One"),
            "{:?}",
            parsed.symbols
        );
    }

    /// A companion object is a class named `Companion` when it declares no
    /// name of its own, and its members hang from it.
    #[test]
    fn a_companion_object_is_a_declaration_of_the_book() {
        let source =
            "class DiagramStore {\n    companion object {\n        fun open(): Int = 0\n    }\n}\n";
        let parsed = KOTLIN
            .parse("src/store.kt", source.as_bytes(), 0)
            .expect("parses");
        let rows: Vec<(&str, &str, &str)> = parsed
            .symbols
            .iter()
            .map(|symbol| (symbol.kind, symbol.name.as_str(), symbol.qname.as_str()))
            .collect();
        assert!(
            rows.contains(&("class", "Companion", "src.DiagramStore.Companion")),
            "{rows:?}"
        );
        assert!(
            rows.contains(&("method", "open", "src.DiagramStore.Companion.open")),
            "{rows:?}"
        );
    }
}
