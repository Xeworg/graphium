//! The settings panel of the plan (PRD §7.1 RF25, §8.5.2 RF22, S19).
//!
//! No test can click a button inside the webview, so what is pinned here is
//! the contract the page keeps with Rust: the controls the panel offers, the
//! event it listens to and the one it emits, the keys it reads (exactly the
//! ones `Settings` serialises), the geometry of the grid (which the human can
//! now change) and the palette of the light theme (which has to keep the
//! contrast of a real theme, S18).

use gx_app::settings::{preset, Settings};

const HTML: &str = include_str!("../frontend/index.html");
const CSS: &str = include_str!("../frontend/style.css");
const JS: &str = include_str!("../frontend/plan.js");

/// The text between two markers of a file.
fn slice_between<'a>(text: &'a str, start: &str, end: &str) -> &'a str {
    let from = text
        .find(start)
        .unwrap_or_else(|| panic!("the file must contain {start:?}"));
    let rest = &text[from + start.len()..];
    let to = rest
        .find(end)
        .unwrap_or_else(|| panic!("{start:?} must be closed by {end:?}"));
    &rest[..to]
}

/// The whole tag that carries `id="{id}"`.
fn tag_of<'a>(html: &'a str, id: &str) -> &'a str {
    let at = html
        .find(&format!("id=\"{id}\""))
        .unwrap_or_else(|| panic!("the page must have an element with id `{id}`"));
    let start = html[..at].rfind('<').expect("the id lives inside a tag");
    let end = at + html[at..].find('>').expect("the tag is closed") + 1;
    &html[start..end]
}

/// The value of one declaration of a CSS block.
fn declaration<'a>(block: &'a str, token: &str) -> &'a str {
    let at = block
        .find(&format!("{token}:"))
        .unwrap_or_else(|| panic!("{token} must be declared: {block}"));
    let rest = &block[at + token.len() + 1..];
    rest[..rest.find(';').unwrap_or(rest.len())].trim()
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

/// The keys the page reads from the object Rust sends.
fn page_payload_keys() -> Vec<String> {
    // `settingsFromPayload` is the only place the page reads the object, so
    // the fields it names there are the contract.
    let body = slice_between(JS, "function settingsFromPayload(payload) {", "\n}");
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

/// The keys of the object the page sends back to Rust.
fn page_sent_keys() -> Vec<String> {
    let body = slice_between(JS, "const SETTINGS_KEYS = [", "];");
    let mut keys: Vec<String> = body
        .split(',')
        .map(|item| item.trim().trim_matches('"').to_string())
        .filter(|item| !item.is_empty())
        .collect();
    keys.sort();
    keys
}

/// The three presets the page offers, as the page writes them.
fn page_presets() -> serde_json::Value {
    let table = slice_between(JS, "const PLAN_PRESETS = ", ";\n");
    serde_json::from_str(table.trim()).expect("the preset table of the page is JSON")
}

#[test]
fn the_panel_has_a_control_for_every_setting() {
    // The six settings of RF25 plus the theme of RF22.
    for id in [
        "settings-background",
        "settings-grid-color",
        "settings-grid-opacity",
        "settings-grid-diagonal",
        "settings-grid-angle",
        "settings-grid-visible",
    ] {
        assert!(
            HTML.contains(&format!("id=\"{id}\"")),
            "the panel must have the control `{id}`"
        );
        assert!(
            HTML.contains(&format!("for=\"{id}\"")),
            "the control `{id}` must be labelled: a panel that cannot be read is not usable"
        );
    }

    let background = tag_of(HTML, "settings-background");
    assert!(
        background.contains("type=\"color\""),
        "the background is a colour: {background}"
    );
    let grid_color = tag_of(HTML, "settings-grid-color");
    assert!(
        grid_color.contains("type=\"color\""),
        "the grid colour is a colour: {grid_color}"
    );

    let opacity = tag_of(HTML, "settings-grid-opacity");
    for (attribute, value) in [
        ("type", "range"),
        ("min", "0"),
        ("max", "1"),
        ("step", "0.01"),
    ] {
        assert!(
            opacity.contains(&format!("{attribute}=\"{value}\"")),
            "the opacity is a range 0..1 with a step of 0.01: {opacity}"
        );
    }

    let diagonal = tag_of(HTML, "settings-grid-diagonal");
    for (attribute, value) in [("type", "range"), ("min", "20"), ("max", "200")] {
        assert!(
            diagonal.contains(&format!("{attribute}=\"{value}\"")),
            "the separation is a range 20..200, the range Rust accepts: {diagonal}"
        );
    }

    let angle = tag_of(HTML, "settings-grid-angle");
    for (attribute, value) in [("type", "range"), ("min", "15"), ("max", "75")] {
        assert!(
            angle.contains(&format!("{attribute}=\"{value}\"")),
            "the angle is a range 15..75, the range Rust accepts: {angle}"
        );
    }

    let visible = tag_of(HTML, "settings-grid-visible");
    assert!(
        visible.contains("type=\"checkbox\""),
        "showing and hiding the grid is a checkbox: {visible}"
    );
}

#[test]
fn the_theme_switch_offers_the_two_themes_of_the_settings() {
    let theme = slice_between(HTML, "name=\"settings-theme\"", "</fieldset>");
    for value in ["value=\"dark\"", "value=\"light\""] {
        assert!(
            theme.contains(value),
            "the theme switch offers {value}: {theme}"
        );
    }
    // The page paints the theme it is given, and the stylesheet answers to
    // the same names the settings file uses.
    assert!(
        JS.contains("dataset.theme"),
        "the page must set `data-theme` on the root element"
    );
    assert!(
        CSS.contains("[data-theme=\"light\"]"),
        "the light theme is `light`, the name the settings use"
    );
    assert!(
        !CSS.contains("[data-theme=\"claro\"]"),
        "one name per theme: `claro` is the preset, `light` is the theme"
    );
}

#[test]
fn the_three_presets_of_the_page_are_the_presets_of_rust() {
    let presets = page_presets();
    let table = presets.as_object().expect("a table of presets");
    assert_eq!(table.len(), 3, "F1 offers exactly three presets: {table:?}");

    for name in ["plano", "claro", "sin grilla"] {
        let expected = preset(name).unwrap_or_else(|| panic!("Rust must know the preset {name}"));
        let from_page = table
            .get(name)
            .unwrap_or_else(|| panic!("the page must offer the preset {name}"));
        let object = from_page
            .as_object()
            .unwrap_or_else(|| panic!("the preset {name} is an object"));
        assert_eq!(
            object.len(),
            rust_keys().len(),
            "the preset {name} carries every setting: {object:?}"
        );
        let as_settings: Settings = serde_json::from_value(from_page.clone())
            .unwrap_or_else(|error| panic!("the preset {name} is a settings object: {error}"));
        assert_eq!(
            as_settings, expected,
            "the preset {name} of the page drifted from `settings::presets`"
        );
    }
}

#[test]
fn the_page_listens_to_the_settings_and_announces_every_change() {
    assert!(
        JS.contains("listen(\"graphium://settings\""),
        "the page has to apply the settings Rust emits"
    );
    assert!(
        JS.contains("emit(\"settings-changed\""),
        "the page has to tell Rust when a control changes"
    );
    assert!(
        JS.contains("const SETTINGS_THROTTLE_MS = 100;"),
        "colour pickers and ranges are throttled to 100 ms like the camera: \
         dragging must not write sixty files"
    );
    // The page never writes the settings file and never decides a default:
    // it shows what Rust sends back.
    for forbidden in ["fetch(", "writeFile", "localStorage", "sessionStorage"] {
        assert!(
            !JS.contains(forbidden),
            "the page must not keep or write settings itself, found {forbidden}"
        );
    }
}

#[test]
fn the_keys_the_page_reads_are_the_ones_rust_serialises() {
    let expected = rust_keys();
    assert_eq!(
        page_payload_keys(),
        expected,
        "the panel reads exactly the settings of `~/.graphium/settings.json`"
    );
    assert_eq!(
        page_sent_keys(),
        expected,
        "the panel sends the whole settings object back"
    );
}

#[test]
fn the_grid_follows_the_settings_and_still_scales_with_the_zoom() {
    let body = slice_between(JS, "function applyGrid() {", "\n}");
    assert!(
        body.contains("grid.grid_diagonal * sine * camera.zoom"),
        "the step is the separation times the sine of the angle times the \
         zoom (RF24, RF25): {body}"
    );
    assert!(
        body.contains("--grid-diagonal"),
        "the separation of the cell comes from the settings: {body}"
    );
    assert!(
        body.contains("--grid-angle"),
        "the angle of the lines comes from the settings: {body}"
    );
    assert!(
        body.contains("grid.grid_visible && camera.zoom >= GRID_HIDE_ZOOM"),
        "a hidden grid is painted with opacity 0: {body}"
    );
    assert!(
        body.contains("--grid-opacity"),
        "the opacity of the grid comes from the settings: {body}"
    );
    assert!(
        !body.contains("GRID_DIAGONAL_PLAN * sine"),
        "the geometry comes from the settings, not from the constant: {body}"
    );
    // The rule that cost four corrections: the tile of the background is one
    // cell of the mesh, scaled with the camera.
    // The tile is rectangular because the period along the rows depends on
    // the angle (see `grid.rs`): the square only closed at 45 degrees.
    let flat: String = CSS.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flat.contains(
            "calc(var(--grid-diagonal) * var(--grid-tan) * var(--zoom)) \
             calc(var(--grid-diagonal) * var(--zoom))"
        ),
        "the tile of the grid must be one cell of the mesh, scaled with the camera"
    );
    assert!(
        CSS.contains("--grid-line: 1px"),
        "the thickness stays one screen pixel whatever the zoom is"
    );
}

#[test]
fn the_light_theme_is_a_real_theme_for_the_drawings() {
    let light = slice_between(CSS, "[data-theme=\"light\"] {", "}");
    for token in [
        "--bg",
        "--node-fill",
        "--node-stroke",
        "--node-text",
        "--edge-stroke",
        "--edge-text",
    ] {
        assert!(
            light.contains(&format!("{token}:")),
            "the light theme must declare {token}, or the drawing loses its \
             contrast (S18): {light}"
        );
    }

    let dark = slice_between(CSS, ":root {", "}");
    for token in [
        "--node-fill",
        "--node-stroke",
        "--node-text",
        "--edge-stroke",
        "--edge-text",
    ] {
        assert_ne!(
            declaration(dark, token),
            declaration(light, token),
            "the light theme is a theme, not a white background: {token} is the same in both"
        );
    }

    // Both themes drive the same variables, so the classes of the SVG keep
    // working in either one (the CSP forbids the <style> of the SVG).
    for (class, property, token) in [
        (".graphium .node-fill", "fill", "--node-fill"),
        (".graphium .node-stroke", "stroke", "--node-stroke"),
        (".graphium .node-text", "fill", "--node-text"),
        (".graphium .edge-stroke", "stroke", "--edge-stroke"),
        (".graphium .edge-text", "fill", "--edge-text"),
    ] {
        let rule = slice_between(CSS, &format!("{class} {{"), "}");
        assert!(
            rule.contains(&format!("{property}: var({token})")),
            "{class} must paint {property}: var({token}): {rule}"
        );
    }
}

#[test]
fn the_panel_lives_outside_the_plan_so_it_never_steals_the_camera() {
    // The gear and the panel are declared after the plan and after the empty
    // state: they are not children of `#plan`, so the wheel and the drag of
    // the camera never see them, even while the panel is closed.
    let empty_state = HTML
        .find("id=\"empty\"")
        .expect("the empty state of the plan");
    let gear = HTML
        .find("id=\"settings-toggle\"")
        .expect("the gear of the settings");
    assert!(
        gear > empty_state,
        "the gear and the panel must be siblings of the plan, not children of it"
    );

    let toggle = tag_of(HTML, "settings-toggle");
    assert!(
        toggle.contains("aria-label=") && toggle.contains("aria-expanded="),
        "the gear is a button a keyboard can reach and a reader can name: {toggle}"
    );
    let close = tag_of(HTML, "settings-close");
    assert!(
        close.contains("aria-label="),
        "the panel can be closed without a mouse: {close}"
    );
    let panel = tag_of(HTML, "settings-panel");
    assert!(
        panel.contains("class=\"settings-panel hidden\""),
        "the panel starts closed, out of the tab order and out of the way: {panel}"
    );
    assert!(
        panel.contains("aria-label="),
        "the panel is named for a screen reader: {panel}"
    );

    let gear_css = slice_between(CSS, ".settings-toggle {", "}");
    assert!(
        gear_css.contains("position: fixed"),
        "the gear floats over the plan: {gear_css}"
    );
    for side in ["top:", "right:"] {
        assert!(
            gear_css.contains(side),
            "the gear sits in the top-right corner of the plan: {gear_css}"
        );
    }
    let panel_css = slice_between(CSS, ".settings-panel {", "}");
    assert!(
        panel_css.contains("position: fixed") && panel_css.contains("top:"),
        "the panel floats over the plan: {panel_css}"
    );

    // Escape closes the panel, so the keyboard has a way out.
    assert!(
        JS.contains("\"Escape\""),
        "the panel must close with Escape"
    );
}
