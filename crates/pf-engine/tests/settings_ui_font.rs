//! The `uiFont` preference round-trips through the engine and is not an undo step.

use pf_engine::Engine;
use pf_engine::commands::settings;
use tempfile::TempDir;

#[test]
fn ui_font_setting_round_trips_without_undo() {
    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("pf.sqlite");
    let mut e = Engine::open(&path).expect("open");
    assert_eq!(settings::ui_font(&e), None);

    settings::set_ui_font(&mut e, "arial").expect("set");
    assert_eq!(settings::ui_font(&e), Some("arial"));
    assert!(!e.can_undo());

    drop(e);
    let reopened = Engine::open(&path).expect("reopen");
    assert_eq!(settings::ui_font(&reopened), Some("arial"));
    assert!(!reopened.can_undo());
}
