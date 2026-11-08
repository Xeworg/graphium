//! The plan of the human (`odd/tasks/f8-planos.md`, B1/B2/B3; S2/S3/S4; V2).
//!
//! The clone of the user plan is a **view instance** that lives in the owner:
//! it has no `diagram_id`, it never enters the store, it does not take one of
//! the twelve slots and no MCP tool can see it, refresh it, close it or
//! update it. What the owner keeps is the diagram it came from, the version it
//! froze (S3), and the place the human left it in on their own plan (B3).
//!
//! What is pinned here is the model Rust owns; the contract the page keeps
//! with it is pinned in `user_plan_page.rs`.

use serde_json::{json, Value};

use gx_app::app::PlanState;
use gx_core::geom::{Point, Rect, Size};

/// The view the owner places against: the copy of the camera is the default
/// one (no translation, zoom 1), so the center of the view is the center of
/// the viewport.
const VIEWPORT: Size = Size::new(1280.0, 800.0);
/// The tolerance the placement keeps around an object (`camera::OBJECT_PADDING`).
const PADDING: f64 = 24.0;

const DOC_V1: &str = r#"{
  "version": 1,
  "kind": "graph",
  "title": "parse_config() — flujo",
  "direction": "TB",
  "nodes": [
    { "id": "read", "label": "read file" },
    { "id": "ok",   "label": "Config" }
  ],
  "edges": [
    { "from": "read", "to": "ok", "label": "utf-8" }
  ]
}"#;

const DOC_V2: &str = r#"{
  "version": 1,
  "kind": "graph",
  "title": "parse_config() — flujo",
  "direction": "TB",
  "nodes": [
    { "id": "read", "label": "read config" },
    { "id": "ok",   "label": "Config" },
    { "id": "done", "label": "ready" }
  ],
  "edges": [
    { "from": "read", "to": "ok", "label": "utf-8" },
    { "from": "ok", "to": "done", "label": "ok" }
  ]
}"#;

/// Installs the engines the owner paints with and the kinds of the registry:
/// the same door the application uses, without a window or a webview.
fn install() {
    gx_app::install_engines().expect("the engines of the application are installed");
    gx_mcp::bootstrap();
}

/// One diagram of `session` on the plan, with its document already stored.
///
/// Every test names its own session: the store is global to the process and
/// the tests of a file run in parallel, so a `clear_canvas` of one test would
/// take the diagrams of another.
fn diagram(store: &gx_mcp::store::DiagramStore, session: &str, document: &str) -> String {
    let diagram_id = store.next_id();
    store.insert(
        diagram_id.clone(),
        "parse_config() — flujo".to_string(),
        "graph".to_string(),
        session.to_string(),
        document.to_string(),
    );
    diagram_id
}

/// Whether two objects of the plan read as one (PRD §7.2: the padding of the
/// object is the tolerance of the placement).
fn overlapping(pos: Point, size: Size, other_pos: Point, other_size: Size) -> bool {
    let inflate = |pos: Point, size: Size| {
        Rect::new(
            pos.x - PADDING,
            pos.y - PADDING,
            size.w + 2.0 * PADDING,
            size.h + 2.0 * PADDING,
        )
    };
    inflate(pos, size).overlaps(&inflate(other_pos, other_size))
}

/// The center of the object of a clone, in plan units.
fn center(paint: &Value) -> (f64, f64) {
    let x = paint["pos"]["x"].as_f64().expect("the clone has a place");
    let y = paint["pos"]["y"].as_f64().expect("the clone has a place");
    let w = paint["bbox"]["w"].as_f64().expect("the clone has a size");
    let h = paint["bbox"]["h"].as_f64().expect("the clone has a size");
    (x + w / 2.0, y + h / 2.0)
}

fn wire<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).expect("the answer of the owner serializes")
}

/// B1 (S2, S4): clonar crea una **instancia de vista** en el plano del
/// usuario, con el documento del momento. El clon no tiene `diagram_id`, el
/// store no crece y el original queda intacto: ninguna tool lo ve.
#[test]
fn cloning_a_diagram_puts_a_view_instance_on_the_user_plan() {
    install();
    let store = gx_mcp::store::diagram_store();
    let session = "pi-planos-usuario-clonar";
    let origin = diagram(store, session, DOC_V1);
    let open_before = store.list_for_session(session).len();
    let summary_before = store.get(&origin).expect("el original está en el plano");

    let state = PlanState::default();
    let paint = state
        .create_clone(&origin, Some(VIEWPORT))
        .expect("el dueño clona el diagrama");
    let clone = wire(&paint);

    // El dibujo es el documento del momento, dibujado por el camino de solo
    // lectura: no hay hook, no hay versión nueva. Viaja como la escena que el
    // pintor monta (`f11-elementos.md` T3/C), no como la cadena de la
    // exportación.
    let labels = |paint: &Value| -> Vec<String> {
        paint["elements"]["nodes"]
            .as_array()
            .expect("el clon trae su dibujo")
            .iter()
            .map(|node| node["label"].as_str().unwrap_or_default().to_string())
            .collect()
    };
    assert!(
        labels(&clone).iter().any(|label| label == "read file"),
        "el clon congela el documento del momento: {clone}"
    );
    assert!(
        clone.get("svg").is_none(),
        "la cadena de la exportación no viaja al clon (S1/S2): {clone}"
    );
    assert_eq!(clone["origin_diagram_id"], origin.as_str());
    assert_eq!(clone["origin_session_id"], session);
    assert_eq!(clone["title"], "parse_config() — flujo");
    assert_eq!(
        clone["snapshot_version"], 1,
        "el clon guarda la versión que copió (S3): {clone}"
    );
    assert_eq!(clone["current_version"], 1);
    assert_eq!(clone["oldest_version"], 1);
    assert_eq!(clone["version_count"], 1);
    assert_eq!(
        clone["updatable"],
        Value::Bool(true),
        "el original sigue en el plano: el refresco está disponible (B2)"
    );

    // El primer clon cae en el centro de la vista (S1, S4).
    let (x, y) = center(&clone);
    assert!((x - VIEWPORT.w / 2.0).abs() < 1e-9, "centro x: {x}");
    assert!((y - VIEWPORT.h / 2.0).abs() < 1e-9, "centro y: {y}");

    // V2: las tools MCP no ven el clon. No ocupa un slot, no tiene
    // `diagram_id` y el original conserva cada dato que tenía.
    let clone_id = clone["clone_id"].as_str().expect("el clon tiene su id");
    assert!(
        clone_id.starts_with("c-"),
        "el id del clon no puede confundirse con un `diagram_id`: {clone_id}"
    );
    assert_eq!(
        store.list_for_session(session).len(),
        open_before,
        "clonar no escribe en el store: no consume uno de los 12 slots (S4)"
    );
    assert!(store.get(clone_id).is_none(), "el clon no es un diagrama");
    assert!(
        store.record(clone_id).is_none(),
        "el clon no tiene documento"
    );
    assert_eq!(
        store.get(&origin).expect("el original sigue"),
        summary_before,
        "clonar no toca el original"
    );

    let listed = gx_mcp::dispatch::call_tool("list_diagrams", &json!({}), session)
        .expect("la tool contesta");
    let ids: Vec<&str> = listed["diagrams"]
        .as_array()
        .expect("la lista de diagramas")
        .iter()
        .filter_map(|entry| entry["diagram_id"].as_str())
        .collect();
    assert!(
        ids.contains(&origin.as_str()) && !ids.contains(&clone_id),
        "la lista del agente trae sus diagramas y ningún clon: {ids:?}"
    );

    let answer =
        gx_mcp::dispatch::call_tool("get_diagram", &json!({ "diagram_id": clone_id }), session)
            .expect("la tool contesta");
    assert_eq!(
        answer["errors"][0]["code"], "unknown_diagram",
        "el agente no puede leer un clon: {answer}"
    );
    let answer = gx_mcp::dispatch::call_tool(
        "update_diagram",
        &json!({ "diagram_id": clone_id, "patch": { "title": "otro" } }),
        session,
    )
    .expect("la tool contesta");
    assert_eq!(
        answer["errors"][0]["code"], "unknown_diagram",
        "el agente no puede actualizar un clon (S4): {answer}"
    );
}

/// B1 (S4): el plano del usuario tiene su propio `occupied`. Un clon solo
/// esquiva a los clones, nunca a los diagramas de las sesiones — esos están
/// en otro plano — y el segundo clon se cascada respecto del primero.
#[test]
fn a_clone_only_avoids_the_clones_of_the_user_plan() {
    install();
    let store = gx_mcp::store::diagram_store();
    let session = "pi-planos-usuario-occupied";
    let first = diagram(store, session, DOC_V1);
    let second = diagram(store, session, DOC_V1);

    // The agent's diagram sits on the center of the view of its own plan:
    // it is not an obstacle for the user plan.
    store.place(
        &first,
        Point::new(VIEWPORT.w / 2.0 - 100.0, VIEWPORT.h / 2.0 - 60.0),
        Size::new(320.0, 200.0),
    );

    let state = PlanState::default();
    let one = wire(
        &state
            .create_clone(&first, Some(VIEWPORT))
            .expect("el primer clon"),
    );
    let (x, y) = center(&one);
    assert!(
        (x - VIEWPORT.w / 2.0).abs() < 1e-9 && (y - VIEWPORT.h / 2.0).abs() < 1e-9,
        "el diagrama de la sesión está en otro plano y no empuja al clon: {one}"
    );

    let two = wire(
        &state
            .create_clone(&second, Some(VIEWPORT))
            .expect("el segundo clon"),
    );
    let one_pos = Point::new(
        one["pos"]["x"].as_f64().expect("lugar"),
        one["pos"]["y"].as_f64().expect("lugar"),
    );
    let one_size = Size::new(
        one["bbox"]["w"].as_f64().expect("tamaño"),
        one["bbox"]["h"].as_f64().expect("tamaño"),
    );
    let two_pos = Point::new(
        two["pos"]["x"].as_f64().expect("lugar"),
        two["pos"]["y"].as_f64().expect("lugar"),
    );
    let two_size = Size::new(
        two["bbox"]["w"].as_f64().expect("tamaño"),
        two["bbox"]["h"].as_f64().expect("tamaño"),
    );
    assert_ne!(one["clone_id"], two["clone_id"], "cada clon tiene su id");
    assert!(
        !overlapping(one_pos, one_size, two_pos, two_size),
        "el segundo clon se cascada: {one_pos:?} y {two_pos:?}"
    );
    assert_eq!(
        state.clones().len(),
        2,
        "el plano del usuario guarda los dos clones"
    );
}

/// S3 (B2): el clon **no sigue** al original. El refresco es del humano y
/// trae la versión actual; el lugar que el humano le dio se conserva cuando
/// el clon todavía entra ahí.
#[test]
fn refreshing_a_clone_pulls_the_current_version_and_keeps_its_place() {
    install();
    let store = gx_mcp::store::diagram_store();
    let origin = diagram(store, "pi-planos-usuario-refresco", DOC_V1);

    let state = PlanState::default();
    let created = state
        .create_clone(&origin, Some(VIEWPORT))
        .expect("el clon nace con la v1");
    let clone_id = created.entry.clone_id.clone();
    let place = Point::new(120.0, 90.0);
    assert!(
        state.move_clone(&clone_id, place),
        "el humano puede mover el clon (B3)"
    );

    // The agent updates the original as many times as it wants.
    store.replace(
        &origin,
        "parse_config() — flujo".to_string(),
        DOC_V2.to_string(),
    );
    assert_eq!(
        state.clones()[0].snapshot_version,
        1,
        "el clon no es espejo: sin refresco sigue en la versión que copió (S3)"
    );

    let refreshed = wire(
        &state
            .view_clone(&clone_id, None)
            .expect("el refresco dibuja la versión actual"),
    );
    assert_eq!(
        refreshed["version"], 2,
        "el refresco trae la actual: {refreshed}"
    );
    assert_eq!(
        refreshed["bring"],
        Value::Bool(false),
        "una versión del historial no mueve la cámara: el humano ya está mirando el clon (B2): {refreshed}"
    );
    assert_eq!(refreshed["snapshot_version"], 2);
    assert_eq!(refreshed["current_version"], 2);
    assert_eq!(refreshed["version_count"], 2);
    let labels: Vec<&str> = refreshed["elements"]["nodes"]
        .as_array()
        .expect("el dibujo")
        .iter()
        .map(|node| node["label"].as_str().unwrap_or_default())
        .collect();
    assert!(
        labels.contains(&"read config"),
        "el dibujo es la v2: {labels:?}"
    );
    assert!(
        !labels.contains(&"read file"),
        "la v1 no queda bajo la v2: {labels:?}"
    );
    assert_eq!(
        refreshed["pos"]["x"], place.x,
        "el lugar del humano se queda cuando el clon entra: {refreshed}"
    );
    assert_eq!(refreshed["pos"]["y"], place.y);

    // Browsing an older version than the snapshot is a preview in place: the
    // object does not move (the recalculation is for newer versions, S4).
    let older = wire(
        &state
            .view_clone(&clone_id, Some(1))
            .expect("la v1 todavía se puede mirar"),
    );
    assert_eq!(older["version"], 1);
    assert_eq!(
        older["snapshot_version"], 2,
        "mirar atrás no baja la foto del clon: {older}"
    );
    assert_eq!(
        older["pos"]["x"], place.x,
        "la vista vieja no mueve el objeto"
    );
    assert_eq!(older["pos"]["y"], place.y);
    assert_eq!(
        older["bbox"], refreshed["bbox"],
        "el rectángulo del clon es el de su foto, no el de la vista prestada"
    );
}

/// S4 (B3): mirar una versión **más nueva** que el snapshot recalcula la
/// posición dentro del plano del usuario, con la cascada de los clones: el
/// objeto creció y no puede tapar a otro clon. Mirar atrás no mueve nada.
#[test]
fn a_newer_version_recalculates_the_place_inside_the_user_plan() {
    install();
    let store = gx_mcp::store::diagram_store();
    let session = "pi-planos-usuario-version";
    let grown = diagram(store, session, DOC_V1);
    let neighbour = diagram(store, session, DOC_V1);

    let state = PlanState::default();
    let first = state
        .create_clone(&grown, Some(VIEWPORT))
        .expect("el primer clon");
    let second = state
        .create_clone(&neighbour, Some(VIEWPORT))
        .expect("el segundo clon");
    // The human drags the first clone right on top of the second one: from
    // there on, growing in place would cover a neighbour.
    let stacked = Point::new(second.entry.pos.x, second.entry.pos.y);
    assert!(state.move_clone(&first.entry.clone_id, stacked));

    store.replace(
        &grown,
        "parse_config() — flujo".to_string(),
        DOC_V2.to_string(),
    );
    let refreshed = wire(
        &state
            .view_clone(&first.entry.clone_id, None)
            .expect("la versión nueva se dibuja"),
    );
    let moved = Point::new(
        refreshed["pos"]["x"].as_f64().expect("lugar"),
        refreshed["pos"]["y"].as_f64().expect("lugar"),
    );
    assert_ne!(
        moved, stacked,
        "la versión nueva reacomoda el clon en el plano del usuario (S4): {refreshed}"
    );
    let size = Size::new(
        refreshed["bbox"]["w"].as_f64().expect("tamaño"),
        refreshed["bbox"]["h"].as_f64().expect("tamaño"),
    );
    assert!(
        !overlapping(moved, size, second.entry.pos, second.entry.bbox),
        "el clon que creció no tapa a su vecino: {moved:?}"
    );
    assert_eq!(
        state.clones()[1].pos,
        second.entry.pos,
        "el vecino del plano del usuario no se mueve"
    );

    // Looking back at the version the clone had frozen is a preview: nothing
    // moves, and the snapshot stays where it was.
    let back = wire(
        &state
            .view_clone(&first.entry.clone_id, Some(1))
            .expect("la v1 se puede mirar"),
    );
    assert_eq!(back["pos"]["x"], refreshed["pos"]["x"]);
    assert_eq!(back["pos"]["y"], refreshed["pos"]["y"]);
    assert_eq!(back["snapshot_version"], 2);
}

/// S4 (B2): si el original se cerró, el clon queda como foto. El refresco se
/// deshabilita, el clon conserva su lugar y el dueño contesta el error que la
/// página muestra en el banner en vez de inventar un dibujo.
#[test]
fn a_clone_survives_the_close_of_its_original_as_a_photo() {
    install();
    let store = gx_mcp::store::diagram_store();
    let session = "pi-planos-usuario-foto";
    let origin = diagram(store, session, DOC_V1);

    let state = PlanState::default();
    let created = state
        .create_clone(&origin, Some(VIEWPORT))
        .expect("el clon nace con la v1");
    let clone_id = created.entry.clone_id.clone();
    let place = Point::new(-260.0, 40.0);
    assert!(state.move_clone(&clone_id, place));

    assert!(store.close(&origin), "el humano cierra el original");

    let clones = state.clones();
    assert_eq!(clones.len(), 1, "el clon no se va con el original");
    assert!(!clones[0].updatable, "el refresco se deshabilita (S4)");
    assert_eq!(clones[0].pos, place, "el clon conserva su lugar");
    assert_eq!(
        (
            clones[0].oldest_version,
            clones[0].current_version,
            clones[0].version_count
        ),
        (1, 1, 1),
        "las versiones que el clon ya había leído quedan como historia de la foto"
    );

    let error = state
        .view_clone(&clone_id, None)
        .expect_err("sin original no hay versión actual que traer");
    assert_eq!(error.code, "unknown_diagram");
    assert_eq!(error.path, "diagram_id");

    // V2: lo que el agente borra de su plano no se lleva el plano del
    // humano, y lo que queda en el plano del humano no lo ve ninguna tool.
    let cleared =
        gx_mcp::dispatch::call_tool("clear_canvas", &json!({}), session).expect("la tool contesta");
    assert!(cleared["ok"].as_bool().unwrap_or(false), "{cleared}");
    assert_eq!(
        store.list_for_session(session).len(),
        0,
        "el agente vació su plano"
    );
    assert_eq!(
        state.clones().len(),
        1,
        "el plano del usuario es independiente del proyecto (S2)"
    );
}

/// V2 (B3): mover con el puntero escribe **solo** la posición de la vista. El
/// original no se mueve, no se escribe una versión, el tamaño no cambia y un
/// clon que no existe no se mueve.
#[test]
fn moving_a_clone_writes_only_the_place_of_the_view() {
    install();
    let store = gx_mcp::store::diagram_store();
    let origin = diagram(store, "pi-planos-usuario-mover", DOC_V1);

    let state = PlanState::default();
    let created = state
        .create_clone(&origin, Some(VIEWPORT))
        .expect("el clon nace con la v1");
    let clone_id = created.entry.clone_id.clone();
    let before = store.get(&origin).expect("el original está en el plano");
    let versions_before = store.versions(&origin).expect("el historial");

    let place = Point::new(-40.0, 260.0);
    assert!(state.move_clone(&clone_id, place), "el clon se mueve");

    let clones = state.clones();
    assert_eq!(clones[0].pos, place);
    assert_eq!(
        clones[0].bbox, created.entry.bbox,
        "mover no cambia el tamaño del objeto"
    );
    assert_eq!(
        store.get(&origin).expect("el original sigue"),
        before,
        "el diagrama del agente no se mueve con el clon"
    );
    assert_eq!(
        store.versions(&origin).expect("el historial"),
        versions_before,
        "mover un clon no escribe una versión"
    );
    assert_eq!(
        clones[0].snapshot_version, 1,
        "mover no es refrescar: la foto no cambia"
    );

    assert!(
        !state.move_clone("c-9999", Point::new(0.0, 0.0)),
        "un clon que no existe no se mueve"
    );
}

/// S4: el plano del usuario muere con el proceso y no tiene tope propio: los
/// doce slots son del store, y los clones no los gastan. Tres diagramas
/// clonados no llenan el plano del agente (y son tres clones, porque un
/// original tiene uno solo: L6).
#[test]
fn the_user_plan_does_not_spend_the_slots_of_the_store() {
    install();
    let store = gx_mcp::store::diagram_store();
    let session = "pi-planos-usuario-slots";
    let state = PlanState::default();
    for _ in 0..3 {
        let origin = diagram(store, session, DOC_V1);
        state
            .create_clone(&origin, Some(VIEWPORT))
            .expect("el plano del usuario no tiene tope");
    }
    assert_eq!(state.clones().len(), 3);
    assert_eq!(
        store.list_for_session(session).len(),
        3,
        "tres clones no agregan nada al store: quedan sus tres diagramas"
    );
}

/// L6 (defecto 1, verbatim: «al precionar el boton de clonar se clona varias
/// veces en mi plano»): un diagrama tiene **un** clon. Volver a clonar no
/// apila otra instancia: el dueño contesta el clon que ya existe, con el lugar
/// que el humano le dejó, y pide a la página que lo traiga a la pantalla.
#[test]
fn cloning_the_same_diagram_again_answers_the_clone_that_already_exists() {
    install();
    let store = gx_mcp::store::diagram_store();
    let session = "pi-planos-usuario-dedupe";
    let origin = diagram(store, session, DOC_V1);

    let state = PlanState::default();
    let first = wire(
        &state
            .create_clone(&origin, Some(VIEWPORT))
            .expect("el primer clon"),
    );
    let place = Point::new(300.0, 120.0);
    let clone_id = first["clone_id"].as_str().expect("el clon tiene id");
    assert!(
        state.move_clone(clone_id, place),
        "el humano mueve el clon de su plano (B3)"
    );

    let again = wire(
        &state
            .create_clone(&origin, Some(VIEWPORT))
            .expect("volver a clonar contesta el clon que ya está"),
    );
    assert_eq!(
        again["clone_id"], first["clone_id"],
        "el mismo diagrama no puede tener dos clones (L6): {again}"
    );
    assert_eq!(
        again["pos"]["x"], place.x,
        "el clon que se trae es el que el humano movió: {again}"
    );
    assert_eq!(again["pos"]["y"], place.y);
    assert_eq!(
        again["bring"],
        Value::Bool(true),
        "la página trae a la pantalla el clon que ya existía: {again}"
    );
    assert!(
        again.get("elements").is_none(),
        "no hay nada nuevo que dibujar: la página conserva el dibujo que tiene: {again}"
    );
    assert_eq!(
        state.clones().len(),
        1,
        "un solo clon por original, por más clics que haya (L6)"
    );
    assert_eq!(
        store.list_for_session(session).len(),
        1,
        "volver a clonar tampoco escribe el store"
    );

    // The original closes: asking for the clone again still answers the photo
    // instead of the error of a diagram that is not on the plan (S4).
    assert!(store.close(&origin), "el humano cierra el original");
    let photo = wire(
        &state
            .create_clone(&origin, Some(VIEWPORT))
            .expect("la foto contesta, aunque el original ya no esté"),
    );
    assert_eq!(photo["clone_id"], first["clone_id"]);
    assert_eq!(
        photo["updatable"],
        Value::Bool(false),
        "la foto no se refresca (S4): {photo}"
    );
    assert!(photo.get("elements").is_none(), "{photo}");
    assert_eq!(state.clones().len(), 1, "la foto tampoco se duplica");
}

/// L6 (defecto 2, hueco de S4: «en si no veo una forma de borrar los planos
/// de mi plano»): el humano borra un clon de su plano. El dueño suelta esa
/// instancia y nada más: el otro clon conserva su lugar, el original sigue en
/// su sesión con su historial, y borrar un id que no está no hace nada.
#[test]
fn removing_a_clone_drops_only_that_view_instance() {
    install();
    let store = gx_mcp::store::diagram_store();
    let session = "pi-planos-usuario-borrar";
    let one = diagram(store, session, DOC_V1);
    let two = diagram(store, session, DOC_V2);

    let state = PlanState::default();
    let first = state
        .create_clone(&one, Some(VIEWPORT))
        .expect("el primer clon");
    let second = state
        .create_clone(&two, Some(VIEWPORT))
        .expect("el segundo clon");
    let removed = first.entry.clone_id.clone();

    assert!(state.remove_clone(&removed), "el humano borra su clon");

    let clones = state.clones();
    assert_eq!(clones.len(), 1, "queda el otro clon");
    assert_eq!(
        clones[0].clone_id, second.entry.clone_id,
        "el clon que no se borró es el que queda"
    );
    assert_eq!(
        clones[0].pos, second.entry.pos,
        "borrar un clon no mueve a los demás"
    );
    assert_eq!(
        clones[0].snapshot_version, second.entry.snapshot_version,
        "ni los refresca ni les cambia la foto"
    );
    assert_eq!(
        store.list_for_session(session).len(),
        2,
        "borrar un clon no borra los diagramas de la sesión"
    );
    assert!(
        store.get(&one).is_some(),
        "el original sigue en su plano aunque su clon ya no esté"
    );

    // The same click twice, or an id the plan never held: a no-op, never an
    // error the human would read in the banner.
    assert!(
        !state.remove_clone(&removed),
        "borrar dos veces el mismo clon no hace nada"
    );
    assert!(!state.remove_clone("c-9999"), "un clon que no está tampoco");
    assert!(!state.move_clone(&removed, Point::new(0.0, 0.0)));
    let error = state
        .view_clone(&removed, None)
        .expect_err("un clon borrado no se puede refrescar");
    assert_eq!(error.code, "unknown_diagram");
    assert_eq!(
        state.clones().len(),
        1,
        "el plano del humano quedó como el humano lo dejó"
    );

    // The diagram can come back: the rule is one clone per original, not a
    // tombstone that forbids cloning it again (L6).
    let again = state
        .create_clone(&one, Some(VIEWPORT))
        .expect("el diagrama se puede volver a clonar");
    assert_ne!(
        again.entry.clone_id, removed,
        "el clon nuevo es otra instancia: el id viejo no vuelve"
    );
    assert_eq!(state.clones().len(), 2, "ahora están los dos clones");
}
