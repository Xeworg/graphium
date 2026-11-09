//! `code_impact`: the bounded walk of the syntactic edges (S7, D3, T5).
//!
//! The BFS runs **in Rust**, not in a recursive CTE: SQLite documents no
//! iteration limit for `WITH RECURSIVE`, so the guard against a cycle is
//! explicit here (ideas §7, the queries explorer). Each level is a handful of
//! index lookups by `src_id` (out-hop) or by `dst_name` (in-hop), and the
//! declared provisions are:
//!
//! - **depth ≤ 3** (a deeper walk is a different question, not a bigger answer),
//! - a **budget** of nodes and edges that the caller owns: `code_impact` has
//!   its own query budget (600 nodes / 1200 edges by default, ~2000 at most,
//!   T5) and `code_diagram` keeps the 300/600 ceilings of the `graph` kind,
//!   because its answer is a document that has to render.
//!
//! The edges are syntactic (D3): a target is a **name**, and a name resolves
//! only while it has at most [`MAX_NAME_FANOUT`] definers. A generic name with
//! a hundred definitions is left unresolved instead of fanning the answer out
//! into noise, and the walk says how many edges it could not resolve.
//!
//! A walk may start at one symbol or at **every declaration of one chapter**
//! (the file seed of T5, «the file as a node»). The chapter walk also answers
//! the connections grouped by file: how many edges leave the chapter towards
//! each file, how many arrive from it, and how many declarations it has.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{Connection, OptionalExtension};

use crate::cut::{CutReason, Truncation};
use crate::lang::name_norm;

/// The deepest walk the tool accepts, in hops.
pub const MAX_DEPTH: u32 = 3;

/// The node ceiling of a `graph` document: the `max_nodes` of the kind.
pub const MAX_NODES: usize = 300;

/// The edge ceiling of a `graph` document: the `max_edges` of the kind.
pub const MAX_EDGES: usize = 600;

/// The node budget of `code_impact` when the agent does not say (S7, L7).
pub const DEFAULT_QUERY_NODES: usize = 600;

/// The ceiling of any query budget: past this the answer is no longer a
/// summary of the code (~25k tokens is the envelope of an agent window).
pub const MAX_QUERY_NODES: usize = 2000;

/// How many definers a name may have and still resolve (ideas §10.1, «BFS»).
pub const MAX_NAME_FANOUT: usize = 5;

/// The largest `IN` list of one statement (the ideas §10.1 chunk of 500).
const IN_CHUNK: usize = 500;

/// The limits of one walk, in nodes and edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalkLimits {
    pub max_nodes: usize,
    pub max_edges: usize,
}

impl WalkLimits {
    /// The ceilings of the `graph` kind: what a renderable document holds.
    pub const GRAPH: Self = Self {
        max_nodes: MAX_NODES,
        max_edges: MAX_EDGES,
    };

    /// The budget of a query (T5): the nodes asked for, [`DEFAULT_QUERY_NODES`]
    /// when absent and clamped to [`MAX_QUERY_NODES`], with the edges at twice
    /// the nodes — the same 2:1 the `graph` kind uses.
    pub fn query(budget: Option<usize>) -> Self {
        let max_nodes = budget
            .unwrap_or(DEFAULT_QUERY_NODES)
            .clamp(1, MAX_QUERY_NODES);
        Self {
            max_nodes,
            max_edges: max_nodes.saturating_mul(2),
        }
    }
}

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

/// Where the walk starts: one symbol, or a whole chapter (T5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seed {
    /// One declaration starts the walk.
    Symbol(i64),
    /// Every declaration of one chapter starts the walk: the file as a node.
    Chapter(Vec<i64>),
}

/// What the caller asks the walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImpactRequest {
    pub seed: Seed,
    /// Hops to follow: clamped to [`MAX_DEPTH`].
    pub depth: u32,
    pub direction: Direction,
    /// The edge kinds the walk follows; empty follows the whole vocabulary.
    pub edges: Vec<String>,
    pub limits: WalkLimits,
}

impl ImpactRequest {
    /// A walk from one symbol with the ceilings of a `graph` document.
    pub fn symbol(id: i64, depth: u32, direction: Direction) -> Self {
        Self {
            seed: Seed::Symbol(id),
            depth,
            direction,
            edges: Vec::new(),
            limits: WalkLimits::GRAPH,
        }
    }

    /// A walk from every declaration of one chapter (T5).
    pub fn chapter(starts: Vec<i64>, depth: u32, direction: Direction) -> Self {
        Self {
            seed: Seed::Chapter(starts),
            depth,
            direction,
            edges: Vec::new(),
            limits: WalkLimits::GRAPH,
        }
    }

    /// The walk follows only these edge kinds.
    pub fn with_edges(mut self, edges: Vec<String>) -> Self {
        self.edges = edges;
        self
    }

    /// The walk answers within these limits.
    pub fn with_limits(mut self, limits: WalkLimits) -> Self {
        self.limits = limits;
        self
    }
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
    /// Why the answer is a slice, when it is one.
    pub truncated: Option<Truncation>,
    /// Edges whose target name has no declaration, or too many to be one.
    pub unresolved: usize,
}

/// One file connected to a chapter seed (T5): the connections view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileLink {
    pub file: String,
    /// Edges from a declaration of the seed chapter into this file.
    pub imports_out: usize,
    /// Edges from this file into the seed chapter.
    pub referenced_by_in: usize,
    /// Declarations the book has in this file.
    pub symbols: usize,
}

/// The answer of a chapter walk: the same shape as the symbol walk plus the
/// grouping of the connected files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileImpact {
    /// The chapter the walk started from.
    pub file: String,
    pub impact: Impact,
    /// The connected files, by path.
    pub files: Vec<FileLink>,
}

/// What a chapter seed asks: the same knobs of a walk, minus the seed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileImpactRequest {
    pub depth: u32,
    pub direction: Direction,
    pub edges: Vec<String>,
    pub limits: WalkLimits,
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

/// Which budget stopped the walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CutKind {
    Nodes,
    Edges,
    Depth,
}

/// Walks the edges from one symbol or one chapter, or `None` when the start is
/// not in the book.
pub fn impact(
    connection: &Connection,
    request: &ImpactRequest,
) -> rusqlite::Result<Option<Impact>> {
    let depth = request.depth.clamp(1, MAX_DEPTH);
    let follow = KindFilter::new(&request.edges);
    let limits = request.limits;
    let mut nodes: BTreeMap<i64, ImpactNode> = BTreeMap::new();
    let mut frontier: Vec<ImpactNode> = Vec::new();
    let mut extra_nodes: BTreeSet<i64> = BTreeSet::new();
    let mut cut: Option<CutKind> = None;

    let starts: Vec<i64> = match &request.seed {
        Seed::Symbol(id) => vec![*id],
        Seed::Chapter(ids) => ids.clone(),
    };
    if let Seed::Symbol(id) = request.seed {
        if node(connection, id)?.is_none() {
            return Ok(None);
        }
    }
    // The frontier of a chapter seed is every declaration of it; a chapter
    // bigger than the budget shows the first declarations and says so.
    for id in starts {
        if nodes.contains_key(&id) {
            continue;
        }
        if nodes.len() >= limits.max_nodes {
            cut = Some(CutKind::Nodes);
            extra_nodes.insert(id);
            continue;
        }
        let Some(found) = node(connection, id)? else {
            // `ON DELETE SET NULL` left an edge whose target is gone: the walk
            // says so instead of inventing a node.
            continue;
        };
        nodes.insert(found.id, found.clone());
        frontier.push(found);
    }

    let mut edges: Vec<ImpactEdge> = Vec::new();
    let mut seen_edges: BTreeSet<(i64, i64, String)> = BTreeSet::new();
    let mut unresolved = 0usize;
    let mut extra_edges = 0usize;

    'levels: for level in 1..=depth {
        if frontier.is_empty() || cut.is_some() {
            break;
        }
        let mut hops: Vec<Hop> = Vec::new();
        if request.direction.follows_out() {
            let (out, missing) = out_hops(connection, &frontier, &follow)?;
            unresolved += missing;
            hops.extend(out);
        }
        if request.direction.follows_in() {
            hops.extend(in_hops(connection, &frontier, &follow)?);
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
            if cut.is_none() {
                if !nodes.contains_key(&discovered) && nodes.len() >= limits.max_nodes {
                    cut = Some(CutKind::Nodes);
                } else if edges.len() >= limits.max_edges {
                    cut = Some(CutKind::Edges);
                }
            }
            match cut {
                None => {
                    if !nodes.contains_key(&discovered) {
                        let Some(found) = node(connection, discovered)? else {
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
                    edges.push(ImpactEdge {
                        from: hop.from,
                        to: hop.to,
                        kind: hop.kind,
                    });
                }
                // The budget bit: the answer keeps what it holds and counts
                // the rest of the level, so `total` is a number and not a
                // guess at an unbounded graph.
                Some(_) => {
                    if !nodes.contains_key(&discovered) {
                        extra_nodes.insert(discovered);
                    }
                    extra_edges += 1;
                }
            }
        }
        // A visited symbol is never expanded again: a cycle costs no hop.
        frontier = next;
        if cut.is_some() {
            break 'levels;
        }
    }

    // The hop cap is a cut only when the walk still had somewhere to go: the
    // answer of a walk that exhausted the graph is complete at three hops.
    if cut.is_none() && request.depth > MAX_DEPTH && !frontier.is_empty() {
        let mut hops: Vec<Hop> = Vec::new();
        if request.direction.follows_out() {
            hops.extend(out_hops(connection, &frontier, &follow)?.0);
        }
        if request.direction.follows_in() {
            hops.extend(in_hops(connection, &frontier, &follow)?);
        }
        let more = hops.iter().any(|hop| {
            if hop.from == hop.to || seen_edges.contains(&(hop.from, hop.to, hop.kind.clone())) {
                return false;
            }
            let discovered = match hop.discover {
                Endpoint::To => hop.to,
                Endpoint::From => hop.from,
            };
            !nodes.contains_key(&discovered)
        });
        if more {
            cut = Some(CutKind::Depth);
        }
    }

    let truncated = match cut {
        Some(CutKind::Nodes) => Some(Truncation::new(
            CutReason::Nodes,
            nodes.len(),
            Some(nodes.len() + extra_nodes.len()),
        )),
        Some(CutKind::Edges) => Some(Truncation::new(
            CutReason::Edges,
            edges.len(),
            Some(edges.len() + extra_edges),
        )),
        Some(CutKind::Depth) => Some(Truncation::new(CutReason::Depth, nodes.len(), None)),
        None => None,
    };

    let mut ordered: Vec<ImpactNode> = nodes.into_values().collect();
    // The answer is ranked by **relevance**, not by the alphabet (L7): the hop
    // that reached a node first, then the distinct edges of this answer that
    // touch it. A capped hub answer leads with its most connected neighbours,
    // which is what makes the character envelope's cliff a ranking and not an
    // arbitrary slice.
    let mut degree: BTreeMap<i64, usize> = BTreeMap::new();
    for edge in &edges {
        *degree.entry(edge.from).or_default() += 1;
        *degree.entry(edge.to).or_default() += 1;
    }
    ordered.sort_by(|left, right| {
        left.depth
            .cmp(&right.depth)
            .then_with(|| {
                degree
                    .get(&right.id)
                    .copied()
                    .unwrap_or_default()
                    .cmp(&degree.get(&left.id).copied().unwrap_or_default())
            })
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

/// Walks from every declaration of one chapter (T5), and groups the
/// connections by file.
///
/// `None` when the path is not a chapter of the book, so the tool answers
/// `invalid_value` instead of an empty neighbourhood that reads as «this file
/// has no connections».
pub fn file_impact(
    connection: &Connection,
    path: &str,
    request: &FileImpactRequest,
) -> rusqlite::Result<Option<FileImpact>> {
    let Some(starts) = chapter_symbols(connection, path)? else {
        return Ok(None);
    };
    let walk = ImpactRequest {
        seed: Seed::Chapter(starts),
        depth: request.depth,
        direction: request.direction,
        edges: request.edges.clone(),
        limits: request.limits,
    };
    let Some(impact) = impact(connection, &walk)? else {
        return Ok(None);
    };
    let files = group_files(connection, path, &impact.nodes, &impact.edges)?;
    Ok(Some(FileImpact {
        file: path.to_string(),
        impact,
        files,
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

/// Every declaration of one chapter, in span order, or `None` for a path the
/// book does not have.
fn chapter_symbols(connection: &Connection, path: &str) -> rusqlite::Result<Option<Vec<i64>>> {
    let file: Option<i64> = connection
        .query_row("SELECT id FROM file WHERE path = ?1", [path], |row| {
            row.get(0)
        })
        .optional()?;
    let Some(file_id) = file else {
        return Ok(None);
    };
    let mut statement =
        connection.prepare("SELECT id FROM symbol WHERE file_id = ?1 ORDER BY start_byte, id")?;
    let ids = statement
        .query_map([file_id], |row| row.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(ids))
}

/// How many declarations the book has in each of `files`.
fn symbol_counts(
    connection: &Connection,
    files: &[String],
) -> rusqlite::Result<BTreeMap<String, usize>> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for chunk in files.chunks(IN_CHUNK) {
        let sql = format!(
            "SELECT f.path, COUNT(s.id) FROM symbol s JOIN file f ON f.id = s.file_id
             WHERE f.path IN ({}) GROUP BY f.path",
            placeholders(chunk.len())
        );
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map(rusqlite::params_from_iter(chunk.iter()), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
        })?;
        for row in rows {
            let (path, count) = row?;
            counts.insert(path, count);
        }
    }
    Ok(counts)
}

/// The connections of a chapter grouped by the file at the other end (T5).
///
/// `imports_out` counts the edges that leave the chapter towards the file and
/// `referenced_by_in` the ones that arrive from it; an edge inside the chapter
/// is not a link to another file and stays out of the grouping.
fn group_files(
    connection: &Connection,
    seed_file: &str,
    nodes: &[ImpactNode],
    edges: &[ImpactEdge],
) -> rusqlite::Result<Vec<FileLink>> {
    let file_of: BTreeMap<i64, &str> = nodes
        .iter()
        .map(|node| (node.id, node.file.as_str()))
        .collect();
    let mut links: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for edge in edges {
        let (Some(from), Some(to)) = (file_of.get(&edge.from), file_of.get(&edge.to)) else {
            continue;
        };
        if *from == seed_file && *to != seed_file {
            links.entry((*to).to_string()).or_default().0 += 1;
        }
        if *to == seed_file && *from != seed_file {
            links.entry((*from).to_string()).or_default().1 += 1;
        }
    }
    let files: Vec<String> = links.keys().cloned().collect();
    let symbols = symbol_counts(connection, &files)?;
    Ok(links
        .into_iter()
        .map(|(file, (imports_out, referenced_by_in))| FileLink {
            symbols: symbols.get(&file).copied().unwrap_or_default(),
            file,
            imports_out,
            referenced_by_in,
        })
        .collect())
}

/// The out-hops of one level: the targets of the edges of the frontier,
/// resolved by the folded collation.
fn out_hops(
    connection: &Connection,
    frontier: &[ImpactNode],
    follow: &KindFilter<'_>,
) -> rusqlite::Result<(Vec<Hop>, usize)> {
    let ids: Vec<i64> = frontier.iter().map(|node| node.id).collect();
    let mut pending: Vec<(i64, String, Option<i64>, String)> = Vec::new();
    for chunk in ids.chunks(IN_CHUNK) {
        let sql = format!(
            "SELECT src_id, kind, dst_id, dst_name FROM edge WHERE src_id IN ({}){}",
            placeholders(chunk.len()),
            follow.sql()
        );
        let mut statement = connection.prepare(&sql)?;
        let params = follow.params(chunk);
        let rows = statement.query_map(params.as_slice(), |row| {
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
fn in_hops(
    connection: &Connection,
    frontier: &[ImpactNode],
    follow: &KindFilter<'_>,
) -> rusqlite::Result<Vec<Hop>> {
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
            "SELECT src_id, kind, dst_name FROM edge WHERE dst_name IN ({}){}",
            placeholders(chunk.len()),
            follow.sql()
        );
        let mut statement = connection.prepare(&sql)?;
        let params = follow.params(chunk);
        let rows = statement.query_map(params.as_slice(), |row| {
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

/// The edge kinds a walk follows (T5): empty is the whole vocabulary.
struct KindFilter<'a> {
    kinds: &'a [String],
}

impl<'a> KindFilter<'a> {
    fn new(kinds: &'a [String]) -> Self {
        Self { kinds }
    }

    /// The `AND kind IN (…)` of the filter, or nothing when it follows all.
    fn sql(&self) -> String {
        if self.kinds.is_empty() {
            String::new()
        } else {
            format!(" AND kind IN ({})", placeholders(self.kinds.len()))
        }
    }

    /// The bound parameters: the `IN` list of the level, then the kinds.
    fn params<'c, T>(&'c self, chunk: &'c [T]) -> Vec<&'c dyn rusqlite::ToSql>
    where
        T: rusqlite::ToSql,
    {
        let mut params: Vec<&dyn rusqlite::ToSql> =
            Vec::with_capacity(chunk.len() + self.kinds.len());
        for value in chunk {
            params.push(value);
        }
        for kind in self.kinds {
            params.push(kind);
        }
        params
    }
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
    use super::{
        placeholders, Direction, WalkLimits, DEFAULT_QUERY_NODES, MAX_DEPTH, MAX_EDGES, MAX_NODES,
        MAX_QUERY_NODES,
    };
    use crate::relations::RELATION_KINDS;

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

    /// T5/S7: the query budget is the agent's, not the document's: 600 nodes by
    /// default, 2000 at most, with the edges at twice the nodes.
    #[test]
    fn the_query_budget_is_its_own_and_has_a_ceiling() {
        let default = WalkLimits::query(None);
        assert_eq!(default.max_nodes, DEFAULT_QUERY_NODES);
        assert_eq!(default.max_edges, DEFAULT_QUERY_NODES * 2);
        let huge = WalkLimits::query(Some(usize::MAX));
        assert_eq!(huge.max_nodes, MAX_QUERY_NODES);
        assert_eq!(huge.max_edges, MAX_QUERY_NODES * 2);
        assert_eq!(WalkLimits::query(Some(0)).max_nodes, 1);
        assert_eq!(WalkLimits::GRAPH.max_nodes, MAX_NODES);
        assert_eq!(WalkLimits::GRAPH.max_edges, MAX_EDGES);
    }

    /// T5: the filter speaks the four kinds of the schema and nothing else.
    #[test]
    fn the_kinds_of_the_filter_are_the_four_relation_kinds() {
        assert_eq!(RELATION_KINDS, ["calls", "imports", "inherits", "uses"]);
        let calls = [String::from("calls")];
        let follow = super::KindFilter::new(&calls);
        assert_eq!(follow.sql(), " AND kind IN (?)");
        assert!(super::KindFilter::new(&[]).sql().is_empty());
    }
}
