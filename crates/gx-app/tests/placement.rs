//! Placement and cascade of the objects on the plan (PRD §7.2, RF27, S5).
//!
//! Two diagrams never overlap and never land on top of each other: a new one
//! goes on the center of the view and, when that spot is taken, cascades
//! until it finds room. When the cascade runs out of steps it opens a new
//! column to the right of everything. This is pure geometry, so it is pinned
//! here instead of being left to the frontend.

use gx_app::camera::{
    object_size, place_with_cascade, CameraSnapshot, CASCADE_OFFSET, CASCADE_STEPS, OBJECT_PADDING,
};
use gx_core::geom::{Rect, Size};

const VIEWPORT: Size = Size::new(1280.0, 800.0);

fn camera() -> CameraSnapshot {
    CameraSnapshot::new(0.0, 0.0, 1.0)
}

/// Two objects overlap when their rectangles, inflated by the tolerance the
/// PRD gives, intersect: touching edges is not overlapping.
fn overlaps(a: Rect, b: Rect) -> bool {
    let inflate = |rect: Rect| {
        Rect::new(
            rect.x - OBJECT_PADDING,
            rect.y - OBJECT_PADDING,
            rect.w + 2.0 * OBJECT_PADDING,
            rect.h + 2.0 * OBJECT_PADDING,
        )
    };
    inflate(a).overlaps(&inflate(b))
}

#[test]
fn the_object_is_the_svg_plus_its_padding() {
    assert_eq!(OBJECT_PADDING, 24.0);
    assert_eq!(
        object_size(Size::new(200.0, 100.0)),
        Size::new(248.0, 148.0)
    );
}

#[test]
fn the_first_diagram_lands_on_the_center_of_the_view() {
    let rect = place_with_cascade(Size::new(200.0, 100.0), &camera(), VIEWPORT, &[]);
    assert_eq!(rect.center().x, VIEWPORT.w / 2.0);
    assert_eq!(rect.center().y, VIEWPORT.h / 2.0);
    assert_eq!(rect.w, 248.0);
    assert_eq!(rect.h, 148.0);
}

#[test]
fn a_second_diagram_on_the_same_spot_cascades() {
    let first = place_with_cascade(Size::new(200.0, 100.0), &camera(), VIEWPORT, &[]);
    let second = place_with_cascade(Size::new(200.0, 100.0), &camera(), VIEWPORT, &[first]);

    assert_eq!(CASCADE_OFFSET, 48.0);
    // The step is 48 px and the tolerance is 24 on each side, so a big object
    // takes more than one step: what matters is that it moved along the
    // diagonal of the cascade and that it no longer lands on the first one.
    assert!(second.x > first.x && second.y > first.y);
    let steps = (second.x - first.x) / CASCADE_OFFSET;
    assert!(
        (steps - steps.round()).abs() < 1e-6,
        "the cascade moves in whole steps of 48 px: {steps}"
    );
    assert!(!overlaps(first, second));
}

#[test]
fn no_two_objects_overlap_whatever_the_order() {
    let mut occupied: Vec<Rect> = Vec::new();
    for _ in 0..CASCADE_STEPS {
        let rect = place_with_cascade(Size::new(320.0, 200.0), &camera(), VIEWPORT, &occupied);
        for other in &occupied {
            assert!(
                !overlaps(rect, *other),
                "two diagrams overlap: {:?} and {:?}",
                rect,
                other
            );
        }
        occupied.push(rect);
    }
    assert_eq!(occupied.len(), CASCADE_STEPS);
}

#[test]
fn after_eight_steps_the_plan_opens_a_new_column() {
    assert_eq!(CASCADE_STEPS, 8);
    let mut occupied: Vec<Rect> = Vec::new();
    // Nine diagrams: the ninth cannot cascade any more.
    for _ in 0..=CASCADE_STEPS {
        let rect = place_with_cascade(Size::new(300.0, 150.0), &camera(), VIEWPORT, &occupied);
        occupied.push(rect);
    }

    let last = *occupied.last().expect("nine placed");
    let right_of_everything = occupied[..occupied.len() - 1]
        .iter()
        .map(|rect| rect.max_x())
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(
        (last.x - (right_of_everything + CASCADE_OFFSET)).abs() < 1e-6,
        "the new column starts after everything plus the offset: {} vs {}",
        last.x,
        right_of_everything + CASCADE_OFFSET
    );
    for other in &occupied[..occupied.len() - 1] {
        assert!(!overlaps(last, *other), "the new column overlaps {other:?}");
    }
}

#[test]
fn a_diagram_far_from_the_view_does_not_steal_the_center() {
    // A diagram placed elsewhere does not move the next one: only real
    // overlaps make the plan cascade.
    let far_away = Rect::new(9000.0, 9000.0, 248.0, 148.0);
    let rect = place_with_cascade(Size::new(200.0, 100.0), &camera(), VIEWPORT, &[far_away]);
    assert_eq!(rect.center().x, VIEWPORT.w / 2.0);
    assert_eq!(rect.center().y, VIEWPORT.h / 2.0);
}
