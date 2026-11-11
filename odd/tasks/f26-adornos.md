# f26-adornos

Etapa 0 of `docs/CATALOGO-DIAGRAMAS.md`: the edge stops being a boolean. The IR
carries one adornment per end, both painters (Rust and the JS twin) draw it, and
the crow's foot stops competing with the arrow for the same tip. Base `main` @
790634a.

## Specs

S1 (user, verbatim): "el 1 tomare la recomendacion y el 2 ncesito que me lo
expliques" — decision 1 of Etapa 0 is taken as recommended: the relation
vocabulary is an **optional** field per kind, not a mandatory global enum. A
kind declares the relations it admits, the contract teaches them, and a document
that writes none gets that kind's default.

S2 (user, verbatim): "ok tomare la recomendacion puedes empezar" — decision 2 is
taken as the hybrid the owner was shown: the IR stores the **drawing truth**
(`head` and `tail`, one adornment per end, closed vocabulary) and the kind's own
table decides which pairs it may express. Documents keep writing semantic names
(`relation`, and later `kind`); they never write a glyph.

S3 (the migration promise the owner accepted): every existing golden stays
**byte-identical**, with one declared exception: `class` composition draws its
diamond for the first time. That diff is expected and stated, never discovered
at gate time.

S4 (the collision S2 removes): `er` stops switching the arrowhead off by hand.
A relation with a `cardinality` draws the crow's foot at **both** ends and no
marker at either, because the crow's foot *is* the notation of that relation.

S5 (closed vocabulary, exactly what Etapa 1 needs): `arrow` (the filled triangle
of today, keeping the marker id `arrow-<emphasis>` so goldens hold),
`open_arrow`, `hollow_triangle`, `diamond`, `hollow_diamond`. Nothing else enters
in this cut; the ISO flowchart glyphs are a different (node-shape) bill.

S6 (the tail is drawn, not stored and ignored): an adornment on the source end
reaches the drawing — same vocabulary, mirrored geometry, `marker-start`.

S7 (class gets the notation it was missing): `composition` draws the filled
diamond at the target end, `aggregation` joins the relation table with the
hollow diamond, and both lose the arrowhead because the diamond already says
which end owns which. `extends` and `implements` keep today's arrow: the hollow
triangle is available in the vocabulary but swapping it is a separate decision
with its own golden.

S8 (the kind teaches what it draws): the `class` rule line stops being a
hand-written sentence and is derived from the relation table, so a relation can
never be painted with an adornment the contract does not mention.

## Tasks

T1 `gx-core`: `Adornment`, `head`/`tail` on `IrEdge` in place of `arrow`, and
one resolution point (`ends()`) where a `cardinality` owns both ends. Test
first: RED on the resolution precedence and on the default head. inline.
T2 `gx-render-svg`: the marker vocabulary (id, path, mirrored start variant),
`marker-end`/`marker-start`, the CSS rules of the hollow glyphs, and `<defs>`
holding only the markers the document references. Test first: one RED per glyph
family. inline.
T3 the scene payload and the JS twin: `SceneEdge.head`/`tail` replace `arrow`,
`glyph.js` builds the marker set from the payload, `scene.js` validates it, the
two Svelte components write the marker, `app.css` carries the twin rules. The
equivalence gate is the proof. inline.
T4 the kinds: `er` states the crow's foot instead of cancelling an arrow;
`class` gains `aggregation`, gives `composition` its diamond and derives S8's
rule; `flow`, `graph`, `mindmap`, `sequence`, `tree` and `flowchart` migrate
mechanically. Goldens: only `class` changes. inline.
T5 docs and close: PRD §6.1/§8.3, `frontend/CONTRACT.md`, the catalogue's Etapa 0
entry, the feature log and the gates. inline.

## Log

L1 (2026-11-11, user, verbatim): "por donde quedamos" — the session resumed on a
clean tree: `main` = `staging` = `origin/main` @ 790634a, the f18–f25 wave
merged. The agreed next step was Etapa 0 of the catalogue.

L2 (2026-11-11, user, verbatim): "el 1 tomare la recomendacion y el 2 ncesito
que me lo expliques" — decision 1 closed on the recommendation; decision 2 was
explained against the code as it is, with the evidence: `arrow` is one boolean
(`crates/gx-core/src/ir.rs:224`), one glyph (`renderer.rs:409`), one end
(`marker-end`, `renderer.rs:432`), `marker-start` was never emitted, `er` had to
cancel the arrow by hand (`er.rs:292`) because the crow's foot already occupies
the tip, and `class` composition declares no arrow because there is no diamond
to draw (`class.rs:530`).

L3 (2026-11-11, user, verbatim): "ok tomare la recomendacion puedes empezar" —
the hybrid of S2 authorized, with the scope the owner was shown: the pieces 1–5
of the recommendation. The third piece the catalogue files under Etapa 0 — the
drawn stereotype in the `flow` boxes — is **not** in this cut: it is node
vocabulary, not the edge slot, and it was not part of what the owner approved.
It stays recorded in the catalogue as the rest of Etapa 0.

L4 (2026-11-11, planning, T1): the scope was cut down while reading the surface.
No shared `EdgeKind` type is introduced: `class` is the only kind with a relation
vocabulary today (`class.rs:74`), so its own table is the per-kind vocabulary S1
asks for, and Etapa 1's `component` rows will bring the shared type when nine
relations exist to share. The platform work of this cut is the adornment slot in
the IR plus the marker vocabulary in both painters — which is exactly what
unblocks Etapa 1.

L5 (2026-11-11, implementation, T4): the `class` table grew its sixth row and
composition finally draws. `RELATIONS` is now `[Relation; 6]` — `association`
(sólida, flecha, sin etiqueta, el default), `extends` (sólida, flecha, is-a,
se nombra), `implements` (punteada, flecha, is-a, se nombra), `composition`
(sólida, **rombo relleno**, se nombra), `aggregation` (sólida, **rombo hueco**,
se nombra) and `dependency` (punteada, flecha, se nombra). The two diamond rows
give up the arrowhead at the target end and keep `tail: None`, because the
diamond already says which end owns which (S7). `extends` and `implements` keep
today's `Adornment::Arrow`: swapping them to the hollow triangle is a separate
decision with its own golden. `ClassEdge.arrow` still overrides the target end
exactly as before — `Some(true)` is the plain arrow, `Some(false)` is nothing —
and it wins over the diamond, which is what the new test pins.

S8 stopped being prose: the first rule is derived from the table by
`ClassKind::relations_rule()`, which asks each row for the word of its style
(`sólida`/`punteada`) and the word of its glyph (`flecha`, `flecha abierta`,
`triángulo hueco`, `diamante relleno`, `diamante hueco`), plus whether the row
names the line and whether it is the default. The line now reads:

`relation` decide la línea: `association` (sólida con flecha, no se nombra, por defecto), `extends` (sólida con flecha, se nombra), `implements` (punteada con flecha, se nombra), `composition` (sólida con diamante relleno, se nombra), `aggregation` (sólida con diamante hueco, se nombra), `dependency` (punteada con flecha, se nombra).

Goldens: only the two declared files moved (S3). `class-shop.svg` gains the
`.diamond-normal` rule, the `diamond-normal` marker in `<defs>` and
`marker-end="url(#diamond-normal)"` on `m-Order-Money`; `class-shop.json` gains
that edge's `head: {kind: marker, adornment: diamond}`. Every other golden — 20
SVGs and 20 scenes — stayed byte-identical, and `git status --short` after both
generators listed those two paths and nothing else.

Gates: `cargo test -p gx-kinds` 116 + 11 + 12 passed, 0 failed;
`cargo test -p gx-app --test golden_svg` 4 passed, 0 failed;
`cargo test --workspace` 42 suites, 866 passed, 0 failed, 2 ignored;
`cargo fmt --all -- --check` clean; `cargo clippy --workspace --all-targets`
0 warnings, 0 errors. Nothing under `gx-render-svg` or the frontend needed to
change: both painters already drew every glyph of S5.

L6 (2026-11-11, close, T5): the cut is complete. **Slices and their boundaries**
— the harness measured the branch by authored lines since `origin/main`, so the
boundary is recorded here: `2e08314` is the platform slice and it is **one**
commit because the compiler forces it (the IR field, the painter, the kinds and
the payload are one atomic change; there is no compiling state in between), and
it carries the 19 regenerated scene payloads beside the sources. `01d2747` is
the painter twin (frontend only). `1d927d9` is the class notation and its single
golden. This entry closes with the documents. A review slice of the whole
feature is the three commits in that order; the platform one is the only one
over the ~400-line guideline, by construction and not by accumulation.

**What the cut leaves in the platform.** `IrEdge` has `head`/`tail`
(`Option<Adornment>`, five glyphs) and one resolution point, `head_end`/
`tail_end`, where a `cardinality` takes both ends. The emitter writes
`marker-end` and `marker-start`, the source-end glyph mirrored with `refX` on the
near side, ids `<glyph>-<emphasis>[-start]` with the plain arrow keeping
`arrow-<emphasis>`, `<defs>` holding only the referenced markers and the style
block holding only the rule of each family the document uses. The twin mirrors
all of it from the payload names, never from a path. `class` derives its rule
line from its relation table (S8) and draws diamonds; `er` states the crow's
foot instead of cancelling an arrow.

**Gates at close.** `cargo fmt --all -- --check` clean; `cargo clippy
--workspace --all-targets` 0 warnings and 0 errors; `cargo test --workspace`
**42 suites, 866 passed, 0 failed** (861 before the cut: six new tests — three on
the resolution, one on the hollow family glyphs, two on the class table — plus
the tests the cut rewrote); `node tools/equivalence.mjs` **21/21 golden
documents**, including `class-shop`, which now carries the diamond on both
sides of the gate. Every golden SVG stayed byte-identical except the declared
`class-shop`.

**Delivery.** Branch `feature/f26-adornos` on `main` @ 790634a. Push, PR and
merge stay the owner's decisions.
