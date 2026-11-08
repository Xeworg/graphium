//! The Lua chapter of the book (T3 Batch 4; S2, S4).
//!
//! **The Lua decision: a table is the module, and a function is a symbol.**
//! Lua has one compound value — the table — and everything the other languages
//! spread over classes, modules, records and namespaces is a table here. The
//! chapter reads that honestly:
//!
//! - **a table at the root of the file is a `module`.** `local Store = {}` is
//!   the canonical Lua module: a table whose fields are its functions and whose
//!   value is what the file `return`s. The book's `module` kind is exactly that
//!   namespace, so the table is the symbol the functions of the file nest under,
//!   and its members are reached by the `qname` the source spells
//!   (`src.store.open` for `function Store.open`).
//! - **a function is a `fn`, and a colon method a `method`.** `function
//!   Store.open(path)` declares a function in the table → `fn open` under
//!   `Store`; `function Store:load(input)` takes the implicit `self` the
//!   language passes → `method load` under `Store` (the one thing Lua calls a
//!   method). `local function parse_config(input)` is a `fn` of the file.
//! - **the other root bindings are `const`s.** `local LIMIT = 3` and
//!   `local render = require("render")` bind a name of the module and are
//!   `const`, whatever the value is (Python's rule for a module-level
//!   assignment).
//!
//! **The two spellings of a function are read as one.** `function M.load() … end`
//! and `M.load = function() … end` are the same declaration in Lua — the first is
//! sugar for the second — so both are read as `fn load` under `M`. A field that
//! is only data (`M.count = 1`, `local M = { name = "x" }`) is not a declaration:
//! a field of the language is not one, the rule every chapter follows for a
//! field. A table with no declaration in the file (`function Store.open()` with
//! no `local Store = {}` anywhere) still gets its `module` symbol, created at the
//! table the function names, so the nesting is never lost.
//!
//! The relations of this grammar (D3):
//!
//! - `calls` — every `function_call`: `parse_config(input)`, `Store.load(input)`
//!   and `self.load(input)` are calls to `parse_config`, `load` and `load`;
//! - `imports` — `require("render")` by the stem of the path it names
//!   (`require("lib.render")` → `render`), which is how a Lua file composes
//!   another one.
//!
//! Lua has **no `inherits` and no `uses`**: the language has no types and no
//! supertypes, and its module pattern (`setmetatable`) is a call. The fixture
//! measures two relation kinds, not four.
//!
//! What is Lua's alone is here; the declarations the book keeps are the tables
//! of [`crate::lang::registry::LUA`] and the pipeline is
//! [`crate::lang::engine`] (S3, T2).

use tree_sitter::Node;

use crate::lang::emit::{dotted_module, emit_named, file_stem, text};
use crate::lang::engine::{Body, Nesting};
use crate::lang::model::ParsedSymbol;
use crate::relations::{push_edges, RawEdge, CALLS, IMPORTS};

/// The language of the chapter, as the table `file` spells it.
pub const LANGUAGE: &str = "lua";

/// The tree-sitter grammar of Lua.
pub fn language(_path: &str) -> tree_sitter::Language {
    tree_sitter::Language::new(tree_sitter_lua::LANGUAGE)
}

/// The module path of a Lua file: its path, because Lua declares no module
/// (`src/store.lua` → `src.store`). `init.lua` names its directory, which is the
/// convention of the language (`src/store/init.lua` → `src.store`).
pub fn module_path(path: &str) -> String {
    dotted_module(path, &["init"])
}

/// An assignment: the names it binds and, when the value is a function bound to
/// a field of a table, the function it declares.
///
/// Both statement forms reach here — `x = 1` at the root of the file and `local
/// x = 1` inside a `variable_declaration` — so the two write the same symbols.
pub(crate) fn assignment(
    node: &Node,
    nesting: &Nesting,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) -> Option<Body> {
    let names = match child_of_kind(node, "variable_list") {
        Some(list) => {
            let mut cursor = list.walk();
            let names: Vec<Node> = list.children_by_field_name("name", &mut cursor).collect();
            names
        }
        None => Vec::new(),
    };
    let values = match child_of_kind(node, "expression_list") {
        Some(list) => {
            let mut cursor = list.walk();
            let values: Vec<Node> = list.children_by_field_name("value", &mut cursor).collect();
            values
        }
        None => Vec::new(),
    };
    for (position, name) in names.iter().enumerate() {
        let value = values.get(position).copied();
        match name.kind() {
            // `local Store = {}` is the module table; any other binding is a
            // `const` of the chapter (and a function value a `fn`).
            "identifier" => {
                let Some(binding) = text(name, source) else {
                    continue;
                };
                let kind = match value.map(|value| value.kind()) {
                    Some("table_constructor") => "module",
                    Some("function_definition") => "fn",
                    _ => "const",
                };
                if kind == "module" {
                    // The `module` symbol of the table, which its functions
                    // nest under: created here, or reused when a function of
                    // the file named that table before this line.
                    let table = TableName {
                        node: *name,
                        name: binding,
                    };
                    let _ = table_symbol(out, nesting, &table);
                } else {
                    emit_named(node, kind, &binding, nesting.prefix, nesting.parent, out);
                }
            }
            // `Store.load = function(…) end`: the second spelling of
            // `function Store.load(…)`.
            "dot_index_expression" | "method_index_expression" => {
                if value.is_none_or(|value| value.kind() != "function_definition") {
                    continue;
                }
                let kind = if name.kind() == "method_index_expression" {
                    "method"
                } else {
                    "fn"
                };
                if let Some((table, field)) = table_and_field(name, source) {
                    declare_in_table(out, nesting, &table, &field, kind, node);
                }
            }
            _ => {}
        }
    }
    None
}

/// A `function` declaration: a function of the file, of a table (`Store.open`)
/// or a method of a table (`Store:load`, the implicit `self`).
pub(crate) fn function(
    node: &Node,
    nesting: &Nesting,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) -> Option<Body> {
    let name = node.child_by_field_name("name")?;
    match name.kind() {
        "identifier" => {
            if let Some(text) = text(&name, source) {
                emit_named(node, "fn", &text, nesting.prefix, nesting.parent, out);
            }
        }
        "dot_index_expression" | "method_index_expression" => {
            let kind = if name.kind() == "method_index_expression" {
                "method"
            } else {
                "fn"
            };
            if let Some((table, field)) = table_and_field(&name, source) {
                declare_in_table(out, nesting, &table, &field, kind, node);
            }
        }
        // `function a.b.c.open()` names a table the book does not model: the
        // function stays out instead of inventing two levels of nesting.
        _ => {}
    }
    None
}

/// Writes `kind name` under the module symbol of `table`, creating that symbol
/// when the file never declared the table.
fn declare_in_table(
    out: &mut Vec<ParsedSymbol>,
    nesting: &Nesting,
    table: &TableName<'_>,
    field: &str,
    kind: &'static str,
    node: &Node,
) {
    let Some(index) = table_symbol(out, nesting, table) else {
        return;
    };
    let prefix = format!("{}{}.", nesting.prefix, table.name);
    emit_named(node, kind, field, &prefix, Some(index), out);
}

/// The `module` symbol of a table: the one the file declared, or one created at
/// the table the function names.
fn table_symbol(
    out: &mut Vec<ParsedSymbol>,
    nesting: &Nesting,
    table: &TableName<'_>,
) -> Option<usize> {
    let qname = format!("{}{}", nesting.prefix, table.name);
    if let Some(index) = out
        .iter()
        .position(|symbol| symbol.qname == qname && symbol.kind == "module")
    {
        return Some(index);
    }
    emit_named(
        &table.node,
        "module",
        &table.name,
        nesting.prefix,
        nesting.parent,
        out,
    )
}

/// The table and the field an index expression names.
fn table_and_field<'a>(node: &'a Node, source: &[u8]) -> Option<(TableName<'a>, String)> {
    let table = node.child_by_field_name("table")?;
    let name = text(&table, source)?;
    if table.kind() != "identifier" {
        return None;
    }
    let field = node
        .child_by_field_name("field")
        .or_else(|| node.child_by_field_name("method"))?;
    let field = text(&field, source)?;
    Some((TableName { node: table, name }, field))
}

/// A table named by the source, with the span of the name (the module symbol
/// the chapter writes for it).
struct TableName<'a> {
    node: Node<'a>,
    name: String,
}

/// The first named child of a kind.
fn child_of_kind<'a>(node: &'a Node, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let children: Vec<Node> = node
        .named_children(&mut cursor)
        .filter(|child| child.kind() == kind)
        .collect();
    children.first().copied()
}

/// The relations of one Lua chapter, as the grammar sees them.
pub(crate) fn relations(root: &Node, source: &[u8]) -> Vec<RawEdge> {
    let mut out = Vec::new();
    visit(root, source, &mut out);
    out
}

/// Walks the tree once and records every relation it recognises.
fn visit(node: &Node, source: &[u8], out: &mut Vec<RawEdge>) {
    let byte = node.start_byte();
    if node.kind() == "function_call" {
        if let Some(callee) = node.child_by_field_name("name") {
            if let Some(name) = called(&callee, source) {
                if name == "require" {
                    if let Some(import) = imported(node, source) {
                        push_edges(out, byte, IMPORTS, vec![import]);
                    }
                }
                push_edges(out, byte, CALLS, vec![name]);
            }
        }
    }

    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    for child in children {
        visit(&child, source, out);
    }
}

/// The name a call targets: `parse_config`, the field of `Store.load`, the
/// method of `self:load`.
fn called(node: &Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "identifier" => text(node, source),
        "dot_index_expression" => node
            .child_by_field_name("field")
            .and_then(|field| text(&field, source)),
        "method_index_expression" => node
            .child_by_field_name("method")
            .and_then(|method| text(&method, source)),
        _ => None,
    }
}

/// The module a `require("lib.render")` names: the stem of the path it writes.
fn imported(node: &Node, source: &[u8]) -> Option<String> {
    let arguments = node.child_by_field_name("arguments")?;
    let mut cursor = arguments.walk();
    let first = arguments.named_children(&mut cursor).next()?;
    if first.kind() != "string" {
        return None;
    }
    let mut inner = first.walk();
    let parts: Vec<String> = first
        .named_children(&mut inner)
        .filter(|child| child.kind() == "string_content")
        .filter_map(|child| text(&child, source))
        .collect();
    let path = parts.concat();
    if path.is_empty() {
        return None;
    }
    // `require("lib.render")` names `lib/render` in the file system idiom of the
    // language; the book stores the module, which is its last segment.
    let last = path.rsplit('.').find(|part| !part.is_empty())?;
    file_stem(last).or_else(|| Some(last.to_string()))
}

#[cfg(test)]
mod tests {
    use crate::lang::registry::LUA;

    /// A table at the root is the module, the functions it declares nest under
    /// it, and the other bindings are `const`s.
    #[test]
    fn a_lua_table_is_the_module_of_its_functions() {
        let source = r#"
local Store = {}

local LIMIT = 3

function Store.open(path)
  return Store.load(path)
end

function Store:render(input)
  return self.load(input)
end

local function parse_config(input)
  return input
end
"#;
        let parsed = LUA
            .parse("src/store.lua", source.as_bytes(), 0)
            .expect("parses");
        let rows: Vec<(&str, &str, &str)> = parsed
            .symbols
            .iter()
            .map(|symbol| (symbol.kind, symbol.name.as_str(), symbol.qname.as_str()))
            .collect();
        for expected in [
            ("module", "Store", "src.store.Store"),
            ("const", "LIMIT", "src.store.LIMIT"),
            ("fn", "open", "src.store.Store.open"),
            ("method", "render", "src.store.Store.render"),
            ("fn", "parse_config", "src.store.parse_config"),
        ] {
            assert!(rows.contains(&expected), "missing {expected:?}: {rows:?}");
        }
        // The members of the table hang from the table, never from the file.
        let open = parsed
            .symbols
            .iter()
            .find(|symbol| symbol.qname == "src.store.Store.open")
            .expect("the fn");
        let parent = open.parent.expect("the fn hangs from its table");
        assert_eq!(parsed.symbols[parent].kind, "module");
        assert_eq!(parsed.symbols[parent].name, "Store");
        // A local of a body is not a declaration of the chapter.
        assert!(
            !parsed.symbols.iter().any(|symbol| symbol.name == "input"),
            "a parameter is not a declaration: {rows:?}"
        );
    }

    /// A table named by a function before the file declares it gets the same
    /// `module` symbol once, never two.
    #[test]
    fn a_table_declared_after_its_first_function_is_one_module() {
        let source = "function Store.open() return 0 end\n\nlocal Store = {}\n";
        let parsed = LUA
            .parse("src/store.lua", source.as_bytes(), 0)
            .expect("parses");
        let stores: Vec<&str> = parsed
            .symbols
            .iter()
            .filter(|symbol| symbol.name == "Store")
            .map(|symbol| symbol.kind)
            .collect();
        assert_eq!(stores, ["module"], "{:?}", parsed.symbols);
    }

    /// The assignment spelling of a function declares the same symbol, and a
    /// table with no declaration of its own still gets its module symbol.
    #[test]
    fn the_two_spellings_of_a_lua_function_declare_the_same_symbol() {
        let source = "Store.load = function(input) return input end\n";
        let parsed = LUA
            .parse("src/store.lua", source.as_bytes(), 0)
            .expect("parses");
        let rows: Vec<(&str, &str, &str)> = parsed
            .symbols
            .iter()
            .map(|symbol| (symbol.kind, symbol.name.as_str(), symbol.qname.as_str()))
            .collect();
        assert!(
            rows.contains(&("module", "Store", "src.store.Store")),
            "{rows:?}"
        );
        assert!(
            rows.contains(&("fn", "load", "src.store.Store.load")),
            "{rows:?}"
        );
    }
}
