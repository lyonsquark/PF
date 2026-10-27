# T10a: Commands: accounts, transactions, starting balance, status rules

**Phase:** 2 · **Deps:** T09, T04 · **Crate(s):** `pf-engine` · **Size:** large (about 450 lines of code plus 500 lines of tests)

## Goal

Implement every user action on accounts and transactions as an engine command that
enforces the Swift app's rules (locks, status rules, signs, transfers, starting balance,
auto-categorize) and is exactly one undo step. The UI tasks T13–T16 call only these functions.

## Read first

- `tasks/T09-engine-core.md` (Engine, Writer, World, world fixture facts)
- `docs/business-rules.md` §3 (R-TXN-1…15), §6 (R-ACC-1…4), R-CAT-1, R-CAT-6
- `docs/testing.md`: L3 rows BEH-ACC-*, BEH-TXN-*
- Swift:
  - `Views/TransactionListView.swift`: `isTransactionLocked`, `bindUndo*`, payee `onChange` auto-categorize, `addToBudget`, `duplicateTransactions`, `deleteTransactions`, `changeStatus`, `attachFile`, `openAttachmentSafely`
  - `Views/AddTransactionView.swift`: `insertTransaction`, `saveAndReset`, `isRecDisabled`, `addPayeeToMatchPatterns`, `autoCategorize`
  - `Views/ContentView.swift`: `EditAccountView` (`transactionSum`, `earliestTransactionDate`, `startingBalanceTx`, `saveStartingBalance`)
  - `Views/AddAccountView.swift`, `Views/DashboardView.swift` (`deleteAccount`), `Views/OrphanedTransactionsView.swift`
- pf-core from T04: the auto-categorize function (R-CAT-1) and substring-only variant (R-CAT-6)

## Deliverables

`crates/pf-engine/src/commands/accounts.rs`, `crates/pf-engine/src/commands/transactions.rs`,
World wrappers in `crates/pf-engine/src/testing.rs`, and tests in
`crates/pf-engine/tests/behaviour_accounts.rs` and `crates/pf-engine/tests/behaviour_transactions.rs`.

### accounts.rs

```rust
pub struct AccountInput {           // form values from Add/Edit Account
    pub name: String, pub account_type: AccountType, pub institution: String,
    pub currency: String, pub is_external: bool,
}

/// "Add Account". name trimmed; empty ⇒ Validation("Account name is required").
/// currency empty ⇒ "USD".
pub fn create_account(e: &mut Engine, input: AccountInput) -> Result<AccountId>;
/// "Edit Account". Same validation. NotFound if missing.
pub fn update_account(e: &mut Engine, id: AccountId, input: AccountInput) -> Result<()>;
/// "Delete Account". R-ACC-1: soft-delete account + its live transactions + its live obligations.
pub fn delete_account(e: &mut Engine, id: AccountId) -> Result<()>;

/// Read-only data for the Edit Account "Starting Balance" section.
pub struct StartingBalanceInfo {
    pub existing: Option<TransactionId>,  // live reality tx of this account with payee == "Starting Balance" (first by id)
    pub amount: Decimal,                  // existing amount or 0
    pub earliest_other: Option<Date>,     // min date of live REALITY tx of this account with payee != "Starting Balance"
    pub balance_date: Date,               // earliest_other − 1 day, or today − 1 day if None
    pub other_sum: Decimal,               // sum of those "other" transactions (same filter as earliest_other)
}
pub fn starting_balance_info(db: &Db, id: AccountId, today: Date) -> StartingBalanceInfo;
/// R-ACC-3: starting = current − other_sum.
pub fn calculate_starting_balance(current: Decimal, other_sum: Decimal) -> Decimal;
/// "Set Starting Balance". R-ACC-2: if existing ⇒ update amount + date (= balance_date);
/// else if amount != 0 ⇒ insert { payee "Starting Balance", memo "Initial account balance",
/// status Cleared, reality, date = balance_date }. If no other tx and amount == 0 ⇒ Ok, no step.
pub fn set_starting_balance(e: &mut Engine, id: AccountId, amount: Decimal) -> Result<()>;
```

Note on R-ACC-2: Swift's filters for "earliest" and "sum" exclude the Starting Balance row
itself **and** scenario rows (`scenario == nil`). Follow Swift (this is more precise than the rule text).

### transactions.rs

```rust
pub enum TxKind { Deposit, Withdrawal, Transfer { to: AccountId } }

pub struct NewTransaction {          // Add Transaction dialog values
    pub kind: TxKind,
    pub account: AccountId,          // source account for transfers
    pub date: Date,
    pub amount: Decimal,             // sign ignored; |amount| used (R-TXN-8)
    pub payee: String,               // ignored for Transfer (payees are generated)
    pub memo: String,                // "" ⇒ None
    pub status: TransactionStatus,
    pub category: Option<CategoryId>,
    pub contexts: Vec<ContextId>,
}
/// "Add Transaction" (or "Add Transfer"). R-TXN-8: scenario = e.state.active_scenario.
/// R-TXN-9: if the new tx is reality AND status Cleared/Reconciled AND date > today ⇒
///   Validation("Cleared transactions cannot be dated in the future").
/// Transfer with to == account ⇒ Validation("Choose a different destination account").
/// Returns the ids created (1, or 2 for a transfer: [from, to]).
pub fn add_transaction(e: &mut Engine, t: NewTransaction) -> Result<Vec<TransactionId>>;

/// R-TXN-1. Pure check used by the UI to disable widgets AND by every edit command.
pub fn is_locked(db: &Db, state: &AppState, id: TransactionId) -> bool;

pub enum TxField {
    Payee(String), Memo(Option<String>), Amount(Decimal), Date(Date),
    Category(Option<CategoryId>), Contexts(Vec<ContextId>), Attachment(Option<String>),
}
/// One inline edit (R-TXN-12). Undo names: "Change Payee", "Change Memo", "Change Amount",
/// "Change Date", "Change Category", "Change Contexts", "Change Attachment".
/// Locked ⇒ Err(Locked). Unchanged value ⇒ Ok, no step (Engine guarantees it).
/// Memo Some("") is stored as None. Date: R-TXN-9 applies (reality + cleared/reconciled + > today ⇒
/// Validation, same message as add).
/// After a successful Payee edit: R-TXN-13. If the tx has no category and is not locked, run
/// pf-core auto-categorize (R-CAT-1) over live categories; on a match apply a SECOND step
/// "Auto-categorize" setting the category. Returns whether auto-categorize fired.
pub fn edit_transaction(e: &mut Engine, id: TransactionId, field: TxField) -> Result<bool>;

/// R-TXN-2..5. Messages (exact):
///   "Future status is only for scenario transactions"
///   "Cannot clear a future-dated scenario transaction"
/// Locked ⇒ Err(Locked). Same status ⇒ Ok, no step. Undo name "Change Status".
/// Promotion (R-TXN-4): scenario tx → Pending/Cleared/Reconciled sets scenario_id = None in the same step.
pub fn change_status(e: &mut Engine, id: TransactionId, status: TransactionStatus) -> Result<()>;

/// R-TXN-6. "Duplicate Transaction" (1) / "Duplicate Transactions" (>1). date = e.today().
/// Locked sources are allowed (Swift does not check). Returns the new ids in input order.
pub fn duplicate_transactions(e: &mut Engine, ids: &[TransactionId]) -> Result<Vec<TransactionId>>;
/// R-TXN-7. "Delete Transaction" / "Delete Transactions". Empty slice ⇒ Ok, no step.
/// Locked rows ⇒ Err(Locked) and nothing deleted (fix: Swift let the context menu delete them).
pub fn delete_transactions(e: &mut Engine, ids: &[TransactionId]) -> Result<()>;
/// R-TXN-15. Live txs with account_id None, date desc then id desc.
pub fn orphaned_transactions(db: &Db) -> Vec<TransactionId>;
/// R-TXN-15 (fix): soft delete all orphans, one step "Delete Orphaned Transactions". Returns the count.
pub fn delete_orphaned(e: &mut Engine) -> Result<usize>;
/// R-TXN-14. Extension allow-list check (case-insensitive): pdf png jpg jpeg gif heic tiff webp
/// txt md rtf csv xls xlsx numbers ppt pptx key. Pure; the UI opens the file with `open`.
pub fn is_safe_attachment(path: &str) -> bool;

/// R-CAT-5 "Rec" button. Validation errors: none; returns false when disabled.
pub fn can_add_payee_pattern(db: &Db, category: CategoryId, payee: &str) -> bool;
/// Appends trimmed payee to category.match_patterns. Undo name "Add Match Pattern".
/// Returns the toast text: `Added "{payee}" to {fullName} auto-match`.
pub fn add_payee_pattern(e: &mut Engine, category: CategoryId, payee: &str) -> Result<String>;
```

R-TXN-11 (Add to Budget) lives in T10c (`budget::add_to_budget`).

World wrappers to add to `testing.rs` (they all go through `add_transaction`, status Cleared, no category):
`add_deposit(acc, "YYYY-MM-DD", "amount", "payee") -> TransactionId`, `add_withdrawal(...)`,
`add_transfer(from, to, "YYYY-MM-DD", "amount") -> (TransactionId, TransactionId)`.

## Steps

1. Write the two behaviour test files (tables below). Use `World::with_fixture("world")` and the "world fixture facts".
2. Write the rule-level unit tests inside the modules (`#[cfg(test)]`) for the pure functions.
3. Implement accounts.rs, then transactions.rs. Every command makes exactly **one** `e.apply` call,
   except `edit_transaction`'s optional second "Auto-categorize" step.
4. Validate (locks, messages) **before** calling `apply`, or return `Err` from inside the closure (rollback is automatic).

## Tests (required)

Rule-level (unit tests in the modules, or in the behaviour files if they need World):

| Test name | Covers | What it checks |
|---|---|---|
| `r_txn_1_reality_mode_nothing_locked` | R-TXN-1 | no active scenario ⇒ `is_locked` false for reality and scenario rows |
| `r_txn_1_scenario_mode_locks_reality_rows` | R-TXN-1 | Plan A active ⇒ reality row locked, Plan A row not |
| `r_txn_1_deleted_row_is_locked` | R-TXN-1 | |
| `r_txn_2_future_on_reality_rejected` | R-TXN-2 | exact message; tx unchanged; no undo step |
| `r_txn_3_clear_future_dated_scenario_rejected` | R-TXN-3 | Plan A Landlord 2026-04-01 → Cleared ⇒ exact message |
| `r_txn_4_pending_promotes_scenario_row` | R-TXN-4 | scenario_id None + status Pending; one undo restores both |
| `r_txn_5_same_status_noop` | R-TXN-5 | `can_undo()` unchanged |
| `r_txn_6_duplicate_copies_fields_dated_today` | R-TXN-6 | all fields equal except id and date == TEST_TODAY |
| `r_txn_7_delete_names` | R-TXN-7 | 1 ⇒ "Delete Transaction", 2 ⇒ "Delete Transactions" |
| `r_txn_8_signs_forced` | R-TXN-8 | deposit of −50 ⇒ +50; withdrawal of 50 ⇒ −50 |
| `r_txn_8_added_tx_gets_active_scenario` | R-TXN-8 | |
| `r_txn_8_empty_memo_is_none` | R-TXN-8 | |
| `r_txn_9_cleared_reality_cannot_be_future` | R-TXN-9 | add and edit both rejected; Pending future date allowed |
| `r_txn_12_unchanged_edit_no_step` | R-TXN-12 | edit payee to same text ⇒ no step |
| `r_txn_12_undo_names_per_field` | R-TXN-12 | each TxField variant ⇒ its name |
| `r_txn_13_payee_edit_autocategorizes_uncategorized` | R-TXN-13 | "Cafe Luna" → edit payee "Cafe Luna 2" ⇒ category Dining; returns true |
| `r_txn_13_no_autocategorize_when_category_set` | R-TXN-13 | |
| `r_txn_14_attachment_allow_list` | R-TXN-14 | `a.PDF` true, `x.exe` false, `noext` false |
| `r_txn_15_orphans_listed_and_deleted` | R-TXN-15 | build with TestDb, a tx with no account ⇒ listed; delete ⇒ count 1, undoable |
| `r_acc_1_delete_account_cascades` | R-ACC-1 | Checking + its 9 tx + Property Tax obligation soft-deleted |
| `r_acc_2_starting_balance_date_is_day_before_earliest_other` | R-ACC-2 | Checking ⇒ balance_date 2026-01-01 (earliest other is 2026-01-02); scenario rows ignored |
| `r_acc_2_no_transactions_zero_amount_noop` | R-ACC-2 | Mortgage, amount 0 ⇒ no step |
| `r_acc_2_no_transactions_uses_today_minus_one` | R-ACC-2 | Mortgage, amount −250000 ⇒ dated 2026-03-14 |
| `r_acc_3_calculate` | R-ACC-3 | current 3000, other_sum 51.95 ⇒ 2948.05 |
| `r_acc_4_validation_empty_name` | – | "Account name is required" |
| `r_cat_5_rec_button_rules` | R-CAT-5 | disabled when already a pattern (case-insensitive); toast text exact |

Behaviour (Given/When/Then), each with `/// BEH-… — Covers: …`:

| Test name | ID | Given / When / Then |
|---|---|---|
| `beh_acc_01_create_account_sorted_and_persisted` | BEH-ACC-01 | **Given** world · **When** create "Amex" (credit) · **Then** live accounts by name are Amex, Checking, External 401k, Mortgage, Savings, Visa; after `reload()` still there |
| `beh_acc_02_delete_account_undo_redo` | BEH-ACC-02 | **Given** world · **When** delete Checking · **Then** account, its 9 tx and Property Tax hidden · **When** undo · **Then** all live, balance 2051.95 · **When** redo · **Then** hidden again |
| `beh_acc_03_starting_balance_set_then_updated` | BEH-ACC-03 | **Given** world, Visa (no starting row) · **When** set 100 · **Then** row dated 2026-01-24 (earliest other 2026-01-25) · **When** set 150 · **Then** the same row id now has 150 (still 1 starting row) |
| `beh_acc_04_calculate_matches_entered_current` | BEH-ACC-04 | **Given** Checking · **When** info = starting_balance_info; amount = calculate(5000, info.other_sum); set_starting_balance · **Then** reality balance == 5000 |
| `beh_txn_01_signs_forced` | BEH-TXN-01 | deposit "-25" ⇒ +25, withdrawal "25" ⇒ −25; balances move accordingly |
| `beh_txn_02_transfer_two_rows_one_undo` | BEH-TXN-02 | **When** transfer 200 Checking→Savings · **Then** payees "Transfer to Savings"/"Transfer from Checking", Checking −200, Savings +200 · **When** undo once · **Then** both gone |
| `beh_txn_03_payee_edit_autocategorize_two_undo_steps` | BEH-TXN-03 | **Given** "Cafe Luna" (uncategorized) · **When** edit payee to "Cafe Luna Downtown" · **Then** Dining · **When** undo · **Then** category None, payee still new · **When** undo · **Then** payee "Cafe Luna" |
| `beh_txn_04_locked_edit_refused` | BEH-TXN-04 | **Given** Plan A active · **When** edit amount of "Starbucks" · **Then** Err(Locked), Db unchanged, no step |
| `beh_txn_05_status_rules` | BEH-TXN-05 | future on reality refused; clearing 2026-04-01 Plan A row refused; Pending on it promotes to reality; undo restores scenario Plan A + status Future |
| `beh_txn_06_duplicate_three_one_undo` | BEH-TXN-06 | 3 ids ⇒ 3 new rows dated 2026-03-15; `live_tx_count` +3; undo ⇒ back |
| `beh_txn_07_delete_multiple_one_step` | BEH-TXN-07 | 2 ids ⇒ `undo_name() == Some("Delete Transactions")` |
| `beh_txn_08_running_balance_independent_of_window` | BEH-TXN-08, R-BAL-7 | running balance of "Whole Foods" via T03's function with PastMonth == with All == 2551.95 |

## Pitfalls

- `is_locked` uses `e.state.active_scenario`, **not** the transaction's own scenario alone.
- Transfers: both rows get the same category and contexts (Swift does this; keep it).
- `edit_transaction(Date)` on a scenario row may be any date; only reality + cleared/reconciled is restricted.
- Use `e.today()`, never the real clock.
- `delete_account` must collect child ids **before** the apply closure borrows the writer.
- Remember to trim the payee for patterns and compare case-insensitively (`to_lowercase()`).

## Done when

```bash
cargo test -p pf-engine
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules      # R-TXN-*, R-ACC-*, R-CAT-5, BEH-ACC-*, BEH-TXN-* present
```

## Out of scope

- Add to Budget (T10c), CSV import (T10c), category/context CRUD (T10b).
- Any UI. Opening attachments (T15c does `std::process::Command::new("open")`).
