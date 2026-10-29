//! Transaction-list filters: time window, search bar state, account rows, and the account view.

use std::collections::HashMap;

use jiff::Span;
use jiff::civil::Date;
use pf_model::*;
use rust_decimal::Decimal;

use crate::balance::{is_cleared, is_visible, running_balances, search_running_balances};

/// R-FLT-1: start of a time period relative to `today`. Archival and All have none.
pub fn period_start(period: TimePeriod, today: Date) -> Option<Date> {
    match period {
        TimePeriod::PastMonth => today.checked_sub(Span::new().months(1)).ok(),
        TimePeriod::Past60Days => today.checked_sub(Span::new().days(60)).ok(),
        TimePeriod::YearToDate => Some(today.first_of_year()),
        TimePeriod::PastYear => today.checked_sub(Span::new().years(1)).ok(),
        TimePeriod::Archival | TimePeriod::All => None,
    }
}

/// Which field the search bar matches against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchScope {
    /// Match the payee.
    #[default]
    Payee,
    /// Match the category name or full name.
    Category,
    /// Match any context name.
    Context,
    /// Match the status name (Pending, Cleared, Reconciled or Future).
    Status,
}

impl SearchScope {
    /// Every scope, in picker order.
    pub const ALL: [SearchScope; 4] = [
        SearchScope::Payee,
        SearchScope::Category,
        SearchScope::Context,
        SearchScope::Status,
    ];

    /// The picker label.
    pub fn label(self) -> &'static str {
        match self {
            SearchScope::Payee => "Payee",
            SearchScope::Category => "Category",
            SearchScope::Context => "Context",
            SearchScope::Status => "Status",
        }
    }
}

/// Search bar state (UI owns it; logic lives here so it is testable).
#[derive(Debug, Clone, PartialEq)]
pub struct SearchState {
    /// Whether the bar is shown.
    pub open: bool,
    /// Query text, as typed.
    pub text: String,
    /// Field searched.
    pub scope: SearchScope,
    /// Whether the date range applies.
    pub use_date_range: bool,
    /// First day of the range (inclusive).
    pub start: Date,
    /// Last day of the range (inclusive).
    pub end: Date,
}

impl SearchState {
    /// R-FLT-4 defaults: closed, empty, Payee, range off, start = today − 1 year, end = today.
    pub fn new(today: Date) -> Self {
        Self {
            open: false,
            text: String::new(),
            scope: SearchScope::default(),
            use_date_range: false,
            // R-FLT-4: a year back. Falls back to today only if the date cannot be built.
            start: period_start(TimePeriod::PastYear, today).unwrap_or(today),
            end: today,
        }
    }

    /// R-FLT-6: closing clears the text and turns the date range off. Scope and dates are kept.
    pub fn close(&mut self) {
        self.open = false;
        self.text.clear();
        self.use_date_range = false;
    }

    /// Swift `isSearchActive`: open && (trimmed text non-empty || use_date_range).
    pub fn is_active(&self) -> bool {
        self.open && (!self.text.trim().is_empty() || self.use_date_range)
    }
}

/// R-SCN-1 + R-FLT-5: every visible live transaction of the account, newest first (date desc, id desc).
pub fn account_transactions(
    db: &Db,
    account: AccountId,
    active: Option<ScenarioId>,
) -> Vec<&Transaction> {
    let mut txs: Vec<&Transaction> = db
        .transactions_for_account(account)
        .filter(|tx| is_visible(tx, active))
        .collect();
    // R-FLT-5: date descending, then id descending
    txs.sort_by(|a, b| b.date.cmp(&a.date).then(b.id.cmp(&a.id)));
    txs
}

/// R-FLT-2: the time window. Archival keeps only old reality rows; other periods always keep
/// scenario rows and keep reality rows dated on or after the period start.
fn in_window(tx: &Transaction, period: TimePeriod, today: Date) -> bool {
    if period == TimePeriod::Archival {
        let cutoff = period_start(TimePeriod::PastYear, today);
        return tx.scenario_id.is_none() && cutoff.is_some_and(|c| tx.date < c);
    }
    if tx.scenario_id.is_some() {
        return true;
    }
    period_start(period, today).is_none_or(|start| tx.date >= start)
}

/// R-FLT-3: does the transaction match the (already lower-cased) query in the given scope?
fn matches_search(db: &Db, tx: &Transaction, scope: SearchScope, query: &str) -> bool {
    match scope {
        SearchScope::Payee => tx.payee.to_lowercase().contains(query),
        SearchScope::Category => tx
            .category_id
            .and_then(|c| db.category(c))
            .is_some_and(|cat| {
                cat.name.to_lowercase().contains(query)
                    || db.category_full_name(cat.id).to_lowercase().contains(query)
            }),
        SearchScope::Context => tx
            .context_ids
            .iter()
            .filter_map(|&c| db.context(c))
            .any(|c| !c.is_deleted && c.name.to_lowercase().contains(query)),
        // The status name as the picker shows it, so "fut" finds Future.
        SearchScope::Status => tx.status.label().to_lowercase().contains(query),
    }
}

/// R-FLT-2..4: apply the time window and (if active) the search to `txs`, keeping order.
pub fn filter_transactions<'a>(
    db: &Db,
    txs: &[&'a Transaction],
    period: TimePeriod,
    search: &SearchState,
    today: Date,
) -> Vec<&'a Transaction> {
    // R-FLT-3: the query is trimmed; a blank query means no text filter
    let query = search
        .open
        .then(|| search.text.trim().to_lowercase())
        .filter(|q| !q.is_empty());
    txs.iter()
        .copied()
        .filter(|tx| !tx.is_deleted)
        .filter(|tx| in_window(tx, period, today))
        .filter(|tx| {
            query
                .as_deref()
                .is_none_or(|q| matches_search(db, tx, search.scope, q))
        })
        // R-FLT-4: inclusive day range
        .filter(|tx| {
            !(search.open && search.use_date_range)
                || (search.start <= tx.date && tx.date <= search.end)
        })
        .collect()
}

/// Everything the account screen needs, in one call (perf target: < 15 ms for 100k rows, T12b).
#[derive(Debug, Clone, PartialEq)]
pub struct AccountView {
    /// Filtered rows, newest first.
    pub rows: Vec<TransactionId>,
    /// R-BAL-7 over ALL visible rows of the account.
    pub running: HashMap<TransactionId, Decimal>,
    /// R-BAL-8, Some only if the search is active.
    pub search_running: Option<HashMap<TransactionId, Decimal>>,
    /// R-BAL-1.
    pub balance: Decimal,
    /// R-BAL-2.
    pub cleared_balance: Decimal,
}

/// Builds the account screen data. See the module docs for the pipeline order.
pub fn account_view(
    db: &Db,
    account: AccountId,
    active: Option<ScenarioId>,
    period: TimePeriod,
    search: &SearchState,
    today: Date,
) -> AccountView {
    let all = account_transactions(db, account, active);
    // R-BAL-7: running balance over the unfiltered rows, so it does not move with the window
    let running = running_balances(&all);
    let balance = all.iter().map(|t| t.amount).sum();
    let cleared_balance = all
        .iter()
        .filter(|t| is_cleared(t.status))
        .map(|t| t.amount)
        .sum();
    let filtered = filter_transactions(db, &all, period, search, today);
    // R-BAL-8: search running balance over the filtered rows, only while the search is active
    let search_running = search
        .is_active()
        .then(|| search_running_balances(&filtered));
    AccountView {
        rows: filtered.iter().map(|t| t.id).collect(),
        running,
        search_running,
        balance,
        cleared_balance,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::testing::{TestDb, d};
    use rust_decimal_macros::dec;

    const TODAY: &str = "2026-03-15";

    fn today() -> Date {
        d(TODAY)
    }

    /// Ids of the rows that survive `filter_transactions` over all live transactions, in order.
    fn kept(db: &Db, period: TimePeriod, search: &SearchState) -> Vec<TransactionId> {
        let txs: Vec<&Transaction> = db.live_transactions().collect();
        filter_transactions(db, &txs, period, search, today())
            .iter()
            .map(|t| t.id)
            .collect()
    }

    #[test]
    fn r_flt_1_period_starts() {
        // Covers: R-FLT-1
        let t = today();
        assert_eq!(
            period_start(TimePeriod::PastMonth, t),
            Some(d("2026-02-15"))
        );
        assert_eq!(
            period_start(TimePeriod::Past60Days, t),
            Some(d("2026-01-14"))
        );
        assert_eq!(
            period_start(TimePeriod::YearToDate, t),
            Some(d("2026-01-01"))
        );
        assert_eq!(period_start(TimePeriod::PastYear, t), Some(d("2025-03-15")));
        assert_eq!(period_start(TimePeriod::Archival, t), None);
        assert_eq!(period_start(TimePeriod::All, t), None);
    }

    #[test]
    fn r_flt_1_past_month_clamps() {
        // Covers: R-FLT-1
        assert_eq!(
            period_start(TimePeriod::PastMonth, d("2026-03-31")),
            Some(d("2026-02-28"))
        );
    }

    #[test]
    fn r_flt_2_scenario_rows_always_pass_window() {
        // Covers: R-FLT-2
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let s = t.scenario("Plan A");
        let future = t.tx(chk, "2027-06-01", "-1", "future").scenario(s).done();
        let ancient = t.tx(chk, "2020-06-01", "-1", "ancient").scenario(s).done();
        let db = t.build();
        let ids = kept(&db, TimePeriod::PastMonth, &SearchState::new(today()));
        assert!(ids.contains(&future));
        assert!(ids.contains(&ancient));
    }

    #[test]
    fn r_flt_2_reality_rows_respect_start() {
        // Covers: R-FLT-2
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let before = t.tx(chk, "2026-02-14", "-1", "before").done();
        let on_start = t.tx(chk, "2026-02-15", "-1", "on start").done();
        let db = t.build();
        let ids = kept(&db, TimePeriod::PastMonth, &SearchState::new(today()));
        assert!(!ids.contains(&before));
        assert!(ids.contains(&on_start));
    }

    #[test]
    fn r_flt_2_archival_only_old_reality() {
        // Covers: R-FLT-2
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let s = t.scenario("Plan A");
        let old_real = t.tx(chk, "2025-03-01", "-1", "old real").done();
        let cutoff_day = t.tx(chk, "2025-03-15", "-1", "exactly one year").done();
        let recent = t.tx(chk, "2026-01-01", "-1", "recent").done();
        let old_scen = t
            .tx(chk, "2025-01-01", "-1", "old scenario")
            .scenario(s)
            .done();
        let db = t.build();
        let ids = kept(&db, TimePeriod::Archival, &SearchState::new(today()));
        assert_eq!(ids, vec![old_real]);
        assert!(!ids.contains(&cutoff_day) && !ids.contains(&recent) && !ids.contains(&old_scen));
    }

    #[test]
    fn r_flt_3_search_payee_case_insensitive() {
        // Covers: R-FLT-3
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let coffee = t.tx(chk, "2026-03-01", "-4", "Coffee").done();
        let rent = t.tx(chk, "2026-03-02", "-900", "Rent").done();
        let db = t.build();
        let mut search = SearchState::new(today());
        search.open = true;
        search.text = "COF".to_string();
        let ids = kept(&db, TimePeriod::All, &search);
        assert_eq!(ids, vec![coffee]);
        assert!(!ids.contains(&rent));
    }

    #[test]
    fn r_flt_3_search_category_name_or_full_name() {
        // Covers: R-FLT-3
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let housing = t.category("Housing", CategoryType::Expense);
        let utilities = t.child_category(housing, "Utilities");
        let hit = t
            .tx(chk, "2026-03-01", "-80", "Power")
            .category(utilities)
            .done();
        let uncat = t.tx(chk, "2026-03-02", "-5", "Misc").done();
        let db = t.build();
        let mut search = SearchState::new(today());
        search.open = true;
        search.scope = SearchScope::Category;
        search.text = "housing/util".to_string();
        let ids = kept(&db, TimePeriod::All, &search);
        assert_eq!(ids, vec![hit]);
        assert!(!ids.contains(&uncat));
        search.text = "utilities".to_string();
        assert_eq!(kept(&db, TimePeriod::All, &search), vec![hit]);
    }

    #[test]
    fn r_flt_3_search_status() {
        // Covers: R-FLT-3
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let future = t
            .tx(chk, "2026-03-01", "-80", "Power")
            .status(TransactionStatus::Future)
            .done();
        let pending = t
            .tx(chk, "2026-03-02", "-5", "Misc")
            .status(TransactionStatus::Pending)
            .done();
        let db = t.build();
        let mut search = SearchState::new(today());
        search.open = true;
        search.scope = SearchScope::Status;
        search.text = "FUTURE".to_string();
        assert_eq!(kept(&db, TimePeriod::All, &search), vec![future]);
        search.text = "fut".to_string();
        assert_eq!(kept(&db, TimePeriod::All, &search), vec![future]);
        search.text = "pend".to_string();
        assert_eq!(kept(&db, TimePeriod::All, &search), vec![pending]);
        search.text = "cleared".to_string();
        assert!(kept(&db, TimePeriod::All, &search).is_empty());
    }

    #[test]
    fn r_flt_3_search_context() {
        // Covers: R-FLT-3
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let trip = t.context("Trip");
        let work = t.context("Work");
        let tagged = t
            .tx(chk, "2026-03-01", "-80", "Hotel")
            .contexts(&[work, trip])
            .done();
        let plain = t.tx(chk, "2026-03-02", "-5", "Lunch").done();
        let db = t.build();
        let mut search = SearchState::new(today());
        search.open = true;
        search.scope = SearchScope::Context;
        search.text = "trip".to_string();
        assert_eq!(kept(&db, TimePeriod::All, &search), vec![tagged]);
        search.text = "WORK".to_string();
        assert_eq!(kept(&db, TimePeriod::All, &search), vec![tagged]);
        assert!(!kept(&db, TimePeriod::All, &search).contains(&plain));
    }

    #[test]
    fn r_flt_3_blank_search_ignored() {
        // Covers: R-FLT-3
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let a = t.tx(chk, "2026-03-01", "-4", "Coffee").done();
        let b = t.tx(chk, "2026-03-02", "-5", "Lunch").done();
        let db = t.build();
        let mut search = SearchState::new(today());
        search.open = true;
        search.text = "   ".to_string();
        let ids = kept(&db, TimePeriod::All, &search);
        assert!(ids.contains(&a) && ids.contains(&b));
        assert!(!search.is_active());
    }

    #[test]
    fn r_flt_4_date_range_inclusive() {
        // Covers: R-FLT-4
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let before = t.tx(chk, "2026-03-01", "-1", "before").done();
        let first = t.tx(chk, "2026-03-05", "-1", "first day").done();
        let last = t.tx(chk, "2026-03-10", "-1", "last day").done();
        let after = t.tx(chk, "2026-03-11", "-1", "after").done();
        let db = t.build();
        let mut search = SearchState::new(today());
        search.open = true;
        search.use_date_range = true;
        search.start = d("2026-03-05");
        search.end = d("2026-03-10");
        let ids = kept(&db, TimePeriod::All, &search);
        assert_eq!(ids, vec![first, last]);
        assert!(!ids.contains(&before) && !ids.contains(&after));
    }

    #[test]
    fn r_flt_4_defaults() {
        // Covers: R-FLT-4
        let s = SearchState::new(today());
        assert_eq!(s.start, d("2025-03-15"));
        assert_eq!(s.end, d("2026-03-15"));
        assert!(!s.open && s.text.is_empty() && !s.use_date_range);
        assert_eq!(s.scope, SearchScope::Payee);
        assert!(!s.is_active());
    }

    #[test]
    fn r_flt_5_order_date_desc_id_desc() {
        // Covers: R-FLT-5
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let old = t.tx(chk, "2026-01-01", "1", "old").done();
        let same_a = t.tx(chk, "2026-02-01", "1", "same a").done();
        let same_b = t.tx(chk, "2026-02-01", "1", "same b").done();
        let db = t.build();
        let ordered: Vec<TransactionId> = account_transactions(&db, chk, None)
            .iter()
            .map(|t| t.id)
            .collect();
        assert_eq!(ordered, vec![same_b, same_a, old]);
    }

    #[test]
    fn r_flt_6_close_clears_text_and_range() {
        // Covers: R-FLT-6
        let mut s = SearchState::new(today());
        s.open = true;
        s.text = "rent".to_string();
        s.use_date_range = true;
        s.scope = SearchScope::Category;
        s.close();
        assert!(!s.open);
        assert!(s.text.is_empty());
        assert!(!s.use_date_range);
        assert_eq!(s.scope, SearchScope::Category, "scope is kept");
    }

    #[test]
    fn account_view_totals_and_search_running() {
        // Covers: R-BAL-1, R-BAL-2
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        t.tx(chk, "2026-03-01", "100", "a")
            .status(TransactionStatus::Pending)
            .done();
        t.tx(chk, "2026-03-02", "-30", "b").done();
        let db = t.build();
        let view = account_view(
            &db,
            chk,
            None,
            TimePeriod::All,
            &SearchState::new(today()),
            today(),
        );
        assert_eq!(view.balance, dec!(70));
        assert_eq!(view.cleared_balance, dec!(-30));
        assert_eq!(view.rows.len(), 2);
        assert!(view.search_running.is_none());
    }
}
