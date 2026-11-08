//! One way for a grammar to write a declaration of the book (S3, L5).
//!
//! A chapter is a path, a language, a hash and two lists ([`super::model`]);
//! what a grammar adds to that is the shape of the tree it walks, never a
//! second way to build a row of the table `symbol`. This module is that one
//! way: the span, the derived search text and the folded collation of a
//! declaration are built here, so the five grammars of the book cannot drift
//! apart while each keeps its own idiom of qualification.
//!
//! The `prefix` these helpers take is the **nesting of the `qname`, separator
//! included**: `""` at the root, `"gx_mcp::"` for the Rust module of a file,
//! `"src.tools.ToolBox."` for the methods of a TypeScript class.

use super::model::{name_norm, search_text_for, ParsedSymbol};

/// Emits a symbol whose name comes from the node's `name` field.
pub fn emit(
    node: &tree_sitter::Node,
    kind: &'static str,
    prefix: &str,
    parent: Option<usize>,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) -> Option<usize> {
    let name = node
        .child_by_field_name("name")
        .and_then(|name| name.utf8_text(source).ok())
        .map(str::trim)
        .filter(|name| !name.is_empty())?
        .to_string();
    emit_named(node, kind, &name, prefix, parent, out)
}

/// Emits a symbol with an explicit name.
pub fn emit_named(
    node: &tree_sitter::Node,
    kind: &'static str,
    name: &str,
    prefix: &str,
    parent: Option<usize>,
    out: &mut Vec<ParsedSymbol>,
) -> Option<usize> {
    let start = node.start_position();
    let end = node.end_position();
    let qname = format!("{prefix}{name}");
    let index = out.len();
    out.push(ParsedSymbol {
        parent,
        kind,
        name: name.to_string(),
        qname,
        search_text: search_text_for(name),
        name_norm: name_norm(name),
        start_line: (start.row as u32).saturating_add(1),
        start_col: (start.column as u32).saturating_add(1),
        end_line: (end.row as u32).saturating_add(1),
        end_col: end.column as u32,
        start_byte: node.start_byte() as u32,
        end_byte: node.end_byte() as u32,
    });
    Some(index)
}

/// The synthetic `module` symbol that anchors the relations of a file: the
/// `qname` is the module path, `name` is the one the language uses to refer to
/// that module, and the span is the top of the file, so it can never win an
/// ownership question against a declaration.
pub fn anchor_symbol(module: &str, name: &str) -> ParsedSymbol {
    let qname = if module.is_empty() {
        name.to_string()
    } else {
        module.to_string()
    };
    ParsedSymbol {
        parent: None,
        kind: "module",
        search_text: search_text_for(name),
        name_norm: name_norm(name),
        name: name.to_string(),
        qname,
        start_line: 1,
        start_col: 1,
        end_line: 1,
        end_col: 1,
        start_byte: 0,
        end_byte: 0,
    }
}

/// The name a language gives to the anchor of a chapter when it names a module
/// by its last segment: `src.tools` → `tools`, `internal.store` → `store`.
/// PHP names a namespace with `\` (`Graphium\Store`), so its anchor is the last
/// segment of that path too; no other language of the book spells a module path
/// with a backslash, so the split cannot change theirs.
pub fn chapter_anchor_name(path: &str, module: &str) -> String {
    module
        .rsplit(['.', ':', '\\'])
        .next()
        .filter(|segment| !segment.is_empty())
        .map(str::to_string)
        .or_else(|| file_stem(path))
        .unwrap_or_else(|| path.to_string())
}

/// The stem of a path, used when a chapter has no module path at all.
pub fn file_stem(path: &str) -> Option<String> {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::to_string)
}

/// The `qname` prefix of a module path: empty at the root, `path.` otherwise.
pub fn prefix_of(module: &str, separator: &str) -> String {
    if module.is_empty() {
        String::new()
    } else {
        format!("{module}{separator}")
    }
}

/// The trimmed text of a node, when it has one.
pub fn text(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    node.utf8_text(source)
        .ok()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// The dotted module path of a file: its directories and its stem, with the
/// stems that name their directory skipped (`index` in TypeScript, `__init__`
/// in Python).
pub fn dotted_module(relative: &str, skip: &[&str]) -> String {
    let parts: Vec<&str> = relative
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    let Some((file, directories)) = parts.split_last() else {
        return String::new();
    };
    let stem = file.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(file);
    let mut segments: Vec<String> = directories
        .iter()
        .map(|directory| identifier(directory))
        .collect();
    if !skip.contains(&stem) {
        segments.push(identifier(stem));
    }
    join_segments(segments)
}

/// The dotted module path of a file whose unit is the directory (Go packages,
/// the fallback of a Java, C# or PHP file with no declaration that names it).
#[cfg(any(
    feature = "lang-go",
    feature = "lang-java",
    feature = "lang-csharp",
    feature = "lang-php"
))]
pub fn dotted_directories(relative: &str) -> String {
    let parts: Vec<&str> = relative
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    let directories: Vec<String> = parts
        .iter()
        .take(parts.len().saturating_sub(1))
        .map(|directory| identifier(directory))
        .collect();
    join_segments(directories)
}

/// The name a C/C++ declarator spells, through the wrappers the grammar puts
/// around it: `int *open_store(char *path)` names `open_store` and
/// `int DiagramStore::load()` names `load`.
///
/// It lives here and not in the two language modules because C++ is a superset
/// of the declarator grammar of C and the two features are independent: the one
/// implementation is compiled when either of them is on.
#[cfg(any(feature = "lang-c", feature = "lang-cpp"))]
pub fn declarator_name(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "identifier" | "field_identifier" | "type_identifier" => text(node, source),
        // A destructor or an operator declares its own spelling: `~Store`,
        // `operator+`.
        "destructor_name" | "operator_name" => text(node, source),
        "qualified_identifier" | "template_function" | "template_type" => node
            .child_by_field_name("name")
            .and_then(|name| declarator_name(&name, source)),
        // Wrappers without a `declarator` field: `(open_store)` and `&value`.
        "parenthesized_declarator" | "reference_declarator" => node
            .named_child(0)
            .and_then(|inner| declarator_name(&inner, source)),
        // `pointer_declarator`, `init_declarator`, `array_declarator`,
        // `function_declarator`… all spell the wrapped declarator with a field.
        _ => node
            .child_by_field_name("declarator")
            .and_then(|inner| declarator_name(&inner, source)),
    }
}

/// Whether a node of a C/C++ tree has a descendant of that kind: it is how a
/// `declaration` tells a function prototype from a variable (`function_declarator`
/// is the difference).
#[cfg(any(feature = "lang-c", feature = "lang-cpp"))]
pub fn contains_kind(node: &tree_sitter::Node, kind: &str) -> bool {
    if node.kind() == kind {
        return true;
    }
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .any(|child| contains_kind(&child, kind));
    found
}

/// The last segment of a path spelled with `.` or `::`: the name a language
/// uses to refer to something declared elsewhere.
#[cfg(any(
    feature = "lang-typescript",
    feature = "lang-java",
    feature = "lang-python",
    feature = "lang-csharp"
))]
pub fn last_segment(text: &str) -> String {
    text.trim()
        .rsplit("::")
        .next()
        .unwrap_or(text)
        .rsplit('.')
        .next()
        .unwrap_or(text)
        .trim()
        .to_string()
}

/// Keeps the identifier characters of a path segment, folding the rest to `_`.
fn identifier(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for char in text.chars() {
        if char.is_alphanumeric() {
            out.push(char);
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

/// Joins the segments of a module path, dropping the empty ones.
fn join_segments(segments: Vec<String>) -> String {
    segments
        .into_iter()
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join(".")
}

#[cfg(test)]
mod tests {
    #[cfg(any(
        feature = "lang-typescript",
        feature = "lang-java",
        feature = "lang-python",
        feature = "lang-csharp"
    ))]
    use super::last_segment;
    use super::{chapter_anchor_name, file_stem};
    use super::{dotted_module, prefix_of};
    // `dotted_directories` only exists for the grammars whose unit is the
    // directory (Go, Java, C#, and the fallback of a PHP file with no
    // `namespace`): the slim build of T4/C3 does not compile it.
    #[cfg(any(
        feature = "lang-go",
        feature = "lang-java",
        feature = "lang-csharp",
        feature = "lang-php"
    ))]
    use super::dotted_directories;

    /// The nesting of a `qname` is the language's: the helpers keep the
    /// separator the grammar chose and drop the empty segments.
    #[test]
    fn the_nesting_of_a_qname_is_the_one_of_the_language() {
        assert_eq!(prefix_of("", "::"), "");
        assert_eq!(prefix_of("gx_mcp::tools", "::"), "gx_mcp::tools::");
        assert_eq!(dotted_module("src/tools.ts", &["index"]), "src.tools");
        assert_eq!(dotted_module("src/store/index.ts", &["index"]), "src.store");
        assert_eq!(
            dotted_module("src/pkg/__init__.py", &["__init__"]),
            "src.pkg"
        );
        // The anchor of a chapter is the last segment of its module path, split
        // with the separator of the language: PHP names a namespace with `\`.
        assert_eq!(chapter_anchor_name("src/tools.ts", "src.tools"), "tools");
        assert_eq!(chapter_anchor_name("x.rs", "gx_mcp::tools"), "tools");
        assert_eq!(
            chapter_anchor_name("src/store.php", "Graphium\\Store"),
            "Store"
        );
        assert_eq!(file_stem("scripts/build.sh"), Some("build".into()));
        #[cfg(any(
            feature = "lang-go",
            feature = "lang-java",
            feature = "lang-csharp",
            feature = "lang-php"
        ))]
        {
            assert_eq!(
                dotted_directories("internal/store/store.go"),
                "internal.store"
            );
            assert_eq!(dotted_directories("main.go"), "");
        }
        #[cfg(any(
            feature = "lang-typescript",
            feature = "lang-java",
            feature = "lang-python",
            feature = "lang-csharp"
        ))]
        {
            assert_eq!(last_segment("java.util.List"), "List");
            assert_eq!(last_segment("crate::store::Store"), "Store");
            // A Go package path is spelled with `/`: its grammar splits that
            // one.
            assert_eq!(last_segment("net/http"), "net/http");
        }
    }
}
