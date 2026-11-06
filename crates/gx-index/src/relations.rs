//! The relations of the book: the four kinds and the ownership of an edge
//! (F9, C1; S7, D3).
//!
//! The vocabulary is the schema's, **not** a language's: `calls`, `imports`,
//! `inherits` and `uses`. A grammar of `lang/` only has to record a relation
//! with the byte it starts at and the textual target as the source spells it;
//! this module decides which symbol of the chapter owns it and drops the
//! duplicates. Nothing here touches Tree-sitter, so the next language of C2
//! plugs into the same edge table without a schema change.
//!
//! The relations are **syntactic, not typed** (D3): there is no resolution of
//! types, so a name defined twice is answered as what it is — a name with two
//! definers — and never as one confident link.
//!
//! Two rules keep the book honest and small:
//!
//! 1. **No self-edges**: a symbol that mentions its own name adds no connection.
//! 2. **A file-scope relation needs a source**. A top-level `use` belongs to no
//!    declaration of the chapter, so the chapter gets one synthetic `module`
//!    symbol (the grammar builds it) to anchor those edges. The anchor is only
//!    appended when a file-scope relation exists: a file with no relations
//!    keeps the exact symbol list it declared.

use std::collections::BTreeSet;

use crate::lang::model::{name_norm, ParsedSymbol};

/// A call by name: `open_store()`, `store.load()`, `DiagramStore::new()`.
pub const CALLS: &str = "calls";
/// A `use` declaration: the segment it binds.
pub const IMPORTS: &str = "imports";
/// An `impl Trait for Type`, by the name of the trait.
pub const INHERITS: &str = "inherits";
/// A type reference: a parameter type, a return type, a field type.
pub const USES: &str = "uses";

/// The four kinds of the schema, in the order the book documents them.
pub const RELATION_KINDS: [&str; 4] = [CALLS, IMPORTS, INHERITS, USES];

/// The longest target name the book keeps: a name longer than this is not an
/// identifier, and storing it would only cost a row.
pub const MAX_NAME_CHARS: usize = 120;

/// One relation of a chapter, ready to be written to the `edge` table.
///
/// `src` is the index of the symbol that owns the relation inside
/// [`crate::lang::ParsedFile::symbols`], and `dst_name` is the target as the
/// source spells it: the book always keeps the text, and resolves it later by
/// the folded collation (`dst_id` stays `NULL` when the chapter is written).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedEdge {
    pub src: usize,
    pub kind: &'static str,
    pub dst_name: String,
}

/// One relation found in the source, before its owning symbol is known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawEdge {
    /// Byte where the construct starts: it decides the owning symbol.
    pub byte: usize,
    pub kind: &'static str,
    pub name: String,
}

/// Attaches every raw relation to the symbol that owns it and returns the
/// edges of the chapter, deduplicated and with no self-edge.
///
/// `anchor` is the symbol the grammar offers for the relations that belong to
/// no declaration (a file-scope `use` in Rust, a top-level `require` in the
/// languages of C2): it is appended to `symbols` **only when one of them needs
/// it**, so a file with no relations keeps the exact symbol list it declared.
pub fn attach(
    mut raw: Vec<RawEdge>,
    symbols: &mut Vec<ParsedSymbol>,
    anchor: ParsedSymbol,
) -> Vec<ParsedEdge> {
    let mut pending: Vec<(Option<usize>, &'static str, String)> = Vec::new();
    let mut needs_anchor = false;
    for edge in raw.drain(..) {
        let Some(name) = clean_name(&edge.name) else {
            continue;
        };
        match owner_of(symbols, edge.byte) {
            Some(index) => {
                // Rule 1: a symbol that names itself adds no connection.
                if symbols[index].name_norm == name_norm(&name) {
                    continue;
                }
                pending.push((Some(index), edge.kind, name));
            }
            None => {
                // Rule 2: a file-scope relation needs the module anchor.
                needs_anchor = true;
                pending.push((None, edge.kind, name));
            }
        }
    }

    if needs_anchor {
        symbols.push(anchor);
        let anchor = symbols.len() - 1;
        for entry in &mut pending {
            if entry.0.is_none() {
                entry.0 = Some(anchor);
            }
        }
    }

    let mut seen: BTreeSet<(usize, &'static str, String)> = BTreeSet::new();
    let mut edges: Vec<ParsedEdge> = Vec::with_capacity(pending.len());
    for (src, kind, dst_name) in pending {
        // A relation that reached here always has an owner: the anchor was
        // created above for the ones that did not.
        let Some(src) = src else { continue };
        if !seen.insert((src, kind, dst_name.clone())) {
            continue;
        }
        edges.push(ParsedEdge {
            src,
            kind,
            dst_name,
        });
    }
    edges
}

/// The innermost symbol whose span contains `byte`.
fn owner_of(symbols: &[ParsedSymbol], byte: usize) -> Option<usize> {
    let byte = u32::try_from(byte).ok()?;
    let mut best: Option<(u32, usize)> = None;
    for (index, symbol) in symbols.iter().enumerate() {
        if symbol.start_byte <= byte && byte < symbol.end_byte {
            let span = symbol.end_byte - symbol.start_byte;
            if best.is_none_or(|(best_span, _)| span <= best_span) {
                best = Some((span, index));
            }
        }
    }
    best.map(|(_, index)| index)
}

/// Normalises a target name: the last `::` segment, without generic arguments,
/// and only when it looks like an identifier.
pub fn clean_name(raw: &str) -> Option<String> {
    let text = raw.trim();
    let text = text.split('<').next().unwrap_or(text).trim();
    let segment = text.rsplit("::").next().unwrap_or(text).trim();
    if segment.is_empty() || segment.chars().count() > MAX_NAME_CHARS {
        return None;
    }
    if !segment
        .chars()
        .all(|char| char.is_alphanumeric() || char == '_')
    {
        return None;
    }
    // Keywords and crates that are never declarations of the book.
    if matches!(
        segment,
        "self" | "Self" | "crate" | "super" | "std" | "core" | "alloc"
    ) {
        return None;
    }
    Some(segment.to_string())
}

#[cfg(test)]
mod tests {
    use super::{clean_name, RELATION_KINDS};

    /// The target of an edge is the last segment of a path, without generic
    /// arguments, and only when it is an identifier: the book never stores a
    /// sentence as a name.
    #[test]
    fn a_target_name_is_the_last_identifier_segment() {
        assert_eq!(clean_name("a::b::render_with"), Some("render_with".into()));
        assert_eq!(clean_name("Vec<Config>"), Some("Vec".into()));
        assert_eq!(clean_name("  Path  "), Some("Path".into()));
        assert_eq!(clean_name("Self"), None);
        assert_eq!(clean_name("crate"), None);
        assert_eq!(clean_name(""), None);
        assert_eq!(clean_name("two words"), None);
        assert_eq!(clean_name("{}"), None);
    }

    /// The edge table is language-agnostic: the four kinds of S7 are the whole
    /// vocabulary, so a new grammar of C2 cannot need a schema change.
    #[test]
    fn the_vocabulary_of_the_edges_is_the_four_kinds() {
        assert_eq!(RELATION_KINDS, ["calls", "imports", "inherits", "uses"]);
    }
}
