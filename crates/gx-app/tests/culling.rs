//! The culling of the plan (PRD RNF1-ter, §7.2, §7.3, S6).
//!
//! No test can open the webview, so what is pinned here is the contract the
//! page keeps with the PRD: the canvas holds **at most three SVGs** of
//! diagrams, an object that is out of the view or seen at `zoom < 0.4` is a
//! box with its title and its size, the SVG that left the DOM is kept and
//! mounted again when the object comes back to the view, and the culling is
//! recomputed from the camera with the rectangles the state already carries
//! (never with a `getBBox()` per object and per frame).

const CSS: &str = include_str!("../frontend/style.css");
const JS: &str = include_str!("../frontend/plan.js");

/// The text between two markers of a file.
fn slice_between<'a>(text: &'a str, start: &str, end: &str) -> &'a str {
    let from = text
        .find(start)
        .unwrap_or_else(|| panic!("the file must contain {start:?}"));
    let rest = &text[from + start.len()..];
    let to = rest
        .find(end)
        .unwrap_or_else(|| panic!("{start:?} must be closed by {end:?}"));
    &rest[..to]
}

/// The value of one declaration of a CSS block.
fn declaration<'a>(block: &'a str, token: &str) -> &'a str {
    let at = block
        .find(&format!("{token}:"))
        .unwrap_or_else(|| panic!("{token} must be declared: {block}"));
    let rest = &block[at + token.len() + 1..];
    rest[..rest.find(';').unwrap_or(rest.len())].trim()
}

/// How many times `needle` appears in `haystack`.
fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

#[test]
fn the_canvas_keeps_at_most_three_svgs_in_the_dom() {
    // RNF1-ter: the ceiling and the zoom of the degradation are named
    // constants, not numbers scattered through the code.
    assert!(
        JS.contains("const MAX_LIVE_SVGS = 3;"),
        "RNF1-ter: el canvas mantiene como máximo 3 SVGs en el DOM"
    );
    assert!(
        JS.contains("const CULL_ZOOM = 0.4;"),
        "RNF1-ter y §7.2: por debajo de 0.4 el diagrama es un recuadro"
    );

    // One single place decides: it takes the objects that pass the test and
    // keeps the SVG of the first `MAX_LIVE_SVGS` of them.
    let culling = slice_between(JS, "function applyCulling() {", "\n}");
    assert!(
        culling.contains("candidates.length"),
        "the ceiling counts the drawings of the page, not the diagrams: {culling}"
    );
    assert!(
        culling.contains("index < MAX_LIVE_SVGS"),
        "the objects past the ceiling lose their SVG: {culling}"
    );
    assert!(
        culling.contains("materializeObject("),
        "the ones inside the ceiling keep their drawing: {culling}"
    );
    assert!(
        culling.contains("cullObject("),
        "the ones outside the ceiling are boxes: {culling}"
    );
    // The function is called nowhere but inside the one place that decides,
    // or two places could disagree about which drawing is in the page.
    let elsewhere = JS.replace(culling, "");
    for definition in [
        "function materializeObject(element) {",
        "function cullObject(element) {",
    ] {
        let name = definition
            .trim_start_matches("function ")
            .split('(')
            .next()
            .expect("a name");
        let callers = elsewhere.replace(definition, "");
        assert!(
            !callers.contains(&format!("{name}(")),
            "`{name}` is decided in `applyCulling` and nowhere else"
        );
    }
}

#[test]
fn an_object_out_of_the_view_or_below_zoom_04_is_a_box() {
    let culling = slice_between(JS, "function applyCulling() {", "\n}");
    assert!(
        culling.contains("const zoomedOut = camera.zoom < CULL_ZOOM;"),
        "RNF1-ter: below `zoom 0.4` every diagram is a box: {culling}"
    );
    assert!(
        culling.contains("if (zoomedOut || !rectsIntersect(element.rect, view)) {"),
        "an object is culled when the zoom is low **or** it left the view: {culling}"
    );

    // The view is the rectangle of the plan the camera shows, in plan units.
    let view = slice_between(JS, "function visibleRect() {", "\n}");
    for token in [
        "-camera.tx / camera.zoom",
        "-camera.ty / camera.zoom",
        "viewport.w / camera.zoom",
        "viewport.h / camera.zoom",
    ] {
        assert!(
            view.contains(token),
            "the visible rectangle of the plan is `{token}`: {view}"
        );
    }

    // Two rectangles that share no point do not intersect: the test is the
    // four comparisons, not a distance or a bounding circle.
    let intersects = slice_between(JS, "function rectsIntersect(a, b) {", "\n}");
    for token in [
        "a.x < b.x + b.w",
        "b.x < a.x + a.w",
        "a.y < b.y + b.h",
        "b.y < a.y + a.h",
    ] {
        assert!(
            intersects.contains(token),
            "`rectsIntersect` must test `{token}`: {intersects}"
        );
    }

    // The PRD fixes the ceiling and not the tie-break: the three objects
    // closest to the centre of the view are the ones that stay materialized.
    assert!(
        culling.contains("candidates.sort((left, right) => left.distance - right.distance)"),
        "the tie-break is the distance to the centre of the view: {culling}"
    );
    assert!(
        culling.contains("distanceToView(element.rect, view)"),
        "the distance is measured with the rectangle of the object: {culling}"
    );
}

#[test]
fn the_box_carries_the_title_and_the_size_of_the_object() {
    // RNF1-ter: the box says which diagram it stands for and how big it is,
    // e.g. `schema — 320 × 180`.
    let text = slice_between(JS, "function boxText(element) {", "\n}");
    assert!(
        text.contains("${title} — ${width} × ${height}"),
        "the box reads `título — ancho × alto`: {text}"
    );
    for token in ["Math.round(element.rect.w)", "Math.round(element.rect.h)"] {
        assert!(
            text.contains(token),
            "the size of the box is the size of the object, `{token}`: {text}"
        );
    }

    // The title is the one Rust knows: the page never invents one (§8.6.1).
    let read = slice_between(JS, "function readObject(element, diagram) {", "\n}");
    assert!(
        read.contains("element.caption = diagram.title"),
        "the title of the box comes from the state: {read}"
    );
    assert!(
        text.contains("element.caption"),
        "the box reads the title the state gave it: {text}"
    );

    // The box occupies the very rectangle of the object, in plan units, and
    // that rectangle is the SVG plus its 24 px padding (PRD §7.2).
    let paint = slice_between(JS, "function paintBox(element) {", "\n}");
    for token in [
        "element.box.style.left",
        "element.box.style.top",
        "element.box.style.width",
        "element.box.style.height",
        "element.boxLabel.textContent",
    ] {
        assert!(
            paint.contains(token),
            "the box is painted with `{token}`: {paint}"
        );
    }
    let insert = slice_between(JS, "function insertDiagram(payload) {", "\n}");
    for token in [
        "w: width + 2 * OBJECT_PADDING",
        "h: height + 2 * OBJECT_PADDING",
    ] {
        assert!(
            insert.contains(token),
            "the rectangle of the object is `{token}` (PRD §7.2): {insert}"
        );
    }

    // The box is created with the object and stays hidden until the culling
    // asks for it, so the plan never changes shape.
    assert!(
        insert.contains("const box = document.createElement(\"div\")"),
        "the box of the object is created with it: {insert}"
    );
    assert!(
        insert.contains("container.box = box;"),
        "the box hangs from the object it stands in for: {insert}"
    );
    assert!(
        insert.contains("\"diagram-box hidden\""),
        "the box starts hidden: {insert}"
    );

    // A dashed border, painted with the tokens of the theme, so it reads in
    // the dark plan and in the light one (RF22, S18).
    let css = slice_between(CSS, ".diagram-box {", "}");
    assert!(
        declaration(css, "border").contains("dashed"),
        "the box of a culled diagram has a dashed border: {css}"
    );
    for token in ["var(--surface)", "var(--text)", "var(--hairline)"] {
        assert!(
            css.contains(token),
            "the box paints with `{token}`, so both themes reach it: {css}"
        );
    }
    let light = slice_between(CSS, "[data-theme=\"light\"] {", "}");
    for token in ["--surface:", "--text:", "--hairline:"] {
        assert!(
            light.contains(token),
            "the light theme declares `{token}`: {light}"
        );
    }
}

#[test]
fn the_svg_of_a_culled_object_is_kept_and_mounted_again() {
    let insert = slice_between(JS, "function insertDiagram(payload) {", "\n}");
    // The node itself is kept on the object: coming back does not parse the
    // markup of the render again.
    assert!(
        insert.contains("container.svg = svg;"),
        "the object keeps the node of its drawing: {insert}"
    );
    assert!(
        !insert.contains("container.svg = payload.svg"),
        "the drawing is kept as a node, not as the string of the render: {insert}"
    );

    let cull = slice_between(JS, "function cullObject(element) {", "\n}");
    assert!(
        cull.contains("element.svg.remove()"),
        "the culling takes the drawing out of the DOM: {cull}"
    );
    assert!(
        !cull.contains("element.svg ="),
        "the detached node is kept, never dropped: {cull}"
    );
    assert!(
        !cull.contains("innerHTML"),
        "the culling never re-parses the markup: {cull}"
    );
    assert!(
        cull.contains("element.box.classList.remove(\"hidden\")"),
        "the box takes the place of the drawing: {cull}"
    );

    // The `div.diagram` and its `div.content` stay where they are: the pan
    // and the zoom never move an object because of the culling (PRD §7.2).
    for forbidden in [
        "element.remove()",
        "element.content.remove()",
        "element.style.left",
        "element.style.top",
    ] {
        assert!(
            !cull.contains(forbidden),
            "the culling changes the content, not the object: found {forbidden}"
        );
    }
    // The content keeps its size while the drawing is out, so the object of
    // the diagram does not change shape when it is culled.
    for token in [
        "content.style.width = `${width}px`",
        "content.style.height = `${height}px`",
    ] {
        assert!(
            insert.contains(token),
            "the content of the object keeps its size, `{token}`: {insert}"
        );
    }

    let mount = slice_between(JS, "function materializeObject(element) {", "\n}");
    assert!(
        mount.contains("element.content.prepend(element.svg)"),
        "the same node goes back into the object: {mount}"
    );
    assert!(
        mount.contains("element.box.classList.add(\"hidden\")"),
        "the box leaves when the drawing comes back: {mount}"
    );
    assert!(
        !mount.contains("innerHTML"),
        "mounting again never re-parses the markup: {mount}"
    );
}

#[test]
fn the_culling_is_recomputed_from_the_camera() {
    // Every camera change ends in `applyCamera()`, so that is where the
    // culling is due: each frame of the pan, each wheel step, each insertion,
    // focus and fit.
    let camera = slice_between(JS, "function applyCamera() {", "\n}");
    assert!(
        camera.contains("applyCulling()"),
        "the culling follows the camera: {camera}"
    );
    let moved = camera
        .find("element.style.transform")
        .expect("the camera moves the objects");
    let culled = camera
        .find("applyCulling()")
        .expect("the camera decides the culling");
    assert!(
        moved < culled,
        "the objects move first and are culled after: {camera}"
    );

    // One call with the camera and one after a state; nowhere else, or two
    // places could disagree about which drawing is in the page.
    assert_eq!(
        count(JS, "applyCulling();"),
        2,
        "the culling is decided in `applyCamera` and after the state"
    );

    // A state may move an object or give it a title (PRD §8.6.1, S8): both
    // are read there and the culling is decided again.
    let sync = slice_between(JS, "function syncObjects() {", "\n}");
    assert!(
        sync.contains("readObject("),
        "the rectangle and the title of the object come from the state: {sync}"
    );
    assert!(
        sync.contains("applyCulling()"),
        "a state recomputes the culling: {sync}"
    );

    // Cheap: the rectangle of the state, never a layout read per object and
    // per frame.
    for definition in [
        "function visibleRect() {",
        "function rectsIntersect(a, b) {",
        "function distanceToView(rect, view) {",
        "function boxText(element) {",
        "function paintBox(element) {",
        "function applyCulling() {",
    ] {
        let body = slice_between(JS, definition, "\n}");
        for forbidden in ["getBBox", "getBoundingClientRect"] {
            assert!(
                !body.contains(forbidden),
                "`{definition}` works with the rectangle of the state, never \
                 with `{forbidden}`: {body}"
            );
        }
    }
    let culling = slice_between(JS, "function applyCulling() {", "\n}");
    assert!(
        culling.contains("element.rect"),
        "the rectangle tested is the one the object carries: {culling}"
    );
}

#[test]
fn the_box_never_takes_the_pointer_of_the_camera() {
    // The wheel and the pan are the camera's: the box is a picture of the
    // object, not a control.
    let css = slice_between(CSS, ".diagram-box {", "}");
    assert_eq!(
        declaration(css, "pointer-events"),
        "none",
        "the box of a culled diagram never takes the pointer: {css}"
    );
    assert!(
        !JS.contains(".box.addEventListener"),
        "the box listens to nothing: the plan keeps the wheel and the drag"
    );
    // It does not bury the controls of the object either: the box is painted
    // under the content, so the `×` of a culled diagram keeps showing up on
    // hover and stays clickable (PRD §7 RF28).
    let insert = slice_between(JS, "function insertDiagram(payload) {", "\n}");
    assert!(
        insert.contains("container.insertBefore(box, content)"),
        "the box goes under the content of the object, not over its `×`: {insert}"
    );
}

#[test]
fn the_insertion_still_answers_after_two_frames_and_paints_every_object() {
    // The behaviour that was already there and that now shares its place with
    // the culling: the page acknowledges a painted render after **two**
    // animation frames (PRD §8.6.1, step 4), and the camera transform of
    // `applyCamera` never skips an object.
    let insert = slice_between(JS, "function insertDiagram(payload) {", "\n}");
    let success = "emit(\"render_ack\", { req_id: payload.req_id, ok: true })";
    let ack = insert
        .find(success)
        .unwrap_or_else(|| panic!("the plan must acknowledge a painted render: {insert}"));
    assert_eq!(
        count(&insert[..ack], "requestAnimationFrame("),
        2,
        "the answer leaves after two animation frames: {insert}"
    );

    let camera = slice_between(JS, "function applyCamera() {", "\n}");
    assert!(
        camera.contains("for (const element of objects.values())"),
        "the camera still walks every object of the plan: {camera}"
    );
    assert!(
        camera.contains("element.style.transform = transform"),
        "every object of the plan moves with the camera: {camera}"
    );
}

/// A2 de `f8-planos.md`: un objeto de un plano que no está en pantalla no
/// guarda su dibujo en la página. Si no, el techo de RNF1-ter contaría
/// dibujos que el humano no ve, y volver a un plano traería SVGs viejos que
/// el culling ya había decidido dejar fuera.
#[test]
fn a_plan_that_is_not_on_screen_keeps_no_drawing_in_the_page() {
    let culling = slice_between(JS, "function applyCulling() {", "\n}");
    assert!(
        culling.contains("element.classList.contains(\"plan-hidden\")"),
        "el plano que no está en pantalla no aporta dibujos al DOM: {culling}"
    );
    let hidden = culling
        .find("plan-hidden")
        .expect("the culling knows the plans that are not on screen");
    let culled = culling
        .find("cullObject(element)")
        .expect("a hidden object loses its drawing");
    assert!(
        hidden < culled,
        "el objeto del plano que no está en pantalla se cull-ea antes de contar: {culling}"
    );
    let css = slice_between(CSS, ".diagram.plan-hidden {", "}");
    assert_eq!(
        declaration(css, "display"),
        "none",
        "el plano que no está en pantalla no se pinta: {css}"
    );
}

/// Un pan en vuelo escribía la cámara en cada frame y pisaba la intención que
/// acababa de llegar: el `opts.focus` de un render terminaba mostrando el
/// diagrama anterior. La intención tiene que cancelar el pan.
#[test]
fn a_camera_intent_cancels_the_pan_in_flight() {
    let body = slice_between(JS, "function applyCameraIntent(intent) {", "\n}");
    assert!(
        body.contains("cancelPan()"),
        "la intención de cámara tiene que cancelar el pan que está corriendo: {body}"
    );
}
