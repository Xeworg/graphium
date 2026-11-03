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
    // The tile has to be a multiple of the period of the pattern: with the
    // tile at the size of the viewport the mesh breaks into strips while
    // the camera moves, and a tile that does not scale breaks the same way
    // when the camera zooms out. It is rectangular because the period along
    // the rows depends on the angle (see `the_tile_keeps_the_mesh_seamless`).
    // The whitespace of the declaration does not matter; the rule does.
    let flat: String = css.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flat.contains(
            "calc(var(--grid-diagonal) * var(--grid-tan) * var(--zoom)) \
             calc(var(--grid-diagonal) * var(--zoom))"
        ),
        "the tile of the grid must be one cell of the mesh, scaled with the camera"
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
    assert!(
        js.contains("--zoom"),
        "the frontend has to publish the zoom: the tile of the grid scales with it"
    );
}

/// The period of the mesh along a row, in plan units: the perpendicular step
/// between two lines divided by the cosine of the inclination.
fn period_along_rows(degrees: f64, separation: f64) -> f64 {
    let theta = degrees * PI / 180.0;
    separation * theta.sin() / theta.cos()
}

/// The period of the mesh along a column, in plan units.
fn period_along_columns(separation: f64) -> f64 {
    separation
}

fn is_a_multiple(length: f64, period: f64) -> bool {
    let ratio = length / period;
    (ratio - ratio.round()).abs() < 1e-6
}

/// The angle is a setting (RF25), so the tile has to keep the mesh seamless
/// for every angle the panel offers, not only for the 45 degrees of the
/// preset. A square tile of side `D` is seamless only when `cot(angle)` is a
/// whole number: it works at 45 degrees and breaks at 20, 30 or 60.
/// `D * tan(angle)` by `D` is seamless at every angle and is the same square
/// at 45 degrees.
#[test]
fn the_tile_keeps_the_mesh_seamless_at_every_angle() {
    for degrees in [15.0, 20.0, 30.0, 45.0, 60.0, 75.0] {
        let theta = degrees * PI / 180.0;
        let rectangle_width = DIAGONAL * theta.tan();
        let rectangle_height = DIAGONAL;

        assert!(
            is_a_multiple(rectangle_width, period_along_rows(degrees, DIAGONAL)),
            "the rectangular tile does not tile the rows at {degrees} degrees"
        );
        assert!(
            is_a_multiple(rectangle_height, period_along_columns(DIAGONAL)),
            "the rectangular tile does not tile the columns at {degrees} degrees"
        );

        let square_is_seamless = is_a_multiple(DIAGONAL, period_along_rows(degrees, DIAGONAL));
        let cotangent_is_whole = {
            let cotangent = 1.0 / theta.tan();
            (cotangent - cotangent.round()).abs() < 1e-6
        };
        assert_eq!(
            square_is_seamless, cotangent_is_whole,
            "the square tile is seamless exactly when cot(angle) is whole ({degrees} degrees)"
        );
    }

    // The proof that the shape of the tile was not arbitrary: at 20 degrees
    // the square is off by more than a pixel and the human would see a seam.
    assert!(
        !is_a_multiple(DIAGONAL, period_along_rows(20.0, DIAGONAL)),
        "a square tile has to break at 20 degrees"
    );
}

/// Cada clase que el SVG pinta necesita su regla en el canvas: la CSP bloquea
/// el `<style>` interno del SVG, así que en la ventana la paleta la pone la
/// hoja de estilos. Sin la regla, el elemento sale con el color del navegador
/// —una línea de vida invisible, una punta de flecha negra— sin que nada falle.
///
/// El conjunto a cubrir no es "todas las clases": es "las que el propio SVG
/// pinta", o sea las que aparecen en su bloque `<style>`. Las que sólo nombran
/// geometría o semántica (`shape-rounded` lleva el `rx` en el atributo,
/// `cardinality-optional` no pinta nada por su cuenta) no necesitan regla.
#[test]
fn every_painted_class_of_the_drawing_has_a_rule_in_the_canvas() {
    let css = include_str!("../frontend/style.css");
    let goldens = [
        include_str!("golden/graph-minimal.svg"),
        include_str!("golden/er-posts.svg"),
        include_str!("golden/tree-src.svg"),
        include_str!("golden/mindmap-feature.svg"),
        include_str!("golden/sequence-order.svg"),
        include_str!("golden/timeline-roadmap.svg"),
        include_str!("golden/swimlane-soporte.svg"),
        include_str!("golden/flowchart-approval.svg"),
        include_str!("golden/state-order.svg"),
    ];

    let mut painted: Vec<String> = Vec::new();
    let mut used: Vec<String> = Vec::new();
    for golden in goldens {
        // Lo que el SVG pinta: los selectores de su bloque de estilo.
        let style_block = golden
            .split("<style>")
            .nth(1)
            .and_then(|rest| rest.split("</style>").next())
            .expect("the golden carries its style block");
        for selector in style_block.split('}') {
            let Some(name) = selector.split('{').next() else {
                continue;
            };
            let name = name.trim().trim_start_matches('.');
            if !name.is_empty() && !name.contains(',') && !painted.iter().any(|k| k == name) {
                painted.push(name.to_string());
            }
        }
        // Lo que el SVG usa: las clases de los elementos.
        for chunk in golden.split("class=\"").skip(1) {
            let Some(names) = chunk.split('"').next() else {
                continue;
            };
            for name in names.split_whitespace() {
                if !used.iter().any(|known| known == name) {
                    used.push(name.to_string());
                }
            }
        }
    }

    let mut missing: Vec<&String> = Vec::new();
    for class in &used {
        if !painted.iter().any(|name| name == class) {
            continue;
        }
        if !css.contains(&format!(".graphium .{class}")) {
            missing.push(class);
        }
    }
    assert!(
        missing.is_empty(),
        "estas clases que el SVG pinta no tienen regla en el canvas y saldrían \
         con el color del navegador: {missing:?}"
    );
}
