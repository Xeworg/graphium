//! The Tauri application: the plan and its window (PRD §7, §8.6.1).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Listener, Manager, WindowEvent};

use crate::camera::CameraSnapshot;

/// How long the frontend has to confirm the paint (PRD §8.6.1, step 5).
pub const RENDER_ACK_TIMEOUT_MS: u64 = 2000;

static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(0);

/// State of the plan that Rust keeps: the camera copy and the pending
/// acknowledgements.
#[derive(Default)]
pub struct PlanState {
    camera: Mutex<CameraSnapshot>,
    pending: Mutex<HashMap<String, Sender<bool>>>,
}

impl PlanState {
    pub fn camera(&self) -> CameraSnapshot {
        *self.camera.lock().expect("camera lock")
    }

    pub fn set_camera(&self, camera: CameraSnapshot) {
        *self.camera.lock().expect("camera lock") = camera;
    }

    fn register_ack(&self, req_id: &str) -> std::sync::mpsc::Receiver<bool> {
        let (sender, receiver) = channel();
        self.pending
            .lock()
            .expect("pending lock")
            .insert(req_id.to_string(), sender);
        receiver
    }

    fn resolve_ack(&self, req_id: &str, ok: bool) {
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

    (application, state)
}

/// Installs the hook that paints a diagram in the window and waits for the
/// frontend to confirm it.
pub fn install_render_hook(handle: tauri::AppHandle, state: Arc<PlanState>) {
    let hook: gx_mcp::tools::RenderHook = Arc::new(move |payload| {
        let req_id = format!("req-{}", REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed));
        let receiver = state.register_ack(&req_id);

        let Some(window) = handle.get_webview_window("main") else {
            return false;
        };
        let _ = window.show();
        let _ = window.unminimize();

        // The render is emitted and the answer is awaited: the timeout of
        // the acknowledgement is the only wait (PRD §8.6.1, step 5). A
        // webview that is not alive yet never answers, and the render ends
        // with `rendered: false` and the reason `ui_timeout`.
        let event = RenderEvent {
            req_id: req_id.clone(),
            diagram_id: payload.diagram_id.clone(),
            svg: payload.svg.clone(),
        };
        if window.emit("graphium://render", event).is_err() {
            return false;
        }

        let acknowledged = receiver
            .recv_timeout(Duration::from_millis(RENDER_ACK_TIMEOUT_MS))
            .unwrap_or(false);
        if !acknowledged {
            state.pending.lock().expect("pending lock").remove(&req_id);
        }
        acknowledged
    });
    gx_mcp::tools::set_render_hook(Some(hook));
}
