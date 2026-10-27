# T23: Reports screen (Profit & Loss, Balance Sheet, Monthly Matrix)

**Phase:** 3 · **Deps:** T13, T08 · **Crate(s):** `pf-ui` · **Size:** medium (≈450 lines + 150 lines tests)

## Goal

Port the Reports screen: a segmented picker for three reports, the date controls, and the drill-down dialogs.
All numbers come from `pf-core::reports` (T08). This task only lays them out, colours them and wires the clicks.

> **Shell contract (T13).** The screen is `screens/reports.rs` with `#[derive(Default)] pub struct State` and
> `pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut egui::Ui)` (state in `ScreenStates.reports`). Drill-downs are new
> `Dialog::ReportTransactions { title: String, ids: Vec<TransactionId> }` and
> `Dialog::ReportBudgetItems { title: String, ids: Vec<BudgetItemId>, start: Date, end: Date }` variants, with arms in
> `dialogs::show_active`, handled in `dialogs/report_drilldown.rs` (`show(cx, ui, …) -> Outcome`). Cache report results in
> `State` with `Cached<K, V>::get(cx.engine.revision(), &key, || …)`. Money via `widgets::money_label`; colours `widgets::colors`.
> Data (T08, `pf_core::reports`): `ReportKind::{ALL, label}`, `default_pnl_range(today)`, `QUICK_SELECT`, `quick_select_range(period, today, start)`,
> `profit_and_loss(db, start, end, scenario) -> Vec<PlRow>` (`PlRow { label, category_id, budgeted, actual, is_total, transaction_ids,
> budget_item_ids }`, `difference()`), `tone(d) -> Tone::{Positive, Negative, Zero}`,
> `balance_sheet(db, as_of, scenario) -> BalanceSheet { as_of, accounts: Vec<BsAccountRow>, total, pending_total, obligations:
> Vec<BsObligationRow>, total_obligations, net_worth, available }`, `MatrixMode::{ALL, label}`, `MONTH_SHORT`,
> `monthly_matrix(db, year, scenario) -> MonthlyMatrix { year, rows: Vec<MatrixRow>, net_actual, net_budget }`,
> `matrix_year_options(year)`, `matrix_cell(mode, actual, budget) -> MatrixCell::{Dash, One, Three}`,
> `matrix_drill(db, year, scenario, category, month)`, `matrix_drill_title`, `drill_rows(db, ids, scenario_active) -> Vec<DrillRow>`,
> `pnl_drill_title`, `budget_drill_rows(db, ids, start, end) -> Vec<BudgetDrillRow>`, `budget_drill_title`.
> Fallback: if a name here differs from the code, use the actual name in `pf-core`/`pf-ui`. Never compute report numbers in the UI;
> add missing pieces to T08's module (additive, with a test).

## Read first

- Swift: `../PFApp/Sources/PFApp/Views/ReportsView.swift`, `../PFApp/Sources/PFApp/Views/ReportSubViews.swift` (whole file)
- Rules: R-RPT-1 … R-RPT-6 (and the R-RPT-3b/3c, 4a…, 5a… sub-rules T08 added), R-OBL-6
- Task: `tasks/T08-reports.md` (signatures above)
- Docs: `docs/egui-cheatsheet.md` (TableBuilder, Grid via `egui::Grid`, DatePickerButton)

## Deliverables

```rust
// crates/pf-ui/src/screens/reports.rs
#[derive(Default)]
pub struct State {
    pub report: ReportKind,                   // T08 (default ProfitAndLoss)
    pub range: Option<(Date, Date)>,          // None until first shown ⇒ default_pnl_range(cx.engine.today())
    pub matrix_year: Option<i16>,             // None ⇒ today's year
    pub matrix_mode: MatrixMode,              // T08 (default All)
    pnl: Cached<(Option<ScenarioId>, Date, Date), Vec<PlRow>>,
    bs: Cached<(Option<ScenarioId>, Date), BalanceSheet>,
    matrix: Cached<(Option<ScenarioId>, i16), MonthlyMatrix>,
}
pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut egui::Ui);

// crates/pf-ui/src/dialogs/report_drilldown.rs
pub fn show_transactions(cx: &mut UiCtx, ui: &mut egui::Ui, title: &str, ids: &[TransactionId]) -> Outcome;
pub fn show_budget_items(cx: &mut UiCtx, ui: &mut egui::Ui, title: &str, ids: &[BudgetItemId], start: Date, end: Date) -> Outcome;
/// Used by T24b for chart clicks: opens Dialog::ReportTransactions.
pub fn open_transactions(cx: &mut UiCtx, title: String, ids: Vec<TransactionId>);
```

If `ReportKind` lacks `Default`, add `#[derive(Default)]` with `#[default] ProfitAndLoss` in T08's module (additive).
`State` lives in `ScreenStates.reports`, so it survives switching screens.

## Layout

```
┌ Reports ───────────────────────────────────────────────────────────────────────┐
│ [ Profit & Loss | Balance Sheet | Monthly Matrix ]                              │
│ P&L:   [2026-02-15📅] - [2026-03-15📅]                    [1M] [60D] [YTD] [1Y]  │
│ BS:    As of: [2026-03-15📅]                                                     │
│ ─────────────────────────────────────────────────────────────────────────────── │
│ (report body)                                                                   │
└─────────────────────────────────────────────────────────────────────────────────┘
```

* Quick buttons from `QUICK_SELECT` (1M / 60D / YTD / 1Y) set the range with `quick_select_range(period, cx.engine.today(), start)` (R-FLT-1, R-RPT-6).
* The Balance Sheet uses `end` as its "as of" date (shared with P&L, as in Swift). The Matrix has its own controls.

### Profit & Loss body

Table columns **Category | Budgeted | Actual | Difference** (right-align the money columns).

* Rows = `profit_and_loss(db, start, end, cx.engine.state.active_scenario)` in order (sorted by label, "Total" last). `is_total` ⇒ bold.
* Money colour from `tone(d)`: Positive green, Negative red, Zero normal (budgeted, actual, `difference()`).
* Clicking a **Budgeted** cell opens `Dialog::ReportBudgetItems { title: budget_drill_title(&row.label), ids: row.budget_item_ids, start, end }`;
  clicking an **Actual** cell opens `Dialog::ReportTransactions { title: pnl_drill_title(&row.label), ids: row.transaction_ids }`. Make those cells `egui::Button::new(text).frame(false)` so they are clickable and findable in tests.

### Balance Sheet body (max width 700)

```
Balance Sheet                       (heading)
As of Mar 15, 2026                  (weak)
┌──────────────────────────────────────────────────┐
│ Account              Balance        Pending      │ (bold header)
│ Checking             $1,234.56     -$45.00 (orange; blank if 0)
│   Chase (small grey institution, if non-empty)   │
│ ───────────────────────────────────────────────  │
│ Total                $9,999.99     -$45.00       │ (bold)
└──────────────────────────────────────────────────┘
┌ Obligation           Balance ┐   (only when there are active obligations)
│ Down Payment       $12,500.00 │   (normal colour if Funding, red if Spending)
│ Total Obligations  $15,500.00 │
└──────────────────────────────┘
┌ Net Worth                               $9,999.99 ┐ (large, bold; red if < 0)
│ Available (Net Worth − Obligations)    -$5,500.01 │
└───────────────────────────────────────────────────┘
```
Use `egui::Grid` with three or two columns, and `egui::Frame::group` backgrounds. Negative balances are red.
Data = `balance_sheet(db, end, active_scenario)`: `accounts` (BsAccountRow name/institution/total/pending), `total`, `pending_total`,
`obligations` (empty ⇒ the section is hidden; `is_funding` ⇒ normal colour, else red), `total_obligations`, `net_worth`, `available`.
R-OBL-6: obligation balances here are `current_balance`, **not** the computed balance (T08 already does this).

### Monthly Matrix body

```
Monthly Matrix        [Actual + Budget | Actual Only | Budget Only]  Year [2026 ▾]
Category         Jan        Feb   …   Dec        Total
Groceries     -$410.00   -$385.10     —        -$795.10   (bold total)
                -$400.00   -$400.00             (budget, small grey)  ← "All" mode only, when budget ≠ 0
                 -$10.00    +$14.90             (diff, small; green if ≥ 0 else orange)
NET              …                                          (bold)
```
* Data = `monthly_matrix(db, year, active_scenario)`. Month headers from `MONTH_SHORT`. The year picker lists `matrix_year_options(year)`.
* Cell rendering = `matrix_cell(mode, actual, budget)`: `Dash` ⇒ "—" (weak); `One(v)` ⇒ v coloured by `tone` (Zero = weak);
  `Three { actual, budget, diff }` ⇒ actual (tone colour), budget (small, weak), diff (small; green ≥ 0, `colors::ORANGE` < 0).
* Wrap the grid in `ScrollArea::both()`. Columns: category min 140 px, months and total 90 px each.
* Clicking a month cell ⇒ `Dialog::ReportTransactions { title: matrix_drill_title(&row.label, year, Some(m)), ids: matrix_drill(db, year,
  scenario, row.category_id, Some(m)) }`; the Total cell passes `None` for the month (R-RPT-5 sub-rules). The NET row is not clickable.

### Drill-down dialogs

* **Transactions** (title as given): rows = `drill_rows(db, &ids, active_scenario.is_some())`; table Date | Account ("Unknown" if none) | Payee | Amount (green > 0 / red < 0) |
  Memo (weak) | Context (names joined ", ") | Status (display label) | Scenario ("Reality" grey or name blue, shown **only**
  when a scenario is active; otherwise an empty column). Sorted date desc. A "Close" button. Use a large Modal (≈1200×400 or
  as big as the window allows).
* **Budget Items** (title as given): rows = `budget_drill_rows(db, &ids, start, end)`; Name | Category ("Uncategorized") | Type | Frequency | Amount (per period) |
  Start Date | Calculated Total (In Window) (bold; green/red; `calculated_total`). Sorted by name. "Close".
* T24b reuses the Transactions drill-down for chart clicks: expose `report_drilldown::open_transactions(cx, title, ids)`.

## Steps

1. Write the tests.
2. Implement `State`, the header controls and the quick buttons (`QUICK_SELECT` + `quick_select_range`).
3. Implement each body, caching T08's results in `State.{pnl, bs, matrix}` (`Cached::get(cx.engine.revision(), &key, …)`).
4. Implement both drill-down dialogs and `open_transactions`.
5. Run Done-when.

## Tests (required)

`crates/pf-ui/tests/ui_reports.rs` with T13's `tests/common::{engine_with_fixture, harness}` (SampleData2) and
`PfApp::new_for_test`; the engine's clock fixed by the helper.

| Test name | Covers | What it checks |
|---|---|---|
| `ui_rpt_01_each_report_renders` | UI-RPT-01 | Switching to each of the three reports renders without panic; P&L shows a "Total" label; BS shows "Net Worth"; Matrix shows "NET" |
| `ui_rpt_quick_select_sets_range` | R-RPT-6 | Clicking "YTD" sets start = Jan 1 of today's year and end = today |
| `ui_rpt_pl_actual_click_opens_drilldown` | R-RPT-3 | Clicking a category's Actual cell opens "{category} Transactions" listing exactly that row's transactions |
| `ui_rpt_pl_budget_click_opens_budget_items` | R-RPT-1 | Clicking Budgeted opens "{category} Budget Items" |
| `ui_rpt_bs_obligation_uses_current_balance` | R-OBL-6 | A Funding obligation with tagged transactions shows its `current_balance` (not the computed balance) in the balance sheet |
| `ui_rpt_bs_hides_obligations_when_none` | R-RPT-4 | With no Funding/Spending obligations there is no "Total Obligations" label |
| `ui_rpt_matrix_mode_switch` | R-RPT-5 | In "Actual Only" no "—" placeholder appears for a cell with only budget; in "All" it shows budget and diff |
| `ui_rpt_matrix_cell_drilldown_title` | R-RPT-5 | Clicking Groceries/Feb opens "Groceries — Feb 2026 Transactions" |
| `ui_rpt_scenario_column_only_when_active` | — | The drill-down shows "Reality" labels only when a scenario is active |

## Pitfalls

- Swift's balance-sheet obligations use `currentBalance`, not `computedBalance`. Do not "fix" this in the UI.
- Month names: use the fixed English short names Jan…Dec (Swift uses the locale; English is acceptable and testable).
- A grid of 13 × N `Button`s per frame is cheap, but **report computation is not**: always go through the cache.
- `DatePickerButton` needs a unique `id_salt` for start, end and as-of.

## Done when

```bash
cargo test -p pf-ui --test ui_reports
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules
```

## Out of scope

- Report math (T08). Charts (T24a/b).
