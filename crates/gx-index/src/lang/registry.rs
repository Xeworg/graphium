//! The registry of the languages of the book (S3, T2): one entry per grammar,
//! and the declarative tables of the five side by side.
//!
//! A language is a [`Grammar`] here — its `lang`, its extensions, the
//! tree-sitter grammar of a path, where its module path and its chapter anchor
//! come from, the node kinds it keeps and the relations it reads — plus, in its
//! own module, the parts that are genuinely imperative and that its
//! [`Rules::hooks`] reach: Go's second pass over the receivers, the name of a
//! Rust `impl`, the declarators of a TypeScript `const`, the constants of a
//! Python module, the `package` path of Java and the `called_name` chain of
//! every `relations()`.
//!
//! Adding language N+1 is therefore: a dependency and a cargo feature in
//! `Cargo.toml`, a module next to the five with its `language()`,
//! `module_path()` and `relations()` (and a hook only where its grammar names a
//! declaration in a way the tables cannot), and its entry below. Nothing else
//! of the engine moves, and the schema of the book does not change: the `lang`
//! of the table `file` is the string of this entry.
//!
//! The entries are what the cargo features left (T4/C3): a slim build lists
//! fewer languages and never a grammar that is not compiled.

use super::engine::Grammar;
// The tables only exist when a language does: a build that compiles no grammar
// imports none of them (T4/C3).
#[cfg(any(
    feature = "lang-rust",
    feature = "lang-typescript",
    feature = "lang-python",
    feature = "lang-go",
    feature = "lang-java",
))]
use super::engine::{container, leaf, leaf_in, Anchor, ModuleSource, Rules, Walk};

#[cfg(feature = "lang-go")]
use super::go;
#[cfg(feature = "lang-java")]
use super::java;
#[cfg(feature = "lang-python")]
use super::python;
#[cfg(feature = "lang-rust")]
use super::rust;
#[cfg(feature = "lang-typescript")]
use super::typescript;

/// Rust: the declarations of a module, with the `impl` blocks named by the type
/// they implement (a hook) and their methods under them.
///
/// `qname`s are `crate::module::Item`, so the whole module path is the anchor of
/// the chapter and the file-scope `use`s are the relations that need one.
#[cfg(feature = "lang-rust")]
pub const RUST: Grammar = Grammar {
    lang: rust::LANGUAGE,
    extensions: &["rs"],
    language: rust::language,
    module: ModuleSource::Path(rust::module_path),
    separator: "::",
    anchor: Anchor::ModulePath,
    walk: Walk::Table(&Rules {
        transparent: &["declaration_list", "field_declaration_list"],
        containers: &[
            container("trait_item", "trait"),
            container("mod_item", "module"),
        ],
        leaves: &[
            // A `fn` of an `impl` or a `trait` is a method; a `fn` of a `mod` is
            // a top-level fn of that module.
            leaf_in(
                "function_item",
                "fn",
                &[("impl", "method"), ("trait", "method")],
            ),
            leaf("struct_item", "type"),
            leaf("enum_item", "type"),
            leaf("union_item", "type"),
            leaf("type_item", "type"),
            leaf("const_item", "const"),
            leaf("static_item", "static"),
        ],
        // The `impl` is named by the type it implements (rust::impl_body).
        hooks: &[("impl_item", rust::impl_body)],
    }),
    relations: rust::relations,
};

/// TypeScript and TSX: one language, one module, the nesting of the language in
/// the `qname` (`src.tools.ToolBox.renderWith`).
///
/// The `export`/`declare` wrappers and the bodies are transparent; a
/// module-level `const`/`let`/`var` is the `const` of the language by its
/// declarators, which is the one hook TypeScript needs.
#[cfg(feature = "lang-typescript")]
pub const TYPESCRIPT: Grammar = Grammar {
    lang: typescript::LANGUAGE,
    extensions: &["ts", "tsx"],
    language: typescript::language,
    module: ModuleSource::Path(typescript::module_path),
    separator: ".",
    anchor: Anchor::LastSegment,
    walk: Walk::Table(&Rules {
        transparent: &[
            "export_statement",
            "ambient_declaration",
            "class_body",
            "interface_body",
            "statement_block",
        ],
        containers: &[
            container("class_declaration", "class"),
            container("abstract_class_declaration", "class"),
            container("interface_declaration", "trait"),
            container("internal_module", "module"),
        ],
        leaves: &[
            leaf("function_declaration", "fn"),
            leaf("method_definition", "method"),
            leaf("method_signature", "method"),
            leaf("abstract_method_signature", "method"),
            leaf("type_alias_declaration", "type"),
            leaf("enum_declaration", "type"),
        ],
        // A declaration group is one `const` per declarator
        // (typescript::declarators).
        hooks: &[
            ("lexical_declaration", typescript::declarators),
            ("variable_declaration", typescript::declarators),
        ],
    }),
    relations: typescript::relations,
};

/// Python: the classes with their methods (a `def` of a class is a `method`)
/// and the module-level constants, which is the one hook Python needs.
#[cfg(feature = "lang-python")]
pub const PYTHON: Grammar = Grammar {
    lang: python::LANGUAGE,
    extensions: &["py"],
    language: python::language,
    module: ModuleSource::Path(python::module_path),
    separator: ".",
    anchor: Anchor::LastSegment,
    walk: Walk::Table(&Rules {
        transparent: &["decorated_definition", "block"],
        containers: &[container("class_definition", "class")],
        leaves: &[leaf_in("function_definition", "fn", &[("class", "method")])],
        // An assignment at the root of the file is the constant of the module
        // (python::module_constant).
        hooks: &[("expression_statement", python::module_constant)],
    }),
    relations: python::relations,
};

/// Go: the package scope of a file. The package of a Go file is its directory,
/// and its methods hang from the type of their receiver whichever order the
/// file declares them in — so the walk is the language's own (T2/L3: the
/// second pass stays imperative) and the pipeline is still the shared one.
#[cfg(feature = "lang-go")]
pub const GO: Grammar = Grammar {
    lang: go::LANGUAGE,
    extensions: &["go"],
    language: go::language,
    module: ModuleSource::Path(go::module_path),
    separator: ".",
    anchor: Anchor::LastSegment,
    walk: Walk::Custom(go::walk),
    relations: go::relations,
};

/// Java: the types of a file, with their constructors and methods as children,
/// nested under the `package` the file declares — which is why the module path
/// of a chapter comes from the tree and not from the path of the file.
#[cfg(feature = "lang-java")]
pub const JAVA: Grammar = Grammar {
    lang: java::LANGUAGE,
    extensions: &["java"],
    language: java::language,
    module: ModuleSource::Tree(java::module_path),
    separator: ".",
    anchor: Anchor::LastSegment,
    walk: Walk::Table(&Rules {
        transparent: &[
            "class_body",
            "interface_body",
            "enum_body",
            "enum_body_declarations",
            "annotation_type_body",
        ],
        containers: &[
            container("class_declaration", "class"),
            container("record_declaration", "class"),
            container("interface_declaration", "trait"),
            container("annotation_type_declaration", "trait"),
            container("enum_declaration", "type"),
        ],
        leaves: &[
            leaf("method_declaration", "method"),
            leaf("constructor_declaration", "method"),
        ],
        hooks: &[],
    }),
    relations: java::relations,
};

/// Every grammar this build ships, in the order the book lists them.
///
/// The list is what the features left (T4/C3): the five languages of C2 by
/// default, fewer in a slim build, and never a grammar that is not compiled.
pub const GRAMMARS: &[Grammar] = &[
    #[cfg(feature = "lang-rust")]
    RUST,
    #[cfg(feature = "lang-typescript")]
    TYPESCRIPT,
    #[cfg(feature = "lang-python")]
    PYTHON,
    #[cfg(feature = "lang-go")]
    GO,
    #[cfg(feature = "lang-java")]
    JAVA,
];

#[cfg(test)]
mod tests {
    use super::GRAMMARS;
    use crate::lang::engine::{Grammar, Walk};

    /// S3/T2: the tables are the walk, so a node kind a table names must be a
    /// kind its grammar really has — a typo would be a silent no-op that no
    /// fixture has to notice. This is the check that keeps adding language N+1
    /// a matter of writing the table down.
    #[test]
    fn every_kind_of_the_tables_is_a_kind_of_its_grammar() {
        for grammar in GRAMMARS {
            let kinds = kinds_of(grammar);
            let Walk::Table(rules) = grammar.walk else {
                continue;
            };
            let mut named: Vec<&'static str> = Vec::new();
            named.extend_from_slice(rules.transparent);
            named.extend(rules.containers.iter().map(|rule| rule.node));
            named.extend(rules.leaves.iter().map(|rule| rule.node));
            named.extend(rules.hooks.iter().map(|(node, _)| *node));
            for kind in named {
                assert!(
                    kinds.contains(&kind),
                    "`{kind}` is not a node kind of `{}`",
                    grammar.lang
                );
            }
        }
    }

    /// Every node kind of one grammar of the registry.
    fn kinds_of(grammar: &Grammar) -> Vec<&'static str> {
        let language = (grammar.language)("probe.rs");
        (0..language.node_kind_count() as u16)
            .filter_map(|id| language.node_kind_for_id(id))
            .collect()
    }
}
