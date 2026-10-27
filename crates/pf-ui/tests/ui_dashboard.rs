//! L5 UI behaviour tests for the dashboard (T14): row navigation, the available-balance toggle,
//! scenario numbers, delete with undo, and the empty database. Headless (egui_kittest); assertions
//! are made on engine state or on the money text the screen draws.

mod common;

use egui_kittest::kittest::Queryable;
use pf_engine::Engine;
use pf_model::testing::TestDb;
use pf_model::{AccountType, Db, ObligationStatus, TransactionStatus};
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
/// UI-NAV-02: clicking an account name on the dashboard opens its detail screen.
fn ui_nav_02_click_account_opens_detail() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    {
        let mut h = harness(&mut app);
        h.run();
        // The sidebar lists the same name first; the dashboard row is the last match.
        h.query_all_by_label("Everyday")
            .last()
            .expect("dashboard row")
            .click();
        h.run();
    }
    assert_eq!(app.screen(), Screen::Account(chk));
}

#[test]
/// UI-DSH-01: ticking "Show Available Balance (deduct obligations)" shows the reduced balances.
fn ui_dsh_01_available_toggle_changes_balances() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    t.tx(chk, "2026-01-05", "1000", "Pay")
        .status(TransactionStatus::Cleared)
        .done();
    t.obligation("Trip").account(chk).balance("300").done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    let mut h = harness(&mut app);
    h.run();
    assert_eq!(
        h.query_all_by_label("$700.00").count(),
        0,
        "reduced amount hidden before the toggle"
    );
    h.get_by_label("Show Available Balance (deduct obligations)")
        .click();
    h.run();
    assert!(
        h.query_all_by_label("$700.00").count() > 0,
        "reduced amount shown after the toggle"
    );
}

#[test]
/// UI-SCN-01: the dashboard numbers follow the active scenario; reality lines appear only then.
fn ui_scn_01_dashboard_numbers_follow_scenario() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    let plan = t.scenario("Plan A");
    t.tx(chk, "2026-01-05", "1000", "Pay")
        .status(TransactionStatus::Cleared)
        .done();
    t.tx(chk, "2026-01-06", "200", "Hold")
        .status(TransactionStatus::Pending)
        .done();
    t.tx(chk, "2026-01-07", "-300", "Plan spend")
        .status(TransactionStatus::Cleared)
        .scenario(plan)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    {
        let mut h = harness(&mut app);
        h.run();
        // Reality: cards (net worth, assets) and the Cleared column show $1,000.00; no reality line.
        assert_eq!(h.query_all_by_label("$1,000.00").count(), 3);
        assert_eq!(h.query_all_by_label("$900.00").count(), 0);
    }
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Reality").click();
        h.run();
        h.get_by_label("Plan A").click();
        h.run();
        // Plan A: cleared 700 (cards and Cleared column), all 900, plus reality lines 1,000 and 1,200.
        assert_eq!(h.query_all_by_label("$900.00").count(), 1, "All column");
        assert_eq!(
            h.query_all_by_label("$1,000.00").count(),
            1,
            "reality cleared line"
        );
        assert_eq!(
            h.query_all_by_label("$1,200.00").count(),
            1,
            "reality all line"
        );
    }
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Plan A").click();
        h.run();
        h.get_by_label("• Plan A").click();
        h.run();
        assert_eq!(
            h.query_all_by_label("$900.00").count(),
            0,
            "back in Reality"
        );
        assert_eq!(
            h.query_all_by_label("$1,000.00").count(),
            3,
            "reality line gone"
        );
    }
    assert_eq!(
        app.engine().lock().expect("lock").state.active_scenario,
        None
    );
}

#[test]
/// UI-DSH / R-ACC-1: Delete from the row menu asks for confirmation; Delete soft-deletes; Undo
/// restores the account.
fn ui_dashboard_delete_account_via_confirm() {
    let mut t = TestDb::new();
    let card = t.account("Old Card", AccountType::Credit);
    t.tx(card, "2026-01-05", "-50", "Coffee")
        .status(TransactionStatus::Cleared)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    let shared = app.engine();
    {
        let mut h = harness(&mut app);
        h.run();
        h.query_all_by_label("Old Card")
            .last()
            .expect("dashboard row")
            .click_secondary();
        h.run();
        h.get_by_label("Delete").click();
        h.run();
        assert!(
            h.query_by_label("Delete Account").is_some(),
            "confirmation shown"
        );
        h.get_by_label("Delete").click();
        h.run();
        assert_eq!(
            h.query_all_by_label("Old Card").count(),
            0,
            "gone from the sidebar and the dashboard"
        );
    }
    assert!(
        shared
            .lock()
            .expect("lock")
            .db()
            .account(card)
            .is_some_and(|a| a.is_deleted),
        "soft-deleted"
    );
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Edit").click();
        h.run();
        h.get_by_label("Undo Delete Account").click();
        h.run();
        assert!(h.query_all_by_label("Old Card").count() > 0, "restored");
    }
    assert!(
        shared
            .lock()
            .expect("lock")
            .db()
            .account(card)
            .is_some_and(|a| !a.is_deleted),
        "undo restores the account"
    );
}

#[test]
/// Empty engine: four cards at $0.00, no account groups, no panic.
fn ui_dashboard_empty_db_renders() {
    let mut app = PfApp::new_for_test(empty_engine());
    let mut h = harness(&mut app);
    h.run();
    for title in ["NET WORTH", "ASSETS", "CREDIT DEBT", "LOAN DEBT"] {
        assert!(h.query_by_label(title).is_some(), "card {title}");
    }
    assert_eq!(h.query_all_by_label("$0.00").count(), 4);
    assert!(h.query_by_label("Cash & Checking").is_none());
    assert!(h.query_by_label("Investments").is_none());
}

#[test]
/// R-BAL-6 with obligations of other statuses: a Completed obligation changes nothing.
fn ui_dsh_01_completed_obligation_is_ignored() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    t.tx(chk, "2026-01-05", "1000", "Pay")
        .status(TransactionStatus::Cleared)
        .done();
    t.obligation("Old")
        .account(chk)
        .balance("300")
        .status(ObligationStatus::Completed)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    let mut h = harness(&mut app);
    h.run();
    h.get_by_label("Show Available Balance (deduct obligations)")
        .click();
    h.run();
    assert_eq!(h.query_all_by_label("$700.00").count(), 0);
    assert!(h.query_all_by_label("$1,000.00").count() > 0);
}

#[test]
/// The Cleared and All headings sit over their figures: each heading's right edge is within a few
/// points of the figures' right edge in the same column.
fn ui_dsh_columns_line_up_with_headings() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    t.tx(chk, "2026-03-01", "1234.5", "Pay")
        .status(TransactionStatus::Cleared)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    let mut h = harness(&mut app);
    h.run();
    let cleared_head = h
        .query_all_by_value("Cleared")
        .last()
        .expect("Cleared heading")
        .rect();
    let all_head = h
        .query_all_by_value("All")
        .last()
        .expect("All heading")
        .rect();
    // The figures in the account row sit below the headings; the cards above also show this amount.
    let figures: Vec<_> = h
        .query_all_by_value("$1,234.50")
        .map(|n| n.rect())
        .filter(|r| r.top() > cleared_head.bottom())
        .collect::<Vec<_>>();
    // Each value is reported by two nodes a fraction of a point apart: keep one per figure.
    let mut kept: Vec<egui::Rect> = Vec::new();
    for r in figures {
        if kept.iter().all(|k| (k.left() - r.left()).abs() > 2.0) {
            kept.push(r);
        }
    }
    let figures = kept;
    assert_eq!(
        figures.len(),
        2,
        "one Cleared and one All figure in the row"
    );
    let (cleared_fig, all_fig) = (figures[0], figures[1]);
    assert!(
        (cleared_head.right() - cleared_fig.right()).abs() < 6.0,
        "Cleared heading {cleared_head:?} over figure {cleared_fig:?}"
    );
    assert!(
        (all_head.right() - all_fig.right()).abs() < 6.0,
        "All heading {all_head:?} over figure {all_fig:?}"
    );
    assert!(
        all_head.left() > cleared_head.left(),
        "All is the right-hand column"
    );
}

#[test]
/// An external account carries an EXTERNAL badge; its name is still the clickable label.
fn ui_dsh_external_account_has_badge() {
    let mut t = TestDb::new();
    t.external_account("Joint", AccountType::Checking);
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    let mut h = harness(&mut app);
    h.run();
    assert!(
        h.query_all_by_value("EXTERNAL").count() > 0,
        "external badge"
    );
    assert!(
        h.query_all_by_label("Joint").count() > 0,
        "name is clickable"
    );
}
