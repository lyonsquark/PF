# T08: Reports (Profit & Loss, Balance Sheet, Monthly Matrix)

**Phase:** 1 · **Deps:** T02, T03 (and T05 for the fixture-based golden tests; see Pitfalls) · **Crate(s):** `pf-core` · **Size:** medium (≈450 lines of code + tests)

## Goal

The numbers behind the Reports screen, as pure functions: P&L rows with drill-down ids, the balance
sheet, and the monthly matrix with its cell display logic and drill-downs. The Reports UI (T23) only
renders what these functions return, so every number a user sees in a report is unit-tested here.

## Read first

- Docs: `docs/business-rules.md` §9 (R-RPT-0 … R-RPT-6), and R-SCN-1, R-PRJ-*, R-FLT-1 (which these build on); `docs/testing.md` (L1, golden tests)
- Swift: `../PFApp/Sources/PFApp/Views/ReportsView.swift` (whole file); `../PFApp/Sources/PFApp/Views/ReportSubViews.swift` (whole file: `generateReportRows`, `PLTransactionDetailView`, `PLBudgetDetailView.calculatedTotal`, `BalanceSheetReportView.buildAccountInfoMap`, `activeObligations`, `MonthlyMatrixReportView.generateMatrixData`, `monthCell`, `drillDown`)
- Code: T02 `projector::generate_dates(item, from, until)`; T03 `is_visible(tx, active)` (R-SCN-1) and `period_start(period, today)` (R-FLT-1); T01 `Db` queries (`live_*`, `category_full_name`, `TestDb`). **Call these; do not re-implement them.** If the names differ from what is written here, use T03's names.

## Deliverables

```text
crates/pf-core/src/lib.rs                     add `pub mod reports;`
crates/pf-core/src/reports/mod.rs             shared helpers + re-exports
crates/pf-core/src/reports/pnl.rs
crates/pf-core/src/reports/drill.rs
crates/pf-core/src/reports/balance_sheet.rs
crates/pf-core/src/reports/matrix.rs
crates/pf-core/src/reports/snapshots/         (insta, committed)
```

```rust
use jiff::civil::Date;
use rust_decimal::Decimal;
use pf_model::{Db, AccountId, CategoryId, ScenarioId, TransactionId, BudgetItemId, ObligationId, TimePeriod};

// ---- mod.rs: shared ----
/// Colour class used by the UI: >0 Positive (green), <0 Negative (red), 0 Zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone { Positive, Negative, Zero }
pub fn tone(d: Decimal) -> Tone;
/// R-RPT-0: the live category of a transaction or budget item (a deleted one ⇒ None).
pub fn live_category(db: &Db, id: Option<CategoryId>) -> Option<&pf_model::Category>;
/// R-RPT-0: true when the live category is of type Transfer.
pub fn is_transfer(db: &Db, id: Option<CategoryId>) -> bool;
/// R-RPT-1 / R-RPT-3c: signed |amount × n| within [start, end].
pub fn budget_total_in_window(item: &pf_model::BudgetItem, start: Date, end: Date) -> Decimal;
/// R-RPT-3d: (today − 1 month, today).
pub fn default_pnl_range(today: Date) -> (Date, Date);
/// R-RPT-6: start from the period (keep `current_start` when the period has no start), end = today.
pub fn quick_select_range(period: TimePeriod, today: Date, current_start: Date) -> (Date, Date);
/// R-RPT-3d quick-select buttons in order: [("1M", PastMonth), ("60D", Past60Days), ("YTD", YearToDate), ("1Y", PastYear)].
pub const QUICK_SELECT: [(&str, TimePeriod); 4];
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportKind { ProfitAndLoss, BalanceSheet, MonthlyMatrix }
impl ReportKind { pub const ALL: [ReportKind; 3]; pub fn label(self) -> &'static str; } // "Profit & Loss" …

// ---- pnl.rs ----
#[derive(Debug, Clone, PartialEq)]
pub struct PlRow {
    pub label: String,                    // category full name, "Uncategorized", or "Total"
    pub category_id: Option<CategoryId>,  // None for Uncategorized and Total
    pub budgeted: Decimal,
    pub actual: Decimal,
    pub is_total: bool,
    pub transaction_ids: Vec<TransactionId>,
    pub budget_item_ids: Vec<BudgetItemId>,
}
impl PlRow { pub fn difference(&self) -> Decimal; }  // actual − budgeted
/// R-RPT-1/2/3: rows sorted by label, then the Total row (always present, even if all others are absent).
pub fn profit_and_loss(db: &Db, start: Date, end: Date, scenario: Option<ScenarioId>) -> Vec<PlRow>;

// ---- drill.rs ----
#[derive(Debug, Clone, PartialEq)]
pub struct DrillRow {
    pub id: TransactionId, pub date: Date, pub account: String, pub payee: String,
    pub amount: Decimal, pub memo: String, pub contexts: String,
    pub status: &'static str, pub scenario: String,
}
/// R-RPT-3b: sorted by date desc, id desc. `scenario_active` controls the Scenario column.
pub fn drill_rows(db: &Db, ids: &[TransactionId], scenario_active: bool) -> Vec<DrillRow>;
pub fn pnl_drill_title(label: &str) -> String;          // "{label} Transactions"
#[derive(Debug, Clone, PartialEq)]
pub struct BudgetDrillRow {
    pub id: BudgetItemId, pub name: String, pub category: String, pub item_type: &'static str,
    pub frequency: &'static str, pub amount: Decimal, pub start_date: Date, pub calculated_total: Decimal,
}
/// R-RPT-3c: sorted by name.
pub fn budget_drill_rows(db: &Db, ids: &[BudgetItemId], start: Date, end: Date) -> Vec<BudgetDrillRow>;
pub fn budget_drill_title(label: &str) -> String;       // "{label} Budget Items"

// ---- balance_sheet.rs ----
#[derive(Debug, Clone, PartialEq)]
pub struct BsAccountRow { pub account_id: AccountId, pub name: String, pub institution: String, pub total: Decimal, pub pending: Decimal }
#[derive(Debug, Clone, PartialEq)]
pub struct BsObligationRow { pub obligation_id: ObligationId, pub name: String, pub balance: Decimal, pub is_funding: bool }
#[derive(Debug, Clone, PartialEq)]
pub struct BalanceSheet {
    pub as_of: Date,
    pub accounts: Vec<BsAccountRow>,
    pub total: Decimal, pub pending_total: Decimal,
    pub obligations: Vec<BsObligationRow>,     // empty ⇒ section hidden
    pub total_obligations: Decimal,
    pub net_worth: Decimal, pub available: Decimal,
}
/// R-RPT-4 … 4d.
pub fn balance_sheet(db: &Db, as_of: Date, scenario: Option<ScenarioId>) -> BalanceSheet;

// ---- matrix.rs ----
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MatrixMode { #[default] All, ActualOnly, BudgetOnly }
impl MatrixMode { pub const ALL: [MatrixMode; 3]; pub fn label(self) -> &'static str; } // "Actual + Budget", "Actual Only", "Budget Only"
pub const MONTH_SHORT: [&str; 12];   // "Jan" … "Dec"
#[derive(Debug, Clone, PartialEq)]
pub struct MatrixRow { pub label: String, pub category_id: Option<CategoryId>, pub actual: [Decimal; 12], pub budget: [Decimal; 12] }
impl MatrixRow { pub fn actual_total(&self) -> Decimal; pub fn budget_total(&self) -> Decimal; }
#[derive(Debug, Clone, PartialEq)]
pub struct MonthlyMatrix { pub year: i16, pub rows: Vec<MatrixRow>, pub net_actual: [Decimal; 12], pub net_budget: [Decimal; 12] }
/// R-RPT-5a/5b/5c.
pub fn monthly_matrix(db: &Db, year: i16, scenario: Option<ScenarioId>) -> MonthlyMatrix;
/// R-RPT-5: current year − 2 ..= current year + 5.
pub fn matrix_year_options(current_year: i16) -> std::ops::RangeInclusive<i16>;
/// R-RPT-5d display decision for one cell.
#[derive(Debug, Clone, PartialEq)]
pub enum MatrixCell { Dash, One(Decimal), Three { actual: Decimal, budget: Decimal, diff: Decimal } }
pub fn matrix_cell(mode: MatrixMode, actual: Decimal, budget: Decimal) -> MatrixCell;
/// R-RPT-5e: ids behind a cell; month is 1..=12, None = the Total column.
pub fn matrix_drill(db: &Db, year: i16, scenario: Option<ScenarioId>, category: Option<CategoryId>, month: Option<u8>) -> Vec<TransactionId>;
pub fn matrix_drill_title(label: &str, year: i16, month: Option<u8>) -> String;  // "Groceries — Mar 2026" / "Groceries 2026"
```

## Steps

1. Read the Swift functions listed above side by side with `docs/business-rules.md` §9.
2. Write the shared helpers (`tone`, `live_category`, `is_transfer`, `budget_total_in_window`, the ranges) with tests.
3. **P&L** (R-RPT-1/2/3): one pass over live budget items (bucket by `live_category` id) and one pass over live
   transactions (filters: account not external, where `account_id == None` **passes**; visible under `scenario`;
   `start <= date <= end`; not transfer). Rows: iterate live categories, skip type Transfer, keep a row if budgeted ≠ 0
   or actual ≠ 0 (label = full name); add "Uncategorized" if non-zero; sort by label (`str` order); append "Total"
   with sums over the kept rows and the union of their ids. Store ids in the order they were encountered (date, id ascending: iterate a sorted list).
4. **Drill rows** (R-RPT-3b/3c), exact strings: account `"Unknown"` when none; contexts joined with `", "`; status = the
   display label (`"Pending"`…); scenario column `""` when `!scenario_active`, else the scenario name or `"Reality"`.
5. **Balance sheet** (R-RPT-4…4d). Obligations: live, status Funding or Spending, in name order (Swift's query is
   unsorted; we sort by name for determinism), with `balance = current_balance`.
6. **Matrix** (R-RPT-5…5e). Jan 1 and Dec 31 inclusive. Rows: every live category (transfer categories included) with any
   non-zero actual or budget month; plus Uncategorized; sorted by label. NET = per-month sums over **everything** that passed
   the filters (by R-RPT-0, this equals the sum of the rows).
7. Golden snapshots: add a test-only `fn render(rows) -> String` per report that prints a fixed-width text table
   (label, then amounts via `to_string()`), and snapshot it for `fixtures/small.md` and `fixtures/SampleData2.md`
   (imported with T05's `import_markdown` and the `America/Chicago` zone). Review every snapshot by eye.
8. Remove nothing else. Keep every public item documented with `///` and its rule ID.

## Tests (required)

Unit tests use small `TestDb` databases. The fixture tests use `fixtures/small.md`, whose expected values
were computed independently. If your code disagrees with them, re-check against the Swift code before
touching a number.

| Test name | Covers | What it checks |
|---|---|---|
| `r_rpt_0_deleted_category_counts_as_uncategorized` | R-RPT-0 | a tx whose category is soft-deleted lands in "Uncategorized" in the P&L and the matrix; a deleted Transfer category no longer excludes its txs |
| `r_rpt_1_budgeted_sign_and_count` | R-RPT-1 | Income +, Bill/Expense −, `abs` applied (a negative stored amount still gives −); n from `generate_dates` |
| `r_rpt_2_actual_filters` | R-RPT-2 | external accounts excluded; orphans **included**; transfers excluded; date bounds inclusive on both ends; deleted txs excluded |
| `r_rpt_2_scenario_visibility` | R-RPT-2, R-SCN-1 | Reality excludes scenario rows; with scenario S, S's rows are included and another scenario's rows are not |
| `r_rpt_3_rows_sorted_total_last` | R-RPT-3 | labels sorted; zero rows omitted; Transfer categories never get a row; Total is last with `is_total` and its sums; `difference()` |
| `r_rpt_3_empty_db_has_only_total` | R-RPT-3 | empty Db ⇒ exactly one row, Total, all zeros |
| `r_rpt_3a_tone` | R-RPT-3a | `tone(1)`, `tone(-1)`, `tone(0)` |
| `r_rpt_3b_drill_rows_format_and_order` | R-RPT-3b | ordering; `"Unknown"` account; contexts `"a, b"`; status label; scenario column `""` / `"Reality"` / name |
| `r_rpt_3c_budget_drill_rows` | R-RPT-3c | sorted by name; category `"Uncategorized"`; type/frequency strings; calculated total signed |
| `r_rpt_3d_default_and_quick_ranges` | R-RPT-3d, R-RPT-6 | today 2026-03-15: default (2026-02-15, 2026-03-15); YTD ⇒ (2026-01-01, today); 60D ⇒ (2026-01-14, today); labels and order of `QUICK_SELECT` and `ReportKind::ALL` |
| `small_pnl_q1_reality` | R-RPT-1/2/3 | small.md, 2026-01-01…2026-03-31, Reality ⇒ rows exactly (label, budgeted, actual): (Dining, 0, −94.75), (Groceries, −1950, −142.30), (Housing, −5220, −3064.99), (Housing/Utilities, −180, −208.55), (Income, 12000, 8000), (Uncategorized, 0, 2050), (Total, 4650, 6539.41) |
| `small_pnl_q1_plan_a` | R-RPT-2 | same range, scenario Plan A ⇒ Income actual 12000, Total actual 10539.41, the other rows unchanged |
| `r_rpt_4a_balance_sheet_accounts` | R-RPT-4, R-RPT-4a | small.md as of 2026-03-15, Reality ⇒ accounts [Checking (Chase) 6476.46, pending −120.45; Rewards Card (Amex) 62.95, pending −6.75]; Brokerage absent (external) |
| `r_rpt_4b_balance_sheet_totals` | R-RPT-4b | total 6539.41; pending −127.20 |
| `r_rpt_4c_obligations_use_stored_balance` | R-RPT-4c | Vacation Fund 500 (Funding) listed, Roof Repair (Completed) not; total obligations 500, even when tagged txs exist (stored, not computed, balance) |
| `r_rpt_4d_net_worth_and_available` | R-RPT-4d | net worth 6539.41, available 6039.41 |
| `r_rpt_4_scenario_and_future_pending` | R-RPT-4a | small.md as of 2026-12-31 with Plan A ⇒ Checking 8976.46 with pending 2379.55 (Future counts as pending); Rewards Card −87.05 with pending −156.75; total 8889.41 |
| `r_rpt_5_year_options_and_modes` | R-RPT-5 | `matrix_year_options(2026) == 2024..=2031`; mode labels and order |
| `r_rpt_5a_matrix_actuals` | R-RPT-5a | small.md 2026 Reality: Housing actual Jan −1500, Feb −1564.99; Dining Feb −88, Mar −6.75; Uncategorized Jan 2500, Mar −450; a Dec 31 tx is included (fix); an external-account tx is excluded |
| `r_rpt_5b_matrix_budget` | R-RPT-5b | Housing budget Jan −1540, Feb −2140, Jul −1500, Aug −2100; Groceries budget Mar −750, Apr −600; Housing/Utilities Jan −90, Feb 0; Income 4000 every month |
| `r_rpt_5c_matrix_rows_and_net` | R-RPT-5c | row labels exactly [Dining, Groceries, Housing, Housing/Utilities, Income, Uncategorized]; NET actual = [5000, 2258.91, −719.50, 0 × 9], Σ 6539.41; NET budget = [1770, 1260, 1620, 1860, 1170, 1710, 1810, 1150, 1810, 1900, 1060, 1900], Σ 19020; a Transfer category with a budget item **does** get a row |
| `r_rpt_5d_cell_display` | R-RPT-5d | All: (0,0) ⇒ Dash; (−5,0) ⇒ One(−5); (−5,−3) ⇒ Three{diff −2}; ActualOnly ⇒ One(actual) even if 0; BudgetOnly ⇒ One(budget) |
| `r_rpt_5e_matrix_drill_and_title` | R-RPT-5e | Housing, Feb ⇒ the ids of the Feb 3 and Feb 28 rows; Uncategorized, Total ⇒ 2 ids; titles `"Housing — Feb 2026"` (U+2014 em dash) and `"Housing 2026"` |
| `golden_reports_small` | R-RPT-1…5 | insta snapshots of the rendered P&L (Q1), balance sheet (2026-03-15) and matrix (2026) for small.md |
| `golden_reports_sampledata2` | R-RPT-1…5 | the same three for SampleData2.md (P&L 2025-01-01…2025-12-31, balance sheet 2025-12-31, matrix 2025) |

## Pitfalls

- **Calendar days, inclusive.** Every range check is `start <= date && date <= end` on `jiff::civil::Date`. Do not add a day.
- **Orphans count in the P&L and matrix but not in the balance sheet** (the balance sheet only iterates accounts). That is Swift's behaviour.
- **Transfer categories:** P&L rows skip them entirely; matrix rows keep them if they have a budget (their actuals are always 0).
- The balance sheet's obligations use the stored `current_balance`, unlike the dashboard (R-OBL-1). Don't "fix" this.
- The fixture tests need T05 (`import_markdown`). If T05 is not `done` yet, implement and pass the unit tests first, mark T08 `blocked` on T05 in STATUS.md, and finish the golden tests after T05.
- `Decimal` equality is numeric (`dec!(-142.30) == dec!(-142.3)`), but `to_string()` keeps the scale, so snapshot output may show `-142.30`. That is fine; keep it stable.

## Done when

```bash
cd PFRust
cargo test -p pf-core reports
cargo insta test -p pf-core --check
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules            # (after T00b) R-RPT-0 … R-RPT-6 covered
```

## Out of scope

- Rendering, colours and sheets (T23). Charts (T08b). Currency formatting is T04's helper; this task returns `Decimal`s.
