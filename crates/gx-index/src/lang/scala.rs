//! The Scala chapter of the book (T3 Batch 4; S2, S4).
//!
//! The chapter keeps the types of a file with their members, nested under the
//! `package` the file declares, which is what names it in the language
//! (`graphium.store.DiagramStore.load`). A file with no `package` keeps its
//! directory, the way a Java file with no `package` does. The module path comes
//! from the tree and not from the path for the same reason as Java's, C#'s,
//! PHP's and Kotlin's: the file names its own module, and the directories are
//! only the fallback.
//!
//! **The three Scala decisions.**
//!
//! - **A `case class` is a class.** The `case` modifier synthesises `apply`,
//!   `unapply`, `copy` and the equality members, and changes no declaration the
//!   source spells, so the book keeps the class and never a synthesised member
//!   (Kotlin's data-class rule of L10). The grammar helps: `case class` and
//!   `class` are the same node kind, told apart by a modifier the tables never
//!   read.
//! - **An `object` is a `class`.** A Scala `object` defines a class *and its
//!   only instance* — the language specification calls it a singleton object and
//!   the compiler emits a real class for it — so the vocabulary's `class` is
//!   what it is, and its members nest under it exactly as the source reaches
//!   them (`Defaults.open`). It is **not** the book's `module` kind: a `module`
//!   is a namespace with no value (a Rust `mod`), while an `object` is a value
//!   of the program — it can extend a class, implement a trait and be passed
//!   around, and the fixture's `object Loader extends Loader` is inheritance
//!   from it.
//! - **A `trait` is a `trait`** (the interface of the vocabulary) and a Scala 3
//!   `enum` is a `type` — a sum type of the language, like the enum of Java,
//!   C#, Kotlin and Dart.
//!
//! **Fields and abstract members are not declarations.** A `val`/`var` inside a
//! class, a trait, an object or an enum is a field of the book's vocabulary —
//! none — exactly as a Java field, a C# property or a Kotlin property is not one;
//! `val NAME = 3` at the root of the file (Scala 3 allows it) is a `const`, and
//! so is a `var` there (a binding of the module, whatever its mutability: the
//! rule Python and Shell already follow). An abstract `val` of a trait
//! (`val name: String`) declares no value and is not a symbol either.
//!
//! **What is not read, and why** (dragged out of the tables on purpose):
//! a Scala 3 `given` names itself only sometimes and its body is an expression,
//! an `extension` adds methods to a type named inside its parameter list, and a
//! `package object` is the package itself, so the three are not symbols of the
//! book; their edges are still read (`extends` and `uses` of a `given` and of an
//! `extension` belong to the declaration that owns the text). A `derives`
//! clause is a generated instance and is not read as `inherits` for the same
//! reason a Swift `where` clause is not (L10).
//!
//! The relations are the four kinds of the schema, read syntactically (D3):
//!
//! - `calls` — `parseConfig(input)`, `store.load(input)`, `DiagramStore(…)`
//!   (the `apply` sugar) and `new DiagramStore(…)`;
//! - `imports` — the last segment of the path of an `import`, the alias of a
//!   renamed one (`import a.b.{Painter => Renamed}` binds `Renamed`);
//! - `inherits` — every type of an `extends` clause, `with Base with Loader`
//!   included;
//! - `uses` — every type a declaration spells: a parameter, a return type, the
//!   type of a `val`/`var`, the aliased type of a `type` definition.
//!
//! What is Scala's alone is here; the declarations the book keeps are the tables
//! of [`crate::lang::registry::SCALA`] and the pipeline is
//! [`crate::lang::engine`] (S3, T2).

use tree_sitter::Node;

use crate::lang::emit::{dotted_directories, emit_named, text};
use crate::lang::engine::{Body, Nesting};
use crate::lang::model::ParsedSymbol;
use crate::relations::{push_edges, RawEdge, CALLS, IMPORTS, INHERITS, USES};

/// The language of the chapter, as the table `file` spells it.
pub const LANGUAGE: &str = "scala";

/// The tree-sitter grammar of Scala.
pub fn language(_path: &str) -> tree_sitter::Language {
    tree_sitter::Language::new(tree_sitter_scala::LANGUAGE)
}

/// The module path of a Scala file: the `package` it declares, or its directory
/// when the file declares none.
///
/// The first `package_clause` of the file wins. A Scala 2 file may nest a second
/// clause (`package graphium { package store { … } }`); the path is the outer
/// one and the inner name is not repeated in the `qname`, which is the
/// namespace-reading rule of C++ (`src/store.cpp` does not nest twice either).
pub fn module_path(root: &Node, path: &str, source: &[u8]) -> String {
    let mut cursor = root.walk();
    let clauses: Vec<Node> = root
        .named_children(&mut cursor)
        .filter(|child| child.kind() == "package_clause")
        .collect();
    for clause in clauses {
        if let Some(name) = package_name(&clause, source) {
            return name;
        }
    }
    dotted_directories(path)
}

/// The dotted name of a `package_clause`: its `package_identifier` is a list of
/// identifiers, and the dots between them are the language's module separator.
fn package_name(node: &Node, source: &[u8]) -> Option<String> {
    let identifier = node.child_by_field_name("name")?;
    let mut cursor = identifier.walk();
    let parts: Vec<String> = identifier
        .named_children(&mut cursor)
        .filter(|child| matches!(child.kind(), "identifier" | "operator_identifier"))
        .filter_map(|child| text(&child, source))
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("."))
    }
}

/// A `val`/`var` definition: a `const` at the root of the file, a field (no
/// declaration of the book) everywhere else.
///
/// The grammar names the bound identifier in the `pattern` field and not in the
/// `name` the table would read, which is why this is a hook. A destructuring
/// pattern (`val (a, b) = pair`) names no single binding and is skipped.
pub(crate) fn value(
    node: &Node,
    nesting: &Nesting,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) -> Option<Body> {
    if nesting.container.is_some() {
        return None;
    }
    let pattern = node.child_by_field_name("pattern")?;
    let name = match pattern.kind() {
        "identifier" | "operator_identifier" => text(&pattern, source)?,
        _ => return None,
    };
    emit_named(node, "const", &name, nesting.prefix, nesting.parent, out);
    None
}

/// The relations of one Scala chapter, as the grammar sees them.
pub(crate) fn relations(root: &Node, source: &[u8]) -> Vec<RawEdge> {
    let mut out = Vec::new();
    visit(root, source, &mut out);
    out
}

/// Walks the tree once and records every relation it recognises.
fn visit(node: &Node, source: &[u8], out: &mut Vec<RawEdge>) {
    let byte = node.start_byte();
    match node.kind() {
        // `import graphium.store.DiagramStore`, `import a.b.{C, D => E}`.
        "import_declaration" => push_edges(out, byte, IMPORTS, imported(node, source)),
        // `extends Base with Loader`: every type of the clause.
        "extends_clause" => push_edges(out, byte, INHERITS, types_of_field(node, "type", source)),
        // `parseConfig(input)`, `store.load(input)`, `DiagramStore(…)`.
        "call_expression" => {
            if let Some(name) = called(node, source) {
                push_edges(out, byte, CALLS, vec![name]);
            }
        }
        // `new DiagramStore(renderer)` names the type it instantiates.
        "instance_expression" => {
            if let Some(name) = instantiated(node, source) {
                push_edges(out, byte, CALLS, vec![name]);
            }
        }
        // The types a declaration spells.
        "parameter" | "class_parameter" | "val_definition" | "var_definition"
        | "val_declaration" => {
            push_edges(out, byte, USES, types_of_field(node, "type", source));
        }
        "function_definition" | "function_declaration" => {
            push_edges(out, byte, USES, types_of_field(node, "return_type", source));
        }
        "type_definition" => push_edges(out, byte, USES, types_of_field(node, "type", source)),
        _ => {}
    }

    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    for child in children {
        visit(&child, source, out);
    }
}

/// The names an `import` binds: the selectors of a grouped import (their alias
/// when they spell one), else the last segment of the path.
fn imported(node: &Node, source: &[u8]) -> Vec<String> {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    let mut names = Vec::new();
    for selector in children
        .iter()
        .filter(|child| child.kind() == "namespace_selectors")
    {
        let mut inner = selector.walk();
        for entry in selector.named_children(&mut inner) {
            match entry.kind() {
                // `import a.b.{Painter => Renamed}` binds `Renamed`.
                "arrow_renamed_identifier" | "as_renamed_identifier" => {
                    if let Some(alias) = entry
                        .child_by_field_name("alias")
                        .and_then(|alias| text(&alias, source))
                    {
                        names.push(alias);
                    }
                }
                // `import a.b.C` and `import a.b.*`: a wildcard binds no name.
                "identifier" | "operator_identifier" => {
                    if let Some(name) = text(&entry, source) {
                        names.push(name);
                    }
                }
                _ => {}
            }
        }
    }
    if !names.is_empty() {
        return names;
    }
    let mut cursor = node.walk();
    let paths: Vec<String> = node
        .children_by_field_name("path", &mut cursor)
        .filter_map(|path| text(&path, source))
        .collect();
    paths.last().cloned().into_iter().collect()
}

/// The name a call expression targets: `parseConfig`, `load` of `store.load`,
/// the applied type of `DiagramStore(…)`.
fn called(node: &Node, source: &[u8]) -> Option<String> {
    let target = node.child_by_field_name("function")?;
    match target.kind() {
        "identifier" | "operator_identifier" => text(&target, source),
        // `store.load(input)`: the field is the member the source calls.
        "field_expression" => target
            .child_by_field_name("field")
            .and_then(|field| text(&field, source)),
        // `parse[T](input)`: the generic wrapper names the function it applies.
        "generic_function" => called(&target, source),
        _ => None,
    }
}

/// The type a `new` expression instantiates.
fn instantiated(node: &Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    let types: Vec<Node> = node
        .named_children(&mut cursor)
        .filter(|child| !matches!(child.kind(), "arguments" | "early_defs" | "template_body"))
        .collect();
    let first = types.first()?;
    type_names(first, source).into_iter().next()
}

/// The names the types of one field spell: the parser allows the field to occur
/// more than once (`extends Base with Loader` is one clause with two types).
fn types_of_field(node: &Node, field: &str, source: &[u8]) -> Vec<String> {
    let mut cursor = node.walk();
    let mut names = Vec::new();
    for child in node.children_by_field_name(field, &mut cursor) {
        for name in type_names(&child, source) {
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names
}

/// Every name a type expression spells: `List[Renderer]` is `List` and
/// `Renderer`, and a path type names its last segment.
fn type_names(node: &Node, source: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    type_names_into(node, source, &mut names);
    names.dedup();
    names
}

/// Collects the names of one type expression.
fn type_names_into(node: &Node, source: &[u8], out: &mut Vec<String>) {
    match node.kind() {
        "type_identifier" => {
            if let Some(name) = text(node, source) {
                out.push(name);
            }
        }
        // `scala.collection.mutable.Buffer` names its last segment: the book
        // stores identifiers, never a path.
        "stable_type_identifier" | "stable_identifier" => {
            let mut cursor = node.walk();
            let segments: Vec<String> = node
                .named_children(&mut cursor)
                .filter(|child| matches!(child.kind(), "identifier" | "type_identifier"))
                .filter_map(|child| text(&child, source))
                .collect();
            if let Some(last) = segments.last() {
                out.push(last.clone());
            }
        }
        // A refinement, an annotation and a singleton type name no declaration
        // of the book: `x.type` is the type of a value, `@unchecked` is an
        // annotation.
        "annotation" | "wildcard" | "singleton_type" | "literal_type" => {}
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
    use crate::lang::registry::SCALA;

    /// The `package` of the file names its chapter, an `object` is a class with
    /// its members under it, a `case class` a class, a `trait` a trait and an
    /// `enum` a type.
    #[test]
    fn the_package_of_a_scala_file_names_its_chapter() {
        let source = r#"
package graphium.store

trait Loader {
  def load(input: String): Int
}

case class Limits(max: Int) {
  def doubled: Int = max * 2
}

enum Mode {
  case Fast, Slow
}

object Defaults {
  val limit: Int = 3

  def open(renderer: Renderer): DiagramStore = new DiagramStore(renderer)
}
"#;
        let parsed = SCALA
            .parse("src/store.scala", source.as_bytes(), 0)
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
            ("method", "doubled", "graphium.store.Limits.doubled"),
            ("type", "Mode", "graphium.store.Mode"),
            ("class", "Defaults", "graphium.store.Defaults"),
            ("method", "open", "graphium.store.Defaults.open"),
        ] {
            assert!(rows.contains(&expected), "missing {expected:?}: {rows:?}");
        }
        // A field of a class or an object is not a declaration, and neither is
        // an enum case: the vocabulary has no field.
        for hidden in ["max", "limit", "Fast", "Slow"] {
            assert!(
                !parsed.symbols.iter().any(|symbol| symbol.name == hidden),
                "`{hidden}` is not a declaration: {rows:?}"
            );
        }
    }

    /// A companion class and its `object` declare the same name: the chapter
    /// keeps both (an `object` is a class, and the compiler emits one), and the
    /// book numbers the second `qname` the way it numbers two `impl` blocks of
    /// one type.
    #[test]
    fn a_companion_object_and_its_class_are_two_declarations_of_one_name() {
        let source = "package store\n\nclass DiagramStore\n\nobject DiagramStore {\n  def open(): Int = 0\n}\n";
        let parsed = SCALA
            .parse("src/store.scala", source.as_bytes(), 0)
            .expect("parses");
        let stores: Vec<(&str, &str)> = parsed
            .symbols
            .iter()
            .filter(|symbol| symbol.name == "DiagramStore")
            .map(|symbol| (symbol.kind, symbol.qname.as_str()))
            .collect();
        assert_eq!(
            stores,
            [
                ("class", "store.DiagramStore"),
                ("class", "store.DiagramStore"),
            ],
            "the companion is the second declaration of the name"
        );
        assert!(
            parsed
                .symbols
                .iter()
                .any(|symbol| symbol.qname == "store.DiagramStore.open"),
            "{:?}",
            parsed.symbols
        );
    }

    /// A file with no `package` keeps its directory, and a `val` at the root of
    /// the file is a `const`.
    #[test]
    fn a_scala_file_without_a_package_keeps_its_directory() {
        let source = "val LIMIT: Int = 3\n\ndef parseConfig(input: String): Int = input.length\n";
        let parsed = SCALA
            .parse("src/graphium/store/One.scala", source.as_bytes(), 0)
            .expect("parses");
        let rows: Vec<(&str, &str, &str)> = parsed
            .symbols
            .iter()
            .map(|symbol| (symbol.kind, symbol.name.as_str(), symbol.qname.as_str()))
            .collect();
        assert!(
            rows.contains(&("const", "LIMIT", "src.graphium.store.LIMIT")),
            "{rows:?}"
        );
        assert!(
            rows.contains(&("fn", "parseConfig", "src.graphium.store.parseConfig")),
            "{rows:?}"
        );
    }
}
