//! The settings of the plan and its file (PRD §7.1 RF25, §8.5.2 RF22, S19).
//!
//! The human owns the look of the plan: the background, the grid and the
//! theme are their settings, never the agent's. The file lives in
//! `~/.graphium/settings.json` and a broken field falls back to its default
//! instead of taking the window down with it.

use std::fs;
use std::path::{Path, PathBuf};

use gx_app::settings::{preset, Settings};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("graphium-settings-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("the scratch directory is created");
    dir
}

#[test]
fn the_defaults_are_the_plano_preset() {
    let settings = Settings::default();
    assert_eq!(settings.background, "#1b1b1b");
    assert_eq!(settings.grid_color, "#ffffff");
    assert!((settings.grid_opacity - 0.14).abs() < 1e-9, "el PRD dice 14%");
    assert!((settings.grid_diagonal - 59.0).abs() < 1e-9);
    assert!((settings.grid_angle_degrees - 45.0).abs() < 1e-9);
    assert!(settings.grid_visible);
    assert_eq!(settings.theme, "dark");
}

#[test]
fn the_three_presets_of_the_prd_exist() {
    let plano = preset("plano").expect("preset plano");
    assert_eq!(plano, Settings::default());

    let claro = preset("claro").expect("preset claro");
    assert_eq!(claro.theme, "light", "claro es un tema, no un fondo blanco");

    let sin_grilla = preset("sin grilla").expect("preset sin grilla");
    assert!(!sin_grilla.grid_visible);
    assert_eq!(sin_grilla.theme, "dark");

    assert!(preset("nope").is_none());
}

#[test]
fn a_missing_file_is_the_default() {
    let dir = temp_dir("missing");
    assert_eq!(
        Settings::load_from(dir.join("settings.json")),
        Settings::default()
    );
}

#[test]
fn what_is_saved_is_loaded() {
    let dir = temp_dir("round-trip");
    let path = dir.join("settings.json");
    let settings = Settings {
        background: "#101820".to_string(),
        grid_color: "#7fd1ff".to_string(),
        grid_opacity: 0.3,
        grid_diagonal: 64.0,
        grid_angle_degrees: 30.0,
        grid_visible: false,
        theme: "light".to_string(),
    };

    settings.save_to(&path).expect("the settings are written");
    assert_eq!(Settings::load_from(&path), settings);
}

#[test]
fn a_broken_field_falls_back_to_its_default() {
    let dir = temp_dir("broken");
    let path = dir.join("settings.json");
    fs::write(
        &path,
        r##"{ "background": "azul", "grid_opacity": 4, "theme": "purple" }"##,
    )
    .expect("the file is written");

    let settings = Settings::load_from(&path);
    assert_eq!(settings.background, "#1b1b1b", "a colour is #rrggbb or nothing");
    assert_eq!(settings.grid_opacity, 1.0, "the opacity clamps to 1");
    assert_eq!(settings.theme, "dark", "the theme is dark or light");
    // The fields that were fine keep their value.
    assert_eq!(settings.grid_color, "#ffffff");
}

#[test]
fn a_file_that_is_not_json_is_the_default() {
    let dir = temp_dir("garbage");
    let path = dir.join("settings.json");
    fs::write(&path, "not json at all").expect("the file is written");
    assert_eq!(Settings::load_from(&path), Settings::default());
}

#[test]
fn unknown_keys_are_ignored() {
    let dir = temp_dir("unknown");
    let path = dir.join("settings.json");
    fs::write(&path, r##"{ "background": "#000000", "zoom": 3 }"##).expect("the file is written");
    assert_eq!(Settings::load_from(&path).background, "#000000");
}

#[test]
fn the_file_lives_under_the_graphium_directory() {
    let path = Settings::path().expect("the home directory is known in the tests");
    assert!(
        path.ends_with(".graphium/settings.json"),
        "the settings live in ~/.graphium/settings.json: {}",
        path.display()
    );
}

#[test]
fn saving_leaves_no_temporary_file_behind() {
    let dir = temp_dir("atomic");
    let path = dir.join("settings.json");

    Settings::default().save_to(&path).expect("written");

    let leftovers: Vec<_> = fs::read_dir(&dir)
        .expect("the directory is readable")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name != "settings.json")
        .collect();
    assert!(
        leftovers.is_empty(),
        "the write is atomic: nothing but settings.json may stay, found {leftovers:?}"
    );
}

#[test]
fn a_missing_directory_is_created() {
    let dir = temp_dir("nested");
    let path: &Path = &dir.join("graphium").join("settings.json");

    Settings::default().save_to(path).expect("written");

    assert!(path.exists());
}
