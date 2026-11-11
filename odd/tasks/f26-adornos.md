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
