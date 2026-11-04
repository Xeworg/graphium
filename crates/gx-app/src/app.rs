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

/// State of the plan that Rust keeps: the camera copy and the pending
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

/// Builds the Tauri application with the plan window.
///
/// `keep_alive` is true in MCP mode: closing the window must not kill the
/// process while a session is alive (PRD §8.6).
///
/// The plan state is created here and shared with the render hook: the setup
/// hook of Tauri 2 only runs when the event loop starts, so a state that
/// lives in the Tauri state manager would not exist yet when the MCP thread
/// asks for it.
pub fn build(keep_alive: bool) -> (tauri::App<tauri::Wry>, Arc<PlanState>) {
    let state = Arc::new(PlanState::default());

    let application = tauri::Builder::default()
        .build(tauri::generate_context!())
        .expect("the tauri application must build");

    // The window is created here, before the event loop starts: the window
    // declared in the configuration only appears when the loop is ready, and
    // by then the page may have already announced itself.
    let handle = application.handle().clone();

    let window = tauri::WebviewWindowBuilder::new(&handle, "main", tauri::WebviewUrl::default())
        .title("Graphium")
        .inner_size(1280.0, 800.0)
        .resizable(true)
        .build()
        .expect("the main window must be created");

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
        let _ = close_handle.emit("graphium://state", state_entries());
    });

    let ready_state = Arc::clone(&state);
    let ready_handle = handle.clone();
    handle.listen("graphium://ready", move |_event| {
        ready_state.set_ready();
        let _ = ready_handle.emit("graphium://settings", ready_state.settings());
    });

    let settings_state = Arc::clone(&state);
    let settings_handle = handle.clone();
    handle.listen("settings-changed", move |event| {
        let Ok(settings) = serde_json::from_str::<Settings>(event.payload()) else {
            return;
        };
        if settings.save().is_err() {
            return;
        }
        // What the file holds is what the panel shows: the sanitised value
        // comes back, so a rejected field never leaves the UI lying.
        let settings = Settings::load();
        settings_state.set_settings(settings.clone());
        // The export follows the theme of the plan: the human changed what it
        // sees, so the next export paints with it (decision L7).
        gx_core::export::install_theme(settings.theme());
        let _ = settings_handle.emit("graphium://settings", settings);
    });

    (application, state)
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

        let Some(window) = handle.get_webview_window("main") else {
            return gx_mcp::tools::RenderStatus::NoWindow;
        };
        let _ = window.show();
        let _ = window.unminimize();

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
        let _ = window.emit("graphium://state", state_entries());

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
}
