# T18: Obligations screen + detail dialog

**Phase:** 3 · **Deps:** T13 · **Crate(s):** `pf-ui` · **Size:** medium (≈350 lines + 150 lines tests)

## Goal

Port the Obligations screen: a list of obligations grouped by status, an "Add Obligation"
dialog that can create a matching context automatically, and an editable detail dialog
showing the computed balance, progress towards the target, and related transactions.
All writes go through engine commands from T10b.

> **Shell contract (T13).** The screen is `crates/pf-ui/src/screens/obligations.rs` exposing `#[derive(Default)] pub struct State`
> and `pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut egui::Ui)`; its state lives in `ScreenStates.obligations`.
> Dialogs are new variants of `dialogs::Dialog` (`Dialog::AddObligation(Box<AddObligationForm>)`,
> `Dialog::ObligationDetail(Box<ObligationDetailForm>)`) with match arms in `dialogs::show_active`; each dialog module exposes
> `pub fn show(cx: &mut UiCtx, ui: &mut egui::Ui, form: &mut Form) -> Outcome` (`Outcome::{KeepOpen, Close}`).
> Open them with `cx.dialogs.open(...)`. Writes go through `cx.engine` with `pf_engine::commands::obligations::*` (T10b).
> Shared widgets: `widgets::{money_label, labeled_text, colors}`. Picker lists: `cx.caches.lookups(cx.engine)` (`Lookups.accounts`, `.contexts`).
> Fallback: if a name here differs from the code, use the actual name in `pf-ui`/`pf-engine`; do not add engine behaviour here.

## Read first

- Docs: `PLAN.md` (UI section), `docs/egui-cheatsheet.md` (Modal, ComboBox, DatePickerButton, kittest), `docs/testing.md` (L5)
- Swift: `../PFApp/Sources/PFApp/Views/ObligationsView.swift` (whole file), `../PFApp/Sources/PFApp/Views/ObligationDetailView.swift` (whole file), `../PFApp/Sources/PFApp/Managers/ObligationManager.swift`
- Rules: R-OBL-1, R-OBL-4, R-OBL-5, R-OBL-6 (`docs/business-rules.md`)
- Engine (T10b, `pf_engine::commands::obligations`): `obligations_by_status(db) -> Vec<(ObligationStatus, Vec<ObligationId>)>`,
  `create_obligation(e, NewObligation { name, target_amount, current_balance, account, context: ContextChoice::{None, Existing(id), New(name)} })`,
  `suggest_context_name(db, name)`, `edit_obligation(e, id, ObligationField::{Name, Status, Target, Balance, DueDate, Notes, Account, Context})`,
  `delete_obligations(e, &[id])`. Balance: `pf-core::obligation::computed_balance` (T03) with scenario `None` (R-OBL-6).

## Deliverables

```rust
// crates/pf-ui/src/screens/obligations.rs
#[derive(Default)]
pub struct State { rows: Cached<(), Vec<(ObligationStatus, Vec<ObligationRow>)>> }   // keyed by engine.revision()
pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut egui::Ui);

// crates/pf-ui/src/dialogs/obligation_add.rs
pub struct AddObligationForm { pub name: String, pub target: String, pub initial: String,
    pub account: Option<AccountId>, pub create_new_context: bool, pub context_name: String, pub context: Option<ContextId> }
pub fn show(cx: &mut UiCtx, ui: &mut egui::Ui, form: &mut AddObligationForm) -> Outcome;

// crates/pf-ui/src/dialogs/obligation_detail.rs
pub struct ObligationDetailForm { pub id: ObligationId, pub name: String, pub target: String, pub initial: String, pub notes: String }
pub fn show(cx: &mut UiCtx, ui: &mut egui::Ui, form: &mut ObligationDetailForm) -> Outcome;
```

Add `Dialog::AddObligation(Box<AddObligationForm>)` and `Dialog::ObligationDetail(Box<ObligationDetailForm>)` to `dialogs/mod.rs`,
plus their arms in `show_active`. `AddObligationForm::default()` has `create_new_context = true` and `initial = "0"`.

## Screen layout

```
┌ Obligations ─────────────────────────────────── [ + Add Obligation ] ┐
│ Funding                                                               │
│ ┌───────────────────────────────────────────────────────────────────┐ │
│ │ Down Payment  (bold)                                    $12,500.00│ │
│ │ Simple Savings  (small, grey)                     of $50,000.00   │ │
│ │ New house fund  (smaller, faint, 1 line)                          │ │
│ │ Due: Dec 1, 2026   Created: 1/1/2026   (small, grey)              │ │
│ └───────────────────────────────────────────────────────────────────┘ │
│ Spending                                                              │
│  …                                                                    │
└───────────────────────────────────────────────────────────────────────┘
```

* Title "Obligations", with a button "+ Add Obligation" at the top right.
* One section per status in `ObligationStatus::ALL` order: Funding, Spending, Future, Deferred, Completed (R-OBL-5).
  A section header is the status string. **Empty sections are hidden.**
* Within a section, obligations are sorted by name (live only).
* Row: name (strong); account name (small, weak) if set; notes (small, weaker, one line, truncated) if non-empty;
  a line with `Due: {Mon D, YYYY}` if a due date is set and `Created: {M/D/YYYY}` if a created date is set.
  On the right: the computed balance (R-OBL-1/R-OBL-6: always under **Reality**, even when a scenario is active) in bold currency; below it, `of {target}` if a target is set.
* Clicking a row opens the detail dialog (`cx.dialogs.open(Dialog::ObligationDetail(...))`). Right-click opens a context menu with **Delete**, which calls the
  delete-obligation command (undo name "Delete Obligation").
* Currency: `widgets::money_label` (`$1,234.56`, `-$1,234.56`).

## Add Obligation dialog ("New Obligation")

```
┌ New Obligation ─────────────────────────────┐
│ Name                  [________________]    │
│ Target Amount         [________________]    │
│ Initial Funded Amount [0_______________]    │
│ Account               [None          ▾]     │
│ ── Context ──                               │
│ [x] Create New Context                      │
│ Context Name          [down-payment____]    │   (if Create New Context)
│ Select Context        [None          ▾]     │   (if not)
│                          [Cancel] [Save]    │
└─────────────────────────────────────────────┘
```

* Editing **Name** regenerates Context Name with `suggest_context_name(cx.engine.db(), &name)` (R-OBL-4) **while "Create New Context" is on**
  (Swift regenerates on every name change, overwriting any manual edit; match that).
* Target Amount: empty means None. Invalid number: show a red "Invalid amount" under the field and disable Save.
* Account picker lists live accounts sorted by name, with "None" first.
* Context picker lists live contexts sorted by name, with "None" first.
* **Save** is disabled when the name is empty **or no account is selected** (Swift rule).
* Save calls `create_obligation(cx.engine, NewObligation { name, target_amount, current_balance, account, context })` with
  `context = ContextChoice::New(context_name)` when Create New Context is on (the engine treats an empty trimmed name as None and
  writes the notes "{name} obligation"), otherwise `ContextChoice::Existing(id)` / `ContextChoice::None`. One undo step. Return `Outcome::Close`.
  On `Err(e)` show `cx.dialogs.alert("Obligation", e.to_string())` after closing, or keep the form open with a red error label (prefer the latter).
* Cancel closes without changes. All text fields have accessible labels (`labelled_by`), see the cheat sheet.

## Obligation detail dialog ("Edit Obligation")

```
┌ Edit Obligation ───────────────────────────────── [Done] ┐
│            [Down Payment_______________]  (large, centred)│
│                    $12,500.00   (36pt, green if ≥0 else red)
│                 [██████░░░░░░░░]   (progress, max 200px)  │
│                  of $50,000.00 goal                       │
│  [Funding|Spending|Future|Deferred|Completed]  (segmented)│
│ ───────────────────────────────────────────────────────── │
│ Details                                                   │
│         Account  [Simple Savings ▾]                       │
│         Context  [#down-payment  ▾]                       │
│   Target Amount  [50000.00]                               │
│ Initial Balance  [12500.00]                               │
│        Due Date  [2026-12-01 📅] (x)   or  [Set Due Date] │
│ Notes                                                     │
│ [Add notes…                         ] (multi-line, 2–4)   │
│ 📅 Created Jan 1, 2026     🕒 Updated Feb 1, 2026 9:30 AM │
│ ───────────────────────────────────────────────────────── │
│ Related Transactions                                  3   │
│ Coffee fund deposit                          $200.00 (green)
│ Feb 1, 2026 • Checking • Plan A(orange)                   │
│ memo text (faint, 1 line)                                 │
└───────────────────────────────────────────────────────────┘
```

* Balance = computed balance under Reality (R-OBL-1). Progress bar only if target > 0:
  `pct = min(1, balance/target)`, drawn with `egui::ProgressBar::new(max(0, pct))`; full bar tinted green, otherwise blue.
* The status segmented control is a row of `selectable_value`s. Changing it calls
  `edit_obligation(cx.engine, id, ObligationField::Status(s))` immediately (one undo step). The engine sets `last_updated_date`; the UI does not.
* Account / Context pickers (context shown as `#name`) commit immediately via `ObligationField::Account` / `ObligationField::Context`.
* Name, Target, Initial Balance and Notes commit **on focus lost or Enter** via `ObligationField::{Name, Target, Balance, Notes}`
  (one undo step each, like R-TXN-12). Empty notes ⇒ None. Target empty ⇒ None.
* Due date (`ObligationField::DueDate`): if set, show a `DatePickerButton` (commits on change) plus an "x" button with hover text
  "Remove due date" (sets None). If not set, show a "Set Due Date" button (sets `cx.engine.today()`).
* Dates line: `Created {Mon D, YYYY}` and `Updated {Mon D, YYYY h:mm AM}` (local time) if present.
* Related Transactions: only when the obligation has a context and there is ≥1 live transaction with that
  context (any scenario), sorted date desc. The header shows the count. Row: payee (medium weight); a line
  `{date} • {account} • {scenario (orange)}` (parts omitted when absent); memo (faint) if non-empty;
  the amount on the right, green if ≥ 0, normal text if negative.
* **Done** closes. Esc / clicking outside also closes (Modal `should_close`), committing any pending text edit first.

## Steps

1. Write the kittest tests below first (they fail).
2. Implement `screens/obligations.rs` (list + context menu). Cache the grouped rows in `State.rows` with
   `Cached::get(cx.engine.revision(), &(), || …)` (no recomputation per frame).
3. Implement the add dialog; wire the "+ Add Obligation" button to open it.
4. Implement the detail dialog; wire the row click.
5. Run Done-when.

## Tests (required)

All in `crates/pf-ui/tests/ui_obligations.rs`, using `common::engine_with_fixture()` and `common::harness(&mut app)` (T13's
`tests/common/mod.rs`) with `PfApp::new_for_test(engine)`. Assert through `app.engine().lock()` after `drop(harness)`.

| Test name | Covers | What it checks |
|---|---|---|
| `ui_obl_01_create_and_change_status_moves_group` | UI-OBL-01 | Add an obligation via the dialog (name, account), Save, then open it and click "Spending"; the engine shows status Spending and the row is under the "Spending" header |
| `ui_obl_sections_hidden_when_empty` | R-OBL-5 | With only Funding obligations, no "Completed" label is rendered |
| `ui_obl_save_disabled_without_account` | — | Name typed and no account ⇒ the Save button is disabled (clicking does nothing; the count is unchanged) |
| `ui_obl_context_name_generated` | R-OBL-4 | Typing "Down Payment" fills the Context Name with "down-payment"; with an existing "down-payment" it fills "down-payment-2" |
| `ui_obl_create_new_context_creates_context` | R-OBL-4 | After Save, a context with that name and notes "Down Payment obligation" exists and is linked |
| `ui_obl_delete_via_context_menu_is_undoable` | R-UND-1 | Delete removes it from the list; Cmd-Z brings it back |
| `ui_obl_list_uses_reality_balance_in_scenario` | R-OBL-6 | With a scenario active that has a tagged scenario transaction, the row still shows the Reality computed balance |
| `ui_obl_detail_shows_computed_balance` | R-OBL-1, R-OBL-6 | Detail shows `current_balance + tagged reality transactions`, formatted |
| `ui_obl_detail_set_and_remove_due_date` | — | "Set Due Date" sets today; "x" clears it |
| `ui_obl_detail_text_edit_commits_once` | R-TXN-12 analogue | Editing Notes then pressing Enter creates exactly one undo step |

## Pitfalls

- R-OBL-6: the list and detail use the **Reality** computed balance even when a scenario is active. Do not pass the active scenario.
- `ProgressBar` takes `f32` in 0..=1; convert the Decimal ratio with care (target > 0 is checked first).
- Never lock the engine yourself: `UiCtx.engine` is already the locked `&mut Engine` for this frame (T13). Copy the values
  you need out of `cx.engine.db()` before calling a command (the borrow checker requires it).
- Every text field needs an accessible label or the kittest queries will not find it.

## Done when

```bash
cargo test -p pf-ui --test ui_obligations
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules
```
Manual: `cargo run -p pf`, open Obligations, add one, change its status, delete, undo.

## Out of scope

- Obligation math (T03) and commands (T10b).
- Dashboard "available balance" (T14). Balance-sheet obligations (T23).
