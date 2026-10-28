//! L5 UI tests for the Contexts screen and editor (T20): archiving moves a context to its section,
//! a refused delete shows the engine's message, a delete removes the tag (undoable), the count, and
//! the default colour. Headless (egui_kittest); assertions are on engine state and on what is drawn.

mod common;

use egui::accesskit::Role;
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

#[test]
/// UI-CTX-01, R-CTX-3: archiving moves "Vacation" under Archived with the pill; unarchiving moves it
/// back.
fn ui_ctx_01_archive_moves_section() {
    let mut t = TestDb::new();
    t.context("Vacation");
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Contexts);
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Vacation").click_secondary();
        h.run();
        h.get_by_label("Archive").click();
        h.run();
        assert!(h.query_by_label("Archived").is_some(), "archived section");
        assert!(h.query_by_label("archived").is_some(), "archived pill");
    }
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Vacation").click_secondary();
        h.run();
        h.get_by_label("Unarchive").click();
        h.run();
        assert!(h.query_by_label("Archived").is_none(), "section gone again");
    }
}

#[test]
/// R-CTX-1: a context used by an obligation cannot be deleted. The alert gives the engine's message,
/// and the context is still there.
fn ui_ctx_delete_refused_shows_alert() {
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let ctx = t.context("down-payment");
    t.obligation("Down Payment")
        .context(ctx)
        .account(chk)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Contexts);
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("down-payment").click_secondary();
        h.run();
        h.get_by_label("Delete").click();
        h.run();
        assert!(h.query_by_label("Cannot Delete").is_some(), "alert title");
        assert!(
            h.query_by_label_contains("Cannot delete context '#down-payment'")
                .is_some(),
            "engine message"
        );
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    assert!(
        e.db().context(ctx).is_some_and(|c| !c.is_deleted),
        "still there"
    );
}

#[test]
/// R-CTX-2: deleting an unused context removes it from transactions; Cmd-Z restores it.
fn ui_ctx_delete_removes_from_transactions() {
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let trip = t.context("trip");
    let tx = t
        .tx(chk, "2026-03-01", "-4", "Hotel")
        .contexts(&[trip])
        .status(TransactionStatus::Cleared)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Contexts);
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("trip").click_secondary();
        h.run();
        h.get_by_label("Delete").click();
        h.run();
    }
    {
        let shared = app.engine();
        let e = shared.lock().expect("engine lock");
        assert!(e.db().transaction(tx).expect("tx").context_ids.is_empty());
    }
    {
        let mut h = harness(&mut app);
        h.run();
        h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
        h.run();
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    assert_eq!(e.db().transaction(tx).expect("tx").context_ids, vec![trip]);
}

#[test]
/// The count beside each context is the number of live transactions carrying it.
fn ui_ctx_count_column() {
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let trip = t.context("trip");
    t.tx(chk, "2026-03-01", "-4", "Hotel")
        .contexts(&[trip])
        .done();
    t.tx(chk, "2026-03-02", "-9", "Train")
        .contexts(&[trip])
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Contexts);
    let mut h = harness(&mut app);
    h.run();
    assert!(h.query_by_label("2").is_some(), "count of two");
}

#[test]
/// R-CTX-4: a new context saved without touching the colour is blue.
fn ui_ctx_create_default_blue() {
    let mut app = PfApp::new_for_test(engine_with(TestDb::new().build()));
    app.set_screen(Screen::Contexts);
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("+ Add Context").click();
        h.run();
        h.get_by_role_and_label(Role::TextInput, "Context Name")
            .click();
        h.run();
        h.get_by_role_and_label(Role::TextInput, "Context Name")
            .type_text("Trip");
        h.run();
        h.get_by_label("Create").click();
        h.run();
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    let c = e
        .db()
        .live_contexts()
        .find(|c| c.name == "Trip")
        .expect("context");
    assert_eq!(c.color_hex.as_deref(), Some("#0000ff"));
}

#[test]
/// The editor of a context lists its transactions with their accounts, and the total underneath. The
/// count on the Contexts screen is next to the name.
fn ui_ctx_edit_lists_transactions_and_total() {
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let card = t.account("Card", AccountType::Credit);
    let trip = t.context("Trip");
    t.tx(chk, "2026-03-01", "-40", "Hotel")
        .contexts(&[trip])
        .status(TransactionStatus::Cleared)
        .done();
    t.tx(card, "2026-03-05", "-12.50", "Taxi")
        .contexts(&[trip])
        .status(TransactionStatus::Cleared)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Contexts);
    let mut h = harness(&mut app);
    h.run();
    h.get_by_label("Trip").click();
    h.run();
    assert!(
        h.query_all_by_value("Transactions (2)").next().is_some(),
        "lists two transactions"
    );
    assert!(
        h.query_all_by_value("Card").next().is_some(),
        "shows the Card account"
    );
    assert!(
        h.query_all_by_value("Checking").next().is_some(),
        "shows the Checking account"
    );
    assert!(
        h.query_all_by_value("-$52.50").next().is_some(),
        "shows the total"
    );
}
