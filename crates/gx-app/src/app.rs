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
}

/// One diagram of the state that Rust owns (PRD §8.6.1, S22).
#[derive(Debug, Clone, Serialize)]
struct StateEntry {
    diagram_id: String,
    title: String,
    kind: String,
    updated_at: String,
    session_id: String,
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
    let ready_state = Arc::clone(&state);
    handle.listen("graphium://ready", move |_event| {
        ready_state.set_ready();
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
        let event = RenderEvent {
            req_id: req_id.clone(),
            diagram_id: payload.diagram_id.clone(),
            svg: payload.svg.clone(),
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
        let diagrams: Vec<StateEntry> = gx_mcp::store::diagram_store()
            .list()
            .into_iter()
            .map(|summary| StateEntry {
                diagram_id: summary.diagram_id,
                title: summary.title,
                kind: summary.kind,
                updated_at: summary.updated_at,
                session_id: summary.session_id,
            })
            .collect();
        let _ = window.emit("graphium://state", diagrams);

        gx_mcp::tools::RenderStatus::Painted
    });
    gx_mcp::tools::set_render_hook(Some(hook));
}
