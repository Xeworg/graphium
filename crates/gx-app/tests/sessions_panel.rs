//! The switcher of plans (S1; `odd/tasks/f8-planos.md`, A2, V1).
//!
//! No test can open the webview, so what is pinned here is the contract the
//! page keeps with Rust: the plan shows **one session at a time** and the
//! «Todas» row is gone, the switcher is built only from `graphium://state`, a
//! render brings the plan of its own session on screen, and every plan keeps
//! its own camera in the page, so coming back to a plan restores the view the
//! human left in it.

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

/// How many times `needle` appears in `haystack`.
fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

/// S1 (verbatim): «podemos sacrificar ese ver todos». El conmutador muestra
/// **un plano a la vez** y la fila fija «Todas» ya no existe: una fila por
/// sesión y nada más.
#[test]
fn the_switcher_shows_one_plan_at_a_time_and_the_todas_row_is_gone() {
    for gone in [
        "allSessionsRow",
        "showsEverySession",
        "show-session-badges",
        "family-all",
    ] {
        assert!(
            !JS.contains(gone),
            "el conmutador no tiene «Todas»: sobra `{gone}`"
        );
    }
    assert!(
        !JS.contains("todas las sesiones") && !JS.contains("\"Todas\""),
        "«Todas» ya no se pinta: ni la etiqueta ni su fila (A2)"
    );
    assert!(
        !HTML.contains("Todas") && !CSS.contains("Todas"),
        "«Todas» ya no existe en la página (A2)"
    );

    // El conmutador se construye solo desde el estado: una fila por sesión.
    let rows = slice_between(JS, "function renderSessions() {", "\n}");
    assert!(
        rows.contains("state.sessions.map(sessionRow)"),
        "las filas son las sesiones que lista Rust: {rows}"
    );
    assert_eq!(
        count(rows, "replaceChildren"),
        1,
        "la lista del conmutador se llena en un solo lugar: {rows}"
    );

    // Un plano a la vez: solo los objetos del plano en pantalla siguen
    // visibles; los de los otros planos salen de la pintura sin cerrarse. La
    // clave es el plano del objeto, que para un diagrama es su sesión y para
    // un clon es el plano del usuario (`f8-planos.md`, B1).
    let visible = slice_between(JS, "function isVisiblePlan(planId) {", "\n}");
    assert!(
        visible.contains("activePlan !== null") && visible.contains("activePlan.id"),
        "un plano a la vez: el plano en pantalla decide qué se ve: {visible}"
    );
    let apply = slice_between(JS, "function applyPlanVisibility() {", "\n}");
    assert!(
        apply.contains("element.classList.toggle(\"plan-hidden\", !isVisiblePlan("),
        "los objetos de los otros planos salen de la pintura: {apply}"
    );
    assert!(
        !apply.contains("emitClose") && !apply.contains("graphium://close"),
        "cambiar de plano no cierra nada: {apply}"
    );

    // The rule that takes them out of the paint paints with the tokens of the
    // theme, like every other piece of chrome of the plan (RF22, S18).
    let css = slice_between(CSS, ".diagram.plan-hidden {", "}");
    assert_eq!(
        declaration(css, "display"),
        "none",
        "el plano que no está en pantalla no se pinta: {css}"
    );
}

/// El conmutador marca el plano en pantalla para el ojo y para un lector, y
/// elegir una fila trae ese plano; la sesión que ya está no se re-elige.
#[test]
fn the_switcher_marks_the_plan_on_screen() {
    let row = slice_between(JS, "function sessionRow(session) {", "\n}");
    assert!(
        row.contains("isVisiblePlan(session.id)"),
        "la fila del plano en pantalla es la marcada: {row}"
    );
    assert!(
        row.contains("\"aria-pressed\""),
        "el conmutador dice qué plano está en pantalla para un lector: {row}"
    );
    assert!(
        row.contains("showPlan("),
        "elegir una fila trae su plano: {row}"
    );

    let show = slice_between(JS, "function showPlan(plan) {", "\n}");
    assert!(
        show.contains("activePlan !== null && activePlan.id === plan.id"),
        "el plano que ya está en pantalla no se vuelve a elegir: {show}"
    );
    assert!(
        show.contains("renderSessions()"),
        "el conmutador repinta la marca del plano en pantalla: {show}"
    );
    assert!(
        !show.contains("emitClose") && !show.contains("graphium://close"),
        "cambiar de plano nunca cierra un diagrama (D5): {show}"
    );
}

/// S1 y A2: la cámara es **por plano**. Cada plano guarda su cámara en la
/// página; al cambiar de plano se guarda la del que se deja y vuelve la del
/// que entra, así el humano no pierde la vista de ningún plano.
#[test]
fn the_camera_of_every_plan_is_kept_in_the_page() {
    assert!(
        JS.contains("const planCameras = new Map();"),
        "cada plano guarda su cámara en la página (A2)"
    );

    let show = slice_between(JS, "function showPlan(plan) {", "\n}");
    let saved = show
        .find("planCameras.set(")
        .expect("the plan being left keeps its camera");
    let switched = show
        .find("activePlan = plan;")
        .expect("the plan on screen changes");
    let restored = show
        .find("restorePlanCamera()")
        .expect("the plan being entered brings its camera back");
    assert!(
        saved < switched,
        "la cámara del plano que se deja se guarda antes de cambiar: {show}"
    );
    assert!(
        switched < restored,
        "la cámara del plano que entra vuelve después del cambio: {show}"
    );
    assert!(
        show.contains("camera.tx") && show.contains("camera.ty") && show.contains("camera.zoom"),
        "lo que se guarda es la cámara entera: {show}"
    );
    assert!(
        show.contains("applyPlanVisibility()"),
        "el cambio de plano también decide qué objetos se ven: {show}"
    );
    assert!(
        show.contains("cancelPan()"),
        "un pan en vuelo no puede pisar la cámara del plano que entra: {show}"
    );

    let restore = slice_between(JS, "function restorePlanCamera() {", "\n}");
    assert!(
        restore.contains("planCameras.get(activePlan.id)"),
        "la cámara que vuelve es la del plano en pantalla: {restore}"
    );
    assert!(
        restore.contains("camera.tx = saved.tx")
            && restore.contains("camera.ty = saved.ty")
            && restore.contains("clampZoom(saved.zoom)"),
        "la vista guardada se aplica tal cual, con el zoom que el humano dejó: {restore}"
    );
    assert!(
        restore.contains("applyCamera()"),
        "la cámara restaurada se pinta y decide el culling (RNF1-ter): {restore}"
    );
    assert!(
        restore.contains("notifyCamera()"),
        "Rust sigue solo la cámara del plano en pantalla: {restore}"
    );

    // Un plano que nadie visitó no tiene cámara guardada: la vista va a su
    // diagrama más nuevo, nunca a un pedazo vacío de la superficie.
    assert!(
        restore.contains("newestCenter(activePlan.id)"),
        "un plano sin cámara guardada se encuadra en su diagrama más nuevo: {restore}"
    );
    let newest = slice_between(JS, "function newestCenter(planId) {", "\n}");
    assert!(
        newest.contains("diagram.session_id === planId"),
        "el diagrama más nuevo es uno del plano: {newest}"
    );
    assert!(
        newest.contains("objectCenter("),
        "la vista se centra en el objeto que Rust colocó: {newest}"
    );
}

/// A1/A2: un render nombra su plano — la sesión viene de Rust — y ese plano
/// entra en pantalla, así el humano ve lo que el agente acaba de dibujar. El
/// objeto guarda la misma clave que usan el conmutador y el culling.
#[test]
fn a_render_brings_the_plan_of_its_own_session_on_screen() {
    let insert = slice_between(JS, "function insertDiagram(payload) {", "\n}");
    assert!(
        insert.contains("payload.session_id"),
        "el render dice de qué sesión es (Rust lo sabe): {insert}"
    );
    assert!(
        insert.contains("showPlan(planOf("),
        "el plano de la sesión del render entra en pantalla: {insert}"
    );
    assert!(
        insert.contains("container.planId = "),
        "el objeto guarda el plano que el conmutador y el culling usan: {insert}"
    );
    assert!(
        insert.contains("framesOnInsert(sessionId)"),
        "el encuadre se decide por plano, no por el plano compartido de antes: {insert}"
    );

    // El primer diagrama de **un** plano se encuadra con la cámara del
    // evento; desde el segundo de ese plano, el plano hace pan.
    let first = slice_between(JS, "function framesOnInsert(planId) {", "\n}");
    assert!(
        first.contains("objects.values()") && first.contains("element.planId"),
        "el encuadre cuenta los objetos del plano del render: {first}"
    );
    assert!(
        first.contains("=== planId"),
        "los objetos del plano se reconocen por su sesión: {first}"
    );
}

/// D4/S2: la sesión del plano en pantalla termina y el dueño se la lleva con
/// sus diagramas. El conmutador lo dice y muestra otro plano, en vez de dejar
/// una superficie vacía sin explicación.
#[test]
fn a_finished_session_leaves_another_plan_on_screen_with_a_notice() {
    let reconcile = slice_between(JS, "function reconcileActivePlan() {", "\n}");
    assert!(
        reconcile.contains("state.sessions.some("),
        "el plano en pantalla sigue vivo mientras su sesión esté listada: {reconcile}"
    );
    assert!(
        reconcile.contains("showPlan("),
        "cuando su sesión termina, el conmutador muestra otro plano: {reconcile}"
    );
    assert!(
        reconcile.contains("muestro el plano de"),
        "el aviso dice qué plano se muestra ahora: {reconcile}"
    );
    assert!(
        reconcile.contains("no queda ningún plano"),
        "sin sesiones, el aviso lo dice en vez de morir en «Todas»: {reconcile}"
    );

    let rows = slice_between(JS, "function renderSessions() {", "\n}");
    assert!(
        rows.contains("sessionNotice.textContent = sessionMessage"),
        "el aviso vive en el conmutador, como antes: {rows}"
    );
    assert!(
        rows.contains("sessionNotice.classList.toggle(\"hidden\""),
        "sin aviso no se pinta la caja del aviso: {rows}"
    );
}

/// El estado vacío es del plano en pantalla: un plano sin diagramas no puede
/// quedar en blanco porque otra sesión sí dibujó.
#[test]
fn the_empty_surface_belongs_to_the_plan_on_screen() {
    let handler = slice_between(JS, "listen(\"graphium://state\", (event) => {", "\n});");
    assert!(
        handler.contains("paintEmpty()"),
        "el estado vacío cuenta los objetos del plano en pantalla: {handler}"
    );
    let count_of_plan = slice_between(JS, "function planDiagramCount() {", "\n}");
    assert!(
        count_of_plan.contains("sessionDiagramCount(activePlan.id)"),
        "los diagramas del plano son los de su sesión: {count_of_plan}"
    );
    let paint = slice_between(JS, "function paintEmpty() {", "\n}");
    assert!(
        paint.contains("planDiagramCount() > 0"),
        "el estado vacío se decide por los objetos del plano en pantalla: {paint}"
    );
}
