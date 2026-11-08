//! The fit of a diagram in the view (PRD §7.2, S23).
//!
//! `fit` is a pure function: the diagram rectangle in plan units and the
//! viewport in CSS pixels go in, the camera that frames it comes out. The
//! margin and the floor are a contract with the PRD, so they are pinned here
//! instead of being left to the frontend.

use gx_app::camera::{fit, CameraSnapshot, FIT_MARGIN, FIT_MIN_ZOOM};
use gx_core::geom::{Rect, Size};

const VIEWPORT: Size = Size::new(1280.0, 800.0);

/// The screen rectangle of a plan rectangle, as the camera paints it:
/// `screen = plan * zoom + translation`.
fn project(rect: Rect, camera: CameraSnapshot) -> Rect {
    Rect::new(
        rect.x * camera.zoom + camera.tx,
        rect.y * camera.zoom + camera.ty,
        rect.w * camera.zoom,
        rect.h * camera.zoom,
    )
}

#[test]
fn a_diagram_that_does_not_fit_is_scaled_down_with_a_margin() {
    let rect = Rect::new(0.0, 0.0, 2000.0, 1000.0);
    let camera = fit(rect, VIEWPORT);
    let screen = project(rect, camera);

    // The width is the tighter side: (1280 - 64) / 2000.
    assert!((camera.zoom - 0.608).abs() < 1e-3, "zoom {}", camera.zoom);
    assert!(
        (screen.x - FIT_MARGIN).abs() < 1e-6,
        "left margin {}",
        screen.x
    );
    assert!(
        (screen.max_x() - (VIEWPORT.w - FIT_MARGIN)).abs() < 1e-6,
        "right margin {}",
        VIEWPORT.w - screen.max_x()
    );
    assert!(screen.y >= FIT_MARGIN - 1e-6);
    assert!(screen.max_y() <= VIEWPORT.h - FIT_MARGIN + 1e-6);
}

#[test]
fn the_fit_never_goes_below_the_floor() {
    let rect = Rect::new(0.0, 0.0, 10000.0, 8000.0);
    let camera = fit(rect, VIEWPORT);
    assert_eq!(
        camera.zoom, FIT_MIN_ZOOM,
        "below the floor the diagram is no longer shrunk: degradation and \
         extract solve the rest (PRD §7.2)"
    );
    assert_eq!(FIT_MIN_ZOOM, 0.4);
}

#[test]
fn the_fit_does_not_blow_a_small_diagram_up() {
    let rect = Rect::new(0.0, 0.0, 100.0, 50.0);
    let camera = fit(rect, VIEWPORT);
    assert_eq!(camera.zoom, 1.0, "the insertion never goes above 100%");
    let screen = project(rect, camera);
    assert!((screen.center().x - VIEWPORT.w / 2.0).abs() < 1e-6);
    assert!((screen.center().y - VIEWPORT.h / 2.0).abs() < 1e-6);
}

#[test]
fn the_fit_centers_the_diagram() {
    let rect = Rect::new(-500.0, 300.0, 900.0, 700.0);
    let camera = fit(rect, VIEWPORT);
    let screen = project(rect, camera);
    assert!((screen.center().x - VIEWPORT.w / 2.0).abs() < 1e-6);
    assert!((screen.center().y - VIEWPORT.h / 2.0).abs() < 1e-6);
}

#[test]
fn a_diagram_bigger_than_the_floor_allows_stays_centered() {
    let rect = Rect::new(-4000.0, -3000.0, 10000.0, 8000.0);
    let camera = fit(rect, VIEWPORT);
    let screen = project(rect, camera);
    assert_eq!(camera.zoom, FIT_MIN_ZOOM);
    assert!((screen.center().x - VIEWPORT.w / 2.0).abs() < 1e-6);
    assert!((screen.center().y - VIEWPORT.h / 2.0).abs() < 1e-6);
}

#[test]
fn an_empty_viewport_does_not_divide_by_zero() {
    let rect = Rect::new(0.0, 0.0, 200.0, 100.0);
    let camera = fit(rect, Size::new(0.0, 0.0));
    assert!(camera.zoom.is_finite() && camera.zoom > 0.0, "{camera:?}");
}

#[test]
fn the_plan_keeps_the_world_point_under_the_center_when_the_window_resizes() {
    // The resize is not an intent of Rust: the camera lives in the frontend,
    // and refitting on resize would throw away the human's view (PRD §7.2,
    // S23). The handler has to preserve the world point under the center of
    // the viewport instead of framing the diagram again.
    let script = include_str!("../frontend/src/App.svelte");
    let start = script
        .find("function onResize() {")
        .expect("the plan must react to a window resize");
    let rest = &script[start..];
    let end = rest.find("\n  }").expect("the resize handler is closed");
    let handler = &rest[..end];
    assert!(
        handler.contains("(next.w - viewport.w) / 2")
            && handler.contains("(next.h - viewport.h) / 2"),
        "the resize handler must move the camera by half of the growth of \
         the viewport, which is what keeps the world point under the center: \
         {handler}"
    );
    assert!(
        !handler.contains("fit("),
        "the resize must not refit the diagram: {handler}"
    );
}
