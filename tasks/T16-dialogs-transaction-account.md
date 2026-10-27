# T16: Dialogs: Add Transaction, Add Account, Edit Account (+ starting balance)

**Phase:** 3 · **Deps:** T13 · **Crate(s):** `pf-ui` · **Size:** medium-large (≈500 lines + tests)

## Goal

Add the three form dialogs and their entry points: the "＋ Add Transaction" button and ⌘N on account screens,
the sidebar "+" (Add Account) button, and "Edit Account" in the sidebar account context menu. Each dialog is an
`egui::Modal` with a form-state struct, and saves through T10a commands.

> If a command or pf-core function name differs from what is written here, use the actual one from
> `pf-core` / `pf-engine`; do not add new engine behaviour here.

## Read first

- `tasks/T13-ui-shell.md` (Dialog enum, Outcome, show_active pattern, Lookups, widgets), `tasks/T15b-…md` (`amount_field`,
  `parse_amount_input`, `category_picker`, `color_from_hex`)
- Rules: R-TXN-8, R-TXN-9, R-TXN-10, R-CAT-5, R-CAT-6, R-ACC-2, R-ACC-3
- Swift: `Views/AddTransactionView.swift` (all), `Views/AddAccountView.swift`, `Views/ContentView.swift` → `EditAccountView`,
  `SidebarView.accountRow` context menu and `accountsHeader` "+"
- T10a: `add_transaction(NewTransaction)`, `create_account(AccountInput)`, `update_account`, `starting_balance_info`,
  `calculate_starting_balance`, `set_starting_balance`, `can_add_payee_pattern`, `add_payee_pattern`; pf-core R-CAT-6 helper (substring-only auto-categorize)

## Deliverables

```
crates/pf-ui/src/dialogs/add_transaction.rs   TxForm + show()
crates/pf-ui/src/dialogs/account.rs           AccountForm, EditAccountForm + show_add(), show_edit()
crates/pf-ui/src/dialogs/mod.rs               (modify) new Dialog variants + match arms
crates/pf-ui/src/sidebar.rs                   (modify) "+" button, "Edit Account" menu item
crates/pf-ui/src/screens/account/mod.rs       (modify) action-row button "＋ Add Transaction"
crates/pf-ui/src/app.rs                       (modify) ⌘N on Screen::Account
```

```rust
// dialogs/mod.rs: add
pub enum Dialog {
    // …existing…
    AddTransaction(Box<TxForm>),
    AddAccount(AccountForm),
    EditAccount(Box<EditAccountForm>),
}

// dialogs/add_transaction.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxType { Deposit, Withdrawal, Transfer }      // labels "Deposit" "Withdrawal" "Transfer"; default Withdrawal
#[derive(Debug, Clone)]
pub struct TxForm {
    pub tx_type: TxType,
    pub account: Option<AccountId>,          // "Account" / "From Account"
    pub to_account: Option<AccountId>,       // "To Account" (Transfer)
    pub payee: String,
    pub category: Option<CategoryId>,
    pub amount_text: String,                 // parsed with parse_amount_input
    pub date: Date,                          // default today
    pub memo: String,
    pub status: TransactionStatus,           // Cleared in Reality, Future in a scenario
    pub contexts: Vec<ContextId>,            // toggled chips, in click order
    pub scenario: Option<ScenarioId>,        // fixed when opened = engine.state.active_scenario
    pub error: Option<String>,
}
impl TxForm {
    pub fn new(account: Option<AccountId>, scenario: Option<ScenarioId>, today: Date) -> Self;
    /// Swift isSaveDisabled: amount missing/0; no account; Transfer ⇒ to missing or == from; else payee empty.
    /// Plus (Swift date picker range): Reality ⇒ date > today disables Save with error "Date cannot be in the future".
    pub fn save_disabled(&self, today: Date) -> bool;
    /// Builds the T10a input. Withdrawal/Deposit/Transfer → TxKind.
    pub fn to_new_transaction(&self) -> Option<NewTransaction>;
    /// R-TXN-10: keep account, date, type (and status, scenario); clear amount, payee, memo, category, contexts, to_account.
    pub fn reset_for_another(&mut self);
}
pub fn show(cx: &mut UiCtx, ui: &mut egui::Ui, form: &mut TxForm) -> Outcome;

// dialogs/account.rs
#[derive(Debug, Clone, Default)]
pub struct AccountForm { pub name: String, pub institution: String, pub account_type: AccountType, pub currency: String /* "USD" */, pub is_external: bool, pub error: Option<String> }
#[derive(Debug, Clone)]
pub struct EditAccountForm {
    pub id: AccountId,
    pub details: AccountForm,
    pub info: StartingBalanceInfo,           // from starting_balance_info() when opened
    pub starting_text: String,               // amount_edit_text(info.amount)
    pub current_text: String,                // amount_edit_text(info.amount + info.other_sum)  (R-ACC-3)
}
pub fn show_add(cx: &mut UiCtx, ui: &mut egui::Ui, form: &mut AccountForm) -> Outcome;
pub fn show_edit(cx: &mut UiCtx, ui: &mut egui::Ui, form: &mut EditAccountForm) -> Outcome;
```

## Layouts

### New Transaction (min 420 × 460)

```
New Transaction
Transaction Type   (Deposit | Withdrawal | Transfer)          ← selectable_value row
── Details ─────────────────────────────────────────────
Account      [Checking ▾]          (Transfer: From Account [..▾]  To Account [..▾], To excludes From)
Payee        [               ]     (hidden for Transfer)
Category     [Select Category ▾]  [Rec]
Amount       [               ]
Date         [2026-03-15]
Memo         [               ]
Status       (Cleared | Pending)   or weak "Status: Future" when in a scenario
Contexts (Tags)   (#trip) (#home) …   ← only if any contexts exist
Adding to Scenario: Plan A          ← ORANGE, only in a scenario
{error in RED, if any}
                              [Cancel] [Save & Another] [Save]
```
- ComboBox placeholders: "Select Account", "Select Category" (= None).
- Typing in Payee runs the R-CAT-6 helper (substring only) and sets `category` when it matches (eager, like Swift).
- **Rec** button: hover text "Add payee to this category's auto-match rules"; enabled iff `can_add_payee_pattern(db, cat, payee)`;
  click ⇒ `add_payee_pattern` ⇒ toast success with the returned text (R-CAT-5). It is a separate undo step.
- Context chips: selected = fill colour, white text; unselected = fill colour × 0.1, colour text, 1 px colour stroke. Click toggles.
- **Save**: `add_transaction(form.to_new_transaction())`. Ok ⇒ close. Err ⇒ `form.error = Some(e.to_string())`, stay open.
- **Save & Another**: same, then `reset_for_another()` and stay open; toast "Transaction saved".
- **Cancel** / Esc ⇒ close without saving.

### New Account (min 380 × 300)

```
New Account
── Account Details ──
Account Name   [            ]
Institution    [            ]
Type           [Checking ▾]      ← labels capitalised: Checking Savings Credit Loan Investment Cash
Currency       [USD         ]
[ ] External Account
                                [Cancel] [Save]   (Save disabled when name is empty)
```
Save ⇒ `create_account(AccountInput{…})`; Ok ⇒ close; Err ⇒ show in RED.

### Edit Account (min 650 × 600)

```
Edit Account
── Account Details ──   (same fields as New Account, pre-filled)
── Starting Balance ──
Earliest transaction:      Mar 1, 2026          ← or weak "No transactions yet"
Starting balance date:     Feb 28, 2026
Starting Balance:          [  1000      ]
A "Starting Balance" transaction exists and will be updated on save.   ← small weak, only if info.existing
──────────
Current Balance:           [  1234.56   ]  [Calculate]
Enter your actual current balance and click Calculate to determine the starting balance.   ← small weak
                                                     [Cancel] [Save]
```
- **Calculate** ⇒ `starting_text = amount_edit_text(calculate_starting_balance(parse(current_text), info.other_sum))` (R-ACC-3).
- **Save** ⇒ `update_account(id, details)` then `set_starting_balance(id, parse(starting_text))` (R-ACC-2). These are two undo steps;
  that is acceptable. If T10a provides a combined command, use it instead. Invalid amount text ⇒ RED error, stay open.
- Swift edits the account live (Cancel cannot revert); Rust edits a copy, so **Cancel discards**. This is an intended improvement.
- Dates displayed as `"%b %-d, %Y"` (jiff `strftime`).

### Entry points

- Sidebar "ACCOUNTS" header: a small "+" button, hover text "Add Account" ⇒ `Dialog::AddAccount(AccountForm { currency: "USD".into(), ..Default::default() })`.
- Sidebar account context menu: **"Edit Account"** (above a separator and "Delete") ⇒ `Dialog::EditAccount(...)` built from the account + `starting_balance_info(db, id, today)`.
- Account action row: button "＋ Add Transaction", hover "Add a new transaction" ⇒ `Dialog::AddTransaction(Box::new(TxForm::new(Some(id), active, today)))`.
- ⌘N on `Screen::Account(id)` (not while a dialog is open) ⇒ the same.

## Steps

1. Unit-test `TxForm::save_disabled`, `to_new_transaction`, `reset_for_another` first (pure).
2. Write the three `show` functions with `egui::Grid::new(...).num_columns(2)` for label/field rows; use `widgets::labeled_text`
   so fields get accessible labels.
3. Add variants and match arms; wire entry points and ⌘N.
4. kittest tests `tests/ui_dialogs.rs`.

## Tests (required)

Unit:

| Test name | Covers | What it checks |
|---|---|---|
| `txform_save_disabled_rules` | R-TXN-8 | amount "" / "0" disabled; no account disabled; Transfer to==from disabled; Withdrawal without payee disabled; valid enabled |
| `txform_reality_future_date_disabled` | R-TXN-9 | Reality + date tomorrow ⇒ disabled with error text; scenario + tomorrow ⇒ enabled |
| `txform_to_new_transaction_kinds` | R-TXN-8 | Withdrawal/Deposit map to TxKind; Transfer carries `to`; amount sign typed by user is irrelevant (engine applies abs) |
| `r_txn_10_reset_for_another_keeps_account_date_type` | R-TXN-10 | after reset: account, date, type kept; payee/amount/memo/category/contexts/to cleared |
| `txform_new_in_scenario_defaults_future` | – | `TxForm::new(_, Some(s), _)` ⇒ status Future; Reality ⇒ Cleared |

kittest (`tests/ui_dialogs.rs`):

| Test name | Covers | What it checks |
|---|---|---|
| `ui_txn_01_add_withdrawal_appears_in_table` | UI-TXN-01, R-TXN-8 | on an account screen click "＋ Add Transaction", type Payee "Lunch", Amount "12.5", click "Save" ⇒ dialog closed; engine has a −12.50 tx "Lunch"; payee label visible |
| `ui_txn_01_save_and_another_keeps_dialog_and_account` | UI-TXN-01, R-TXN-10 | "Save & Another" ⇒ tx saved, dialog still open, Payee field empty, Account combo still shows the account |
| `ui_add_transfer_creates_two_rows` | R-TXN-8 | choose "Transfer", pick To Account, amount 100, Save ⇒ two txs "Transfer to …"/"Transfer from …" |
| `ui_add_transaction_autocategorizes_payee` | R-CAT-6 | category with pattern "uber"; type payee "Uber Trip" ⇒ Category combo shows that category |
| `ui_rec_button_adds_pattern` | R-CAT-5 | pick category, type new payee, click "Rec" ⇒ pattern added; button now disabled |
| `ui_cmd_n_opens_add_transaction` | – | `key_press_modifiers(COMMAND, N)` on account screen ⇒ "New Transaction" heading present |
| `ui_add_account_via_sidebar_plus` | – | click "+" (hover "Add Account"), type name, Save ⇒ account in sidebar |
| `ui_add_account_save_disabled_without_name` | – | Save disabled while name empty |
| `ui_edit_account_calculate_and_save_starting_balance` | R-ACC-2, R-ACC-3 | account with txs summing 200; open Edit Account, type Current Balance 1000, Calculate ⇒ Starting 800; Save ⇒ "Starting Balance" tx of 800 dated the day before the earliest |
| `ui_edit_account_cancel_discards` | – | change name, Cancel ⇒ name unchanged |

**Accessible labels** (via `labeled_text` / `labelled_by`): "Payee", "Amount", "Memo", "Account Name", "Institution", "Currency",
"Starting Balance", "Current Balance"; ComboBoxes via `ComboBox::from_label("Account")`, "From Account", "To Account", "Category", "Type";
buttons by text ("Save", "Save & Another", "Cancel", "Rec", "Calculate", "＋ Add Transaction", "+").

## Pitfalls

- Two "Save" buttons can never be visible at once (one modal at a time), so `get_by_label("Save")` is unambiguous.
- Do not create the transaction in `show` while iterating lookups; collect the action, apply after drawing.
- The Modal must not close on outside click while a ComboBox popup is open. Check `ModalResponse::should_close` semantics; if it
  closes unexpectedly, only treat Esc as cancel.
- Amount text is free text; validation happens on Save. Never store a half-typed `Decimal`.
- Use `jiff::civil::Date::strftime("%b %-d, %Y")` for display; `.to_string()` gives ISO.

## Done when

```bash
cargo test -p pf-ui txform
cargo test -p pf-ui --test ui_dialogs
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules
cargo run -p pf    # manual: add each tx type, Save & Another, Rec, add/edit account, starting balance Calculate
```

## Out of scope

- Budget item and projection dialogs (T17). CSV import (T22).
