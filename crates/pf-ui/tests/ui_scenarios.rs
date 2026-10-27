//! L5 UI tests for the Scenarios screen (T19): toggling the active scenario, the checkmark, creating
//! and editing a scenario, and deleting the active one. Headless (egui_kittest); assertions are on
//! engine state and on what the screen draws.

mod common;

use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::kittest::{NodeT, Queryable};
use pf_engine::Engine;
use pf_model::testing::TestDb;
use pf_model::{AccountType, Db, TransactionStatus};
use pf_ui::PfApp;
use pf_ui::screen::Screen;

use common::{empty_engine, harness};

/// Engine holding `db`, on a temp file (never the app's database).
fn engine_with(db: Db) -> Engine {
    let mut e = empty_engine();
    e.replace_db(db)
        .expect("write the test database to the temp file");
    e
}

/// The "Plan A" row on the screen. The toolbar's scenario menu also says "Plan A"; the screen is
/// drawn after it, so its row is the last match.
fn scenario_row<'a>(h: &'a egui_kittest::Harness<'_>, name: &'a str) -> egui_kittest::Node<'a> {
    h.get_all_by_label(name).last().expect("the scenario row")
}

/// Scenario "Plan A" with one transaction in it, and a checking account.
fn plan_app() -> (PfApp, pf_model::ScenarioId) {
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let plan = t.scenario("Plan A");
    t.tx(chk, "2026-03-02", "-9", "Snack")
        .status(TransactionStatus::Future)
        .scenario(plan)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Scenarios);
    (app, plan)
}

#[test]
/// R-SCN-2: a click makes the scenario active; a second click returns to Reality.
fn ui_scn_click_row_toggles_active() {
    let (mut app, plan) = plan_app();
    {
        let mut h = harness(&mut app);
        h.run();
        scenario_row(&h, "Plan A").click();
        h.run();
    }
    assert_eq!(
        app.engine()
            .lock()
            .expect("engine lock")
            .state
            .active_scenario,
        Some(plan)
    );
    {
        let mut h = harness(&mut app);
        h.run();
        scenario_row(&h, "Plan A").click();
        h.run();
    }
    assert_eq!(
        app.engine()
            .lock()
            .expect("engine lock")
            .state
            .active_scenario,
        None
    );
}

#[test]
/// The active scenario is marked "Active" on its row.
fn ui_scn_active_row_shows_checkmark() {
    let (mut app, plan) = plan_app();
    app.engine()
        .lock()
        .expect("engine lock")
        .set_active_scenario(Some(plan));
    let mut h = harness(&mut app);
    h.run();
    assert!(h.query_by_label("Active").is_some(), "active marker");
}

#[test]
/// R-SCN-4: a new scenario gets the default purple colour.
fn ui_scn_create_via_dialog() {
    let (mut app, _) = plan_app();
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("+ Add Scenario").click();
        h.run();
        h.get_by_role_and_label(Role::TextInput, "Name").click();
        h.run();
        h.get_by_role_and_label(Role::TextInput, "Name")
            .type_text("Vacation");
        h.run();
        h.get_by_label("Create").click();
        h.run();
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    let s = e
        .db()
        .live_scenarios()
        .find(|s| s.name == "Vacation")
        .expect("scenario");
    assert_eq!(s.color_hex.as_deref(), Some("#800080"));
}

#[test]
/// R-SCN-4: Create is disabled while the name is blank.
fn ui_scn_create_disabled_when_name_empty() {
    let (mut app, _) = plan_app();
    let mut h = harness(&mut app);
    h.run();
    h.get_by_label("+ Add Scenario").click();
    h.run();
    assert!(h.get_by_label("Create").accesskit_node().is_disabled());
}

#[test]
/// Edit from the row's menu renames the scenario, as one undo step.
fn ui_scn_edit_renames() {
    let (mut app, plan) = plan_app();
    {
        let mut h = harness(&mut app);
        h.run();
        scenario_row(&h, "Plan A").click_secondary();
        h.run();
        h.get_by_label("Edit Scenario").click();
        h.run();
        h.get_by_role_and_label(Role::TextInput, "Name").click();
        h.run();
        h.key_press_modifiers(Modifiers::COMMAND, Key::A);
        h.run();
        h.get_by_role_and_label(Role::TextInput, "Name")
            .type_text("Plan B");
        h.run();
        h.get_by_label("Save").click();
        h.run();
    }
    let shared = app.engine();
    let mut e = shared.lock().expect("engine lock");
    assert_eq!(e.db().scenario(plan).expect("scenario").name, "Plan B");
    e.undo().expect("undo");
    assert_eq!(e.db().scenario(plan).expect("scenario").name, "Plan A");
}

#[test]
/// R-SCN-3: deleting the active scenario makes Reality active and hides its transactions; Cmd-Z
/// brings the scenario back.
fn ui_scn_delete_active_deactivates_and_hides_tx() {
    let (mut app, plan) = plan_app();
    app.engine()
        .lock()
        .expect("engine lock")
        .set_active_scenario(Some(plan));
    {
        let mut h = harness(&mut app);
        h.run();
        scenario_row(&h, "Plan A").click_secondary();
        h.run();
        h.get_by_label("Delete").click();
        h.run();
    }
    {
        let shared = app.engine();
        let e = shared.lock().expect("engine lock");
        assert_eq!(e.state.active_scenario, None, "back to Reality");
        assert!(e.db().live_scenarios().next().is_none(), "scenario deleted");
        assert!(
            e.db().live_transactions().all(|t| t.scenario_id.is_none()),
            "scenario transactions hidden"
        );
    }
    let mut h = harness(&mut app);
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run();
    assert!(
        h.query_by_label("Plan A").is_some(),
        "scenario back after undo"
    );
}
