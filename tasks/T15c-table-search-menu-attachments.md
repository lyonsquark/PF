# T15c: Transaction table search bar, row context menu, attachments, ⌘F

**Phase:** 3 · **Deps:** T15b · **Crate(s):** `pf-ui` · **Size:** medium (≈400 lines + tests)

## Goal

Finish the transaction table: the search bar (scope, date range, result count, search running balance),
the right-click context menu (Add to Budget, Duplicate, Delete), the attachment column (attach, open safely,
remove), the search toggle button and ⌘F. After this, the account screen matches the Swift `TransactionListView`.

> If a command or pf-core function name differs from what is written here, use the actual one from
> `pf-core` / `pf-engine`; do not add new engine behaviour here.

## Read first

- `tasks/T15a-…md`, `tasks/T15b-…md`, `tasks/T13-ui-shell.md` (FileDialogs, Dialog::Alert, Toasts, shortcuts)
- Rules: R-FLT-3, R-FLT-4, R-FLT-6, R-BAL-8, R-TXN-6, R-TXN-7, R-TXN-11, R-TXN-14, R-UND-5
- Swift: `Views/TransactionListView.swift`: search bar (top of `body`), `filteredTransactions`, `searchRunningBalances`,
  `.contextMenu(forSelectionType:)`, attachment column, `openAttachmentSafely`, `attachFile`, toolbar search toggle, `addToBudget`
- T10a: `duplicate_transactions`, `delete_transactions`, `edit_transaction(TxField::Attachment)`, `is_safe_attachment`;
  T10c: `budget::add_to_budget`; T03: search filter functions

## Deliverables

```
crates/pf-ui/src/screens/account/search.rs   SearchState, SearchSpec, search_bar()
crates/pf-ui/src/screens/account/menu.rs     row context menu
crates/pf-ui/src/screens/account/rows.rs     (modify) RowsKey gains `search: Option<SearchSpec>`; build_rows applies it
crates/pf-ui/src/screens/account/table.rs    (modify) attachment cell, context menu, BLUE running balance when searching
crates/pf-ui/src/app.rs                      (modify) ⌘F toggles search on the Account screen
```

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchScope { #[default] Payee, Category, Context }   // labels "Payee", "Category", "Context"

/// UI state of the bar (lives in account::State).
pub struct SearchState {
    pub open: bool,
    pub text: String,
    pub scope: SearchScope,
    pub use_range: bool,
    pub start: Date,   // default today − 1 year (R-FLT-4), set when the bar first opens
    pub end: Date,     // default today
}
impl SearchState {
    /// R-FLT-6: closing clears text and turns the range off.
    pub fn close(&mut self);
    /// Some(spec) when the search actively filters: open AND (trimmed text non-empty OR use_range).
    pub fn active_spec(&self) -> Option<SearchSpec>;
}
#[derive(Debug, Clone, PartialEq)]
pub struct SearchSpec { pub text: String /* trimmed */, pub scope: SearchScope, pub range: Option<(Date, Date)> }
```

`build_rows` with `Some(spec)`: after the window filter, apply R-FLT-3 (text) and R-FLT-4 (inclusive range) using the
pf-core filter functions; then set each row's `running` to the **search running balance** (R-BAL-8: cumulative over the
filtered rows only, oldest→newest by (date, id)). Add `pub searching: bool` to the result, or return
`(Vec<TxRow>, bool)`, so the table can colour the column BLUE.

## Layout

Search bar (shown under the action row when `open`), on a light grey frame:

```
🔍 [Search Payee…            ] (Payee|Category|Context)  [x] Date Range [2025-03-15] to [2026-03-15]   12 results   ✕
```
- Text field: hint `Search {scope}…`, width 250, accessible label "Search" (`widget_info`).
- Scope: three `selectable_value`s in a row (segmented look), labels "Payee", "Category", "Context".
- `ui.checkbox(&mut use_range, "Date Range")`; when on: two `DatePickerButton`s (`id_salt("search_start")`, `"search_end"`) separated by weak "to".
- Right side: `"{n} results"` (small weak) when text non-empty or range on; then the "✕" button with hover text "Close search" ⇒ `close()` and `open = false`.

Action row additions:
- Search toggle button "🔍" with hover text "Search transactions" (closed) / "Hide search" (open). Toggling closed ⇒ `close()`.
- ⌘F (in `app.rs::handle_shortcuts`, only on `Screen::Account`, not while a dialog is open) toggles the same way and focuses the field when opening.

Attachment column (📎), replacing T15a's read-only cell:
- Has attachment: "📎" button, hover text = the path. Click ⇒ if `is_safe_attachment(path)` ⇒ `cx.files.open_path(path)` (error ⇒ toast),
  else `Dialog::Alert { title: "Unsafe Attachment", message: "This attachment cannot be opened because its file type is not allowed. Only PDFs, images, and text files may be opened." }`.
  When unlocked, also a small "✕" button, hover "Remove attachment" ⇒ `edit_transaction(id, TxField::Attachment(None))`.
- No attachment and unlocked: a weak "📎+" button, hover "Attach file" ⇒ `cx.files.pick_file("Select a file to attach to this transaction", &[])`;
  Some(path) ⇒ `TxField::Attachment(Some(path.display().to_string()))`.

Row context menu (`resp.context_menu` on the row response from TableBuilder, i.e. `row.response()`):
- On open, if the right-clicked row is not selected, the selection becomes just that row.
- **"Add to Budget"**: uses the first selected row (in row order) ⇒ `commands::budget::add_to_budget(engine, id)` ⇒ toast success `Added "{payee}" to Bills` (R-TXN-11).
- **"Duplicate"**: all selected ⇒ `duplicate_transactions` (R-TXN-6). Select the new rows afterwards.
- **"Delete"**: shown only if at least one selected row is unlocked; deletes the unlocked ones only ⇒ `delete_transactions` (R-TXN-7); remove them from the selection.
- Every item ends with `ui.close()`. Errors ⇒ toast.

## Steps

1. Extend `rows.rs` with search and write its unit tests first.
2. `search.rs` bar + toggle + ⌘F.
3. Attachment cell (uses `FakeDialogs` in tests).
4. Context menu.
5. kittest tests in `tests/ui_account_search.rs` and `tests/ui_account_menu.rs`.

## Tests (required)

Unit (`rows.rs` / `search.rs`):

| Test name | Covers | What it checks |
|---|---|---|
| `r_flt_3_search_payee_case_insensitive` | R-FLT-3 | "coff" matches "Coffee Shop"; whitespace-only text ⇒ not active |
| `r_flt_3_search_category_matches_name_or_full_name` | R-FLT-3 | "Food/Ca" and "Cafe" both match a "Food/Cafe" row |
| `r_flt_3_search_context_any_name` | R-FLT-3 | "tri" matches a tx tagged #trip and #home |
| `r_flt_4_date_range_inclusive` | R-FLT-4 | rows on start and end days included; day after excluded |
| `r_flt_6_close_clears_text_and_range` | R-FLT-6 | after `close()`, text empty and `use_range` false |
| `r_bal_8_search_running_balance_over_filtered_rows` | R-BAL-8 | 3 matching rows of 5 ⇒ running = cumulative over the 3 only |

kittest:

| Test name | Covers | What it checks |
|---|---|---|
| `ui_txn_04_search_filters_and_shows_count` | UI-TXN-04 | open search (click "🔍"), type "coff" in "Search" ⇒ only matching payees visible; "1 results" text present (Swift wording kept) |
| `ui_txn_04_cmd_f_toggles_search` | UI-TXN-04 | `key_press_modifiers(COMMAND, F)` opens; again closes and clears |
| `ui_txn_05_context_menu_duplicate` | UI-TXN-05, R-TXN-6 | right-click (`click_secondary`) a row ⇒ "Duplicate" ⇒ one more row dated today; `undo_name()` "Duplicate Transaction" |
| `ui_txn_05_context_menu_delete_multi` | UI-TXN-05, R-TXN-7 | select 2 rows (⌘-click), right-click, "Delete" ⇒ both gone; undo name "Delete Transactions" |
| `ui_txn_05_delete_hidden_when_all_locked` | R-TXN-1 | scenario mode, right-click a reality row ⇒ no "Delete" item |
| `ui_txn_05_add_to_budget_toast` | R-TXN-11 | "Add to Budget" ⇒ a Bill budget item exists with the payee; toast `Added "…" to Bills` |
| `ui_und_01_cmd_z_restores_deleted_row` | UI-UND-01 | delete via the menu, then Edit menu shows "Undo Delete Transaction"; `key_press_modifiers(COMMAND, Z)` (no field focused) ⇒ row visible again |
| `ui_attach_file_via_fake_dialog` | R-TXN-14 | FakeDialogs answer `/tmp/x.pdf`; click "📎+" ⇒ attachment set; undo name "Change Attachment" |
| `ui_open_unsafe_attachment_shows_alert` | R-TXN-14 | attachment `/tmp/x.exe`; click "📎" ⇒ alert "Unsafe Attachment"; FakeDialogs `opened()` empty |
| `ui_open_safe_attachment_calls_open` | R-TXN-14 | `/tmp/x.pdf` ⇒ `opened() == [/tmp/x.pdf]` |

**Accessible labels:** "Search" field, scope buttons, "Date Range" checkbox, "🔍" (and hover text), "✕" ("Close search"),
attachment buttons (hover texts), menu items by visible text.

## Pitfalls

- The cache key must include the search spec, or the table will not update while typing.
- "{n} results": Swift prints "1 results"; keep it (parity).
- `row.response().context_menu(…)` must be called inside the row closure; record what was chosen in a local and act after the table.
- Duplicating should not change the scroll position wildly; selecting the new rows is enough.
- Do not open real files in tests: always `FakeDialogs`.

## Done when

```bash
cargo test -p pf-ui search
cargo test -p pf-ui --test ui_account_search --test ui_account_menu
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules
cargo run -p pf    # manual: search each scope, date range, right-click actions, attach/open/remove a PDF
```

## Out of scope

- Keyboard Delete/Backspace to delete rows (Swift does not have it).
- Status items in the context menu (Swift has none; status is changed in the Status column).
- CSV import button (T22), Add Transaction button (T16).
