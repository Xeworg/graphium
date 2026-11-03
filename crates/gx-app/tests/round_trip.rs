//! The mechanics of the render round-trip (PRD §8.6.1).
//!
//! The window is a Tauri webview, so it cannot be opened from a test. What
//! can be tested is the part that decides the outcome: the acknowledgement
//! that unblocks the render, and the state that Rust owns.

use std::sync::Arc;
use std::time::Duration;

use gx_app::app::PlanState;

#[test]
fn the_acknowledgement_unblocks_the_render() {
    let state = PlanState::default();
    let receiver = state.register_ack("req-1");

    state.resolve_ack("req-1", true);

    assert_eq!(
        receiver.recv_timeout(Duration::from_millis(500)),
        Ok(true),
        "the render has to be unblocked by the acknowledgement"
    );
}

#[test]
fn a_rejected_paint_is_also_an_acknowledgement() {
    let state = PlanState::default();
    let receiver = state.register_ack("req-2");

    state.resolve_ack("req-2", false);

    assert_eq!(receiver.recv_timeout(Duration::from_millis(500)), Ok(false));
}

#[test]
fn an_unknown_acknowledgement_is_ignored() {
    let state = PlanState::default();
    state.resolve_ack("req-does-not-exist", true);
}

#[test]
fn the_camera_copy_is_the_one_the_frontend_pushed() {
    let state = PlanState::default();
    state.set_camera(gx_app::camera::CameraSnapshot::new(12.0, -34.0, 1.75));

    let camera = state.camera();
    assert_eq!(camera.tx, 12.0);
    assert_eq!(camera.ty, -34.0);
    assert_eq!(camera.zoom, 1.75);
}

#[test]
fn the_state_is_shared_between_the_window_and_the_hook() {
    let state = Arc::new(PlanState::default());
    let other = Arc::clone(&state);
    other.set_camera(gx_app::camera::CameraSnapshot::new(1.0, 2.0, 1.0));
    assert_eq!(state.camera().tx, 1.0);
}

#[test]
fn the_render_hook_maps_a_paint_into_a_reason() {
    use gx_mcp::tools::{RenderHookPayload, RenderStatus};

    let payload = RenderHookPayload {
        diagram_id: "d-0001".to_string(),
        kind: "graph".to_string(),
        svg: "<svg/>".to_string(),
        width: 100.0,
        height: 50.0,
        focus: None,
        metrics: gx_mcp::tools::Metrics {
            nodes: 2,
            edges: 1,
            layout_ms: 1,
            render_ms: 0,
            svg_bytes: 6,
            elapsed_ms: None,
        },
    };

    assert!(RenderStatus::Painted.painted());
    assert_eq!(RenderStatus::Painted.reason(), None);
    assert_eq!(RenderStatus::Headless.reason(), Some("headless"));
    assert_eq!(RenderStatus::NoWindow.reason(), Some("no_window"));
    assert_eq!(RenderStatus::Timeout.reason(), Some("ui_timeout"));
    assert_eq!(RenderStatus::Error.reason(), Some("ui_error"));
    assert!(!RenderStatus::Timeout.painted());
    let _ = payload;
}
