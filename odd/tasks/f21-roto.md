# f21-roto

The broken things the four-lens critical review proved, fixed without design
decisions: the export that paints text black on dark, the canvas that clips
labels, the gantt that overlaps its own bars, the swimlane with no air, and
the kind limits that skip their guards. Branch `feature/f21-roto` stacked on
`feature/f20-reuso` @ a767fe8, worktree `~/Proyectos/graphium-f21`.

## Specs

S1 (user, verbatim): «dale sigue ese orden» — the cut order of the critical
review synthesis: f21 bugs first, then f22 (what the drawing does not say),
then f23 (contract ergonomics).

S2 (export truth): `write_style` emits the four row rules the window CSS has
(`crates/gx-app/frontend/src/app.css:320-331`): `row-badge`/`row-note` take
`--edge-text`, `row-key` is semibold, `row-nullable` takes the muted opacity —
so the exported artifact paints rows the same colors the window does
(evidence: `golden/er-posts.svg:39-50` and `golden/c4-context.svg:44` carry
the classes but the export stylesheet had no rules for them).

S3 (canvas truth): `canvas_rect` returns the union of the node boxes **and**
the rendered edge-label boxes, padded (`f21` uses 12 plan units) — labels with
their center outside the viewBox (`dataflow-etl` 2 of 4 labels, `state-order`
"crear", `class-shop` "extends") fit, and no stroke touches the frame.

S4 (gantt truth): tasks of the same lane that overlap in time pack into
sub-rows — the lane height grows by the number of sub-rows it needs, no two
bars of one lane share a region (evidence: `gantt-plan.svg` had a 118 px
overlap between `g-planos` and `g-permiso`).

S5 (swimlane truth): the column pitch of a lanes row is the widest box in the
lane plus the gutter, not a fixed 150 px (evidence: `swimlane-soporte` gaps of
0.33 px and 6.25 px between steps; timeline's fixed-width boxes were healthy
by accident).

S6 (limits truth): the two missing limit profiles exist and the kinds declare
them — `KindLimits::SEQUENCE` (40 participants, 200 messages — the PRD's
40 carriles × 200 eventos) and `KindLimits::TREE` (500 nodes / 499 edges, for
tree and mindmap) — so the engine's guards actually fire (`sequence.rs` and
`tree.rs`/`mindmap.rs` declared `GRAPH` and silently skipped the checks).

Out of scope (goes to f22 with its design decisions): the time axis, role
glyphs (initial vs final, class stereotypes), milestone rendering, class
members, swimlane connectors (a contract change), diamond/cylinder label
geometry, flowchart stubs, sequence activations.

## Tasks

- T1 (S2, S6): export row rules + limit profiles — renderer.rs write_style,
  kind.rs consts, sequence/tree/mindmap LIMITS.
- T2 (S3): canvas_rect union with labels + padding — geometry.rs, goldens.
- T3 (S4, S5): lanes engine — sub-row packing for gantt, pitch from widest
  box — gx-layout, goldens.
- T4: orchestrator — gates, goldens regen + hand review, commits, close.

## Risk

- Item 3: every golden SVG changes (viewBox padding is global); the pins are
  regenerated and reviewed against S3's rule, not rubber-stamped.

## Log

- L1 (2026-11-09, user, verbatim): S1.
- L2 (2026-11-09, evidence): the five bug families come from the four-lens
  review (visual reader findings 1-4 + 9; semantic finding 1; consistency
  finding 11), each with file:coordinate evidence in the review synthesis.
- L3 (2026-11-09, execution): the density ceilings of the two new profiles
  were 0.0 on the first cut and the golden suite caught both (tree-src at
  0.83, sequence-order at 1.67 — a 0.0 density is not "unchecked", it is an
  impossible ceiling); the profiles carry the family density (tree 3.0,
  sequence 5.0 = the PRD maximum 200/40). The canvas contract broke two more
  pins than the three test files (export_golden's 20th artifact,
  scene_payload's bbox assertion) — all updated to the padded contract. The
  lanes engine learned mid-cut that a gantt of only spans loses its columns
  if columns advance by point widths alone: every order is a column, points
  widen, spans do not. Final gates: fmt OK, clippy 0, cargo test --workspace
  42 suites 0 failed, parity 71/71, equivalence 19/19 (chromium local, fresh
  dist).
