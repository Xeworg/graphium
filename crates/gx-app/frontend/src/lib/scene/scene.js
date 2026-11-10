// The door from the wire to the painter (`f11-elementos.md`, T2; S3, S4).
//
// `sceneFromPayload(payload)` takes a `graphium://render` payload and answers
// the scene the plan mounts, or `null` when the payload carries no usable
// scene. Since the flip of T3/C the scene is the only shape of the wire: the
// `{@html}` branch of the string era is gone, so `null` means "there is
// nothing this painter can mount" and the page answers the render as not
// painted instead of painting a broken drawing.
//
// Three rules decide this module:
//
// - **It is pure and it never throws** (S3): no DOM, no clock, no measuring.
//   A payload the painter cannot trust is `null`, never an exception in the
//   middle of a render, because a thrown scene would take the whole plan down.
// - **It invents nothing** (S3): every field it returns is a field of the
//   scene Rust built (`gx-core/src/scene.rs`), with the ids and the numbers
//   untouched. The one thing it adds is `bands[].kind`, which is *read* from
//   the id the drawing already uses (`cluster-`/`lane-`): the two families of
//   band are two layers, and the painter needs to know which one it holds.
// - **It measures nothing** (S4): `label` arrives pre-broken with `\n`, and
//   `center`/`size` arrive measured. Nothing here asks the browser anything.
//
// What counts as "odd", in two tiers:
//
// 1. **Structural**: no canvas, no tokens, no measured box, an id that is not
//    an id … the whole scene is unusable, so the answer is `null` and the
//    string branch paints (or nothing, when the payload has no string either).
// 2. **Version skew**: an unknown `shape`, `emphasis` or `style`. The drawing
//    is still usable, so the value falls back to the neutral one (`box`,
//    `normal`, `solid`) instead of blanking the plan over a word the frontend
//    does not know yet.
//
// An edge whose route has fewer than two points is dropped, not the scene: it
// is the same rule the emitter applies (`renderer.rs` draws no line for an
// empty route), so the painter draws exactly the edges the export would.

/** The shapes of `gx-core/src/ir.rs`. */
const SHAPES = ["box", "rounded", "cylinder", "diamond", "ellipse", "note"];

/** The shape of a node, or the loud end of the run: a shape the painter does
 * not know never paints as a plausible box (`f19-formas.md`, S5). */
function shapeOf(shape) {
  if (!SHAPES.includes(shape)) {
    throw new Error(`the scene names a shape the painter does not know: ${shape}`);
  }
  return shape;
}
/** The emphases of `gx-core/src/ir.rs`. */
const EMPHASES = ["normal", "highlight", "muted"];
/** The key roles of a row. */
const ROW_KEYS = ["none", "pk", "fk", "pk_fk"];
/** The bounds of a relation. */
const BOUNDS = ["optional", "required"];
/** The maxima of a relation. */
const MAXIMA = ["one", "many"];

/** The tokens that are a single number (`SceneTokens`). */
const NUMBER_TOKENS = [
  "text_size",
  "node_padding",
  "corner_radius",
  "border_width",
  "line_height",
  "regular_weight",
  "semibold_weight",
  "arrow_length",
  "crow_foot",
  "crow_foot_spread",
  "badge_width",
  "muted_opacity",
];
/** The tokens that are a dash pattern of two numbers. */
const DASH_TOKENS = ["lifeline_dash", "dash_pattern", "group_dash"];

// --- small readers ---------------------------------------------------------

const isRecord = (value) =>
  typeof value === "object" && value !== null && !Array.isArray(value);
const isNumber = (value) => typeof value === "number" && Number.isFinite(value);
const isText = (value) => typeof value === "string";
const isId = (value) => isText(value) && value !== "";

/** A point of the drawing, or `null` when the payload does not carry one. */
function point(raw) {
  if (!isRecord(raw) || !isNumber(raw.x) || !isNumber(raw.y)) {
    return null;
  }
  return { x: raw.x, y: raw.y };
}

/** A measured size, or `null`: the painter never computes one (S4). */
function size(raw) {
  if (!isRecord(raw) || !isNumber(raw.w) || !isNumber(raw.h) || raw.w <= 0 || raw.h <= 0) {
    return null;
  }
  return { w: raw.w, h: raw.h };
}

/** The rectangle of the canvas or of a band: a rectangle has no "empty". */
function box(raw) {
  if (
    !isRecord(raw) ||
    !isNumber(raw.x) ||
    !isNumber(raw.y) ||
    !isNumber(raw.w) ||
    !isNumber(raw.h) ||
    raw.w <= 0 ||
    raw.h <= 0
  ) {
    return null;
  }
  return { x: raw.x, y: raw.y, w: raw.w, h: raw.h };
}

// --- the pieces of the scene ----------------------------------------------

/** The frozen numbers of the drawing, all of them or nothing (S5). */
function tokensOf(raw) {
  if (!isRecord(raw)) {
    return null;
  }
  const tokens = {};
  for (const key of NUMBER_TOKENS) {
    if (!isNumber(raw[key])) {
      return null;
    }
    tokens[key] = raw[key];
  }
  for (const key of DASH_TOKENS) {
    const dash = raw[key];
    if (!Array.isArray(dash) || dash.length !== 2 || !dash.every(isNumber)) {
      return null;
    }
    tokens[key] = [dash[0], dash[1]];
  }
  return tokens;
}

/** Where the paint takes its colours from: the CSS variables of the page. */
function paletteOf(raw) {
  if (raw === undefined || raw === null) {
    return null;
  }
  if (!isRecord(raw)) {
    return null;
  }
  const palette = { source: isText(raw.source) ? raw.source : "", theme: isText(raw.theme) ? raw.theme : "" };
  for (const slot of ["bg", "node_fill", "node_stroke", "node_text", "edge_stroke", "edge_text", "accent"]) {
    palette[slot] = isText(raw[slot]) ? raw[slot] : "";
  }
  return palette;
}

/** The rows of a table node, or `null` when one of them is not a row. */
function rowsOf(raw) {
  if (raw === undefined) {
    return [];
  }
  if (!Array.isArray(raw)) {
    return null;
  }
  const rows = [];
  for (const row of raw) {
    if (!isRecord(row) || !isText(row.text)) {
      return null;
    }
    rows.push({
      text: row.text,
      note: isText(row.note) ? row.note : null,
      key: ROW_KEYS.includes(row.key) ? row.key : "none",
      nullable: row.nullable === true,
    });
  }
  return rows;
}

/** One node of the graph, or `null` when its box never arrived (S4). */
function nodeOf(raw) {
  if (!isRecord(raw) || !isId(raw.id) || !isText(raw.label)) {
    return null;
  }
  const center = point(raw.center);
  const measured = size(raw.size);
  const rows = rowsOf(raw.rows);
  if (center === null || measured === null || rows === null) {
    return null;
  }
  return {
    id: raw.id,
    label: raw.label,
    shape: shapeOf(raw.shape),
    emphasis: EMPHASES.includes(raw.emphasis) ? raw.emphasis : "normal",
    style: raw.style === "dashed" ? "dashed" : "solid",
    center,
    size: measured,
    rows,
    meta: isRecord(raw.meta) ? raw.meta : {},
  };
}

/** The crow's foot of an edge, when the kind has one. */
function cardinalityOf(raw) {
  if (raw === undefined || raw === null) {
    return null;
  }
  if (
    !isRecord(raw) ||
    !BOUNDS.includes(raw.from_min) ||
    !MAXIMA.includes(raw.from_max) ||
    !BOUNDS.includes(raw.to_min) ||
    !MAXIMA.includes(raw.to_max)
  ) {
    return undefined;
  }
  return {
    from_min: raw.from_min,
    from_max: raw.from_max,
    to_min: raw.to_min,
    to_max: raw.to_max,
  };
}

/**
 * One routed edge, `null` when it is broken, or `"skip"` when the route has no
 * line to draw (the rule of the emitter).
 */
function edgeOf(raw) {
  if (!isRecord(raw) || !isId(raw.id) || !isText(raw.from) || !isText(raw.to)) {
    return null;
  }
  if (!Array.isArray(raw.points)) {
    return null;
  }
  const points = [];
  for (const candidate of raw.points) {
    const at = point(candidate);
    if (at === null) {
      return null;
    }
    points.push(at);
  }
  const cardinality = cardinalityOf(raw.cardinality);
  if (cardinality === undefined) {
    return null;
  }
  const labelAt = raw.label_at === undefined || raw.label_at === null ? null : point(raw.label_at);
  if (raw.label_at !== undefined && raw.label_at !== null && labelAt === null) {
    return null;
  }
  if (points.length < 2) {
    return "skip";
  }
  return {
    id: raw.id,
    from: raw.from,
    to: raw.to,
    label: isText(raw.label) ? raw.label : null,
    emphasis: EMPHASES.includes(raw.emphasis) ? raw.emphasis : "normal",
    style: raw.style === "dashed" ? "dashed" : "solid",
    arrow: raw.arrow === true,
    from_row: isNumber(raw.from_row) ? raw.from_row : null,
    to_row: isNumber(raw.to_row) ? raw.to_row : null,
    cardinality,
    points,
    label_at: labelAt,
  };
}

/**
 * One band of the drawing: a container of a graph or a lane of a timeline.
 *
 * The id says which one it is (`cluster-<id>` / `lane-<id>`, S3), so `kind` is
 * read and never guessed, and a band with an id that names neither is not a
 * band the painter can place.
 */
function bandOf(raw) {
  if (!isRecord(raw) || !isId(raw.id)) {
    return null;
  }
  const kind = raw.id.startsWith("cluster-")
    ? "cluster"
    : raw.id.startsWith("lane-")
      ? "lane"
      : null;
  const bbox = box(raw.bbox);
  if (kind === null || bbox === null) {
    return null;
  }
  return {
    id: raw.id,
    kind,
    label: isText(raw.label) ? raw.label : null,
    bbox,
    depth: isNumber(raw.depth) ? raw.depth : 0,
    members: Array.isArray(raw.members) ? raw.members.filter(isText) : [],
    gutter: isNumber(raw.gutter) ? raw.gutter : null,
  };
}

/** The time span of a bar: the stretch of time an event covers. */
function spanOf(raw) {
  if (!isRecord(raw) || !isNumber(raw.start) || !isNumber(raw.end)) {
    return null;
  }
  return { start: raw.start, end: raw.end };
}

/**
 * The bar of an event that lasts, when the payload carries one.
 *
 * The four numbers are the geometry `gx-render-svg/src/geometry.rs` decided, so
 * the painter draws them instead of recomputing the length of the box (S5).
 * A bar that is not a rectangle is a value this frontend does not know yet:
 * the event keeps its box and loses the mark, never the whole drawing.
 */
function barOf(raw) {
  if (raw === undefined || raw === null) {
    return null;
  }
  if (
    !isRecord(raw) ||
    !isNumber(raw.x) ||
    !isNumber(raw.y) ||
    !isNumber(raw.w) ||
    !isNumber(raw.h)
  ) {
    return null;
  }
  return { x: raw.x, y: raw.y, w: raw.w, h: raw.h };
}

/** One event of a timeline: `IrEvent` plus its measured box (S3, S4). */
function eventOf(raw) {
  if (!isRecord(raw) || !isId(raw.id) || !isText(raw.lane) || !isText(raw.label)) {
    return null;
  }
  const center = point(raw.center);
  const measured = size(raw.size);
  if (center === null || measured === null) {
    return null;
  }
  return {
    id: raw.id,
    lane: raw.lane,
    label: raw.label,
    order: isNumber(raw.order) ? raw.order : 0,
    span: spanOf(raw.span),
    center,
    size: measured,
    bar: barOf(raw.bar),
  };
}

/** A collection that must be a list of good elements, or nothing at all. */
function listOf(raw, read) {
  if (raw === undefined) {
    return [];
  }
  if (!Array.isArray(raw)) {
    return null;
  }
  const out = [];
  for (const entry of raw) {
    const value = read(entry);
    if (value === null) {
      return null;
    }
    if (value === "skip") {
      continue;
    }
    out.push(value);
  }
  return out;
}

// --- the door --------------------------------------------------------------

/**
 * The scene of an `elements` payload, or `null` when it is not one the painter
 * can mount.
 */
export function sceneFromElements(raw) {
  if (!isRecord(raw)) {
    return null;
  }
  const canvas = box(raw.canvas);
  const tokens = tokensOf(raw.tokens);
  const palette = paletteOf(raw.palette);
  const nodes = listOf(raw.nodes, nodeOf);
  const edges = listOf(raw.edges, edgeOf);
  const bands = listOf(raw.bands, bandOf);
  const events = listOf(raw.events, eventOf);
  if (
    canvas === null ||
    tokens === null ||
    palette === null ||
    nodes === null ||
    edges === null ||
    bands === null ||
    events === null
  ) {
    return null;
  }
  return {
    canvas,
    tokens,
    direction: isText(raw.direction) ? raw.direction : null,
    layout_hint: isText(raw.layout_hint) ? raw.layout_hint : null,
    palette,
    nodes,
    edges,
    bands,
    events,
  };
}

/**
 * The scene a render payload carries, or `null`.
 *
 * `null` is the answer for a payload with no usable scene. Since the flip of
 * T3/C every runtime payload carries one (`graphium://render`,
 * `graphium://version-paint` and `graphium://clone-paint`), and a payload
 * without it has nothing the plan could mount.
 */
export function sceneFromPayload(payload) {
  if (!isRecord(payload)) {
    return null;
  }
  return sceneFromElements(payload.elements);
}
