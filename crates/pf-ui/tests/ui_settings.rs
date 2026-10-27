//! L5 UI tests for the Settings screen font row (T21 preferences). Headless (egui_kittest) on a
//! temp database; assertions are on the engine's stored `uiFont` setting.

mod common;

use egui::accesskit::Role;
use egui_kittest::kittest::Queryable;
use pf_engine::commands::settings;
use pf_ui::PfApp;
use pf_ui::screen::Screen;

use common::{empty_engine, harness};

#[test]
/// Choosing a font in the Settings combo saves `uiFont` through the engine.
fn settings_font_combo_saves_choice() {
    let mut app = PfApp::new_for_test(empty_engine());
    app.set_screen(Screen::Settings);
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_role_and_label(Role::ComboBox, "Font").click();
        h.run();
        h.get_by_label("Arial").click();
        h.run();
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    assert_eq!(settings::ui_font(&e), Some("arial"));
}

#[test]
/// A saved choice is shown in the combo when Settings opens.
fn settings_font_combo_shows_saved_choice() {
    let mut engine = empty_engine();
    settings::set_ui_font(&mut engine, "verdana").expect("set");
    let mut app = PfApp::new_for_test(engine);
    app.set_screen(Screen::Settings);
    let mut h = harness(&mut app);
    h.run();
    let combo = h.get_by_role_and_label(Role::ComboBox, "Font");
    assert_eq!(combo.value().as_deref(), Some("Verdana"));
}
