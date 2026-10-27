# T15b: Transaction table inline editing, lock rules, status, auto-categorize

**Phase:** 3 · **Deps:** T15a · **Crate(s):** `pf-ui` · **Size:** medium-large (≈450 lines + tests)

## Goal

Make the T15a table editable in place, exactly like the Swift `TransactionListView`: date picker, payee,
amount, category, memo, context tags and status, each committing **one** undo step per edit through
`commands::transactions::{edit_transaction, change_status}`. Locked rows (scenario mode, reality rows) are
shown disabled. Errors from the engine appear as toasts.

> If a command or pf-core function name differs from what is written here, use the actual one from
> `pf-core` / `pf-engine`; do not add new engine behaviour here.

## Read first

- `tasks/T15a-account-detail-table.md`, `tasks/T13-ui-shell.md` (widgets, Lookups, Toasts)
- Rules: R-TXN-1, R-TXN-2, R-TXN-3, R-TXN-4, R-TXN-5, R-TXN-9, R-TXN-12, R-TXN-13, R-CAT-4
- Swift: `Views/TransactionListView.swift` (columns, `bindUndo*`, `changeStatus`, `isTransactionLocked`, Payee `onChange`),
  `Views/AmountEditField.swift` (all)
- T10a: `edit_transaction(e, id, TxField) -> Result<bool>`, `change_status(e, id, status)`, `is_locked`

## Deliverables

```
crates/pf-ui/src/widgets/amount_field.rs   AmountField widget (shared with dialogs)
crates/pf-ui/src/widgets/status.rs         status colours + status picker
crates/pf-ui/src/widgets/contexts.rs       context chips + toggle menu
crates/pf-ui/src/widgets/category.rs       category picker ComboBox
crates/pf-ui/src/screens/account/edit.rs   CellEdit buffer + commit helpers
crates/pf-ui/src/screens/account/table.rs  (modify) editable cells
```

```rust
// widgets/amount_field.rs: port of AmountEditField.swift
/// Text shown while editing: no grouping, no currency, at most 2 decimals, trailing zeros trimmed
/// (1234.5 ⇒ "1234.5", -12 ⇒ "-12", 0.125 ⇒ "0.13" (round half away from zero), 1000 ⇒ "1000").
pub fn amount_edit_text(d: Decimal) -> String;
/// Parse user input: strip "$" and ",", trim; None if not a decimal.
pub fn parse_amount_input(s: &str) -> Option<Decimal>;
/// Idle: formatted money label (GREEN if positive; weak if locked). Click (when !locked) ⇒ edit mode with a TextEdit
/// pre-filled by amount_edit_text and focused. Enter / focus lost ⇒ Some(new) if it parses AND differs; Esc ⇒ cancel (None).
/// `buf` holds the edit text while editing (None when idle); the caller stores it.
pub fn amount_field(ui: &mut egui::Ui, a11y_label: &str, value: Decimal, locked: bool, buf: &mut Option<String>) -> Option<Decimal>;

// widgets/status.rs
pub fn status_color(s: TransactionStatus) -> Option<egui::Color32>;   // Cleared ⇒ None (normal text), Pending GREEN, Reconciled BROWN, Future RED
/// ComboBox listing Cleared, Pending, Reconciled, Future (in that order, Swift picker order), coloured. Returns Some(new) when picked ≠ current.
pub fn status_picker(ui: &mut egui::Ui, id_salt: impl std::hash::Hash, current: TransactionStatus, enabled: bool) -> Option<TransactionStatus>;

// widgets/category.rs
/// ComboBox: "None" first, then Lookups.categories (already sorted, R-CAT-4). Returns Some(new) when changed.
pub fn category_picker(ui: &mut egui::Ui, id_salt: impl std::hash::Hash, current: Option<CategoryId>, cats: &[(CategoryId, String)], enabled: bool) -> Option<Option<CategoryId>>;

// widgets/contexts.rs
/// Chips "#name" (fill colour × 0.15, text colour), or weak "No Context".
pub fn context_chips(ui: &mut egui::Ui, chips: &[(ContextId, String, Option<String>)]);
/// "⊕" menu (hover text "Add or remove context tags") listing non-archived contexts, "✓ " prefix when selected.
/// Clicking toggles: remove if present, else append at the end. Returns Some(new list) when changed.
pub fn context_toggle_menu(ui: &mut egui::Ui, current: &[ContextId], all: &[(ContextId, String, Option<String>)]) -> Option<Vec<ContextId>>;

// screens/account/edit.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditCol { Payee, Memo, Amount }
/// The one text cell being edited (immediate-mode buffers must outlive a frame).
#[derive(Debug, Clone, PartialEq)]
pub struct CellEdit { pub id: TransactionId, pub col: EditCol, pub text: String }
/// Commit one field; maps the engine result to UI feedback (toast on Err; nothing on Ok(false/true)).
pub fn commit(cx: &mut UiCtx, id: TransactionId, field: TxField);
pub fn commit_status(cx: &mut UiCtx, id: TransactionId, s: TransactionStatus);
```
Add `pub(crate) cell: Option<CellEdit>` and `pub(crate) amount_buf: Option<(TransactionId, String)>` to the account `State`.

## Cell behaviour (match Swift)

| Column | Widget (unlocked) | Commit | Undo name (engine) |
|---|---|---|---|
| Date | `DatePickerButton::new(&mut d).id_salt(("date", id))` on a copy `d` of the row date | when `d != row.date` ⇒ `TxField::Date(d)` immediately. The engine rejects a future date for reality Cleared/Reconciled rows (R-TXN-9) ⇒ toast with its message | Change Date |
| Payee | `TextEdit::singleline` frame NONE, full width | Enter or focus lost, only if changed (R-TXN-12). The engine then auto-categorizes (R-TXN-13) as its own step | Change Payee (+ Auto-categorize) |
| Amount | `amount_field` | Enter or focus lost, only if parsed and changed; Esc cancels | Change Amount |
| Category | `category_picker` | on change | Change Category |
| Memo | like Payee; empty ⇒ `TxField::Memo(None)` | Enter or focus lost | Change Memo |
| Context | `context_chips` + `context_toggle_menu` (menu only when unlocked) | on change ⇒ `TxField::Contexts(list)` | Change Contexts |
| Status | `status_picker` | on change ⇒ `change_status`; errors (R-TXN-2/R-TXN-3) ⇒ toast `"⚠️ {message}"` | Change Status |

**Text-cell pattern** (Payee/Memo). Copy it exactly; the buffer survives frames only while that cell is being edited:

```rust
let editing = st.cell.as_ref().filter(|c| c.id == row.id && c.col == EditCol::Payee);
let mut text = editing.map(|c| c.text.clone()).unwrap_or_else(|| row.payee.clone());
let resp = ui.add_enabled(!row.locked, egui::TextEdit::singleline(&mut text).frame(egui::Frame::NONE).desired_width(f32::INFINITY));
resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, !row.locked, format!("Payee {}", row.payee)));
if resp.has_focus() {
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) { st.cell = None; resp.surrender_focus(); }
    else { st.cell = Some(CellEdit { id: row.id, col: EditCol::Payee, text }); }
} else if resp.lost_focus() {
    if let Some(c) = st.cell.take().filter(|c| c.id == row.id && c.col == EditCol::Payee) {
        if c.text != row.payee { pending.push(Pending::Field(row.id, TxField::Payee(c.text))); }
    }
}
```
Collect commits in a local `Vec<Pending>` during drawing, and apply them **after** the table closure (the closure borrows `ui`
and the cached rows; the commit bumps the revision and invalidates the rows).

**Locked rows** (R-TXN-1): every widget is disabled (`add_enabled(false, …)` / `add_enabled_ui(false, …)`), weak text, no context menu
button. The engine also refuses edits (`Err(Locked)`), so a bug in the UI cannot corrupt data.

**Date picker range.** Swift disallows future dates in the picker for reality Cleared/Reconciled rows. `DatePickerButton` has no
max-date option, so rely on the engine's validation and show the toast. Check `egui_extras-0.37.0/src/datepicker/button.rs`
for a range option first; if it has one, use it.

## Steps

1. Unit-test `amount_edit_text` and `parse_amount_input` first.
2. Build the four widgets with small kittest tests each (`Harness::new_ui` on just the widget).
3. Wire them into `table.rs` with the pending-commit pattern.
4. kittest tests in `tests/ui_account_edit.rs`.

## Tests (required)

Unit:

| Test name | Covers | What it checks |
|---|---|---|
| `amount_edit_text_formats_like_swift` | – | 1234.5⇒"1234.5", -12⇒"-12", 0.125⇒"0.13", 1000⇒"1000", -0.5⇒"-0.5" |
| `parse_amount_input_strips_symbols` | – | "$1,234.50"⇒1234.50, " -12 "⇒-12, "abc"⇒None, ""⇒None |
| `status_color_matches_swift` | – | Cleared None, Pending GREEN, Reconciled BROWN, Future RED |

kittest (`tests/ui_account_edit.rs`):

| Test name | Covers | What it checks |
|---|---|---|
| `ui_txn_02_payee_commit_on_enter_one_undo_entry` | UI-TXN-02, R-TXN-12 | click "Payee Coffee", type " Shop", press Enter ⇒ payee "Coffee Shop"; `undo_name() == Some("Change Payee")`; one undo restores "Coffee" |
| `ui_txn_02_unchanged_payee_creates_no_undo` | R-TXN-12 | focus and Enter without typing ⇒ `undo_name()` unchanged |
| `ui_txn_02_escape_cancels_edit` | – | type, then Esc ⇒ payee unchanged, no undo step |
| `ui_txn_03_locked_rows_render_disabled` | UI-TXN-03, R-TXN-1 | activate a scenario ⇒ the reality row's "Payee …" node is disabled (`node.is_disabled()`, or check the accesskit `disabled` flag; see kittest `Node` API); a scenario row's is enabled |
| `ui_txn_13_payee_edit_autocategorizes` | R-TXN-13 | uncategorized row; category "Coffee" has pattern "starbucks"; edit payee to "Starbucks #12" ⇒ category set; undo once ⇒ category None, payee kept; undo again ⇒ payee old |
| `ui_amount_edit_commits_and_cancels` | R-TXN-12 | click amount, type "-42.5", Enter ⇒ amount −42.5, undo name "Change Amount"; Esc path ⇒ unchanged |
| `ui_status_future_on_reality_shows_toast_no_change` | R-TXN-2 | pick "Future" on a reality row ⇒ status unchanged; toast text contains "Future status is only for scenario transactions" |
| `ui_status_pending_promotes_scenario_row` | R-TXN-4 | in scenario mode, pick "Pending" on a scenario row ⇒ row becomes reality (`scenario_id == None`) |
| `ui_contexts_toggle_adds_and_removes` | – | open "⊕" menu, click "trip" ⇒ tx has context; again ⇒ removed |
| `ui_category_picker_sets_category` | – | choose a category in the row's ComboBox ⇒ engine updated, undo name "Change Category" |

ComboBoxes in rows: give each one an id salt with the row id and an accessible label (`widget_info` with `Role::ComboBox`,
label `"Category {payee}"` / `"Status {payee}"`). Test payees must be unique.

**Accessible labels:** `Payee {payee}`, `Memo {payee}`, `Amount {payee}`, `Category {payee}`, `Status {payee}`, `Date {payee}`,
menu button "⊕" with hover text "Add or remove context tags".

## Pitfalls

- Never commit on every keystroke. Swift's `bindUndo` committed per change, but R-TXN-12 (and undo sanity) require commit on Enter or focus loss.
- `resp.lost_focus()` is true for the one frame after Enter; `has_focus()` is false then. Handle the commit in that frame.
- Apply commits after drawing; the rows cache is borrowed while drawing.
- The ⌘Z shortcut is ignored by the shell while a cell has focus (the field does its own undo). Do not change that.
- egui ComboBox inside a 24 px row: `ComboBox::from_id_salt(...).width(150.0)`; keep the row height ≥ 24.
- Disabled-ness in kittest: inspect `egui_kittest-0.37.0/src/node.rs` for the right accessor before writing the test.

## Done when

```bash
cargo test -p pf-ui amount
cargo test -p pf-ui --test ui_account_edit
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules
cargo run -p pf    # manual: edit every column, undo/redo each from the Edit menu, try a scenario (locked rows)
```

## Out of scope

- Search, context menu (duplicate/delete/add to budget), attachments, keyboard delete (T15c).
- Add Transaction dialog (T16).
