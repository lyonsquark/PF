# T24a: Charts dashboard, account filter, and the egui_plot charts

**Phase:** 3 · **Deps:** T13, T08b · **Crate(s):** `pf-ui` · **Size:** large (≈550 lines + 150 lines tests)

## Goal

Replace the Swift WebView/ECharts charts with native egui charts. This task builds the **Charts** screen: a header with
the scenario menu and an account multi-select, then a vertical stack of 9 chart cards. It implements the six line/bar
charts with `egui_plot` 0.38. The other three cards (donut, waterfall, Sankey) are placeholders here and are filled by T24b.

> **Shell contract (T13).** T13 created the placeholder `screens/charts.rs`. Turn it into the module directory
> `screens/charts/mod.rs` (same `#[derive(Default)] pub struct State` + `pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut egui::Ui)`,
> state in `ScreenStates.charts`), with one sub-module per chart. Scenario menu: `crate::scenario_menu::show(cx, ui)`.
> Colours: `widgets::colors::{GREEN, BLUE, RED, ORANGE}`. Cache every chart's data in `State` with
> `Cached<K, V>::get(cx.engine.revision(), &key, || …)`.
> Data (T08b, `pf_core::charts`): `ChartFilter { accounts, scenario, today }`, `default_selection(db)`, `selection_label(db, &sel)`,
> `select_all_button_label(db, &sel)`, `CHART_TITLES`, `NOT_ENOUGH_DATA`, `NO_BUDGETS`, `category_hex(name)`, `month_label_short`,
> `BALANCE_PERIODS` + `balance_over_time(db, &f, period) -> Vec<BalancePoint { date, balance }>`,
> `spending_comparison(db, &f, month_a, month_b) -> SpendingComparison { a, b: CumulativeSeries { label, points }, total_a, total_b }`,
> `step_month(d, ±1)`, `budget_pacing(db, &f, base_date) -> BudgetPacing { total_budget, budget_line, actual_line, spent, over_budget }`,
> `PL_MONTH_OPTIONS` + `profit_loss_by_month(db, &f, months) -> Vec<MonthSummary { label, income, expenses, .. }>` (`net()`),
> `TREND_PERIODS` + `income_expense_trend(db, &f, period)`, `DISTRIBUTION_PERIODS` + `expense_distribution(db, &f, period) -> Vec<Slice
> { name, amount, color }>`, `distribution_height(rows)`.
> Fallback: if a name here differs from the code, use the actual name in `pf-core`/`pf-ui`. Never compute chart data in the UI; add
> missing pieces to `pf_core::charts` with a test.

## Read first

- Swift: `../PFApp/Sources/PFApp/Views/Charts/ChartsDashboardView.swift`, `AccountMultiSelectView.swift`,
  `NetWorthChartView.swift`, `SpendingComparisonChartView.swift`, `BudgetVsActualChartView.swift`, `ProfitLossChartView.swift`,
  `ExpenseDistributionChartView.swift`, `IncomeExpenseTrendChartView.swift` (header controls and empty states)
- HTML (look and feel only): `../PFApp/Sources/PFApp/WebCharts/{networth,spending_comparison,budget_vs_actual,profitloss,expense_distribution,income_expense_trend}.html`
- `docs/egui-cheatsheet.md` → "Plots". Verified egui_plot 0.38 API: `Plot::new(id).height(h).legend(Legend::default())
  .x_axis_formatter(|mark, _range| …).y_axis_formatter(…).allow_zoom(false).allow_drag(false).allow_scroll(false)
  .include_y(0.0).show(ui, |p| …)`; `Line::new(name, PlotPoints::from(vec![[x, y]])).color(c).width(w).style(LineStyle::Dashed { length: 6.0 }).fill(0.0)`;
  `BarChart::new(name, bars).color(c).horizontal()`; `Bar::new(x, h).width(w).fill(c).name(n).base_offset(o)`;
  `PlotUi::pointer_coordinate()`, `PlotResponse::response`.
- Rules: R-CHT-1 … R-CHT-4, R-CHT-7, R-CHT-8, R-CHT-10, R-CHT-11 (these six charts do not respond to clicks)
- Task: `tasks/T08b-chart-data.md` (signatures above)

## Deliverables

```rust
// crates/pf-ui/src/screens/charts/mod.rs   (replaces T13's screens/charts.rs)
#[derive(Default)]
pub struct State {
    pub selected: Option<BTreeSet<AccountId>>,  // None until first shown ⇒ default_selection(db) (live, non-external)
    pub networth_period: TimePeriod,            // BALANCE_PERIODS, default PastYear
    pub compare_a: Option<Date>,                // None ⇒ today's month; B ⇒ step_month(a, -1)
    pub compare_b: Option<Date>,
    pub pacing_date: Option<Date>,              // None ⇒ today
    pub pl_months: u8,                          // PL_MONTH_OPTIONS, 0 ⇒ treat as 6
    pub breakdown_period: TimePeriod,           // T24b; BREAKDOWN_PERIODS default 1M
    pub distribution_period: TimePeriod,        // DISTRIBUTION_PERIODS default 1Y
    pub trend_period: TimePeriod,               // TREND_PERIODS default 1Y
    pub waterfall_date: Option<Date>, pub sankey_date: Option<Date>,   // T24b
    // one Cached<K, V> per chart, K = (ChartFilter-ish key, chart params)
}
pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut egui::Ui);
pub(crate) fn filter(cx: &UiCtx, st: &State) -> ChartFilter;   // { accounts, scenario: active, today }
pub(crate) fn chart_card(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui));

// crates/pf-ui/src/screens/charts/style.rs
pub const GREY: Color32;                 // #8e8e93 (others come from widgets::colors)
pub fn money_axis(v: f64) -> String;     // "$12k" (JS '$'+(v/1000).toFixed(0)+'k'); networth: "$950" below 1000 in magnitude
pub fn hex_color(hex: &str) -> Color32;  // "#rrggbb" from category_hex / Slice.color → Color32 (via widgets::color_from_hex)

// crates/pf-ui/src/widgets/account_multiselect.rs
pub fn account_multiselect(ui: &mut egui::Ui, db: &Db, accounts: &[(AccountId, String)], selected: &mut BTreeSet<AccountId>);

// one file per chart: crates/pf-ui/src/screens/charts/{networth,spending,pacing,profitloss,distribution,trend}.rs
pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut egui::Ui);
```

If `TimePeriod` lacks a suitable `Default`, initialise the periods in a `State::new()` called on first show (keep `Default` derivable).

## Screen layout

```
┌ Charts Dashboard (title, bold) ──────────── Scenario [Reality ▾]  Accounts [🏦 All Accounts] ┐
│ ┌ Account Balances Over Time ───────────────────────────────────────────────────────────────┐ │
│ │ $45,210.00 (large, bold)                                           [1Y ▾]                 │ │
│ │ (line chart, 250 px)                                                                      │ │
│ └───────────────────────────────────────────────────────────────────────────────────────────┘ │
│ ┌ Cumulative Spending (Month vs Month) ┐ … 9 cards in total, full width, stacked vertically   │
└───────────────────────────────────────────────────────────────────────────────────────────────┘
```

* The whole screen is a vertical `ScrollArea`. Each card is `egui::Frame::group(ui.style())` with a heading title,
  rounded corners and inner margin; width = the available width. Chart height 250 px (Sankey 340, distribution auto).
* Card order and titles (exact) = `CHART_TITLES` (T08b):
  1. "Account Balances Over Time" (T24a)
  2. "Cumulative Spending (Month vs Month)" (T24a)
  3. "Budget Pacing vs Actuals" (T24a)
  4. "Profit & Loss (Cashflow)" (T24a)
  5. "Categorical Breakdown" (T24b: placeholder label "Coming in T24b")
  6. "Monthly Cashflow Waterfall" (T24b: placeholder)
  7. "Expense Distribution" (T24a)
  8. "Income vs Expense Trend" (T24a)
  9. "Money Flow" (T24b: placeholder)
* Header: the scenario menu (`scenario_menu::show(cx, ui)`; the Swift dashboard shows it even though the toolbar hides it on this screen)
  and the account multi-select, each with a small caption "Scenario" / "Accounts" above it.

### Account multi-select

A small button `🏦 {label}` that toggles a popup (`egui::Popup` or `ui.menu_button`; check the 0.37 source for `Popup`) with:
"Filter Accounts" (strong), a "Select All"/"Deselect All" button (shows Deselect All when everything is selected), then one
checkbox per live account (`cx.caches.lookups(cx.engine).accounts`). The button label is `selection_label(db, &sel)`, and the
toggle button label is `select_all_button_label(db, &sel)`. On first show, set `st.selected = Some(default_selection(db))`
(all **non-external** live accounts).

### Charts (data from T08b; the UI only draws)

| # | Header (left / right) | Plot |
|---|---|---|
| 1 Net worth (R-CHT-1) — `balance_over_time` | left: last point's balance (title2, bold, currency). right: ComboBox over `BALANCE_PERIODS` (1M / 60D / YTD / 1Y / All) | One blue (#007aff) line, width 2.5, filled to 0 with low alpha. X = days since 1970 (`f64`); x formatter shows `Mon D` (or `Mon YYYY` when the range is over 120 days). Hover label: date + currency. Zoom/drag allowed on X only. |
| 2 Spending comparison (R-CHT-2) — `spending_comparison` | left: "Cumulative Comparison" (headline) and caption `Total: {total_b} vs {total_a}`. right: two month steppers `‹ Mar 2026 ›` (`step_month(d, ∓1)`) with "vs" between (A first, then B) | Series `a` blue width 3; series `b` `GREY` dashed width 2; legend = each series' `label`; x formatter `Day {n}`. |
| 3 Budget pacing (R-CHT-3) — `budget_pacing` | left: `{spent} Spent` (headline, red if `over_budget`) and caption `Budget: {total_budget}`. right: DatePickerButton | "Budget Pacing" grey dashed width 2; "Actual Spending" blue width 3 filled with low alpha. X = day of month (1..). `budget_line` / `actual_line` give (day, amount). If `total_budget ≤ 0` show `NO_BUDGETS` instead of the plot. |
| 4 Profit & loss (R-CHT-4) — `profit_loss_by_month` | left: "Monthly Profit & Loss". right: ComboBox over `PL_MONTH_OPTIONS` (3M / 6M / 9M / 12M) | Grouped bars per month: Income green and Expenses red (both positive heights; offset x by ±0.18 with width 0.32), plus a "Net" blue line through the month centres. X formatter = `MonthSummary.label`. A centred legend row below: ● Income ● Expenses ● Net Income. No click action (R-CHT-11). |
| 7 Expense distribution (R-CHT-7) — `expense_distribution` | left: `{Σ amount} Total` (headline). right: ComboBox over `DISTRIBUTION_PERIODS` | Horizontal `BarChart` (one `Bar` per category, ranked largest at the top) with each bar's colour = `hex_color(&slice.color)`; y formatter = the category name for integer marks; draw a value label to the right of each bar (`p.text(egui_plot::Text::new(name, PlotPoint::new(x, y), label))`, verified in 0.38). Height = `distribution_height(rows)`. No click action (R-CHT-11). |
| 8 Income vs expense trend (R-CHT-8) — `income_expense_trend` | left: the period label ("Past Year" / "All"). right: segmented over `TREND_PERIODS` | Two lines per month: Income green and Expenses red, both width 2.5, filled with alpha 0.1. X formatter = month label. |

Every chart shows `NOT_ENOUGH_DATA` (weak, centred, same height) when its series is empty, except where noted above.
Y axes use `money_axis`. Disable zoom/drag/scroll on all plots except #1 (prevents accidental panning while scrolling the page).

## Steps

1. Write the tests.
2. Move `screens/charts.rs` to `screens/charts/mod.rs`; add `style.rs`, `chart_card`, `State`, and the account multi-select widget.
3. The screen skeleton with all 9 cards (the T24b cards as placeholders).
4. Implement charts 1, 2, 3, 4, 7, 8, each caching its T08b output in a `Cached<K, V>` field of `State`
   (`get(cx.engine.revision(), &(selected accounts, scenario, params), …)`).
5. Run Done-when; then look at each chart manually with `fixtures/SampleData3.md` loaded.

## Tests (required)

`crates/pf-ui/tests/ui_charts.rs` (T13's `tests/common::{engine_with_fixture, harness}`, `PfApp::new_for_test`) plus unit tests in `style.rs`.

| Test name | Covers | What it checks |
|---|---|---|
| `ui_cht_dashboard_renders_with_fixture` | UI-CHT-01 (part) | All 9 card titles are present with the fixture; no panic over 3 frames |
| `ui_cht_dashboard_renders_empty_db` | UI-CHT-01 (part) | With an empty Db, every T24a chart shows "Not enough data" (pacing shows "No budgets set for this month.") |
| `ui_cht_account_filter_defaults_to_non_external` | — | Initial selection excludes external accounts; the label reads "{n} Accounts" or "All Accounts" accordingly |
| `ui_cht_select_all_toggle` | — | "Deselect All" ⇒ the label "No Accounts"; "Select All" ⇒ "All Accounts" |
| `ui_cht_period_picker_changes_cache_key` | R-CHT-1 | Switching networth 1Y → 1M produces a different (shorter) series from the cache |
| `ui_cht_month_stepper` | R-CHT-2 | Clicking "‹" on A moves it back one month (Jan → Dec of the previous year) |
| `money_axis_format` | — | `money_axis(12345.0) == "$12k"`, `money_axis(-2000.0) == "$-2k"` (match the JS `'$'+(v/1000).toFixed(0)+'k'`) |
| `hex_color_matches_category_hex` | R-CHT-10 | `hex_color(&category_hex("Groceries"))` equals the RGB of that hex (T08b golden-tests the hex itself) |
| `ui_cht_plot_charts_ignore_clicks` | R-CHT-11 | Clicking inside charts 1, 2, 3, 4, 7 and 8 opens no dialog |

## Pitfalls

- egui_plot ids must be unique per chart: `Plot::new("chart_networth")`, etc.
- The plot's X must be `f64`. Convert `jiff::civil::Date` with `(date - date(1970,1,1)).get_days()`, (or the helper T08b provides, if any).
- Never convert money to `f64` before the last step (T08b may already give `f64` series; that is fine).
- The month stepper must wrap years correctly; use `jiff` month arithmetic (`date.checked_add(1.month())`), not manual maths.
- egui_plot draws the legend inside the plot. For chart 4 the Swift legend is below the plot, so draw it as a separate row and disable the plot legend.

## Done when

```bash
cargo test -p pf-ui --test ui_charts
cargo test -p pf-ui --lib style
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules
```

## Out of scope

- Chart data (T08b). Donut, waterfall and Sankey (T24b). Drill-down dialogs (T23 provides `open_transactions`, used by T24b).
