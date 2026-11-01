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

#[test]
/// A drill-down window is as tall as its rows need (up to a scroll limit), not the whole window: its
/// Close button sits well above the bottom edge.
fn ui_rpt_drill_window_is_not_full_height() {
    use egui::accesskit::Role;
    use egui_kittest::kittest::Queryable;
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    let groceries = t.category("Groceries", CategoryType::Expense);
    t.tx(chk, "2026-03-05", "-100", "Store")
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
    let window = egui::Vec2::new(1000.0, 800.0);
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Reports);
    let mut h = egui_kittest::Harness::builder()
        .with_size(window)
        .build_ui(|ui| app.ui_root(ui));
    h.run();
    click_button(&mut h, "-$40.00");
    h.run();
    let close = h.get_by_role_and_label(Role::Button, "Close").rect();
    assert!(
        close.bottom() < window.y * 0.6,
        "Close sits at {close:?}: the window grew to the full height"
    );
}

#[test]
/// Changing a transaction's category in a drill-down window moves it to that category, takes it out of
/// the window, and the Profit & Loss figure behind the window updates.
fn ui_rpt_drill_category_change_updates_window_and_report() {
    use egui::accesskit::Role;
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let groceries = t.category("Groceries", CategoryType::Expense);
    let dining = t.category("Dining", CategoryType::Expense);
    let store = t
        .tx(chk, "2026-03-05", "-100", "Store")
        .category(groceries)
        .status(TransactionStatus::Cleared)
        .done();
    t.tx(chk, "2026-02-20", "-50", "Market")
        .category(groceries)
        .status(TransactionStatus::Cleared)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Reports);
    {
        let mut h = harness(&mut app);
        h.run();
        click_button(&mut h, "-$150.00");
        h.run();
        assert!(shows(&h, "Store"), "the window lists Store");
        // Open the Store row's category picker and choose Dining (the last match is the open list).
        h.get_by_role_and_label(Role::ComboBox, "Category Store")
            .click();
        h.run();
        h.get_all_by_role_and_label(Role::Button, "Dining")
            .last()
            .expect("Dining in the picker")
            .click();
        h.run();
        h.run();
        assert!(!shows(&h, "Store"), "Store leaves the window");
    }
    assert_eq!(
        app.screen(),
        Screen::Reports,
        "a picker click does not navigate"
    );
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    assert_eq!(
        e.db().transaction(store).and_then(|x| x.category_id),
        Some(dining)
    );
    drop(e);
}

#[test]
/// Clicking a row in a drill-down window closes it and shows that transaction in its account, selected.
fn ui_rpt_drill_row_click_shows_transaction_in_account() {
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let groceries = t.category("Groceries", CategoryType::Expense);
    let store = t
        .tx(chk, "2026-03-05", "-100", "Store")
        .category(groceries)
        .status(TransactionStatus::Cleared)
        .done();
    t.tx(chk, "2026-02-20", "-50", "Market")
        .category(groceries)
        .status(TransactionStatus::Cleared)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Reports);
    {
        let mut h = harness(&mut app);
        h.run();
        click_button(&mut h, "-$150.00");
        h.run();
        h.get_all_by_value("Store")
            .last()
            .expect("the Store row")
            .click();
        h.run();
        h.run();
    }
    assert_eq!(app.screen(), Screen::Account(chk));
    assert_eq!(app.account_selection(), vec![store]);
}

#[test]
/// A row click on a transaction the time period leaves out widens the period to All, so it shows.
fn ui_rpt_drill_row_click_widens_period_when_needed() {
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let groceries = t.category("Groceries", CategoryType::Expense);
    let store = t
        .tx(chk, "2026-01-10", "-100", "Store")
        .category(groceries)
        .status(TransactionStatus::Cleared)
        .done();
    let mut engine = engine_with(t.build());
    engine
        .set_time_period(pf_model::TimePeriod::PastMonth)
        .expect("set period");
    let mut app = PfApp::new_for_test(engine);
    app.set_screen(Screen::Reports);
    {
        let mut h = harness(&mut app);
        h.run();
        // The 1Y quick range brings January into the report.
        click_button(&mut h, "1Y");
        h.run();
        click_button(&mut h, "-$100.00");
        h.run();
        h.get_all_by_value("Store")
            .last()
            .expect("the Store row")
            .click();
        h.run();
        h.run();
    }
    assert_eq!(app.screen(), Screen::Account(chk));
    assert_eq!(app.account_selection(), vec![store]);
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    assert_eq!(e.state.time_period, pf_model::TimePeriod::All);
}

/// Sends a wheel scroll over `pos` and lets it animate.
fn wheel(h: &mut egui_kittest::Harness<'_>, pos: egui::Pos2, delta: egui::Vec2) {
    h.event(egui::Event::PointerMoved(pos));
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta,
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Unknown,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(30);
}

/// A database with `n` categories, each with one February spend of $7.
fn many_categories_db(n: usize) -> Db {
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    for i in 0..n {
        let c = t.category(&format!("Cat {i:02}"), CategoryType::Expense);
        t.tx(chk, "2026-02-10", "-7", "Store")
            .category(c)
            .status(TransactionStatus::Cleared)
            .done();
    }
    t.build()
}

#[test]
/// R-RPT-5d: the matrix's month headings stay in place while its rows scroll, and the last row is
/// reachable.
fn ui_rpt_matrix_header_stays_when_scrolling_down() {
    let mut app = PfApp::new_for_test(engine_with(many_categories_db(60)));
    app.set_screen(Screen::Reports);
    let mut h = harness(&mut app);
    h.run();
    h.get_by_label("Monthly Matrix").click();
    h.run();
    let header = h.get_by_label("Jan").rect();
    let first = h.get_by_label("Cat 00").rect();
    assert!(!shows(&h, "NET"), "the last row starts below the window");
    wheel(&mut h, first.center(), egui::Vec2::new(0.0, -5000.0));
    assert!(shows(&h, "NET"), "the rows scrolled to the end");
    assert!(
        !shows(&h, "Cat 00"),
        "the first row scrolled out of the window"
    );
    let after = h.get_by_label("Jan").rect();
    assert!(
        (after.min.y - header.min.y).abs() < 0.5,
        "the heading moved: {} to {}",
        header.min.y,
        after.min.y
    );
    let net = h.get_by_label("NET").rect();
    assert!(net.min.y > after.max.y, "NET is drawn below the heading");
}

#[test]
/// The matrix still scrolls sideways: the headings move with their columns and stay lined up with the
/// amounts under them.
fn ui_rpt_matrix_header_moves_with_columns_sideways() {
    let mut app = PfApp::new_for_test(engine_with(report_db()));
    app.set_screen(Screen::Reports);
    let mut h = harness(&mut app);
    h.run();
    h.get_by_label("Monthly Matrix").click();
    h.run();
    pick_cutoff(&mut h, "Dec");
    let before = h.get_by_label("Feb").rect();
    let cell = h.get_by_role_and_label(Role::Button, "-$50.00").rect();
    assert!(
        (before.max.x - cell.max.x).abs() < 12.0,
        "Feb heading {} and its amount {} line up",
        before.max.x,
        cell.max.x
    );
    let over_table = h.get_by_label("Groceries").rect().center();
    wheel(&mut h, over_table, egui::Vec2::new(-5000.0, 0.0));
    let after = h.get_by_label("Feb").rect();
    let cell_after = h.get_by_role_and_label(Role::Button, "-$50.00").rect();
    assert!(after.min.x < before.min.x, "the heading scrolled sideways");
    assert!(
        (after.max.x - cell_after.max.x).abs() < 12.0,
        "still lined up after scrolling"
    );
    assert!(shows(&h, "Total"), "the last column is reachable");
    let total = h.get_by_label("Total").rect();
    assert!(total.max.x <= 1200.0, "the Total heading is on screen");
}

#[test]
/// R-RPT-5d: a three-line cell fits in its row, in the default Actual + Budget mode.
fn ui_rpt_matrix_three_line_cell_fits_its_row() {
    let mut app = PfApp::new_for_test(engine_with(report_db()));
    app.set_screen(Screen::Reports);
    let mut h = harness(&mut app);
    h.run();
    h.get_by_label("Monthly Matrix").click();
    h.run();
    // March's actual is -$100, its budget -$40, its difference -$60; the Groceries row comes first.
    let diff = h
        .get_all_by_label("-$60.00")
        .map(|n| n.rect())
        .min_by(|a, b| a.min.y.total_cmp(&b.min.y))
        .expect("the difference line");
    let net = h.get_by_label("NET").rect();
    assert!(
        diff.max.y <= net.min.y + 3.0,
        "the third line ({}) runs into the next row ({})",
        diff.max.y,
        net.min.y
    );
}

#[test]
/// R-RPT-3b: a report's transactions window shows the total of its rows, and the total follows when a
/// category change takes a row out.
fn ui_rpt_drill_total_follows_category_change() {
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let groceries = t.category("Groceries", CategoryType::Expense);
    t.category("Dining", CategoryType::Expense);
    let mut ids = Vec::new();
    for (payee, amount) in [("Store", "-100"), ("Market", "-50"), ("Deli", "-25")] {
        ids.push(
            t.tx(chk, "2026-03-05", amount, payee)
                .category(groceries)
                .status(TransactionStatus::Cleared)
                .done(),
        );
    }
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Settings);
    app.open_dialog_for_test(Dialog::ReportTransactions {
        title: "Groceries Transactions".to_owned(),
        ids,
    });
    let mut h = harness(&mut app);
    h.run();
    assert!(shows(&h, "Total"));
    assert!(shows(&h, "-$175.00"), "the total of the three rows");
    h.get_by_role_and_label(Role::ComboBox, "Category Store")
        .click();
    h.run();
    h.get_all_by_role_and_label(Role::Button, "Dining")
        .last()
        .expect("Dining in the picker")
        .click();
    h.run();
    h.run();
    assert!(!shows(&h, "-$175.00"), "the old total is gone");
    assert!(shows(&h, "-$75.00"), "the total of the two rows left");
}

#[test]
/// R-RPT-3b: the drill-down total sits under the Amount column, right-aligned with the amounts, and a
/// long memo does not push it or the window off to the right.
fn ui_rpt_drill_total_lines_up_with_amounts() {
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let groceries = t.category("Groceries", CategoryType::Expense);
    let long_memo = "a very long memo that runs on ".repeat(12);
    let ids = vec![
        t.tx(chk, "2026-03-05", "-100", "Store")
            .category(groceries)
            .memo(&long_memo)
            .status(TransactionStatus::Cleared)
            .done(),
        t.tx(chk, "2026-03-06", "-50", "Market")
            .category(groceries)
            .status(TransactionStatus::Cleared)
            .done(),
    ];
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Settings);
    app.open_dialog_for_test(Dialog::ReportTransactions {
        title: "Groceries Transactions".to_owned(),
        ids,
    });
    let mut h = harness(&mut app);
    h.run();
    h.run();
    let amount = h.get_by_label("-$100.00").rect();
    let total = h.get_by_label("-$150.00").rect();
    let status = h.get_by_label("Status").rect();
    assert!(
        (total.max.x - amount.max.x).abs() < 1.0,
        "the total's right edge {} is under the amounts' {}",
        total.max.x,
        amount.max.x
    );
    assert!(total.min.y > amount.max.y, "the total is below the rows");
    assert!(status.max.x <= 1200.0, "the table stays on screen");
}

/// The on-screen rectangle of the first button labelled `label` (a NET cell of the same amount is a
/// plain label, so this finds the category row's).
fn button_rect(h: &egui_kittest::Harness<'_>, label: &str) -> egui::Rect {
    h.get_by_role_and_label(Role::Button, label).rect()
}

#[test]
/// R-RPT-5d: the category names and their heading stay in place while the months scroll sideways, also
/// when the swipe is over the names.
fn ui_rpt_matrix_first_column_stays_when_scrolling_sideways() {
    let mut app = PfApp::new_for_test(engine_with(report_db()));
    app.set_screen(Screen::Reports);
    let mut h = harness(&mut app);
    h.run();
    h.get_by_label("Monthly Matrix").click();
    h.run();
    pick_cutoff(&mut h, "Dec");
    let name = h.get_by_label("Groceries").rect();
    let heading = h.get_by_label("Category").rect();
    let month = h.get_by_label("Feb").rect();
    let over_cell = button_rect(&h, "-$50.00").center();
    wheel(&mut h, over_cell, egui::Vec2::new(-300.0, 0.0));
    let after = h.get_by_label("Feb").rect();
    assert!(after.min.x < month.min.x, "the months scrolled");
    assert_eq!(h.get_by_label("Groceries").rect().min.x, name.min.x);
    assert_eq!(h.get_by_label("Category").rect().min.x, heading.min.x);
    wheel(&mut h, name.center(), egui::Vec2::new(-100.0, 0.0));
    assert!(
        h.get_by_label("Feb").rect().min.x < after.min.x,
        "a swipe over the names moves the months too"
    );
    assert_eq!(h.get_by_label("Groceries").rect().min.x, name.min.x);
}

#[test]
/// The frozen names scroll down with their rows, whichever side the wheel is over.
fn ui_rpt_matrix_names_and_amounts_scroll_together() {
    // Row "Cat nn" has its only amount, -$7.00, in February; the NET row's February is -$420.00.
    let mut app = PfApp::new_for_test(engine_with(many_categories_db(60)));
    app.set_screen(Screen::Reports);
    let mut h = harness(&mut app);
    h.run();
    h.get_by_label("Monthly Matrix").click();
    h.run();
    let over_amounts = h
        .get_all_by_role_and_label(Role::Button, "-$7.00")
        .next()
        .expect("a cell")
        .rect()
        .center();
    wheel(&mut h, over_amounts, egui::Vec2::new(0.0, -5000.0));
    let net = h.get_by_label("NET").rect();
    let net_cell = h
        .get_all_by_label("-$420.00")
        .next()
        .expect("NET's February")
        .rect();
    assert!(
        (net.center().y - net_cell.center().y).abs() < 8.0,
        "wheel over the amounts: NET {} and its cell {} share a row",
        net.center().y,
        net_cell.center().y
    );
    wheel(&mut h, net.center(), egui::Vec2::new(0.0, 5000.0));
    assert!(
        shows(&h, "Cat 00"),
        "a wheel over the names scrolls back up"
    );
    let name = h.get_by_label("Cat 00").rect();
    let cell = h
        .get_all_by_role_and_label(Role::Button, "-$7.00")
        .next()
        .expect("a cell")
        .rect();
    assert!(
        (name.center().y - cell.center().y).abs() < 8.0,
        "wheel over the names: Cat 00 {} and its cell {} share a row",
        name.center().y,
        cell.center().y
    );
}

/// True when `text` is drawn at least once (a row's Total and NET can show the same amount).
fn appears(h: &egui_kittest::Harness<'_>, text: &str) -> bool {
    h.query_all_by_label(text).next().is_some() || h.query_all_by_value(text).next().is_some()
}

/// Picks `month` in the matrix's "Budget total cutoff" menu.
fn pick_cutoff(h: &mut egui_kittest::Harness<'_>, month: &str) {
    h.get_by_role_and_label(Role::ComboBox, "Budget total cutoff")
        .click();
    h.run();
    // A heading of the same name is a plain label; the menu's entry is a button.
    h.get_all_by_role_and_label(Role::Button, month)
        .last()
        .expect("the month in the menu")
        .click();
    h.run();
    // The harness's click is not a pointer click, so the menu does not close by itself as it does for
    // a real click.
    h.key_press(egui::Key::Escape);
    h.run();
    h.run();
}

/// True when the matrix draws the heading of `month`: outside the open menu, only a heading has it.
fn month_shown(h: &egui_kittest::Harness<'_>, month: &str) -> bool {
    h.query_by_label(month).is_some()
}

/// Groceries spends in February and in May, with a monthly budget of 40 (the test clock is March 2026).
fn cutoff_db() -> Db {
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let groceries = t.category("Groceries", CategoryType::Expense);
    t.tx(chk, "2026-02-20", "-50", "Market")
        .category(groceries)
        .status(TransactionStatus::Cleared)
        .done();
    t.tx(chk, "2026-05-05", "-100", "Store")
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

#[test]
/// R-RPT-5f: the matrix starts at the current month: later months are not shown or totalled. Picking a
/// later cutoff brings them in.
fn ui_rpt_matrix_cutoff_hides_and_excludes_later_months() {
    let mut app = PfApp::new_for_test(engine_with(cutoff_db()));
    app.set_screen(Screen::Reports);
    let mut h = harness(&mut app);
    h.run();
    h.get_by_label("Monthly Matrix").click();
    h.run();
    assert!(appears(&h, "Budget total cutoff"));
    assert!(!month_shown(&h, "Apr"), "a month after March is not shown");
    // January to March: actual -50, budget -120, difference +70.
    assert!(appears(&h, "-$120.00"), "the budget total stops at March");
    assert!(appears(&h, "$70.00"), "so does the difference");
    assert!(!appears(&h, "-$480.00"), "not the whole year's budget");
    assert!(!appears(&h, "-$150.00"), "May's spend is not in the total");

    pick_cutoff(&mut h, "May");
    assert!(month_shown(&h, "Apr"), "April is shown now");
    assert!(!month_shown(&h, "Jun"), "June is not");
    assert!(appears(&h, "-$150.00"), "May's spend is in the total now");
    assert!(appears(&h, "-$200.00"), "five months of budget");

    pick_cutoff(&mut h, "Dec");
    assert!(month_shown(&h, "Nov"), "the whole year is shown");
    assert!(appears(&h, "-$480.00"), "the whole year's budget");
}

#[test]
/// R-RPT-5f: an active scenario's later months are cut off too, until a later cutoff is picked.
fn ui_rpt_matrix_cutoff_applies_to_scenarios() {
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let fun = t.category("Fun", CategoryType::Expense);
    let plan = t.scenario("Plan");
    t.tx(chk, "2026-02-01", "-3", "Now").category(fun).done();
    t.tx(chk, "2026-09-01", "-777", "Trip")
        .category(fun)
        .scenario(plan)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Reports);
    {
        let shared = app.engine();
        shared.lock().expect("engine lock").state.active_scenario = Some(plan);
    }
    let mut h = harness(&mut app);
    h.run();
    h.get_by_label("Monthly Matrix").click();
    h.run();
    assert!(!appears(&h, "-$777.00"), "September is past the cutoff");
    assert!(!appears(&h, "-$780.00"), "and not in the total");
    pick_cutoff(&mut h, "Sep");
    assert!(appears(&h, "-$777.00"), "the scenario's month is back");
    assert!(appears(&h, "-$780.00"), "and in the total");
}

#[test]
/// R-RPT-5f: another year starts from its own default, December for a past year.
fn ui_rpt_matrix_cutoff_resets_with_the_year() {
    let mut app = PfApp::new_for_test(engine_with(cutoff_db()));
    app.set_screen(Screen::Reports);
    let mut h = harness(&mut app);
    h.run();
    h.get_by_label("Monthly Matrix").click();
    h.run();
    pick_cutoff(&mut h, "Feb");
    assert!(!month_shown(&h, "Mar"), "March is past the cutoff");
    h.get_by_role_and_label(Role::ComboBox, "Matrix Year")
        .click();
    h.run();
    h.get_all_by_label("2025").last().expect("2025").click();
    h.run();
    h.run();
    assert!(month_shown(&h, "Nov"), "a past year shows all its months");
}

#[test]
/// R-RPT-5f: a Total cell opens only the transactions it adds up.
fn ui_rpt_matrix_total_drill_stops_at_cutoff() {
    let mut app = PfApp::new_for_test(engine_with(cutoff_db()));
    app.set_screen(Screen::Reports);
    let mut h = harness(&mut app);
    h.run();
    h.get_by_label("Monthly Matrix").click();
    h.run();
    // February's -$50.00 is also the row's Total: the last button is the Total cell.
    h.get_all_by_role_and_label(Role::Button, "-$50.00")
        .last()
        .expect("the Total cell")
        .click();
    h.run();
    h.run();
    assert!(shows(&h, "Groceries 2026"), "the Total's drill-down");
    assert!(shows(&h, "Market"), "February's spend is listed");
    assert!(!shows(&h, "Store"), "May's spend is past the cutoff");
}
