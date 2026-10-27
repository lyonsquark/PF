# T20: Categories screen + Contexts screen

**Phase:** 3 · **Deps:** T13 · **Crate(s):** `pf-ui` · **Size:** medium (≈450 lines + 200 lines tests)

## Goal

Port two management screens. **Categories**: a tree grouped by type, with a New/Edit editor covering
icon, notes, auto-match patterns, "apply rules to uncategorized", type and parent. **Contexts** ("tags"):
Active and Archived sections, archive/unarchive, a New/Edit editor with colour, and a guarded delete.

> **Shell contract (T13).** Screens `screens/categories.rs` and `screens/contexts.rs`, each with `#[derive(Default)] pub struct State`
> and `pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut egui::Ui)` (state in `ScreenStates.categories` / `.contexts`).
> Editors are new `Dialog::CategoryEdit(Box<CategoryForm>)` / `Dialog::ContextEdit(Box<ContextForm>)` variants with arms in
> `dialogs::show_active`; modules expose `show(cx, ui, form) -> Outcome`. Alerts use `cx.dialogs.alert(title, message)`.
> Commands (T10b): `commands::categories::{save_category(e, Option<CategoryId>, CategoryInput) -> SaveCategoryResult { id, rejected_patterns },
> apply_rules_to_uncategorized(e, id, patterns_csv) -> usize, delete_category(e, id), category_tree(db) -> Vec<(CategoryId, usize)>}` and
> `commands::contexts::{create_context(e, ContextInput), update_context(e, id, ContextInput), set_context_archived(e, id, bool),
> delete_context(e, id), contexts_by_section(db) -> (active, archived)}`.
> The colour widget is `widgets/color.rs` (from T19). If T19 is not done yet, create that file with exactly the API in T19's
> Deliverables and note it in STATUS. Fallback: if a name here differs from the code, use the actual name in `pf-ui`/`pf-engine`;
> do not add engine behaviour here.

## Read first

- Docs: `docs/egui-cheatsheet.md`, `docs/testing.md` (L5)
- Swift: `../PFApp/Sources/PFApp/Views/CategoriesView.swift` (whole file), `../PFApp/Sources/PFApp/Views/ContextsView.swift` (whole file)
- Rules: R-CAT-2, R-CAT-3, R-CAT-4, R-CAT-7, R-CAT-8, R-CTX-1, R-CTX-2, R-CTX-3, R-CTX-4
- Engine: `pf_engine::commands::{categories, contexts}` (T10b; signatures above)

## Deliverables

```rust
// crates/pf-ui/src/screens/categories.rs
#[derive(Default)] pub struct State { rows: Cached<(), Vec<CategoryRow>> }   // sections + tree, keyed by revision
pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut egui::Ui);
// crates/pf-ui/src/dialogs/category_edit.rs
pub struct CategoryForm { pub editing: Option<CategoryId>, pub name: String, pub icon: String, pub notes: String,
    pub patterns: String /* comma separated */, pub category_type: CategoryType, pub parent: Option<CategoryId> }
pub fn show(cx: &mut UiCtx, ui: &mut egui::Ui, form: &mut CategoryForm) -> Outcome;

// crates/pf-ui/src/screens/contexts.rs
#[derive(Default)] pub struct State { counts: Cached<(), BTreeMap<ContextId, usize>> }
pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut egui::Ui);
// crates/pf-ui/src/dialogs/context_edit.rs
pub struct ContextForm { pub editing: Option<ContextId>, pub name: String, pub notes: String, pub rgb: [u8; 3] }
pub fn show(cx: &mut UiCtx, ui: &mut egui::Ui, form: &mut ContextForm) -> Outcome;
```

## Categories screen

```
┌ Categories ─────────────────────────────────── [ + Add Category ] ┐
│ Income                                                             │
│   💵 Salary                                                    ✎   │
│      Primary Income  (small, grey)                                 │
│      ↳ Bonus                                                   ✎   │
│ Expense                                                            │
│   Groceries                                                    ✎   │
│   Housing                                                      ✎   │
│      ↳ Utilities                                               ✎   │
│ Transfer                                                           │
│   …                                                                │
└────────────────────────────────────────────────────────────────────┘
```

* One section per `CategoryType` in order **Income, Expense, Transfer** (header = the capitalised type).
  A section is shown only if it has at least one live top-level category of that type.
* Within a section: use `category_tree(db)` order (parents before children, siblings by name), filtered to that type:
  depth 0 rows plain, depth ≥ 1 rows indented 20 px × depth with a "↳" prefix (R-CAT-8).
* Row: icon (only if it is an emoji; SF Symbol names render nothing, see PLAN.md), name, notes (small, weak) if
  non-empty, and a "✎" on the right.
* **Double-click** a row ⇒ open the editor prefilled (patterns joined with `", "`). Single click does nothing (matches Swift).
* Right-click ⇒ context menu "Delete" ⇒ `delete_category(cx.engine, id)` (R-CAT-7: cascades to subcategories and clears
  `category_id` on affected transactions, one undo step "Delete Category"). No confirmation.

### Category editor ("New Category" / "Edit Category")

```
┌ Edit Category ──────────────────────────────────────────────┐
│ Name                         [Groceries______________]      │
│ Icon (Emoji or SF Symbol)    [🛒_____________________]      │
│ Notes                        [Food shopping           ]     │  (multi-line, 3–6 rows)
│ Payee Auto-Match Rules (comma separated)                    │
│ [KROGER, WHOLE FOODS, ^ALDI.*        ] [🪄]                 │  (multi-line, 2–4 rows)
│ Type  [Income | Expense | Transfer]                         │  (segmented)
│ ── Hierarchy ──                                             │
│ Parent Category [None            ▾]                         │
│                                         [Cancel] [Save]     │
└─────────────────────────────────────────────────────────────┘
```

* **🪄 (apply rules)** button: hover text "Apply rules to uncategorized transactions" (or "Save category first to apply
  rules" when creating). Disabled when creating a new category or when the patterns text is blank. Clicking calls
  `apply_rules_to_uncategorized(cx.engine, id, &form.patterns)` (R-CAT-3) with the patterns **as currently typed** (not yet saved), then shows an alert titled
  "Apply Rules" with the message `Applied to N uncategorized transaction(s).` and an OK button.
* **Type** segmented control; it is **disabled when editing a category that has live subcategories**. Changing the type
  clears the selected parent.
* **Parent Category** picker: "None" plus live top-level categories of the **same type**, excluding the category being edited.
* **Save** is disabled while the trimmed name is empty. Save calls `save_category(cx.engine, form.editing, CategoryInput { name,
  category_type, icon, notes, patterns_csv, parent })` (the engine trims and blanks ⇒ None). The engine validates patterns
  (R-CAT-2) and returns `rejected_patterns`. If that list is non-empty, show an alert titled "Invalid Regex Pattern"
  with the message
  `The following patterns are not valid regular expressions and were removed:\n• p1\n• p2` and an OK button.
* Cancel / Esc closes.

## Contexts screen

```
┌ Contexts ───────────────────────────────────── [ + Add Context ] ┐
│ Active                                                            │
│  ● Vacation                                                12  ✎  │
│    Hawaii Trip (small, grey)                                      │
│ Archived                                                          │
│  ● Tax Prep 2025  (archived)                                3  ✎  │   (row drawn at 60% opacity)
└───────────────────────────────────────────────────────────────────┘
```

* `contexts_by_section(db)` gives (active, archived), each sorted by name. Section "Active" is always shown. Section "Archived" is shown only if
  non-empty, and its rows are dimmed (multiply text and dot colours by 0.6 alpha) (R-CTX-3).
* Row: a colour dot (context colour, blue `#0000ff` if None or invalid, R-CTX-4); the name; an "archived" pill (small,
  grey background) for archived rows; notes (small, weak) if non-empty; on the right, the count of live transactions
  carrying this context, then "✎".
* **Click** a row ⇒ open the editor prefilled.
* Right-click (Active): "Archive", a separator, "Delete". Right-click (Archived): "Unarchive", a separator, "Delete".
  Archive and Unarchive call `set_context_archived(cx.engine, id, true/false)` (one undo step each).
* Delete ⇒ `delete_context(cx.engine, id)`. If it returns `Err(e)` (R-CTX-1 Validation), show
  `cx.dialogs.alert("Cannot Delete", e.to_string())`. The message is the engine's, verbatim. Otherwise it is one undo step (R-CTX-2).

### Context editor ("New Context" / "Edit Context")

* Fields: "Context Name" (text), "Context Color" (colour field; default blue `[0,0,255]`), "Notes" (multi-line, 3–6 rows).
* **Save** is disabled while the trimmed name is empty. Save calls `create_context` / `update_context` with
  `ContextInput { name, color_hex: Some(to_hex(rgb)), notes }` (the engine trims; empty notes ⇒ None).

## Steps

1. Write the tests.
2. Categories: screen, editor, both alerts. Cache the tree rows in `State.rows` (`Cached::get(cx.engine.revision(), &(), …)`).
3. Contexts: screen, editor, Cannot Delete alert. Cache the per-context transaction counts in `State.counts`
   (one pass over transactions per revision).
4. Run Done-when.

## Tests (required)

`crates/pf-ui/tests/ui_categories.rs` and `crates/pf-ui/tests/ui_contexts.rs` (T13's `tests/common::{engine_with_fixture, harness}`,
`PfApp::new_for_test`).

| Test name | Covers | What it checks |
|---|---|---|
| `ui_cat_01_child_shown_under_parent` | UI-CAT-01, R-CAT-8 | Create "Coffee" with parent "Dining Out" via the editor ⇒ the row appears right after "Dining Out" with the "↳" prefix |
| `ui_cat_sections_by_type` | R-CAT-8 | The "Income" header comes before "Expense"; no "Transfer" header when there are no transfer categories |
| `ui_cat_invalid_regex_alert` | R-CAT-2 | Save with patterns "ok, [bad" ⇒ the "Invalid Regex Pattern" alert lists "• [bad"; the category keeps only "ok" |
| `ui_cat_apply_rules_alert` | R-CAT-3 | Editing a category, then clicking 🪄 ⇒ the alert shows "Applied to N uncategorized transaction(s)." with N equal to the engine count |
| `ui_cat_apply_disabled_for_new` | — | In "New Category", the 🪄 button is disabled |
| `ui_cat_parent_picker_same_type_only` | — | With type Income, the parent list holds no Expense categories; changing type clears the parent |
| `ui_cat_delete_cascades_undo` | R-CAT-7 | Delete the parent ⇒ the parent and child disappear and their transactions show no category; Cmd-Z restores both, categories included |
| `ui_ctx_01_archive_moves_section` | UI-CTX-01, R-CTX-3 | Archive "Vacation" ⇒ it is under the "Archived" header with the "archived" pill; Unarchive moves it back |
| `ui_ctx_delete_refused_shows_alert` | R-CTX-1 | Deleting a context used by an obligation shows "Cannot Delete" with the exact engine message; the context still exists |
| `ui_ctx_delete_removes_from_transactions` | R-CTX-2 | Delete an unused-by-obligation context ⇒ no transaction lists it; undo restores |
| `ui_ctx_count_column` | — | The count shown equals the live transactions with that context |
| `ui_ctx_create_default_blue` | R-CTX-4 | New context saved without touching the colour ⇒ `color_hex == "#0000ff"` |

## Pitfalls

- Double-click: `response.double_clicked()`. A single click must **not** open the Categories editor.
- The 🪄 button applies the **typed** patterns. Validate them through the same engine/core function as Save.
- R-CAT-7 (updated): deleting a category clears `category_id` on affected live transactions, and undo restores it. The engine
  does this; the UI only calls `delete_category`.
- An emoji check: treat the icon as an emoji if its first char is outside ASCII (simple rule; SF Symbol names are ASCII).

## Done when

```bash
cargo test -p pf-ui --test ui_categories --test ui_contexts
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules
```

## Out of scope

- Pattern matching logic (T04) and commands (T10b).
- The "Rec" button in Add Transaction (T16).
