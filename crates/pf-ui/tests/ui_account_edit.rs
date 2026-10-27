//! L5 UI tests for inline editing in the account table (T15b): payee, amount, date, category,
//! memo, contexts, status, locked rows and auto-categorize. Headless (egui_kittest). Assertions are
//! made on engine state (what was committed, and how many undo steps it took), not on pixels.

mod common;

use egui::{Key, Modifiers};
use egui_kittest::kittest::{NodeT, Queryable};
use pf_engine::Engine;
use pf_model::testing::TestDb;
use pf_model::{AccountType, CategoryType, Db, TransactionStatus};
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

/// A harness wide enough for the Status column (the last one), which sits past 1200 pt.
fn wide_harness(app: &mut PfApp) -> egui_kittest::Harness<'_> {
    egui_kittest::Harness::builder()
        .with_size(egui::Vec2::new(2000.0, 800.0))
        .build_ui(|ui| app.ui_root(ui))
}

/// Focuses the field labelled `label`, selects all its text and types `text` over it.
fn replace_text(h: &mut egui_kittest::Harness<'_>, label: &str, text: &str) {
    h.get_by_label(label).focus();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run();
    h.get_by_label(label).type_text(text);
    h.run();
}

#[test]
/// R-TXN-12, UI-TXN-02: click the payee, type, press Enter ⇒ one "Change Payee" step; one undo
/// restores the old payee.
fn ui_txn_02_payee_commit_on_enter_one_undo_entry() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    let tx = t
        .tx(chk, "2026-03-01", "-4.50", "Coffee")
        .status(TransactionStatus::Cleared)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Account(chk));
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Payee Coffee").click();
        h.run();
        h.get_by_label("Payee Coffee").type_text(" Shop");
        h.run();
        h.key_press(Key::Enter);
        h.run();
    }
    let shared = app.engine();
    let mut e = shared.lock().expect("engine lock");
    assert_eq!(e.db().transaction(tx).expect("row").payee, "Coffee Shop");
    assert_eq!(e.undo_name(), Some("Change Payee"));
    e.undo().expect("undo");
    assert_eq!(e.db().transaction(tx).expect("row").payee, "Coffee");
}

#[test]
/// R-TXN-12: focusing the payee and pressing Enter without typing makes no undo step.
fn ui_txn_02_unchanged_payee_creates_no_undo() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    t.tx(chk, "2026-03-01", "-4.50", "Coffee")
        .status(TransactionStatus::Cleared)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Account(chk));
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Payee Coffee").click();
        h.run();
        h.key_press(Key::Enter);
        h.run();
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    assert_eq!(e.undo_name(), None);
}

#[test]
/// R-TXN-12: Esc during a payee edit cancels it; the row keeps its payee and no step is made.
fn ui_txn_02_escape_cancels_edit() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    let tx = t
        .tx(chk, "2026-03-01", "-4.50", "Coffee")
        .status(TransactionStatus::Cleared)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Account(chk));
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Payee Coffee").click();
        h.run();
        h.get_by_label("Payee Coffee").type_text(" Shop");
        h.run();
        h.key_press(Key::Escape);
        h.run();
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    assert_eq!(e.db().transaction(tx).expect("row").payee, "Coffee");
    assert_eq!(e.undo_name(), None);
}

#[test]
/// UI-TXN-03, R-TXN-1: under a scenario, the reality row's payee is disabled and the scenario
/// row's is enabled.
fn ui_txn_03_locked_rows_render_disabled() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    let plan = t.scenario("Plan A");
    t.tx(chk, "2026-03-01", "-4.50", "Coffee")
        .status(TransactionStatus::Cleared)
        .done();
    t.tx(chk, "2026-03-02", "-9.00", "Snack")
        .status(TransactionStatus::Pending)
        .scenario(plan)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Account(chk));
    {
        let shared = app.engine();
        let mut e = shared.lock().expect("engine lock");
        e.set_active_scenario(Some(plan));
    }
    let mut h = harness(&mut app);
    h.run();
    assert!(
        h.get_by_label("Payee Coffee")
            .accesskit_node()
            .is_disabled(),
        "reality row locked"
    );
    assert!(
        !h.get_by_label("Payee Snack").accesskit_node().is_disabled(),
        "scenario row editable"
    );
}

#[test]
/// UI-TXN-13 / R-TXN-13: an uncategorized row whose payee now matches a category pattern gets that
/// category as a second step "Auto-categorize". One undo removes the category; another restores
/// the old payee.
fn ui_txn_13_payee_edit_autocategorizes() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    let coffee = t.category("Coffee", CategoryType::Expense);
    t.patterns(coffee, &["starbucks"]);
    let tx = t
        .tx(chk, "2026-03-01", "-4.50", "Morning")
        .status(TransactionStatus::Cleared)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Account(chk));
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Payee Morning").click();
        h.run();
        replace_text(&mut h, "Payee Morning", "Starbucks #12");
        h.key_press(Key::Enter);
        h.run();
    }
    let shared = app.engine();
    let mut e = shared.lock().expect("engine lock");
    let row = e.db().transaction(tx).expect("row").clone();
    assert_eq!(row.payee, "Starbucks #12");
    assert_eq!(row.category_id, Some(coffee), "auto-categorized");
    e.undo().expect("undo auto-categorize");
    let row = e.db().transaction(tx).expect("row").clone();
    assert_eq!(row.category_id, None);
    assert_eq!(row.payee, "Starbucks #12");
    e.undo().expect("undo payee");
    assert_eq!(e.db().transaction(tx).expect("row").payee, "Morning");
}

#[test]
/// R-TXN-12: an amount commits on Enter as one "Change Amount" step; Esc cancels with no change.
fn ui_amount_edit_commits_and_cancels() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    let tx = t
        .tx(chk, "2026-03-01", "-4.50", "Coffee")
        .status(TransactionStatus::Cleared)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Account(chk));
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Amount Coffee").click();
        h.run();
        replace_text(&mut h, "Amount Coffee", "-42.5");
        h.key_press(Key::Enter);
        h.run();
    }
    let revision_after_commit = {
        let shared = app.engine();
        let e = shared.lock().expect("engine lock");
        assert_eq!(
            e.db().transaction(tx).expect("row").amount.to_string(),
            "-42.5"
        );
        assert_eq!(e.undo_name(), Some("Change Amount"));
        e.revision()
    };
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Amount Coffee").click();
        h.run();
        replace_text(&mut h, "Amount Coffee", "99");
        h.key_press(Key::Escape);
        h.run();
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    assert_eq!(
        e.db().transaction(tx).expect("row").amount.to_string(),
        "-42.5"
    );
    assert_eq!(e.revision(), revision_after_commit, "Esc makes no change");
}

#[test]
/// R-TXN-2: Future is refused on a reality row. The status stays the same and a toast explains why.
fn ui_status_future_on_reality_shows_toast_no_change() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    let tx = t
        .tx(chk, "2026-03-01", "-4.50", "Coffee")
        .status(TransactionStatus::Cleared)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Account(chk));
    {
        let mut h = wide_harness(&mut app);
        h.run();
        h.get_by_label("Status Coffee").click();
        h.run();
        h.get_by_label("Future").click();
        h.run();
        assert!(
            h.query_by_label_contains("Future status is only for scenario transactions")
                .is_some(),
            "toast shown"
        );
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    assert_eq!(
        e.db().transaction(tx).expect("row").status,
        TransactionStatus::Cleared
    );
}

#[test]
/// R-TXN-4: under a scenario, picking Pending on a scenario row promotes it to reality.
fn ui_status_pending_promotes_scenario_row() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    let plan = t.scenario("Plan A");
    let tx = t
        .tx(chk, "2026-03-02", "-9.00", "Snack")
        .status(TransactionStatus::Cleared)
        .scenario(plan)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Account(chk));
    {
        let shared = app.engine();
        let mut e = shared.lock().expect("engine lock");
        e.set_active_scenario(Some(plan));
    }
    {
        let mut h = wide_harness(&mut app);
        h.run();
        h.get_by_label("Status Snack").click();
        h.run();
        h.get_by_label("Pending").click();
        h.run();
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    let row = e.db().transaction(tx).expect("row");
    assert_eq!(row.status, TransactionStatus::Pending);
    assert_eq!(row.scenario_id, None, "promoted to reality");
}

#[test]
/// The "⊕" menu toggles a context: the first click adds it, the second removes it.
fn ui_contexts_toggle_adds_and_removes() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    let trip = t.context("trip");
    let tx = t
        .tx(chk, "2026-03-01", "-4.50", "Coffee")
        .status(TransactionStatus::Cleared)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Account(chk));
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Payee Coffee").hover();
        h.run();
        h.get_by_label("+").click();
        h.run();
        h.run();
        h.get_by_label("trip").click();
        h.run();
    }
    {
        let shared = app.engine();
        let e = shared.lock().expect("engine lock");
        assert_eq!(e.db().transaction(tx).expect("row").context_ids, vec![trip]);
    }
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Payee Coffee").hover();
        h.run();
        h.get_by_label("+").click();
        h.run();
        h.get_by_label("• trip").click();
        h.run();
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    assert!(e.db().transaction(tx).expect("row").context_ids.is_empty());
}

#[test]
/// The category picker commits the chosen category as one "Change Category" step.
fn ui_category_picker_sets_category() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    let food = t.category("Food", CategoryType::Expense);
    let tx = t
        .tx(chk, "2026-03-01", "-4.50", "Bakery")
        .status(TransactionStatus::Cleared)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Account(chk));
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Category Bakery").click();
        h.run();
        h.get_by_label("Food").click();
        h.run();
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    assert_eq!(e.db().transaction(tx).expect("row").category_id, Some(food));
    assert_eq!(e.undo_name(), Some("Change Category"));
}

#[test]
/// The table scrolls sideways, so the last column (Status) is reachable in a normal window: a
/// horizontal wheel over the table brings the picker on screen and its popup opens.
fn ui_table_scrolls_to_status_column() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    t.tx(chk, "2026-03-01", "-4.50", "Coffee")
        .status(TransactionStatus::Cleared)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Account(chk));
    let mut h = harness(&mut app);
    h.run();
    let over_table = h.get_by_label("Payee Coffee").rect().center();
    h.event(egui::Event::PointerMoved(over_table));
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::Vec2::new(-2000.0, 0.0),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Unknown,
        modifiers: Modifiers::NONE,
    });
    // The scroll animates over several frames; step through them.
    h.run_steps(30);
    h.get_by_label("Status Coffee").click();
    h.run();
    assert!(
        h.query_by_label("Pending").is_some(),
        "status popup opened after scrolling"
    );
}

#[test]
/// Scrolling sideways stops at the end of the table: scrolling far past it must not pull the left
/// (Date) column away from the viewport's left edge.
fn ui_table_does_not_scroll_past_its_end() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    t.tx(chk, "2026-03-01", "-4.50", "Coffee")
        .status(TransactionStatus::Cleared)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Account(chk));
    let mut h = egui_kittest::Harness::builder()
        .with_size(egui::Vec2::new(1000.0, 800.0))
        .build_ui(|ui| app.ui_root(ui));
    h.run();
    let at_start = h.get_by_label("Payee Coffee").rect().min.x;
    let over_table = h.get_by_label("Payee Coffee").rect().center();
    h.event(egui::Event::PointerMoved(over_table));
    for _ in 0..200 {
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::Vec2::new(-40.0, 0.0),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(3);
    }
    h.run_steps(60);
    let after = h.get_by_label("Payee Coffee").rect().min.x;
    assert!(
        after <= at_start + 1.0,
        "the table moved past its end: payee x {after} is right of its start {at_start}"
    );
}

#[test]
/// A trackpad scroll toward the left end must not leave a gap there: the table springs back at most
/// to its start, never past it (the rubber band seen on macOS).
fn ui_table_trackpad_does_not_bounce_past_left_end() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    t.tx(chk, "2026-03-01", "-4.50", "Coffee")
        .status(TransactionStatus::Cleared)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Account(chk));
    let mut h = egui_kittest::Harness::builder()
        .with_size(egui::Vec2::new(1000.0, 800.0))
        .build_ui(|ui| app.ui_root(ui));
    h.run();
    let at_start = h.get_by_label("Payee Coffee").rect().min.x;
    h.event(egui::Event::PointerMoved(
        h.get_by_label("Payee Coffee").rect().center(),
    ));
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::Vec2::new(400.0, 0.0),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Trackpad,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(2);
    let during = h.get_by_label("Payee Coffee").rect().min.x;
    assert!(
        during <= at_start + 0.5,
        "the table moved past its left end: payee x {during}, start {at_start}"
    );
}
