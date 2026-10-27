//! L5 UI tests for the account search bar (T15c): the "Find" toggle, the search text, the result
//! count and ⌘F. Headless (egui_kittest); assertions are on what the screen draws.

mod common;

use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable;
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

/// An account with two payees, "Coffee Shop" and "Groceries".
fn two_payee_app() -> (PfApp, pf_model::AccountId) {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    t.tx(chk, "2026-03-01", "-4.50", "Coffee Shop")
        .status(TransactionStatus::Cleared)
        .done();
    t.tx(chk, "2026-03-02", "-40", "Groceries")
        .status(TransactionStatus::Cleared)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Account(chk));
    (app, chk)
}

#[test]
/// UI-TXN-04, R-FLT-3: "Find" opens the bar; typing "coff" in "Search" shows only the matching
/// payee and the count "1 results" (Swift wording).
fn ui_txn_04_search_filters_and_shows_count() {
    let (mut app, _) = two_payee_app();
    let mut h = harness(&mut app);
    h.run();
    h.get_by_label("Find").click();
    h.run();
    h.get_by_label("Search").type_text("coff");
    h.run();
    assert!(
        h.query_by_label("Payee Coffee Shop").is_some(),
        "match shown"
    );
    assert!(
        h.query_by_label("Payee Groceries").is_none(),
        "non-match hidden"
    );
    assert!(h.query_by_label("1 results").is_some(), "result count");
}

#[test]
/// UI-TXN-04: ⌘F opens the bar and closes it again, which also clears its text.
fn ui_txn_04_cmd_f_toggles_search() {
    let (mut app, _) = two_payee_app();
    {
        let mut h = harness(&mut app);
        h.run();
        assert!(h.query_by_label("Search").is_none(), "closed at first");
        h.key_press_modifiers(Modifiers::COMMAND, Key::F);
        h.run();
        assert!(h.query_by_label("Search").is_some(), "opened by ⌘F");
        h.get_by_label("Search").type_text("coff");
        h.run();
        assert!(h.query_by_label("Payee Groceries").is_none());
    }
    {
        let mut h = harness(&mut app);
        h.run();
        h.key_press_modifiers(Modifiers::COMMAND, Key::F);
        h.run();
        assert!(h.query_by_label("Search").is_none(), "closed by ⌘F again");
        assert!(
            h.query_by_label("Payee Groceries").is_some(),
            "text cleared, all rows back"
        );
    }
}
