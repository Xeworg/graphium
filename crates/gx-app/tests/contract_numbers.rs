//! The numbers and tables of the contract, pinned against the one place the
//! plan keeps them (PRD §7.1, §7.2, §8.5.2; CONTRACT.md §6 and §3).
//!
//! The Svelte app is the only frontend of the tree. The numbers it paints
//! with are still a contract with Rust — the grid of RF24, the zoom the
//! contract offers, the ceiling of live SVGs, the keys of `Settings`, the
//! themes and the wire events — so they are pinned here, against
//! `frontend/src/lib/contract.js` (and the tables beside it), exactly as the
//! vanilla suites pinned them when the page lived in `frontend/plan.js`.
//!
//! What moved in the single-frontend cut is the pointer, never the value:
//! `59`, `45`, `41.72`, `24`, `3`, `0.4` and `0.05 … 4.0` mean the same thing
//! they meant before.

use std::f64::consts::PI;

use gx_app::settings::{preset, Settings, PRESET_NAMES};
use gx_core::render::Theme;

/// The numbers, the settings keys, the themes and the events of the contract.
const CONTRACT: &str = include_str!("../frontend/src/lib/contract.js");
/// The code that reads the settings payload and publishes the grid.
const GRID: &str = include_str!("../frontend/src/lib/grid.js");
/// The presets the panel offers.
const PRESETS: &str = include_str!("../frontend/src/lib/presets.js");
/// The controls of the settings panel and the ranges they offer.
const PANEL: &str = include_str!("../frontend/src/lib/SettingsPanel.svelte");
/// The Rust half of the wire: the event names the window listens to.
const RUST_APP: &str = include_str!("../src/app.rs");

/// The numeric value of one `export const NAME = <number>;` of the contract.
fn number(name: &str) -> f64 {
    let marker = format!("export const {name} = ");
    let start = CONTRACT
        .find(&marker)
        .unwrap_or_else(|| panic!("contract.js must export `{name}`"));
    let rest = &CONTRACT[start + marker.len()..];
    let literal = rest
        .split(';')
        .next()
        .expect("the declaration ends with `;`")
        .trim();
    literal.parse().unwrap_or_else(|error| {
        panic!("`{name}` of contract.js must be a number, found `{literal}`: {error}")
    })
}

/// The entries of one `export const NAME = [ … ];` of the contract.
fn list(name: &str) -> Vec<String> {
    let marker = format!("export const {name} = [");
    let start = CONTRACT
        .find(&marker)
        .unwrap_or_else(|| panic!("contract.js must export the list `{name}`"));
    let rest = &CONTRACT[start + marker.len()..];
    let body = rest.split("];").next().expect("the list is closed");
    body.split(',')
        .map(|item| item.trim().trim_matches('"').to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

/// The values of the `EVENTS` map of the contract, in file order.
fn events() -> Vec<String> {
    let start = CONTRACT
        .find("export const EVENTS = {")
        .expect("contract.js must name the events");
    let body = CONTRACT[start..]
        .split("};")
        .next()
        .expect("the event map is closed");
    // The keys are bare identifiers and the values are the only strings.
    body.split('"')
        .enumerate()
        .filter(|(index, _)| index % 2 == 1)
        .map(|(_, value)| value.to_string())
        .collect()
}

/// The keys of the object Rust sends the page.
fn rust_keys() -> Vec<String> {
    let value = serde_json::to_value(Settings::default()).expect("the settings serialise");
    let mut keys: Vec<String> = value
        .as_object()
        .expect("the settings are an object")
        .keys()
        .cloned()
        .collect();
    keys.sort();
    keys
}

/// The keys the Svelte page reads from the object Rust sends, as named by
/// `settingsFromPayload` — the only place the page reads it.
fn page_read_keys() -> Vec<String> {
    let marker = "export function settingsFromPayload(payload) {";
    let start = GRID
        .find(marker)
        .expect("grid.js must read the settings payload");
    let rest = &GRID[start + marker.len()..];
    let body = rest.split("\n}").next().expect("the reader is closed");
    let mut keys: Vec<String> = body
        .match_indices("p.")
        .map(|(at, _)| {
            body[at + 2..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect::<String>()
        })
        .filter(|key| !key.is_empty())
        .collect();
    keys.sort();
    keys.dedup();
    keys
}

/// The object of one control of the settings panel.
fn field(key: &str) -> &'static str {
    let marker = format!("key: \"{key}\"");
    let start = PANEL
        .find(&marker)
        .unwrap_or_else(|| panic!("the settings panel must offer `{key}`"));
    let rest = &PANEL[start..];
    let end = rest.find("},").expect("the control is closed") + 1;
    &rest[..end]
}

/// `PLAN_PRESETS` of `presets.js` as JSON. The table is a JavaScript object
/// literal with the keys of the presets bare (except `sin grilla`), so the
/// test quotes them instead of asking a dependency to parse JavaScript.
fn presets_json() -> serde_json::Value {
    let marker = "export const PLAN_PRESETS = {";
    let start = PRESETS
        .find(marker)
        .expect("presets.js must hold the preset table");
    let body = PRESETS[start + marker.len()..]
        .split("\n};")
        .next()
        .expect("the preset table is closed");
    let quoted = |raw: &str| format!("\"{}\"", raw.trim().trim_matches('"'));
    let mut entries: Vec<String> = Vec::new();
    let mut current = String::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        if let Some(name) = line.strip_suffix(": {") {
            current.push_str(&format!("{}: {{", quoted(name)));
            continue;
        }
        if line == "}," || line == "}" {
            // The last member of the object carries a comma that JSON does not
            // allow before the closing brace.
            if current.ends_with(',') {
                current.pop();
            }
            current.push('}');
            entries.push(std::mem::take(&mut current));
            continue;
        }
        let (key, value) = line
            .split_once(':')
            .unwrap_or_else(|| panic!("`{line}` of the preset table is a key and a value"));
        current.push_str(&format!(
            "{}: {},",
            quoted(key),
            value.trim().trim_end_matches(',')
        ));
    }
    let json = format!("{{{}}}", entries.join(","));
    serde_json::from_str(&json)
        .unwrap_or_else(|error| panic!("the preset table of presets.js is JSON: {error}\n{json}"))
}

#[test]
fn the_contract_file_pins_every_number_of_the_plan() {
    // RF24: the grid the PRD fixes (the values `tests/grid.rs` checks the
    // stylesheet against).
    assert_eq!(
        number("GRID_DIAGONAL_PLAN"),
        59.0,
        "the diagonal of the cell"
    );
    assert_eq!(
        number("GRID_ANGLE_DEGREES"),
        45.0,
        "the deviation of the lines"
    );
    assert_eq!(
        number("GRID_HIDE_ZOOM"),
        0.3,
        "below this zoom the grid fades"
    );
    // RF25, PRD §7.2: the zoom the contract offers and the step of the wheel.
    assert_eq!(number("ZOOM_MIN"), 0.05, "the floor of the zoom");
    assert_eq!(number("ZOOM_MAX"), 4.0, "the ceiling of the zoom");
    assert_eq!(number("ZOOM_STEP"), 1.1, "one step of the wheel");
    // The throttles and the pan of PRD §7.2.
    assert_eq!(number("CAMERA_THROTTLE_MS"), 100.0, "the camera throttle");
    assert_eq!(
        number("SETTINGS_THROTTLE_MS"),
        100.0,
        "the settings throttle"
    );
    assert_eq!(number("PAN_MS"), 250.0, "the animated pan");
    // PRD §7.2 and RNF1-ter: the padding and the culling.
    assert_eq!(number("OBJECT_PADDING"), 24.0, "the padding of an object");
    assert_eq!(number("MAX_LIVE_SVGS"), 3.0, "the ceiling of live SVGs");
    assert_eq!(number("CULL_ZOOM"), 0.4, "the zoom of the degradation");
}

#[test]
fn the_perpendicular_step_of_the_grid_follows_the_major_diagonal() {
    let diagonal = number("GRID_DIAGONAL_PLAN");
    let angle = number("GRID_ANGLE_DEGREES");
    let step = diagonal * (angle * PI / 180.0).sin();
    assert!(
        (step - 41.72).abs() < 0.01,
        "the perpendicular step is {step:.3}, the contract says 41.72"
    );
}

#[test]
fn the_numbers_agree_with_the_rust_side_where_rust_keeps_them() {
    assert_eq!(
        number("OBJECT_PADDING"),
        gx_app::camera::OBJECT_PADDING,
        "the padding of an object is the same on both sides (camera.rs)"
    );
    let (floor, ceiling) = (number("ZOOM_MIN"), number("ZOOM_MAX"));
    assert!(
        floor <= gx_app::camera::FIT_MIN_ZOOM && gx_app::camera::FIT_MIN_ZOOM <= ceiling,
        "the fit of the camera lives inside the zoom range of the contract"
    );
    assert!(floor < ceiling, "the range of the zoom is not empty");
}

#[test]
fn the_panel_offers_the_ranges_rust_accepts() {
    let opacity = field("grid_opacity");
    assert!(
        opacity.contains("min: 0,")
            && opacity.contains("max: 1,")
            && opacity.contains("step: 0.01,"),
        "the opacity is a range 0..1 with a step of 0.01: {opacity}"
    );
    let diagonal = field("grid_diagonal");
    assert!(
        diagonal.contains("min: 20,") && diagonal.contains("max: 200,"),
        "the separation is a range 20..200: {diagonal}"
    );
    let angle = field("grid_angle_degrees");
    assert!(
        angle.contains("min: 15,") && angle.contains("max: 75,"),
        "the angle is a range 15..75: {angle}"
    );

    // The same ends `Settings::sanitized` clamps to, so a control can never
    // offer a value the file would quietly pull back.
    let clamp = |settings: Settings| settings.sanitized();
    assert_eq!(
        clamp(Settings {
            grid_opacity: -1.0,
            ..Settings::default()
        })
        .grid_opacity,
        0.0
    );
    assert_eq!(
        clamp(Settings {
            grid_opacity: 5.0,
            ..Settings::default()
        })
        .grid_opacity,
        1.0
    );
    assert_eq!(
        clamp(Settings {
            grid_diagonal: 1.0,
            ..Settings::default()
        })
        .grid_diagonal,
        20.0
    );
    assert_eq!(
        clamp(Settings {
            grid_diagonal: 500.0,
            ..Settings::default()
        })
        .grid_diagonal,
        200.0
    );
    assert_eq!(
        clamp(Settings {
            grid_angle_degrees: 1.0,
            ..Settings::default()
        })
        .grid_angle_degrees,
        15.0
    );
    assert_eq!(
        clamp(Settings {
            grid_angle_degrees: 100.0,
            ..Settings::default()
        })
        .grid_angle_degrees,
        75.0
    );
}

#[test]
fn the_settings_keys_are_the_ones_rust_serialises() {
    let mut contract = list("SETTINGS_KEYS");
    contract.sort();
    assert_eq!(
        contract,
        rust_keys(),
        "contract.js lists exactly the keys of `Settings`"
    );
    assert_eq!(
        page_read_keys(),
        rust_keys(),
        "the Svelte page reads exactly the settings of `~/.graphium/settings.json`"
    );
}

#[test]
fn the_themes_of_the_contract_are_the_enum_of_rust() {
    assert_eq!(
        list("THEMES"),
        Theme::ALL.map(|theme| theme.as_str().to_string()).to_vec(),
        "the seven themes of the stylesheet are the seven of `Theme`"
    );
}

#[test]
fn every_event_of_the_contract_is_an_event_rust_knows() {
    let events = events();
    assert_eq!(
        events.len(),
        18,
        "the contract names every event of CONTRACT.md §3: {events:?}"
    );
    // `graphium://pending` has no emitter in this tree (CONTRACT.md §5.7), so
    // it is the only name `app.rs` does not carry.
    let orphans: Vec<&String> = events
        .iter()
        .filter(|name| name.as_str() != "graphium://pending")
        .filter(|name| !RUST_APP.contains(name.as_str()))
        .collect();
    assert!(
        orphans.is_empty(),
        "these events of the contract are not named by `app.rs`: {orphans:?}"
    );
}

#[test]
fn the_presets_of_the_page_are_the_presets_of_rust() {
    let table = presets_json();
    let object = table.as_object().expect("a table of presets");
    assert_eq!(
        object.len(),
        PRESET_NAMES.len(),
        "presets.js offers every preset of Rust: {:?}",
        object.keys().collect::<Vec<_>>()
    );

    for name in PRESET_NAMES {
        let expected = preset(name).unwrap_or_else(|| panic!("Rust must know the preset {name}"));
        let from_page = object
            .get(name)
            .unwrap_or_else(|| panic!("presets.js must offer the preset {name}"));
        let as_settings: Settings = serde_json::from_value(from_page.clone())
            .unwrap_or_else(|error| panic!("the preset {name} is a settings object: {error}"));
        assert_eq!(
            as_settings, expected,
            "the preset {name} of presets.js drifted from `settings::presets`"
        );
    }
}
