//! The panel of the open diagrams (PRD §7 RF26, RF28, S1, S3).
//!
//! No test can click a button inside the webview, so what is pinned here is
//! the contract the page keeps with Rust: the panel exists with a list and a
//! close button per entry, it is built **only** from `graphium://state`
//! (PRD §8.6.1, S8), it asks Rust to close (`graphium://close` with
//! `diagram_id`) and never deletes a diagram by itself, the camera
//! **travels** in an animated pan of 250 ms (PRD §7.2), and an insertion on a
//! plan that already has objects never touches the zoom of the human (PRD
//! §7.2, L2 of `odd/tasks/f2-varios-en-el-plano.md`).

const HTML: &str = include_str!("../frontend/index.html");
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

/// The whole tag that carries `id="{id}"`.
fn tag_of<'a>(html: &'a str, id: &str) -> &'a str {
    let at = html
        .find(&format!("id=\"{id}\""))
        .unwrap_or_else(|| panic!("the page must have an element with id `{id}`"));
    let start = html[..at].rfind('<').expect("the id lives inside a tag");
    let end = at + html[at..].find('>').expect("the tag is closed") + 1;
    &html[start..end]
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
fn the_panel_lists_the_open_diagrams_with_a_close_button_each() {
    // The page has the panel and the list its entries go in.
    let panel = tag_of(HTML, "panel");
    assert!(
        panel.contains("aria-label="),
        "the panel is named for a keyboard and a reader: {panel}"
    );
    let list = tag_of(HTML, "panel-list");
    assert!(
        list.trim_start().starts_with("<ul"),
        "the entries of the panel are a list, not a paragraph: {list}"
    );

    // RF26: every entry reads the title, the kind and when it was updated,
    // and carries its own close button.
    let item = slice_between(JS, "function panelItem(diagram) {", "\n}");
    for field in ["diagram.title", "diagram.kind", "diagram.updated_at"] {
        assert!(
            item.contains(field),
            "an entry of the panel reads `{field}` (RF26): {item}"
        );
    }
    assert!(
        item.contains("\"panel-item-close\""),
        "an entry carries its own close button (RF26): {item}"
    );
    assert!(
        item.contains("\"aria-label\""),
        "the close button of an entry has a name, or a reader announces `×`: {item}"
    );
    assert!(
        item.contains("emitClose(diagram.diagram_id)"),
        "closing an entry asks Rust to close that diagram (RF28): {item}"
    );
}

#[test]
fn the_panel_is_built_only_from_the_state_event() {
    // PRD §8.6.1, S8: the panel is rebuilt from `graphium://state` and from
    // nothing else. The event is the whole model, so the page has no state
    // of its own to add to it.
    let handler = slice_between(JS, "listen(\"graphium://state\", (event) => {", "\n});");
    assert!(
        handler.contains("state.diagrams = diagrams"),
        "the state event is what the page keeps (S8): {handler}"
    );
    assert!(
        handler.contains("renderPanel()"),
        "the panel is rebuilt when the state arrives (S8): {handler}"
    );

    let body = slice_between(JS, "function renderPanel() {", "\n}");
    assert!(
        body.contains("state.diagrams"),
        "the list comes from the state Rust owns: {body}"
    );
    assert_eq!(
        count(JS, "panelList.replaceChildren"),
        1,
        "the list of the panel is filled in one single place"
    );
    assert!(
        body.contains("panelList.replaceChildren"),
        "that single place is `renderPanel`: {body}"
    );
    assert_eq!(
        count(JS, "document.createElement(\"li\")"),
        1,
        "an entry is created in one single place, or two shapes of entry can drift"
    );

    // No other event builds the panel: a render paints an object, never an
    // entry, and the settings never touch it.
    let render = slice_between(JS, "listen(\"graphium://render\", (event) => {", "\n});");
    assert!(
        !render.contains("renderPanel"),
        "the render event must not add an entry: the panel comes from the state (S8): {render}"
    );
    let settings = slice_between(JS, "listen(\"graphium://settings\", (event) => {", "\n});");
    assert!(
        !settings.contains("renderPanel"),
        "the settings event must not touch the panel: {settings}"
    );
}

#[test]
fn closing_asks_rust_and_the_page_never_deletes_a_diagram_itself() {
    // The event the page emits, with the key Rust reads (RF28, S3).
    let emit_close = slice_between(JS, "function emitClose(diagramId) {", "\n}");
    assert!(
        emit_close.contains("emit(\"graphium://close\", { diagram_id: diagramId })"),
        "the page asks Rust to close with `graphium://close` and `diagram_id`: {emit_close}"
    );

    // The `×` over the object is the second door to the same place (RF28).
    let insert = slice_between(JS, "function insertDiagram(payload) {", "\n}");
    assert!(
        insert.contains("\"diagram-close\""),
        "the object carries its own `×` (RF28, S3): {insert}"
    );
    assert!(
        insert.contains("emitClose(payload.diagram_id)"),
        "the `×` of the object asks Rust to close that diagram: {insert}"
    );

    // The `×` lives on the object and the plan drags the camera from the
    // background: the pointer down on the button must not start a pan.
    let at = insert
        .find("\"pointerdown\"")
        .expect("the `×` has to survive the drag of the camera");
    let handler = &insert[at..];
    let end = handler
        .find("});")
        .expect("the pointer down handler is closed");
    assert!(
        handler[..end].contains("stopPropagation()"),
        "the `×` must not start the pan of the camera: {}",
        &handler[..end]
    );

    // Nothing of the model is deleted here: the only removal is the painted
    // object of a diagram the state stopped listing.
    assert_eq!(
        count(JS, "objects.delete("),
        1,
        "a diagram leaves the plan from the state, not from a click"
    );
    let sync = slice_between(JS, "function syncObjects() {", "\n}");
    assert!(
        sync.contains("state.diagrams"),
        "what stays painted is what the state lists: {sync}"
    );
    assert!(
        sync.contains("objects.delete("),
        "the removal reads the state: {sync}"
    );
}

#[test]
fn the_camera_travels_in_an_animated_pan_of_250_ms() {
    assert!(
        JS.contains("const PAN_MS = 250;"),
        "the pan of the plan lasts 250 ms (PRD §7.2, RF26, RF27)"
    );
    let pan = slice_between(JS, "function panTo(target) {", "\n}");
    for token in ["requestAnimationFrame", "PAN_MS", "performance.now()"] {
        assert!(
            pan.contains(token),
            "the pan is animated over `{token}`, never a jump: {pan}"
        );
    }

    // RF26: choosing an entry moves the camera to that diagram.
    let select = slice_between(JS, "function selectDiagram(diagramId) {", "\n}");
    assert!(
        select.contains("selectedId = diagramId"),
        "the chosen diagram is marked as the selected one: {select}"
    );
    assert!(
        select.contains("renderPanel()"),
        "the mark is painted with the list, not on top of it: {select}"
    );
    assert!(
        select.contains("focusDiagram(diagram)"),
        "choosing an entry moves the camera to that diagram (RF26): {select}"
    );

    let focus = slice_between(JS, "function focusDiagram(diagram) {", "\n}");
    assert!(
        focus.contains("panTo(cameraOn(center))"),
        "the selection travels with the animated pan: {focus}"
    );
    assert!(
        !focus.contains("camera.tx"),
        "the selection reads the camera, it never writes it directly: {focus}"
    );

    // Keeping the zoom (RF26): the target of a pan carries the translation
    // and nothing else.
    let target = slice_between(JS, "function cameraOn(center) {", "\n}");
    assert!(
        target.contains("camera.zoom"),
        "the target is centered at the zoom of the human: {target}"
    );
    assert!(
        !target.contains("zoom:"),
        "the pan never carries a zoom of its own: {target}"
    );
}

#[test]
fn an_insertion_on_a_plan_with_objects_keeps_the_zoom_of_the_human() {
    // PRD §7.2; A1/A2 of `f8-planos.md`: one plan per session, so only the
    // first diagram of **its** plan is framed; from the second one of that
    // plan on, the camera pans until the new object.
    let first = slice_between(JS, "function framesOnInsert(sessionId) {", "\n}");
    for token in ["objects.values()", "element.sessionId", "=== sessionId"] {
        assert!(
            first.contains(token),
            "the plan frames a render only when its own plan holds no object \
             yet, with `{token}`: {first}"
        );
    }

    let insert = slice_between(JS, "function insertDiagram(payload) {", "\n}");
    let asks = insert
        .find("framesOnInsert(")
        .expect("the insertion has to ask whether the plan is empty");
    let lands = insert
        .find("objects.set(")
        .expect("the insertion lands the object on the plan");
    assert!(
        asks < lands,
        "the answer is read before the new object lands, or the first diagram \
         counts as the second one: {insert}"
    );

    assert_eq!(
        count(JS, "applyCameraIntent("),
        2,
        "one definition and one call: the camera of the event is applied in a \
         single place"
    );
    // El encuadre de la cámara del evento se aplica en dos casos y en un solo
    // sitio: el primer diagrama del plano y cuando el agente pidió
    // `opts.focus` (PRD §7, RF27).
    let framed = slice_between(
        insert,
        "if (first || payload.focus) {",
        "} else if (placement) {",
    );
    assert!(
        framed.contains("applyCameraIntent(payload.camera)"),
        "the first diagram and the focused one use the camera of the event: {framed}"
    );
    assert!(
        insert.contains("payload.focus"),
        "`opts.focus` tiene que llegar a la decisión de la cámara: {insert}"
    );

    let after = insert
        .find("} else if (placement) {")
        .expect("a later insertion has its own branch");
    let later = &insert[after..];
    assert!(
        later.contains("panTo(cameraOn(center))"),
        "a later insertion pans until the new object (§7.2, L2): {later}"
    );
    assert!(
        !later.contains("applyCameraIntent"),
        "a later insertion must not apply the camera of the event: {later}"
    );
    assert!(
        !insert.contains("camera.zoom ="),
        "the insertion never writes the zoom: the human owns it: {insert}"
    );
}

#[test]
fn the_panel_is_a_sibling_of_the_plan_and_reads_in_both_themes() {
    // The panel and its toggle are siblings of `#plan`, never children: the
    // wheel and the drag of the camera listen on the plan, so a control over
    // it can never steal them.
    let plan = HTML.find("<div id=\"plan\"").expect("the plan of the page");
    let panel = HTML.find("id=\"panel\"").expect("the panel of the page");
    assert!(panel > plan, "the panel comes after the plan in the page");
    let between = &HTML[plan..panel];
    assert_eq!(
        count(between, "</div>"),
        count(between, "<div"),
        "every element opened before the panel is closed before it starts: the \
         panel is a sibling of the plan, not a child"
    );

    let toggle = tag_of(HTML, "panel-toggle");
    assert!(
        toggle.contains("aria-label=")
            && toggle.contains("aria-expanded=")
            && toggle.contains("aria-controls=\"panel\""),
        "hiding and showing the panel is one named button a keyboard reaches: {toggle}"
    );
    let panel_tag = tag_of(HTML, "panel");
    assert!(
        panel_tag.contains("class=\"panel\""),
        "the panel starts open, the way RF26 asks for it: {panel_tag}"
    );

    let body = slice_between(JS, "function setDiagramPanelOpen(open) {", "\n}");
    assert!(
        body.contains("panel.classList.toggle(\"hidden\", !open)"),
        "the panel is hidden and shown, not rebuilt: {body}"
    );
    assert!(
        body.contains("\"aria-expanded\""),
        "the toggle says whether the panel is open: {body}"
    );

    // The panel is chrome of the page: it never listens to the camera.
    for forbidden in [
        "panel.addEventListener(\"wheel\"",
        "panel.addEventListener(\"pointerdown\"",
    ] {
        assert!(
            !JS.contains(forbidden),
            "the panel must not intercept the camera, found {forbidden}"
        );
    }

    // Legible in both themes: the panel paints with the tokens of the theme,
    // and the light theme is the one that redefines them (RF22, S18).
    let css = slice_between(CSS, ".panel {", "}");
    for token in ["var(--surface)", "var(--surface-border)", "var(--text)"] {
        assert!(
            css.contains(token),
            "the panel paints with `{token}`, so both themes reach it: {css}"
        );
    }
    let light = slice_between(CSS, "[data-theme=\"light\"] {", "}");
    for token in ["--surface:", "--surface-border:", "--text:"] {
        assert!(
            light.contains(token),
            "the light theme declares `{token}`: {light}"
        );
    }

    // The `×` of the object shows up when the pointer walks over it and never
    // before, and it does not eat the pointer while it is invisible.
    let close = slice_between(CSS, ".diagram-close {", "}");
    assert_eq!(
        declaration(close, "opacity"),
        "0",
        "the `×` is invisible until the pointer arrives: {close}"
    );
    assert_eq!(
        declaration(close, "pointer-events"),
        "none",
        "an invisible `×` never takes a click: {close}"
    );
    assert!(
        CSS.contains(".diagram:hover .diagram-close"),
        "the `×` appears when the pointer walks over the object"
    );
}

/// El `×` de un objeto cuelga de su esquina: sin un contorno visible, cuando
/// la esquina del rectángulo no tiene tinta (un árbol es ancho abajo, no
/// arriba) el botón parece flotar solo. Al pasar el puntero se ve el objeto.
#[test]
fn the_close_button_hangs_from_a_visible_object() {
    let style = include_str!("../frontend/style.css");
    let flat: String = style.split_whitespace().collect::<Vec<_>>().join(" ");

    assert!(
        flat.contains(".diagram:hover > .content, .diagram:focus-within > .content {"),
        "el objeto tiene que mostrar su contorno al pasar el puntero"
    );
    assert!(
        flat.contains("outline: 1px dashed var(--hairline);"),
        "el contorno se dibuja con el token del tema, no con un color suelto"
    );
    // `outline` no entra en el layout: el objeto no cambia de tamaño al pasar
    // el puntero.
    assert!(
        !flat.contains(".diagram:hover > .content, .diagram:focus-within > .content { border:"),
        "el contorno no puede cambiar el tamaño del objeto"
    );
}

/// El `×` de un objeto tiene que poder apretarse: si vive fuera del
/// rectángulo del objeto, el puntero sale del `:hover` al ir hacia él y el
/// botón se esconde justo antes del clic.
#[test]
fn the_close_button_lives_inside_its_object() {
    let style = include_str!("../frontend/style.css");
    let flat: String = style.split_whitespace().collect::<Vec<_>>().join(" ");

    let block = flat
        .split(".diagram-close {")
        .nth(1)
        .and_then(|rest| rest.split('}').next())
        .expect("the close button has a rule of its own");
    assert!(
        !block.contains("top: -") && !block.contains("right: -"),
        "el `×` no puede colgar fuera del objeto: {block}"
    );
    assert!(
        block.contains("top: 4px") && block.contains("right: 4px"),
        "el `×` se ancla dentro de la esquina del objeto: {block}"
    );
}
