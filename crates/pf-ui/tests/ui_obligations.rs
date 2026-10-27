//! L5 UI tests for the Obligations screen and its dialogs (T18): sections, the New Obligation dialog
//! (validation, context names, creating the context), delete with undo, the Reality balance in a
//! scenario, and the detail dialog (balance, due date, a committed text edit). Headless
//! (egui_kittest); assertions are on engine state and on what the screen draws.

mod common;

use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::kittest::{NodeT, Queryable};
use pf_engine::Engine;
use pf_engine::testing::TEST_TODAY;
use pf_model::testing::TestDb;
use pf_model::{AccountType, Db, ObligationStatus};
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

/// Clicks the text field labelled `label` (by role), then types `text` into it.
fn type_field(h: &mut egui_kittest::Harness<'_>, label: &str, text: &str) {
    h.get_by_role_and_label(Role::TextInput, label).click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, label)
        .type_text(text);
    h.run();
}

/// Picks `option` from the combo box labelled `label`.
fn pick(h: &mut egui_kittest::Harness<'_>, label: &str, option: &str) {
    h.get_by_role_and_label(Role::ComboBox, label).click();
    h.run();
    let options: Vec<_> = h.get_all_by_label(option).collect();
    options.last().expect("the option").click();
    h.run();
}

#[test]
/// UI-OBL-01: a new obligation is added through the dialog, then moved to Spending from its detail.
fn ui_obl_01_create_and_change_status_moves_group() {
    let mut t = TestDb::new();
    t.account("Checking", AccountType::Checking);
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Obligations);
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("+ Add Obligation").click();
        h.run();
        type_field(&mut h, "Name", "Down Payment");
        pick(&mut h, "Account", "Checking");
        h.get_by_label("Save").click();
        h.run();
        assert!(h.query_by_label("Down Payment").is_some(), "card shown");
        h.get_by_label("Down Payment").click();
        h.run();
        h.get_by_label("Spending").click();
        h.run();
        h.get_by_label("Done").click();
        h.run();
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    let ob = e
        .db()
        .live_obligations()
        .find(|o| o.name == "Down Payment")
        .expect("obligation");
    assert_eq!(ob.status, ObligationStatus::Spending);
}

#[test]
/// R-OBL-5: a status with no obligations has no section.
fn ui_obl_sections_hidden_when_empty() {
    let mut t = TestDb::new();
    t.obligation("Car").done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Obligations);
    let mut h = harness(&mut app);
    h.run();
    assert!(h.query_by_label("Funding").is_some(), "funding section");
    assert!(
        h.query_by_label("Completed").is_none(),
        "no completed section"
    );
}

#[test]
/// Save stays disabled until an account is chosen.
fn ui_obl_save_disabled_without_account() {
    let mut t = TestDb::new();
    t.account("Checking", AccountType::Checking);
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Obligations);
    let mut h = harness(&mut app);
    h.run();
    h.get_by_label("+ Add Obligation").click();
    h.run();
    type_field(&mut h, "Name", "Down Payment");
    assert!(
        h.get_by_label("Save").accesskit_node().is_disabled(),
        "no account yet"
    );
}

#[test]
/// R-OBL-4: the context name follows the obligation name; an existing name gets a "-2".
fn ui_obl_context_name_generated() {
    let mut t = TestDb::new();
    t.context("down-payment");
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Obligations);
    let mut h = harness(&mut app);
    h.run();
    h.get_by_label("+ Add Obligation").click();
    h.run();
    type_field(&mut h, "Name", "Down Payment");
    assert_eq!(
        h.get_by_role_and_label(Role::TextInput, "Context Name")
            .value()
            .as_deref(),
        Some("down-payment-2")
    );
}

#[test]
/// R-OBL-4: "Create New Context" makes a linked context named after the obligation, with the notes
/// "{name} obligation".
fn ui_obl_create_new_context_creates_context() {
    let mut t = TestDb::new();
    t.account("Checking", AccountType::Checking);
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Obligations);
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("+ Add Obligation").click();
        h.run();
        type_field(&mut h, "Name", "Down Payment");
        pick(&mut h, "Account", "Checking");
        h.get_by_label("Save").click();
        h.run();
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    let ob = e
        .db()
        .live_obligations()
        .find(|o| o.name == "Down Payment")
        .expect("obligation");
    let ctx = e
        .db()
        .context(ob.context_id.expect("linked context"))
        .expect("context");
    assert_eq!(ctx.name, "down-payment");
    assert_eq!(ctx.notes.as_deref(), Some("Down Payment obligation"));
}

#[test]
/// Delete from the card's menu removes it, and Cmd-Z brings it back.
fn ui_obl_delete_via_context_menu_is_undoable() {
    let mut t = TestDb::new();
    t.obligation("Car fund").done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Obligations);
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Car fund").click_secondary();
        h.run();
        h.get_by_label("Delete").click();
        h.run();
        assert!(h.query_by_label("Car fund").is_none(), "removed");
    }
    {
        let mut h = harness(&mut app);
        h.run();
        h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
        h.run();
        assert!(h.query_by_label("Car fund").is_some(), "back after undo");
    }
}

#[test]
/// R-OBL-6: the card shows the Reality balance (100 funded + 50 reality), not the scenario's 900.
fn ui_obl_list_uses_reality_balance_in_scenario() {
    let (mut app, plan) = scenario_app();
    app.engine()
        .lock()
        .expect("engine lock")
        .set_active_scenario(Some(plan));
    let mut h = harness(&mut app);
    h.run();
    assert!(h.query_by_label("$150.00").is_some(), "reality balance");
    assert!(
        h.query_by_label("$1,050.00").is_none(),
        "not the scenario balance"
    );
}

/// One obligation "Fund" with 100 funded, a reality deposit of 50 and a scenario deposit of 900,
/// all tagged with its context. Returns the app and the scenario id.
fn scenario_app() -> (PfApp, pf_model::ScenarioId) {
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let plan = t.scenario("Plan A");
    let ctx = t.context("fund");
    t.obligation("Fund")
        .balance("100")
        .context(ctx)
        .account(chk)
        .done();
    t.tx(chk, "2026-03-01", "50", "Deposit")
        .contexts(&[ctx])
        .done();
    t.tx(chk, "2026-03-02", "900", "Scenario deposit")
        .contexts(&[ctx])
        .scenario(plan)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Obligations);
    (app, plan)
}

#[test]
/// R-OBL-1: the detail shows the computed balance: funded plus the tagged reality rows.
fn ui_obl_detail_shows_computed_balance() {
    let (mut app, _) = scenario_app();
    let mut h = harness(&mut app);
    h.run();
    h.get_by_label("Fund").click();
    h.run();
    assert!(
        h.query_all_by_label("$150.00").count() >= 2,
        "card and detail"
    );
}

#[test]
/// "Set Due Date" sets today; the "x" clears it.
fn ui_obl_detail_set_and_remove_due_date() {
    let mut t = TestDb::new();
    t.obligation("Car fund").done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Obligations);
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Car fund").click();
        h.run();
        h.get_by_label("Set Due Date").click();
        h.run();
    }
    {
        let shared = app.engine();
        let e = shared.lock().expect("engine lock");
        let ob = e.db().live_obligations().next().expect("obligation");
        assert_eq!(ob.due_date, Some(TEST_TODAY));
    }
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("x").click();
        h.run();
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    assert_eq!(
        e.db()
            .live_obligations()
            .next()
            .expect("obligation")
            .due_date,
        None
    );
}

#[test]
/// R-TXN-12 analogue: notes commit once, when the field loses focus, as one undo step.
fn ui_obl_detail_text_edit_commits_once() {
    let mut t = TestDb::new();
    t.obligation("Car fund").done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Obligations);
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Car fund").click();
        h.run();
        type_field(&mut h, "Notes", "for the car");
        h.get_by_role_and_label(Role::TextInput, "Name").click();
        h.run();
        h.get_by_role_and_label(Role::TextInput, "Name").click();
        h.run();
    }
    let shared = app.engine();
    let mut e = shared.lock().expect("engine lock");
    let ob = e
        .db()
        .live_obligations()
        .next()
        .expect("obligation")
        .clone();
    assert_eq!(ob.notes.as_deref(), Some("for the car"));
    e.undo().expect("undo");
    assert_eq!(
        e.db().live_obligations().next().expect("obligation").notes,
        None
    );
}
