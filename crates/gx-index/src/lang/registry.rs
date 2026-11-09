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
    feature = "lang-swift",
    feature = "lang-kotlin",
    feature = "lang-dart",
    feature = "lang-scala",
    feature = "lang-r",
    feature = "lang-lua",
    feature = "lang-zig",
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
    feature = "lang-swift",
    feature = "lang-kotlin",
    feature = "lang-dart",
    feature = "lang-scala",
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
    // Swift keeps leaves of its own; Kotlin and Dart name their declarations
    // with `leaf_in` and their hooks.
    feature = "lang-swift",
    feature = "lang-scala",
))]
use super::engine::leaf;
#[cfg(any(
    feature = "lang-rust",
    feature = "lang-python",
    feature = "lang-ruby",
    feature = "lang-swift",
    feature = "lang-kotlin",
    feature = "lang-scala",
    feature = "lang-zig",
))]
use super::engine::leaf_in;

#[cfg(feature = "lang-c")]
use super::c;
#[cfg(feature = "lang-cpp")]
use super::cpp;
#[cfg(feature = "lang-csharp")]
use super::csharp;
#[cfg(feature = "lang-dart")]
use super::dart;
#[cfg(feature = "lang-go")]
use super::go;
#[cfg(feature = "lang-java")]
use super::java;
#[cfg(feature = "lang-javascript")]
use super::javascript;
#[cfg(feature = "lang-kotlin")]
use super::kotlin;
#[cfg(feature = "lang-lua")]
use super::lua;
#[cfg(feature = "lang-php")]
use super::php;
#[cfg(feature = "lang-python")]
use super::python;
#[cfg(feature = "lang-r")]
use super::r;
#[cfg(feature = "lang-ruby")]
use super::ruby;
#[cfg(feature = "lang-rust")]
use super::rust;
#[cfg(feature = "lang-scala")]
use super::scala;
#[cfg(feature = "lang-shell")]
use super::shell;
#[cfg(feature = "lang-sql")]
use super::sql;
#[cfg(feature = "lang-swift")]
use super::swift;
#[cfg(feature = "lang-typescript")]
use super::typescript;
#[cfg(feature = "lang-zig")]
use super::zig;

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
    builtins: &[],
    relations: rust::relations,
    detail: None,
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
    builtins: &[],
    relations: typescript::relations,
    detail: None,
};

/// The runtime names of Python (T6): the builtins of the language and the
/// modules and classes of its standard library. No chapter of a worktree
/// declares them, so D3 leaves a relation whose target is one of them out of
/// the rate while no chapter of the book declares that name (the rule and its
/// guard live in `crate::resolution`).
///
/// The entries are folded the way `name_norm` folds a target (a lowercase), so
/// `Path` of `pathlib` is the entry `path` — and `os.path` reads with it. The
/// list is **knowledge, not a search**: it holds the names that appear as
/// targets in real code, and adding one is one line of data.
#[cfg(feature = "lang-python")]
const PYTHON_BUILTINS: &[&str] = &[
    // The modules of the standard library, by the segment an `import` binds.
    "abc",
    "argparse",
    "asyncio",
    "base64",
    "bisect",
    "collections",
    "concurrent",
    "contextlib",
    "copy",
    "csv",
    "dataclasses",
    "datetime",
    "decimal",
    "enum",
    "fractions",
    "functools",
    "hashlib",
    "heapq",
    "hmac",
    "html",
    "http",
    "importlib",
    "inspect",
    "io",
    "itertools",
    "json",
    "logging",
    "math",
    "multiprocessing",
    "operator",
    "os",
    "path",
    "pathlib",
    "pickle",
    "platform",
    "pprint",
    "purepath",
    "queue",
    "random",
    "re",
    "shutil",
    "signal",
    "socket",
    "sqlite3",
    "ssl",
    "string",
    "struct",
    "subprocess",
    "sys",
    "tempfile",
    "textwrap",
    "threading",
    "time",
    "traceback",
    "types",
    "typing",
    "unittest",
    "urllib",
    "uuid",
    "warnings",
    "weakref",
    "xml",
    "zipfile",
    // The builtins of the language: types, functions and constants.
    "abs",
    "all",
    "any",
    "ascii",
    "bin",
    "bool",
    "bytearray",
    "bytes",
    "callable",
    "chr",
    "classmethod",
    "complex",
    "dict",
    "dir",
    "divmod",
    "enumerate",
    "eval",
    "exec",
    "filter",
    "float",
    "format",
    "frozenset",
    "getattr",
    "globals",
    "hasattr",
    "hash",
    "help",
    "hex",
    "id",
    "input",
    "int",
    "isinstance",
    "issubclass",
    "iter",
    "len",
    "list",
    "locals",
    "map",
    "max",
    "memoryview",
    "min",
    "next",
    "object",
    "oct",
    "open",
    "ord",
    "pow",
    "print",
    "property",
    "range",
    "repr",
    "reversed",
    "round",
    "set",
    "setattr",
    "slice",
    "sorted",
    "staticmethod",
    "str",
    "sum",
    "super",
    "tuple",
    "type",
    "vars",
    "zip",
    // The exceptions every program raises without importing anything.
    "attributeerror",
    "baseexception",
    "exception",
    "indexerror",
    "keyerror",
    "nameerror",
    "notimplementederror",
    "oserror",
    "runtimeerror",
    "stopiteration",
    "typeerror",
    "valueerror",
    "zerodivisionerror",
];

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
    builtins: PYTHON_BUILTINS,
    relations: python::relations,
    detail: None,
};

/// The runtime names of Go (T6): the predeclared identifiers of the language
/// and the packages and types of its standard library, folded the way
/// `name_norm` folds a target. `import "net/http"` binds `http`, so the
/// package is the last segment of its path; a type of `fmt` or `io`
/// (`fmt.Stringer`, `io.Reader`) is the name a call or a signature spells.
#[cfg(feature = "lang-go")]
const GO_BUILTINS: &[&str] = &[
    // The packages of the standard library, by the segment an import binds.
    "archive",
    "atomic",
    "bufio",
    "bytes",
    "compress",
    "container",
    "context",
    "crypto",
    "database",
    "debug",
    "embed",
    "encoding",
    "errors",
    "expvar",
    "flag",
    "fmt",
    "go",
    "hash",
    "html",
    "http",
    "image",
    "index",
    "io",
    "json",
    "log",
    "maps",
    "math",
    "mime",
    "net",
    "os",
    "path",
    "plugin",
    "reflect",
    "regexp",
    "runtime",
    "slices",
    "sort",
    "sql",
    "strconv",
    "strings",
    "sync",
    "syscall",
    "testing",
    "text",
    "time",
    "tls",
    "unicode",
    "unsafe",
    "url",
    "utf8",
    "xml",
    // The types of the standard library a signature names by its last
    // segment (`fmt.Stringer`, `io.Reader`, `sync.Mutex`).
    "buffer",
    "conn",
    "duration",
    "handler",
    "listener",
    "mutex",
    "reader",
    "request",
    "responsewriter",
    "rwmutex",
    "stringer",
    "waitgroup",
    "writer",
    // The predeclared identifiers of the language.
    "any",
    "append",
    "bool",
    "byte",
    "cap",
    "clear",
    "close",
    "comparable",
    "complex",
    "complex128",
    "complex64",
    "copy",
    "delete",
    "error",
    "false",
    "float32",
    "float64",
    "imag",
    "int",
    "int16",
    "int32",
    "int64",
    "int8",
    "iota",
    "len",
    "make",
    "max",
    "min",
    "new",
    "nil",
    "panic",
    "print",
    "println",
    "real",
    "recover",
    "rune",
    "string",
    "true",
    "uint",
    "uint16",
    "uint32",
    "uint64",
    "uint8",
    "uintptr",
];

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
    builtins: GO_BUILTINS,
    relations: go::relations,
    detail: None,
};

/// The runtime names of Java (T6): `java.lang` (which every file imports
/// without writing it) and the types and packages of the standard library a
/// chapter names in a signature or an `extends`, folded the way `name_norm`
/// folds a target.
#[cfg(feature = "lang-java")]
const JAVA_BUILTINS: &[&str] = &[
    // java.lang: the types no file has to import.
    "autocloseable",
    "boolean",
    "byte",
    "char",
    "charsequence",
    "class",
    "cloneable",
    "comparable",
    "deprecated",
    "double",
    "enum",
    "error",
    "exception",
    "float",
    "functionalinterface",
    "illegalargumentexception",
    "illegalstateexception",
    "int",
    "integer",
    "iterable",
    "long",
    "math",
    "number",
    "object",
    "override",
    "record",
    "runnable",
    "runtimeexception",
    "short",
    "string",
    "stringbuilder",
    "suppresswarnings",
    "system",
    "thread",
    "throwable",
    "unsupportedoperationexception",
    "void",
    // The collections, the streams and the rest of the standard library.
    "arraylist",
    "arrays",
    "bigdecimal",
    "biginteger",
    "bufferedreader",
    "bufferedwriter",
    "calendar",
    "charset",
    "collection",
    "collections",
    "collector",
    "collectors",
    "comparator",
    "completablefuture",
    "concurrent",
    "connection",
    "consumer",
    "date",
    "deque",
    "drivermanager",
    "duration",
    "executor",
    "executors",
    "file",
    "files",
    "function",
    "future",
    "hashmap",
    "hashset",
    "inputstream",
    "instant",
    "io",
    "iterator",
    "level",
    "linkedhashmap",
    "linkedlist",
    "list",
    "localdate",
    "localdatetime",
    "logger",
    "logmanager",
    "map",
    "matcher",
    "net",
    "optional",
    "outputstream",
    "path",
    "paths",
    "pattern",
    "predicate",
    "queue",
    "random",
    "reader",
    "regex",
    "requirenonnull",
    "resultset",
    "scanner",
    "set",
    "sqlexception",
    "standardcharsets",
    "statement",
    "stream",
    "supplier",
    "time",
    "treemap",
    "treeset",
    "uri",
    "url",
    "util",
    "uuid",
    "writer",
];

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
    builtins: JAVA_BUILTINS,
    relations: java::relations,
    detail: None,
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
    builtins: &[],
    relations: javascript::relations,
    detail: None,
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
    builtins: &[],
    relations: c::relations,
    detail: None,
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
    builtins: &[],
    relations: cpp::relations,
    detail: None,
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
    builtins: &[],
    relations: csharp::relations,
    detail: None,
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
    builtins: &[],
    relations: php::relations,
    detail: None,
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
    builtins: &[],
    relations: ruby::relations,
    detail: None,
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
    builtins: &[],
    relations: sql::relations,
    detail: Some(sql::tables),
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
    builtins: &[],
    relations: shell::relations,
    detail: None,
};

/// Swift: the declarations of a file with their methods, and the module named
/// by the path of the file (Swift declares no package in the source).
///
/// Swift spells a class, a struct, an actor, an enum and an extension with one
/// node kind told apart by a keyword, so the kinds of those five are the hook
/// `swift::type_declaration`; a protocol is a `trait` of the vocabulary. An
/// extension is an `impl` block of the type it extends (L10), a property at the
/// root of the file is a `const` and a property inside a type is a field, and a
/// `deinit`/`subscript` spells its name with no field (the `swift::member`
/// hook). Every relation kind of the schema exists in Swift (see `swift.rs`).
#[cfg(feature = "lang-swift")]
pub const SWIFT: Grammar = Grammar {
    lang: swift::LANGUAGE,
    extensions: &["swift"],
    language: swift::language,
    module: ModuleSource::Path(swift::module_path),
    separator: ".",
    anchor: Anchor::LastSegment,
    walk: Walk::Table(&Rules {
        transparent: &["class_body", "protocol_body", "enum_class_body"],
        containers: &[container("protocol_declaration", "trait")],
        leaves: &[
            // A `func` of a type, a protocol or an extension is a method; at the
            // root of the file it is a `fn`.
            leaf_in(
                "function_declaration",
                "fn",
                &[
                    ("class", "method"),
                    ("type", "method"),
                    ("trait", "method"),
                    ("impl", "method"),
                ],
            ),
            leaf("protocol_function_declaration", "method"),
            // `init` spells its name in the `name` field (an `init` keyword).
            leaf("init_declaration", "method"),
            leaf("typealias_declaration", "type"),
            leaf("associatedtype_declaration", "type"),
        ],
        hooks: &[
            // class, struct, actor, enum and extension are one node kind
            // (swift::type_declaration).
            ("class_declaration", swift::type_declaration),
            // A property is a `const` at the root of the file and a field
            // everywhere else (swift::property).
            ("property_declaration", swift::property),
            // `deinit` and `subscript` name themselves with a keyword.
            ("deinit_declaration", swift::member),
            ("subscript_declaration", swift::member),
        ],
    }),
    builtins: &[],
    relations: swift::relations,
    detail: None,
};

/// Kotlin: the types of a file with their functions, nested under the `package`
/// the file declares — which is why the module path of a chapter comes from the
/// tree (like Java and C#) and not from the path.
///
/// Kotlin spells a class and an interface with one node kind and marks an enum
/// with a `class_modifier`, so the kind is the hook `kotlin::type_declaration`.
/// A companion object is a class named `Companion` when it declares no name
/// (the name the language gives it), a data class is a class (the `data`
/// modifier synthesises members the source never spells) and a `typealias`
/// names itself in the field the table would read as a type. Every relation
/// kind of the schema exists in Kotlin (see `kotlin.rs`).
#[cfg(feature = "lang-kotlin")]
pub const KOTLIN: Grammar = Grammar {
    lang: kotlin::LANGUAGE,
    extensions: &["kt", "kts"],
    language: kotlin::language,
    module: ModuleSource::Tree(kotlin::module_path),
    separator: ".",
    anchor: Anchor::LastSegment,
    walk: Walk::Table(&Rules {
        transparent: &["class_body", "enum_class_body"],
        containers: &[container("object_declaration", "class")],
        leaves: &[
            // A `fun` of a type is a method and a top-level `fun` is a `fn`.
            leaf_in(
                "function_declaration",
                "fn",
                &[("class", "method"), ("trait", "method"), ("type", "method")],
            ),
        ],
        hooks: &[
            // class, interface and enum class are one node kind
            // (kotlin::type_declaration).
            ("class_declaration", kotlin::type_declaration),
            // `companion object` names itself `Companion` when it has no name.
            ("companion_object", kotlin::companion),
            // A property is a `const` at the root of the file and a field
            // everywhere else (kotlin::property).
            ("property_declaration", kotlin::property),
            // `typealias Handler = …` keeps its name in the `type` field.
            ("type_alias", kotlin::alias),
        ],
    }),
    builtins: &[],
    relations: kotlin::relations,
    detail: None,
};

/// Dart: the types of a file with their methods and constructors, and the
/// module named by the path of the file (a `library` directive names a library,
/// never a path).
///
/// A mixin is a `trait` of the vocabulary, an enum and an extension a `type`.
/// Dart spells a member one wrapper at a time — `member_declaration`,
/// `method_declaration`, `method_signature`, `function_signature` — and the
/// name lives in the innermost of them while the span the book needs is the
/// outermost, so the declarations of a file and of a type are hooks
/// (`dart::declaration`): a member is a `fn` at the root of the file and a
/// `method` inside a type. A `typedef` and a top-level variable name themselves
/// in a list or with no field (two hooks more). Every relation kind of the
/// schema exists in Dart, and `with` is `inherits` (see `dart.rs`).
#[cfg(feature = "lang-dart")]
pub const DART: Grammar = Grammar {
    lang: dart::LANGUAGE,
    extensions: &["dart"],
    language: dart::language,
    module: ModuleSource::Path(dart::module_path),
    separator: ".",
    anchor: Anchor::LastSegment,
    walk: Walk::Table(&Rules {
        transparent: &[
            "class_body",
            "extension_body",
            "enum_body",
            "member_declaration",
        ],
        containers: &[
            container("class_declaration", "class"),
            container("mixin_declaration", "trait"),
            container("enum_declaration", "type"),
            container("extension_declaration", "type"),
            container("extension_type_declaration", "type"),
        ],
        leaves: &[],
        hooks: &[
            // A method, an abstract method and a constructor are one hook: the
            // name is in the signature and the span is the declaration
            // (dart::declaration).
            ("function_declaration", dart::declaration),
            ("method_declaration", dart::declaration),
            ("getter_declaration", dart::declaration),
            ("setter_declaration", dart::declaration),
            ("declaration", dart::declaration),
            // `typedef Handler = …` names the alias with no field
            // (dart::alias).
            ("type_alias", dart::alias),
            // A top-level variable is one `const` per declarator
            // (dart::constants).
            ("top_level_variable_declaration", dart::constants),
        ],
    }),
    builtins: &[],
    relations: dart::relations,
    detail: None,
};

/// Scala: the types of a file with their members, nested under the `package`
/// the file declares — which is why the module path of a chapter comes from the
/// tree (like Java, C#, PHP and Kotlin) and not from the path.
///
/// A `case class` is a class (the `case` modifier synthesises members the source
/// never spells: Kotlin's data-class rule) and **an `object` is a class** — the
/// language calls it a singleton object and the compiler emits a real class for
/// it, so its members nest under it and it can extend a class or implement a
/// trait, which a namespace cannot (see `scala.rs`). A `val`/`var` names its
/// binding in the `pattern` field, so a root-level binding is a `const` by the
/// one hook Scala needs; a `val` inside a type is a field and no declaration of
/// the book. Every relation kind of the schema exists in Scala.
#[cfg(feature = "lang-scala")]
pub const SCALA: Grammar = Grammar {
    lang: scala::LANGUAGE,
    extensions: &["scala", "sc"],
    language: scala::language,
    module: ModuleSource::Tree(scala::module_path),
    separator: ".",
    anchor: Anchor::LastSegment,
    walk: Walk::Table(&Rules {
        transparent: &[
            "template_body",
            "enum_body",
            "package_clause",
            "package_object",
        ],
        containers: &[
            container("class_definition", "class"),
            // A singleton object is a class (see `scala.rs`).
            container("object_definition", "class"),
            container("trait_definition", "trait"),
            container("enum_definition", "type"),
        ],
        leaves: &[
            // A `def` of a type is a method; at the root of the file it is a
            // `fn` (Scala 3 allows top-level definitions).
            leaf_in(
                "function_definition",
                "fn",
                &[("class", "method"), ("trait", "method"), ("type", "method")],
            ),
            leaf_in(
                "function_declaration",
                "fn",
                &[("class", "method"), ("trait", "method"), ("type", "method")],
            ),
            leaf("type_definition", "type"),
        ],
        // A `val`/`var` is a `const` at the root of the file and a field
        // everywhere else (scala::value).
        hooks: &[
            ("val_definition", scala::value),
            ("var_definition", scala::value),
        ],
    }),
    builtins: &[],
    relations: scala::relations,
    detail: None,
};

/// R: the functions a script binds with `<-` as `fn` and every other binding as
/// `const`, with the file as the module (`R/store.R` → `R.store`).
///
/// R has no containers, no types and (as this grammar reads it) no `inherits`:
/// the tables keep no leaf and no container, and the two declarations of the
/// language come from the one hook on the assignment. Its relation kinds are
/// `calls` and `imports` (see `r.rs`).
#[cfg(feature = "lang-r")]
pub const R: Grammar = Grammar {
    // A script is spelled `.R` as often as `.r`, and both are R: an extension
    // never depends on the case the author chose.
    lang: r::LANGUAGE,
    extensions: &["r", "R"],
    language: r::language,
    module: ModuleSource::Path(r::module_path),
    separator: ".",
    anchor: Anchor::LastSegment,
    walk: Walk::Table(&Rules {
        transparent: &[],
        containers: &[],
        leaves: &[],
        // `parseConfig <- function(…)` and `LIMIT <- 3` are one hook: the name
        // of the binding is an operand of the operator (r::assignment).
        hooks: &[("binary_operator", r::assignment)],
    }),
    builtins: &[],
    relations: r::relations,
    detail: None,
};

/// Lua: the tables of a file as `module`s with their functions under them, and
/// the other root bindings as `const`s, with the file as the module
/// (`src/store.lua` → `src.store`; `init.lua` names its directory).
///
/// **A table is the module** (see `lua.rs`): `function M.open()` is a `fn` of
/// `M` and `function M:load()` a `method` of it, and `M.load = function()` — the
/// same declaration spelled the other way — is read as the same symbol. A field
/// that is only data is not a declaration. Its relation kinds are `calls` and
/// `imports` (`require`).
#[cfg(feature = "lang-lua")]
pub const LUA: Grammar = Grammar {
    lang: lua::LANGUAGE,
    extensions: &["lua"],
    language: lua::language,
    module: ModuleSource::Path(lua::module_path),
    separator: ".",
    anchor: Anchor::LastSegment,
    walk: Walk::Table(&Rules {
        transparent: &["variable_declaration"],
        containers: &[],
        leaves: &[],
        hooks: &[
            ("assignment_statement", lua::assignment),
            ("function_declaration", lua::function),
        ],
    }),
    builtins: &[],
    relations: lua::relations,
    detail: None,
};

/// Zig: the `const`s of a file, where a `const` that holds a `struct`, `union`,
/// `enum` or `opaque` value is a `type` whose `fn`s are its methods, and the
/// file is the module (`src/store.zig` → `src.store`).
///
/// The grammar spells `variable_declaration` with no field at all, so a
/// declaration (and the type it holds) is the one hook of the chapter
/// (`zig::declaration`). **The nesting of a const-held type is the type's body**:
/// a `fn` at the root of the file is a `fn` and the same node inside such a
/// `const` is a `method`. A `container_field` is a field, not a declaration; Zig
/// has no `inherits`. Relation kinds: `calls`, `imports` (`@import`) and `uses`.
#[cfg(feature = "lang-zig")]
pub const ZIG: Grammar = Grammar {
    lang: zig::LANGUAGE,
    extensions: &["zig"],
    language: zig::language,
    module: ModuleSource::Path(zig::module_path),
    separator: ".",
    anchor: Anchor::LastSegment,
    walk: Walk::Table(&Rules {
        // The body of a type is the body of the `const` that holds it.
        transparent: &[
            "struct_declaration",
            "union_declaration",
            "enum_declaration",
            "opaque_declaration",
        ],
        containers: &[],
        leaves: &[leaf_in("function_declaration", "fn", &[("type", "method")])],
        hooks: &[("variable_declaration", zig::declaration)],
    }),
    builtins: &[],
    relations: zig::relations,
    detail: None,
};

/// Every grammar this build ships, in the order the book lists them.
///
/// The list is what the features left (T4/C3): the default build ships every
/// language of the book (the single-binary decision of L9), fewer in a slim
/// build, and never a grammar that is not compiled. Batch 1 of T3
/// (JavaScript/JSX, C, C++, C#), Batch 2 (PHP, Ruby, SQL, Shell), Batch 3
/// (Swift, Kotlin, Dart) and Batch 4 (Scala, R, Lua, Zig) come after the
/// original five, in the order they landed — and Batch 4 is the twentieth
/// language of the approved list (L6).
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
    #[cfg(feature = "lang-swift")]
    SWIFT,
    #[cfg(feature = "lang-kotlin")]
    KOTLIN,
    #[cfg(feature = "lang-dart")]
    DART,
    #[cfg(feature = "lang-scala")]
    SCALA,
    #[cfg(feature = "lang-r")]
    R,
    #[cfg(feature = "lang-lua")]
    LUA,
    #[cfg(feature = "lang-zig")]
    ZIG,
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

    /// T6: the runtime names of a language are folded the way the metric folds
    /// a target (`name_norm` is a lowercase), so a lookup of an edge can never
    /// miss a name the list means to exclude; and the list belongs to the
    /// language whose runtime spells it. The three weak languages of T6 carry
    /// one; the other seventeen keep publishing their rate over every relation
    /// they read, so their numbers do not move in this cut.
    #[test]
    fn the_runtime_names_of_a_language_are_folded_and_its_own() {
        use crate::lang::name_norm;

        for grammar in GRAMMARS {
            for name in grammar.builtins {
                assert_eq!(
                    *name,
                    name_norm(name),
                    "`{name}` of `{}` is not folded: the lookup would miss it",
                    grammar.lang
                );
            }
            let expected = matches!(grammar.lang, "python" | "go" | "java");
            assert_eq!(
                !grammar.builtins.is_empty(),
                expected,
                "the runtime list of `{}` is the one of T6",
                grammar.lang
            );
        }
    }
}
