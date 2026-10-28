//! L5 UI tests for the Reports screen (T23): each report renders, the quick ranges, the drill-downs from
//! the Profit & Loss and the monthly matrix, the balance sheet's obligations, and the scenario column.
//! Headless (egui_kittest); assertions are on what the screen draws and on the dialogs it opens.

mod common;

use egui::accesskit::Role;
use egui_kittest::kittest::Queryable;
use pf_engine::Engine;
use pf_model::testing::TestDb;
use pf_model::{AccountType, BudgetFrequency, BudgetItemType, CategoryType, Db, TransactionStatus};
use pf_ui::PfApp;
use pf_ui::dialogs::Dialog;
use pf_ui::screen::Screen;

use common::{empty_engine, harness};

/// Engine holding `db`, on a temp file (never the app's database).
fn engine_with(db: Db) -> Engine {
    let mut e = empty_engine();
    e.replace_db(db)
        .expect("write the test database to the temp file");
    e
}

/// Checking, a Groceries category, a Groceries spend of 100 on 2026-03-05 and of 50 on 2026-02-10, and a
/// monthly Groceries budget item of 40 starting in January.
fn report_db() -> Db {
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let groceries = t.category("Groceries", CategoryType::Expense);
    t.tx(chk, "2026-03-05", "-100", "Store")
        .category(groceries)
        .status(TransactionStatus::Cleared)
        .done();
    t.tx(chk, "2026-02-20", "-50", "Market")
        .category(groceries)
        .status(TransactionStatus::Cleared)
        .done();
    t.budget_item(
        BudgetItemType::Expense,
        "Groceries",
        "40",
        "2026-01-01",
        BudgetFrequency::Monthly,
    )
    .category(groceries)
    .done();
    t.build()
}

/// True when the screen shows `text`: as a button or heading (its accessible name), or as a plain label
/// (its value, since plain labels carry their text as a value).
fn shows(h: &egui_kittest::Harness<'_>, text: &str) -> bool {
    h.query_by_label(text).is_some() || h.query_by_value(text).is_some()
}

/// Clicks the first button labelled `label` (the first match is the left-most, drawn first).
fn click_button(h: &mut egui_kittest::Harness<'_>, label: &str) {
    let first: Vec<_> = h.get_all_by_role_and_label(Role::Button, label).collect();
    first.first().expect("a button with that label").click();
    h.run();
}

#[test]
/// UI-RPT-01: each of the three reports renders and shows its signature label.
fn ui_rpt_01_each_report_renders() {
    let mut app = PfApp::new_for_test(engine_with(report_db()));
    app.set_screen(Screen::Reports);
    let mut h = harness(&mut app);
    h.run();
    assert!(shows(&h, "Total"), "P&L total");
    h.get_by_label("Balance Sheet").click();
    h.run();
    assert!(shows(&h, "Net Worth"), "balance sheet");
    h.get_by_label("Monthly Matrix").click();
    h.run();
    assert!(shows(&h, "NET"), "matrix net row");
}

#[test]
/// R-RPT-6: YTD sets the start to January 1 of this year and the end to today.
fn ui_rpt_quick_select_sets_range() {
    let mut app = PfApp::new_for_test(engine_with(report_db()));
    app.set_screen(Screen::Reports);
    let mut h = harness(&mut app);
    h.run();
    click_button(&mut h, "YTD");
    assert!(shows(&h, "2026-01-01"), "start is Jan 1");
    assert!(shows(&h, "2026-03-15"), "end is today");
}

#[test]
/// R-RPT-3: the actual figure of a category opens the transactions behind it.
fn ui_rpt_pl_actual_click_opens_drilldown() {
    let mut app = PfApp::new_for_test(engine_with(report_db()));
    app.set_screen(Screen::Reports);
    {
        let mut h = harness(&mut app);
        h.run();
        click_button(&mut h, "-$150.00");
        assert!(shows(&h, "Groceries Transactions"), "drill-down heading");
        assert!(shows(&h, "Store"), "lists the transaction");
    }
}

#[test]
/// R-RPT-1: the budgeted figure opens the budget items behind it.
fn ui_rpt_pl_budget_click_opens_budget_items() {
    let mut app = PfApp::new_for_test(engine_with(report_db()));
    app.set_screen(Screen::Reports);
    {
        let mut h = harness(&mut app);
        h.run();
        let budgeted: Vec<_> = h
            .get_all_by_role_and_label(Role::Button, "-$40.00")
            .collect();
        budgeted.first().expect("a budgeted figure").click();
        h.run();
        assert!(shows(&h, "Groceries Budget Items"), "drill-down heading");
    }
}

#[test]
/// R-OBL-6, R-RPT-4: a funding obligation shows its own balance, not the balance with its tagged rows.
fn ui_rpt_bs_obligation_uses_current_balance() {
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let ctx = t.context("fund");
    t.obligation("Fund")
        .balance("100")
        .context(ctx)
        .account(chk)
        .done();
    t.tx(chk, "2026-03-01", "50", "Deposit")
        .contexts(&[ctx])
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Reports);
    let mut h = harness(&mut app);
    h.run();
    h.get_by_label("Balance Sheet").click();
    h.run();
    assert!(shows(&h, "Total Obligations"), "obligations section");
    assert!(!shows(&h, "$150.00"), "not the computed balance");
    assert!(
        h.query_all_by_value("$100.00").count() > 0,
        "the funded balance"
    );
}

#[test]
/// R-RPT-4: with no obligations there is no obligations total.
fn ui_rpt_bs_hides_obligations_when_none() {
    let mut app = PfApp::new_for_test(engine_with(report_db()));
    app.set_screen(Screen::Reports);
    let mut h = harness(&mut app);
    h.run();
    h.get_by_label("Balance Sheet").click();
    h.run();
    assert!(!shows(&h, "Total Obligations"));
}

#[test]
/// R-RPT-5: the matrix's mode can be changed; in "Actual Only" the grid still shows its net row.
fn ui_rpt_matrix_mode_switch() {
    let mut app = PfApp::new_for_test(engine_with(report_db()));
    app.set_screen(Screen::Reports);
    let mut h = harness(&mut app);
    h.run();
    h.get_by_label("Monthly Matrix").click();
    h.run();
    h.get_by_role_and_label(Role::ComboBox, "Matrix Mode")
        .click();
    h.run();
    let options: Vec<_> = h.get_all_by_label("Actual Only").collect();
    options.last().expect("option").click();
    h.run();
    assert!(shows(&h, "NET"));
}

#[test]
/// R-RPT-5: a matrix cell opens the transactions of its category and month, with that heading.
fn ui_rpt_matrix_cell_drilldown_title() {
    let mut app = PfApp::new_for_test(engine_with(report_db()));
    app.set_screen(Screen::Reports);
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Monthly Matrix").click();
        h.run();
        click_button(&mut h, "-$50.00");
        assert!(
            h.query_by_label_contains("Feb 2026").is_some(),
            "month in the heading"
        );
    }
}

#[test]
/// The Scenario column appears only while a scenario is active.
fn ui_rpt_scenario_column_only_when_active() {
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let plan = t.scenario("Plan A");
    let tx = t
        .tx(chk, "2026-03-05", "-9", "Snack")
        .status(TransactionStatus::Future)
        .scenario(plan)
        .done();
    let db = t.build();
    let mut app = PfApp::new_for_test(engine_with(db));
    app.set_screen(Screen::Reports);
    app.open_dialog_for_test(Dialog::ReportTransactions {
        title: "Check".into(),
        ids: vec![tx],
    });
    {
        let mut h = harness(&mut app);
        h.run();
        assert!(!shows(&h, "Scenario"), "no scenario column in Reality");
    }
    app.engine()
        .lock()
        .expect("engine lock")
        .set_active_scenario(Some(plan));
    let mut h = harness(&mut app);
    h.run();
    assert!(shows(&h, "Scenario"), "scenario column while active");
}

#[test]
/// R-RPT-4: the balance sheet's Pending figures sit in their own column next to Balance, not at the
/// frame's right edge.
fn ui_rpt_bs_pending_column_next_to_balance() {
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    t.tx(chk, "2026-03-01", "100", "Pay")
        .status(TransactionStatus::Cleared)
        .done();
    t.tx(chk, "2026-03-02", "-264.66", "Chase")
        .status(TransactionStatus::Pending)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Reports);
    let mut h = harness(&mut app);
    h.run();
    h.get_by_label("Balance Sheet").click();
    h.run();
    // The first match of each figure is the account row (the total rows and net worth come later).
    let balance = h
        .query_all_by_value("-$164.66")
        .next()
        .expect("balance figure");
    let pending = h
        .query_all_by_value("-$264.66")
        .next()
        .expect("pending figure");
    let gap = pending.rect().right() - balance.rect().right();
    assert!(
        gap.abs() < 160.0,
        "pending figure is {gap} points from the balance figure's right edge"
    );
}

#[test]
/// R-RPT-4d: the Balance Sheet's Net Worth says it includes pending and future transactions, since the
/// Dashboard's Net Worth does not.
fn ui_rpt_bs_net_worth_note_mentions_pending() {
    let mut app = PfApp::new_for_test(engine_with(report_db()));
    app.set_screen(Screen::Reports);
    let mut h = harness(&mut app);
    h.run();
    h.get_by_label("Balance Sheet").click();
    h.run();
    assert!(
        shows(&h, "Includes pending and future transactions."),
        "net worth note"
    );
}

/// Opens the drill-down of the P&L figure `figure` in an 800-point window, where the names are long.
/// Returns the window's heading rectangle and the rectangle of its last column header (`last`).
fn narrow_drill(figure: &str, last: &str) -> (egui::Rect, egui::Rect, egui::Vec2) {
    use egui_kittest::kittest::Queryable;
    let mut t = TestDb::new();
    let chk = t.account(
        "A very long checking account name here",
        AccountType::Checking,
    );
    let groceries = t.category(
        "Groceries and household supplies for the whole family",
        CategoryType::Expense,
    );
    for i in 0..3 {
        t.tx(
            chk,
            "2026-03-05",
            "-100",
            &format!("A very long payee name number {i} indeed"),
        )
        .category(groceries)
        .status(TransactionStatus::Cleared)
        .done();
    }
    t.budget_item(
        BudgetItemType::Expense,
        "A very long budget item name for the test",
        "40",
        "2026-01-01",
        BudgetFrequency::Monthly,
    )
    .category(groceries)
    .done();
    let window = egui::Vec2::new(800.0, 640.0);
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Reports);
    let mut h = egui_kittest::Harness::builder()
        .with_size(window)
        .build_ui(|ui| app.ui_root(ui));
    h.run();
    click_button(&mut h, figure);
    h.run();
    let heading = h
        .query_all_by_value("Groceries and household supplies for the whole family Transactions")
        .chain(h.query_all_by_value(
            "Groceries and household supplies for the whole family Budget Items",
        ))
        .next()
        .expect("drill-down heading")
        .rect();
    let last_header = h
        .query_all_by_value(last)
        .next()
        .expect("last column header")
        .rect();
    (heading, last_header, window)
}

#[test]
/// The Actual drill-down stays inside a narrow window even with long names: a wider table would push
/// the dialog and its text off screen.
fn ui_rpt_actual_drill_fits_narrow_window() {
    let (heading, last, window) = narrow_drill("-$300.00", "Status");
    assert!(
        heading.left() >= 0.0,
        "heading starts inside the window: {heading:?}"
    );
    assert!(
        last.right() <= window.x,
        "last column ends inside the window: {last:?}"
    );
}

#[test]
/// The Budgeted drill-down stays inside a narrow window even with long names.
fn ui_rpt_budgeted_drill_fits_narrow_window() {
    let (heading, last, window) = narrow_drill("-$40.00", "Calculated Total (In Window)");
    assert!(
        heading.left() >= 0.0,
        "heading starts inside the window: {heading:?}"
    );
    assert!(
        last.right() <= window.x,
        "last column ends inside the window: {last:?}"
    );
}
