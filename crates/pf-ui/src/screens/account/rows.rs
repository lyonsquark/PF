//! Row data for the account transaction table (T15a). `build_rows` is pure and cached by the
//! screen per `RowsKey`; it is never called from the table's row closure.

use std::collections::HashMap;

use jiff::civil::Date;
use pf_core::filter::SearchState;
use pf_core::{balance, categorize, filter};
use pf_engine::AppState;
use pf_engine::commands::transactions::is_locked;
use pf_model::{
    AccountId, CategoryId, ContextId, Db, ScenarioId, TimePeriod, Transaction, TransactionId,
    TransactionStatus,
};
use rust_decimal::Decimal;

/// One table row, fully resolved for drawing (no lookups needed while painting).
#[derive(Debug, Clone, PartialEq)]
pub struct TxRow {
    /// Transaction id.
    pub id: TransactionId,
    /// Calendar date of the movement.
    pub date: Date,
    /// Payee text.
    pub payee: String,
    /// Signed amount.
    pub amount: Decimal,
    /// Live category, if any.
    pub category_id: Option<CategoryId>,
    /// "None" if no or a deleted category (R-CAT-7); else "{emoji} Parent/Child" (R-CAT-4).
    pub category_label: String,
    /// Memo, or "" when none.
    pub memo: String,
    /// Attachment path, if any.
    pub attachment: Option<String>,
    /// Live contexts in transaction order: (id, name, colour hex).
    pub contexts: Vec<(ContextId, String, Option<String>)>,
    /// R-BAL-7 running balance at this row.
    pub running: Decimal,
    /// Reconciliation status.
    pub status: TransactionStatus,
    /// True for rows that belong to a scenario.
    pub is_scenario: bool,
    /// R-TXN-1 lock state under the active scenario.
    pub locked: bool,
}

/// Cache key for the account rows.
#[derive(Debug, Clone, PartialEq)]
pub struct RowsKey {
    /// The account shown.
    pub account: AccountId,
    /// Active scenario; `None` is Reality.
    pub active: Option<ScenarioId>,
    /// Time window (R-FLT-1, R-FLT-2).
    pub period: TimePeriod,
    /// Injected "today" (R-FLT-1).
    pub today: Date,
    /// The search, only when it actively filters (open, and text or a date range). `None` otherwise.
    pub search: Option<SearchState>,
}

/// The rows for the table, and whether they are a search result (the running column then shows
/// the search running balance, R-BAL-8).
#[derive(Debug, Clone, PartialEq)]
pub struct Rows {
    /// Newest first.
    pub rows: Vec<TxRow>,
    /// True when a search filters the rows.
    pub searching: bool,
}

/// Pure. 1) visible txs of the account (R-SCN-1); 2) window filter (R-FLT-2) and, when searching,
/// the text and date-range filters (R-FLT-3, R-FLT-4); 3) the running balance: over ALL visible
/// rows when not searching (R-BAL-7), or over the filtered rows when searching (R-BAL-8).
/// Order is date desc, id desc (R-FLT-5).
pub fn build_rows(db: &Db, state: &AppState, key: &RowsKey) -> Rows {
    // R-SCN-1 + R-FLT-5: `account_transactions` already returns the newest-first order.
    let visible = filter::account_transactions(db, key.account, key.active);
    let search = key
        .search
        .clone()
        .unwrap_or_else(|| SearchState::new(key.today));
    let window = filter::filter_transactions(db, &visible, key.period, &search, key.today);
    let searching = key.search.is_some();
    let running = if searching {
        // R-BAL-8: cumulative over the rows that passed the filters.
        balance::search_running_balances(&window)
    } else {
        // R-BAL-7: over all visible rows, so the running balance does not move with the window.
        balance::running_balances(&visible)
    };
    let rows = window
        .into_iter()
        .map(|tx| row(db, state, tx, &running))
        .collect();
    Rows { rows, searching }
}

/// One row for a transaction. `running` holds the R-BAL-7 value for every visible row.
fn row(
    db: &Db,
    state: &AppState,
    tx: &Transaction,
    running: &HashMap<TransactionId, Decimal>,
) -> TxRow {
    let category_id = tx.category_id.filter(|&c| live_category(db, c));
    TxRow {
        id: tx.id,
        date: tx.date,
        payee: tx.payee.clone(),
        amount: tx.amount,
        category_id,
        category_label: category_label(db, category_id),
        memo: tx.memo.clone().unwrap_or_default(),
        attachment: tx.attachment_path.clone().filter(|p| !p.is_empty()),
        contexts: tx
            .context_ids
            .iter()
            .filter_map(|&c| db.context(c))
            .filter(|c| !c.is_deleted)
            .map(|c| (c.id, c.name.clone(), c.color_hex.clone()))
            .collect(),
        running: running.get(&tx.id).copied().unwrap_or(Decimal::ZERO),
        status: tx.status,
        is_scenario: tx.scenario_id.is_some(),
        locked: is_locked(db, state, tx.id),
    }
}

/// True when the category exists and is not soft-deleted.
fn live_category(db: &Db, id: CategoryId) -> bool {
    db.category(id).is_some_and(|c| !c.is_deleted)
}

/// R-CAT-7 and R-CAT-4: "None" without a live category; else the emoji icon (when the icon is an
/// emoji) and the full "Parent/Child" name, as the Swift picker shows it.
fn category_label(db: &Db, id: Option<CategoryId>) -> String {
    let Some(cat) = id.and_then(|c| db.category(c)) else {
        return "None".to_string();
    };
    let name = db.category_full_name(cat.id);
    match cat.icon.as_deref() {
        Some(icon) if categorize::is_emoji(icon) => format!("{icon} {name}"),
        _ => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_core::filter::SearchScope;
    use pf_model::testing::{TestDb, d, m};
    use pf_model::{AccountType, CategoryType, RowKey, TimePeriod};

    const TODAY: &str = "2026-03-15";

    fn key(acc: AccountId, active: Option<ScenarioId>, period: TimePeriod) -> RowsKey {
        RowsKey {
            account: acc,
            active,
            period,
            today: d(TODAY),
            search: None,
        }
    }

    /// An open search with `text` in `scope` (and no date range).
    fn text_search(text: &str, scope: SearchScope) -> SearchState {
        SearchState {
            open: true,
            text: text.to_string(),
            scope,
            ..SearchState::new(d(TODAY))
        }
    }

    /// Runs `build_rows` with an open search, for the active scenario None and TimePeriod::All.
    fn searched(db: &Db, acc: AccountId, search: SearchState) -> Rows {
        let mut k = key(acc, None, TimePeriod::All);
        k.search = Some(search);
        build_rows(db, &state(None), &k)
    }

    fn state(active: Option<ScenarioId>) -> AppState {
        AppState {
            active_scenario: active,
            time_period: TimePeriod::PastMonth,
        }
    }

    /// Payees of the rows, in row order.
    fn payees(rows: &[TxRow]) -> Vec<&str> {
        rows.iter().map(|r| r.payee.as_str()).collect()
    }

    /// Runs `build_rows` with the active scenario `active` and returns the rows.
    fn rows_for(
        db: &Db,
        acc: AccountId,
        active: Option<ScenarioId>,
        period: TimePeriod,
    ) -> Vec<TxRow> {
        build_rows(db, &state(active), &key(acc, active, period)).rows
    }

    #[test]
    /// Covers: R-SCN-1. No active scenario ⇒ scenario rows absent; active S ⇒ reality + S rows,
    /// not other scenarios; deleted rows never.
    fn r_scn_1_rows_reality_excludes_scenario_rows() {
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let s = t.scenario("Plan A");
        let other = t.scenario("Plan B");
        t.tx(chk, "2026-03-01", "-1", "Real").done();
        t.tx(chk, "2026-03-02", "-2", "Plan A row")
            .scenario(s)
            .done();
        t.tx(chk, "2026-03-03", "-3", "Plan B row")
            .scenario(other)
            .done();
        t.tx(chk, "2026-03-04", "-4", "Deleted").deleted().done();
        let db = t.build();

        let reality = rows_for(&db, chk, None, TimePeriod::All);
        assert_eq!(payees(&reality), vec!["Real"]);

        let plan = rows_for(&db, chk, Some(s), TimePeriod::All);
        let mut names = payees(&plan);
        names.sort();
        assert_eq!(names, vec!["Plan A row", "Real"]);
    }

    #[test]
    /// Covers: R-BAL-7. Three txs on two dates, one a same-date tie: running values accumulate
    /// by (date, id), oldest first.
    fn r_bal_7_running_balance_oldest_to_newest() {
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let a = t.tx(chk, "2026-03-01", "100", "A").done();
        let b = t.tx(chk, "2026-03-02", "-30", "B").done();
        let c = t.tx(chk, "2026-03-02", "-20", "C").done();
        let db = t.build();
        let rows = rows_for(&db, chk, None, TimePeriod::All);
        // Newest first: C (same date, higher id), B, A.
        assert_eq!(payees(&rows), vec!["C", "B", "A"]);
        let running = |id: TransactionId| rows.iter().find(|r| r.id == id).map(|r| r.running);
        assert_eq!(running(a), Some(m("100")));
        assert_eq!(running(b), Some(m("70")));
        assert_eq!(running(c), Some(m("50")));
    }

    #[test]
    /// Covers: R-BAL-7. The newest row's running balance is the same for PastMonth and All.
    fn r_bal_7_running_balance_stable_across_period() {
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        t.tx(chk, "2025-01-01", "500", "Old").done();
        t.tx(chk, "2026-03-10", "-40", "New").done();
        let db = t.build();
        let month = rows_for(&db, chk, None, TimePeriod::PastMonth);
        let all = rows_for(&db, chk, None, TimePeriod::All);
        assert_eq!(payees(&month), vec!["New"]);
        assert_eq!(payees(&all), vec!["New", "Old"]);
        assert_eq!(month[0].running, m("460"));
        assert_eq!(all[0].running, m("460"));
    }

    #[test]
    /// Covers: R-FLT-2. Archival ⇒ only reality rows dated before today − 1 year.
    fn r_flt_2_archival_shows_only_old_reality() {
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let s = t.scenario("Plan A");
        t.tx(chk, "2025-03-01", "-1", "old real").done();
        t.tx(chk, "2025-03-15", "-1", "exactly one year").done();
        t.tx(chk, "2026-01-01", "-1", "recent").done();
        t.tx(chk, "2025-01-01", "-1", "old scenario")
            .scenario(s)
            .done();
        let db = t.build();
        let rows = rows_for(&db, chk, Some(s), TimePeriod::Archival);
        assert_eq!(payees(&rows), vec!["old real"]);
    }

    #[test]
    /// Covers: R-FLT-2. A future-dated scenario row is visible under PastMonth; an old reality row
    /// is not.
    fn r_flt_2_scenario_rows_pass_window() {
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let s = t.scenario("Plan A");
        t.tx(chk, "2027-06-01", "-1", "future plan")
            .scenario(s)
            .done();
        t.tx(chk, "2026-01-01", "-1", "old real").done();
        let db = t.build();
        let rows = rows_for(&db, chk, Some(s), TimePeriod::PastMonth);
        assert_eq!(payees(&rows), vec!["future plan"]);
    }

    #[test]
    /// Covers: R-FLT-5. Date descending, then id descending.
    fn r_flt_5_sorted_date_desc_then_id_desc() {
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        t.tx(chk, "2026-03-01", "-1", "a").done();
        t.tx(chk, "2026-03-05", "-1", "b").done();
        t.tx(chk, "2026-03-05", "-1", "c").done();
        let db = t.build();
        let rows = rows_for(&db, chk, None, TimePeriod::All);
        assert_eq!(payees(&rows), vec!["c", "b", "a"]);
    }

    #[test]
    /// Covers: R-TXN-1. Active scenario ⇒ reality rows locked, scenario rows not; Reality ⇒ none
    /// locked.
    fn r_txn_1_locked_flag_in_scenario_mode() {
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let s = t.scenario("Plan A");
        t.tx(chk, "2026-03-01", "-1", "real").done();
        t.tx(chk, "2026-03-02", "-2", "plan").scenario(s).done();
        let db = t.build();

        let plan = rows_for(&db, chk, Some(s), TimePeriod::All);
        for r in &plan {
            assert_eq!(r.locked, !r.is_scenario, "row {}", r.payee);
        }
        let reality = rows_for(&db, chk, None, TimePeriod::All);
        assert!(reality.iter().all(|r| !r.locked));
    }

    #[test]
    /// Covers: R-CAT-7, R-CAT-4. A transaction whose category is soft-deleted shows "None"; a live
    /// child category shows "Parent/Child".
    fn rows_category_label_none_when_deleted_category() {
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let food = t.category("Food", CategoryType::Expense);
        let cafe = t.child_category(food, "Cafe");
        let old = t.category("Old", CategoryType::Expense);
        t.tx(chk, "2026-03-01", "-4", "Latte").category(cafe).done();
        t.tx(chk, "2026-03-02", "-5", "Stale").category(old).done();
        t.delete(RowKey::Category(old));
        let db = t.build();
        let rows = rows_for(&db, chk, None, TimePeriod::All);
        let label = |p: &str| {
            rows.iter()
                .find(|r| r.payee == p)
                .map(|r| r.category_label.clone())
        };
        assert_eq!(label("Stale").as_deref(), Some("None"));
        assert_eq!(label("Latte").as_deref(), Some("Food/Cafe"));
    }

    #[test]
    /// Covers: R-FLT-3. The payee search ignores case; blank text is no search at all.
    fn r_flt_3_search_payee_case_insensitive() {
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        t.tx(chk, "2026-03-01", "-4", "Coffee Shop").done();
        t.tx(chk, "2026-03-02", "-40", "Groceries").done();
        let db = t.build();

        let found = searched(&db, chk, text_search("COFF", SearchScope::Payee));
        assert!(found.searching);
        assert_eq!(payees(&found.rows), vec!["Coffee Shop"]);

        let blank = SearchState {
            open: true,
            text: "   ".into(),
            ..SearchState::new(d(TODAY))
        };
        assert!(
            !blank.is_active(),
            "whitespace-only text is not an active search"
        );
    }

    #[test]
    /// Covers: R-FLT-3. The category scope matches the plain name or the "Parent/Child" name.
    fn r_flt_3_search_category_matches_name_or_full_name() {
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let food = t.category("Food", CategoryType::Expense);
        let cafe = t.child_category(food, "Cafe");
        t.tx(chk, "2026-03-01", "-4", "Morning")
            .category(cafe)
            .done();
        t.tx(chk, "2026-03-02", "-9", "Other").done();
        let db = t.build();

        for text in ["Food/Ca", "Cafe"] {
            let found = searched(&db, chk, text_search(text, SearchScope::Category));
            assert_eq!(payees(&found.rows), vec!["Morning"], "query {text:?}");
        }
    }

    #[test]
    /// Covers: R-FLT-3. The context scope matches any context name on the row.
    fn r_flt_3_search_context_any_name() {
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let trip = t.context("trip");
        let home = t.context("home");
        t.tx(chk, "2026-03-01", "-4", "Hotel")
            .contexts(&[trip, home])
            .done();
        t.tx(chk, "2026-03-02", "-9", "Rent")
            .contexts(&[home])
            .done();
        let db = t.build();

        let found = searched(&db, chk, text_search("tri", SearchScope::Context));
        assert_eq!(payees(&found.rows), vec!["Hotel"]);
    }

    #[test]
    /// Covers: R-FLT-4. The date range includes its first and last day, and excludes the day after.
    fn r_flt_4_date_range_inclusive() {
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        t.tx(chk, "2026-03-01", "-1", "Before").done();
        t.tx(chk, "2026-03-02", "-1", "Start").done();
        t.tx(chk, "2026-03-04", "-1", "End").done();
        t.tx(chk, "2026-03-05", "-1", "After").done();
        let db = t.build();

        let range = SearchState {
            open: true,
            use_date_range: true,
            start: d("2026-03-02"),
            end: d("2026-03-04"),
            ..SearchState::new(d(TODAY))
        };
        let found = searched(&db, chk, range);
        assert_eq!(payees(&found.rows), vec!["End", "Start"]);
    }

    #[test]
    /// Covers: R-BAL-8. With a search, the running balance accumulates over the matching rows only,
    /// oldest first; without one it accumulates over every row (R-BAL-7).
    fn r_bal_8_search_running_balance_over_filtered_rows() {
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        t.tx(chk, "2026-03-01", "-10", "Coffee A").done();
        t.tx(chk, "2026-03-02", "-20", "Lunch").done();
        t.tx(chk, "2026-03-03", "-30", "Coffee B").done();
        t.tx(chk, "2026-03-04", "-40", "Dinner").done();
        t.tx(chk, "2026-03-05", "-50", "Coffee C").done();
        let db = t.build();

        let found = searched(&db, chk, text_search("coffee", SearchScope::Payee));
        assert!(found.searching);
        // Matches oldest first: A -10, B -40, C -90. Shown newest first.
        let running: Vec<Decimal> = found.rows.iter().map(|r| r.running).collect();
        assert_eq!(running, vec![m("-90"), m("-40"), m("-10")]);

        let all = build_rows(&db, &state(None), &key(chk, None, TimePeriod::All));
        assert!(!all.searching);
        // All rows oldest first: -10, -30, -60, -100, -150. Shown newest first.
        let all_running: Vec<Decimal> = all.rows.iter().map(|r| r.running).collect();
        assert_eq!(
            all_running,
            vec![m("-150"), m("-100"), m("-60"), m("-30"), m("-10")]
        );
    }
}
