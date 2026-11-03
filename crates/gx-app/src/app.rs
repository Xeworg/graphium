//! The Tauri application: the plan and its window (PRD §7, §8.6.1).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Listener, Manager, WindowEvent};

use crate::camera::CameraSnapshot;

/// How long the frontend has to confirm the paint (PRD §8.6.1, step 5).
pub const RENDER_ACK_TIMEOUT_MS: u64 = 2000;
/// How long the frontend has to announce itself as ready.
pub const READY_TIMEOUT_MS: u64 = 2000;

static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(0);

/// State of the plan that Rust keeps: the camera copy and the pending
/// acknowledgements.
#[derive(Default)]
pub struct PlanState {
    camera: Mutex<CameraSnapshot>,
    pending: Mutex<HashMap<String, Sender<bool>>>,
    ready: AtomicBool,
}

impl PlanState {
    pub fn camera(&self) -> CameraSnapshot {
        *self.camera.lock().expect("camera lock")
    }

    pub fn set_camera(&self, camera: CameraSnapshot) {
        *self.camera.lock().expect("camera lock") = camera;
    }

    /// Whether the frontend already announced itself (`graphium://ready`).
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Relaxed)
    }

    pub fn set_ready(&self) {
        self.ready.store(true, Ordering::Relaxed);
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
pub fn build(keep_alive: bool) -> tauri::App<tauri::Wry> {
    tauri::Builder::default()
        .setup(move |app| {
            app.manage(PlanState::default());
            let handle = app.handle().clone();

            let Some(window) = app.get_webview_window("main") else {
                return Err(Box::<dyn std::error::Error>::from(std::io::Error::other(
                    "the main window is missing",
                )));
            };

            // Closing the window does not kill the process while an MCP
            // session is alive: the window hides and the next render brings
            // it back (PRD §8.6).
            let hidden = window.clone();
            window.on_window_event(move |event| {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    if keep_alive {
                        api.prevent_close();
                        let _ = hidden.hide();
                    }
                }
            });

            let camera_window = window.clone();
            let ack_window = window.clone();
            let ready_window = window.clone();
            let camera_handle = handle.clone();
            let ack_handle = handle.clone();
            let ready_handle = handle.clone();
            camera_window.listen("camera-changed", move |event| {
                if let Ok(payload) = serde_json::from_str::<CameraPayload>(event.payload()) {
                    let state = camera_handle.state::<PlanState>();
                    state.set_camera(CameraSnapshot::new(payload.tx, payload.ty, payload.zoom));
                }
            });
            ack_window.listen("render_ack", move |event| {
                if let Ok(payload) = serde_json::from_str::<AckPayload>(event.payload()) {
                    let state = ack_handle.state::<PlanState>();
                    state.resolve_ack(&payload.req_id, payload.ok);
                }
            });
            ready_window.listen("graphium://ready", move |_event| {
                ready_handle.state::<PlanState>().set_ready();
            });

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("the tauri application must build")
}

/// Installs the hook that paints a diagram in the window and waits for the
/// frontend to confirm it.
pub fn install_render_hook(handle: tauri::AppHandle) {
    let hook: gx_mcp::tools::RenderHook = Arc::new(move |payload| {
        let req_id = format!("req-{}", REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed));
        let receiver = handle.state::<PlanState>().register_ack(&req_id);

        let Some(window) = handle.get_webview_window("main") else {
            return false;
        };
        let _ = window.show();
        let _ = window.unminimize();

        // The frontend announces itself when the webview is alive; the
        // render waits for that announcement before giving up (PRD §8.6.1).
        let deadline = Instant::now() + Duration::from_millis(READY_TIMEOUT_MS);
        while !handle.state::<PlanState>().is_ready() {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

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
            let state = handle.state::<PlanState>();
            state.pending.lock().expect("pending lock").remove(&req_id);
        }
        acknowledged
    });
    gx_mcp::tools::set_render_hook(Some(hook));
}
