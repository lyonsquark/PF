# Swift → Rust source map

Every Swift file in `../PFApp/Sources/PFApp/` and where its behaviour goes in PFRust. When a
task says "port X", read the Swift file listed here. Paths on the left are relative to
`PFApp/Sources/PFApp/`. Module paths on the right are relative to `PFRust/`. UI module names are
suggestions; the owning task may refine them.

## App entry

| Swift file | What it does | Rust module(s) | Task(s) |
|---|---|---|---|
| `PFApp.swift` | DB path + legacy migration, ModelContainer, purge on launch, starts API, Cmd-S backup command, AppDelegate activation | `apps/pf/src/main.rs` (open engine, start API, run eframe); `pf-engine::engine` (`Engine::open`, `default_db_path`, purge); menu ⌘S in `pf-ui::shell` | T09, T13 |

## Models

| Swift file | Rust module(s) | Task(s) |
|---|---|---|
| `Models/Account.swift` | `pf-model` (`Account`, `AccountType`) | T01 |
| `Models/Transaction.swift` | `pf-model` (`Transaction`, `TransactionStatus` + labels/colours) | T01 |
| `Models/Category.swift` | `pf-model` (`Category`, `CategoryType`, `full_name`) | T01 |
| `Models/Context.swift` | `pf-model` (`Context`, `is_archived`) | T01 |
| `Models/Scenario.swift` | `pf-model` (`Scenario`) | T01 |
| `Models/Obligation.swift` | `pf-model` (`Obligation`, `ObligationStatus`) | T01 |
| `Models/BudgetItem.swift` | `pf-model` (`BudgetItem`, `BudgetItemType`, `BudgetFrequency`) | T01 |
| `Models/ImportMapping.swift` | `pf-model` (`ImportMapping`) | T01 |
| `Models/TimePeriod.swift` | `pf-model` (`TimePeriod`); `pf-core::filter` (start dates, R-FLT-1) | T01, T03 |
| `Models/BudgetProjector.swift` | `pf-core::projector` (`generate_dates`) | T02 |

## Managers

| Swift file | Rust module(s) | Task(s) |
|---|---|---|
| `Managers/ScenarioManager.swift` | `pf-engine::state::AppState` (active scenario, time period), `Engine::toggle_scenario`; balance in `pf-core::balance` | T03, T09 |
| `Managers/ObligationManager.swift` | `pf-core::obligation` (computed balance, reserved, available) | T03 |

## Services

| Swift file | Rust module(s) | Task(s) |
|---|---|---|
| `Services/UndoService.swift` | `pf-engine::engine` + `writer` (generic before/after undo), purge in `pf-store` + `Engine::open` | T07, T09 |
| `Services/MarkdownDataService.swift` | `pf-core::markdown` (import/export); `pf-engine::commands::backup` (wipe + replace, export) | T05, T10c |
| `Services/BackupManager.swift` | `pf-engine::commands::backup` (`perform_backup`, `backup_filename`, folder setting); toast/alert in `pf-ui` | T10c, T13, T21 |
| `Services/CSVImportService.swift` | `pf-core::csv_import` (parse, detect, lookup, unmapped, plan); `pf-engine::commands::import` (apply as one step) | T06, T10c |
| `Services/CSVDataService.swift` | Legacy simple importer, **not used** by any view (superseded by CSVImportService). Not ported. | – |
| `Services/APIServer.swift` | `pf-api::server` (tiny_http thread), `pf-api::router` (routing, query/path parsing) | T12 |
| `Services/APIHandlers.swift` | `pf-core::api_format` (all text/markdown/JSON bodies), `pf-api::router` (handlers), `pf-engine::commands::import::import_csv_with_saved_mappings` | T11, T12, T10c |

## Views

| Swift file | Rust module(s) | Task(s) |
|---|---|---|
| `Views/ContentView.swift` (`AppScreen`, `ContentView`, `SidebarView`, toolbar scenario menu, toast overlay) | `pf-ui::app` (`PfApp`, `Screen`), `pf-ui::shell` (sidebar, menu bar, toasts) | T13 |
| `Views/ContentView.swift` (`EditAccountView`, starting balance) | `pf-ui::dialogs::account` → `commands::accounts::{update_account, starting_balance_info, set_starting_balance}` | T10a, T16 |
| `Views/ContentView.swift` (`AccountDetailContainer`) | `pf-ui::screens::account` | T15a |
| `Views/ScenariosMenu.swift` | `pf-ui::shell::scenario_menu` | T13 |
| `Views/DashboardView.swift` | `pf-ui::screens::dashboard`; numbers from `pf-core::balance` + `obligation`; delete → `commands::accounts::delete_account` | T03, T14 |
| `Views/AccountBalanceView.swift` | `pf-ui::widgets::account_balance` (header), `pf-core::balance` | T03, T15a |
| `Views/AccountDetailView.swift` | `pf-ui::screens::account` (time period picker, import CSV button, add button) | T15a |
| `Views/TransactionListView.swift` | `pf-ui::screens::transactions` (table T15a, inline edit T15b, search/context menu/attachments T15c); rules in `commands::transactions` and `pf-core::filter` | T03, T10a, T15a, T15b, T15c |
| `Views/AmountEditField.swift` | `pf-ui::widgets::amount_field` | T15b |
| `Views/AddTransactionView.swift` | `pf-ui::dialogs::add_transaction` → `commands::transactions::{add_transaction, add_payee_pattern}` | T10a, T16 |
| `Views/AddAccountView.swift` | `pf-ui::dialogs::account` → `commands::accounts::create_account` | T10a, T16 |
| `Views/OrphanedTransactionsView.swift` | `pf-ui::dialogs::orphaned` → `commands::transactions::{orphaned_transactions, delete_orphaned}` | T10a, T21 |
| `Views/BudgetView.swift` | `pf-ui::screens::budget` → `commands::budget` | T10c, T17 |
| `Views/BudgetItemEditorView.swift` | `pf-ui::dialogs::budget_item` → `commands::budget::{save_budget_item, reproject_item, default_budget_input}` | T10c, T17 |
| `Views/BudgetProjectionView.swift` | `pf-ui::dialogs::projection` → `commands::budget::{generate_projection, last_projection}` | T10c, T17 |
| `Views/ObligationsView.swift` | `pf-ui::screens::obligations` → `commands::obligations` | T10b, T18 |
| `Views/ObligationDetailView.swift` | `pf-ui::dialogs::obligation_detail` → `commands::obligations::edit_obligation` | T10b, T18 |
| `Views/ScenariosView.swift` (+ `EditScenarioSheet`) | `pf-ui::screens::scenarios` → `commands::scenarios` | T10b, T19 |
| `Views/CategoriesView.swift` | `pf-ui::screens::categories` → `commands::categories` | T10b, T20 |
| `Views/ContextsView.swift` (+ `Color(hex:)`, `toHex`) | `pf-ui::screens::contexts` → `commands::contexts`; hex parsing in `pf-core` (R-CTX-4) | T04, T10b, T20 |
| `Views/SettingsView.swift` (+ `TextDocument`) | `pf-ui::screens::settings` → `commands::backup`, Monarch CSV entry to the wizard | T10c, T21 |
| `Views/CSVImportFlowView.swift` | `pf-ui::dialogs::csv_import` (wizard steps) → `commands::import::{run_csv_import, saved_column_assignments}` | T10c, T22 |
| `Views/ColumnMappingView.swift` | `pf-ui::dialogs::csv_import::columns` | T22 |
| `Views/TranslationMappingView.swift` | `pf-ui::dialogs::csv_import::translations`; mapping building logic in `pf-core::csv_import` | T06, T22 |
| `Views/ReportsView.swift` | `pf-ui::screens::reports` (picker, date range, quick-select) | T23 |
| `Views/ReportSubViews.swift` (P&L, P&L drill-downs, Balance Sheet, Monthly Matrix) | `pf-core::reports` (data); `pf-ui::screens::reports::{pl, balance_sheet, matrix}` | T08, T23 |

## Views/Charts

| Swift file (+ WebCharts HTML) | Rust module(s) | Task(s) |
|---|---|---|
| `Views/Charts/ChartsDashboardView.swift` | `pf-ui::screens::charts` (grid of chart cards, shared account selection) | T24a |
| `Views/Charts/AccountMultiSelectView.swift` | `pf-ui::widgets::account_multi_select` | T24a |
| `Views/Charts/WebChart.swift` (WKWebView bridge, `categoryHex`) | No WebView. `categoryHex` → `pf-core::charts::category_color` (R-CHT-10); click events become egui hover/click handling | T08b, T24a |
| `Views/Charts/NetWorthChartView.swift` + `networth.html` | `pf-core::charts::networth`, `pf-ui::charts::networth` (egui_plot lines) | T08b, T24a |
| `Views/Charts/SpendingComparisonChartView.swift` + `spending_comparison.html` | `pf-core::charts::spending_comparison`, `pf-ui::charts::spending_comparison` (lines) | T08b, T24a |
| `Views/Charts/BudgetVsActualChartView.swift` + `budget_vs_actual.html` | `pf-core::charts::budget_vs_actual`, `pf-ui::charts::budget_vs_actual` (lines) | T08b, T24a |
| `Views/Charts/ProfitLossChartView.swift` + `profitloss.html` | `pf-core::charts::profit_loss`, `pf-ui::charts::profit_loss` (bars) | T08b, T24a |
| `Views/Charts/IncomeExpenseTrendChartView.swift` + `income_expense_trend.html` | `pf-core::charts::income_expense_trend`, `pf-ui::charts::income_expense_trend` (lines/bars) | T08b, T24a |
| `Views/Charts/CategoryBreakdownChartView.swift` + `donut.html` | `pf-core::charts::category_breakdown`, `pf-ui::charts::donut` (custom painter) | T08b, T24b |
| `Views/Charts/ExpenseDistributionChartView.swift` + `expense_distribution.html` | `pf-core::charts::expense_distribution`, `pf-ui::charts::donut` | T08b, T24b |
| `Views/Charts/CashflowWaterfallChartView.swift` + `waterfall.html` | `pf-core::charts::waterfall`, `pf-ui::charts::waterfall` (bars with `base_offset` or painter) | T08b, T24b |
| `Views/Charts/SankeyChartView.swift` + `sankey.html` | `pf-core::charts::sankey`, `pf-ui::charts::sankey` (custom painter) | T08b, T24b |

## Resources

| Swift | Rust | Task |
|---|---|---|
| `WebCharts/echarts.min.js` | Not ported (no JavaScript). | – |
| `Resources/AppIcon.png`, `Resources/Info.plist` | `scripts/build_app.sh` (icns via `sips`/`iconutil`, plist for PFRust.app) | T25 |
| `../../build_app.sh` (PFApp root) | reference for `scripts/build_app.sh` | T25 |
| `../../Tests/PFAppTests/*.swift` | Their scenarios are covered by BEH-SCN-02, BEH-OBL-01 and BEH-MD-01/02 | T05, T10b |
