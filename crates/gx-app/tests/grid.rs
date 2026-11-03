//! The geometry of the plan grid (PRD §7.1, RF24, decision S23).
//!
//! The grid lives in the frontend, but its numbers are a contract: the CSS,
//! the PRD and this test have to say the same thing. The test locks the
//! geometry so a change in one place does not silently change the others.

use std::f64::consts::PI;

/// The deviation of the lines from the vertical (RF24).
const ANGLE_DEGREES: f64 = 45.0;
/// The diagonal of the rhombus: the measure the human sees (RF24).
const DIAGONAL: f64 = 59.0;
/// The perpendicular step between two lines of the same family.
const STEP: f64 = 41.72;

fn sin_degrees(degrees: f64) -> f64 {
    (degrees * PI / 180.0).sin()
}

fn cos_degrees(degrees: f64) -> f64 {
    (degrees * PI / 180.0).cos()
}

#[test]
fn the_perpendicular_step_follows_the_major_diagonal() {
    let expected = DIAGONAL * sin_degrees(ANGLE_DEGREES);
    assert!(
        (expected - STEP).abs() < 0.01,
        "the step is {expected:.3}, the stylesheet says {STEP}"
    );
}

#[test]
fn the_cell_is_a_rhombus_with_equal_sides() {
    // The sides of the cell: the step over the sine of the angle between
    // the two families (90 degrees at 45).
    let side = STEP / sin_degrees(2.0 * ANGLE_DEGREES);
    // The diagonals of the cell.
    let major = 2.0 * side * cos_degrees(ANGLE_DEGREES);
    let minor = 2.0 * side * sin_degrees(ANGLE_DEGREES);

    assert!(
        (major - DIAGONAL).abs() < 0.5,
        "the major diagonal is {major:.2}, the PRD says {DIAGONAL}"
    );
    assert!(
        (minor - DIAGONAL).abs() < 0.5,
        "the minor diagonal is {minor:.2}: at 45 degrees both diagonals of \
         the cell have to measure the same"
    );
    // A rhombus with equal diagonals is a square turned on its corner: the
    // cells read as diamonds, not as stripes.
    assert!((major - minor).abs() < 0.5);
}

#[test]
fn the_stylesheet_uses_the_same_numbers() {
    let css = include_str!("../frontend/style.css");
    assert!(
        css.contains("--grid-angle: 45deg"),
        "the stylesheet must declare the angle of RF24"
    );
    assert!(
        css.contains("--grid-diagonal: 59px"),
        "the stylesheet must declare the diagonal of RF24"
    );
    assert!(
        css.contains("--grid-step: 41.72px"),
        "the stylesheet must declare the perpendicular step"
    );
    // The phase is computed per family, never with a shared offset.
    assert!(
        css.contains("--shift-a: calc(var(--grid-cos) * var(--tx) - var(--grid-sin) * var(--ty))"),
        "the phase of the first family is the pan projected on its normal"
    );
    assert!(
        css.contains("--shift-b: calc(var(--grid-cos) * var(--tx) + var(--grid-sin) * var(--ty))"),
        "the phase of the second family is the pan projected on its normal"
    );
    assert!(
        !css.contains("background-position: calc(-1 * var(--tx))"),
        "a shared offset would break the lattice while panning"
    );
}

#[test]
fn the_prd_states_the_same_geometry() {
    let prd = include_str!("../../../docs/PRD.md");
    assert!(
        prd.contains("separación 59 px"),
        "RF24 keeps the separation"
    );
    assert!(
        prd.contains("45° de la vertical"),
        "RF24 states the deviation from the vertical"
    );
    assert!(
        prd.contains("41.72 px"),
        "RF24 states the perpendicular step derived from the diagonal"
    );
}

#[test]
fn the_frontend_keeps_the_sine_and_the_cosine_in_sync() {
    let js = include_str!("../frontend/plan.js");
    assert!(
        js.contains("GRID_ANGLE_DEGREES = 45"),
        "the frontend owns the angle of the grid"
    );
    assert!(
        js.contains("--grid-sin"),
        "the frontend has to publish the sine to the stylesheet"
    );
    assert!(
        js.contains("--grid-cos"),
        "the frontend has to publish the cosine to the stylesheet"
    );
}
