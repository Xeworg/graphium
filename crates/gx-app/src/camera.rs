//! The camera copy and the placement of a diagram on the plan (PRD §7.2).
//!
//! The camera lives in the frontend. Rust keeps only a copy
//! ([`CameraSnapshot`]) that the frontend pushes with the `camera-changed`
//! event, and uses it to compute where a new diagram goes.

use gx_core::geom::{Point, Rect, Size};
use serde::{Deserialize, Serialize};

/// The margin left around a diagram when the view fits it (PRD §7.2).
pub const FIT_MARGIN: f64 = 32.0;
/// The floor of the fit: below this zoom the diagram is no longer shrunk
/// (PRD §7.2, S23). Degradation and `extract` solve the rest.
pub const FIT_MIN_ZOOM: f64 = 0.4;
/// The ceiling of the fit: an insertion never goes above 100% (PRD §7.2).
pub const FIT_MAX_ZOOM: f64 = 1.0;
/// The padding of the object of a diagram around its SVG (PRD §7.2).
pub const OBJECT_PADDING: f64 = 24.0;
/// The offset of one step of the cascade (PRD §7.2).
pub const CASCADE_OFFSET: f64 = 48.0;
/// How many cascade steps before the plan opens a new column (PRD §7.2).
pub const CASCADE_STEPS: usize = 8;

/// Copy of the frontend camera, in screen pixels and zoom factor.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
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

/// The size of the object of a diagram: its SVG plus the padding the PRD
/// gives it on every side (PRD §7.2).
pub fn object_size(svg: Size) -> Size {
    Size::new(svg.w + 2.0 * OBJECT_PADDING, svg.h + 2.0 * OBJECT_PADDING)
}

/// Where a diagram of `svg` goes without landing on top of another one.
///
/// It starts on the center of the view (RF27) and, while a step of the
/// cascade lands within the tolerance of an occupied rectangle, moves
/// `(48, 48)`. After [`CASCADE_STEPS`] steps the spot is still taken, so the
/// plan opens a column to the right of everything (PRD §7.2).
pub fn place_with_cascade(
    svg: Size,
    camera: &CameraSnapshot,
    viewport: Size,
    occupied: &[Rect],
) -> Rect {
    let size = object_size(svg);
    let mut rect = place(size, camera, viewport);

    for _ in 0..=CASCADE_STEPS {
        if !lands_on_another(rect, occupied) {
            return rect;
        }
        rect = Rect::new(
            rect.x + CASCADE_OFFSET,
            rect.y + CASCADE_OFFSET,
            rect.w,
            rect.h,
        );
    }

    // The cascade ran out: a new column, aligned with the top of everything.
    let right = occupied
        .iter()
        .map(|other| other.max_x())
        .fold(f64::NEG_INFINITY, f64::max);
    let top = occupied
        .iter()
        .map(|other| other.min_y())
        .fold(f64::INFINITY, f64::min);
    Rect::new(right + CASCADE_OFFSET, top, rect.w, rect.h)
}

/// Whether `rect` lands within the tolerance of an occupied rectangle.
///
/// The tolerance is the padding of the object: two diagrams that are closer
/// than that read as one, so they count as overlapping (PRD §7.2).
fn lands_on_another(rect: Rect, occupied: &[Rect]) -> bool {
    occupied
        .iter()
        .any(|other| inflated(rect).overlaps(&inflated(*other)))
}

/// The rectangle grown by the tolerance of the PRD on every side: two
/// diagrams closer than that read as one.
fn inflated(rect: Rect) -> Rect {
    Rect::new(
        rect.x - OBJECT_PADDING,
        rect.y - OBJECT_PADDING,
        rect.w + 2.0 * OBJECT_PADDING,
        rect.h + 2.0 * OBJECT_PADDING,
    )
}

/// The camera that frames `rect` in `viewport` (PRD §7.2, S23).
///
/// The zoom is the one that leaves [`FIT_MARGIN`] on the tighter side,
/// clamped between [`FIT_MIN_ZOOM`] and [`FIT_MAX_ZOOM`], so a huge diagram
/// stays readable instead of shrinking forever and a small one is never
/// blown up. The diagram is centered in the viewport.
pub fn fit(rect: Rect, viewport: Size) -> CameraSnapshot {
    let available_w = (viewport.w - 2.0 * FIT_MARGIN).max(0.0);
    let available_h = (viewport.h - 2.0 * FIT_MARGIN).max(0.0);

    let zoom_x = if rect.w > 0.0 {
        available_w / rect.w
    } else {
        FIT_MAX_ZOOM
    };
    let zoom_y = if rect.h > 0.0 {
        available_h / rect.h
    } else {
        FIT_MAX_ZOOM
    };
    let zoom = zoom_x.min(zoom_y).clamp(FIT_MIN_ZOOM, FIT_MAX_ZOOM);

    // The center of the diagram lands on the center of the viewport.
    let center = rect.center();
    CameraSnapshot::new(
        viewport.w / 2.0 - center.x * zoom,
        viewport.h / 2.0 - center.y * zoom,
        zoom,
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
