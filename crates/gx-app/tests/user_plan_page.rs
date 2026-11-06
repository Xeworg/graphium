//! The page of the user plan (`odd/tasks/f8-planos.md`, B1/B2/B3; S1/S2/S4;
//! V2).
//!
//! No test can click inside the webview, so what is pinned here is the contract
//! the page keeps with the owner: «Mi plano» is the first row of the switcher
//! and never leaves it, the panel item carries the button that asks Rust for a
//! clone, the clones are objects of the user plan (and of no other plan), the
//! refresh and the history of a clone are questions to Rust, and dragging an
//! object moves the clone while dragging the background keeps panning the
//! camera.

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

/// The value of one declaration of a CSS block.
fn declaration<'a>(block: &'a str, token: &str) -> &'a str {
    let at = block
        .find(&format!("{token}:"))
        .unwrap_or_else(|| panic!("{token} must be declared: {block}"));
    let rest = &block[at + token.len() + 1..];
    rest[..rest.find(';').unwrap_or(rest.len())].trim()
}

/// S4 (verbatim): «el conmutador muestra el plano del usuario arriba de las
/// sesiones». La fila es propia, no sale de `state.sessions` y está siempre:
/// el plano del humano no depende de que haya una sesión viva.
#[test]
fn the_user_plan_is_the_first_row_of_the_switcher_and_never_leaves() {
    assert!(
        JS.contains("const USER_PLAN_ID = \"user\";"),
        "el plano del usuario tiene su propia clave en el conmutador (S4)"
    );
    assert!(
        JS.contains("label: \"Mi plano\""),
        "la fila se llama «Mi plano» (B1)"
    );

    let rows = slice_between(JS, "function renderSessions() {", "\n}");
    let user = rows
        .find("userPlanRow()")
        .unwrap_or_else(|| panic!("el plano del usuario es una fila del conmutador: {rows}"));
    let sessions = rows
        .find("state.sessions.map(sessionRow)")
        .unwrap_or_else(|| panic!("las sesiones siguen siendo las filas del conmutador: {rows}"));
    assert!(
        user < sessions,
        "el plano del usuario va arriba de las sesiones (S4): {rows}"
    );
    assert_eq!(
        rows.matches("userPlanRow()").count(),
        1,
        "la fila del plano del usuario se arma en un solo lugar: {rows}"
    );

    // The row belongs to the human, not to a session: it is built with no
    // input, it marks the plan on screen and choosing it brings that plan.
    let row = slice_between(JS, "function userPlanRow() {", "\n}");
    assert!(
        row.contains("isVisiblePlan(USER_PLAN_ID)"),
        "la fila se marca cuando su plano está en pantalla: {row}"
    );
    assert!(
        row.contains("\"aria-pressed\""),
        "el lector sabe qué plano está en pantalla: {row}"
    );
    assert!(
        row.contains("showPlan(USER_PLAN)"),
        "elegir la fila trae el plano del usuario: {row}"
    );
    assert!(
        !row.contains("state.sessions"),
        "la fila del plano del usuario no depende de ninguna sesión: {row}"
    );
}

/// S1/S4: el plano del usuario es un plano más para la cámara y el estado
/// vacío, y no se lo lleva ninguna sesión que termina.
#[test]
fn the_user_plan_is_a_plan_like_any_other_for_the_camera() {
    // Its camera lives in the same map as every other plan (S4).
    let show = slice_between(JS, "function showPlan(plan) {", "\n}");
    assert!(
        show.contains("planCameras.set(activePlan.id"),
        "la cámara del plano que se deja se guarda por su clave, el del usuario incluido: {show}"
    );

    // A plan nobody has visited yet frames its newest object: for the user
    // plan, the newest clone.
    let restore = slice_between(JS, "function restorePlanCamera() {", "\n}");
    assert!(
        restore.contains("planCameras.get(activePlan.id)"),
        "la cámara que vuelve es la del plano en pantalla: {restore}"
    );
    assert!(
        restore.contains("newestCenter(activePlan.id)"),
        "un plano sin cámara guardada se encuadra en su objeto más nuevo: {restore}"
    );
    let newest = slice_between(JS, "function newestCenter(planId) {", "\n}");
    assert!(
        newest.contains("planId === USER_PLAN_ID") && newest.contains("state.clones"),
        "el objeto más nuevo del plano del usuario es su clon más nuevo: {newest}"
    );

    // The user plan never ends with a session: the switcher keeps it.
    let reconcile = slice_between(JS, "function reconcileActivePlan() {", "\n}");
    assert!(
        reconcile.contains("activePlan.id === USER_PLAN_ID"),
        "terminar una sesión no se lleva el plano del usuario: {reconcile}"
    );

    // The empty surface counts the objects of the plan on screen.
    let count = slice_between(JS, "function planDiagramCount() {", "\n}");
    assert!(
        count.contains("state.clones.length"),
        "el plano del usuario cuenta sus clones: {count}"
    );
    assert!(
        JS.contains("const emptyUser = document.getElementById(\"empty-user\");"),
        "el plano del usuario tiene su propio estado vacío (B1)"
    );
    assert!(
        slice_between(JS, "function paintEmpty() {", "\n}").contains("emptyUser"),
        "el estado vacío del plano del usuario se pinta desde un solo lugar"
    );
}

/// B1 (S2, S4): clonar es un botón del ítem del panel, con la misma gramática
/// que la `×`, y la página **pregunta**: el clon es una instancia de vista del
/// dueño, así que la página no inventa ni un id ni un dibujo.
#[test]
fn the_panel_item_asks_rust_for_a_clone() {
    let item = slice_between(JS, "function panelItem(diagram) {", "\n}");
    assert!(
        item.contains("panel-item-clone"),
        "el ítem del panel lleva el botón de clonar: {item}"
    );
    assert!(
        item.contains("`clonar ${diagram.title} al plano del usuario`"),
        "el botón dice qué clona y para quién: {item}"
    );
    assert!(
        item.contains("emitClone(diagram.diagram_id)"),
        "clonar es una pregunta al dueño: {item}"
    );

    let emit = slice_between(JS, "function emitClone(diagramId) {", "\n}");
    assert!(
        emit.contains("emit(\"graphium://clone\", { diagram_id: diagramId })"),
        "la página pide el clon con el id del diagrama original: {emit}"
    );
    assert!(
        !JS.contains("\"c-\""),
        "la página no inventa ids de clon: los numera el dueño (S4)"
    );
}

/// B1 (S2, S4): los clones son objetos del plano del usuario y de ningún otro.
/// Salen del estado, se ocultan con el plano que no está en pantalla, no
/// tienen `×` y no hay forma de que el agente los toque desde la página.
#[test]
fn the_clones_are_objects_of_the_user_plan_only() {
    // The state carries them, additive to the diagrams (B1).
    let handler = slice_between(JS, "listen(\"graphium://state\", (event) => {", "\n});");
    assert!(
        handler.contains("state.clones = Array.isArray(payload.clones)"),
        "los clones vienen del estado que emite el dueño: {handler}"
    );

    // The reconciliation knows both kinds of object and drops only what the
    // state does not list any more.
    let sync = slice_between(JS, "function syncObjects() {", "\n}");
    assert!(
        sync.contains("objectIds()"),
        "el estado decide qué objetos quedan en el plano: {sync}"
    );
    let ids = slice_between(JS, "function objectIds() {", "\n}");
    assert!(
        ids.contains("state.diagrams") && ids.contains("state.clones"),
        "los objetos abiertos son los diagramas y los clones: {ids}"
    );

    // A clone is an object of the user plan: that is the key the switcher and
    // the culling use.
    let clone = slice_between(JS, "function createCloneObject(clone) {", "\n}");
    assert!(
        clone.contains("\"diagram clone\"") || clone.contains("diagram clone"),
        "el clon es un objeto del plano con su propia clase: {clone}"
    );
    assert!(
        clone.contains("planId = USER_PLAN_ID"),
        "el clon pertenece al plano del usuario: {clone}"
    );
    assert!(
        !clone.contains("emitClose") && !clone.contains("graphium://close"),
        "el `×` de un clon nunca cierra el diagrama de la sesión (S4): {clone}"
    );
    assert!(
        clone.contains("createCloneClose(container)"),
        "el clon lleva su `×`: el humano puede borrarlo de su plano (L6): {clone}"
    );

    let visible = slice_between(JS, "function applyPlanVisibility() {", "\n}");
    assert!(
        visible.contains("isVisiblePlan(element.planId"),
        "los objetos se esconden por su plano, el del usuario incluido: {visible}"
    );

    // The place and the size come from the owner, like a diagram's.
    let read = slice_between(JS, "function readClone(element, clone) {", "\n}");
    assert!(
        read.contains("clone.pos") && read.contains("clone.bbox"),
        "el clon se pinta donde el dueño lo colocó: {read}"
    );
    assert!(
        read.contains("clone.title"),
        "el título del clon viene del estado: {read}"
    );
}

/// B2 (S3, S4): el refresco y el historial del clon son preguntas al dueño.
/// «Actualizar el clon» se deshabilita cuando el original ya no está, y la
/// página nunca escribe el store: pide la versión y pinta la respuesta.
#[test]
fn the_refresh_and_the_history_of_a_clone_are_asked_to_rust() {
    let tools = slice_between(JS, "function createCloneTools(container) {", "\n}");
    assert!(
        tools.contains("actualizar el clon"),
        "la afinidad del refresco se llama «Actualizar el clon» (B2): {tools}"
    );
    assert!(
        tools.contains("graphium://clone-view") || tools.contains("refreshClone("),
        "el refresco pide la versión actual al dueño: {tools}"
    );

    let update = slice_between(JS, "function updateCloneTools(container) {", "\n}");
    assert!(
        update.contains("updatable"),
        "el refresco se habilita solo mientras el original esté (B2): {update}"
    );
    assert!(
        update.contains("cloneRefresh.disabled") || update.contains("refresh.disabled"),
        "el botón del refresco se deshabilita de verdad: {update}"
    );
    assert!(
        update.contains("oldest_version") && update.contains("version_count"),
        "la historia del clon se lee como la del panel, «v3/7»: {update}"
    );
    assert!(
        update.contains("v${"),
        "el indicador del clon es el mismo del panel: {update}"
    );
    assert!(
        update.contains("shown >= oldest && shown <= current"),
        "una versión que el historial descartó cae en la más nueva dibujable, \
         como en el panel (RF14): {update}"
    );

    let view = slice_between(JS, "function viewCloneVersion(clone, version) {", "\n}");
    assert!(
        view.contains("emit(\"graphium://clone-view\""),
        "mirar una versión del original es una pregunta al dueño: {view}"
    );
    assert!(
        view.contains("viewedCloneVersion.set"),
        "la página recuerda qué versión pidió: {view}"
    );
    assert!(
        !view.contains("graphium://close") && !view.contains("emitClose"),
        "mirar el historial no cierra nada: {view}"
    );

    let refresh = slice_between(JS, "function refreshClone(clone) {", "\n}");
    assert!(
        refresh.contains("emit(\"graphium://clone-view\", { clone_id: clone.clone_id })"),
        "el refresco pide la actual, sin número de versión: {refresh}"
    );

    // The answer of the owner mounts the drawing the clone asked for.
    assert!(
        JS.contains("listen(\"graphium://clone-paint\""),
        "el dueño dibuja y la página monta la respuesta"
    );
}

/// B1 (S2): el dibujo del clon no viaja en el estado. Un clon que la página
/// nunca dibujó (la ventana volvió a nacer) se pide al dueño, y el clon recién
/// clonado aparece en el plano del usuario, que entra en pantalla.
#[test]
fn the_page_asks_for_a_drawing_it_does_not_have() {
    let sync = slice_between(JS, "function syncObjects() {", "\n}");
    assert!(
        sync.contains("element.svg") || sync.contains("element.cloneSvg"),
        "la página sabe si el clon ya tiene dibujo: {sync}"
    );
    assert!(
        sync.contains("emit(\"graphium://clone-view\", {") && sync.contains("snapshot_version"),
        "sin dibujo, la página pide la versión que el clon guarda: {sync}"
    );

    let paint = slice_between(JS, "function applyClonePaint(paint) {", "\n}");
    assert!(
        paint.contains("if (paint.svg)") && paint.contains("mountCloneDrawing("),
        "la respuesta del dueño monta el dibujo del clon, y sin dibujo no pisa \
         el que la página ya tiene (L6: volver a clonar): {paint}"
    );
    assert!(
        paint.contains("if (!paint.bring)"),
        "la página trae a la pantalla el clon que el dueño contestó (L6): {paint}"
    );
    assert!(
        paint.contains("showPlan(USER_PLAN)"),
        "clonar trae el plano del usuario a la pantalla (S2): {paint}"
    );
    assert!(
        paint.contains("updateCloneTools(element)"),
        "la barra del clon se repinta con la respuesta del dueño: {paint}"
    );
    assert!(
        paint.contains("panTo(") || paint.contains("applyCameraIntent("),
        "el plano viaja hasta el clon que acaba de llegar: {paint}"
    );
    let mount = slice_between(
        JS,
        "function mountCloneDrawing(element, payloadSvg) {",
        "\n}",
    );
    assert!(
        mount.contains("querySelector(\"svg\")"),
        "el dibujo del clon es el SVG que el dueño renderizó: {mount}"
    );
    assert!(
        mount.contains("element.svg ="),
        "el dibujo queda en el objeto, como el de un diagrama: {mount}"
    );
}

/// B3 (S1, S4, V2): arrastrar un **objeto** mueve el clon y arrastrar el
/// **fondo** sigue moviendo la cámara. Los objetos de un plano de sesión no se
/// arrastran nunca, y mover un clon escribe solo su posición: no hay store, no
/// hay cámara.
#[test]
fn dragging_a_clone_moves_it_and_the_background_still_pans() {
    let down = slice_between(
        JS,
        "plan.addEventListener(\"pointerdown\", (event) => {",
        "\n});",
    );
    assert!(
        down.contains("cloneUnder("),
        "el puntero distingue un objeto del plano del usuario del fondo: {down}"
    );
    assert!(
        down.contains("dragging = {") && down.contains("camera.tx"),
        "arrastrar el fondo sigue moviendo la cámara (B3): {down}"
    );
    assert!(
        down.contains("moveCloneStart("),
        "arrastrar un objeto empieza el movimiento del clon: {down}"
    );
    assert!(
        down.contains("closest(\"button\")"),
        "un botón del clon no arrastra el objeto: {down}"
    );

    // Only the user plan moves objects; a session plan is the agent's.
    let under = slice_between(JS, "function cloneUnder(target) {", "\n}");
    assert!(
        under.contains("activePlan.id !== USER_PLAN_ID"),
        "los objetos del plano de una sesión no se arrastran (S4): {under}"
    );
    assert!(
        under.contains("closest(\".diagram.clone\")"),
        "solo un clon es un objeto que se mueve: {under}"
    );

    // The move is in plan units: the pointer travels in screen pixels.
    let travel = slice_between(
        JS,
        "plan.addEventListener(\"pointermove\", (event) => {",
        "\n});",
    );
    assert!(
        travel.contains("moving !== null") && travel.contains("/ zoom"),
        "el movimiento del clon descuenta el zoom de la cámara (B3): {travel}"
    );
    // The camera drag is untouched: the background still translates the view
    // and still tells Rust where the camera is (PRD §7.2; A2).
    assert!(
        travel.contains("camera.tx = dragging.tx") && travel.contains("notifyCamera()"),
        "el arrastre del fondo sigue panneando la cámara como antes: {travel}"
    );

    // The owner keeps the place, and the page asks for nothing else.
    let end = slice_between(JS, "function endDrag(event) {", "\n}");
    assert!(
        end.contains("emit(\"graphium://clone-move\""),
        "soltar el clon escribe su posición en el dueño (B3, V2): {end}"
    );
    assert!(
        end.contains("pos: { x: element.rect.x, y: element.rect.y }"),
        "lo que viaja es la posición del objeto: {end}"
    );
    assert!(
        !end.contains("notifyCamera") && !end.contains("graphium://close"),
        "mover un clon no toca la cámara ni cierra nada: {end}"
    );
    assert!(
        end.contains("move.x") && end.contains("element.rect.x !== move.x"),
        "un clic sin movimiento no escribe una posición: {end}"
    );
}

/// L6 (defecto 2: «no veo una forma de borrar los planos de mi plano»): el
/// `×` del clon borra la **instancia de vista** del plano del humano. Es la
/// misma gramática del `×` de un diagrama de sesión, y la página no borra nada
/// por su cuenta: pregunta al dueño y el estado que vuelve saca el objeto.
#[test]
fn the_clone_has_its_own_cross_and_it_asks_the_owner_to_remove_it() {
    let object = slice_between(JS, "function createCloneObject(clone) {", "\n}");
    assert!(
        object.contains("createCloneClose(container)"),
        "el `×` se arma con el objeto del clon: {object}"
    );

    let clone = slice_between(JS, "function createCloneClose(container) {", "\n}");
    assert!(
        clone.contains("\"diagram-close clone-close\""),
        "el `×` del clon es el mismo botón del plano, con su propia clase: {clone}"
    );
    assert!(
        clone.contains("`cerrar el clon`"),
        "el botón dice qué cierra: {clone}"
    );
    assert!(
        clone.contains("emitCloneRemove(container.cloneId)"),
        "el `×` pregunta al dueño: {clone}"
    );
    assert!(
        !clone.contains("objects.delete") && !clone.contains("element.remove()"),
        "la página no se lleva el objeto por su cuenta: espera el estado (S8): {clone}"
    );
    assert!(
        clone.contains("event.stopPropagation()"),
        "el `×` no arrastra el clon al apretarlo (B3): {clone}"
    );

    let emit = slice_between(JS, "function emitCloneRemove(cloneId) {", "\n}");
    assert!(
        emit.contains("emit(\"graphium://clone-remove\", { clone_id: cloneId })"),
        "borrar un clon es una pregunta al dueño: {emit}"
    );

    // The `×` keeps the pointer for itself: pressing it never drags the clone.
    assert!(
        JS.contains("listen(\"graphium://clone-paint\"")
            && JS.contains("applyClonePaint(event.payload || {})"),
        "el estado que vuelve del dueño es el que repinta el plano"
    );
    let ids = slice_between(JS, "function objectIds() {", "\n}");
    assert!(
        ids.contains("state.clones"),
        "un clon que el dueño ya no lista deja el plano: {ids}"
    );

    // The tools of the clone leave the corner to the `×`, and the button
    // paints with the tokens of the plan (RF22).
    let tools = slice_between(CSS, ".diagram-clone-tools {", "}");
    let offset = declaration(tools, "right");
    let pixels: f64 = offset
        .trim_end_matches("px")
        .parse()
        .unwrap_or_else(|_| panic!("the toolbar keeps a right offset: {tools}"));
    assert!(
        pixels >= 24.0,
        "la barra del clon deja lugar al `×` en la esquina: {offset}"
    );
    assert!(
        CSS.contains(".diagram-close:hover"),
        "el `×` del clon hereda la gramática del `×` del plano"
    );
}

/// B3/S4: el clon entra en la maquinaria de selección de la página — agarrarlo
/// lo elige — y ←/→ caminan el historial de su original con las mismas flechas
/// que el panel, sin salir del plano del usuario.
#[test]
fn the_clone_is_selected_and_walks_its_history_with_the_keyboard() {
    let start = slice_between(JS, "function moveCloneStart(event, element) {", "\n}");
    assert!(
        start.contains("selectedId = element.cloneId"),
        "agarrar un clon lo deja elegido, como elegir un ítem del panel: {start}"
    );

    let keys = slice_between(
        JS,
        "window.addEventListener(\"keydown\", (event) => {",
        "\n});",
    );
    assert!(
        keys.contains("state.clones.find"),
        "las flechas encuentran el clon elegido: {keys}"
    );
    assert!(
        keys.contains("viewCloneVersion(clone,"),
        "las flechas caminan el historial del original dentro del clon (B3): {keys}"
    );
    assert!(
        keys.contains("clone.updatable"),
        "una foto sin original no navega: {keys}"
    );
}

/// S4 y RF22: la fila del plano del usuario, el botón de clonar y las
/// herramientas del clon se pintan con los tokens del tema, y el objeto se
/// lee como algo que se puede agarrar.
#[test]
fn the_user_plan_paints_with_the_tokens_of_the_theme() {
    let row = slice_between(CSS, ".session-row.user-plan {", "}");
    assert!(
        declaration(row, "box-shadow").contains("var("),
        "la fila del plano del usuario se separa con un token del tema: {row}"
    );
    let dot = slice_between(CSS, ".session-dot.user-plan-dot {", "}");
    assert!(
        declaration(dot, "background").contains("var("),
        "el punto del plano del usuario no es una familia: es el acento: {dot}"
    );
    let clone = slice_between(CSS, ".diagram.clone {", "}");
    assert!(
        declaration(clone, "cursor").contains("grab"),
        "el clon se lee como algo que se puede arrastrar (B3): {clone}"
    );
    let tools = slice_between(CSS, ".diagram-clone-tools {", "}");
    assert!(
        declaration(tools, "pointer-events").contains("none"),
        "las herramientas del clon no tapan el objeto hasta que el puntero llega: {tools}"
    );
    let shown = slice_between(CSS, ".diagram.clone:hover .diagram-clone-tools,", "}");
    assert!(
        declaration(shown, "pointer-events").contains("auto"),
        "con el puntero encima las herramientas se pueden usar: {shown}"
    );
    let button = slice_between(CSS, ".panel-item-clone {", "}");
    assert!(
        declaration(button, "cursor").contains("pointer"),
        "el botón de clonar es un botón: {button}"
    );
    let disabled = slice_between(CSS, ".clone-refresh:disabled {", "}");
    assert!(
        !declaration(disabled, "opacity").is_empty(),
        "sin original el refresco se lee deshabilitado (B2): {disabled}"
    );

    // The HTML carries the empty state of the user plan.
    assert!(
        HTML.contains("id=\"empty-user\""),
        "el plano del usuario tiene su propio estado vacío"
    );
}
