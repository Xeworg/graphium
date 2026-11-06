//! The history of versions in the window (RF14; feature doc
//! `f2-cierre.md`, T5).
//!
//! No test can click a button inside the webview, so what is pinned here is
//! the contract the page keeps with Rust: the panel item carries
//! «◀ v3/7 ▶», the page asks Rust to view a version
//! (`graphium://version-view` with `diagram_id` and `version`) and paints
//! the answer where the live drawing was (`graphium://version-paint`),
//! walking back to the newest version brings the live drawing back without
//! a render, ←/→ navigate the selected diagram, a re-render of the same id
//! replaces the object (and the view with it), and the page never restores
//! a version by itself — the whole view is read-only (L5).

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

/// The value of one declaration of a CSS block.
fn declaration<'a>(block: &'a str, token: &str) -> &'a str {
    let at = block
        .find(token)
        .unwrap_or_else(|| panic!("the block must declare {token:?}: {block}"));
    let rest = &block[at + token.len()..];
    let end = rest.find(';').expect("the declaration ends with `;`");
    rest[..end].trim()
}

/// The panel item carries the history when the diagram has one to show: the
/// nav appears only past the first version, so a diagram without history
/// keeps the item the previous cuts shipped.
#[test]
fn the_panel_item_carries_the_history_past_the_first_version() {
    let item = slice_between(JS, "function panelItem(diagram) {", "\n}");
    assert!(
        item.contains("diagram.version_count > 1"),
        "the nav exists only when there is a history to walk: {item}"
    );
    assert!(
        item.contains("item.append(versionNav(diagram))"),
        "the nav belongs to the item of the diagram: {item}"
    );
}

/// The indicator reads the position inside the versions the store keeps:
/// the numbers are absolute and consecutive (`oldest_version..=
/// current_version`), so the position is the subtraction, and a history at
/// its cap reads «v20/20» (PRD §11.1).
#[test]
fn the_indicator_names_the_position_between_the_saved_versions() {
    let nav = slice_between(JS, "function versionNav(diagram) {", "\n}");
    for token in [
        "diagram.oldest_version",
        "diagram.current_version",
        "diagram.version_count",
    ] {
        assert!(
            nav.contains(token),
            "the position comes from the state Rust publishes: {token}: {nav}"
        );
    }
    assert!(
        nav.contains("v${") && nav.contains("}/${diagram.version_count}"),
        "the label reads «v3/7»: {nav}"
    );
}

/// The page asks Rust; it never renders a version by itself and it never
/// writes the store (L5): the whole view is read-only.
#[test]
fn the_page_asks_rust_to_view_a_version_and_paints_the_answer() {
    let view = slice_between(JS, "function viewVersion(diagram, version) {", "\n}");
    assert!(
        view.contains("emit(\"graphium://version-view\""),
        "the view is a question to Rust: {view}"
    );
    assert!(
        view.contains("viewedVersion.set(diagram.diagram_id, target)"),
        "the page remembers which version it asked for: {view}"
    );
    assert!(
        JS.contains("listen(\"graphium://version-paint\""),
        "the answer of Rust paints the preview: {JS}"
    );
    // Nothing the history does writes the plan back: no close, no state
    // write, no placement — the preview borrows the drawing only.
    let paint = slice_between(JS, "listen(\"graphium://version-paint\"", "\n});");
    assert!(
        !paint.contains("emitClose"),
        "viewing never closes: {paint}"
    );
    assert!(
        !paint.contains("graphium://close"),
        "viewing never writes the plan: {paint}"
    );
}

/// Walking back to the newest version brings the live drawing back without
/// asking Rust again: the page holds it (`element.svg`), the indicator
/// reads the last position, and the preview leaves the plan.
#[test]
fn the_newest_version_brings_the_live_drawing_back() {
    let view = slice_between(JS, "function viewVersion(diagram, version) {", "\n}");
    assert!(
        view.contains("viewedVersion.delete(diagram.diagram_id)"),
        "the live drawing is not a view: {view}"
    );
    assert!(
        view.contains("restoreLive("),
        "the live drawing comes back from the page, not from a render: {view}"
    );
    let restore = slice_between(JS, "function restoreLive(element) {", "\n}");
    assert!(
        restore.contains("element.svg"),
        "the restore mounts the live drawing the object holds: {restore}"
    );
    assert!(
        restore.contains("element.previewSvg.remove()"),
        "the preview leaves the plan: {restore}"
    );
}

/// The preview paints where the live drawing was: the object keeps its
/// rectangle (the culling keeps testing the rectangle Rust placed) and the
/// plan never changes shape because of the history (L5).
#[test]
fn the_preview_takes_the_place_of_the_live_drawing() {
    let paint = slice_between(JS, "function paintPreview(element, svg) {", "\n}");
    assert!(
        paint.contains("element.rect.x + element.rect.w / 2"),
        "the preview centers on the rectangle of the object: {paint}"
    );
    assert!(
        !paint.contains("element.rect ="),
        "the preview never moves the object: {paint}"
    );
    assert!(
        paint.contains("element.content.prepend(svg)"),
        "the preview mounts where insertDiagram leaves the drawing: {paint}"
    );
    assert!(
        paint.contains("element.previewSvg = svg"),
        "the preview stays reachable for the restore: {paint}"
    );
}

/// The culling mounts back what the human was viewing: an object that left
/// the view while a preview was up comes back with the preview, not with
/// the live drawing under it.
#[test]
fn the_culling_mounts_the_viewed_drawing_back() {
    let materialize = slice_between(JS, "function materializeObject(element) {", "\n}");
    assert!(
        materialize.contains("element.previewSvg"),
        "the culling mounts the preview the human asked for: {materialize}"
    );
}

/// ←/→ walk the history of the selected diagram, and a field that holds
/// the keyboard keeps its arrows: the history never steals them.
#[test]
fn the_keyboard_walks_the_history_of_the_selected_diagram() {
    let keys = slice_between(
        JS,
        "window.addEventListener(\"keydown\", (event) => {",
        "\n});",
    );
    for token in ["ArrowLeft", "ArrowRight"] {
        assert!(
            keys.contains(token),
            "the arrows of the keyboard navigate: {token}: {keys}"
        );
    }
    assert!(
        keys.contains("input, textarea, select"),
        "a field that holds the keyboard keeps its arrows: {keys}"
    );
    assert!(
        keys.contains("selectedId"),
        "the keyboard walks the diagram the human selected: {keys}"
    );
    assert!(
        keys.contains("diagram.version_count > 1"),
        "a diagram without history ignores the arrows: {keys}"
    );
}

/// A re-render of the same id (`opts.replace_id`, `update_diagram`)
/// replaces the object on the plan: the old container leaves, and with it
/// the preview the human was viewing — the newest version is newer now.
#[test]
fn a_re_render_of_the_same_id_replaces_the_object_and_the_view() {
    let insert = slice_between(JS, "function insertDiagram(payload) {", "\n}");
    assert!(
        insert.contains("objects.get(payload.diagram_id)"),
        "the insert looks for the object it replaces: {insert}"
    );
    assert!(
        insert.contains("previous.remove()"),
        "the old container leaves the plan: {insert}"
    );
    assert!(
        insert.contains("viewedVersion.delete(payload.diagram_id)"),
        "a new render is the live one again: {insert}"
    );
}

/// The nav paints with the tokens of the theme, so it reads in every theme
/// the selector offers (PRD §8.5.2).
#[test]
fn the_history_nav_paints_with_the_tokens_of_the_theme() {
    let nav = slice_between(CSS, ".panel-item-versions {", "}");
    assert!(
        declaration(nav, "color").contains("var("),
        "the label paints with a token, not a literal: {nav}"
    );
    let step = slice_between(CSS, ".panel-item-version-step {", "}");
    assert!(
        declaration(step, "cursor").contains("pointer"),
        "the steps are clickable: {step}"
    );
    let disabled = slice_between(CSS, ".panel-item-version-step:disabled {", "}");
    assert!(
        !declaration(disabled, "opacity").is_empty(),
        "a step with nowhere to go reads as disabled: {disabled}"
    );
}

/// The HTML keeps its ids: the nav lives in the panel, the banner that
/// answers an impossible version is the one the plan already shows.
#[test]
fn the_answers_of_rust_arrive_where_the_human_reads_them() {
    assert!(
        HTML.contains("id=\"panel\""),
        "the nav lives in the panel of open diagrams"
    );
    let banner = slice_between(JS, "function showError(payload) {", "\n}");
    assert!(
        banner.contains("bannerMessage"),
        "an impossible version is answered in the banner: {banner}"
    );
}
