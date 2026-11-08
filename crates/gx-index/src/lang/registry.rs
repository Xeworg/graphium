//! The registry of the languages of the book (S3, T2; T3): one entry per
//! grammar, and the declarative tables of every grammar of the build side by
//! side.
//!
//! A language is a [`Grammar`] here — its `lang`, its extensions, the
//! tree-sitter grammar of a path, where its module path and its chapter anchor
//! come from, the node kinds it keeps and the relations it reads — plus, in its
//! own module, the parts that are genuinely imperative and that its
//! [`Rules::hooks`] reach: Go's second pass over the receivers, the name of a
//! Rust `impl`, the declarators of a TypeScript/JavaScript `const`, the
//! constants of a Python module, the `package` path of Java, the declarator of
//! a C/C++ function, the constants of a C module and the `namespace` path of
//! C#.
//!
//! Adding language N+1 is therefore: a dependency and a cargo feature in
//! `Cargo.toml`, a module next to the others with its `language()`,
//! `module_path()` and `relations()` (and a hook only where its grammar names a
//! declaration in a way the tables cannot), and its entry below. Nothing else
//! of the engine moves, and the schema of the book does not change: the `lang`
//! of the table `file` is the string of this entry, and the DDL names no
//! language at all (T3/S4: the registry of the build is the list, and a chapter
//! of a language the registry does not know is refused at insert).
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
    feature = "lang-javascript",
    feature = "lang-c",
    feature = "lang-cpp",
    feature = "lang-csharp",
    feature = "lang-php",
    feature = "lang-ruby",
    feature = "lang-sql",
    feature = "lang-shell",
))]
use super::engine::{Anchor, ModuleSource, Rules, Walk};
// The constructors are imports of their own: a slim build that compiles none
// of the languages that use one of them does not warn about it.
#[cfg(any(
    feature = "lang-rust",
    feature = "lang-typescript",
    feature = "lang-python",
    feature = "lang-java",
    feature = "lang-javascript",
    feature = "lang-csharp",
    feature = "lang-php",
    feature = "lang-ruby",
))]
use super::engine::container;
#[cfg(any(
    feature = "lang-rust",
    feature = "lang-typescript",
    feature = "lang-python",
    feature = "lang-java",
    feature = "lang-javascript",
    feature = "lang-csharp",
    feature = "lang-php",
    feature = "lang-ruby",
    feature = "lang-shell",
))]
use super::engine::leaf;
#[cfg(any(feature = "lang-rust", feature = "lang-python", feature = "lang-ruby"))]
use super::engine::leaf_in;

#[cfg(feature = "lang-c")]
use super::c;
#[cfg(feature = "lang-cpp")]
use super::cpp;
#[cfg(feature = "lang-csharp")]
use super::csharp;
#[cfg(feature = "lang-go")]
use super::go;
#[cfg(feature = "lang-java")]
use super::java;
#[cfg(feature = "lang-javascript")]
use super::javascript;
#[cfg(feature = "lang-php")]
use super::php;
#[cfg(feature = "lang-python")]
use super::python;
#[cfg(feature = "lang-ruby")]
use super::ruby;
#[cfg(feature = "lang-rust")]
use super::rust;
#[cfg(feature = "lang-shell")]
use super::shell;
#[cfg(feature = "lang-sql")]
use super::sql;
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

/// JavaScript and JSX: one grammar, one module, the nesting of the language in
/// the `qname` (`src.tools.ToolBox.renderWith`).
///
/// The `export` wrappers, the class body and a block are transparent; a
/// module-level `const`/`let`/`var` is the `const` of the language by its
/// declarators, which is the one hook JavaScript needs. There is no `uses`
/// table: JavaScript has no type annotations (see `javascript.rs`).
#[cfg(feature = "lang-javascript")]
pub const JAVASCRIPT: Grammar = Grammar {
    lang: javascript::LANGUAGE,
    extensions: &["js", "jsx", "mjs", "cjs"],
    language: javascript::language,
    module: ModuleSource::Path(javascript::module_path),
    separator: ".",
    anchor: Anchor::LastSegment,
    walk: Walk::Table(&Rules {
        transparent: &["export_statement", "class_body", "statement_block"],
        containers: &[container("class_declaration", "class")],
        leaves: &[
            leaf("function_declaration", "fn"),
            leaf("generator_function_declaration", "fn"),
            leaf("method_definition", "method"),
        ],
        hooks: &[
            ("lexical_declaration", javascript::declarators),
            ("variable_declaration", javascript::declarators),
        ],
    }),
    relations: javascript::relations,
};

/// C: the declarations of a translation unit, named by their declarators and
/// with the file as the module (`src.store.open_store`).
///
/// The tables keep no leaf and no container of C: its declarations are named by
/// a declarator chain and a `struct` specifier is a declaration only when it
/// has a body, so they are four hooks (see `c.rs`). `.h` belongs to C and only
/// to C, so an extension never depends on the features of the build.
#[cfg(feature = "lang-c")]
pub const C: Grammar = Grammar {
    lang: c::LANGUAGE,
    extensions: &["c", "h"],
    language: c::language,
    module: ModuleSource::Path(c::module_path),
    separator: ".",
    anchor: Anchor::LastSegment,
    walk: Walk::Table(&Rules {
        transparent: &[
            "declaration_list",
            "field_declaration_list",
            "linkage_specification",
            "preproc_if",
            "preproc_ifdef",
            "preproc_else",
        ],
        containers: &[],
        leaves: &[],
        hooks: &[
            ("function_definition", c::function_definition),
            ("struct_specifier", c::specifier),
            ("union_specifier", c::specifier),
            ("enum_specifier", c::specifier),
            ("type_definition", c::type_definition),
            ("declaration", c::declaration),
        ],
    }),
    relations: c::relations,
};

/// C++: the types and the functions of a translation unit, with the methods of
/// a class under it and the file as the module (`src.store.DiagramStore.load`).
///
/// A `namespace` is transparent: the path of the file already names the source,
/// so `src/store.cpp` is `src.store` and a `namespace store` does not nest the
/// `qname` twice. A specifier is a declaration only with a body, and a
/// `function_definition`/`declaration` is a `method` or a `fn` by the body that
/// owns it (see `cpp.rs`).
#[cfg(feature = "lang-cpp")]
pub const CPP: Grammar = Grammar {
    lang: cpp::LANGUAGE,
    extensions: &["cpp", "cc", "cxx", "c++", "hpp", "hh", "hxx", "h++"],
    language: cpp::language,
    module: ModuleSource::Path(cpp::module_path),
    separator: ".",
    anchor: Anchor::LastSegment,
    walk: Walk::Table(&Rules {
        transparent: &[
            "declaration_list",
            "field_declaration_list",
            "template_declaration",
            "linkage_specification",
            "namespace_definition",
            "preproc_if",
            "preproc_ifdef",
            "preproc_else",
        ],
        containers: &[],
        leaves: &[leaf("alias_declaration", "type")],
        hooks: &[
            ("function_definition", cpp::function_definition),
            ("declaration", cpp::declaration),
            ("field_declaration", cpp::field_declaration),
            ("class_specifier", cpp::specifier),
            ("struct_specifier", cpp::specifier),
            ("union_specifier", cpp::specifier),
            ("enum_specifier", cpp::specifier),
        ],
    }),
    relations: cpp::relations,
};

/// C#: the types of a file, with their constructors and methods as children,
/// nested under the namespace the file declares — which is why the module path
/// of a chapter comes from the tree (like Java) and not from the path.
///
/// A record is a class of the vocabulary, a struct is a type and an interface a
/// trait; a field or a property is not a declaration of the book. C# needs no
/// hook at all.
#[cfg(feature = "lang-csharp")]
pub const CSHARP: Grammar = Grammar {
    lang: csharp::LANGUAGE,
    extensions: &["cs"],
    language: csharp::language,
    module: ModuleSource::Tree(csharp::module_path),
    separator: ".",
    anchor: Anchor::LastSegment,
    walk: Walk::Table(&Rules {
        transparent: &[
            "declaration_list",
            "namespace_declaration",
            "file_scoped_namespace_declaration",
        ],
        containers: &[
            container("class_declaration", "class"),
            container("interface_declaration", "trait"),
            container("struct_declaration", "type"),
            container("record_declaration", "class"),
            container("enum_declaration", "type"),
        ],
        leaves: &[
            leaf("method_declaration", "method"),
            leaf("constructor_declaration", "method"),
            leaf("delegate_declaration", "type"),
        ],
        hooks: &[],
    }),
    relations: csharp::relations,
};

/// PHP: the types of a file, with their methods as children, nested under the
/// `namespace` the file declares — which is why the module path of a chapter
/// comes from the tree (like Java and C#) and not from the path.
///
/// A trait and an interface are both a `trait` of the vocabulary and an enum is
/// a `type`; a property is not a declaration of the book. The `const` group is
/// the one hook: `const LIMIT = 3;` names its declaration in a `const_element`
/// (see `php.rs`).
#[cfg(feature = "lang-php")]
pub const PHP: Grammar = Grammar {
    lang: php::LANGUAGE,
    extensions: &["php"],
    language: php::language,
    module: ModuleSource::Tree(php::module_path),
    separator: "\\",
    anchor: Anchor::LastSegment,
    walk: Walk::Table(&Rules {
        transparent: &[
            "namespace_definition",
            "declaration_list",
            "enum_declaration_list",
            "compound_statement",
        ],
        containers: &[
            container("class_declaration", "class"),
            container("interface_declaration", "trait"),
            container("trait_declaration", "trait"),
            container("enum_declaration", "type"),
        ],
        leaves: &[
            leaf("method_declaration", "method"),
            leaf("function_definition", "fn"),
        ],
        // A `const` group is one `const` per `const_element` (php::constants).
        hooks: &[("const_declaration", php::constants)],
    }),
    relations: php::relations,
};

/// Ruby: the classes and the modules of a file, with the methods of both under
/// them, and the constants assigned at the root of the file or of a class body.
///
/// A `def` outside a class or a module is a `fn` of the file; the metaprogrammed
/// declarations (`attr_accessor`, `define_method`) stay calls, which is the
/// honest scope of a syntactic pass (see `ruby.rs`).
#[cfg(feature = "lang-ruby")]
pub const RUBY: Grammar = Grammar {
    lang: ruby::LANGUAGE,
    extensions: &["rb"],
    language: ruby::language,
    module: ModuleSource::Path(ruby::module_path),
    separator: "::",
    anchor: Anchor::LastSegment,
    walk: Walk::Table(&Rules {
        transparent: &["body_statement"],
        containers: &[container("class", "class"), container("module", "module")],
        leaves: &[
            leaf_in("method", "fn", &[("class", "method"), ("module", "method")]),
            leaf("singleton_method", "method"),
        ],
        // `LIMIT = 3` is a constant; `@renderer = renderer` is not
        // (ruby::constant).
        hooks: &[("assignment", ruby::constant)],
    }),
    relations: ruby::relations,
};

/// SQL: the objects a schema declares — the tables, the views and the types as
/// `type`, the functions and the triggers as `fn` — with the file as the module.
///
/// Every declaration is a hook: a `CREATE` statement of this grammar has no
/// `name` field and names its object in an `object_reference` child (see
/// `sql.rs`). Its only relation kind is `uses` — a query, an index and a
/// foreign key read a table — and the decision is documented in that module.
#[cfg(feature = "lang-sql")]
pub const SQL: Grammar = Grammar {
    lang: sql::LANGUAGE,
    extensions: &["sql"],
    language: sql::language,
    module: ModuleSource::Path(sql::module_path),
    separator: ".",
    anchor: Anchor::LastSegment,
    walk: Walk::Table(&Rules {
        transparent: &["statement"],
        containers: &[],
        leaves: &[],
        hooks: &[
            ("create_table", sql::declaration),
            ("create_view", sql::declaration),
            ("create_materialized_view", sql::declaration),
            ("create_type", sql::declaration),
            ("create_function", sql::declaration),
            ("create_trigger", sql::declaration),
        ],
    }),
    relations: sql::relations,
};

/// Shell: the functions of a script and the variables assigned at its root,
/// with the file as the module (`scripts.build`).
///
/// Bash is the grammar of the language of the book: it answers `.sh` and
/// `.bash`. Shell needs no hook at all — a `function_definition` and a
/// `variable_assignment` both spell their name in a field — and its relation
/// kinds are `calls` and `imports` (see `shell.rs`).
#[cfg(feature = "lang-shell")]
pub const SHELL: Grammar = Grammar {
    lang: shell::LANGUAGE,
    extensions: &["sh", "bash"],
    language: shell::language,
    module: ModuleSource::Path(shell::module_path),
    separator: ".",
    anchor: Anchor::LastSegment,
    walk: Walk::Table(&Rules {
        transparent: &[],
        containers: &[],
        leaves: &[
            leaf("function_definition", "fn"),
            leaf("variable_assignment", "const"),
        ],
        hooks: &[],
    }),
    relations: shell::relations,
};

/// Every grammar this build ships, in the order the book lists them.
///
/// The list is what the features left (T4/C3): the five languages of C2 by
/// default, fewer in a slim build, and never a grammar that is not compiled.
/// Batch 1 of T3 (JavaScript/JSX, C, C++, C#) and Batch 2 (PHP, Ruby, SQL,
/// Shell) come after the five and are OFF by default, so the default binary and
/// its behaviour do not change.
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
    #[cfg(feature = "lang-javascript")]
    JAVASCRIPT,
    #[cfg(feature = "lang-c")]
    C,
    #[cfg(feature = "lang-cpp")]
    CPP,
    #[cfg(feature = "lang-csharp")]
    CSHARP,
    #[cfg(feature = "lang-php")]
    PHP,
    #[cfg(feature = "lang-ruby")]
    RUBY,
    #[cfg(feature = "lang-sql")]
    SQL,
    #[cfg(feature = "lang-shell")]
    SHELL,
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
