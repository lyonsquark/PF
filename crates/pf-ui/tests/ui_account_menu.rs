//! L5 UI tests for the account row context menu and the attachment column (T15c): Duplicate,
//! Delete, Add to Budget, undo, and attachments through the fake file dialogs. Headless
//! (egui_kittest). Assertions are on engine state and on what the screen draws.

mod common;

use std::path::PathBuf;

use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable;
use pf_engine::Engine;
use pf_engine::testing::TEST_TODAY;
use pf_model::testing::TestDb;
use pf_model::{AccountType, BudgetItemType, Db, TransactionStatus};
use pf_ui::PfApp;
use pf_ui::files::FakeDialogs;
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
/// UI-TXN-05, R-TXN-6: right-click a row, choose Duplicate ⇒ one more row, dated today, and one
/// "Duplicate Transaction" step.
fn ui_txn_05_context_menu_duplicate() {
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
        h.get_by_label("Payee Coffee").click_secondary();
        h.run();
        h.get_by_label("Duplicate").click();
        h.run();
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    assert_eq!(e.db().live_transactions().count(), 2);
    let copy = e
        .db()
        .live_transactions()
        .find(|t| t.date == TEST_TODAY)
        .expect("a copy dated today");
    assert_eq!(copy.payee, "Coffee");
    assert_eq!(e.undo_name(), Some("Duplicate Transaction"));
}

#[test]
/// UI-TXN-05, R-TXN-7: select two rows, right-click, Delete ⇒ both are gone in one
/// "Delete Transactions" step.
fn ui_txn_05_context_menu_delete_multi() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    t.tx(chk, "2026-03-01", "-1", "Alpha")
        .status(TransactionStatus::Cleared)
        .done();
    t.tx(chk, "2026-03-02", "-2", "Bravo")
        .status(TransactionStatus::Cleared)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Account(chk));
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Payee Alpha").click();
        h.run();
        h.get_by_label("Payee Bravo")
            .click_modifiers(Modifiers::COMMAND);
        h.run();
        h.get_by_label("Payee Bravo").click_secondary();
        h.run();
        h.get_by_label("Delete").click();
        h.run();
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    assert_eq!(e.db().live_transactions().count(), 0);
    assert_eq!(e.undo_name(), Some("Delete Transactions"));
}

#[test]
/// R-TXN-1: under a scenario a reality row is locked, so its menu has no Delete.
fn ui_txn_05_delete_hidden_when_all_locked() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    let plan = t.scenario("Plan A");
    t.tx(chk, "2026-03-01", "-4.50", "Coffee")
        .status(TransactionStatus::Cleared)
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
    h.get_by_label("Payee Coffee").click_secondary();
    h.run();
    assert!(h.query_by_label("Duplicate").is_some(), "menu is open");
    assert!(
        h.query_by_label("Delete").is_none(),
        "no Delete for a locked row"
    );
}

#[test]
/// R-TXN-11: "Add to Budget" makes a Bill named after the payee and says so in a toast.
fn ui_txn_05_add_to_budget_toast() {
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
        h.get_by_label("Payee Coffee").click_secondary();
        h.run();
        h.get_by_label("Add to Budget").click();
        h.run();
        assert!(
            h.query_by_label_contains("Added \"Coffee\" to Bills")
                .is_some(),
            "toast shown"
        );
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    let bill = e
        .db()
        .live_budget_items()
        .find(|b| b.name == "Coffee")
        .expect("a budget item named after the payee");
    assert_eq!(bill.item_type, BudgetItemType::Bill);
}

#[test]
/// UI-UND-01: after a menu delete the undo step is named, and ⌘Z with no field focused brings the
/// row back.
fn ui_und_01_cmd_z_restores_deleted_row() {
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
        h.get_by_label("Payee Coffee").click_secondary();
        h.run();
        h.get_by_label("Delete").click();
        h.run();
        assert!(h.query_by_label("Payee Coffee").is_none(), "row deleted");
    }
    {
        let shared = app.engine();
        let e = shared.lock().expect("engine lock");
        assert_eq!(e.undo_name(), Some("Delete Transaction"));
    }
    {
        let mut h = harness(&mut app);
        h.run();
        h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
        h.run();
        assert!(h.query_by_label("Payee Coffee").is_some(), "row back");
    }
}

/// An account with one row "Coffee" that has an attachment at `path`.
fn attached_app(path: Option<&str>, files: FakeDialogs) -> (PfApp, pf_model::AccountId) {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    let mut tx = t
        .tx(chk, "2026-03-01", "-4.50", "Coffee")
        .status(TransactionStatus::Cleared);
    if let Some(p) = path {
        tx = tx.attachment(p);
    }
    tx.done();
    let mut app = PfApp::new_for_test_with_files(engine_with(t.build()), files);
    app.set_screen(Screen::Account(chk));
    (app, chk)
}

#[test]
/// R-TXN-14: "Attach" asks for a file and sets it; the undo step is "Change Attachment".
fn ui_attach_file_via_fake_dialog() {
    let files = FakeDialogs::default();
    files.push_answer(Some(PathBuf::from("/tmp/x.pdf")));
    let (mut app, _) = attached_app(None, files.clone());
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Payee Coffee").hover();
        h.run();
        h.get_by_label("Attach").click();
        h.run();
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    let tx = e.db().live_transactions().next().expect("row");
    assert_eq!(tx.attachment_path.as_deref(), Some("/tmp/x.pdf"));
    assert_eq!(e.undo_name(), Some("Change Attachment"));
}

#[test]
/// R-TXN-14: an attachment that may not be opened gets an alert and is not handed to the OS.
fn ui_open_unsafe_attachment_shows_alert() {
    let files = FakeDialogs::default();
    let (mut app, _) = attached_app(Some("/tmp/x.exe"), files.clone());
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Payee Coffee").hover();
        h.run();
        h.get_by_label("Open").click();
        h.run();
        assert!(
            h.query_by_label("Unsafe Attachment").is_some(),
            "alert shown"
        );
    }
    assert!(files.opened().is_empty(), "nothing opened");
}

#[test]
/// R-TXN-14: an allowed attachment is opened with the OS.
fn ui_open_safe_attachment_calls_open() {
    let files = FakeDialogs::default();
    let (mut app, _) = attached_app(Some("/tmp/x.pdf"), files.clone());
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Payee Coffee").hover();
        h.run();
        h.get_by_label("Open").click();
        h.run();
    }
    assert_eq!(files.opened(), vec![PathBuf::from("/tmp/x.pdf")]);
}

#[test]
/// The row's hover buttons (Attach here) are hidden until the pointer is over the row.
fn ui_row_buttons_show_on_hover() {
    let (mut app, _) = attached_app(None, FakeDialogs::default());
    let mut h = harness(&mut app);
    h.run();
    assert!(
        h.query_by_label("Attach").is_none(),
        "hidden when not hovered"
    );
    h.get_by_label("Payee Coffee").hover();
    h.run();
    assert!(
        h.query_by_label("Attach").is_some(),
        "shown when the row is hovered"
    );
}
