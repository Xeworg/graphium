//! The camera copy and the placement of a diagram on the plan (PRD §7.2).
//!
//! The camera lives in the frontend. Rust keeps only a copy
//! ([`CameraSnapshot`]) that the frontend pushes with the `camera-changed`
//! event, and uses it to compute where a new diagram goes.

use gx_core::geom::{Point, Rect, Size};

/// Copy of the frontend camera, in screen pixels and zoom factor.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CameraSnapshot {
    pub tx: f64,
    pub ty: f64,
    pub zoom: f64,
}

impl CameraSnapshot {
    pub fn new(tx: f64, ty: f64, zoom: f64) -> Self {
        Self { tx, ty, zoom }
    }

    /// The world point at the center of the viewport.
    pub fn view_center(&self, viewport: Size) -> Point {
        let zoom = if self.zoom > 0.0 { self.zoom } else { 1.0 };
        Point::new(
            (viewport.w / 2.0 - self.tx) / zoom,
            (viewport.h / 2.0 - self.ty) / zoom,
        )
    }
}

/// Where a diagram of `size` goes so that its center lands on the center of
/// the current view (PRD §7.2, RF27).
pub fn place(size: Size, camera: &CameraSnapshot, viewport: Size) -> Rect {
    let center = camera.view_center(viewport);
    Rect::new(
        center.x - size.w / 2.0,
        center.y - size.h / 2.0,
        size.w,
        size.h,
    )
}

#[cfg(test)]
mod tests {
    use super::{place, CameraSnapshot};
    use gx_core::geom::{Rect, Size};

    const VIEWPORT: Size = Size::new(1280.0, 800.0);

    #[test]
    fn the_diagram_lands_on_the_center_of_the_view() {
        let camera = CameraSnapshot::new(0.0, 0.0, 1.0);
        let rect = place(Size::new(200.0, 100.0), &camera, VIEWPORT);
        assert_eq!(rect.center(), gx_core::geom::Point::new(640.0, 400.0));
    }

    #[test]
    fn the_zoom_does_not_change_the_size_of_the_diagram() {
        let camera = CameraSnapshot::new(0.0, 0.0, 2.0);
        let rect = place(Size::new(200.0, 100.0), &camera, VIEWPORT);
        assert_eq!(rect.w, 200.0);
        assert_eq!(rect.h, 100.0);
    }

    #[test]
    fn the_pan_moves_the_diagram_with_the_view() {
        let camera = CameraSnapshot::new(-100.0, -50.0, 1.0);
        let rect = place(Size::new(200.0, 100.0), &camera, VIEWPORT);
        assert_eq!(rect.center(), gx_core::geom::Point::new(740.0, 450.0));
    }

    #[test]
    fn the_placement_is_a_rectangle() {
        let camera = CameraSnapshot::new(0.0, 0.0, 1.0);
        let rect = place(Size::new(10.0, 10.0), &camera, VIEWPORT);
        assert_eq!(rect, Rect::new(635.0, 395.0, 10.0, 10.0));
    }
}
