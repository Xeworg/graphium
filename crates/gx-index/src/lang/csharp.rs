//! The C# chapter of the book (T3 Batch 1; S2, S4).
//!
//! The chapter keeps the types of a file — the classes (a `record` is a class),
//! the interfaces, the structs, the enums and the delegates — with their
//! constructors and their methods as children, nested under the namespace the
//! file declares, which is what names it in the language:
//! `Graphium.Store.DiagramStore.Load`. A file with no namespace keeps its
//! directory, the way a Java file with no `package` does.
//!
//! The relations of this grammar are the four kinds of the schema, read
//! syntactically (D3):
//!
//! - `calls` — `OpenStore()`, `store.Load()`, `new DiagramStore()`;
//! - `imports` — the last segment of a `using` (`System.Collections.Generic` →
//!   `Generic`);
//! - `inherits` — the base class and every implemented interface;
//! - `uses` — every type of a declaration: a parameter, a return, a field, a
//!   property, a cast, a generic argument.
//!
//! A `using System.Collections.Generic` names a namespace, and the book keeps
//! its last segment for the same reason Java keeps the last segment of an
//! import: it is the name the source uses. A field or a property is not a
//! declaration of the book (Java and TypeScript keep theirs the same way), and
//! a method body is not walked.
//!
//! What is C#'s alone is here; the declarations the book keeps are the tables
//! of [`crate::lang::registry::CSHARP`] and the pipeline is
//! [`crate::lang::engine`] (S3, T2). C# needs no hook at all: the namespace is
//! where the module path of the chapter comes from, and the pipeline asks for
//! it.

use crate::lang::emit::{dotted_directories, last_segment, text};
use crate::relations::{push_edges, RawEdge, CALLS, IMPORTS, INHERITS, USES};

/// The language of the chapter, as the table `file` spells it.
pub const LANGUAGE: &str = "csharp";

/// The tree-sitter grammar of C#: one grammar answers every C# path.
pub fn language(_path: &str) -> tree_sitter::Language {
    tree_sitter::Language::new(tree_sitter_c_sharp::LANGUAGE)
}

/// The module path of a C# file: the namespace it declares, or its directory
/// when the file declares none. The first namespace of the file wins: a file
/// with two of them is legal C# and rare, and the book names the chapter once.
pub fn module_path(root: &tree_sitter::Node, path: &str, source: &[u8]) -> String {
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if !matches!(
            child.kind(),
            "namespace_declaration" | "file_scoped_namespace_declaration"
        ) {
            continue;
        }
        if let Some(name) = child
            .child_by_field_name("name")
            .and_then(|name| text(&name, source))
            .filter(|name| !name.is_empty())
        {
            return name;
        }
    }
    dotted_directories(path)
}

/// The relations of one C# chapter, as the grammar sees them.
pub(crate) fn relations(root: &tree_sitter::Node, source: &[u8]) -> Vec<RawEdge> {
    let mut out = Vec::new();
    visit(root, source, &mut out);
    out
}

/// Walks the tree once and records every relation it recognises.
fn visit(node: &tree_sitter::Node, source: &[u8], out: &mut Vec<RawEdge>) {
    let byte = node.start_byte();
    match node.kind() {
        "using_directive" => {
            let target = node
                .child_by_field_name("name")
                .or_else(|| node.named_child(0));
            if let Some(name) = target.and_then(|target| last_name(&target, source)) {
                push_edges(out, byte, IMPORTS, vec![name]);
            }
        }
        "invocation_expression" => {
            if let Some(name) = node
                .child_by_field_name("function")
                .and_then(|function| called(&function, source))
            {
                out.push(RawEdge {
                    byte,
                    kind: CALLS,
                    name,
                });
            }
        }
        // `new DiagramStore(…)`: a call to the constructor by the name written.
        "object_creation_expression" => {
            if let Some(name) = node
                .child_by_field_name("type")
                .and_then(|constructed| last_name(&constructed, source))
            {
                out.push(RawEdge {
                    byte,
                    kind: CALLS,
                    name,
                });
            }
        }
        // `class DiagramStore : Base, ILoader`: the base class and every
        // implemented interface.
        "base_list" => {
            let mut cursor = node.walk();
            let children: Vec<tree_sitter::Node> = node.named_children(&mut cursor).collect();
            for child in children {
                if let Some(name) = last_name(&child, source) {
                    out.push(RawEdge {
                        byte,
                        kind: INHERITS,
                        name,
                    });
                }
            }
        }
        // The type references of the declarations, read from the fields the
        // grammar types as a type.
        "variable_declaration" | "parameter" | "property_declaration" | "cast_expression" => {
            if let Some(declared) = node.child_by_field_name("type") {
                push_edges(out, byte, USES, type_names(&declared, source));
            }
        }
        "method_declaration" => {
            if let Some(returned) = node.child_by_field_name("returns") {
                push_edges(out, byte, USES, type_names(&returned, source));
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    let children: Vec<tree_sitter::Node> = node.named_children(&mut cursor).collect();
    for child in children {
        visit(&child, source, out);
    }
}

/// The name a call expression targets, when it has one.
fn called(function: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    match function.kind() {
        "identifier" => text(function, source),
        // `store.Load()`: the method name.
        "member_access_expression" => function
            .child_by_field_name("name")
            .and_then(|name| text(&name, source)),
        // `OpenStore<int>()`: the name of the generic method.
        "generic_name" => function
            .named_child(0)
            .and_then(|name| called(&name, source)),
        _ => None,
    }
}

/// The last name of a type spelled as a path: `Graphium.Store.DiagramStore` →
/// `DiagramStore`, `List<ILoader>` → `List`, `int` → nothing.
fn last_name(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "identifier" => text(node, source),
        "qualified_name" => node
            .child_by_field_name("name")
            .and_then(|name| last_name(&name, source)),
        "generic_name" => node
            .named_child(0)
            .and_then(|name| last_name(&name, source)),
        "nullable_type" | "array_type" | "pointer_type" | "ref_type" | "scoped_type" => node
            .child_by_field_name("type")
            .and_then(|inner| last_name(&inner, source)),
        "predefined_type" => None,
        _ => node
            .named_child(0)
            .and_then(|inner| last_name(&inner, source)),
    }
}

/// Every type name a type expression spells, generics and arrays included:
/// `List<ILoader>` names `List` and `ILoader`.
fn type_names(node: &tree_sitter::Node, source: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    type_names_into(node, source, &mut names);
    names.dedup();
    names
}

/// Collects the type names of one type expression.
fn type_names_into(node: &tree_sitter::Node, source: &[u8], out: &mut Vec<String>) {
    match node.kind() {
        "identifier" => {
            if let Some(name) = text(node, source) {
                // The last segment of an imported path is the name the book
                // stores for a Java import; a C# qualifier is a namespace and
                // never a declaration, so only the name is kept.
                out.push(last_segment(&name));
            }
        }
        "qualified_name" => {
            if let Some(name) = node.child_by_field_name("name") {
                type_names_into(&name, source, out);
            }
        }
        "generic_name" => {
            if let Some(base) = node.named_child(0) {
                type_names_into(&base, source, out);
            }
            let mut cursor = node.walk();
            let children: Vec<tree_sitter::Node> = node.named_children(&mut cursor).collect();
            for child in children {
                if child.kind() == "type_argument_list" {
                    type_names_into(&child, source, out);
                }
            }
        }
        "type_argument_list" | "tuple_type" => {
            let mut cursor = node.walk();
            let children: Vec<tree_sitter::Node> = node.named_children(&mut cursor).collect();
            for child in children {
                type_names_into(&child, source, out);
            }
        }
        "tuple_element" => {
            if let Some(declared) = node.child_by_field_name("type") {
                type_names_into(&declared, source, out);
            }
        }
        "nullable_type" | "array_type" | "pointer_type" | "ref_type" | "scoped_type" => {
            if let Some(inner) = node.child_by_field_name("type") {
                type_names_into(&inner, source, out);
            }
        }
        // `int`, `string`, `void`… has no declaration in the book (D3).
        "predefined_type" => {}
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use crate::lang::registry::CSHARP;

    /// The prefix of every `qname` is the namespace of the file, and a file
    /// without one keeps its directory.
    #[test]
    fn the_namespace_of_a_csharp_file_names_its_chapter() {
        let block = "namespace Graphium.Store\n{\n    class One {}\n}\n";
        let parsed = CSHARP
            .parse("src/One.cs", block.as_bytes(), 0)
            .expect("parses");
        assert!(
            parsed
                .symbols
                .iter()
                .any(|symbol| symbol.qname == "Graphium.Store.One"),
            "{:?}",
            parsed.symbols
        );

        let scoped = "namespace Graphium.App;\n\nclass Two {}\n";
        let parsed = CSHARP
            .parse("src/Two.cs", scoped.as_bytes(), 0)
            .expect("parses");
        assert!(
            parsed
                .symbols
                .iter()
                .any(|symbol| symbol.qname == "Graphium.App.Two"),
            "{:?}",
            parsed.symbols
        );

        let loose = "class Three {}\n";
        let parsed = CSHARP
            .parse("src/loose/Three.cs", loose.as_bytes(), 0)
            .expect("parses");
        assert_eq!(parsed.symbols[0].qname, "src.loose.Three");
    }

    /// A record is a class of the vocabulary, a struct is a type and an
    /// interface a trait, the same way Java names its own.
    #[test]
    fn the_types_of_a_csharp_file_map_to_the_vocabulary_of_the_book() {
        let source = r#"
namespace Graphium.Store;

public interface ILoader { int Load(string input); }

public struct Limits { public int Max; }

public class DiagramStore : ILoader
{
    public int Load(string input) { return input.Length; }
}

public record Pair(string Left, string Right);

public enum Mode { Fast }

public delegate int Handler(string input);
"#;
        let parsed = CSHARP
            .parse("src/Store.cs", source.as_bytes(), 0)
            .expect("parses");
        let kinds: Vec<(&str, &str)> = parsed
            .symbols
            .iter()
            .map(|symbol| (symbol.kind, symbol.name.as_str()))
            .collect();
        for expected in [
            ("trait", "ILoader"),
            ("method", "Load"),
            ("type", "Limits"),
            ("class", "DiagramStore"),
            ("class", "Pair"),
            ("type", "Mode"),
            ("type", "Handler"),
        ] {
            assert!(kinds.contains(&expected), "missing {expected:?}: {kinds:?}");
        }
    }
}
