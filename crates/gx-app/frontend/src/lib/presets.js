// The presets of the settings panel, copied verbatim from the vanilla page
// (`crates/gx-app/frontend/plan.js:58-214`, `PLAN_PRESETS`, `THEME_PRESETS`
// and `fillSwatches`).
//
// Rust owns the presets (`crates/gx-app/src/settings.rs`, `settings::presets`)
// and `crates/gx-app/tests/settings_panel.rs` keeps both sides equal. This
// table does **not** decide anything: it fills the chips of the theme fieldset
// with the four colours each theme paints, so the dots beside a theme name can
// never drift from the preset of that theme.
//
// `sin grilla` is a preset and not a look: it is the grid of `plano` turned
// off, so it has no chip. The chips are `THEME_CHIPS`, in panel order.

import { THEMES } from "./contract.js";

/** The eight presets of RF25 / `f6-themes.md`, key by key. */
export const PLAN_PRESETS = {
  plano: {
    accent: "#6ea8fe",
    background: "#1b1b1b",
    edge_stroke: "#8a8a96",
    edge_text: "#b9b9c2",
    grid_angle_degrees: 45,
    grid_color: "#ffffff",
    grid_diagonal: 59,
    grid_opacity: 0.14,
    grid_visible: true,
    node_fill: "#2a2a30",
    node_stroke: "#4d4d57",
    node_text: "#e9e9ec",
    theme: "dark",
  },
  claro: {
    accent: "#2f6fd0",
    background: "#f7f7f8",
    edge_stroke: "#6f6f7a",
    edge_text: "#4a4a54",
    grid_angle_degrees: 45,
    grid_color: "#000000",
    grid_diagonal: 59,
    grid_opacity: 0.1,
    grid_visible: true,
    node_fill: "#ffffff",
    node_stroke: "#c7c7cf",
    node_text: "#1b1b1f",
    theme: "light",
  },
  "sin grilla": {
    accent: "#6ea8fe",
    background: "#1b1b1b",
    edge_stroke: "#8a8a96",
    edge_text: "#b9b9c2",
    grid_angle_degrees: 45,
    grid_color: "#ffffff",
    grid_diagonal: 59,
    grid_opacity: 0.14,
    grid_visible: false,
    node_fill: "#2a2a30",
    node_stroke: "#4d4d57",
    node_text: "#e9e9ec",
    theme: "dark",
  },
  brasa: {
    accent: "#f5a623",
    background: "#131316",
    edge_stroke: "#8f867c",
    edge_text: "#cbbcab",
    grid_angle_degrees: 45,
    grid_color: "#ffffff",
    grid_diagonal: 59,
    grid_opacity: 0.16,
    grid_visible: true,
    node_fill: "#1e1e24",
    node_stroke: "#3d3d47",
    node_text: "#f2e9e1",
    theme: "brasa",
  },
  arcilla: {
    accent: "#c05a3a",
    background: "#faf6f0",
    edge_stroke: "#8a7a68",
    edge_text: "#5c4d40",
    grid_angle_degrees: 45,
    grid_color: "#000000",
    grid_diagonal: 59,
    grid_opacity: 0.1,
    grid_visible: true,
    node_fill: "#fffdf9",
    node_stroke: "#d9cbb7",
    node_text: "#3a2f27",
    theme: "arcilla",
  },
  aurora: {
    accent: "#9a8cf0",
    background: "#0e1626",
    edge_stroke: "#7288b0",
    edge_text: "#a9bcd9",
    grid_angle_degrees: 45,
    grid_color: "#c8dcff",
    grid_diagonal: 59,
    grid_opacity: 0.13,
    grid_visible: true,
    node_fill: "#16213a",
    node_stroke: "#2c3d5e",
    node_text: "#e6ecf7",
    theme: "aurora",
  },
  neblina: {
    accent: "#2f8f86",
    background: "#eef1f2",
    edge_stroke: "#6f8080",
    edge_text: "#4c5c5d",
    grid_angle_degrees: 45,
    grid_color: "#000000",
    grid_diagonal: 59,
    grid_opacity: 0.1,
    grid_visible: true,
    node_fill: "#fafbfb",
    node_stroke: "#cbd3d5",
    node_text: "#22302f",
    theme: "neblina",
  },
  pitaya: {
    accent: "#e06a8d",
    background: "#100e13",
    edge_stroke: "#94707f",
    edge_text: "#c99fb0",
    grid_angle_degrees: 45,
    grid_color: "#ffffff",
    grid_diagonal: 59,
    grid_opacity: 0.13,
    grid_visible: true,
    node_fill: "#1b161c",
    node_stroke: "#4d3a45",
    node_text: "#f0dce4",
    theme: "pitaya",
  },
};

/** One name per look: the chip of a theme shows the palette of its preset. */
export const THEME_PRESETS = {
  dark: "plano",
  light: "claro",
  brasa: "brasa",
  arcilla: "arcilla",
  aurora: "aurora",
  neblina: "neblina",
  pitaya: "pitaya",
};

/** The label the panel gives each theme (`index.html:211-248`). */
const THEME_LABELS = {
  dark: "plano",
  light: "claro",
  brasa: "brasa",
  arcilla: "arcilla",
  aurora: "aurora",
  neblina: "neblina",
  pitaya: "pitaya",
};

/** The four colours of a theme that the dots beside its name show. */
export const SWATCH_KEYS = ["background", "node_fill", "node_text", "accent"];

/**
 * The chips of the theme fieldset, in the order the vanilla page lists them:
 * the seven themes of `THEMES`, each with the preset it paints and its four
 * dots. The page never invents a colour: they all come from `PLAN_PRESETS`.
 */
export const THEME_CHIPS = THEMES.map((value) => ({
  value,
  label: THEME_LABELS[value],
  swatches: SWATCH_KEYS.map(
    (key) => PLAN_PRESETS[THEME_PRESETS[value]][key],
  ),
}));
