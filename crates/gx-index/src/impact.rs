//! `code_impact`: the bounded walk of the syntactic edges (S7, D3).
//!
//! The BFS runs **in Rust**, not in a recursive CTE: SQLite documents no
//! iteration limit for `WITH RECURSIVE`, so the guard against a cycle is
//! explicit here (ideas §7, the queries explorer). Each level is a handful of
//! index lookups by `src_id` (out-hop) or by `dst_name` (in-hop), and the two
//! declared provisions are:
//!
//! - **depth ≤ 3** (a deeper walk is a different question, not a bigger answer),
//! - **budget 300 nodes / 600 edges**, the ceilings of the `graph` kind, so the
//!   answer always fits in the document that `code_diagram` builds from it.
//!
//! The edges are syntactic (D3): a target is a **name**, and a name resolves
//! only while it has at most [`MAX_NAME_FANOUT`] definers. A generic name with
//! a hundred definitions is left unresolved instead of fanning the answer out
//! into noise, and the walk says how many edges it could not resolve.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{Connection, OptionalExtension};

use crate::lang::name_norm;

/// The deepest walk the tool accepts, in hops.
pub const MAX_DEPTH: u32 = 3;

/// The node ceiling of an answer: the `max_nodes` of the `graph` kind.
pub const MAX_NODES: usize = 300;

/// The edge ceiling of an answer: the `max_edges` of the `graph` kind.
pub const MAX_EDGES: usize = 600;

/// How many definers a name may have and still resolve (ideas §10.1, «BFS»).
pub const MAX_NAME_FANOUT: usize = 5;

/// The largest `IN` list of one statement (the ideas §10.1 chunk of 500).
const IN_CHUNK: usize = 500;

/// Which edges the walk follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// The edges that point at the symbol: what a change would affect.
    In,
    /// The edges the symbol points at: what it reaches.
    Out,
    /// Both, which is the neighbourhood a diagram draws.
    Both,
}

impl Direction {
    /// The direction of a tool argument.
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "in" => Some(Direction::In),
            "out" => Some(Direction::Out),
            "both" => Some(Direction::Both),
            _ => None,
        }
    }

    /// The direction as the tool spells it.
    pub fn as_str(&self) -> &'static str {
        match self {
            Direction::In => "in",
            Direction::Out => "out",
            Direction::Both => "both",
        }
    }

    fn follows_out(&self) -> bool {
        matches!(self, Direction::Out | Direction::Both)
    }

    fn follows_in(&self) -> bool {
        matches!(self, Direction::In | Direction::Both)
    }
}

/// What the caller asks the walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImpactRequest {
    /// The `symbol.id` of the start.
    pub start: i64,
    /// Hops to follow: clamped to [`MAX_DEPTH`].
    pub depth: u32,
    pub direction: Direction,
}

/// One symbol the walk reached, with the hop that reached it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImpactNode {
    pub id: i64,
    pub kind: String,
    pub name: String,
    pub qname: String,
    pub file: String,
    pub line: u32,
    /// 0 for the start symbol, 1..=`depth` for the rest.
    pub depth: u32,
}

/// One edge of the walk, between two symbols of the answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImpactEdge {
    pub from: i64,
    pub to: i64,
    pub kind: String,
}

/// The answer of the walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Impact {
    /// The depth actually used: `request.depth` clamped to [`MAX_DEPTH`].
    pub depth: u32,
    /// The start symbol (depth 0) and everything it reached, shallow first.
    pub nodes: Vec<ImpactNode>,
    /// The edges between the nodes of the answer.
    pub edges: Vec<ImpactEdge>,
    /// Whether the walk hit a provision and stopped early.
    pub truncated: bool,
    /// Edges whose target name has no declaration, or too many to be one.
    pub unresolved: usize,
}

/// One hop of the walk, and which of its two ends is the symbol it discovers.
///
/// An out-hop discovers the target (the symbol it reaches); an in-hop
/// discovers the source (the symbol that points at the frontier). The walk
/// treats them the same way once this is said.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Hop {
    from: i64,
    kind: String,
    to: i64,
    discover: Endpoint,
}

/// The end of a hop the walk has not visited yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Endpoint {
    From,
    To,
}

/// Walks the edges from one symbol, or `None` when the start is not in the book.
pub fn impact(
    connection: &Connection,
    request: &ImpactRequest,
) -> rusqlite::Result<Option<Impact>> {
    let Some(start) = node(connection, request.start)? else {
        return Ok(None);
    };
    let depth = request.depth.clamp(1, MAX_DEPTH);
    let mut nodes: BTreeMap<i64, ImpactNode> = BTreeMap::new();
    nodes.insert(start.id, start.clone());
    let mut edges: Vec<ImpactEdge> = Vec::new();
    let mut seen_edges: BTreeSet<(i64, i64, String)> = BTreeSet::new();
    let mut unresolved = 0usize;
    let mut truncated = false;
    let mut frontier: Vec<ImpactNode> = vec![start];

    'levels: for level in 1..=depth {
        if frontier.is_empty() {
            break;
        }
        let mut hops: Vec<Hop> = Vec::new();
        if request.direction.follows_out() {
            let from_out = out_hops(connection, &frontier)?;
            unresolved += from_out.1;
            hops.extend(from_out.0);
        }
        if request.direction.follows_in() {
            hops.extend(in_hops(connection, &frontier)?);
        }

        let mut next: Vec<ImpactNode> = Vec::new();
        for hop in hops {
            // The cycle guard: the start is never a hop of itself, and an edge
            // already answered by this walk is not answered twice.
            if hop.from == hop.to || !seen_edges.insert((hop.from, hop.to, hop.kind.clone())) {
                continue;
            }
            let discovered = match hop.discover {
                Endpoint::To => hop.to,
                Endpoint::From => hop.from,
            };
            if !nodes.contains_key(&discovered) {
                if nodes.len() >= MAX_NODES {
                    truncated = true;
                    break 'levels;
                }
                let Some(found) = node(connection, discovered)? else {
                    // `ON DELETE SET NULL` left an edge whose target is gone:
                    // the walk says so instead of inventing a node.
                    unresolved += 1;
                    continue;
                };
                let found = ImpactNode {
                    depth: level,
                    ..found
                };
                nodes.insert(found.id, found.clone());
                next.push(found);
            }
            if edges.len() >= MAX_EDGES {
                truncated = true;
                break 'levels;
            }
            edges.push(ImpactEdge {
                from: hop.from,
                to: hop.to,
                kind: hop.kind,
            });
        }
        // A visited symbol is never expanded again: a cycle costs no hop.
        frontier = next;
    }

    let mut ordered: Vec<ImpactNode> = nodes.into_values().collect();
    ordered.sort_by(|left, right| {
        left.depth
            .cmp(&right.depth)
            .then_with(|| left.qname.cmp(&right.qname))
    });
    Ok(Some(Impact {
        depth,
        nodes: ordered,
        edges,
        truncated,
        unresolved,
    }))
}

/// One symbol as the walk needs it.
fn node(connection: &Connection, id: i64) -> rusqlite::Result<Option<ImpactNode>> {
    connection
        .query_row(
            "SELECT s.id, s.kind, s.name, s.qname, f.path, s.start_line
             FROM symbol s JOIN file f ON f.id = s.file_id WHERE s.id = ?1",
            [id],
            |row| {
                Ok(ImpactNode {
                    id: row.get(0)?,
                    kind: row.get(1)?,
                    name: row.get(2)?,
                    qname: row.get(3)?,
                    file: row.get(4)?,
                    line: row.get::<_, i64>(5)? as u32,
                    depth: 0,
                })
            },
        )
        .optional()
}

/// The out-hops of one level: the targets of the edges of the frontier,
/// resolved by the folded collation.
fn out_hops(
    connection: &Connection,
    frontier: &[ImpactNode],
) -> rusqlite::Result<(Vec<Hop>, usize)> {
    let ids: Vec<i64> = frontier.iter().map(|node| node.id).collect();
    let mut pending: Vec<(i64, String, Option<i64>, String)> = Vec::new();
    for chunk in ids.chunks(IN_CHUNK) {
        let sql = format!(
            "SELECT src_id, kind, dst_id, dst_name FROM edge WHERE src_id IN ({})",
            placeholders(chunk.len())
        );
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map(rusqlite::params_from_iter(chunk.iter()), |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        for row in rows {
            pending.push(row?);
        }
    }

    // One batch of names per level, never one query per edge.
    let names: BTreeSet<String> = pending
        .iter()
        .filter(|(_, _, dst_id, _)| dst_id.is_none())
        .map(|(_, _, _, dst_name)| name_norm(dst_name))
        .collect();
    let names: Vec<String> = names.into_iter().collect();
    let resolved = resolve_names(connection, &names)?;

    let mut hops = Vec::new();
    let mut unresolved = 0usize;
    for (from, kind, dst_id, dst_name) in pending {
        match dst_id {
            // A resolved edge is used as it is: a later stage may write it.
            Some(to) => hops.push(Hop {
                from,
                kind,
                to,
                discover: Endpoint::To,
            }),
            None => match resolved.get(&name_norm(&dst_name)) {
                Some(targets) => {
                    for target in targets {
                        hops.push(Hop {
                            from,
                            kind: kind.clone(),
                            to: *target,
                            discover: Endpoint::To,
                        });
                    }
                }
                None => unresolved += 1,
            },
        }
    }
    Ok((hops, unresolved))
}

/// The in-hops of one level: the edges whose textual target names the
/// frontier. The join runs through `name_norm` on the Rust side (delta 6), so
/// a name found by `code_search` resolves the same way here.
fn in_hops(connection: &Connection, frontier: &[ImpactNode]) -> rusqlite::Result<Vec<Hop>> {
    let mut by_norm: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    let mut wanted: BTreeSet<String> = BTreeSet::new();
    for node in frontier {
        by_norm.entry(node.name_norm()).or_default().push(node.id);
        // The edge keeps the target as the source spells it, so both the name
        // and its folded form are worth an index lookup.
        wanted.insert(node.name.clone());
        wanted.insert(node.name_norm());
    }
    let wanted: Vec<String> = wanted.into_iter().collect();

    let mut hops = Vec::new();
    for chunk in wanted.chunks(IN_CHUNK) {
        let sql = format!(
            "SELECT src_id, kind, dst_name FROM edge WHERE dst_name IN ({})",
            placeholders(chunk.len())
        );
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map(rusqlite::params_from_iter(chunk.iter()), |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (from, kind, dst_name) = row?;
            if let Some(targets) = by_norm.get(&name_norm(&dst_name)) {
                for target in targets {
                    hops.push(Hop {
                        from,
                        kind: kind.clone(),
                        to: *target,
                        discover: Endpoint::From,
                    });
                }
            }
        }
    }
    Ok(hops)
}

/// The definers of every name, dropping the names with more than
/// [`MAX_NAME_FANOUT`] of them.
fn resolve_names(
    connection: &Connection,
    names: &[String],
) -> rusqlite::Result<BTreeMap<String, Vec<i64>>> {
    let mut resolved: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    for chunk in names.chunks(IN_CHUNK) {
        let sql = format!(
            "SELECT id, name_norm FROM symbol WHERE name_norm IN ({})",
            placeholders(chunk.len())
        );
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map(rusqlite::params_from_iter(chunk.iter()), |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (id, norm) = row?;
            let targets = resolved.entry(norm).or_default();
            if targets.len() <= MAX_NAME_FANOUT {
                targets.push(id);
            }
        }
    }
    // The name that grew past the fan-out is a generic name: it stops being an
    // answer at all instead of answering a hundred definitions.
    resolved.retain(|_, targets| targets.len() <= MAX_NAME_FANOUT);
    Ok(resolved)
}

/// `?, ?, …` for a dynamic `IN` list. The values are always bound, never
/// interpolated.
fn placeholders(count: usize) -> String {
    std::iter::repeat_n("?", count)
        .collect::<Vec<_>>()
        .join(",")
}

/// The folded collation of a node's name, the same one the book indexes.
trait NameNorm {
    fn name_norm(&self) -> String;
}

impl NameNorm for ImpactNode {
    fn name_norm(&self) -> String {
        name_norm(&self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::{placeholders, Direction, MAX_DEPTH, MAX_EDGES, MAX_NODES};

    #[test]
    fn a_direction_is_parsed_from_its_three_names() {
        for (text, expected) in [
            ("in", Direction::In),
            ("OUT", Direction::Out),
            (" both ", Direction::Both),
        ] {
            assert_eq!(Direction::parse(text), Some(expected), "`{text}`");
        }
        assert_eq!(Direction::parse("sideways"), None);
        assert_eq!(Direction::parse(""), None);
        assert_eq!(Direction::In.as_str(), "in");
    }

    /// S7: the provisions are the ceilings of the `graph` kind, and the depth
    /// is the one the doc fixes.
    #[test]
    fn the_provisions_are_the_ones_the_document_declares() {
        assert_eq!(MAX_DEPTH, 3);
        assert_eq!((MAX_NODES, MAX_EDGES), (300, 600));
        assert_eq!(placeholders(3), "?,?,?");
    }
}
