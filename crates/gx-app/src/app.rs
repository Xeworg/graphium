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
use gx_core::geom::Size;

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

/// The state of the plan that Rust keeps: the camera copy and the pending
/// acknowledgements.
#[derive(Default)]
pub struct PlanState {
    camera: Mutex<CameraSnapshot>,
    pending: Mutex<HashMap<String, Sender<bool>>>,
    ready: AtomicBool,
    settings: Mutex<Settings>,
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

#[derive(Debug, Clone, Serialize)]
struct RenderEvent {
    req_id: String,
    diagram_id: String,
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
        .map(|summary| StateEntry {
            diagram_id: summary.diagram_id,
            title: summary.title,
            kind: summary.kind,
            updated_at: summary.updated_at,
            session_id: summary.session_id,
            pos: summary.pos,
            bbox: summary.bbox,
        })
        .collect()
}

/// The state of the plan the panel reads (PRD §8.6.1, S8; feature
/// `f3-sesiones`, S1/S3/D3/D5).
///
/// Besides the diagrams, it carries **every** session of the owner, because
/// the human is the exception to the isolation of D3: the selector lists them
/// all and chooses which ones the plan shows. Each diagram already names its
/// session (`session_id`), which is the key the selector and the badge use.
#[derive(Debug, Clone, Serialize)]
struct PlanPayload {
    diagrams: Vec<StateEntry>,
    /// The sessions in the deterministic order of the registry (a
    /// `BTreeMap`, never a `HashMap`): the rows never jump around.
    sessions: Vec<gx_mcp::session::SessionView>,
}

/// The whole state of the plan: what is drawn and who drew it.
///
/// The store is read first on purpose: reading it is the tick that ends the
/// sessions whose grace finished (D4, S2), so a session that ended is already
/// gone from the list the selector reads instead of lingering as a row with no
/// diagrams.
fn plan_payload() -> PlanPayload {
    let diagrams = state_entries();
    let sessions = gx_mcp::session::session_registry()
        .list()
        .into_iter()
        .map(|record| record.view())
        .collect();
    PlanPayload { diagrams, sessions }
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
        let _ = close_handle.emit("graphium://state", plan_payload());
    });

    let ready_state = Arc::clone(&state);
    let ready_handle = handle.clone();
    handle.listen("graphium://ready", move |_event| {
        ready_state.set_ready();
        let _ = ready_handle.emit("graphium://settings", ready_state.settings());
        // The selector of sessions is built from the state, like the panel
        // (S8): the page gets one as soon as it is alive, so the rows are
        // there before the first render and never invented by the page.
        let _ = ready_handle.emit("graphium://state", plan_payload());
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
        // another (PRD §7.2, RF27).
        let occupied: Vec<gx_core::geom::Rect> = gx_mcp::store::diagram_store()
            .list()
            .into_iter()
            .filter(|summary| summary.diagram_id != payload.diagram_id)
            .filter_map(|summary| match (summary.pos, summary.bbox) {
                (Some(pos), Some(bbox)) => {
                    Some(gx_core::geom::Rect::new(pos.x, pos.y, bbox.w, bbox.h))
                }
                _ => None,
            })
            .collect();
        let placement = viewport.map(|viewport| {
            camera::place_with_cascade(
                Size::new(payload.width, payload.height),
                &state.camera(),
                viewport,
                &occupied,
            )
        });
        if let Some(rect) = placement {
            gx_mcp::store::diagram_store().place(
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
        let _ = window.emit("graphium://state", plan_payload());

        gx_mcp::tools::RenderStatus::Painted
    });
    gx_mcp::tools::set_render_hook(Some(hook));
}

#[cfg(test)]
mod tests {
    use super::StateEntry;

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
        };

        let json = serde_json::to_value(&entry).expect("the state serializes");

        assert!(json.get("pos").is_none(), "{json}");
        assert!(json.get("bbox").is_none(), "{json}");
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
    /// carry **every** session of the owner — the human sees all of them — and
    /// say which session drew each diagram, or the selector cannot group a row
    /// with its diagrams.
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

        let payload = serde_json::to_value(super::plan_payload()).expect("the state serializes");

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

        // D4: a session in grace is still a row of the selector — the human
        // sees everything (D3) — with its state spelled out for the page.
        registry.begin_grace(&claude_id, std::time::SystemTime::now());
        let in_grace = serde_json::to_value(super::plan_payload()).expect("serializes");
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
                "the session of a diagram is one the selector lists, or the row \
                 and its diagrams would disagree: {entry}"
            );
        }
    }
}
