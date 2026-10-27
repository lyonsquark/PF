# T03: pf-core scenario visibility, balances, running balances, filters, obligations

**Phase:** 1 · **Deps:** T01 · **Crate(s):** `pf-core` · **Size:** medium (about 450 lines of code, 500 of tests)

## Goal

All the read-side maths the dashboard, account screen, reports and API need, as pure functions over
`&Db`: which transactions are visible in a scenario, account balances (all / cleared / reality),
dashboard totals and groups, running balances, the transaction-list filter (time period + search),
and obligation balances. Nothing here mutates the `Db` or reads the clock.

## Read first

- Rules: R-SCN-1, R-SCN-2, R-BAL-1 … R-BAL-9, R-FLT-1 … R-FLT-6, R-OBL-1, R-OBL-2, R-OBL-3, R-OBL-5
- Swift: `Managers/ScenarioManager.swift`, `Managers/ObligationManager.swift`, `Models/TimePeriod.swift`
- Swift: `Views/DashboardView.swift` (balanceMaps, clearedBalanceMap, summary totals, `body` groups)
- Swift: `Views/AccountBalanceView.swift`, `Views/AccountDetailView.swift` (`filteringPredicate`)
- Swift: `Views/TransactionListView.swift`: `filteredTransactions`, `runningBalances`, `searchRunningBalances`, `isSearchActive`
- Swift: `Views/ObligationsView.swift` (grouping by status)

## Deliverables

```rust
// crates/pf-core/src/lib.rs
pub mod balance;
pub mod filter;
pub mod obligation;
```

### `balance.rs`
```rust
use std::collections::HashMap;
use pf_model::*;
use rust_decimal::Decimal;

/// R-SCN-1: is `tx` visible under `active` (None = Reality)? Deleted rows are never visible.
pub fn is_visible(tx: &Transaction, active: Option<ScenarioId>) -> bool;

/// R-SCN-2: new active scenario after the user picks `clicked` (None = "Reality") while `current` is active.
/// Same id → None; otherwise → clicked.
pub fn toggle_scenario(current: Option<ScenarioId>, clicked: Option<ScenarioId>) -> Option<ScenarioId>;

/// `Cleared` or `Reconciled`.
pub fn is_cleared(status: TransactionStatus) -> bool;

/// R-BAL-1: sum of visible amounts for the account (all statuses).
pub fn account_balance(db: &Db, account: AccountId, active: Option<ScenarioId>) -> Decimal;
/// R-BAL-2: same, cleared/reconciled only.
pub fn account_cleared_balance(db: &Db, account: AccountId, active: Option<ScenarioId>) -> Decimal;

/// R-BAL-1..4: everything the dashboard needs in ONE pass over live transactions.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BalanceMaps {
    pub all: HashMap<AccountId, Decimal>,             // visible under active (R-BAL-1)
    pub cleared: HashMap<AccountId, Decimal>,         // visible + cleared (R-BAL-2)
    pub reality_all: HashMap<AccountId, Decimal>,     // scenario_id == None (R-BAL-3)
    pub reality_cleared: HashMap<AccountId, Decimal>,
}
pub fn balance_maps(db: &Db, active: Option<ScenarioId>) -> BalanceMaps;   // R-BAL-4: account None skipped

/// R-BAL-6: the maps the dashboard *displays*. When `show_available`, subtract
/// `obligation::total_reserved(db, account, None)` from `all` and `cleared` for every live account.
/// Missing entries count as 0 (an account with no transactions can go negative).
pub fn displayed_balances(db: &Db, maps: &BalanceMaps, show_available: bool) -> (HashMap<AccountId, Decimal>, HashMap<AccountId, Decimal>); // (cleared, all)

/// R-BAL-5: summary cards, computed from a *cleared* map (pass the displayed cleared map).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DashboardTotals { pub net_worth: Decimal, pub assets: Decimal, pub credit_debt: Decimal, pub loan_debt: Decimal }
pub fn dashboard_totals(db: &Db, cleared: &HashMap<AccountId, Decimal>) -> DashboardTotals;

/// R-BAL-9: dashboard account groups, in order, empty groups omitted, accounts sorted by name.
#[derive(Debug, Clone, PartialEq)]
pub struct AccountGroup { pub title: &'static str, pub accounts: Vec<AccountId> }
pub fn dashboard_groups(db: &Db) -> Vec<AccountGroup>;

/// R-BAL-7: running balances. `txs` must be ordered NEWEST first (the list's order, see R-FLT-5).
/// Iterate oldest→newest, per account, skipping deleted rows and rows with no account.
pub fn running_balances(txs: &[&Transaction]) -> HashMap<TransactionId, Decimal>;

/// R-BAL-8: cumulative sum over `txs` sorted oldest→newest by (date, id), ignoring accounts.
pub fn search_running_balances(txs: &[&Transaction]) -> HashMap<TransactionId, Decimal>;
```

### `filter.rs`
```rust
use jiff::civil::Date;

/// R-FLT-1. PastMonth = today − 1 month (jiff clamps Mar 31 → Feb 28), Past60Days = today − 60 days,
/// YearToDate = Jan 1 of today's year, PastYear = today − 1 year, Archival/All = None.
pub fn period_start(period: TimePeriod, today: Date) -> Option<Date>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchScope { #[default] Payee, Category, Context }   // labels "Payee" "Category" "Context"
impl SearchScope { pub const ALL: [SearchScope; 3]; pub fn label(self) -> &'static str; }

/// Search bar state (UI owns it; logic lives here so it is testable).
#[derive(Debug, Clone, PartialEq)]
pub struct SearchState {
    pub open: bool,
    pub text: String,
    pub scope: SearchScope,
    pub use_date_range: bool,
    pub start: Date,
    pub end: Date,
}
impl SearchState {
    /// R-FLT-4 defaults: closed, empty, Payee, range off, start = today − 1 year, end = today.
    pub fn new(today: Date) -> Self;
    /// R-FLT-6: closing clears text and turns the date range off (scope and dates are kept).
    pub fn close(&mut self);
    /// Swift `isSearchActive`: open && (trimmed text non-empty || use_date_range).
    pub fn is_active(&self) -> bool;
}

/// R-SCN-1 + R-FLT-5: every visible transaction of the account, newest first (date desc, id desc).
/// This is the Swift @Query result BEFORE the time-window filter.
pub fn account_transactions<'a>(db: &'a Db, account: AccountId, active: Option<ScenarioId>) -> Vec<&'a Transaction>;

/// R-FLT-2..4: apply the time window and (if active) the search to `txs`, keeping order.
pub fn filter_transactions<'a>(db: &Db, txs: &[&'a Transaction], period: TimePeriod, search: &SearchState, today: Date) -> Vec<&'a Transaction>;

/// Everything the account screen needs, in one call (perf target: < 15 ms for 100k rows, T12b).
#[derive(Debug, Clone, PartialEq)]
pub struct AccountView {
    pub rows: Vec<TransactionId>,                       // filtered, newest first
    pub running: HashMap<TransactionId, Decimal>,       // R-BAL-7 over ALL visible rows of the account
    pub search_running: Option<HashMap<TransactionId, Decimal>>, // R-BAL-8, Some only if search.is_active()
    pub balance: Decimal,                               // R-BAL-1
    pub cleared_balance: Decimal,                       // R-BAL-2
}
pub fn account_view(db: &Db, account: AccountId, active: Option<ScenarioId>, period: TimePeriod, search: &SearchState, today: Date) -> AccountView;
```

Filter rules, exactly (Swift `filteredTransactions`):
1. Deleted rows are dropped.
2. `period == Archival`: keep iff `scenario_id.is_none() && date < today − 1 year`.
3. Otherwise: rows with `scenario_id.is_some()` are **always kept**; reality rows are kept iff
   `period_start` is None or `date >= start`.
4. If `search.open` and the trimmed text is non-empty, with case-insensitive containment (`to_lowercase().contains`):
   * Payee: the payee contains the query.
   * Category: the category's `name` **or** `full_name` contains it (no category → excluded).
   * Context: any of the transaction's (live) contexts' names contains it.
5. If `search.open && search.use_date_range`: keep `search.start <= date <= search.end` (inclusive days).

### `obligation.rs`
```rust
/// R-OBL-1: current_balance + sum of amounts of live transactions carrying the obligation's
/// context and visible under `active` (R-SCN-1). No context → current_balance.
pub fn computed_balance(db: &Db, ob: &Obligation, active: Option<ScenarioId>) -> Decimal;
/// R-OBL-2: sum of computed balances of the account's LIVE obligations with status Funding or Spending.
pub fn total_reserved(db: &Db, account: AccountId, active: Option<ScenarioId>) -> Decimal;
/// R-OBL-3.
pub fn available_balance(total: Decimal, reserved: Decimal) -> Decimal;
/// R-OBL-5: live obligations grouped by status in ObligationStatus::ALL order, each group sorted by
/// name; empty groups omitted.
pub fn obligations_by_status(db: &Db) -> Vec<(ObligationStatus, Vec<ObligationId>)>;
```

## Steps

1. Add the three modules. Write the tests for `balance.rs` first, using `TestDb`.
2. Implement `balance.rs`. `balance_maps` must be a **single pass** over `db.live_transactions()`.
3. `obligation.rs` (`displayed_balances` calls `total_reserved`, so do obligations before finishing balances).
4. `filter.rs`: `period_start` with `today.checked_sub(jiff::Span::new().months(1))` etc.
5. `account_view` = `account_transactions` → `running_balances` (on the unfiltered list) → `filter_transactions`
   → `search_running_balances` (on the filtered list, if active) → balances.

## Tests (required)

Use a fixed `today = d("2026-03-15")` everywhere.

| Test name | Covers | What it checks |
|---|---|---|
| `r_scn_1_reality_sees_only_reality` | R-SCN-1 | reality tx visible, scenario tx not, deleted not |
| `r_scn_1_scenario_sees_reality_plus_own` | R-SCN-1 | under S: reality + S visible; another scenario T not |
| `r_scn_2_toggle_same_turns_off` | R-SCN-2 | toggle(Some(S), Some(S)) → None |
| `r_scn_2_toggle_other_switches` | R-SCN-2 | toggle(Some(S), Some(T)) → Some(T); toggle(None, Some(S)) → Some(S); toggle(Some(S), None) → None |
| `r_bal_1_balance_sums_visible_all_statuses` | R-BAL-1 | pending + cleared + future (scenario) summed appropriately |
| `r_bal_1_balance_excludes_other_accounts_and_deleted` | R-BAL-1 | |
| `r_bal_2_cleared_balance_only_cleared_reconciled` | R-BAL-2 | pending and future excluded |
| `r_bal_3_reality_maps_ignore_active_scenario` | R-BAL-3 | with S active, `reality_all` excludes S rows but `all` includes them |
| `r_bal_4_orphans_contribute_nothing` | R-BAL-4 | orphan tx is in no map |
| `r_bal_5_totals_by_account_type` | R-BAL-5 | checking+savings+cash+investment = assets; credit = credit debt; loan = loan debt; net worth = all |
| `r_bal_5_external_accounts_excluded` | R-BAL-5 | external checking not in assets or net worth |
| `r_bal_6_available_subtracts_reserved` | R-BAL-6 | a funding obligation of 300 reduces displayed cleared and all by 300 |
| `r_bal_6_available_off_is_identity` | R-BAL-6 | show_available=false gives maps equal to the input |
| `r_bal_6_reserved_uses_reality` | R-BAL-6 | with S active, the scenario tx tagged with the obligation's context does NOT change the reserved amount (Swift passes no scenario) |
| `r_bal_7_running_balance_oldest_to_newest` | R-BAL-7 | 3 rows → 10, 15, 12 at the correct ids |
| `r_bal_7_running_balance_same_date_by_id` | R-BAL-7 | same-date rows accumulate in id order |
| `r_bal_7_running_balance_independent_of_window` | R-BAL-7 | `account_view` with PastMonth vs All: the same running value for a row in both |
| `r_bal_7_running_balance_per_account` | R-BAL-7 | two accounts accumulate separately |
| `r_bal_8_search_running_only_filtered` | R-BAL-8 | search "coffee" → running sums only coffee rows; `None` when search inactive |
| `r_bal_9_groups_order_and_membership` | R-BAL-9 | titles in order, cash goes in "Cash & Checking", empty groups omitted, sorted by name |
| `r_flt_1_period_starts` | R-FLT-1 | each period for today=2026-03-15: 2026-02-15, 2026-01-14, 2026-01-01, 2025-03-15, None, None |
| `r_flt_1_past_month_clamps` | R-FLT-1 | today 2026-03-31 → 2026-02-28 |
| `r_flt_2_scenario_rows_always_pass_window` | R-FLT-2 | a scenario row dated 2027 or 2020 is kept under PastMonth |
| `r_flt_2_reality_rows_respect_start` | R-FLT-2 | a row before the start is dropped; a row exactly on the start is kept |
| `r_flt_2_archival_only_old_reality` | R-FLT-2 | Archival keeps reality rows older than one year, drops recent rows and all scenario rows |
| `r_flt_3_search_payee_case_insensitive` | R-FLT-3 | "COF" matches "Coffee" |
| `r_flt_3_search_category_name_or_full_name` | R-FLT-3 | "housing/util" matches child "Utilities" of "Housing"; uncategorized excluded |
| `r_flt_3_search_context` | R-FLT-3 | matches any context name |
| `r_flt_3_blank_search_ignored` | R-FLT-3 | open with "   " → no text filtering |
| `r_flt_4_date_range_inclusive` | R-FLT-4 | start and end days are both included |
| `r_flt_4_defaults` | R-FLT-4 | `SearchState::new(today)` range = 2025-03-15..2026-03-15 |
| `r_flt_5_order_date_desc_id_desc` | R-FLT-5 | `account_transactions` order |
| `r_flt_6_close_clears_text_and_range` | R-FLT-6 | after `close()`: !open, text empty, use_date_range false |
| `r_obl_1_computed_balance_adds_context_txs` | R-OBL-1 | 500 + (+200 − 50) = 650 |
| `r_obl_1_no_context_is_current_balance` | R-OBL-1 | |
| `r_obl_1_respects_scenario_visibility` | R-OBL-1 | the scenario row counts only under that scenario |
| `r_obl_2_reserved_only_funding_spending_live` | R-OBL-2 | Future/Deferred/Completed and deleted obligations excluded |
| `r_obl_3_available_is_total_minus_reserved` | R-OBL-3 | |
| `r_obl_5_grouped_by_status_in_order` | R-OBL-5 | order Funding, Spending, Future, Deferred, Completed; names sorted; empty omitted |
| `prop_balances_sum_matches` | R-BAL-1 | property: Σ over accounts of `all` == Σ of visible amounts with Some(account) |

## Pitfalls

- **R-BAL-6 uses Reality for reserved amounts** even when a scenario is active. Swift's dashboard calls
  `totalReserved(for: account)` with the default `nil` scenario. Match it.
- Swift's `totalReserved` iterated `account.obligations` without filtering deleted ones; the rules say
  **live only**. That is an intentional fix, and the test asserts live-only.
- The running balance is computed on the list **before** the time window (R-BAL-7). Computing it on the
  filtered list is the most likely bug.
- `HashMap` iteration order is random; never assert on it. Sort keys in tests.
- Never use the system clock; `today` is a parameter.
- Case-insensitive: use `str::to_lowercase()` on both sides (not ASCII-only lowercasing).

## Done when

```bash
cargo test -p pf-core balance filter obligation
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules | grep -E "R-(SCN-[12]|BAL|FLT|OBL-[1235])"   # no output
```

## Out of scope

- Context-name generation (R-OBL-4, T04). Mutations (T10). P&L/report maths (T08). Formatting money (T04).
