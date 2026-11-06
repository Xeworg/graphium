//! The Tauri application: the plan and its window (PRD §7, §8.6.1).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Listener, Manager, WindowEvent};

use crate::camera::{self, CameraSnapshot};
use crate::settings::Settings;
use gx_core::geom::{Point, Rect, Size};

/// How long the frontend has to confirm the paint (PRD §8.6.1, step 5).
pub const RENDER_ACK_TIMEOUT_MS: u64 = 2000;
/// How long the first render waits for the webview to announce itself.
///
/// The webview says `graphium://ready` when its scripts are alive; a render
/// that arrives before that would be emitted to nobody and would end as a
/// timeout. The wait is bounded and only happens until the page is up.
pub const READY_TIMEOUT_MS: u64 = 2000;

static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(0);

/// The label of the plan window: the one the frontend, the tray and the
/// render hook look for (S3).
pub const MAIN_WINDOW: &str = "main";

/// The icon of the tray, embedded in the binary: the process lives in the tray
/// and must not depend on a file of the installation (S1).
const TRAY_ICON: &[u8] = include_bytes!("../icons/icon.png");

/// The state of the plan that Rust keeps: the camera copy, the pending
/// acknowledgements and the clones of the user plan.
#[derive(Default)]
pub struct PlanState {
    camera: Mutex<CameraSnapshot>,
    pending: Mutex<HashMap<String, Sender<bool>>>,
    ready: AtomicBool,
    settings: Mutex<Settings>,
    /// The clones of the user plan (`f8-planos.md`, B1): view instances of
    /// this owner, never diagrams of the store.
    clones: Mutex<Vec<PlanClone>>,
    /// The last number handed to a clone. Ids are never reused, like the
    /// `diagram_id` of the store (PRD §11.1).
    last_clone: AtomicU64,
}

impl PlanState {
    pub fn camera(&self) -> CameraSnapshot {
        *self.camera.lock().expect("camera lock")
    }

    pub fn set_camera(&self, camera: CameraSnapshot) {
        *self.camera.lock().expect("camera lock") = camera;
    }

    /// The settings of the plan the human chose (PRD §7.1 RF25).
    pub fn settings(&self) -> Settings {
        self.settings.lock().expect("settings lock").clone()
    }

    /// Replaces the settings: the panel changed something.
    pub fn set_settings(&self, settings: Settings) {
        *self.settings.lock().expect("settings lock") = settings;
    }

    /// Whether the frontend already announced itself (`graphium://ready`).
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Relaxed)
    }

    /// Marks the frontend as alive.
    pub fn set_ready(&self) {
        self.ready.store(true, Ordering::Relaxed);
    }

    /// Waits until the frontend announces itself, bounded by `timeout`.
    ///
    /// Returns whether the page is up. The wait only matters for the first
    /// render after the window is created or recreated.
    pub fn wait_ready(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while !self.is_ready() {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        true
    }

    /// Registers a pending render and returns the channel that the
    /// acknowledgement of the frontend will answer.
    pub fn register_ack(&self, req_id: &str) -> std::sync::mpsc::Receiver<bool> {
        let (sender, receiver) = channel();
        self.pending
            .lock()
            .expect("pending lock")
            .insert(req_id.to_string(), sender);
        receiver
    }

    /// Resolves a pending render with the answer of the frontend.
    pub fn resolve_ack(&self, req_id: &str, ok: bool) {
        if let Some(sender) = self.pending.lock().expect("pending lock").remove(req_id) {
            let _ = sender.send(ok);
        }
    }
}

// --- The plan of the human (`f8-planos.md`, B1/B2/B3; S2/S3/S4) ----------

/// One clone of the user plan.
///
/// A clone is a **view instance**: it lives here, in the owner, and never in
/// the store. It has no `diagram_id`, it does not take one of the twelve slots
/// and no MCP tool can see it, refresh it, close it or update it (S4, V2).
/// What it keeps is where it came from, the version of the source document it
/// froze (S3), and the place the human left it in on their own plan (B3).
#[derive(Debug, Clone)]
struct PlanClone {
    clone_id: String,
    origin_diagram_id: String,
    origin_session_id: String,
    title: String,
    /// The newest version of the original the clone pulled: the photo of S3.
    /// Browsing an older version never lowers it.
    snapshot_version: u64,
    /// The versions of the original the clone can browse, as the store had
    /// them the last time the clone was drawn. They are the history the photo
    /// keeps: once the original closes, the store has none and the clone
    /// still says which version it is (S4).
    oldest_version: u64,
    version_count: usize,
    /// Top-left corner of the object on the user plan, in plan units.
    pos: Point,
    /// Size of the object, padding included (PRD §7.2). It is the size of the
    /// **photo**, so a preview of an older version never reshapes the object.
    bbox: Size,
}

impl PlanClone {
    /// The clone as the state lists it (B1): the place the owner keeps and the
    /// versions the store can still draw. `updatable` is false once the
    /// original is closed: the clone stays as a photo and the refresh is
    /// disabled (S4).
    fn entry(&self, store: &gx_mcp::store::DiagramStore) -> CloneEntry {
        let live = store.versions(&self.origin_diagram_id);
        let range = live.as_ref().and_then(|versions| {
            versions
                .first()
                .map(|first| (first.version, versions.len()))
        });
        let (oldest_version, version_count) =
            range.unwrap_or((self.oldest_version, self.version_count));
        CloneEntry {
            clone_id: self.clone_id.clone(),
            origin_diagram_id: self.origin_diagram_id.clone(),
            origin_session_id: self.origin_session_id.clone(),
            title: self.title.clone(),
            snapshot_version: self.snapshot_version,
            current_version: oldest_version + version_count.saturating_sub(1) as u64,
            oldest_version,
            version_count,
            updatable: live.is_some(),
            pos: self.pos,
            bbox: self.bbox,
        }
    }
}

/// One clone of the user plan, as the state and the paint events carry it.
///
/// It is the shape the page reads to paint the object on the user plan and to
/// offer its history: the same numbers a diagram of the state carries, plus
/// the original it came from and the version it froze. The SVG does not travel
/// here: the owner draws it when the clone is created, refreshed or browsed,
/// and answers with `graphium://clone-paint`.
#[derive(Debug, Clone, Serialize)]
pub struct CloneEntry {
    pub clone_id: String,
    pub origin_diagram_id: String,
    pub origin_session_id: String,
    pub title: String,
    /// The newest version the clone pulled (S3).
    pub snapshot_version: u64,
    /// The newest version the **original** has right now; equals
    /// `snapshot_version` until someone updates the original.
    pub current_version: u64,
    /// The oldest version the history of the original keeps right now.
    pub oldest_version: u64,
    /// How many versions the history of the original keeps.
    pub version_count: usize,
    /// Whether the original is still on a plan, so the clone can pull its
    /// current version (B2).
    pub updatable: bool,
    /// Top-left corner of the object on the user plan, in plan units.
    pub pos: Point,
    /// Size of the object, padding included (PRD §7.2).
    pub bbox: Size,
}

/// One clone with the drawing the owner rendered for it: the answer to
/// `graphium://clone` and `graphium://clone-view` (B1, B2).
#[derive(Debug, Clone, Serialize)]
pub struct ClonePaint {
    #[serde(flatten)]
    pub entry: CloneEntry,
    /// The version of the original the drawing shows: the current one after a
    /// refresh, an older one while the human browses the history.
    pub version: u64,
    pub svg: String,
}

impl PlanState {
    /// The clones of the user plan, in the order they arrived.
    pub fn clones(&self) -> Vec<CloneEntry> {
        let store = gx_mcp::store::diagram_store();
        self.clones
            .lock()
            .expect("clones lock")
            .iter()
            .map(|clone| clone.entry(store))
            .collect()
    }

    /// Clones a diagram onto the user plan (B1; S2, S4).
    ///
    /// The drawing is the **current** document of the original, read through
    /// the read-only path of the history view: the newest saved version is the
    /// current document, so `render_version_view` draws it without calling the
    /// hook, without storing anything and without writing a version. The clone
    /// lands on the center of the view it arrived in — the camera the owner
    /// keeps is the one of the plan on screen —, cascading only against the
    /// clones of the user plan: the diagrams of the sessions are on other plans
    /// and never push it (S1, S4). The page then brings the user plan on screen
    /// and travels until the clone. Nothing of the store changes.
    pub fn create_clone(
        &self,
        diagram_id: &str,
        viewport: Option<Size>,
    ) -> Result<ClonePaint, gx_mcp::tools::ErrorPayload> {
        let store = gx_mcp::store::diagram_store();
        let Some(summary) = store.get(diagram_id) else {
            return Err(unknown_diagram(diagram_id, store));
        };
        let versions = store.versions(diagram_id).unwrap_or_default();
        let Some(current) = versions.last().map(|version| version.version) else {
            // A diagram on the plan always has its first version (RF14); a
            // store without one cannot be cloned.
            return Err(unknown_diagram(diagram_id, store));
        };
        let svg = gx_mcp::tools::render_version_view(diagram_id, current)?;

        // A window that cannot tell its size places the clone against a view of
        // zero: the center of that view is the corner the camera looks at, and
        // the hand of the human moves it from there (B3).
        let occupied = self.clone_rects(None);
        let rect = camera::place_with_cascade(
            svg_size(&svg),
            &self.camera(),
            viewport.unwrap_or_default(),
            &occupied,
        );

        let number = self.last_clone.fetch_add(1, Ordering::SeqCst) + 1;
        let clone = PlanClone {
            clone_id: format!("c-{number:04}"),
            origin_diagram_id: diagram_id.to_string(),
            origin_session_id: summary.session_id,
            title: summary.title,
            snapshot_version: current,
            oldest_version: versions.first().map_or(current, |version| version.version),
            version_count: versions.len(),
            pos: Point::new(rect.x, rect.y),
            bbox: Size::new(rect.w, rect.h),
        };
        let entry = clone.entry(store);
        self.clones.lock().expect("clones lock").push(clone);
        Ok(ClonePaint {
            entry,
            version: current,
            svg,
        })
    }

    /// Pulls a version of the original into a clone (B2; S3, S4).
    ///
    /// `version` is the version the human asked for in the clone's history;
    /// `None` is «Actualizar el clon», which asks for the current one. The
    /// drawing comes from the same read-only path the history of a diagram
    /// uses, so the original is never touched. A version **newer** than the
    /// snapshot is a new photo: the clone takes it as its own and its place is
    /// recalculated with more calm, cascading inside the user plan only (S1,
    /// S4). An older version is a preview in place, like the one the session
    /// plans keep (T5): nothing moves.
    pub fn view_clone(
        &self,
        clone_id: &str,
        version: Option<u64>,
    ) -> Result<ClonePaint, gx_mcp::tools::ErrorPayload> {
        let store = gx_mcp::store::diagram_store();
        let Some(found) = self
            .clones
            .lock()
            .expect("clones lock")
            .iter()
            .find(|clone| clone.clone_id == clone_id)
            .cloned()
        else {
            return Err(unknown_clone(clone_id));
        };
        let origin = found.origin_diagram_id.clone();
        let versions = store.versions(&origin).unwrap_or_default();
        let target = version.unwrap_or_else(|| {
            versions
                .last()
                .map_or(found.snapshot_version, |last| last.version)
        });
        // The read-only render also answers the error of an original that is
        // no longer on a plan: a photo cannot pull a version any more (S4).
        let svg = gx_mcp::tools::render_version_view(&origin, target)?;

        let mut updated = found;
        if target > updated.snapshot_version {
            // The photo grew: recalculate its place inside the user plan, with
            // the places of the other clones as the only obstacles.
            let size = camera::object_size(svg_size(&svg));
            let occupied = self.clone_rects(Some(clone_id));
            let rect = place_with_cascade_from(updated.pos, size, &occupied);
            updated.pos = Point::new(rect.x, rect.y);
            updated.bbox = Size::new(rect.w, rect.h);
            updated.snapshot_version = target;
            if let Some(summary) = store.get(&origin) {
                updated.title = summary.title;
            }
            if let Some(last) = versions.last() {
                updated.oldest_version =
                    versions.first().map_or(last.version, |first| first.version);
                updated.version_count = versions.len();
            }
        }
        let entry = updated.entry(store);
        if let Some(slot) = self
            .clones
            .lock()
            .expect("clones lock")
            .iter_mut()
            .find(|clone| clone.clone_id == clone_id)
        {
            *slot = updated;
        }
        Ok(ClonePaint {
            entry,
            version: target,
            svg,
        })
    }

    /// Puts a clone where the human dragged it (B3, V2).
    ///
    /// Only the place of the view changes: the store is not written, the
    /// original does not move and no version is recorded. Answers `false` for
    /// a clone that is not on the user plan.
    pub fn move_clone(&self, clone_id: &str, pos: Point) -> bool {
        let mut clones = self.clones.lock().expect("clones lock");
        let Some(clone) = clones.iter_mut().find(|clone| clone.clone_id == clone_id) else {
            return false;
        };
        clone.pos = pos;
        true
    }

    /// The rectangles the user plan already occupies: the objects of its own
    /// clones. `skip` leaves one out — the clone being recalculated is not an
    /// obstacle to itself.
    fn clone_rects(&self, skip: Option<&str>) -> Vec<Rect> {
        self.clones
            .lock()
            .expect("clones lock")
            .iter()
            .filter(|clone| Some(clone.clone_id.as_str()) != skip)
            .map(|clone| Rect::new(clone.pos.x, clone.pos.y, clone.bbox.w, clone.bbox.h))
            .collect()
    }
}

/// The error of a diagram that is not on a plan, with the wording the tools
/// own (`contract::unknown_diagram`): the human reads the same answer a tool
/// would get when the diagram they are cloning closed in the meantime.
fn unknown_diagram(
    diagram_id: &str,
    store: &gx_mcp::store::DiagramStore,
) -> gx_mcp::tools::ErrorPayload {
    gx_mcp::tools::contract_payload(
        gx_mcp::contract::unknown_diagram(diagram_id, &store.known_ids()),
        "diagram_id",
    )
}

/// The error of a clone that is not on the user plan: the page asked for a
/// clone of a window that no longer exists, and the notice the human reads is
/// the one the plan already shows.
fn unknown_clone(clone_id: &str) -> gx_mcp::tools::ErrorPayload {
    gx_mcp::tools::ErrorPayload {
        path: "clone_id".to_string(),
        code: "unknown_diagram".to_string(),
        message: format!("el clon `{clone_id}` no está en el plano del usuario"),
        hint: Some("cloná el diagrama otra vez desde el panel".to_string()),
    }
}

/// The size of a drawing, in plan units: the `width`/`height` of its root
/// `<svg>`, which the renderer always writes (PRD §7.2). A drawing without
/// them contributes no size, the way a diagram without geometry contributes no
/// rectangle.
fn svg_size(svg: &str) -> Size {
    let attribute = |name: &str| {
        let needle = format!("{name}=\"");
        svg.find(&needle).and_then(|at| {
            let rest = &svg[at + needle.len()..];
            let end = rest.find('"')?;
            rest[..end].parse::<f64>().ok()
        })
    };
    Size::new(
        attribute("width").unwrap_or_default(),
        attribute("height").unwrap_or_default(),
    )
}

/// Where a clone goes when its photo grew (S4): it keeps the place the human
/// gave it and, while that place lands on another clone of the user plan, it
/// takes the step of the cascade the plan uses. It is the «recalcular su
/// posición con más tranquilidad» of S1, and the same policy as
/// [`camera::place_with_cascade`], with the place of the clone as the origin
/// instead of the center of the view.
fn place_with_cascade_from(start: Point, size: Size, occupied: &[Rect]) -> Rect {
    let mut rect = Rect::new(start.x, start.y, size.w, size.h);
    for _ in 0..=camera::CASCADE_STEPS {
        if !lands_on_another(rect, occupied) {
            return rect;
        }
        rect = Rect::new(
            rect.x + camera::CASCADE_OFFSET,
            rect.y + camera::CASCADE_OFFSET,
            rect.w,
            rect.h,
        );
    }

    // The cascade ran out: a new column, to the right of everything of the
    // user plan (PRD §7.2).
    let right = occupied
        .iter()
        .map(|other| other.max_x())
        .fold(f64::NEG_INFINITY, f64::max);
    let top = occupied
        .iter()
        .map(|other| other.min_y())
        .fold(f64::INFINITY, f64::min);
    Rect::new(right + camera::CASCADE_OFFSET, top, rect.w, rect.h)
}

/// Whether `rect` lands within the tolerance of an occupied rectangle: the
/// tolerance is the padding of the object, so two clones closer than that read
/// as one (PRD §7.2).
fn lands_on_another(rect: Rect, occupied: &[Rect]) -> bool {
    occupied.iter().any(|other| {
        let inflated = |rect: Rect| {
            Rect::new(
                rect.x - camera::OBJECT_PADDING,
                rect.y - camera::OBJECT_PADDING,
                rect.w + 2.0 * camera::OBJECT_PADDING,
                rect.h + 2.0 * camera::OBJECT_PADDING,
            )
        };
        inflated(rect).overlaps(&inflated(*other))
    })
}

#[derive(Debug, Clone, Serialize)]
struct RenderEvent {
    req_id: String,
    diagram_id: String,
    /// The session that drew the diagram, which is the plan it belongs to
    /// (`f8-planos.md`, A1/A2). The page brings that plan on screen; the
    /// state that names the session arrives only after the acknowledgement.
    session_id: String,
    svg: String,
    /// Where the diagram goes in plan units: the center of the view it
    /// arrived in (PRD §7.2, `place`). Absent when the window size is
    /// unknown: the plan then places it at its own center.
    #[serde(skip_serializing_if = "Option::is_none")]
    placement: Option<Placement>,
    /// The camera that frames the diagram (PRD §7.2, `fit`). The camera
    /// lives in the frontend: this is an intent, not a write.
    #[serde(skip_serializing_if = "Option::is_none")]
    camera: Option<CameraSnapshot>,
    /// The box of `opts.focus` in plan units (PRD §7, RF27). When it is
    /// there the page applies the camera even if the plan is already full:
    /// the agent asked to be shown this, not to keep the view.
    #[serde(skip_serializing_if = "Option::is_none")]
    focus: Option<FocusBox>,
}

/// A box in plan units, as the frontend reads it.
#[derive(Debug, Clone, Copy, Serialize)]
struct FocusBox {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// The top-left corner of a diagram in plan units.
#[derive(Debug, Clone, Copy, Serialize)]
struct Placement {
    x: f64,
    y: f64,
}

/// The size of the plan in CSS pixels, or `None` when the window cannot tell.
fn viewport_of(window: &tauri::WebviewWindow) -> Option<Size> {
    let size = window.inner_size().ok()?;
    let logical = size.to_logical::<f64>(window.scale_factor().ok()?);
    if logical.width <= 0.0 || logical.height <= 0.0 {
        return None;
    }
    Some(Size::new(logical.width, logical.height))
}

/// One diagram of the state that Rust owns (PRD §8.6.1, S22).
#[derive(Debug, Clone, Serialize)]
struct StateEntry {
    diagram_id: String,
    title: String,
    kind: String,
    updated_at: String,
    session_id: String,
    /// Top-left corner of the object on the plan, in plan units (PRD §7.2).
    #[serde(skip_serializing_if = "Option::is_none")]
    pos: Option<gx_core::geom::Point>,
    /// Size of the object, padding included (PRD §7.2).
    #[serde(skip_serializing_if = "Option::is_none")]
    bbox: Option<gx_core::geom::Size>,
    /// Newest version of the source document the history keeps (RF14). The
    /// numbers are absolute and consecutive: the store discards the oldest
    /// version past its cap without restarting the numbering, so the range
    /// `oldest_version..=current_version` names every saved version and the
    /// panel positions any of them without asking twice.
    current_version: u64,
    oldest_version: u64,
    /// How many versions the history keeps, capped at `MAX_VERSIONS` (RF14).
    /// It is the «7» of the indicator «v3/7»; the «3» is the position of a
    /// version inside `oldest_version..=current_version`.
    version_count: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct CameraPayload {
    tx: f64,
    ty: f64,
    zoom: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct AckPayload {
    req_id: String,
    ok: bool,
}

/// The list of diagrams the plan shows, in the shape the panel reads.
///
/// One builder for every path that publishes the state: the panel is built
/// only from this event, so two shapes there would be two panels (PRD §8.6.1).
fn state_entries() -> Vec<StateEntry> {
    gx_mcp::store::diagram_store()
        .list()
        .into_iter()
        .map(|summary| {
            let versions = gx_mcp::store::diagram_store()
                .versions(&summary.diagram_id)
                .unwrap_or_default();
            StateEntry {
                diagram_id: summary.diagram_id,
                title: summary.title,
                kind: summary.kind,
                updated_at: summary.updated_at,
                session_id: summary.session_id,
                pos: summary.pos,
                bbox: summary.bbox,
                current_version: versions.last().map_or(0, |version| version.version),
                oldest_version: versions.first().map_or(0, |version| version.version),
                version_count: versions.len(),
            }
        })
        .collect()
}

/// The state of the plan the panel reads (PRD §8.6.1, S8; feature
/// `f3-sesiones`, S1/S3/D3/D5; `f8-planos.md`, A2).
///
/// Besides the diagrams, it carries **every** session of the owner, because
/// the human is the exception to the isolation of D3: the switcher lists them
/// all and brings one plan on screen at a time. Each diagram already names
/// its session (`session_id`), which is the plan it belongs to. The clones of
/// the user plan travel here too (`f8-planos.md`, B1): they are the objects of
/// the human's own surface, and the page builds them from this same event.
#[derive(Debug, Clone, Serialize)]
struct PlanPayload {
    diagrams: Vec<StateEntry>,
    /// The sessions in the deterministic order of the registry (a
    /// `BTreeMap`, never a `HashMap`): the rows never jump around.
    sessions: Vec<gx_mcp::session::SessionView>,
    /// The clones of the user plan, in the order they arrived.
    clones: Vec<CloneEntry>,
}

/// The whole state of the plan: what is drawn and who drew it.
///
/// The store is read first on purpose: reading it is the tick that ends the
/// sessions whose grace finished (D4, S2), so a session that ended is already
/// gone from the list the switcher reads instead of lingering as a row with no
/// diagrams.
fn plan_payload(state: &PlanState) -> PlanPayload {
    let diagrams = state_entries();
    let sessions = gx_mcp::session::session_registry()
        .list()
        .into_iter()
        .map(|record| record.view())
        .collect();
    // The clones are of the owner, not of the store: the state is the only
    // door they go out through (`f8-planos.md`, B1, S4).
    let clones = state.clones();
    PlanPayload {
        diagrams,
        sessions,
        clones,
    }
}

/// Builds the Tauri application with the plan window.
///
/// `keep_alive` is true in MCP mode: closing the window must not kill the
/// process while a session is alive (PRD §8.6).
///
/// `start_window` is false for the owner that lives in the tray (S3): there the
/// window is a view, and it is created on demand by the render hook or by the
/// tray. Everything else — the camera, the settings, the close of a diagram —
/// is the same in both modes, so there is one builder and not two.
///
/// The plan state is created here and shared with the render hook: the setup
/// hook of Tauri 2 only runs when the event loop starts, so a state that
/// lives in the Tauri state manager would not exist yet when the MCP thread
/// asks for it.
pub fn build(keep_alive: bool, start_window: bool) -> (tauri::App<tauri::Wry>, Arc<PlanState>) {
    let state = Arc::new(PlanState::default());

    let application = tauri::Builder::default()
        .build(tauri::generate_context!())
        .expect("the tauri application must build");

    // The window is created here, before the event loop starts: the window
    // declared in the configuration only appears when the loop is ready, and
    // by then the page may have already announced itself.
    let handle = application.handle().clone();
    if start_window {
        create_main_window(&handle, keep_alive).expect("the main window must be created");
    }

    let camera_state = Arc::clone(&state);
    let ack_state = Arc::clone(&state);
    handle.listen("camera-changed", move |event| {
        if let Ok(payload) = serde_json::from_str::<CameraPayload>(event.payload()) {
            camera_state.set_camera(CameraSnapshot::new(payload.tx, payload.ty, payload.zoom));
        }
    });
    handle.listen("render_ack", move |event| {
        if let Ok(payload) = serde_json::from_str::<AckPayload>(event.payload()) {
            ack_state.resolve_ack(&payload.req_id, payload.ok);
        }
    });
    // The settings live in `~/.graphium/settings.json` and belong to the
    // human: the window reads them at start and writes them when the panel
    // changes something (PRD §7.1 RF25, S19).
    state.set_settings(Settings::load());

    // Closing a diagram from the plan belongs to the human: the window asks,
    // the model answers (PRD §7, RF28). The id stays invalid and is never
    // handed out again, and the window survives.
    let close_handle = handle.clone();
    let close_state = Arc::clone(&state);
    handle.listen("graphium://close", move |event| {
        #[derive(Deserialize)]
        struct ClosePayload {
            diagram_id: String,
        }
        let Ok(payload) = serde_json::from_str::<ClosePayload>(event.payload()) else {
            return;
        };
        if !gx_mcp::store::diagram_store().close(&payload.diagram_id) {
            return;
        }
        let _ = close_handle.emit(
            "graphium://closed",
            serde_json::json!({ "diagram_id": payload.diagram_id }),
        );
        let _ = close_handle.emit("graphium://state", plan_payload(&close_state));
    });

    // Viewing a saved version of the history belongs to the human (RF14; T5
    // of `f2-cierre.md`): the panel asks, Rust draws the version read-only.
    // The preview renders without the hook, stores nothing and writes no
    // version — the plan only borrows the drawing the version had when it
    // was current. The answer goes back as its own event, so the page can
    // swap the drawing in place without moving the object on the plan.
    let version_handle = handle.clone();
    handle.listen("graphium://version-view", move |event| {
        #[derive(Deserialize)]
        struct VersionViewPayload {
            diagram_id: String,
            version: u64,
        }
        let Ok(payload) = serde_json::from_str::<VersionViewPayload>(event.payload()) else {
            return;
        };
        match gx_mcp::tools::render_version_view(&payload.diagram_id, payload.version) {
            Ok(svg) => {
                let _ = version_handle.emit(
                    "graphium://version-paint",
                    serde_json::json!({
                        "diagram_id": payload.diagram_id,
                        "version": payload.version,
                        "svg": svg,
                    }),
                );
            }
            Err(error) => {
                // A version the history does not keep (or a diagram that
                // closed while the human browsed it) is answered in the
                // banner the plan already shows, where the human reads it.
                let _ = version_handle.emit("graphium://error", error);
            }
        }
    });

    let ready_state = Arc::clone(&state);
    let ready_handle = handle.clone();
    handle.listen("graphium://ready", move |_event| {
        ready_state.set_ready();
        let _ = ready_handle.emit("graphium://settings", ready_state.settings());
        // The switcher of plans is built from the state, like the panel (S8):
        // the page gets one as soon as it is alive, so the rows are there
        // before the first render and never invented by the page.
        let _ = ready_handle.emit("graphium://state", plan_payload(&ready_state));
    });

    // The user plan (`f8-planos.md`, B1/B2/S2/S4): the human clones a diagram
    // of any session onto their own surface, refreshes it and browses the
    // history of the original inside it. The clone is a view instance of the
    // owner — no store write, no `diagram_id`, no slot of the twelve — so the
    // page asks and Rust answers with the drawing and the entry the state
    // lists. `graphium://clone-view` answers both the refresh (no version) and
    // a step of the history.
    let clone_handle = handle.clone();
    let clone_state = Arc::clone(&state);
    handle.listen("graphium://clone", move |event| {
        #[derive(Deserialize)]
        struct ClonePayload {
            diagram_id: String,
        }
        let Ok(payload) = serde_json::from_str::<ClonePayload>(event.payload()) else {
            return;
        };
        let viewport = clone_handle
            .get_webview_window(MAIN_WINDOW)
            .as_ref()
            .and_then(viewport_of);
        match clone_state.create_clone(&payload.diagram_id, viewport) {
            Ok(paint) => {
                // The drawing goes first: the page creates the object with it,
                // so a state that lists a clone nobody has drawn yet never
                // happens, and the clone the human asked for never flickers.
                let _ = clone_handle.emit("graphium://clone-paint", paint);
                let _ = clone_handle.emit("graphium://state", plan_payload(&clone_state));
            }
            Err(error) => {
                let _ = clone_handle.emit("graphium://error", error);
            }
        }
    });

    let view_handle = handle.clone();
    let view_state = Arc::clone(&state);
    handle.listen("graphium://clone-view", move |event| {
        #[derive(Deserialize)]
        struct CloneViewPayload {
            clone_id: String,
            version: Option<u64>,
        }
        let Ok(payload) = serde_json::from_str::<CloneViewPayload>(event.payload()) else {
            return;
        };
        match view_state.view_clone(&payload.clone_id, payload.version) {
            Ok(paint) => {
                // A newer version may have moved the clone, so the state (the
                // place the owner keeps) leaves before the drawing.
                let _ = view_handle.emit("graphium://state", plan_payload(&view_state));
                let _ = view_handle.emit("graphium://clone-paint", paint);
            }
            Err(error) => {
                let _ = view_handle.emit("graphium://error", error);
            }
        }
    });

    // Moving a clone belongs to the human and only writes where the view is
    // (B3, V2): the store, the original and its history stay untouched.
    let move_handle = handle.clone();
    let move_state = Arc::clone(&state);
    handle.listen("graphium://clone-move", move |event| {
        #[derive(Deserialize)]
        struct CloneMovePayload {
            clone_id: String,
            pos: Position,
        }
        #[derive(Deserialize)]
        struct Position {
            x: f64,
            y: f64,
        }
        let Ok(payload) = serde_json::from_str::<CloneMovePayload>(event.payload()) else {
            return;
        };
        if !payload.pos.x.is_finite() || !payload.pos.y.is_finite() {
            return;
        }
        if !move_state.move_clone(&payload.clone_id, Point::new(payload.pos.x, payload.pos.y)) {
            return;
        }
        let _ = move_handle.emit("graphium://state", plan_payload(&move_state));
    });

    let settings_state = Arc::clone(&state);
    let settings_handle = handle.clone();
    handle.listen("settings-changed", move |event| {
        let Ok(settings) = serde_json::from_str::<Settings>(event.payload()) else {
            return;
        };
        // Picking another theme makes it the base again: the colours the
        // human had picked for the previous theme are replaced by the palette
        // of the new one (`f6-themes.md`, R3). The decision lives here and not
        // in the page, which never invents a colour (S19).
        let settings = settings.sanitized();
        let settings = if settings.theme != Settings::load().theme {
            settings.with_theme_palette()
        } else {
            settings
        };
        if settings.save().is_err() {
            return;
        }
        // What the file holds is what the panel shows: the sanitised value
        // comes back, so a rejected field never leaves the UI lying.
        let settings = Settings::load();
        settings_state.set_settings(settings.clone());
        // The export follows the palette of the plan: the human changed what
        // they see, so the next export paints with it (decision L7).
        gx_core::export::install_palette(settings.palette());
        let _ = settings_handle.emit("graphium://settings", settings);
    });

    (application, state)
}

/// Creates the plan window with the close behaviour of S3.
///
/// It is one function for the three paths that can create it — the solo-ui
/// start, the tray and the first diagram — so the window never behaves in two
/// ways. With `keep_alive`, closing it hides it: `hide`+`show`, never destroy
/// and rebuild (tauri#9307), because the page must survive the next render.
pub fn create_main_window(
    handle: &tauri::AppHandle,
    keep_alive: bool,
) -> tauri::Result<tauri::WebviewWindow> {
    let window =
        tauri::WebviewWindowBuilder::new(handle, MAIN_WINDOW, tauri::WebviewUrl::default())
            .title("Graphium")
            .inner_size(1280.0, 800.0)
            .resizable(true)
            .build()?;

    // Closing the window does not kill the process while an MCP session is
    // alive: the window hides and the next render brings it back (PRD §8.6).
    let hidden = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            if keep_alive {
                api.prevent_close();
                let _ = hidden.hide();
            }
        }
    });
    Ok(window)
}

/// Shows the plan window, creating it when it does not exist yet (S3).
///
/// `show()` + `unminimize()` + `set_focus()`: the same three steps whether the
/// human clicked the tray or an agent painted a diagram, so the plan is in
/// front of whoever asked for it. A window that cannot be created answers
/// `None` instead of panicking: the caller decides what that means.
pub fn show_main_window(
    handle: &tauri::AppHandle,
    keep_alive: bool,
) -> Option<tauri::WebviewWindow> {
    let window = match handle.get_webview_window(MAIN_WINDOW) {
        Some(window) => window,
        None => create_main_window(handle, keep_alive).ok()?,
    };
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
    Some(window)
}

/// Puts Graphium in the tray (S1, S3, S8).
///
/// The process starts there, with no window: «Mostrar el plano» opens the plan
/// and «Salir» takes the graceful exit of S5 — the same path as `graphium quit`
/// and SIGTERM, so no socket is left dirty.
pub fn install_tray(handle: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let show = MenuItem::with_id(handle, "show", "Mostrar el plano", true, None::<&str>)?;
    let quit = MenuItem::with_id(handle, "quit", "Salir", true, None::<&str>)?;
    let menu = Menu::with_items(handle, &[&show, &quit])?;
    let icon = tauri::image::Image::from_bytes(TRAY_ICON)?;

    TrayIconBuilder::new()
        .icon(icon)
        .tooltip("Graphium")
        .menu(&menu)
        // A left click shows the plan where the platform reports it; on Linux
        // the indicator has no click callback, so there the menu is the door.
        .show_menu_on_left_click(cfg!(target_os = "linux"))
        .on_menu_event(|handle, event| match event.id.as_ref() {
            "show" => {
                show_main_window(handle, true);
            }
            "quit" => crate::relay::request_quit(),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle(), true);
            }
        })
        .build(handle)?;
    Ok(())
}

/// The rectangles the placement of a render has to avoid: the objects of the
/// same session that are already on its plan (PRD §7.2, RF27; `f8-planos.md`,
/// A1 and S1).
///
/// Each session draws on its own surface, so a diagram only collides with the
/// objects its own session already placed: the objects of another session are
/// on another plan and never push it. The diagram being rendered is skipped
/// as well: a re-render (`opts.replace_id`, `update_diagram`) is not an
/// obstacle to itself. A diagram without a place contributes no rectangle.
fn occupied_of_session(
    diagrams: &[gx_mcp::store::DiagramSummary],
    session_id: &str,
    diagram_id: &str,
) -> Vec<gx_core::geom::Rect> {
    diagrams
        .iter()
        .filter(|summary| summary.session_id == session_id && summary.diagram_id != diagram_id)
        .filter_map(|summary| match (summary.pos, summary.bbox) {
            (Some(pos), Some(bbox)) => Some(gx_core::geom::Rect::new(pos.x, pos.y, bbox.w, bbox.h)),
            _ => None,
        })
        .collect()
}

/// Installs the hook that paints a diagram in the window and waits for the
/// frontend to confirm it.
pub fn install_render_hook(handle: tauri::AppHandle, state: Arc<PlanState>) {
    let hook: gx_mcp::tools::RenderHook = Arc::new(move |payload| {
        // Without a display the process works headless: the diagram is
        // rendered and stored, but nothing is painted (PRD §8.6).
        if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
            return gx_mcp::tools::RenderStatus::Headless;
        }

        let req_id = format!("req-{}", REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed));
        let receiver = state.register_ack(&req_id);

        let Some(window) = show_main_window(&handle, true) else {
            return gx_mcp::tools::RenderStatus::NoWindow;
        };

        // The first render after the window is created can arrive before the
        // page is alive; emitting then would reach nobody. The wait is
        // bounded by `READY_TIMEOUT_MS` and only happens until the page has
        // announced itself (PRD §8.6.1).
        if !state.wait_ready(Duration::from_millis(READY_TIMEOUT_MS)) {
            return gx_mcp::tools::RenderStatus::Error;
        }

        // The render is emitted and the answer is awaited: the timeout of
        // the acknowledgement is the only wait (PRD §8.6.1, step 5).
        //
        // The diagram lands on the center of the view it arrives in, and the
        // view fits it: a diagram that does not fit is cut by the window,
        // and a cut diagram cannot be read (PRD §7.2, S17 and S23).
        let viewport = viewport_of(&window);
        // The objects already on the plan, so a new one never lands on top of
        // another (PRD §7.2, RF27). Each session has its own plan, so only the
        // objects of the same session are in the way (`f8-planos.md`, A1/S1):
        // the store knows the session of the render because the diagram is
        // inserted before the hook runs.
        let store = gx_mcp::store::diagram_store();
        let summary = store.get(&payload.diagram_id);
        let session_id = summary
            .as_ref()
            .map_or_else(String::new, |summary| summary.session_id.clone());
        let occupied = occupied_of_session(&store.list(), &session_id, &payload.diagram_id);
        let placement = viewport.map(|viewport| {
            camera::place_with_cascade(
                Size::new(payload.width, payload.height),
                &state.camera(),
                viewport,
                &occupied,
            )
        });
        if let Some(rect) = placement {
            store.place(
                &payload.diagram_id,
                gx_core::geom::Point::new(rect.x, rect.y),
                gx_core::geom::Size::new(rect.w, rect.h),
            );
        }
        // `opts.focus` wins over the frame of the whole diagram: the agent
        // asked to be shown one node (PRD §7, RF27). The box arrives in SVG
        // coordinates, so the object padding of the plan is added back.
        let focus_box = match (payload.focus, placement) {
            (Some(box_), Some(rect)) => Some(FocusBox {
                x: rect.x + camera::OBJECT_PADDING + box_.x,
                y: rect.y + camera::OBJECT_PADDING + box_.y,
                w: box_.w,
                h: box_.h,
            }),
            _ => None,
        };
        let fit = match (focus_box, placement, viewport) {
            (Some(box_), _, Some(viewport)) => Some(camera::fit(
                gx_core::geom::Rect::new(box_.x, box_.y, box_.w, box_.h),
                viewport,
            )),
            (None, Some(rect), Some(viewport)) => Some(camera::fit(rect, viewport)),
            _ => None,
        };
        let event = RenderEvent {
            req_id: req_id.clone(),
            diagram_id: payload.diagram_id.clone(),
            session_id,
            svg: payload.svg.clone(),
            placement: placement.map(|rect| Placement {
                x: rect.x,
                y: rect.y,
            }),
            camera: fit,
            focus: focus_box,
        };
        if window.emit("graphium://render", event).is_err() {
            return gx_mcp::tools::RenderStatus::Error;
        }

        let acknowledged = receiver
            .recv_timeout(Duration::from_millis(RENDER_ACK_TIMEOUT_MS))
            .unwrap_or(false);
        if !acknowledged {
            state.pending.lock().expect("pending lock").remove(&req_id);
            return gx_mcp::tools::RenderStatus::Timeout;
        }

        // The model of the diagrams lives in Rust (PRD §8.6.1, S22): the
        // frontend rebuilds its panel from this event.
        let _ = window.emit("graphium://state", plan_payload(&state));

        gx_mcp::tools::RenderStatus::Painted
    });
    gx_mcp::tools::set_render_hook(Some(hook));
}

#[cfg(test)]
mod tests {
    use super::{occupied_of_session, RenderEvent, StateEntry};

    /// PRD §8.6.1: el panel lateral se construye **solo** desde este evento,
    /// así que tiene que traer dónde está cada objeto y cuánto mide. Sin eso
    /// el panel no puede listar ni encuadrar.
    #[test]
    fn the_state_of_the_plan_carries_where_each_diagram_is() {
        let entry = StateEntry {
            diagram_id: "d-0001".to_string(),
            title: "schema".to_string(),
            kind: "er".to_string(),
            updated_at: "2026-11-03T12:00:00Z".to_string(),
            session_id: "stdio".to_string(),
            pos: Some(gx_core::geom::Point::new(120.0, -40.0)),
            bbox: Some(gx_core::geom::Size::new(320.0, 180.0)),
            current_version: 3,
            oldest_version: 1,
            version_count: 3,
        };

        let json = serde_json::to_value(&entry).expect("the state serializes");

        assert_eq!(json["diagram_id"], "d-0001");
        assert_eq!(json["pos"]["x"], 120.0);
        assert_eq!(json["pos"]["y"], -40.0);
        assert_eq!(json["bbox"]["w"], 320.0);
        assert_eq!(json["bbox"]["h"], 180.0);
    }

    /// Un diagrama que todavía no tiene sitio no inventa coordenadas: el
    /// campo no aparece, en vez de aparecer en cero.
    #[test]
    fn a_diagram_without_a_place_omits_the_geometry() {
        let entry = StateEntry {
            diagram_id: "d-0002".to_string(),
            title: "sin colocar".to_string(),
            kind: "graph".to_string(),
            updated_at: "2026-11-03T12:00:00Z".to_string(),
            session_id: "stdio".to_string(),
            pos: None,
            bbox: None,
            current_version: 1,
            oldest_version: 1,
            version_count: 1,
        };

        let json = serde_json::to_value(&entry).expect("the state serializes");

        assert!(json.get("pos").is_none(), "{json}");
        assert!(json.get("bbox").is_none(), "{json}");
    }

    /// RF14 (T5 de `f2-cierre.md`): el panel muestra el indicador «v3/7» y
    /// navega el historial, así que el estado tiene que traer qué versiones
    /// guarda cada diagrama: la más vieja que sobrevive, la actual y cuántas
    /// hay. Los números son absolutos y consecutivos (el store descarta la
    /// más vieja sin reiniciar la numeración), así que el rango bastan para
    /// posicionar cualquier versión y para armar el «3» de «v3/7».
    #[test]
    fn the_state_of_the_plan_carries_the_version_history_of_each_diagram() {
        let store = gx_mcp::store::diagram_store();
        let diagram_id = store.next_id();
        store.insert(
            diagram_id.clone(),
            "flujo".to_string(),
            "graph".to_string(),
            "stdio-historial".to_string(),
            "{}".to_string(),
        );
        store.replace(
            &diagram_id,
            "flujo".to_string(),
            "{\"version\":1,\"kind\":\"graph\"}".to_string(),
        );

        let payload = serde_json::to_value(super::plan_payload(&super::PlanState::default()))
            .expect("the state serializes");
        let entry = payload["diagrams"]
            .as_array()
            .expect("the state carries the diagrams")
            .iter()
            .find(|entry| entry["diagram_id"].as_str() == Some(diagram_id.as_str()))
            .expect("the diagram is on the plan");
        assert_eq!(
            entry["oldest_version"], 1,
            "el insert es la primera versión"
        );
        assert_eq!(entry["current_version"], 2, "el replace es la segunda");
        assert_eq!(entry["version_count"], 2);
    }

    /// The identity a session declares, resolved the way the transports
    /// resolve it (D1).
    fn declared_identity(
        client: &str,
        session: &str,
        project: &str,
    ) -> gx_mcp::session::ResolvedIdentity {
        gx_mcp::session::resolve(
            &gx_mcp::session::Declared {
                client: Some(client.to_string()),
                session: Some(session.to_string()),
                project: Some(project.to_string()),
            },
            &gx_mcp::session::ClientFingerprint::default(),
            &gx_mcp::session::ProcessFingerprint::default(),
        )
    }

    /// S1/S3/D3: the panel is built only from this event, so the state has to
    /// carry **every** session of the owner — the human sees all of them, one
    /// plan at a time — and say which session drew each diagram, or the
    /// switcher cannot group a row with its diagrams.
    #[test]
    fn the_state_of_the_plan_carries_every_session_and_what_it_drew() {
        let pid = std::process::id();
        let pi_id = format!("pi-panel-{pid}");
        let claude_id = format!("claude-panel-{pid}");

        // Two live sessions, each with one diagram on the plan.
        let registry = gx_mcp::session::session_registry();
        registry.connect(&pi_id, &declared_identity("pi", &pi_id, "/work/graphium"));
        registry.connect(
            &claude_id,
            &declared_identity("claude", &claude_id, "/work/api"),
        );
        let store = gx_mcp::store::diagram_store();
        let pi_diagram = store.next_id();
        store.insert(
            pi_diagram.clone(),
            "schema".to_string(),
            "er".to_string(),
            pi_id.clone(),
            "{}".to_string(),
        );
        let claude_diagram = store.next_id();
        store.insert(
            claude_diagram.clone(),
            "flujo".to_string(),
            "graph".to_string(),
            claude_id.clone(),
            "{}".to_string(),
        );

        let payload = serde_json::to_value(super::plan_payload(&super::PlanState::default()))
            .expect("the state serializes");

        let sessions = payload["sessions"]
            .as_array()
            .expect("the state carries the sessions the human can choose");
        let ids: Vec<&str> = sessions
            .iter()
            .filter_map(|session| session["id"].as_str())
            .collect();
        assert!(
            ids.windows(2).all(|pair| pair[0] < pair[1]),
            "the sessions come in a deterministic order, not in the order of a map: {ids:?}"
        );

        let pi = sessions
            .iter()
            .find(|session| session["id"].as_str() == Some(pi_id.as_str()))
            .expect("the Pi session is in the state");
        assert_eq!(pi["family"], "pi");
        assert_eq!(pi["state"], "active");
        assert_eq!(pi["project"], "/work/graphium");
        assert!(
            pi["label"]
                .as_str()
                .is_some_and(|label| label.starts_with("Pi · graphium")),
            "the row shows the label of the session: {pi}"
        );

        let claude = sessions
            .iter()
            .find(|session| session["id"].as_str() == Some(claude_id.as_str()))
            .expect("the Claude session is in the state");
        assert_eq!(claude["family"], "claude");

        // D4: a session in grace is still a row of the switcher — the human
        // sees every session (D3) — with its state spelled out for the page.
        registry.begin_grace(&claude_id, std::time::SystemTime::now());
        let in_grace = serde_json::to_value(super::plan_payload(&super::PlanState::default()))
            .expect("serializes");
        let claude = in_grace["sessions"]
            .as_array()
            .expect("the state carries the sessions")
            .iter()
            .find(|session| session["id"].as_str() == Some(claude_id.as_str()))
            .expect("a session in grace is still listed")
            .clone();
        assert_eq!(claude["state"], "in_grace");

        let diagrams = payload["diagrams"]
            .as_array()
            .expect("the state carries the diagrams of the plan");
        for (diagram_id, session_id) in [(&pi_diagram, &pi_id), (&claude_diagram, &claude_id)] {
            let entry = diagrams
                .iter()
                .find(|entry| entry["diagram_id"].as_str() == Some(diagram_id.as_str()))
                .unwrap_or_else(|| panic!("{diagram_id} is on the plan"));
            assert_eq!(
                entry["session_id"].as_str(),
                Some(session_id.as_str()),
                "a diagram says which session drew it"
            );
            assert!(
                sessions
                    .iter()
                    .any(|session| session["id"] == entry["session_id"]),
                "the session of a diagram is one the switcher lists, or the row \
                 and its diagrams would disagree: {entry}"
            );
        }
    }

    /// Un diagrama de la tienda, con la geometría que el plano le dio (o
    /// ninguna, si todavía no tiene sitio).
    fn summary_of(
        diagram_id: &str,
        session_id: &str,
        geometry: Option<(f64, f64, f64, f64)>,
    ) -> gx_mcp::store::DiagramSummary {
        gx_mcp::store::DiagramSummary {
            diagram_id: diagram_id.to_string(),
            title: diagram_id.to_string(),
            kind: "graph".to_string(),
            updated_at: "2026-11-05T12:00:00Z".to_string(),
            session_id: session_id.to_string(),
            revision: 1,
            pos: geometry.map(|(x, y, _, _)| gx_core::geom::Point::new(x, y)),
            bbox: geometry.map(|(_, _, w, h)| gx_core::geom::Size::new(w, h)),
        }
    }

    /// S1, A1: cada sesión dibuja en su plano. La colocación de un render
    /// solo puede colisionar con los objetos de **su** sesión: los de otra
    /// sesión están en otro plano y no lo empujan. El diagrama que se está
    /// dibujando tampoco es un obstáculo para sí mismo (re-render con
    /// `opts.replace_id`), y un diagrama sin sitio no aporta rectángulo.
    #[test]
    fn a_placement_only_avoids_the_objects_of_its_own_session() {
        let diagrams = vec![
            summary_of("d-0001", "pi-1", Some((10.0, 20.0, 300.0, 180.0))),
            summary_of("d-0002", "claude-1", Some((500.0, 20.0, 300.0, 180.0))),
            summary_of("d-0003", "pi-1", Some((900.0, 400.0, 120.0, 80.0))),
            summary_of("d-0004", "pi-1", None),
        ];

        let occupied = occupied_of_session(&diagrams, "pi-1", "d-0003");

        assert_eq!(
            occupied,
            vec![gx_core::geom::Rect::new(10.0, 20.0, 300.0, 180.0)],
            "solo el objeto de la propia sesión ocupa lugar en su plano"
        );
    }

    /// A2: el evento del render nombra la sesión que lo dibujó. La página
    /// necesita esa clave para poner en pantalla el plano de esa sesión: el
    /// estado que la trae llega recién después del acuse.
    #[test]
    fn the_render_names_the_plan_of_the_diagram() {
        let event = RenderEvent {
            req_id: "req-1".to_string(),
            diagram_id: "d-0001".to_string(),
            session_id: "pi-1".to_string(),
            svg: "<svg/>".to_string(),
            placement: None,
            camera: None,
            focus: None,
        };

        let json = serde_json::to_value(&event).expect("the render serializes");

        assert_eq!(json["session_id"], "pi-1");
    }

    /// Un documento chiquito para el plano del usuario: el dueño lo dibuja
    /// con los motores reales, sin ventana.
    const MINIMAL: &str = r#"{
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

    /// B1: el estado del plano es la única puerta de los clones. Trae, por
    /// cada uno, el lugar y el tamaño con los que la página arma el objeto, la
    /// versión que congeló y si el original sigue en el plano; y **no** trae
    /// el `diagram_id` del original como si fuera uno: el clon tiene su propia
    /// clave, que el store no conoce (S4).
    #[test]
    fn the_state_of_the_plan_carries_the_clones_of_the_user_plan() {
        crate::install_engines().expect("the engines of the application are installed");
        gx_mcp::bootstrap();
        let store = gx_mcp::store::diagram_store();
        let diagram_id = store.next_id();
        store.insert(
            diagram_id.clone(),
            "flujo".to_string(),
            "graph".to_string(),
            "stdio-planos-usuario".to_string(),
            MINIMAL.to_string(),
        );

        let state = super::PlanState::default();
        let paint = state
            .create_clone(&diagram_id, Some(gx_core::geom::Size::new(1280.0, 800.0)))
            .expect("el dueño clona el diagrama");

        let payload =
            serde_json::to_value(super::plan_payload(&state)).expect("the state serializes");
        let clones = payload["clones"]
            .as_array()
            .expect("the state carries the clones of the user plan");
        assert_eq!(clones.len(), 1, "el clon que el humano pidió está ahí");
        let clone = &clones[0];
        assert_eq!(clone["clone_id"], paint.entry.clone_id.as_str());
        assert_eq!(clone["origin_diagram_id"], diagram_id.as_str());
        assert_eq!(clone["origin_session_id"], "stdio-planos-usuario");
        assert_eq!(clone["title"], "flujo");
        assert_eq!(clone["snapshot_version"], 1);
        assert_eq!(clone["updatable"], true);
        assert!(
            clone["pos"]["x"].is_number() && clone["bbox"]["w"].is_number(),
            "la página arma el objeto con el lugar y el tamaño del dueño: {clone}"
        );
        assert!(
            clone.get("diagram_id").is_none(),
            "un clon no es un diagrama: no lleva `diagram_id` (S4): {clone}"
        );
        assert!(
            !store.known_ids().contains(&paint.entry.clone_id),
            "el id del clon no está en el store: ninguna tool lo ve (V2)"
        );
    }
}
