# T17: Budget screen, budget item editor, projection dialog

**Phase:** 3 · **Deps:** T13, T10c · **Crate(s):** `pf-ui` · **Size:** large (≈550 lines + tests)

## Goal

Implement the Budget screen (Bills / Expenses / Income tabs with a table), the Add/Edit budget item dialog
(including Specific Months, End Date and "Reproject to Scenario"), and the "Generate Projection" dialog with
per-item date previews. All writes go through T10c `commands::budget::*`.

> If a command or pf-core function name differs from what is written here, use the actual one from
> `pf-core` / `pf-engine`; do not add new engine behaviour here.

## Read first

- `tasks/T13-ui-shell.md` (Dialog, Outcome, Lookups, widgets), `tasks/T15b-…md` (`amount_field`/`parse_amount_input`,
  `category_picker`), `tasks/T16-…md` (form dialog style with `egui::Grid`)
- Rules: R-BUD-1, R-BUD-2, R-PRJ-1…R-PRJ-9
- Swift: `Views/BudgetView.swift`, `Views/BudgetItemEditorView.swift`, `Views/BudgetProjectionView.swift` (all three, fully)
- T10c `crates/pf-engine/src/commands/budget.rs`: `BudgetItemInput`, `default_budget_input`, `save_budget_item`,
  `delete_budget_items`, `budget_items_by_type`, `ProjectionTarget`, `ProjectionRequest`, `generate_projection`,
  `last_projection`, `reproject_item`; T02 `pf_core::projector::generate_dates`

## Deliverables

```
crates/pf-ui/src/screens/budget.rs          State, show(): tabs, toolbar, table, context menu
crates/pf-ui/src/dialogs/budget_item.rs     BudgetItemForm + show()
crates/pf-ui/src/dialogs/projection.rs      ProjectionForm + show()
crates/pf-ui/src/dialogs/mod.rs             (modify) Dialog::BudgetItemEditor(Box<BudgetItemForm>), Dialog::Projection(Box<ProjectionForm>)
```

```rust
// screens/budget.rs
#[derive(Default)]
pub struct State {
    pub tab: BudgetItemType,                               // default Bill
    pub selection: BTreeSet<BudgetItemId>,
    rows: Cached<BudgetItemType, Vec<BudgetRow>>,
}
pub struct BudgetRow {
    pub id: BudgetItemId, pub name: String, pub category_label: Option<String>, pub amount: Decimal,
    pub is_income: bool, pub frequency_label: String, pub start: Date, pub account: Option<String>,
}
/// Pure: R-BUD-1 rows for a tab. frequency_label = "Specific Months ({n})" for SpecificMonths, else the frequency label.
pub fn build_rows(db: &Db, tab: BudgetItemType) -> Vec<BudgetRow>;
pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut egui::Ui);

// dialogs/budget_item.rs
pub struct BudgetItemForm {
    pub id: Option<BudgetItemId>,          // None = add
    pub input: BudgetItemInput,            // from default_budget_input(tab, today) or the existing item
    pub amount_text: String,
    pub has_end_date: bool,
    pub end_date: Date,                    // shown when has_end_date; default today + 1 year
    pub error: Option<String>,
}
impl BudgetItemForm {
    pub fn new_for(tab: BudgetItemType, today: Date) -> Self;
    pub fn edit(db: &Db, id: BudgetItemId, today: Date) -> Option<Self>;
    /// Swift nameLabel: Bill "Payee", Expense "Name (autofilled from Category)", Income "Source".
    pub fn name_label(&self) -> &'static str;
    /// Fills input.amount/end_date from the text fields; Err(message) if the amount does not parse.
    pub fn to_input(&self) -> Result<BudgetItemInput, String>;
}
pub fn show(cx: &mut UiCtx, ui: &mut egui::Ui, form: &mut BudgetItemForm) -> Outcome;

// dialogs/projection.rs
pub struct ProjectionForm {
    pub new_scenario: bool, pub new_name: String, pub target: Option<ScenarioId>,
    pub start: Date, pub end: Date,                  // defaults today, today + 3 months; prefilled by last_projection (R-PRJ-8)
    pub selected: BTreeSet<BudgetItemId>,            // all live items initially
    pub generated: Option<(usize, f64)>,             // (count, time shown) for the footer + auto-close after 1.5 s
    pub error: Option<String>,
}
impl ProjectionForm {
    pub fn new(db: &Db, today: Date) -> Self;
    /// R-PRJ-7: disabled when nothing selected, or (new && trimmed name empty), or (!new && target None).
    pub fn generate_disabled(&self) -> bool;
}
pub fn show(cx: &mut UiCtx, ui: &mut egui::Ui, form: &mut ProjectionForm) -> Outcome;
```

## Layouts

### Budget screen

```
Budget                                                     [Project]  [＋ Add]
        ( Bills | Expenses | Incomes )      ← tab labels: type label + "s" (Swift): "Bills", "Expenses", "Incomes"
┌ Payee ──────────┬ Category ─────────┬ Amount ──┬ Frequency ───────────┬ Start Date ──┬ Account ─┐
│ Landlord        │ 🏠 Housing         │ $1,500.00│ Monthly              │ Jan 1, 2026  │ Checking │
│ Gym             │ Uncategorized      │    $40.00│ Specific Months (3)  │ Jan 5, 2026  │ —        │
└─────────────────┴───────────────────┴──────────┴──────────────────────┴──────────────┴──────────┘
```
- First column header: "Payee" (Bills), "Expense Name" (Expenses), "Source" (Income).
- Category: weak label; missing ⇒ weaker "Uncategorized". Amount right-aligned, GREEN for income. Start date `%b %-d, %Y`. Account or "—" (weak).
- Selection like T15a (click, ⌘-click). **Double-click** a row ⇒ open the editor (`row.response().double_clicked()`).
- Context menu: **"Edit"** (enabled only when exactly one row is selected) and **"Delete"** ⇒ `delete_budget_items(selected)`.
- "[Project]" ⇒ `Dialog::Projection(Box::new(ProjectionForm::new(db, today)))`; "[＋ Add]" ⇒ editor for the current tab.
- Empty tab: weak "No budget items".

### Budget item editor (title "Add Bill" / "Edit Income" …, i.e. "Add {type}" / "Edit {type}"), min 420 × 460

```
Type        (Bill | Expense | Income)
Category    [Select Category ▾]          ← when changed and Name is empty ⇒ Name = category's plain name
{name_label}[                ]
Amount      [                ]
Account     [Select Account ▾]
Start Date  [2026-01-01]
[ ] End Date   [2027-03-15]               ← date shown only when ticked (R-BUD-2: engine rejects end < start; show its error)
── Recurrence ──
Frequency   [Monthly ▾]                   ← Weekly, Monthly, Every Other Month, Quarterly, Yearly, Specific Months
 [x]Jan [ ]Feb [ ]Mar [x]Apr              ← only for Specific Months: 12 checkboxes in a 4-column egui::Grid, short month names
 [ ]May …
── Notes (Optional) ──
[ multiline, 3 rows             ]
── Reproject to Scenario ──               ← only when editing AND last_projection() gives scenario+start+end
Scenario: **Plan A**       Mar 1, 2026 → Jun 1, 2026      [Reproject]
                                                    [Cancel] [Save]   (Save disabled when the trimmed name is empty)
```
- Save ⇒ `save_budget_item(engine, form.id, form.to_input()?)`; Ok ⇒ close; Err ⇒ RED error.
- Reproject ⇒ `reproject_item(engine, id, form.to_input()?)` ⇒ success toast with the returned text
  (`Added 12 transactions to Plan A`); the dialog stays open. Note: Swift's reproject only **adds** rows (see the T10c rule-conflict note).

### Generate Projection, min 520 × 380

```
Generate Projection
[ ] New Scenario    [Select… ▾]  or  [Scenario Name      ]
From [2026-03-15]        Until [2026-06-15]
─────────────────────────────────────────────────────────────
Bills                                                    None   ← "All" when not all selected, "None" when all selected
[x] Landlord   $1,500.00 · 3/1, 4/1, 5/1, 6/1
[x] Gym        $40.00    · no dates                              ← weak when no dates
Expenses …                    Income … (GREEN amounts)            ← sections only if non-empty
─────────────────────────────────────────────────────────────
✓ Generated 12 transactions                                      ← after Generate (singular "transaction" when 1)
                                                [Cancel] [Generate]
```
- Item rows inside `egui::ScrollArea::vertical().max_height(300.0)`; date preview = `generate_dates(item, start, end)` formatted `M/d`
  (`%-m/%-d`), joined with ", ", truncated to one line. Cache the previews in the form keyed on `(revision, start, end)`.
- Generate ⇒ `generate_projection(engine, ProjectionRequest { target: if new { New(name) } else { Existing(id) }, start, end, items })`;
  Ok(result) ⇒ `generated = Some((result.count, now))`, toast too; the dialog closes automatically 1.5 s later
  (`ui.ctx().request_repaint_after(…)`). Err ⇒ RED error.
- After a projection into a new scenario, the scenario appears in the scenario menu (it is live data; nothing extra to do).

## Steps

1. Unit tests for `build_rows`, `BudgetItemForm::name_label/to_input`, `ProjectionForm::generate_disabled` first.
2. Budget screen with the table and menu.
3. Editor dialog, then projection dialog.
4. kittest tests in `tests/ui_budget.rs`.

## Tests (required)

Unit:

| Test name | Covers | What it checks |
|---|---|---|
| `r_bud_1_rows_by_tab_sorted_live_only` | R-BUD-1 | three types split into tabs; sorted by name; deleted absent |
| `budget_frequency_label_specific_months_count` | – | SpecificMonths with [1,4,7] ⇒ "Specific Months (3)"; Monthly ⇒ "Monthly" |
| `budget_form_name_label_per_type` | – | Payee / Name (autofilled from Category) / Source |
| `budget_form_to_input_rejects_bad_amount` | – | amount_text "abc" ⇒ Err; "1,500" ⇒ 1500 |
| `budget_form_end_date_only_when_ticked` | R-BUD-2 | has_end_date false ⇒ `end_date: None` |
| `r_prj_7_generate_disabled_rules` | R-PRJ-7 | no items; new with blank name; existing with no target ⇒ disabled; valid ⇒ enabled |
| `r_prj_8_projection_form_prefills_last` | R-PRJ-8 | settings present ⇒ target/start/end prefilled |

kittest (`tests/ui_budget.rs`):

| Test name | Covers | What it checks |
|---|---|---|
| `ui_bud_01_create_specific_months_item_appears_in_tab` | UI-BUD-01 | Budget → "＋ Add" → type Payee "Gym", Amount "40", Frequency "Specific Months", tick "Jan","Apr","Jul" → Save ⇒ row "Gym" with "Specific Months (3)" in Bills |
| `ui_bud_01_expense_category_autofills_name` | – | Expense tab, Add, pick category "Groceries" ⇒ name field contains "Groceries" |
| `ui_bud_02_projection_generates_and_reports_count` | UI-BUD-02, R-PRJ-6 | monthly bill on Jan 31; "Project" → New Scenario "Plan" → From Jan 1 Until Apr 30 → Generate ⇒ "Generated 4 transactions" visible; engine has 4 Future txs in "Plan" dated Jan 31, Feb 28, Mar 31, Apr 30 |
| `ui_projection_all_none_toggle` | – | click "None" in Bills ⇒ those unchecked and Generate disabled when nothing left |
| `ui_budget_delete_via_context_menu` | – | select, right-click "Delete" ⇒ row gone; undo name "Delete Budget Item" |
| `ui_budget_double_click_opens_editor` | – | double-click row (kittest: two clicks, or `click_button` twice; check API) ⇒ "Edit Bill" heading |
| `ui_budget_reproject_section_only_after_projection` | R-PRJ-9 | before any projection, "Reproject" absent; after one, present; clicking adds rows and shows the toast |

**Accessible labels:** tab buttons ("Bills", "Expenses", "Incomes"), "Project", "＋ Add", form fields via `labeled_text`
("Payee"/"Source"/"Name (autofilled from Category)", "Amount", "Notes", "Scenario Name"), ComboBoxes via `from_label`
("Type", "Category", "Account", "Frequency", "Scenario"), checkboxes by month name and "End Date", "New Scenario",
section toggles "All"/"None" (if several sections show "None", use `query_all_by_label` and pick by order).

## Pitfalls

- Tab labels are Swift's `rawValue + "s"`: **"Incomes"**, not "Income". Section headings in the projection dialog are "Bills", "Expenses", "Income".
- `BudgetItemType` needs `Default` (Bill) for `State::default()`; if pf-model lacks it, pick `Bill` explicitly in a manual `Default` impl.
- The date preview calls the projector per item: cache it, never inside a per-frame loop over thousands of items.
- Auto-close: store the time of generation and close when `now - t >= 1.5`; request a repaint so it actually closes.
- Specific Months ⇒ `input.specific_months` sorted ascending (the engine also normalises).

## Done when

```bash
cargo test -p pf-ui budget
cargo test -p pf-ui --test ui_budget
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules
cargo run -p pf    # manual: add/edit each type, specific months, end date, project into a new scenario, reproject
```

## Out of scope

- Budget reports (T23), budget pacing chart (T24a).
- "Add to Budget" from the transaction menu (T15c).
