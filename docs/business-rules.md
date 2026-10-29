# Business Rules

Every behaviour PFRust must reproduce, extracted from the Swift app. Each rule has an
**ID** (`R-XXX-n`). Rules are the contract between the Swift app and the Rust port:

* Every rule ID **must** be covered by at least one automated test whose name or doc
  comment contains the ID (e.g. `fn r_bal_2_scenario_adds_to_reality()`).
  `cargo xtask rules` (task T00b) fails if any ID below has no test.
* The **Source** column points at the Swift code that defines the behaviour. When in
  doubt, read the source and match it — including its quirks, unless the rule says
  "(fix)" — in that case the Rust port intentionally differs, and the test asserts the fixed behaviour.

Notation: "live" = `!is_deleted`. "Reality" = `scenario_id == None`.
"Active scenario" = `AppState.active_scenario: Option<ScenarioId>`.

---

## 1. Scenario visibility & balances (`pf-core::balance`)

| ID | Rule | Source |
|---|---|---|
| R-SCN-1 | A transaction is **visible** under active scenario `S` iff it is live AND (`scenario_id == None` OR `scenario_id == Some(S)`). With no active scenario (Reality) it is visible iff live AND `scenario_id == None`. | `ScenarioManager.balance`, `AccountDetailView.filteringPredicate` |
| R-SCN-2 | Toggling a scenario that is already active turns it off (back to Reality); toggling a different one switches to it. Comparison is by id (Swift compared by name — (fix)). | `ScenarioManager.toggleScenario` |
| R-BAL-1 | Account **balance** = sum of `amount` over visible transactions of that account (R-SCN-1). All statuses count. | `ScenarioManager.balance`, `AccountBalanceView.calculateBalance` |
| R-BAL-2 | Account **cleared balance** = same as R-BAL-1 but only statuses `Cleared` or `Reconciled`. | `AccountBalanceView.calculateClearedBalance` |
| R-BAL-3 | **Reality balances** (shown alongside when a scenario is active) ignore the active scenario entirely: reality-all and reality-cleared. | `DashboardView.balanceMaps` |
| R-BAL-4 | Transactions with `account_id == None` contribute to no balance. | `DashboardView.balanceMaps` |
| R-BAL-5 | Dashboard summary cards use **cleared** balances (R-BAL-2) in Reality, and **all visible** balances (R-BAL-1, so pending and future rows of the active scenario too) when a scenario is active (changed from Swift by the owner; a line under the cards says which). Excluding external accounts: **Net Worth** = all non-external accounts; **Assets** = non-external checking + savings + cash + investment; **Credit Debt** = non-external credit; **Loan Debt** = non-external loan. | `DashboardView` summary totals |
| R-BAL-6 | With "Show Available Balance" on, each account's displayed balance (cleared and all) has `total_reserved(account)` (R-OBL-2) subtracted, and summary cards use those reduced values. The reserved amount is always computed under **Reality** (no scenario), even when a scenario is active, because Swift calls `totalReserved(for:)` without a scenario. | `DashboardView.clearedBalanceMap` |
| R-BAL-7 | **Running balance** in the transaction table: per account, iterate *all* live transactions of the query (not just the filtered time window) from oldest to newest, accumulating `amount`; each row shows the cumulative value at that row. Ties on the same date keep store order (sort by `(date, id)`). | `TransactionListView.runningBalances` |
| R-BAL-8 | When search is active, a second "search running balance" is shown: cumulative sum over only the filtered rows sorted oldest→newest. | `TransactionListView.searchRunningBalances` |
| R-BAL-9 | Dashboard account groups, in order: "Cash & Checking" (checking + cash), "Savings", "Credit Cards", "Loans", "Investments". Empty groups are hidden. Accounts sorted by name. | `DashboardView.body` |

## 2. Obligations (`pf-core::obligation`)

| ID | Rule | Source |
|---|---|---|
| R-OBL-1 | Obligation **computed balance** = `current_balance` + sum of amounts of transactions tagged with the obligation's context that are live and visible under the given scenario (R-SCN-1). No context ⇒ just `current_balance`. | `ObligationManager.computedBalance` |
| R-OBL-2 | Account **total reserved** = sum of computed balances of the account's live obligations whose status is `Funding` or `Spending`. | `ObligationManager.totalReserved` |
| R-OBL-3 | **Available balance** = total balance − total reserved. | `ObligationManager.availableBalance` |
| R-OBL-4 | New-obligation auto context name: lowercase the name, split on whitespace, join with `-`, keep only letters, digits and `-`. If empty → "". If a context with that name exists (case-insensitive), append `-2`, `-3`, … until unique. | `ObligationsView.generateContextName` |
| R-OBL-5 | Obligations list is grouped by status in `ObligationStatus::ALL` order. | `ObligationsView` |
| R-OBL-6 | The obligations list and detail show the computed balance (R-OBL-1) under **Reality**; the balance sheet report uses the raw `current_balance`. | `ObligationsView`, `ObligationDetailView`, `BalanceSheetReportView` |

## 3. Transactions — editing rules (`pf-engine::commands`)

| ID | Rule | Source |
|---|---|---|
| R-TXN-1 | **Lock rule**: deleted ⇒ locked. Reality mode (no active scenario) ⇒ everything editable. Scenario mode ⇒ reality rows (`scenario_id == None`) are locked; scenario rows editable. Locked rows cannot be edited by any command (engine returns `Err(Locked)`). | `TransactionListView.isTransactionLocked` |
| R-TXN-2 | Setting status `Future` on a reality transaction is rejected with message "Future status is only for scenario transactions" and **no change** happens. (Swift's toast says "changed to Pending" but does not change it; Rust matches the no-op, message text: `"Future status is only for scenario transactions"`.) | `changeStatus` |
| R-TXN-3 | Setting status `Cleared` or `Reconciled` on a *scenario* transaction whose date is after today is rejected ("Cannot clear a future-dated scenario transaction"), no change. | `changeStatus` |
| R-TXN-4 | Setting status `Pending`, `Cleared` or `Reconciled` on a scenario transaction **promotes** it to reality (`scenario_id = None`) in the same undoable step. Undo restores both status and scenario. | `changeStatus` |
| R-TXN-5 | Setting the same status is a no-op (no undo entry). | `changeStatus` |
| R-TXN-6 | **Duplicate**: each selected transaction is copied with `date = today`, same amount, payee, memo, attachment, status, account, scenario, category, contexts. One undo step for the whole batch. | `duplicateTransactions` |
| R-TXN-7 | **Delete**: soft-deletes all selected transactions as one undo step; name "Delete Transaction" (1) or "Delete Transactions" (>1). | `deleteTransactions` |
| R-TXN-8 | **Add**: Deposit ⇒ `+|amount|`; Withdrawal ⇒ `−|amount|`; Transfer ⇒ two transactions: source `−|amount|` payee `"Transfer to {dest}"`, destination `+|amount|` payee `"Transfer from {source}"`, both sharing date, memo, status, scenario, category, contexts. Empty memo ⇒ `None`. Added transactions get the active scenario. | `AddTransactionView.insertTransaction` |
| R-TXN-9 | Add dialog date picker: when the new transaction is reality (no active scenario), the date cannot be after today, whatever the status. Inline date edits: reality rows with status cleared/reconciled cannot be dated after today. | `AddTransactionView`, `TransactionListView` date column |
| R-TXN-10 | "Save & Another" keeps account, date and type; resets amount, payee, memo, category, contexts, transfer destination. | `saveAndReset` |
| R-TXN-11 | **Add to Budget** from a transaction creates a `BudgetItem { type: Bill, name: payee, amount, start_date: date, frequency: Monthly, category, account }` and shows toast `Added "{payee}" to Bills`. | `addToBudget` |
| R-TXN-12 | Inline edit of a text field (payee, memo) or amount commits **once** when the field loses focus or Enter is pressed, producing one undo entry named "Change Payee"/"Change Memo"/"Change Amount". Changing date/category/contexts commits immediately ("Change Date", "Change Category", "Change Contexts"). Committing an unchanged value creates no undo entry. | `bindUndo*` |
| R-TXN-13 | After a payee edit, if the transaction has no category and is not locked, run auto-categorize (R-CAT-1) and set the category as a separate undo step "Auto-categorize". | Payee column `onChange` |
| R-TXN-14 | Attachments: only open files whose type is PDF, image, plain text, RTF, spreadsheet or presentation (check by extension: pdf png jpg jpeg gif heic tiff webp txt md rtf csv xls xlsx numbers ppt pptx key); otherwise show "unsafe attachment" alert. Opening uses macOS `open`. Removing an attachment sets `attachment_path = None` (undoable, (fix): Swift's wasn't). | `openAttachmentSafely` |
| R-TXN-15 | **Orphaned transactions** = live transactions with `account_id == None`, sorted date desc. "Delete All" removes them (soft delete, one undo step — (fix): Swift hard-deleted). | `OrphanedTransactionsView` |

## 4. Transaction list filtering (`pf-core::filter`)

| ID | Rule | Source |
|---|---|---|
| R-FLT-1 | Time period start dates relative to today: PastMonth = today − 1 month; Past60Days = today − 60 days; YearToDate = Jan 1 of this year; PastYear = today − 1 year; Archival & All = none. | `TimePeriod.startDate` |
| R-FLT-2 | Window filter: Archival ⇒ only reality rows dated before (today − 1 year). Otherwise: scenario rows always pass (future projections stay visible); reality rows pass if no start date or `date >= start`. | `filteredTransactions` |
| R-FLT-3 | Search (when open and text non-empty, trimmed): scope Payee ⇒ payee contains query (case-insensitive); Category ⇒ category name or full name contains query; Context ⇒ any context name contains query; Status ⇒ status name (Pending, Cleared, Reconciled, Future) contains query (PFRust addition, not in Swift); Memo ⇒ memo contains query (PFRust addition; no memo ⇒ no match). | `filteredTransactions` |
| R-FLT-4 | Search date range (when toggled): keep `start <= date <= end` (inclusive days). Default range: today − 1 year … today. | `filteredTransactions` |
| R-FLT-5 | Default sort: date descending, then id descending. | `TransactionListView.init` |
| R-FLT-6 | Closing search clears text and turns off the date range. | search close button |

## 5. Categories & auto-categorization (`pf-core::categorize`)

| ID | Rule | Source |
|---|---|---|
| R-CAT-1 | **Auto-categorize(payee)**: empty/whitespace payee ⇒ None. Otherwise iterate live categories in name order, then their patterns in order; first match wins. A pattern matches if payee contains it case-insensitively, OR the pattern compiles as a regex and matches the payee. | `TransactionListView.autoCategorize` |
| R-CAT-2 | **Pattern validation** on save: patterns come from a comma-separated string, trimmed, empties dropped. A pattern "looks like regex" if it contains any of `^$.*+?[]{}()|\`. Regex-looking patterns that fail to compile are removed and reported to the user in an alert listing them as `• pattern`. | `CategoriesView.saveCategory` |
| R-CAT-3 | **Apply rules to uncategorized**: for the category being edited, apply its (valid) patterns to every live transaction with no category and non-empty payee; set the category on matches; report `Applied to N uncategorized transaction(s).` One undo step. | `applyRulesToUncategorized` |
| R-CAT-4 | `full_name` = `"{parent.name}/{name}"` if parent exists, else `name`. Category pickers list categories sorted by full name, case-insensitive. | `Category.fullName`, `sortedCategories` |
| R-CAT-5 | **Rec button** (add-transaction dialog): adds the trimmed payee to the selected category's match patterns. Disabled when no category, empty payee, or the payee is already a pattern (case-insensitive equal). Shows `Added "{payee}" to {fullName} auto-match`. | `addPayeeToMatchPatterns` |
| R-CAT-6 | Add-transaction dialog auto-fills category while typing payee: first category with a pattern contained (case-insensitive) in the payee (substring only, no regex). | `AddTransactionView.autoCategorize` |
| R-CAT-7 | Deleting a category soft-deletes it and its subcategories (cascade) **and sets `category_id = None` on every live transaction that referenced any of them**; undo restores the categories and those transactions' category. One undo step. | `Category` delete rules |
| R-CAT-8 | Categories view has one section per type (Income, Expense, Transfer); within each, a tree: parents then indented children, sorted by name. | `CategoriesView.categoryRow` |

## 6. Contexts, Scenarios, Accounts (`pf-engine::commands`)

| ID | Rule | Source |
|---|---|---|
| R-CTX-1 | A context referenced by any live obligation cannot be deleted; error: `Cannot delete context '#{ctx}' because it is currently assigned to the obligation '{ob}'. Please change the obligation's context or delete the obligation first.` | `ContextsView.deleteContext` |
| R-CTX-2 | Deleting a context soft-deletes it and removes it from every live transaction's context list. Undo restores both. | `deleteContext` |
| R-CTX-3 | Contexts view: two sections, Active (not archived) and Archived. Each row's transaction count is the live transactions tagged with the context that are visible under the active scenario (R-SCN-1); clicking the count opens them in a drill-down window (R-RPT-3b, without the Context column, with a total underneath). The scenario menu is shown here (PFRust; Swift hid it). | `ContextsView` |
| R-CTX-4 | Colour hex parsing: strip `#`, 6 hex digits = RGB, 8 = RGBA; anything else ⇒ None (render default blue). | `Color(hex:)` |
| R-SCN-4 | Creating a scenario requires a non-empty name (Swift checks the untrimmed name; Rust trims, a (fix)); empty notes are stored as None ((fix): Swift stored ""). |
| R-SCN-3 | Deleting a scenario: if it is active, deactivate first; soft-delete the scenario and all its live transactions, one undo step "Delete Scenario". | `deleteScenarioSafely` |
| R-ACC-1 | Deleting an account soft-deletes it, its live transactions and its live obligations; one undo step "Delete Account". | `DashboardView.deleteAccount` |
| R-ACC-2 | **Starting balance**: the account editor finds a transaction with payee "Starting Balance". The "other transactions" are the account's live **reality** transactions whose payee is not "Starting Balance"; the starting-balance row itself is the live reality row with payee "Starting Balance". Starting-balance date = (earliest other transaction date) − 1 day, or today − 1 day if none. Save: if it exists, update amount and date; else if amount ≠ 0 create `{payee: "Starting Balance", memo: "Initial account balance", status: Cleared}`. If no transactions and amount = 0: do nothing. | `EditAccountView.saveStartingBalance` |
| R-ACC-3 | "Calculate" button: starting = entered current balance − sum of all other (non-starting-balance) transactions. Editor opens with current = starting + that sum. | `EditAccountView` |
| R-ACC-4 | Sidebar groupings: A-Z (flat, by name), Institution (grouped by institution), Type (grouped by account type). | `SidebarView.AccountGrouping` |
| R-PRG-1 | **Purge on launch**: hard-delete every soft-deleted row (all entity types) before the UI starts, and remove dangling context links from `transaction_contexts`. Never purge mid-session. | `UndoService.purgeSoftDeleted` |

## 7. Undo / redo (`pf-engine::undo`)

| ID | Rule | Source |
|---|---|---|
| R-UND-1 | Every user mutation goes through the engine and is one undo step with a human name (e.g. "Delete Account"). | `UndoService.perform` |
| R-UND-2 | Undo restores the exact prior state of every row touched (including relationships and soft-delete flags); redo re-applies it. Undo→redo→undo is stable (ping-pong). | `UndoCoordinator` |
| R-UND-3 | A new edit after an undo clears the redo stack. | NSUndoManager semantics |
| R-UND-4 | The undo history is in-memory only and empty at launch. | — |
| R-UND-5 | Edit menu shows `Undo {name}` / `Redo {name}`; disabled when empty. Shortcuts Cmd-Z / Shift-Cmd-Z. | macOS standard |
| R-UND-6 | Every applied step is persisted to SQLite in a single SQL transaction before `apply` returns; undo/redo are persisted the same way. | `try? context.save()` |

## 8. Budget & projection (`pf-core::projector`, `pf-core::budget`)

| ID | Rule | Source |
|---|---|---|
| R-PRJ-1 | `generate_dates(item, from, until) -> Vec<Date>`: effective end = `min(until, item.end_date)` if end_date set. All dates are calendar days. | `BudgetProjector.generateDates` |
| R-PRJ-2 | Weekly: start at `item.start_date`, step +7 days while `<= end`; emit those `>= from`. | `generateSimpleDates` |
| R-PRJ-3 | Month-based: preferred day = day-of-month of `start_date`. For offset k = 0, inc, 2·inc, … the candidate is (start month + k) with day clamped to that month's last day (Jan 31 → Feb 28/29 → Mar 31; never drifts). inc: Monthly 1, EveryOtherMonth 2, Quarterly 3, Yearly 12, SpecificMonths 1. Stop when candidate > end. Emit when candidate >= from and >= start_date. | `generateDates` |
| R-PRJ-4 | SpecificMonths emits only candidates whose month is in `specific_months`. Empty list ⇒ nothing. | `generateDates` |
| R-PRJ-5 | Safety stop after 1200 months of offset. | `generateDates` |
| R-PRJ-6 | **Generate projection** into scenario S for selected items: for each generated date create a transaction: amount = `−|amount|` for Expense/Bill, `+|amount|` for Income; payee = name for Bill and Income, `"Expense: {name}"` for Expense; memo `"Auto-generated from Budget"`; status `Future`; account = item.account; scenario = S; category = item.category. One undo step "Generate Projection". Returns the count. | `BudgetProjectionView.generateProjection` |
| R-PRJ-7 | Projection can target a new scenario by name (trimmed, non-empty; created in the same undo step) or an existing one. Generate disabled when no target. | `BudgetProjectionView` |
| R-PRJ-8 | After generating, save settings `lastProjectionScenario`, `lastProjectionStartDate`, `lastProjectionEndDate`; the dialog prefills from them next time. | `prefillFromLastProjection` |
| R-PRJ-9 | **Reproject** a single item (budget item editor): first save the editor's pending edits to the item, then **add** that item's generated transactions for the range to the chosen scenario per R-PRJ-6. Existing rows are **not** removed (matches Swift). Toast: `Added {n} transaction(s) to {scenario}` ("transaction" when n == 1). One undo step "Reproject". | `reprojectItem` |
| R-BUD-1 | Budget view tabs: Bills, Expenses, Income (by `item_type`), live items only, sorted by name. | `BudgetView` |
| R-BUD-2 | Budget item editor: SpecificMonths shows 12 month toggles; End Date toggle; end date cannot precede start date. | `BudgetItemEditorView` |

## 9. Reports (`pf-core::reports`)

All report dates are calendar days and all ranges are **inclusive on both ends** (fix: Swift compared
timestamps, so a start date carrying the current time of day silently dropped earlier same-day
transactions, and the matrix dropped Dec 31 transactions after midnight).

| ID | Rule | Source |
|---|---|---|
| R-RPT-0 | In every report and chart, a transaction or budget item whose `category_id` points at a soft-deleted category counts as **uncategorized** (consistent with R-CAT-7). "Transfer" means the live category's type is `Transfer`. (fix: Swift kept the stale reference, so rows and totals could disagree.) | — |
| R-RPT-1 | **P&L budgeted**: for each live budget item, n = number of `generate_dates(item, start, end)`; total = amount × n; signed `+|total|` for Income else `−|total|`; summed per category (None = uncategorized). | `ProfitAndLossReportView.generateReportRows` |
| R-RPT-2 | **P&L actual**: live transactions, account not external (orphans count — Swift's `account?.isExternal != true`), visible under active scenario (R-SCN-1), `start <= date <= end`, category type ≠ Transfer; summed per category. | same |
| R-RPT-3 | P&L rows: one per non-transfer category with budgeted ≠ 0 or actual ≠ 0 (label = full name), plus "Uncategorized" if non-zero; sorted by label; then a final "Total" row. Difference = actual − budgeted. Each row drills down to its transactions and budget items. | same |
| R-RPT-3a | P&L amount colours: > 0 green, < 0 red, 0 normal. The Total row is bold. Total budgeted/actual = sums over the listed rows. | same |
| R-RPT-3b | **Transaction drill-down** (shared by P&L, matrix, waterfall, Sankey): rows sorted by date descending (ties: id descending). Columns: Date, Account (name, or "Unknown" if none), Payee, Amount, Memo ("" if none), Context (names joined with ", "), Status (display label), Scenario (only when a scenario is active: the scenario name, or "Reality" for reality rows; otherwise ""). Title for P&L: `"{label} Transactions"`. PFRust additions: a row can be clicked to show that transaction in its account, selected and scrolled to (the account's time period widens to All if it leaves the transaction out); in a report window, changing a row's category takes the row out of the window, while a context window keeps it. | `PLTransactionDetailView` |
| R-RPT-3c | **Budget drill-down**: the row's budget items sorted by name. Columns: Name, Category (name or "Uncategorized"), Type, Frequency, Amount (per period), Start Date, Calculated Total (In Window) = signed `|amount × n|` per R-RPT-1. Title `"{label} Budget Items"`. | `PLBudgetDetailView` |
| R-RPT-3d | Report screen defaults: P&L range start = today − 1 month, end = today. Quick-select buttons "1M", "60D", "YTD", "1Y" apply R-RPT-6. Report picker order: "Profit & Loss", "Balance Sheet", "Monthly Matrix". | `ReportsView` |
| R-RPT-4 | **Balance sheet** as of a date (default today; the start date is ignored). Accounts: live, **non-external**, sorted by name. | `BalanceSheetReportView` |
| R-RPT-4a | Per account: `total` = sum of amounts of visible transactions (R-SCN-1) with `date <= as_of`; `pending` = the part of that sum whose status is `Pending` or `Future`. The institution is shown under the name when non-empty. Pending is displayed only when ≠ 0. | `buildAccountInfoMap` |
| R-RPT-4b | Total row = Σ account totals and Σ pending (pending shown only when ≠ 0). Negative totals red, pending orange. | same |
| R-RPT-4c | **Active obligations** = live obligations with status Funding or Spending (any account, no scenario filter). Each shows its stored `current_balance` (**not** the computed balance of R-OBL-1: Swift quirk, keep), coloured normal for Funding and red otherwise. "Total Obligations" = Σ `current_balance`. The section is hidden when there are none. | `activeObligations`, `totalObligations` |
| R-RPT-4d | Summary: "Net Worth" = total of R-RPT-4b; "Available (Net Worth − Obligations)" = net worth − total obligations. Negative values red. | body |
| R-RPT-5 | **Monthly matrix**: year selector offers current year − 2 … current year + 5 (default current year); mode picker "Actual + Budget" (default), "Actual Only", "Budget Only". | `MonthlyMatrixReportView` |
| R-RPT-5a | Actuals: live transactions, account not external (orphans count), Jan 1 ≤ date ≤ Dec 31 of the year, category type ≠ Transfer, visible (R-SCN-1); summed per (category, month). | `generateMatrixData` |
| R-RPT-5b | Budget: for each live budget item, every date of `generate_dates(item, Jan 1, Dec 31)` adds `+|amount|` (Income) or `−|amount|` (Bill/Expense) to its (category, month). | same |
| R-RPT-5c | Rows: every live category (any type, transfer included) with a non-zero actual or budget in any month, labelled with its full name; plus "Uncategorized" if non-zero; sorted by label. Each row has 12 month cells plus a Total (Σ months). A final **"NET"** row holds per-month grand totals of actual and budget and their sums. | same |
| R-RPT-5d | Cell display: Actual Only ⇒ actual; Budget Only ⇒ budget; Actual + Budget ⇒ "—" when both are 0, else actual, and when budget ≠ 0 also budget (small) and diff = actual − budget (green if ≥ 0, else orange). Value colours: > 0 green, < 0 red, 0 secondary. Month headers are English short names "Jan" … "Dec". | `monthCell` |
| R-RPT-5e | Clicking a category row's month cell or Total cell drills down (R-RPT-3b) to the transactions behind that number (same filters as R-RPT-5a, the same category, or uncategorized for "Uncategorized"), restricted to that month for month cells. Title `"{label} — {Mon} {year}"` (em dash, e.g. `"Groceries — Mar 2026"`) or `"{label} {year}"` for Total. The NET row is not clickable. | `drillDown` |
| R-RPT-6 | Report quick-select buttons set the date range from a `TimePeriod` (R-FLT-1), end = today. | `ReportsView.quickSelectButton` |

## 10. Charts data (`pf-core::charts`)

Each chart's *data* is a pure function in `pf-core::charts` returning `Decimal`s (converted to
`f64` only in `pf-ui`); rendering lives in `pf-ui` (T24a/T24b). Where Swift sorts a dictionary by
value only (undefined order on ties), Rust breaks ties by name ascending.

| ID | Rule | Source |
|---|---|---|
| R-CHT-0 | **Common filter F**: live transaction, `account_id` is in the selected account set (orphans never included), visible under the active scenario (R-SCN-1). "F⁻ᵗ" = F plus category type ≠ Transfer. "Spending" = F⁻ᵗ plus `amount < 0`. | every chart view |
| R-CHT-0a | Account selection defaults to all live **non-external** accounts the first time the Charts screen opens. Selector label: all accounts selected ⇒ "All Accounts"; none ⇒ "No Accounts"; one ⇒ that account's name; else `"{n} Accounts"`. The button reads "Deselect All" when all are selected, else "Select All". | `ChartsDashboardView`, `AccountMultiSelectView` |
| R-CHT-0b | Month window of a base date = first … last day of its month. Labels: short `"MMM yy"` (e.g. "Mar 26"), long `"MMMM yyyy"` (e.g. "March 2026"), English. Period pickers use labels 1M/60D/YTD/1Y/All → TimePeriod (R-FLT-1); a period window is `[start or −∞, today]`. | chart views |
| R-CHT-0c | Empty data ⇒ the card shows "Not enough data" (budget pacing: "No budgets set for this month."). Chart cards in order with titles: "Account Balances Over Time", "Cumulative Spending (Month vs Month)", "Budget Pacing vs Actuals", "Profit & Loss (Cashflow)", "Categorical Breakdown", "Monthly Cashflow Waterfall", "Expense Distribution", "Income vs Expense Trend", "Money Flow". | `ChartsDashboardView` |
| R-CHT-1 | **Account balances over time**: periods 1M, 60D, YTD, 1Y (default), All. Transactions F (transfers **included**), sorted by (date, id) ascending. | `NetWorthChartView` |
| R-CHT-1a | Walk all of them keeping a running sum. For each transaction inside the window: before adding the first point, push `(window_start, running − amount)`; then one point per day holding the running sum after the day's last transaction. Window start for All = the first transaction's date (fix: Swift used `distantPast`, year 1). Transactions dated after today are excluded entirely (fix: Swift added them to the final point). | `chartData` |
| R-CHT-1b | After the walk: no points and running ≠ 0 ⇒ two points `(window_start, running)`, `(today, running)`. Otherwise, if the last point is before today, append `(today, running)`. No points and running = 0 ⇒ empty. Header value = last point's balance (0 when empty). Line colour `#007aff`, light area fill. | same |
| R-CHT-2 | **Cumulative spending, month A vs month B**: A defaults to this month, B to last month; each has ◀ ▶ month steppers. Spending transactions only. | `SpendingComparisonChartView` |
| R-CHT-2a | Series for a month: day d = 1 … maxDay with value = cumulative Σ |amount| up to day d. maxDay for A = today's day if A is the current month, else the month's length; maxDay for B = the month's length (Swift quirk: B is never truncated). | `buildSeries` |
| R-CHT-2b | Labels `"MMM yy"`; when A and B are the same month, add the suffixes `" (A)"` / `" (B)"`. Header `"Total: {B total} vs {A total}"` (B first, Swift order). A is a solid `#007aff` line, B a dashed `#8e8e93` line; the x-axis is labelled `"Day {d}"`. | same |
| R-CHT-3 | **Budget pacing**: base date (default today) selects the month. Total budget = Σ over live budget items with type ≠ Income of `|amount| × count(generate_dates(item, month_first, month_last))` (budget items are **not** filtered by account selection). If total ≤ 0 ⇒ "No budgets set for this month.". | `BudgetVsActualChartView` |
| R-CHT-3a | Budget line: for d = 1 … days_in_month, `total / days_in_month × d` (dashed `#8e8e93`, "Budget Pacing"). Actual line: spending transactions in the month, cumulative Σ |amount| for d = 1 … min(base date's day, days_in_month) (solid `#007aff`, "Actual Spending"). | `buildChartData` |
| R-CHT-3b | Header `"{spent} Spent"` (red when spent > total), where spent = the last actual point (0 if none), and `"Budget: {total}"`. | body |
| R-CHT-4 | **Profit & loss by month**: picker 3M, 6M (default), 9M, 12M. Window `[today − N months, today]`, F⁻ᵗ. Group by calendar month in chronological order, labelled `"MMM yy"`; income = Σ positive amounts, expenses = Σ |non-positive amounts|, net = income − expenses. Only months that have transactions appear. Bars: income green `#34c759`, expenses red `#ff3b30`; net is a `#007aff` line. Legend "Income", "Expenses", "Net Income". Clicking does nothing. | `ProfitLossChartView` |
| R-CHT-5 | **Category breakdown (donut)**: periods 1M (default), YTD, 1Y, All. Spending in the window, grouped by **group name** = parent category's name if the category has a parent, else the category's name, else "Uncategorized". Sorted by amount descending; the top 6 are kept, and the rest are summed into "Other" (only if > 0). Header `"{total} Total"`. Tooltip: name, amount, `"{pct}% of total"`. Clicking does nothing. | `CategoryBreakdownChartView` |
| R-CHT-6 | **Monthly cashflow waterfall**: base date month, F⁻ᵗ. income = Σ positive; expenses grouped by group name (R-CHT-5). Empty if income = 0 and there are no expense groups. | `CashflowWaterfallChartView` |
| R-CHT-6a | Bars in order: "Income" (0 → income); the top 6 expense groups descending, each (running → running − value); if more groups remain, one "Other" bar for their sum; finally "Savings" (0 → running). Colours: income `#34c759`; expense bars cycle `#ff3b30 #ff6b55 #ff9500 #af52de #32ade6 #ffcc00 #ff6961 #ff453a`; savings `#007aff` if ≥ 0 else `#ff3b30`. Bar labels: `+$x` income, `-$x` expense, signed savings, whole dollars. | `chartData`, `waterfall.html` |
| R-CHT-6b | Header: `"MMMM yyyy"`, `"{income} In"` (green), `"{expenses} Out"` (red). Clicking a bar drills down (R-RPT-3b): Income ⇒ positive transactions; Savings ⇒ all filtered transactions; Other ⇒ negative transactions whose group is not in the top 6; otherwise ⇒ negative transactions of that group. Title `"{label} — {MMMM yyyy}"`. | `handleBarTap` |
| R-CHT-7 | **Expense distribution**: periods 1M, 60D, YTD, 1Y (default), All. Spending in the window grouped by **category name** (not parent; else "Uncategorized"), sorted descending, top 15. Horizontal bars with the largest at the top, value label on the right, colour R-CHT-10. Card height = 34 px × rows + 40 px. Header `"{total} Total"`. | `ExpenseDistributionChartView` |
| R-CHT-8 | **Income vs expense trend**: periods 1Y (default), All. F⁻ᵗ in the window; per calendar month (chronological, only months with data) income = Σ positive and expenses = Σ |non-positive|; label `"MMM yy"`. Header shows the period label ("Past Year" / "All"). Income line `#34c759`, expenses `#ff3b30`, smooth with a light area fill. | `IncomeExpenseTrendChartView` |
| R-CHT-9 | **Money flow (Sankey)**: base date month, F⁻ᵗ. Income by category name (no category ⇒ "Income"); expenses by group name (R-CHT-5); sub-category breakdown (group → category name) only for categories that have a parent. Empty (⇒ "Not enough data") if there is no income **or** no expense (fix: Swift rendered a blank chart when only one side existed). savings = max(0, total income − total expenses). Header same as R-CHT-6b. | `SankeyChartView` |
| R-CHT-9a | Nodes, in order, unique by name (first occurrence wins): income nodes by amount descending (colour `#34c759`); expense group nodes by amount descending (colour R-CHT-10); sub-category nodes, groups by name ascending, subs by amount descending (R-CHT-10); "Savings" (`#30d158`) if savings > 0. | `chartJSON` |
| R-CHT-9b | Links: for each income i and each expense group e, `i × e / total_income` if > 0.01; for each income i, `i × savings / total_income` → "Savings" if > 0.01; for each group → sub, its amount if > 0.01. Link order follows node order. | same |
| R-CHT-9c | Clicking a node named n (trimmed) drills down: "Savings" ⇒ all filtered transactions; otherwise transactions whose group name == n, or category name == n, or (category name, else "Income" if amount > 0 else "Uncategorized") == n. Title `"{n} — {MMMM yyyy}"`. | `handleNodeTap` |
| R-CHT-11 | Only the waterfall (R-CHT-6) and Sankey (R-CHT-9) charts respond to clicks (drill-down); clicks on the other charts do nothing. |
| R-CHT-10 | Category colour `category_hex(name)`: "Other" and "Uncategorized" ⇒ `#8e8e93`. Else `h = 0i64; for each UTF-8 byte b: h = b + (h << 5) − h` with **wrapping** i64 arithmetic; hue = (|h| mod 360) / 360 (use `unsigned_abs`); HSB(hue, s = 0.7, b = 0.9) → RGB by the 6-sector formula in `WebChart.swift`; each channel `trunc(c × 255)` formatted `#rrggbb` lowercase. Goldens: Groceries `#92e544`, Dining `#4744e5`, Housing `#6744e5`, Utilities `#44e58a`, Shopping `#e54444`, Income `#e59d44`. | `Color.categoryHex` |

## 11. CSV import (`pf-core::csv_import`)

| ID | Rule | Source |
|---|---|---|
| R-CSV-1 | Parsing: split into lines, drop blank lines; first line = headers; split each line on commas outside double quotes; quotes toggle and are removed; each field trimmed. (Keep this exact behaviour, including no support for escaped `""` — the `csv` crate is **not** used, so we match Swift byte for byte.) | `parseCSV`, `splitCSVLine` |
| R-CSV-2 | Column auto-detect on lowercased, trimmed headers (first match wins per field): Date: `date` or contains `transaction date`; Payee: `merchant`/`payee`/`description`/`name`; Amount: `amount`; Category: `category`; Memo: `memo`/`notes`/`note`; Account: `account`/`account name`; Tags: `tags`/`tag`/`labels`; Original Statement: `original statement` or contains `original`. The checks are an if/else chain per header in that order. | `detectColumns` |
| R-CSV-3 | Manual mapping: fields Date, Payee, Amount, Category, Memo, Account, Tags, (Skip). Multiple columns may map to Payee or Memo; their non-empty trimmed values are joined with `" | "` in column order. | `CSVColumnMapping` |
| R-CSV-4 | Mappings lookup: `(type, source.trim().lowercase(), account)` must match exactly (account `None` = global). | `lookupMapping` |
| R-CSV-5 | Unmapped collection (pre-import screen): accounts (Monarch only) whose value is not an existing account name (ci) and has no mapping; categories not matching an existing full name (ci) and unmapped; payees (Original Statement column for Monarch, else payee column) unmapped. Each list sorted, deduped. | `collectUnmappedValues` |
| R-CSV-6 | Date parsing tries `MM/dd/yyyy`, then `yyyy-MM-dd`, then ISO-8601; rows with unparseable date or amount are silently skipped (not counted). | `importTransactions` |
| R-CSV-7 | Amount: strip `$` and `,`, trim, parse Decimal; negate if `negate_amounts`. | same |
| R-CSV-8 | Monarch mode (= an Account column is mapped): account resolved per row via account mapping (global) or raw name, matched case-insensitively; not found ⇒ skippedUnmapped. Bank mode: target account required. | same |
| R-CSV-9 | Unless `allow_unmapped_payees`, a row whose payee has no payee mapping is skipped (skippedUnmapped). Monarch uses the Original Statement value (or payee column) for this check. | same |
| R-CSV-10 | Payee: Monarch ⇒ mapping of Original Statement if any, else the joined Merchant payee; Bank ⇒ mapping of joined payee, else raw. | same |
| R-CSV-11 | **Duplicate** = existing transaction (Swift checked all, including deleted — (fix): live only) in same account, same day, same amount, payee equal case-insensitively ⇒ skippedDuplicates. Also dedupe against rows already imported in the same batch? Swift did **not** (it checked the pre-fetched list only); match Swift. | same |
| R-CSV-12 | Category: mapped (account-scoped for bank, global for Monarch) or raw; matched case-insensitively against full name or name. | same |
| R-CSV-13 | Tags column: split on `,`, trim; match existing contexts ci or auto-create new ones. | same |
| R-CSV-14 | Imported transactions are status `Cleared`, reality. Whole import = one undo step "Import CSV" ((fix): Swift tried to clear the undo stack, but that call was a no-op because SwiftData's undo manager is nil). Result = `{imported, skipped_duplicates, skipped_unmapped}`. | same |
| R-CSV-15 | Column assignments chosen in the UI are saved as `"column"` ImportMappings for the account (source = header, target = field); the next import for that account starts from them. `negate_amounts` is saved too. Read `CSVImportFlowView.savedColumnAssignments` / `performImport` for exact behaviour. | `CSVImportFlowView` |
| R-CSV-16 | Translation mapping step: for each unmapped value the user picks an existing target or creates a new one (category with optional parent, account with type/external). Saved as ImportMappings. Port `TranslationMappingView.buildMappings` / `createNewCategoriesAndImport`. | `TranslationMappingView` |

## 12. Markdown backup (`pf-core::markdown`) — see `docs/markdown-format.md`

| ID | Rule | Source |
|---|---|---|
| R-MD-1 | Export produces the exact Swift format (section order, headers, column order, escaping). Golden-file tested. | `MarkdownDataService.export` |
| R-MD-2 | Import accepts every format variant the Swift importer accepts (old/new BudgetItems, old/new ImportMappings, Categories with/without Match Patterns). | `import` |
| R-MD-3 | Round trip: `import(export(db)) == db` modulo ids. Property-tested. | — |
| R-MD-4 | `|` in values is escaped as `｜` (U+FF5C) on export and restored on import. | `esc`/`unesc` |
| R-MD-5 | Importing a backup from Settings asks for confirmation, wipes all data (hard delete), then imports, and **clears the undo stack** (not undoable, same as Swift). Progress text is shown while importing. | `SettingsView.importMarkdown` |
| R-MD-6 | Name lookups during import (category, account, context, scenario) are exact-string, first created wins; duplicate names are skipped for Context/Category/Scenario/Account. | `cached*` |
| R-MD-7 | Cmd-S backup writes `PFBackup_yyyy-MM-dd_HH-mm-ss.md` into the backup folder; if none set, prompt for one first; toast `Backup saved to: {path}` for 4 s; errors shown in an alert. | `BackupManager.performBackup` |

## 13. HTTP API (`pf-api`) — must be byte-compatible with Swift clients

| ID | Rule | Source |
|---|---|---|
| R-API-1 | Listen on `127.0.0.1:{apiPort}` (default 8989) only. If the port is taken, log a warning and continue without the API. | `APIServer.start` |
| R-API-2 | Routes: `GET /api/db-path` (text), `GET /api/scenarios`, `GET /api/accounts/balances`, `GET /api/accounts/{name}/balance`, `GET /api/accounts/{name}/transactions?n=10`, `POST /api/accounts/{name}/import` (CSV body), `GET /api/export/markdown`. Unknown ⇒ 404 `Not Found: {METHOD} {path}`. | `route` |
| R-API-3 | `format` query param: `json` (default) \| `text` \| `markdown`/`md`. Content-Type `application/json` for json else `text/plain`, plus `; charset=utf-8`. Headers include `Connection: close`, `Access-Control-Allow-Origin: *`. | `route`, `sendResponse` |
| R-API-4 | `scenario` query param: missing, empty or `reality` (ci) ⇒ Reality; unknown name ⇒ error `Scenario '{name}' not found.` (json `{"error": …}` / text `Error: …`), status 200 (Swift quirk — keep). | `resolveScenario` |
| R-API-5 | Account names in paths are percent-decoded and matched case-insensitively; not found ⇒ 404 `Account not found: {name}`. Query `+` ⇒ space. | `extractAccountName`, `HTTPRequest.parse` |
| R-API-6 | Balances JSON: `{"accounts":[{"balance","external","institution","name","type"}…],"total"}` pretty-printed with sorted keys; balance/total as JSON numbers; total excludes external. Text & markdown formats exactly as Swift (currency formatted `$1,234.56`, negative `-$1,234.56`). | `formatBalances*` |
| R-API-7 | Transactions: visible per R-SCN-1, sorted date desc, first n; running balance computed over all visible rows oldest→newest. JSON keys sorted: `amount, category?, contexts?, date (yyyy-MM-dd), memo? (if non-empty), payee, runningBalance, scenario?, status`; wrapper `{account, count, transactions}`. Text/markdown exactly as Swift. | `handleTransactions`, `formatTransactions*` |
| R-API-8 | Import endpoint requires saved `"column"` mappings for the account, else error `No saved column mappings for account '{name}'. Please import via the UI first to set up column mappings.`; runs R-CSV with `allow_unmapped_payees = true`; status 400 if the response contains "error" or "Error:". | `handleImportCSV` |
| R-API-9 | API mutations go through the engine (undoable in the UI, persisted) and trigger a UI repaint. | — |
| R-API-10 | Requests up to 10 MB; malformed request ⇒ 400 `Bad Request`. | `handleConnection` |

## 14. Statement reconciliation (`pf-core::reconcile`)

New PFRust behaviour; there is no Swift source.

| ID | Rule | Source |
|---|---|---|
| R-REC-1 | **Statement JSON**: an object with `period_start`, `period_end` (`YYYY-MM-DD`), `opening_balance`, `closing_balance` (decimal strings; `$` and `,` stripped as R-CSV-7) and `lines` (array, may be empty); optional `institution`, `account_hint`, `total_credits`, `total_debits`. Each line has `date` and `amount` (required), optional `post_date`, `description`, `section`, `reference`. Unknown keys are ignored. Errors (exact): `Statement is not valid JSON: {serde message}`, `Missing field: {name}`, `Line {n}: missing field: {name}` (n is 1-based), `Invalid date in {field}: {value}`, `Invalid amount in {field}: {value}` (`{field}` is `line {n} amount` for lines), `Statement period ends before it starts.` | — (new) |
| R-REC-2 | **Statement CSV**: parsed with R-CSV-1. Headers matched case-insensitively after trimming: `date` (required), `description` (or `payee`, `merchant`, `name`), `amount` (required), optional `post date`, `section`, `reference`. Dates per R-CSV-6, amounts per R-CSV-7. A row whose date or amount does not parse is an error `Line {n}: invalid date or amount.` (not skipped: a statement must be complete). Balances and the period come from the caller. Missing required column: `The CSV needs Date and Amount columns.` | — (new) |
| R-REC-3 | **Self-check**: with S = Σ line amounts, the statement adds up *asset style* if opening + S = closing, and *owed style* if opening − S = closing. If neither holds, the error is `Statement does not add up: opening {o}, lines {S}, closing {c}. Off by {d}.` (amounts with `format_currency`; d = the smaller of \|opening + S − closing\| and \|opening − S − closing\|). If negating the lines of exactly one non-empty `section` would make either style add up, append ` Flipping the signs under '{section}' would make it add up.` (first such section in line order). | — (new) |
| R-REC-4 | **Totals check** (only when both `total_credits` and `total_debits` are present): the pair {Σ positive line amounts, Σ \|negative line amounts\|} must equal the pair {total_credits, total_debits} in either order. Otherwise: `Statement totals do not match: credits {a} and debits {b} listed, lines give {x} and {y}.` | — (new) |
| R-REC-5 | **Two readings**: when the statement adds up in style X with the lines as printed, it also adds up in the other style with every line negated. `self_check` returns both: (AsPrinted, X) first, then (Flipped, other). If S = 0 both styles hold for both readings; then return (AsPrinted, style by account type: Credit or Loan ⇒ Owed, else Asset) and (Flipped, the same style). | — (new) |
| R-REC-6 | **PFRust sign of balances**: asset style balances are used as printed; owed style balances are negated (owed 1,764.30 ⇒ −1,764.30), because PFRust stores a card's debt as a negative balance. | — (new) |
| R-REC-7 | **Choosing the reading**: for each of the two readings, count statement lines that have a candidate (R-REC-8) with exactly the same amount within 10 days. More matches wins. On a tie: `preferred` if given; else the reading whose style fits the account type (Credit or Loan ⇒ Owed, else Asset); else AsPrinted. The report states the chosen reading, its style, and both counts. | — (new) |
| R-REC-8 | **Candidates**: live transactions of the account in Reality (`scenario_id` None), dated from period start − 7 days to period end + 7 days inclusive, whose status is not Reconciled. Reconciled ones in the same window are kept apart for R-REC-12. | — (new) |
| R-REC-9 | **Pass 1 and 2 (exact amount)**: a line and a candidate may pair when their amounts are equal and their day distance is at most 3 (pass 1, confidence High) or at most 10 (pass 2, Medium, on what pass 1 left). Day distance = the smaller of \|line date − tx date\| and \|post date − tx date\|. Within a pass, list every allowed (line, candidate) pair, sort by day distance, then payee score descending, then line index, then transaction date, then transaction id; take each pair in that order if both sides are still free. | — (new) |
| R-REC-10 | **Pass 3 (amount differs)**: on what is left, a line and a candidate may pair when both amounts have the same sign, the day distance is at most 3, the payee score is at least 0.5, and \|line amount − tx amount\| ≤ 25% of \|line amount\|. Same sort and greedy taking as R-REC-9, with \|amount difference\| ascending inserted after day distance. | — (new) |
| R-REC-11 | **Payee score**: `normalize_description` upper-cases, replaces every character that is not a letter or digit with a space, splits on spaces, drops tokens that contain a digit, drops the stop words `POS DEBIT CREDIT ACH SQ TST PURCHASE PAYMENT CARD CHECKCARD VISA MC ONLINE RECURRING WWW COM INC LLC CO THE`, and joins the rest with single spaces. Score = shared tokens ÷ the smaller token count (0 when either is empty). An alias (normalized description, payee) whose description equals the line's normalized description and whose payee equals the transaction's payee case-insensitively gives 1.0. Aliases are the account's `statementPayee` mappings plus every `payee` mapping (source normalized, target as is). | — (new) |
| R-REC-12 | **Sections**: Matched (pass 1 and 2 pairs), AmountDiffers (pass 3 pairs), AlreadyReconciled (unpaired lines with an exact-amount Reconciled transaction within 3 days; the nearest one, ties by id), StatementOnly (other unpaired lines), PfOnlyInPeriod (unpaired candidates dated inside the period), PfOnlyOutside (unpaired candidates outside it). Each section is sorted by date, then line index or transaction id. | — (new) |
| R-REC-13 | **Balance checks**: reconciled balance = Σ amounts of the account's live Reality Reconciled transactions. Opening difference = statement opening (PFRust sign) − reconciled balance; `first_time` is true when the account has no Reconciled transaction. Difference for a choice = statement closing (PFRust sign) − (statement opening (PFRust sign) + Σ accepted pairs' amounts (the PFRust amount, or the statement amount where chosen) + Σ added lines' amounts + adjustment). The default choice accepts every Matched and AmountDiffers pair (PFRust amounts) and adds every StatementOnly line. | — (new) |
| R-REC-14 | **Apply** ("Reconcile Statement", one step): refused with `Switch to Reality to reconcile.` while a scenario is active. Each chosen pair's transaction must be live, in Reality and in the account, else `Transaction {id} cannot be reconciled.` and nothing changes. Its status becomes Reconciled; with "use the statement amount" its amount becomes the line's amount under the report's reading. Each added line becomes a new Reality transaction in the account: date = line date, amount = line amount under the reading, payee = the alias target for the line's normalized description if one exists, else the description trimmed (or `Statement line` when empty), memo = the description when the payee differs from it, category = R-CAT-1 on the payee, status Reconciled. An adjustment of A ≠ 0 adds a Reconciled transaction dated period end, payee `Reconciliation adjustment`, memo `Statement {start} to {end}` (`YYYY-MM-DD`), amount A, no category. With `mark_before`, every live Reality transaction of the account dated before period start and not Reconciled becomes Reconciled. | — (new) |
| R-REC-15 | **Aliases**: for each chosen pair whose payee score was below 1.0 and whose transaction payee is not empty, save an `ImportMapping` (type `statementPayee`, source = the line's normalized description, target = the transaction's payee, account = the account), in the same step. If a `statementPayee` mapping with that source already exists for the account, its target is updated instead. Empty normalized descriptions are not saved. | — (new) |
| R-REC-16 | **Remembered reading**: the step sets setting `reconcileReading.{account id}` to `asPrinted` or `flipped` (the report's reading). `preferred_reading` reads it; any other value is ignored. | — (new) |
| R-REC-17 | Changing the amount or date of a Reconciled transaction, or deleting one, asks first (`This transaction is Reconciled. Change it anyway?`). Other fields and status changes do not ask. | — (new) |
