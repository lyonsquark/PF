# T08b: Chart data for the 9 charts

**Phase:** 1 · **Deps:** T02, T03, T08 (and T05 for the fixture tests) · **Crate(s):** `pf-core` · **Size:** large (≈550 lines of code + tests)

## Goal

Every number, label, colour and drill-down behind the Charts screen, as pure, tested functions in
`pf-core::charts`. The UI tasks T24a (line/bar charts with egui_plot) and T24b (donut, waterfall and
Sankey with a custom painter) only *draw* these structs. The Swift app built JSON for ECharts in a
WebView; we replace that JSON with typed Rust structs.

## Read first

- Docs: `docs/business-rules.md` §10 (R-CHT-0 … R-CHT-10) and R-SCN-1, R-FLT-1, R-RPT-0, R-RPT-3b
- Swift (each `chartData` / `chartJSON` / drill handler): `../PFApp/Sources/PFApp/Views/Charts/*.swift`, in particular
  `ChartsDashboardView.swift`, `AccountMultiSelectView.swift`, `NetWorthChartView.swift`, `SpendingComparisonChartView.swift`,
  `BudgetVsActualChartView.swift`, `ProfitLossChartView.swift`, `CategoryBreakdownChartView.swift`,
  `CashflowWaterfallChartView.swift`, `ExpenseDistributionChartView.swift`, `IncomeExpenseTrendChartView.swift`,
  `SankeyChartView.swift`, `WebChart.swift` (`categoryHex`)
- ECharts templates (colours, sort order, labels): `../PFApp/Sources/PFApp/WebCharts/*.html`
- Code: T02 `projector::generate_dates`; T03 `is_visible(tx, active)` (R-SCN-1) and `period_start(period, today)` (R-FLT-1); T04 `color::parse_hex`; T08 `reports::{live_category, is_transfer, drill_rows}`. Reuse them.

## Deliverables

```text
crates/pf-core/src/lib.rs                       add `pub mod charts;`
crates/pf-core/src/charts/mod.rs                ChartFilter, selection, labels, colours, group_name, CHART_TITLES
crates/pf-core/src/charts/balance.rs            R-CHT-1
crates/pf-core/src/charts/spending.rs           R-CHT-2, R-CHT-3
crates/pf-core/src/charts/monthly.rs            R-CHT-4, R-CHT-8
crates/pf-core/src/charts/categories.rs         R-CHT-5, R-CHT-7
crates/pf-core/src/charts/waterfall.rs          R-CHT-6
crates/pf-core/src/charts/sankey.rs             R-CHT-9
```

```rust
use std::collections::BTreeSet;
use jiff::civil::Date;
use rust_decimal::Decimal;
use pf_model::{Db, AccountId, ScenarioId, TransactionId, TimePeriod};

// ---- mod.rs ----
/// The inputs every chart shares (R-CHT-0).
#[derive(Debug, Clone)]
pub struct ChartFilter { pub accounts: BTreeSet<AccountId>, pub scenario: Option<ScenarioId>, pub today: Date }
/// R-CHT-0: F (`transfers: true`) or F⁻ᵗ (`transfers: false`), sorted by (date, id) ascending.
pub fn filtered_transactions<'a>(db: &'a Db, f: &ChartFilter, transfers: bool) -> Vec<&'a pf_model::Transaction>;
/// R-CHT-0a
pub fn default_selection(db: &Db) -> BTreeSet<AccountId>;               // live, non-external
pub fn selection_label(db: &Db, selected: &BTreeSet<AccountId>) -> String;  // "All Accounts" / "No Accounts" / name / "3 Accounts"
pub fn select_all_button_label(db: &Db, selected: &BTreeSet<AccountId>) -> &'static str; // "Deselect All" / "Select All"
/// R-CHT-0b
pub fn month_bounds(d: Date) -> (Date, Date);
pub fn month_label_short(d: Date) -> String;      // "Mar 26"
pub fn month_label_long(d: Date) -> String;       // "March 2026"
pub fn period_window(period: TimePeriod, today: Date) -> (Option<Date>, Date);
/// R-CHT-0c: card titles in display order.
pub const CHART_TITLES: [&str; 9];
pub const NOT_ENOUGH_DATA: &str = "Not enough data";
pub const NO_BUDGETS: &str = "No budgets set for this month.";
/// R-CHT-5 grouping: parent name, else category name, else "Uncategorized" (live categories only, R-RPT-0).
pub fn group_name(db: &Db, tx: &pf_model::Transaction) -> String;
/// R-CHT-7 grouping: category name, else "Uncategorized".
pub fn category_name_or_uncategorized(db: &Db, tx: &pf_model::Transaction) -> String;
/// R-CHT-10
pub fn category_hex(name: &str) -> String;
// For the UI: parse colours with T04's `crate::color::parse_hex` (do not add a second parser).
/// A labelled amount with colour: donut slices and distribution bars.
#[derive(Debug, Clone, PartialEq)]
pub struct Slice { pub name: String, pub amount: Decimal, pub color: String }

// ---- balance.rs: R-CHT-1 ----
pub const BALANCE_PERIODS: [(&str, TimePeriod); 5];   // 1M, 60D, YTD, 1Y, All
#[derive(Debug, Clone, PartialEq)]
pub struct BalancePoint { pub date: Date, pub balance: Decimal }
pub fn balance_over_time(db: &Db, f: &ChartFilter, period: TimePeriod) -> Vec<BalancePoint>;

// ---- spending.rs: R-CHT-2, R-CHT-3 ----
#[derive(Debug, Clone, PartialEq)]
pub struct CumulativeSeries { pub label: String, pub points: Vec<(u8, Decimal)> }   // (day, cumulative)
#[derive(Debug, Clone, PartialEq)]
pub struct SpendingComparison { pub a: CumulativeSeries, pub b: CumulativeSeries, pub total_a: Decimal, pub total_b: Decimal }
/// `month_a` / `month_b`: any date inside the month.
pub fn spending_comparison(db: &Db, f: &ChartFilter, month_a: Date, month_b: Date) -> SpendingComparison;
pub fn step_month(d: Date, delta: i32) -> Date;      // the ◀ ▶ buttons (clamps the day)
#[derive(Debug, Clone, PartialEq)]
pub struct BudgetPacing {
    pub total_budget: Decimal,             // ≤ 0 ⇒ the UI shows NO_BUDGETS and the lines are empty
    pub budget_line: Vec<(u8, Decimal)>,
    pub actual_line: Vec<(u8, Decimal)>,
    pub spent: Decimal,
    pub over_budget: bool,
}
pub fn budget_pacing(db: &Db, f: &ChartFilter, base_date: Date) -> BudgetPacing;

// ---- monthly.rs: R-CHT-4, R-CHT-8 ----
pub const PL_MONTH_OPTIONS: [(&str, u8); 4];   // ("3M",3) ("6M",6) ("9M",9) ("12M",12); default 6
pub const TREND_PERIODS: [(&str, TimePeriod); 2];   // ("1Y", PastYear), ("All", All)
#[derive(Debug, Clone, PartialEq)]
pub struct MonthSummary { pub year: i16, pub month: i8, pub label: String, pub income: Decimal, pub expenses: Decimal }
impl MonthSummary { pub fn net(&self) -> Decimal; }
pub fn profit_loss_by_month(db: &Db, f: &ChartFilter, months: u8) -> Vec<MonthSummary>;
pub fn income_expense_trend(db: &Db, f: &ChartFilter, period: TimePeriod) -> Vec<MonthSummary>;

// ---- categories.rs: R-CHT-5, R-CHT-7 ----
pub const BREAKDOWN_PERIODS: [(&str, TimePeriod); 4];     // 1M (default), YTD, 1Y, All
pub const DISTRIBUTION_PERIODS: [(&str, TimePeriod); 5];  // 1M, 60D, YTD, 1Y (default), All
pub fn category_breakdown(db: &Db, f: &ChartFilter, period: TimePeriod) -> Vec<Slice>;    // ≤ 6 + "Other"
pub fn expense_distribution(db: &Db, f: &ChartFilter, period: TimePeriod) -> Vec<Slice>;  // ≤ 15, largest first
pub fn distribution_height(rows: usize) -> f32;                                          // 34 × rows + 40

// ---- waterfall.rs: R-CHT-6 ----
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarKind { Income, Expense, Other, Savings }
#[derive(Debug, Clone, PartialEq)]
pub struct WaterfallBar { pub label: String, pub y_start: Decimal, pub y_end: Decimal, pub kind: BarKind, pub color: String }
#[derive(Debug, Clone, PartialEq)]
pub struct Waterfall { pub month_label: String, pub total_in: Decimal, pub total_out: Decimal, pub bars: Vec<WaterfallBar> }
pub fn waterfall(db: &Db, f: &ChartFilter, base_date: Date) -> Waterfall;   // bars empty ⇒ NOT_ENOUGH_DATA
/// R-CHT-6b: (title, transaction ids) for a clicked bar label.
pub fn waterfall_drill(db: &Db, f: &ChartFilter, base_date: Date, label: &str) -> (String, Vec<TransactionId>);

// ---- sankey.rs: R-CHT-9 ----
#[derive(Debug, Clone, PartialEq)]
pub struct SankeyNode { pub name: String, pub color: String }
#[derive(Debug, Clone, PartialEq)]
pub struct SankeyLink { pub source: usize, pub target: usize, pub value: Decimal }   // indices into nodes
#[derive(Debug, Clone, PartialEq)]
pub struct Sankey { pub month_label: String, pub total_in: Decimal, pub total_out: Decimal,
                    pub nodes: Vec<SankeyNode>, pub links: Vec<SankeyLink> }    // nodes empty ⇒ NOT_ENOUGH_DATA
pub fn sankey(db: &Db, f: &ChartFilter, base_date: Date) -> Sankey;
pub fn sankey_drill(db: &Db, f: &ChartFilter, base_date: Date, node: &str) -> (String, Vec<TransactionId>);
```

Drill-down ids are rendered by T08's `drill_rows` (R-RPT-3b), so the UI reuses the P&L detail sheet.

## Steps

1. Write `mod.rs` helpers and `category_hex` first, with tests (the goldens are in R-CHT-10).
   - `category_hex`: `let mut h: i64 = 0; for b in name.bytes() { h = (b as i64).wrapping_add(h.wrapping_shl(5)).wrapping_sub(h); }`,
     then `hue = (h.unsigned_abs() % 360) as f64 / 360.0`, then the 6-sector HSB→RGB from `WebChart.swift` with s = 0.7 and
     b = 0.9, each channel `(c * 255.0) as u8` (truncation), formatted `format!("#{:02x}{:02x}{:02x}", r, g, b)`.
   - `month_label_short`: English 3-letter month + space + 2-digit year (`"Mar 26"`). `month_label_long`: full month name + 4-digit year. Write the 12 names as constants; don't depend on a locale.
   - `period_window`: `(period_start(period, today), today)` using T03's function.
2. `filtered_transactions` (R-CHT-0): live; `account_id` is `Some(id)` with `id` in `f.accounts`; visible for
   `f.scenario`; `transfers == false` ⇒ drop `reports::is_transfer`. Sort by `(date, id)`.
3. Port each chart from its Swift `chartData` exactly, applying the "(fix)" items written in the rule table
   (R-CHT-1a: the window start for All is the first transaction's date, and dates after today are dropped; R-CHT-9: empty when either side is missing).
   **Ties:** wherever Swift sorts a dictionary by value, sort by `(amount desc, name asc)`.
   - Month arithmetic: `today − N months` with jiff (`today.checked_sub(Span::new().months(N))`, which clamps the day).
   - `Expense` = |amount| of a transaction with `amount <= 0` for the monthly charts (Swift's `else` branch). Spending charts use `amount < 0`.
   - Budget pacing: `total / days_in_month × d` in Decimal. Tests compare after `round_dp(2)`.
4. Drill-downs: return the ids in `(date, id)` ascending order (T08's `drill_rows` re-sorts them for display), and the title strings from R-CHT-6b / R-CHT-9c (use U+2014 `—`).
5. Tests (table). Fixture tests import `fixtures/small.md` with T05's `import_markdown` (zone `America/Chicago`) and use
   `ChartFilter { accounts: {Checking, Rewards Card}, scenario: None, today: 2026-03-15 }` unless stated otherwise.
   (`default_selection` on small.md gives exactly these two accounts; Brokerage is external.)
6. Add insta snapshots of `format!("{:#?}", …)` for each chart on small.md, and review them by eye.

## Tests (required)

| Test name | Covers | What it checks |
|---|---|---|
| `r_cht_0_filter_rules` | R-CHT-0 | orphans and unselected accounts excluded; deleted excluded; scenario visibility; `transfers: false` drops Transfer-category txs; sorted by (date, id) |
| `r_cht_0a_default_selection_and_labels` | R-CHT-0a | small.md ⇒ {Checking, Rewards Card}; labels "All Accounts" (all 3 live accounts selected), "No Accounts", "Checking", "2 Accounts"; button "Deselect All" only when all are selected |
| `r_cht_0b_month_helpers` | R-CHT-0b | `month_bounds(2024-02-10) == (2024-02-01, 2024-02-29)`; `"Feb 24"`, `"February 2024"`; `step_month(2026-03-31, -1) == 2026-02-28` |
| `r_cht_0c_titles_and_messages` | R-CHT-0c | `CHART_TITLES` exact strings in order; the message constants |
| `r_cht_1a_balance_points_small` | R-CHT-1, R-CHT-1a | 1Y ⇒ 15 points: first (2025-03-15, 0), then (2026-01-24, 2500), (2026-01-25, 6500), (2026-01-31, 5000), (2026-02-03, 4935.01), (2026-02-14, 4847.01), (2026-02-17, 4758.91), (2026-02-25, 8758.91), (2026-02-28, 7258.91), (2026-03-01, 7138.46), (2026-03-02, 6996.16), (2026-03-05, 6996.16), (2026-03-10, 6546.16), (2026-03-12, 6539.41), last (2026-03-15, 6539.41) |
| `r_cht_1a_all_period_starts_at_first_tx` | R-CHT-1a | period All ⇒ first point date 2026-01-24 with balance 0 (fix) |
| `r_cht_1a_future_rows_excluded` | R-CHT-1a | scenario Plan A (all its rows are dated after today) ⇒ the same 15 points as Reality, ending (2026-03-15, 6539.41); the 03-25 / 04-01 / 04-05 rows contribute nothing (fix) |
| `r_cht_1b_edge_cases` | R-CHT-1b | no txs ⇒ empty; all txs before the window ⇒ two points at window start and today with the running sum; header value = last balance |
| `r_cht_2a_cumulative_series` | R-CHT-2, R-CHT-2a | A = Mar 2026 (current month) ⇒ 15 points; day 1 = 120.45, day 2 = 262.75, day 10 = 712.75, day 12..15 = 719.50. B = Feb 2026 ⇒ 28 points; day 3 = 64.99, day 14 = 152.99, day 17 = 241.09, day 28 = 1741.09; `total_a` 719.50, `total_b` 1741.09 |
| `r_cht_2b_labels_same_month_suffix` | R-CHT-2b | labels "Mar 26"/"Feb 26"; A == B ⇒ "Mar 26 (A)" and "Mar 26 (B)"; B is never truncated even if it is the current month |
| `r_cht_3_budget_total_and_message` | R-CHT-3 | small.md, March 2026 ⇒ total 2380 (Groceries 5 × 150, Gym 40, Mortgage 1500, Water 90; Insurance has no March date; Salary is Income). A Db with only Income items ⇒ total 0 (UI shows `NO_BUDGETS`) |
| `r_cht_3a_pacing_lines` | R-CHT-3a | budget line has 31 points; day 15 ≈ 1151.61 and day 31 = 2380.00 (`round_dp(2)`); actual line has 15 points (base date 2026-03-15) ending at 719.50 |
| `r_cht_3b_spent_and_over_budget` | R-CHT-3b | spent 719.50, `over_budget == false`; a Db where spending exceeds the budget ⇒ true |
| `r_cht_4_pl_by_month_small` | R-CHT-4 | 6 months ⇒ [Jan 26: income 6500, expenses 1500, net 5000], [Feb 26: 4000, 1741.09, 2258.91], [Mar 26: 0, 719.50, −719.50]; transfers excluded; only months with data |
| `r_cht_4_options` | R-CHT-4 | `PL_MONTH_OPTIONS` labels and values |
| `r_cht_5_breakdown_groups_top6_other` | R-CHT-5 | small.md 1M ⇒ [Housing 1708.55, Uncategorized 450, Groceries 142.30, Dining 6.75] (Utilities folded into its parent Housing; no Other); a synthetic Db with 8 groups ⇒ 6 + "Other" = the sum of the rest; colours from `category_hex`, Other `#8e8e93` |
| `r_cht_6a_waterfall_feb` | R-CHT-6, R-CHT-6a | Feb 2026 ⇒ bars Income (0→4000, `#34c759`), Housing (4000→2346.91, `#ff3b30`), Dining (2346.91→2258.91, `#ff6b55`), Savings (0→2258.91, `#007aff`); total_in 4000, total_out 1741.09; label "February 2026" |
| `r_cht_6a_waterfall_negative_savings_and_other` | R-CHT-6a | Mar 2026 ⇒ Income (0→0), Uncategorized, Groceries, Housing, Dining, then Savings (0→−719.50) in red `#ff3b30`; a Db with 8 expense groups ⇒ exactly one "Other" bar before Savings |
| `r_cht_6b_waterfall_drill` | R-CHT-6b | Feb: "Income" ⇒ [Payroll]; "Housing" ⇒ the 3 Housing-group txs (Mortgage, A｜B Hardware, City Water); "Savings" ⇒ all 5 Feb txs (4 Checking + Bistro Nights); title `"Housing — February 2026"` |
| `r_cht_7_distribution_by_category_name` | R-CHT-7 | 1Y ⇒ [Housing 3064.99, Uncategorized 450, Utilities 208.55, Groceries 142.30, Dining 94.75] (by category name, **not** parent); top 15 cap; `distribution_height(5) == 210.0` |
| `r_cht_8_trend_small` | R-CHT-8 | 1Y ⇒ same three months as `r_cht_4_pl_by_month_small`, chronological, labels "Jan 26", "Feb 26", "Mar 26" |
| `r_cht_9_sankey_empty_rules` | R-CHT-9 | Mar 2026 (no reality income) ⇒ nodes empty (fix); an empty Db ⇒ nodes empty |
| `r_cht_9a_sankey_nodes_feb` | R-CHT-9a | Feb 2026 ⇒ nodes in order [Income `#34c759`, Housing, Dining, Utilities, Savings `#30d158`] with R-CHT-10 colours for the expense nodes |
| `r_cht_9b_sankey_links_feb` | R-CHT-9b | links (Income→Housing 1653.09), (Income→Dining 88), (Income→Savings 2258.91), (Housing→Utilities 88.10); two income sources split proportionally in a synthetic Db; a link ≤ 0.01 is dropped |
| `r_cht_9c_sankey_drill` | R-CHT-9c | "Utilities" ⇒ [City Water]; "Income" ⇒ [Payroll]; "Savings" ⇒ all 5 Feb txs; " Housing " is trimmed to Housing (3 txs); title `"Utilities — February 2026"` |
| `r_cht_10_category_hex_goldens` | R-CHT-10 | Groceries `#92e544`, Dining `#4744e5`, Housing `#6744e5`, Utilities `#44e58a`, Shopping `#e54444`, Income `#e59d44`, Other/Uncategorized `#8e8e93`, and "A very long category name that overflows the hash" `#44e557` (exercises the wrapping) |
| `r_cht_10_hex_parses_with_t04` | R-CHT-10 | every colour constant used by the charts parses with `crate::color::parse_hex` |
| `golden_charts_small` | R-CHT-1…9 | insta snapshot of every chart struct for small.md (Debug format), reviewed by eye |

## Pitfalls

- **Decimal until the UI.** Every struct holds `Decimal`. Converting to `f64` here would make the tests flaky; T24 converts.
- **Two different groupings.** The donut, waterfall and Sankey group by *parent* (`group_name`); expense distribution groups by *category name*. Don't unify them.
- **Two different expense tests.** The monthly charts (P&L, trend) treat `amount <= 0` as expense; the spending charts use `amount < 0`.
- Swift's spending comparison only truncates series **A** to today. Keep that quirk (R-CHT-2a).
- Budget pacing ignores the account selection for the *budget* (all live non-income items), but the *actual* line respects it.
- `today` comes from `ChartFilter`. Never read the clock.
- Fixture tests need T05. If T05 isn't done, write the synthetic-Db tests first and mark the task `blocked` on T05 for the rest.

## Done when

```bash
cd PFRust
cargo test -p pf-core charts
cargo insta test -p pf-core --check
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules            # (after T00b) R-CHT-0 … R-CHT-10 covered
```

## Out of scope

- Drawing, hover, tooltips, the account-selector popover, and period pickers (T24a/T24b).
- Currency formatting of headers (T04 helper, used by T24).
