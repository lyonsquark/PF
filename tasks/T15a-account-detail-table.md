# T15a: Account detail screen + virtualised read-only transaction table

**Phase:** 3 · **Deps:** T13 · **Crate(s):** `pf-ui` · **Size:** medium-large (≈450 lines + tests)

## Goal

Implement the account detail screen: header (editable name, institution, balance and cleared balance),
the scenario label and time-period picker, an action row, and a **virtualised** transaction table showing
all nine Swift columns **read-only**, with running balance and row selection. T15b makes cells editable;
T15c adds search, the context menu and attachments. The table must stay smooth with 100,000 rows.

> If a command or pf-core function name differs from what is written here, use the actual one from
> `pf-core` / `pf-engine`; do not add new engine behaviour here.

## Read first

- `tasks/T13-ui-shell.md` (UiCtx, Cached, Lookups, widgets, colors), `docs/egui-cheatsheet.md` → "Virtualised table"
- Rules: R-SCN-1, R-BAL-1, R-BAL-2, R-BAL-7, R-FLT-1, R-FLT-2, R-FLT-5, R-TXN-1, R-CTX-4
- Swift: `Views/AccountDetailView.swift`, `Views/AccountBalanceView.swift`, `Views/TransactionListView.swift`
  (`filteredTransactions`, `runningBalances`, the `Table` columns, `statusColor`), `Models/TimePeriod.swift`
- pf-core T03 APIs (visibility, balances, running balance, window filter), T10a `transactions::is_locked`, T09 `Engine::set_time_period`

## Deliverables

Convert the T13 placeholder `screens/account.rs` into a directory module:

```
crates/pf-ui/src/screens/account/
├── mod.rs      State, show(): header, period row, action row, calls table::show
├── rows.rs     TxRow, RowsKey, build_rows() (pure, unit-tested)
└── table.rs    TableBuilder drawing + selection
```

```rust
// rows.rs
#[derive(Debug, Clone, PartialEq)]
pub struct TxRow {
    pub id: TransactionId,
    pub date: Date,
    pub payee: String,
    pub amount: Decimal,
    pub category_id: Option<CategoryId>,
    pub category_label: String,            // "None" if no/deleted category; else lookup label ("{emoji} Parent/Child")
    pub memo: String,                      // "" if None
    pub attachment: Option<String>,
    pub contexts: Vec<(ContextId, String, Option<String>)>,  // (id, name, color_hex), transaction order
    pub running: Decimal,                  // R-BAL-7
    pub status: TransactionStatus,
    pub is_scenario: bool,                 // scenario_id.is_some()
    pub locked: bool,                      // R-TXN-1 via commands::transactions::is_locked
}

/// Cache key. T15c adds the search spec; keep the struct extensible.
#[derive(Debug, Clone, PartialEq)]
pub struct RowsKey { pub account: AccountId, pub active: Option<ScenarioId>, pub period: TimePeriod, pub today: Date }

/// Pure. 1) visible txs of the account (R-SCN-1); 2) running balance over ALL of them, oldest→newest by (date, id) (R-BAL-7);
/// 3) window filter (R-FLT-2, start from R-FLT-1 with `today`); 4) sort date desc, id desc (R-FLT-5).
pub fn build_rows(db: &Db, state: &AppState, key: &RowsKey) -> Vec<TxRow>;

// mod.rs
#[derive(Default)]
pub struct State {
    pub(crate) rows: Cached<RowsKey, Vec<TxRow>>,
    pub(crate) header: Cached<(AccountId, Option<ScenarioId>), (Decimal, Decimal)>,   // (balance R-BAL-1, cleared R-BAL-2)
    pub selection: BTreeSet<TransactionId>,
    pub(crate) anchor: Option<TransactionId>,   // for shift-click ranges
    pub(crate) for_account: Option<AccountId>,  // clear selection when the account changes
    pub(crate) name_edit: Option<String>,       // header name buffer while focused
}
pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut egui::Ui, id: AccountId);
```

## Layout

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ Checking                    (large bold, editable)              $1,234.56    │  balance: size 22 bold, RED if < 0
│ Chase                       (weak, size 16)            Cleared: $1,100.00    │  shown only if cleared ≠ balance; RED if < 0 else weak
│ ──────────────────────────────────────────────────────────────────────────── │
│ Scenario: Plan A  (ORANGE bold, only if active)        Time Period [Past Month ▾] │
│ [＋ Add Transaction] [Import CSV] [🔍]      ← action row (buttons added by T16 / T22 / T15c)│
│ ┌──────────┬───────────────┬──────────┬──────────────┬────────┬───┬─────────────┬────────────┬───────────┐ │
│ │ Date     │ Payee         │ Amount   │ Category     │ Memo   │📎 │ Context     │ Running Bal│ Status    │ │
│ ├──────────┼───────────────┼──────────┼──────────────┼────────┼───┼─────────────┼────────────┼───────────┤ │
│ │2026-03-14│ Coffee Shop   │  -$4.50  │ ☕ Food/Cafe │        │   │ #trip       │ $1,234.56  │ Cleared   │ │
│ │2026-03-12│ Paycheck      │ $2,000.00│ Salary       │ March  │ 📎│ No Context  │ $1,239.06  │ Pending   │ │
│ └──────────┴───────────────┴──────────┴──────────────┴────────┴───┴─────────────┴────────────┴───────────┘ │
└──────────────────────────────────────────────────────────────────────────────┘
```

Header details:
- **Name**: `TextEdit::singleline` with large font (`.font(egui::TextStyle::Heading)`), frame NONE, buffer in `st.name_edit`
  while focused; on `lost_focus()` with a changed non-empty trimmed name ⇒ `commands::accounts::update_account(engine, id, AccountInput{ name, ..current })`.
  Errors ⇒ toast. Give it the accessible label "Account Name" (`widget_info` as in pitfalls).
- **Institution**: weak label below.
- **Balance**: `(balance, cleared)` from pf-core for this account under the active scenario.
- **Time Period**: `egui::ComboBox::from_label("Time Period")` with the six `TimePeriod` labels in `TimePeriod::ALL` order
  ("Past Month", "Past 60 Days", "Year to Date", "Past Year", "Archival", "All"). Selecting calls `cx.engine.set_time_period(p)`
  (persisted by the engine; error ⇒ toast). The selected value comes from `cx.engine.state.time_period`.
- **Action row**: an `ui.horizontal` that T16 (Add Transaction), T22 (Import CSV, Reality only) and T15c (search toggle) fill.
  In T15a it contains nothing; leave a `// T16/T22/T15c add buttons here` comment and a `fn action_row(cx, st, ui, id)`.

Table (`table.rs`), using `egui_extras::TableBuilder`:
- `.striped(true).resizable(true).sense(Sense::click()).auto_shrink([false, false])` (check the method names in
  `egui_extras-0.37.0/src/table.rs`), `max_scroll_height(f32::INFINITY)`; row height 24 (it fits a ComboBox in T15b).
- Columns (initial widths): Date 110, Payee `remainder().at_least(150)`, Amount 100, Category 160, Memo 140,
  📎 36, Context 170, Running Balance 110, Status 100. Header labels exactly: `Date`, `Payee`, `Amount`, `Category`,
  `Memo`, `📎`, `Context`, `Running Balance`, `Status`.
- Cells (read-only in this task):
  - Date `YYYY-MM-DD`.
  - Amount right-aligned; GREEN when ≥ 0, normal otherwise; locked ⇒ `.weak()` (and GREEN at 60 % alpha for positives), from `AmountEditField.foregroundColor`.
  - Category label (or weak "None").
  - 📎: "📎" if attachment else empty.
  - Context: chips `#name` (small text, rounded frame with fill `color.gamma_multiply(0.15)`, text `color`; colour from
    `widgets::color_from_hex`, default BLUE), or weak "No Context". Put chips in a horizontal `ScrollArea` with hidden bars.
  - Running Balance: right-aligned, weak.
  - Status: label `Cleared` (normal) / `Pending` (GREEN) / `Reconciled` (BROWN) / `Future` (RED).
  - Locked rows: render the text weak.
- **Selection**: click ⇒ select only that row; ⌘-click ⇒ toggle; ⇧-click ⇒ range from `anchor` in current row order.
  `row.set_selected(st.selection.contains(&id))`. Drop selected ids that are no longer in `rows` (after deletes/filtering).
- Empty state: if no rows, centred weak text "No transactions".

## Steps

1. Write `rows.rs` + unit tests first, using `pf_model::TestDb` and a fixed `today = date(2026, 3, 15)`.
2. `table.rs`: draw from `&[TxRow]`. Collect clicks into local variables and apply them to `st.selection` after the table
   closure (TableBuilder closures borrow `ui`).
3. `mod.rs`: header + period row + action row + table. Get rows via
   `st.rows.get(cx.engine.revision(), &key, || build_rows(cx.engine.db(), &cx.engine.state, &key))`. Clone the `Vec` only if you must; prefer
   passing `&[TxRow]` to the table (borrow `st.rows` and `st.selection` separately: split the `State` fields).
4. When `st.for_account != Some(id)`: clear selection, set `for_account`.
5. kittest tests in `crates/pf-ui/tests/ui_account.rs`.

## Tests (required)

Unit (`rows.rs`):

| Test name | Covers | What it checks |
|---|---|---|
| `r_scn_1_rows_reality_excludes_scenario_rows` | R-SCN-1 | no active scenario ⇒ scenario rows absent; active S ⇒ reality + S rows, not other scenarios; deleted rows never |
| `r_bal_7_running_balance_oldest_to_newest` | R-BAL-7 | 3 txs on 2 dates incl. same-date tie ⇒ running values cumulative by (date, id) |
| `r_bal_7_running_balance_stable_across_period` | R-BAL-7 | the newest row's running balance is identical for PastMonth and All |
| `r_flt_2_archival_shows_only_old_reality` | R-FLT-2 | Archival ⇒ only reality rows dated before today − 1 year |
| `r_flt_2_scenario_rows_pass_window` | R-FLT-2 | future-dated scenario row visible under PastMonth |
| `r_flt_5_sorted_date_desc_then_id_desc` | R-FLT-5 | order check |
| `r_txn_1_locked_flag_in_scenario_mode` | R-TXN-1 | active scenario ⇒ reality rows `locked`, scenario rows not; Reality ⇒ none locked |
| `rows_category_label_none_when_deleted_category` | R-CAT-7 | tx whose category is soft-deleted ⇒ label "None" |

kittest (`tests/ui_account.rs`):

| Test name | Covers | What it checks |
|---|---|---|
| `ui_account_header_shows_balance_and_cleared` | R-BAL-1, R-BAL-2 | with a pending tx, both the balance and "Cleared:" texts are present; without pending, "Cleared:" absent |
| `ui_account_time_period_changes_rows` | R-FLT-1 | an old tx's payee label absent under "Past Month", present after choosing "All" in "Time Period"; `engine.state.time_period` updated |
| `ui_account_scenario_label_when_active` | – | activate scenario ⇒ "Scenario: {name}" present |
| `ui_account_rename_commits_on_enter` | – | click "Account Name", type, press Enter ⇒ account renamed in engine; one undo step |
| `ui_account_selection_click_and_cmd_click` | – | click row A ⇒ {A}; ⌘-click B (`node.click_modifiers(Modifiers::COMMAND)`) ⇒ {A,B}; ⇧-click D ⇒ range; click C ⇒ {C} (assert via `State`, exposed through `app` test accessor `account_selection()`) |
| `ui_account_large_table_renders` | perf smoke | 20,000 txs render one frame without panic (no timing assertion) |

Add `pub fn account_selection(&self) -> Vec<TransactionId>` on `PfApp` (test accessor) for the selection test.

**Accessible labels:** "Account Name" (header field, via `widget_info`), "Time Period" (ComboBox::from_label), table header texts.
For row clicks in kittest, click the payee label text (unique test payees).

## Pitfalls

- `TableBuilder::body(|body| body.rows(h, n, |row| …))` runs only for visible rows, but `rows` must already be built: **never** call
  `build_rows` inside the row closure or every frame without the cache.
- Running balance uses *all visible* txs, not the windowed ones (R-BAL-7); compute before filtering.
- `TextEdit::frame(Frame::NONE)` takes a Frame (not bool).
- Accessible name for a field without a visible label (verified):
  `resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, ui.is_enabled(), "Account Name"));` then in tests
  `h.get_by_label("Account Name").click(); h.run(); …type_text(…)`. Use `click()` rather than `focus()`.
- Do not press ⌘Z in a test while a text field is focused (the field undoes its own text).
- Split borrows: `let State { rows, selection, .. } = st;`.

## Done when

```bash
cargo test -p pf-ui rows
cargo test -p pf-ui --test ui_account
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules
cargo run -p pf    # manual: open an account with many transactions, scroll quickly, change the period
```

## Out of scope

- Editing cells, status changes (T15b). Search, context menu, duplicate/delete, attachments, ⌘F (T15c).
- Add Transaction dialog (T16), CSV import (T22).
