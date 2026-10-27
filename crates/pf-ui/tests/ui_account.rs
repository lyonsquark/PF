//! L5 UI behaviour tests for the account detail screen (T15a): header balances, the time period,
//! the scenario label, renaming, row selection and a large table. Headless (egui_kittest);
//! assertions are made on engine state or on the text the screen draws.

mod common;

use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable;
use pf_engine::Engine;
use pf_model::testing::TestDb;
use pf_model::{AccountType, Db, TimePeriod, TransactionStatus};
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

#[test]
/// R-BAL-1, R-BAL-2: with a pending transaction, the balance and the "Cleared:" line are shown;
/// without one, "Cleared:" is absent.
fn ui_account_header_shows_balance_and_cleared() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    t.tx(chk, "2026-03-01", "1000", "Pay")
        .status(TransactionStatus::Cleared)
        .done();
    t.tx(chk, "2026-03-02", "-100", "Hold")
        .status(TransactionStatus::Pending)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Account(chk));
    {
        let mut h = harness(&mut app);
        h.run();
        assert!(h.query_all_by_label("$900.00").count() > 0, "balance");
        assert!(h.query_by_label("Cleared:").is_some(), "cleared line");
        assert!(
            h.query_all_by_label("$1,000.00").count() > 0,
            "cleared value"
        );
    }

    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    t.tx(chk, "2026-03-01", "1000", "Pay")
        .status(TransactionStatus::Cleared)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Account(chk));
    let mut h = harness(&mut app);
    h.run();
    assert!(h.query_by_label("Cleared:").is_none(), "no cleared line");
}

#[test]
/// R-FLT-1: an old transaction is hidden under "Past Month" and shown after choosing "All".
fn ui_account_time_period_changes_rows() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    t.tx(chk, "2025-06-01", "-10", "Ancient Payee")
        .status(TransactionStatus::Cleared)
        .done();
    t.tx(chk, "2026-03-10", "-5", "Recent Payee")
        .status(TransactionStatus::Cleared)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Account(chk));
    {
        let mut h = harness(&mut app);
        h.run();
        assert!(
            h.query_by_label("Ancient Payee").is_none(),
            "hidden in Past Month"
        );
        assert!(h.query_by_label("Recent Payee").is_some());
        h.get_by_label("Time Period").click();
        h.run();
        h.get_by_label("All").click();
        h.run();
        assert!(h.query_by_label("Ancient Payee").is_some(), "shown in All");
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    assert_eq!(e.state.time_period, TimePeriod::All);
}

#[test]
/// Under an active scenario the toolbar shows "Scenario: {name}" on the account screen.
fn ui_account_scenario_label_when_active() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    let plan = t.scenario("Plan A");
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Account(chk));
    {
        let shared = app.engine();
        let mut e = shared.lock().expect("engine lock");
        e.set_active_scenario(Some(plan));
    }
    let mut h = harness(&mut app);
    h.run();
    assert!(h.query_by_label("Scenario: Plan A").is_some());
}

#[test]
/// Renaming: click "Account Name", type, press Enter ⇒ the account is renamed in the engine, and
/// one undo step restores the old name.
fn ui_account_rename_commits_on_enter() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Account(chk));
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Account Name").click();
        h.run();
        h.key_press(Key::End);
        h.run();
        h.get_by_label("Account Name").type_text(" Plus");
        h.run();
        h.key_press(Key::Enter);
        h.run();
    }
    let shared = app.engine();
    let mut e = shared.lock().expect("engine lock");
    assert_eq!(
        e.db().account(chk).map(|a| a.name.as_str()),
        Some("Everyday Plus")
    );
    assert_eq!(e.undo_name(), Some("Edit Account"));
    assert!(e.undo().expect("undo").is_some());
    assert_eq!(
        e.db().account(chk).map(|a| a.name.as_str()),
        Some("Everyday")
    );
}

#[test]
/// Selection: click A ⇒ {A}; ⌘-click B ⇒ {A,B}; ⇧-click D ⇒ the range from the anchor B to D
/// (rows show newest first: D, C, B, A); click C ⇒ {C}.
fn ui_account_selection_click_and_cmd_click() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    t.tx(chk, "2026-03-01", "-1", "Alpha Row")
        .status(TransactionStatus::Cleared)
        .done();
    let b = t
        .tx(chk, "2026-03-02", "-2", "Bravo Row")
        .status(TransactionStatus::Cleared)
        .done();
    let c = t
        .tx(chk, "2026-03-03", "-3", "Charlie Row")
        .status(TransactionStatus::Cleared)
        .done();
    let d = t
        .tx(chk, "2026-03-04", "-4", "Delta Row")
        .status(TransactionStatus::Cleared)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Account(chk));
    let mut h = harness(&mut app);
    h.run();
    h.get_by_label("Alpha Row").click();
    h.run();
    h.get_by_label("Bravo Row")
        .click_modifiers(Modifiers::COMMAND);
    h.run();
    h.get_by_label("Delta Row")
        .click_modifiers(Modifiers::SHIFT);
    h.run();
    drop(h);
    assert_eq!(app.account_selection(), sorted(&[b, c, d]));

    let mut h = harness(&mut app);
    h.run();
    h.get_by_label("Charlie Row").click();
    h.run();
    drop(h);
    assert_eq!(app.account_selection(), vec![c]);
}

#[test]
/// Perf smoke: 20,000 transactions render one frame without panicking (no timing assertion).
fn ui_account_large_table_renders() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    for i in 0..20_000u32 {
        let day = 1 + i % 28;
        let date = format!("2026-03-{day:02}");
        t.tx(chk, &date, "-1.25", &format!("Payee {i}")).done();
    }
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Account(chk));
    let mut h = harness(&mut app);
    h.run();
    // Newest first: the last id dated 2026-03-28 (day 28) is the top row. Rows far down are
    // virtualised away, so only the top rows have labels.
    assert!(h.query_by_label("Payee 19991").is_some(), "top row drawn");
    assert!(
        h.query_by_label("Payee 0").is_none(),
        "oldest row is virtualised out"
    );
}

/// Sorted copy of ids for comparison with the selection set.
fn sorted(ids: &[pf_model::TransactionId]) -> Vec<pf_model::TransactionId> {
    let mut v = ids.to_vec();
    v.sort();
    v
}
