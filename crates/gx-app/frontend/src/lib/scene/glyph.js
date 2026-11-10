// The glyph math of the painter (`f11-elementos.md`, T2; S4, S5).
//
// Every number the scene does not carry — where the lines of a label sit, the
// points of a crow's foot, the rim of a cylinder, the fold of a note, the
// diamond of a decision, the lifeline of an actor, the table of a node with
// rows — is computed **here**, once, and it is the arithmetic of
// `gx-render-svg/src/geometry.rs`: the exporter writes those numbers as markup
// and the painter draws the very same ones. That is the honest split of S5:
// the glyphs are drawn, the payload never ships them as paths.
//
// Two rules hold for this whole module:
//
// - **Nothing measures** (S4): no `getBBox`, no `measureText`, no layout read.
//   Labels arrive pre-broken with `\n` and the boxes arrive measured.
// - **Nothing colours**: the classes are the classes of the emitter and the
//   colours are the CSS custom properties of `src/app.css` (S3).
//
// Pure on purpose: no DOM, so the probes can call it from node and compare the
// numbers with the ones the emitter writes.

// --- numbers ---------------------------------------------------------------

/**
 * A number as an SVG attribute, rounded the way `fmt_num` rounds (two
 * decimals), so a mounted scene and the exported string never disagree by a
 * float tail.
 */
export function num(value) {
  if (!Number.isFinite(value)) {
    return 0;
  }
  const scaled = value * 100;
  const lower = Math.floor(scaled);
  const remainder = scaled - lower;
  // Round-half-to-even at two decimals: the canonical rule of
  // `gx-core/src/fmt.rs` (PRD §8.5.1). `Math.round` rounds every .5 up, so
  // 91.125 became 91.13 while the export wrote 91.12 — the exact drift the
  // equivalence gate caught on the baselines of the c4 golden.
  let rounded;
  if (remainder > 0.5) rounded = lower + 1;
  else if (remainder < 0.5) rounded = lower;
  else rounded = lower % 2 === 0 ? lower : lower + 1;
  const result = rounded / 100;
  return Object.is(result, -0) ? 0 : result;
}

/** A list of points as the `points` attribute of a polyline or a polygon. */
export function pointsText(points) {
  return points.map((point) => `${num(point.x)},${num(point.y)}`).join(" ");
}

// --- labels (S4) ------------------------------------------------------------

/**
 * The lines a label breaks into: exactly what `str::lines()` does in Rust, so
 * a label with a trailing `\n` does not get an empty line.
 */
export function textLines(label) {
  if (typeof label !== "string" || label === "") {
    return [];
  }
  const lines = label.split("\n");
  if (lines[lines.length - 1] === "") {
    lines.pop();
  }
  return lines.map((line) => (line.endsWith("\r") ? line.slice(0, -1) : line));
}

/**
 * The baseline of each line of a label centered on `center_y`: one formula for
 * the whole drawing, and a single line lands exactly on the center.
 */
export function centeredBaselines(centerY, count, tokens) {
  const lineHeight = tokens.text_size * tokens.line_height;
  const total = lineHeight * count;
  return Array.from(
    { length: count },
    (_, index) => centerY - total / 2 + lineHeight * (index + 0.5),
  );
}

// --- the box of a node ------------------------------------------------------

/** The rectangle of a placed node, from the center of its measured box (S4). */
export function nodeRect(node) {
  return {
    x: node.center.x - node.size.w / 2,
    y: node.center.y - node.size.h / 2,
    w: node.size.w,
    h: node.size.h,
  };
}

// --- classes (S3) -----------------------------------------------------------

/** The class of a shape: the names the emitter writes. An unknown shape is a
 * loud end of the run, never a plausible box (`f19-formas.md`, S5). */
export function shapeClass(shape) {
  switch (shape) {
    case "box":
      return "shape-box";
    case "rounded":
      return "shape-rounded";
    case "cylinder":
      return "shape-cylinder";
    case "diamond":
      return "shape-diamond";
    case "ellipse":
      return "shape-ellipse";
    case "note":
      return "shape-note";
    case "circle":
      return "shape-circle";
    default:
      throw new Error(`the painter does not know the shape: ${shape}`);
  }
}

/**
 * The emphasis of a shape or a text: highlighting a shape changes its stroke
 * and highlighting a text changes its fill, so they need their own class.
 */
export function emphasisClass(emphasis, forText) {
  if (emphasis === "highlight") {
    return forText ? "emphasis-highlight-text" : "emphasis-highlight";
  }
  return emphasis === "muted" ? "emphasis-muted" : "";
}

/** The classes of the box of a node: the classes `write_node` writes. */
export function shapeClasses(node) {
  const classes = ["node-fill", "node-stroke", shapeClass(node.shape)];
  if (node.style === "dashed") {
    classes.push("stroke-dashed");
  }
  const emphasis = emphasisClass(node.emphasis, false);
  if (emphasis !== "") {
    classes.push(emphasis);
  }
  return classes.join(" ");
}

/** The classes of the line of an edge: the classes `write_edge` writes. */
export function edgeClasses(edge) {
  const classes = ["edge-stroke"];
  if (edge.style === "dashed") {
    classes.push("stroke-dashed");
  }
  const emphasis = emphasisClass(edge.emphasis, false);
  if (emphasis !== "") {
    classes.push(emphasis);
  }
  return classes.join(" ");
}

/** The classes of the label of an edge. */
export function edgeTextClasses(edge) {
  const emphasis = emphasisClass(edge.emphasis, true);
  return emphasis === "" ? "edge-text text" : `edge-text text ${emphasis}`;
}

/** The classes of the label of a node or of an event. */
export function nodeTextClasses(emphasis) {
  const highlight = emphasisClass(emphasis, true);
  return highlight === "" ? "node-text text" : `node-text text ${highlight}`;
}

/** The classes of the name of a row: its key and its nullability are painted. */
export function rowNameClasses(row) {
  const classes = ["node-text", "text"];
  if (row.key === "pk" || row.key === "pk_fk") {
    classes.push("row-key");
  }
  if (row.nullable) {
    classes.push("row-nullable");
  }
  return classes.join(" ");
}

/** The marker of an emphasis: the arrowheads of `<defs>` (S5). */
export function markerId(emphasis) {
  switch (emphasis) {
    case "highlight":
      return "arrow-highlight";
    case "muted":
      return "arrow-muted";
    default:
      return "arrow-normal";
  }
}

/**
 * The markers the scene needs, sorted by id, like the `BTreeSet` of the
 * emitter: only the ones an edge references, never the whole family.
 */
export function arrowMarkers(scene) {
  const used = new Set();
  for (const edge of scene.edges) {
    if (edge.arrow) {
      used.add(markerId(edge.emphasis));
    }
  }
  return Array.from(used).sort();
}

// --- the table of a node with rows (PRD §6.1) -------------------------------

/**
 * Where the header, the rule and the rows of a table node sit: the numbers
 * `Table::of` computes in Rust, from the same tokens.
 */
export function tableOf(rect, tokens) {
  const padding = tokens.node_padding;
  const line = tokens.text_size * tokens.line_height;
  const headerY = rect.y + padding + line / 2;
  return {
    padding,
    line,
    header_y: headerY,
    rule_y: headerY + line / 2,
    badge_x: rect.x + padding / 2,
    name_x: rect.x + padding + tokens.badge_width,
    note_x: rect.x + rect.w - padding / 2,
    row_y: (index) => headerY + line / 2 + line * (index + 0.5),
  };
}

/** The text of the key badge of a row: `PK`, `FK`, `PK, FK` or nothing. */
export function badgeText(key) {
  switch (key) {
    case "pk":
      return "PK";
    case "fk":
      return "FK";
    case "pk_fk":
      return "PK, FK";
    default:
      return "";
  }
}

// --- the shapes (S5) --------------------------------------------------------

/**
 * The two paths of a cylinder: the body with its rounded ends and the rim that
 * closes the top. The rim height is the corner radius, capped to a quarter of
 * the box so a short node still has a readable body.
 */
/** The circle dot inside the envelope: the frozen diameter, at the left edge
 * of the envelope, so the arrows that arrive land on it (`f19-formas.md`,
 * S8/S9). The twin of `geometry::glyph_rect` in `gx-render-svg`. */
export function circleGeometry(rect, tokens) {
  return {
    cx: num(rect.x + tokens.node_padding + tokens.circle_diameter / 2),
    cy: num(rect.y + rect.h / 2),
    r: num(tokens.circle_diameter / 2),
  };
}

export function cylinderGeometry(rect, tokens) {
  const rim = Math.min(tokens.corner_radius, rect.h / 4);
  // Cubics instead of elliptical arcs: an arc flag is a bare `0`/`1`, which
  // would break the two-decimal rule of the drawing (geometry.rs).
  const bulge = (rim * 4) / 3;
  const left = rect.x;
  const right = rect.x + rect.w;
  const top = rect.y + rim;
  const bottom = rect.y + rect.h - rim;
  const at = (x, y) => `${num(x)} ${num(y)}`;
  return {
    rim,
    bulge,
    body: [
      `M ${at(left, top)}`,
      `C ${at(left, top - bulge)} ${at(right, top - bulge)} ${at(right, top)}`,
      `L ${at(right, bottom)}`,
      `C ${at(right, bottom + bulge)} ${at(left, bottom + bulge)} ${at(left, bottom)}`,
      "Z",
    ].join(" "),
    rimPath: [
      `M ${at(left, top)}`,
      `C ${at(left, top + bulge)} ${at(right, top + bulge)} ${at(right, top)}`,
    ].join(" "),
  };
}

/** The two paths of a note: the sheet and the fold of its corner. */
export function noteGeometry(rect, tokens) {
  const fold = Math.min(
    tokens.corner_radius,
    rect.w / 4,
    rect.h / 4,
  );
  const left = rect.x;
  const right = rect.x + rect.w;
  const top = rect.y;
  const bottom = rect.y + rect.h;
  const foldX = rect.x + rect.w - fold;
  const foldY = rect.y + fold;
  const at = (x, y) => `${num(x)} ${num(y)}`;
  return {
    fold,
    body: [
      `M ${at(left, top)}`,
      `L ${at(foldX, top)}`,
      `L ${at(right, foldY)}`,
      `L ${at(right, bottom)}`,
      `L ${at(left, bottom)}`,
      "Z",
    ].join(" "),
    foldPath: [
      `M ${at(foldX, top)}`,
      `L ${at(foldX, foldY)}`,
      `L ${at(right, foldY)}`,
    ].join(" "),
  };
}

/** The four points of a diamond, in the order the emitter writes them. */
export function diamondPoints(rect) {
  const centerX = rect.x + rect.w / 2;
  const centerY = rect.y + rect.h / 2;
  return [
    `${num(centerX)},${num(rect.y)}`,
    `${num(rect.x + rect.w)},${num(centerY)}`,
    `${num(centerX)},${num(rect.y + rect.h)}`,
    `${num(rect.x)},${num(centerY)}`,
  ].join(" ");
}

// --- the crow's foot of a relation (PRD §6.1, S5) --------------------------

/**
 * The unit direction of the line at one of its ends, pointing into the box.
 *
 * A route of a single point has no direction: the emitter answers `(1, 0)`
 * rather than a `NaN`, and the painter does the same.
 */
export function directionAt(points, atEnd) {
  const fallback = { x: 0, y: 0 };
  const first = points.length > 0 ? points[0] : fallback;
  const last = points.length > 0 ? points[points.length - 1] : fallback;
  const before = points.length > 1 ? points[points.length - 2] : last;
  const after = points.length > 1 ? points[1] : first;
  const from = atEnd ? last : first;
  const to = atEnd ? before : after;
  const dx = from.x - to.x;
  const dy = from.y - to.y;
  const length = Math.sqrt(dx * dx + dy * dy);
  if (length <= Number.EPSILON) {
    return { x: 1, y: 0 };
  }
  return { x: dx / length, y: dy / length };
}

/**
 * The glyphs of the crow's foot at one end of a relation, in plan units.
 *
 * The notation is the crow's foot: each end says how many rows of the other
 * table can point here and whether zero is allowed. The geometry is **not**
 * shipped as a path (S5): the size travels in the tokens and the painter
 * rebuilds these very points, the ones `crow_foot_mark` computes in Rust.
 */
export function crowFootMark(at, direction, optional, many, tokens) {
  const size = tokens.crow_foot;
  const back = (distance) => ({
    x: at.x - direction.x * distance,
    y: at.y - direction.y * distance,
  });
  const side = (point, offset) => ({
    x: point.x - direction.y * offset,
    y: point.y + direction.x * offset,
  });
  const glyph = many
    ? (() => {
        // The foot opens TOWARDS the table: the ankle sits away from it and
        // the three toes touch its edge. Opening away from the table draws an
        // arrowhead, which is the opposite meaning.
        const spread = size * 0.62;
        return {
          kind: "many",
          ankle: back(size),
          toes: [side(at, spread), side(at, -spread)],
        };
      })()
    : (() => {
        // Exactly one: a single stroke across the relation.
        const mark = back(size * 0.6);
        const half = size * tokens.crow_foot_spread;
        return { kind: "one", from: side(mark, half), to: side(mark, -half) };
      })();
  return {
    glyph,
    optional: optional ? back(size * 1.45) : null,
    radius: size / 3.5,
  };
}

/**
 * The path of a fork: the three legs of the crow's foot, one `M … L …` each.
 *
 * The third leg ends on the line itself (`at`), which is the toe that the line
 * already covers.
 */
export function crowFootPath(mark, at) {
  const leg = (to) =>
    `M ${num(mark.ankle.x)} ${num(mark.ankle.y)} L ${num(to.x)} ${num(to.y)}`;
  return [leg(at), ...mark.toes.map(leg)].join(" ");
}

/** The arrows of the two ends of a relation, or nothing when it has none. */
export function cardinalityMarks(edge) {
  if (edge.cardinality === null || edge.points.length < 2) {
    return [];
  }
  const first = edge.points[0];
  const last = edge.points[edge.points.length - 1];
  const endDirection = directionAt(edge.points, true);
  const startDirection = directionAt(edge.points, false);
  return [
    { at: last, direction: endDirection, bound: edge.cardinality.to_min, max: edge.cardinality.to_max },
    { at: first, direction: startDirection, bound: edge.cardinality.from_min, max: edge.cardinality.from_max },
  ];
}

// --- the lifeline of an actor (PRD §6, S5) ---------------------------------

/** From the bottom border of the actor to the bottom of the canvas. */
export function lifelineGeometry(node, canvas) {
  return {
    x: node.center.x,
    y1: node.center.y + node.size.h / 2,
    y2: canvas.y + canvas.h,
  };
}

/** Whether the drawing draws a lifeline under every actor. */
export function drawsLifelines(scene) {
  return scene.layout_hint === "lanes";
}

// --- the paint plan of the drawing -----------------------------------------

// The components below this module spell what these functions decide. The plan
// is computed **once per scene** (`diagramPaint`) instead of once per element
// instance: a 300-node drawing has ~900 components, and a `$derived` per
// component costs more than the drawing itself (measured, see L5 of
// `f11-elementos.md`). One plan, one reactive read, the same numbers.

/** The paint of one text: the lines, where each one sits, and its classes. */
export function textPaint({
  lines = [],
  baselines = [],
  x = 0,
  classes = "",
  size = 14,
  // `null` is the SVG default (`start`): the attribute is omitted, exactly as
  // the emitter omits it on the rows of a table.
  anchor = "middle",
  weight = null,
  dominantBaseline = "central",
}) {
  return {
    lines,
    baselines: baselines.map(num),
    x: num(x),
    classes,
    size: num(size),
    anchor,
    weight,
    dominantBaseline,
  };
}

/** The paint of a node (or an event): its shape, its box and its texts. */
export function nodePaint(node, tokens) {
  const rect = nodeRect(node);
  const lines = textLines(node.label);
  const classes = nodeTextClasses(node.emphasis);
  const isTable = node.rows.length > 0;
  const table = isTable ? tableOf(rect, tokens) : null;
  return {
    id: node.id,
    shape: node.shape,
    classes: shapeClasses(node),
    // How a probe and a screen reader name the element: the label of the
    // drawing, with its line breaks turned into the spaces they stand for.
    name: lines.join(" "),
    rect: {
      x: num(rect.x),
      y: num(rect.y),
      w: num(rect.w),
      h: num(rect.h),
      radius: node.shape === "rounded" ? num(tokens.corner_radius) : null,
      // The ellipse halves, computed from the raw rect and rounded once: the
      // emitter writes fmt_num(w / 2) from the raw width, and rounding before
      // dividing drifts exactly on the boundary (`f19-formas.md`, L6).
      rx: node.shape === "ellipse" ? num(rect.w / 2) : null,
      ry: node.shape === "ellipse" ? num(rect.h / 2) : null,
    },
    center: { x: num(node.center.x), y: num(node.center.y) },
    circle:
      node.shape === "circle" ? circleGeometry(nodeRect(node), tokens) : null,
    cylinder:
      node.shape === "cylinder" ? cylinderGeometry(rect, tokens) : null,
    note: node.shape === "note" ? noteGeometry(rect, tokens) : null,
    diamond: node.shape === "diamond" ? diamondPoints(rect) : null,
    // A node has no bar: only an event that lasts does (`eventPaint`).
    bar: null,
    isTable,
    // The header of a table is one text with the label as it arrived; a plain
    // node breaks its label on the `\n` and centers the lines (S4).
    header: isTable
      ? textPaint({
          lines: [node.label],
          baselines: [table.header_y],
          x: node.center.x,
          classes,
          size: tokens.text_size,
          anchor: "middle",
          weight: tokens.semibold_weight,
        })
      : null,
    rule: isTable
      ? {
          x1: num(rect.x),
          y1: num(table.rule_y),
          x2: num(rect.x + rect.w),
          y2: num(table.rule_y),
          width: num(tokens.border_width),
        }
      : null,
    label:
      isTable || lines.length === 0
        ? null
        : node.label_at !== undefined && node.label_at !== null
          ? // The beside placement of the circle (`f19-formas.md`, S8): the
            // scene computed the point; the painter only spells it.
            textPaint({
              lines,
              baselines: [node.label_at.y],
              x: node.label_at.x,
              classes,
              size: tokens.text_size,
              anchor: "start",
            })
          : textPaint({
              lines,
              baselines: centeredBaselines(node.center.y, lines.length, tokens),
              x: node.center.x,
              classes,
              size: tokens.text_size,
              anchor: "middle",
            }),
    rows: isTable
      ? node.rows.map((row, index) => ({
          badge: textPaint({
            lines: [badgeText(row.key)],
            baselines: [table.row_y(index)],
            x: table.badge_x,
            classes: "row-badge text",
            size: tokens.text_size * 0.85,
            anchor: null,
          }),
          name: textPaint({
            lines: [row.text],
            baselines: [table.row_y(index)],
            x: table.name_x,
            classes: rowNameClasses(row),
            size: tokens.text_size,
            anchor: null,
          }),
          note:
            row.note === null
              ? null
              : textPaint({
                  lines: [row.note],
                  baselines: [table.row_y(index)],
                  x: table.note_x,
                  classes: "row-note text",
                  size: tokens.text_size * 0.9,
                  anchor: "end",
                }),
        }))
      : [],
  };
}

/** The paint of an edge: the route, its marker, its crow's feet and its label. */
export function edgePaint(edge, tokens) {
  const classes =
    edge.cardinality === null || edge.points.length < 2
      ? "cardinality"
      : edge.cardinality.to_min === "optional"
        ? "cardinality cardinality-optional"
        : "cardinality";
  return {
    id: edge.id,
    classes: edgeClasses(edge),
    // The name of the line for a screen reader: the two ends, as the drawing
    // names them (`m-<from>-<to>` is built from the very same ids).
    name: `${edge.from} → ${edge.to}`,
    points: pointsText(edge.points),
    marker: edge.arrow ? `url(#${markerId(edge.emphasis)})` : null,
    feet: cardinalityMarks(edge).map((mark) => {
      const foot = crowFootMark(
        mark.at,
        mark.direction,
        mark.bound === "optional",
        mark.max === "many",
        tokens,
      );
      return {
        kind: foot.glyph.kind,
        classes,
        path: foot.glyph.kind === "many" ? crowFootPath(foot.glyph, mark.at) : null,
        from:
          foot.glyph.kind === "one"
            ? { x: num(foot.glyph.from.x), y: num(foot.glyph.from.y) }
            : null,
        to:
          foot.glyph.kind === "one"
            ? { x: num(foot.glyph.to.x), y: num(foot.glyph.to.y) }
            : null,
        circle:
          foot.optional === null
            ? null
            : { cx: num(foot.optional.x), cy: num(foot.optional.y), r: num(foot.radius) },
      };
    }),
    label:
      edge.label === null || edge.label_at === null
        ? null
        : textPaint({
            lines: [edge.label],
            baselines: [edge.label_at.y],
            x: edge.label_at.x,
            classes: edgeTextClasses(edge),
            size: tokens.text_size,
            anchor: "middle",
          }),
  };
}

/** The paint of a band: the box of a container or of a lane, and its name. */
export function bandPaint(band, tokens) {
  const isLane = band.kind === "lane";
  const label =
    band.label !== null && band.label.trim() !== "" ? band.label : null;
  return {
    id: band.id,
    isLane,
    box: {
      x: num(band.bbox.x),
      y: num(band.bbox.y),
      w: num(band.bbox.w),
      h: num(band.bbox.h),
    },
    radius: isLane ? null : num(tokens.corner_radius),
    label:
      label === null
        ? null
        : textPaint({
            lines: [label],
            // A container writes its name at the corner, indented by its depth;
            // a lane writes it in the gutter the layout reserved, centered on
            // its own height. Both are rules of the emitter.
            baselines: [
              isLane
                ? band.bbox.y + band.bbox.h / 2
                : band.bbox.y + tokens.text_size,
            ],
            x: isLane
              ? band.bbox.x + tokens.node_padding
              : band.bbox.x + tokens.node_padding * (1 + band.depth),
            classes: isLane ? "lane-label text" : "group-label text",
            size: tokens.text_size,
            anchor: "start",
          }),
  };
}

/**
 * The bar of an event that lasts, from the numbers the scene carries
 * (`f11-elementos.md`, T4/D).
 *
 * The geometry is **not** computed here: the layout sized the box from the
 * span and `gx-render-svg/src/geometry.rs` decided the rectangle, so the
 * painter only rounds the four numbers it was given (S5: the payload carries
 * numbers, not paths). Absent when the event is a point in time.
 */
export function eventBar(event) {
  const bar = event.bar;
  if (bar === null || bar === undefined) {
    return null;
  }
  return { x: num(bar.x), y: num(bar.y), w: num(bar.w), h: num(bar.h) };
}

/** An event of a timeline is a rounded box with its label, and its bar. */
export function eventPaint(event, tokens) {
  return {
    ...nodePaint(
      {
        id: event.id,
        label: event.label,
        shape: "rounded",
        emphasis: "normal",
        style: "solid",
        center: event.center,
        size: event.size,
        rows: [],
        meta: {},
      },
      tokens,
    ),
    bar: eventBar(event),
  };
}

/**
 * The whole paint plan of a scene: one pass, computed once, read by the
 * components as plain data.
 */
export function diagramPaint(scene) {
  const tokens = scene.tokens;
  const markers = arrowMarkers(scene).map((id) => ({
    id,
    length: num(tokens.arrow_length),
    half: num(tokens.arrow_length / 2),
  }));
  return {
    canvas: {
      x: num(scene.canvas.x),
      y: num(scene.canvas.y),
      w: num(scene.canvas.w),
      h: num(scene.canvas.h),
    },
    markers,
    clusters: scene.bands
      .filter((band) => band.kind === "cluster")
      .map((band) => bandPaint(band, tokens)),
    lanes: scene.bands
      .filter((band) => band.kind === "lane")
      .map((band) => bandPaint(band, tokens)),
    lifelines: drawsLifelines(scene)
      ? scene.nodes.map((node) => {
          const line = lifelineGeometry(node, scene.canvas);
          return { x: num(line.x), y1: num(line.y1), y2: num(line.y2) };
        })
      : [],
    edges: scene.edges.map((edge) => edgePaint(edge, tokens)),
    nodes: [
      ...scene.nodes.map((node) => nodePaint(node, tokens)),
      ...scene.events.map((event) => eventPaint(event, tokens)),
    ],
  };
}
