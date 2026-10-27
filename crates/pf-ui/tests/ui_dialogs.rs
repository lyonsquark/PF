//! L5 UI tests for the T16 dialogs: New Transaction (Save, Save & Another, transfers, auto-categorize,
//! Rec), New Account and Edit Account (starting balance). Headless (egui_kittest); assertions are on
//! engine state and on what the dialogs draw.

mod common;

use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::kittest::{NodeT, Queryable};
use pf_engine::Engine;
use pf_model::testing::TestDb;
use pf_model::{AccountId, AccountType, CategoryType, Db};
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

/// Clicks the field labelled `label` (so it has focus), then types `text` into it.
fn type_into(h: &mut egui_kittest::Harness<'_>, label: &str, text: &str) {
    h.get_by_label(label).click();
    h.run();
    h.get_by_label(label).type_text(text);
    h.run();
}

/// Clicks the field labelled `label`, selects its existing text, and types `text` over it.
fn replace_text(h: &mut egui_kittest::Harness<'_>, label: &str, text: &str) {
    h.get_by_label(label).click();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run();
    h.get_by_label(label).type_text(text);
    h.run();
}

/// Clicks the combo box labelled `label` (a picker's accessible name, not the caption text).
fn open_picker(h: &mut egui_kittest::Harness<'_>, label: &str) {
    h.get_by_role_and_label(Role::ComboBox, label).click();
    h.run();
}

/// An account "Everyday" with no rows, shown on its account screen.
fn account_app() -> (PfApp, AccountId) {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Account(chk));
    (app, chk)
}

#[test]
/// UI-TXN-01, R-TXN-8: "+ Add Transaction", a payee and an amount, then Save ⇒ the dialog closes and
/// a withdrawal of −12.50 named "Lunch" exists.
fn ui_txn_01_add_withdrawal_appears_in_table() {
    let (mut app, chk) = account_app();
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("+ Add Transaction").click();
        h.run();
        type_into(&mut h, "Payee", "Lunch");
        type_into(&mut h, "Amount", "12.5");
        h.run();
        h.get_by_label("Save").click();
        h.run();
        assert!(
            h.query_by_label("New Transaction").is_none(),
            "dialog closed"
        );
        assert!(h.query_by_label("Payee Lunch").is_some(), "row shown");
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    let tx = e
        .db()
        .live_transactions()
        .find(|t| t.payee == "Lunch")
        .expect("saved row");
    assert_eq!(tx.account_id, Some(chk));
    assert_eq!(tx.amount.to_string(), "-12.5");
}

#[test]
/// UI-TXN-01, R-TXN-10: "Save & Another" saves, keeps the dialog open, clears the payee and keeps the
/// account.
fn ui_txn_01_save_and_another_keeps_dialog_and_account() {
    let (mut app, chk) = account_app();
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("+ Add Transaction").click();
        h.run();
        type_into(&mut h, "Payee", "Lunch");
        type_into(&mut h, "Amount", "12.5");
        h.run();
        h.get_by_label("Save & Another").click();
        h.run();
        assert!(
            h.query_by_label("New Transaction").is_some(),
            "dialog still open"
        );
        assert_eq!(
            h.get_by_role_and_label(Role::TextInput, "Payee")
                .value()
                .as_deref(),
            Some(""),
            "payee cleared"
        );
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    assert_eq!(e.db().live_transactions().count(), 1);
    assert!(
        e.db()
            .live_transactions()
            .all(|t| t.account_id == Some(chk)),
        "the saved row is on the account"
    );
}

#[test]
/// R-TXN-8: a transfer makes two rows, one out of the From account and one into the To account.
fn ui_add_transfer_creates_two_rows() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    t.account("Savings", AccountType::Savings);
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Account(chk));
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("+ Add Transaction").click();
        h.run();
        h.get_by_label("Transfer").click();
        h.run();
        open_picker(&mut h, "To Account");
        // "Savings" is also in the sidebar; the open list is drawn last, so take the last match.
        let options: Vec<_> = h.get_all_by_label("Savings").collect();
        options.last().expect("the Savings option").click();
        h.run();
        type_into(&mut h, "Amount", "100");
        h.run();
        h.get_by_label("Save").click();
        h.run();
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    assert_eq!(e.db().live_transactions().count(), 2, "two rows");
    assert!(
        e.db()
            .live_transactions()
            .any(|t| t.amount.to_string() == "-100")
    );
    assert!(
        e.db()
            .live_transactions()
            .any(|t| t.amount.to_string() == "100")
    );
}

#[test]
/// R-CAT-6: typing a payee that contains a category pattern selects that category.
fn ui_add_transaction_autocategorizes_payee() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    let transport = t.category("Transport", CategoryType::Expense);
    t.patterns(transport, &["uber"]);
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Account(chk));
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("+ Add Transaction").click();
        h.run();
        type_into(&mut h, "Payee", "Uber Trip");
        type_into(&mut h, "Amount", "18");
        h.run();
        h.get_by_label("Save").click();
        h.run();
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    let tx = e
        .db()
        .live_transactions()
        .find(|t| t.payee == "Uber Trip")
        .expect("saved row");
    assert_eq!(tx.category_id, Some(transport));
}

#[test]
/// R-CAT-5: "Rec" adds the typed payee to the chosen category's rules, then is disabled.
fn ui_rec_button_adds_pattern() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    let food = t.category("Food", CategoryType::Expense);
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Account(chk));
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("+ Add Transaction").click();
        h.run();
        open_picker(&mut h, "Category");
        h.get_by_label("Food").click();
        h.run();
        type_into(&mut h, "Payee", "Corner Cafe");
        h.run();
        h.get_by_label("Rec").click();
        h.run();
        assert!(
            h.get_by_label("Rec").accesskit_node().is_disabled(),
            "Rec disabled after adding"
        );
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    let cat = e.db().category(food).expect("category");
    assert!(
        cat.match_patterns
            .iter()
            .any(|p| p.eq_ignore_ascii_case("corner cafe")),
        "pattern added: {:?}",
        cat.match_patterns
    );
}

#[test]
/// ⌘N on an account screen opens New Transaction.
fn ui_cmd_n_opens_add_transaction() {
    let (mut app, _) = account_app();
    let mut h = harness(&mut app);
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::N);
    h.run();
    assert!(h.query_by_label("New Transaction").is_some());
}

#[test]
/// The sidebar "+" opens New Account; a name and Save add the account to the sidebar.
fn ui_add_account_via_sidebar_plus() {
    let mut t = TestDb::new();
    t.account("Everyday", AccountType::Checking);
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Settings);
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Add Account").click();
        h.run();
        type_into(&mut h, "Account Name", "Travel Fund");
        h.run();
        h.get_by_label("Save").click();
        h.run();
        assert!(h.query_by_label("Travel Fund").is_some(), "in the sidebar");
    }
}

#[test]
/// Save is disabled while the account name is blank.
fn ui_add_account_save_disabled_without_name() {
    let mut app = PfApp::new_for_test(engine_with(TestDb::new().build()));
    app.set_screen(Screen::Settings);
    let mut h = harness(&mut app);
    h.run();
    h.get_by_label("Add Account").click();
    h.run();
    assert!(h.get_by_label("Save").accesskit_node().is_disabled());
}

#[test]
/// R-ACC-2, R-ACC-3: with 200 of other rows, a current balance of 1000 calculates a starting balance
/// of 800; Save writes a "Starting Balance" row of 800 the day before the earliest row.
fn ui_edit_account_calculate_and_save_starting_balance() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    t.tx(chk, "2026-03-01", "120", "Pay one").done();
    t.tx(chk, "2026-03-02", "80", "Pay two").done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Settings);
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Everyday").click_secondary();
        h.run();
        h.get_by_label("Edit Account").click();
        h.run();
        replace_text(&mut h, "Current Balance", "1000");
        h.run();
        h.get_by_label("Calculate").click();
        h.run();
        assert_eq!(
            h.get_by_role_and_label(Role::TextInput, "Starting Balance")
                .value()
                .as_deref(),
            Some("800")
        );
        h.get_by_label("Save").click();
        h.run();
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    let start = e
        .db()
        .live_transactions()
        .find(|t| t.payee == "Starting Balance")
        .expect("starting balance row");
    assert_eq!(start.amount.to_string(), "800");
    assert_eq!(start.date.to_string(), "2026-02-28");
}

#[test]
/// Cancel in Edit Account discards the change to the name.
fn ui_edit_account_cancel_discards() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Settings);
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Everyday").click_secondary();
        h.run();
        h.get_by_label("Edit Account").click();
        h.run();
        type_into(&mut h, "Account Name", " Renamed");
        h.run();
        h.get_by_label("Cancel").click();
        h.run();
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    assert_eq!(e.db().account(chk).expect("account").name, "Everyday");
}
