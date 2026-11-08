//! The shared core of the language engine (S3, T2): one pipeline for every
//! grammar of the book.
//!
//! Every grammar of `lang/` used to repeat, in its own `parse()`, the same
//! twenty lines: build a parser for its tree-sitter language, parse the bytes,
//! derive the module path of the file, walk the tree with a node-kind → kind
//! table, read the relations, attach them to the declarations and build the
//! chapter. That skeleton is here now, and a language contributes what is
//! really its own:
//!
//! - the tree-sitter language of a path ([`Grammar::language`]) — TypeScript
//!   answers `.tsx` with the TSX grammar and `.ts` with the TypeScript one;
//! - where the module path of a chapter comes from ([`ModuleSource`]) — the
//!   path of the file, or the tree, as Java's `package` does;
//! - how the synthetic `module` anchor of a chapter is named ([`Anchor`]);
//! - the node kinds the book keeps, the kinds that are transparent and the
//!   kinds whose body nests a `qname` ([`Rules`]) — or, when the scope of the
//!   language is not a tree of containers, the walk itself ([`Walk::Custom`]);
//! - the relations it reads ([`Grammar::relations`]).
//!
//! The genuinely imperative parts of a language stay in its own module and
//! reach the walk through [`Rules::hooks`]: Go's second pass over the
//! receivers, the name of a Rust `impl`, the declarators of a TypeScript
//! `const`, the constants of a Python module, the `package` path of Java and
//! the `called_name` chain of every `relations()`.
//!
//! Nothing here knows a language: the pipeline is written once and the data of
//! each grammar lives in [`crate::lang::registry`], so the node-kind tables of
//! the five languages sit side by side and a sixth language is one entry more.

use tree_sitter::{Language, Node};

use super::emit::{anchor_symbol, chapter_anchor_name, emit, file_stem, prefix_of};
use super::model::{content_hash, ExtractError, ParsedFile, ParsedSymbol};
use crate::relations::{attach, RawEdge};

/// One grammar of the book: the data the shared core needs to turn a file of
/// this language into a chapter.
#[derive(Clone, Copy)]
pub struct Grammar {
    /// The language as the table `file` spells it: `rust`, `typescript`…
    pub lang: &'static str,
    /// The extensions the grammar answers to, without the dot.
    pub extensions: &'static [&'static str],
    /// The tree-sitter grammar of a path: one module may answer two
    /// (`.ts` and `.tsx` share the language of the book).
    pub language: fn(&str) -> Language,
    /// Where the module path of a chapter comes from.
    pub module: ModuleSource,
    /// What a container puts between its `qname` and its children's: `::` in
    /// Rust, `.` in the four others.
    pub separator: &'static str,
    /// How the synthetic `module` anchor of the chapter is named.
    pub anchor: Anchor,
    /// The declarations the book keeps.
    pub walk: Walk,
    /// The names this language's **runtime and standard library** provide,
    /// folded the way [`crate::lang::name_norm`] folds a target (`str`, `len`,
    /// `path`, `fmt`, `string`). No chapter of a worktree can ever declare
    /// them, so the measure of D3 leaves a relation whose target is one of
    /// them out of the rate — and out of it **only while no chapter of the
    /// book declares that name**, so the rule can never hide a resolution
    /// (`crate::resolution`). Empty means the language publishes its rate over
    /// every relation it reads.
    pub builtins: &'static [&'static str],
    /// The relations of one chapter, as the grammar reads them.
    pub relations: fn(&Node, &[u8]) -> Vec<RawEdge>,
}

impl Grammar {
    /// Parses one file into the pruned chapter of the book.
    pub fn parse(
        &self,
        path: &str,
        bytes: &[u8],
        mtime_ns: i64,
    ) -> Result<ParsedFile, ExtractError> {
        extract(self, path, bytes, mtime_ns)
    }
}

/// Where the module path of a chapter comes from.
#[derive(Clone, Copy)]
pub enum ModuleSource {
    /// The relative path of the file is the whole of it: its directories and
    /// its name, in the idiom of the language.
    Path(fn(&str) -> String),
    /// The tree names the module before the path does: a Java file is named by
    /// the `package` it declares, and by its directory only when it declares
    /// none.
    Tree(fn(&Node, &str, &[u8]) -> String),
}

impl ModuleSource {
    /// The module path of one chapter: the prefix of every `qname` of the file.
    fn of(&self, root: &Node, path: &str, source: &[u8]) -> String {
        match self {
            ModuleSource::Path(of) => of(path),
            ModuleSource::Tree(of) => of(root, path, source),
        }
    }
}

/// How the synthetic `module` symbol that anchors the file-scope relations of a
/// chapter is named.
#[derive(Clone, Copy)]
pub enum Anchor {
    /// The whole module path: a Rust chapter **is** the module, so
    /// `crates/x/src/demo.rs` anchors as `x::demo` and not as `demo`.
    ModulePath,
    /// The last segment of the module path, or the stem of the file when the
    /// chapter has no module path at all (`src/tools.ts` → `tools`): the idiom
    /// of TypeScript, Python, Go and Java.
    LastSegment,
}

impl Anchor {
    /// The name of the anchor of one chapter.
    fn name(&self, path: &str, module: &str) -> String {
        match self {
            Anchor::ModulePath if module.is_empty() => {
                file_stem(path).unwrap_or_else(|| path.to_string())
            }
            Anchor::ModulePath => module.to_string(),
            Anchor::LastSegment => chapter_anchor_name(path, module),
        }
    }
}

/// How a language walks its tree.
#[derive(Clone, Copy)]
pub enum Walk {
    /// The tables of [`Rules`]: the whole walk is data.
    Table(&'static Rules),
    /// The language walks its own scope and writes the symbols itself, when its
    /// declarations are not a tree of containers: Go's package scope, whose
    /// methods are re-parented once the whole file is read.
    Custom(fn(&Node, &str, &[u8], &mut Vec<ParsedSymbol>)),
}

/// The declarative tables of one language: the whole of its symbol walk.
#[derive(Clone, Copy)]
pub struct Rules {
    /// Node kinds that are not declarations of the book and nest nothing: the
    /// walk continues at the same `qname` nesting. This is where a language
    /// unwraps its wrappers (`export`/`declare` in TypeScript, a decorated
    /// definition in Python) and enters a body (`class_body`, a
    /// `declaration_list`).
    pub transparent: &'static [&'static str],
    /// Node kinds whose body nests a `qname`.
    pub containers: &'static [Container],
    /// Node kinds that are declarations with no nesting.
    pub leaves: &'static [Leaf],
    /// Node kinds the tables cannot cover, and the language keeps: tried once
    /// the tables did not know the node.
    pub hooks: &'static [(&'static str, Hook)],
}

/// A declaration whose body nests: `class ToolBox { … }` puts every child of
/// its body under `ToolBox.`.
#[derive(Clone, Copy)]
pub struct Container {
    /// The node kind of the container: `class_declaration`.
    pub node: &'static str,
    /// The kind of the symbol it writes: `class`.
    pub kind: &'static str,
}

/// A declaration with no nesting: `fn render_with`, `const LIMIT`.
#[derive(Clone, Copy)]
pub struct Leaf {
    /// The node kind of the declaration.
    pub node: &'static str,
    /// The kind of the symbol at the root of a file, or inside a body that is
    /// not a container.
    pub kind: &'static str,
    /// The kind the leaf takes inside a container of that kind: a `fn` of a
    /// Rust `impl` is a `method`, and so is a `def` of a Python `class`.
    pub inside: &'static [(&'static str, &'static str)],
}

impl Leaf {
    /// The kind of the leaf, which depends on the container that owns it.
    fn kind_in(&self, container: Option<&'static str>) -> &'static str {
        container
            .and_then(|container| {
                self.inside
                    .iter()
                    .find(|(inside, _)| *inside == container)
                    .map(|(_, kind)| *kind)
            })
            .unwrap_or(self.kind)
    }
}

/// A container of the tables, spelled short: `container("class_body", "class")`.
///
/// The tables of [`crate::lang::registry`] are read as data — one line per node
/// kind — so they are written with these three constructors instead of a struct
/// literal each.
pub const fn container(node: &'static str, kind: &'static str) -> Container {
    Container { node, kind }
}

/// A leaf of the tables that keeps its kind at every nesting:
/// `leaf("const_item", "const")`.
pub const fn leaf(node: &'static str, kind: &'static str) -> Leaf {
    Leaf {
        node,
        kind,
        inside: &[],
    }
}

/// A leaf of the tables whose kind depends on the container that owns it:
/// `leaf_in("function_item", "fn", &[("impl", "method")])`.
pub const fn leaf_in(
    node: &'static str,
    kind: &'static str,
    inside: &'static [(&'static str, &'static str)],
) -> Leaf {
    Leaf { node, kind, inside }
}

/// The nesting a node is walked in.
#[derive(Clone, Copy)]
pub struct Nesting<'a> {
    /// The prefix of the `qname`s of the node, separator included.
    pub prefix: &'a str,
    /// The symbol that contains the node, when one does.
    pub parent: Option<usize>,
    /// The kind of the container that owns the node, when one does.
    pub container: Option<&'static str>,
}

/// What a hook answers when it emitted a container: the symbol it wrote and the
/// nesting of its body. A hook that emits leaves and nests nothing answers
/// `None`.
#[derive(Clone)]
pub struct Body {
    /// Index of the container in the symbols of the chapter.
    pub index: usize,
    /// The prefix of the `qname`s of its body, separator included.
    pub prefix: String,
    /// The kind of the container, which decides the kind of a leaf inside it.
    pub container: &'static str,
}

/// A node kind a language keeps itself, with the body it nests when it does.
pub type Hook = fn(&Node, &Nesting, &[u8], &mut Vec<ParsedSymbol>) -> Option<Body>;

/// Parses one file into the chapter of the book: the pipeline every grammar of
/// the book shares.
fn extract(
    grammar: &Grammar,
    path: &str,
    bytes: &[u8],
    mtime_ns: i64,
) -> Result<ParsedFile, ExtractError> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&(grammar.language)(path))
        .map_err(|error| ExtractError::Grammar {
            language: grammar.lang,
            detail: error.to_string(),
        })?;
    let tree = parser.parse(bytes, None).ok_or(ExtractError::NoTree {
        language: grammar.lang,
    })?;
    let root = tree.root_node();

    let module = grammar.module.of(&root, path, bytes);
    let prefix = prefix_of(&module, grammar.separator);
    let mut symbols = Vec::new();
    match grammar.walk {
        Walk::Table(rules) => walk(
            &root,
            Nesting {
                prefix: &prefix,
                parent: None,
                container: None,
            },
            rules,
            grammar.separator,
            bytes,
            &mut symbols,
        ),
        Walk::Custom(walk) => walk(&root, &prefix, bytes, &mut symbols),
    }

    // The relations are attached after the declarations exist: the ownership
    // of an edge is decided by the spans of the chapter (ideas §10.1).
    let raw = (grammar.relations)(&root, bytes);
    let anchor = anchor_symbol(&module, &grammar.anchor.name(path, &module));
    let edges = attach(raw, &mut symbols, anchor);

    Ok(ParsedFile {
        path: path.to_string(),
        lang: grammar.lang,
        content_hash: content_hash(bytes),
        size: bytes.len() as i64,
        mtime_ns,
        symbols,
        edges,
    })
}

/// Walks the declarations of a node with the tables of the registry.
///
/// A transparent kind is walked at the same nesting, a container is emitted and
/// its body walked under it, a leaf is emitted and nests nothing, and a hook is
/// the one node kind the language names itself.
fn walk(
    node: &Node,
    nesting: Nesting,
    rules: &'static Rules,
    separator: &'static str,
    source: &[u8],
    out: &mut Vec<ParsedSymbol>,
) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        let kind = child.kind();
        if rules.transparent.contains(&kind) {
            walk(&child, nesting, rules, separator, source, out);
        } else if let Some(rule) = rules.containers.iter().find(|rule| rule.node == kind) {
            let Some(index) = emit(
                &child,
                rule.kind,
                nesting.prefix,
                nesting.parent,
                source,
                out,
            ) else {
                continue;
            };
            let own = format!("{}{}{}", nesting.prefix, out[index].name, separator);
            walk(
                &child,
                Nesting {
                    prefix: &own,
                    parent: Some(index),
                    container: Some(rule.kind),
                },
                rules,
                separator,
                source,
                out,
            );
        } else if let Some(rule) = rules.leaves.iter().find(|rule| rule.node == kind) {
            emit(
                &child,
                rule.kind_in(nesting.container),
                nesting.prefix,
                nesting.parent,
                source,
                out,
            );
        } else if let Some((_, hook)) = rules.hooks.iter().find(|(node, _)| *node == kind) {
            let Some(body) = hook(&child, &nesting, source, out) else {
                continue;
            };
            walk(
                &child,
                Nesting {
                    prefix: &body.prefix,
                    parent: Some(body.index),
                    container: Some(body.container),
                },
                rules,
                separator,
                source,
                out,
            );
        }
        // Anything else is not a declaration of the book: a comment, a string,
        // a `use` (the relations of the chapter keep it, not the symbols).
    }
}

#[cfg(test)]
mod tests {
    use super::{Anchor, Leaf};

    /// The kind of a leaf is the one its container gives it: a `fn` of a Rust
    /// `impl` is a `method`, and the same `fn` of a `mod` stays a `fn`.
    #[test]
    fn a_leaf_takes_the_kind_of_the_container_that_owns_it() {
        let leaf = Leaf {
            node: "function_item",
            kind: "fn",
            inside: &[("impl", "method"), ("trait", "method")],
        };
        assert_eq!(leaf.kind_in(None), "fn");
        assert_eq!(leaf.kind_in(Some("mod")), "fn");
        assert_eq!(leaf.kind_in(Some("impl")), "method");
        assert_eq!(leaf.kind_in(Some("trait")), "method");
    }

    /// The anchor of a chapter is named the widest way its language names the
    /// module: Rust by the whole module path, the four others by its last
    /// segment (and the stem of the file when there is no module path).
    #[test]
    fn the_anchor_of_a_chapter_is_named_the_way_of_its_language() {
        assert_eq!(Anchor::ModulePath.name("src/main.rs", ""), "main");
        assert_eq!(
            Anchor::ModulePath.name("crates/x/src/demo.rs", "x::demo"),
            "x::demo"
        );
        assert_eq!(
            Anchor::LastSegment.name("src/tools.ts", "src.tools"),
            "tools"
        );
        assert_eq!(Anchor::LastSegment.name("index.ts", ""), "index");
    }
}
