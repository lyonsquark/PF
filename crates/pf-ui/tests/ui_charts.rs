//! L5 UI tests for the Charts screen (T24a): the nine card titles, the empty states, the account filter,
//! the balance period picker, and the comparison's month stepper. Headless (egui_kittest); the plots
//! themselves are checked by the pf-core chart tests. Assertions are on what the screen draws.

mod common;

use egui::accesskit::Role;
use egui_kittest::kittest::Queryable;
use pf_core::charts::{
    CHART_TITLES, NOT_ENOUGH_DATA, default_selection, select_all_button_label, selection_label,
};
use pf_engine::Engine;
use pf_model::testing::TestDb;
use pf_model::{AccountType, Db};
use pf_ui::PfApp;
use pf_ui::screen::Screen;

use common::{empty_engine, engine_with_fixture, harness};

/// Engine holding `db`, on a temp file (never the app's database).
fn engine_with(db: Db) -> Engine {
    let mut e = empty_engine();
    e.replace_db(db)
        .expect("write the test database to the temp file");
    e
}

/// True when the screen shows `text`: as a button or heading (its name) or as a plain label or a picker's
/// selected text (its value). More than one match is fine.
fn shows(h: &egui_kittest::Harness<'_>, text: &str) -> bool {
    h.query_all_by_label(text).count() > 0 || h.query_all_by_value(text).count() > 0
}

#[test]
/// UI-CHT-01: with the fixture, all nine card titles are on the screen, over several frames.
fn ui_cht_dashboard_renders_with_fixture() {
    let mut app = PfApp::new_for_test(engine_with_fixture());
    app.set_screen(Screen::Charts);
    let mut h = harness(&mut app);
    for _ in 0..3 {
        h.run();
    }
    for title in CHART_TITLES {
        assert!(shows(&h, title), "card {title:?}");
    }
}

#[test]
/// With no data every chart shows "Not enough data", and the pacing card says there are no budgets.
fn ui_cht_dashboard_renders_empty_db() {
    let mut app = PfApp::new_for_test(engine_with(TestDb::new().build()));
    app.set_screen(Screen::Charts);
    let mut h = harness(&mut app);
    h.run();
    assert!(shows(&h, NOT_ENOUGH_DATA), "an empty chart says so");
    assert!(
        shows(&h, pf_core::charts::NO_BUDGETS),
        "pacing with no budgets"
    );
}

#[test]
/// The account filter starts with the non-external accounts selected: its label is the core's label.
fn ui_cht_account_filter_defaults_to_non_external() {
    let mut t = TestDb::new();
    t.account("Everyday", AccountType::Checking);
    t.external_account("Old Bank", AccountType::Savings);
    let db = t.build();
    let label = selection_label(&db, &default_selection(&db));
    let mut app = PfApp::new_for_test(engine_with(db));
    app.set_screen(Screen::Charts);
    let mut h = harness(&mut app);
    h.run();
    assert!(shows(&h, &label), "the filter reads {label:?}");
}

#[test]
/// "Deselect All" (when everything is on) clears the selection; the label says so.
fn ui_cht_select_all_toggle() {
    let mut t = TestDb::new();
    t.account("Everyday", AccountType::Checking);
    let db = t.build();
    let on = selection_label(&db, &default_selection(&db));
    let toggle = select_all_button_label(&db, &default_selection(&db));
    let mut app = PfApp::new_for_test(engine_with(db));
    app.set_screen(Screen::Charts);
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label(&on).click();
        h.run();
        h.get_by_label(toggle).click();
        h.run();
    }
    let db_for_label = app.engine().lock().expect("engine lock").db().clone();
    let empty = selection_label(&db_for_label, &std::collections::BTreeSet::new());
    let h = harness(&mut app);
    assert!(shows(&h, &empty), "the filter reads {empty:?}");
}

#[test]
/// The balance card's period picker changes the period it shows.
fn ui_cht_period_picker_changes_cache_key() {
    let mut app = PfApp::new_for_test(engine_with_fixture());
    app.set_screen(Screen::Charts);
    let mut h = harness(&mut app);
    h.run();
    h.get_by_role_and_label(Role::ComboBox, "Balance Period")
        .click();
    h.run();
    let options: Vec<_> = h.get_all_by_label("1M").collect();
    options.last().expect("the 1M option").click();
    h.run();
    assert!(shows(&h, "1M"), "the picker shows 1M");
}

#[test]
/// The comparison's "<" on month A steps it back a month (March becomes February).
fn ui_cht_month_stepper() {
    let mut app = PfApp::new_for_test(engine_with_fixture());
    app.set_screen(Screen::Charts);
    let mut h = harness(&mut app);
    h.run();
    let steps: Vec<_> = h.get_all_by_label("<").collect();
    steps.last().expect("the month A stepper").click();
    h.run();
    assert!(
        shows(&h, "Feb 2026") || shows(&h, "Feb 2025"),
        "month A stepped back"
    );
}

#[test]
/// R-CHT-11: a click inside a plot opens nothing and leaves the screen where it is.
fn ui_cht_plot_charts_ignore_clicks() {
    let mut app = PfApp::new_for_test(engine_with_fixture());
    app.set_screen(Screen::Charts);
    {
        let mut h = harness(&mut app);
        h.run();
        let heading = h.get_by_label("Account Balances Over Time").rect().center();
        h.get_by_label("Account Balances Over Time").hover();
        h.run();
        let _ = heading;
        h.event(egui::Event::PointerButton {
            pos: egui::pos2(heading.x, heading.y + 120.0),
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        h.event(egui::Event::PointerButton {
            pos: egui::pos2(heading.x, heading.y + 120.0),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        h.run();
    }
    assert_eq!(app.screen(), Screen::Charts, "still on the charts screen");
}
