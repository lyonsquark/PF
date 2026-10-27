# T22: CSV import wizard (column mapping → translation mapping → summary)

**Phase:** 3 · **Deps:** T15a, T10c · **Crate(s):** `pf-ui` · **Size:** large (≈550 lines + 250 lines tests)

## Goal

Port the multi-step CSV import used in two places. **Bank mode** imports into one account, started from the
Account detail screen. **Monarch mode** resolves the account per row, started from Settings ▸ "Import from
Monarch CSV". The wizard shows parsed columns with samples and lets the user assign fields, then asks how to
map unrecognised accounts, categories and payees, then imports through one engine command and shows a summary.

> **Shell contract (T13).** The wizard is a new `Dialog::CsvWizard(Box<CsvWizard>)` variant with an arm in `dialogs::show_active`;
> the module exposes `pub fn show(cx: &mut UiCtx, ui: &mut egui::Ui, w: &mut CsvWizard) -> Outcome`. Files use
> `cx.files.pick_file("Import CSV", &["csv", "txt"])`; tests use `FakeDialogs` via `PfApp::new_for_test_with_files`. Alerts use
> `cx.dialogs.alert(...)`. Lists use `cx.caches.lookups(cx.engine)` (`accounts`, `categories`).
> Core (T06, `pf_core::csv_import`): `parse_csv(text) -> ParsedCsv { headers, rows }`, `detect_columns(&headers) -> ColumnMapping`,
> `ImportField::{PICKER, as_str, parse}`, `ColumnMapping::{assignments, apply_assignments}`, `validate_required(&assignments)`,
> `saved_column_assignments(db, &headers, account, is_monarch)`, `saved_negate(db, account)`,
> `collect_unmapped(db, &rows, &mapping, account, is_monarch) -> UnmappedValues`, `TranslationChoices::defaults(&unmapped)`,
> `CategoryChoice::{Unmapped, MapToExisting(full), CreateNew { name, parent }}`, `ImportResult::summary_lines()`.
> Engine (T10c, `pf_engine::commands::import`): `run_csv_import(e, CsvImportRequest) -> ImportResult` (one step "Import CSV");
> account creation uses `commands::accounts::create_account(e, AccountInput)` (T10a); a parent category uses
> `commands::categories::save_category(e, None, CategoryInput)` (T10b).
> Fallback: if a name here differs from the code, use the actual name in `pf-core`/`pf-engine`/`pf-ui`. Never re-implement parsing,
> detection or import logic in `pf-ui`; mark the task `blocked` instead.

## Read first

- Swift: `../PFApp/Sources/PFApp/Views/CSVImportFlowView.swift`, `ColumnMappingView.swift`, `TranslationMappingView.swift`
  (all whole files), `../PFApp/Sources/PFApp/Services/CSVImportService.swift` (`CSVColumnMapping.assignments` / `applyAssignments`)
- Rules: R-CSV-1 … R-CSV-16
- Tasks: `tasks/T06-csv-import.md` (core types above), `tasks/T10c-commands-budget-import-backup.md` (`CsvImportRequest`, `run_csv_import`),
  `tasks/T15a-account-detail-table.md` (Account detail toolbar), `tasks/T21-settings-ui.md` (Monarch entry point)

## Deliverables

```rust
// crates/pf-ui/src/dialogs/csv_wizard.rs
pub enum WizardStep { ColumnMapping, Translation, Summary(ImportResult) }   // ImportResult from T06/T10c
pub enum SubDialog { NewAccount(NewAccountForm), NewParentCategory(String) }
pub struct CsvWizard {
    pub is_monarch: bool,
    pub target: Option<AccountId>,                        // Some in bank mode
    pub parsed: ParsedCsv,                                // T06 parse_csv, done once in `open`
    pub mapping: ColumnMapping,                           // T06 detect_columns + saved assignments
    pub assignments: Vec<(usize, String, ImportField)>,   // ColumnMapping::assignments(&headers)
    pub step: WizardStep,
    pub unmapped: UnmappedValues,
    pub choices: TranslationChoices,                      // T06; initialised with TranslationChoices::defaults
    pub sub_dialog: Option<SubDialog>,
}
/// Entry point used by Account detail (bank) and Settings (Monarch): `cx.dialogs.open(Dialog::CsvWizard(Box::new(…)))`.
/// Replaces T21's stub if one exists.
pub fn open(cx: &mut UiCtx, csv_text: &str, is_monarch: bool, target: Option<AccountId>);
pub fn show(cx: &mut UiCtx, ui: &mut egui::Ui, w: &mut CsvWizard) -> Outcome;
```

Also add an **"Import CSV…"** button to the Account detail toolbar if T15a did not:
`cx.files.pick_file("Import CSV", &["csv", "txt"])`, read it (error ⇒ `cx.dialogs.alert("Import CSV", "Failed to load CSV: {err}")`),
then `csv_wizard::open(cx, &text, false, Some(account_id))`.

## Step 1: column mapping ("Map CSV Columns")

```
┌ Map CSV Columns ──────────────────────────────────────────────┐
│ Verify how each CSV column maps to a transaction field.        │
│ ───────────────────────────────────────────────────────────── │
│ CSV Column          Maps To              Sample Data           │
│ Date                [Date        ▾]      02/01/2026            │
│ Description         [Payee       ▾]      KROGER #123           │
│ Amount              [Amount      ▾]      -45.10                │
│ Memo                [(Skip)      ▾]                            │
│ ───────────────────────────────────────────────────────────── │
│ [ ] Negate amounts (flip +/− signs)                            │
│ ───────────────────────────────────────────────────────────── │
│ [Cancel]                                          [Continue]   │
└───────────────────────────────────────────────────────────────┘
```

* On open: `parse_csv(text)` (R-CSV-1), `detect_columns(&headers)` (R-CSV-2), then
  `mapping.apply_assignments(&saved_column_assignments(db, &headers, target, is_monarch))` (R-CSV-15). Then build the rows with
  `mapping.assignments(&headers)` (one per header; `ImportField::Skip` when unassigned).
* Bank mode only: preload `mapping.negate` (the ColumnMapping negate flag; check the field name in T06) from
  `saved_negate(db, account)` (the "setting" ImportMapping, documented in data-model.md).
* Each row: header (180 px, one line), a ComboBox (id salt = `("csvcol", i)`) over `ImportField::PICKER`
  (labels via `as_str()`: Date, Payee, Amount, Category, Memo, Account, Tags, (Skip)), and the sample value from the **first**
  data row (weak text) when that column exists. Rows sit in a `ScrollArea` with max height 300.
* **Continue**: `validate_required(&assignments)`; on `Err(msg)` show `cx.dialogs.alert("Missing Required Columns", msg)` (T06 builds
  `Required columns not mapped: … Every import needs at least Date, Payee, and Amount.`). The wizard must not be lost while the alert
  shows, so render the error as a red label inside the wizard (title "Missing Required Columns") rather than replacing the dialog.
  Otherwise `mapping.apply_assignments(…)` (R-CSV-3), `collect_unmapped(db, &rows, &mapping, target, is_monarch)` (R-CSV-5),
  `choices = TranslationChoices::defaults(&unmapped)`, and go to step 2.
* **Cancel** / Esc closes the wizard.

## Step 2: translation mapping ("Map Unrecognized Values")

```
┌ Map Unrecognized Values ─────────────────────────────────────────────────────┐
│ The following values from your CSV weren't recognized. Map them to existing  │
│ items or create new ones.                                                    │
│ ──────────────────────────────────────────────────────────────────────────── │
│ 🏦 Accounts                                                  (Monarch only)   │
│  Chase Checking ••1234   →  [Skip (don't import) ▾]  [New…]                  │
│ 🏷 Categories                                   [Create Parent Category…]     │
│  Restaurants & Bars                                                           │
│   (•) Unmapped   ( ) Existing: [Select…   ▾]   ( ) New: [Restaurants…] [None▾] ⚠│
│ 👤 Payees                                                                      │
│  SQ *BLUE BOTTLE 1234     →  [SQ *BLUE BOTTLE 1234_____]                      │
│ ──────────────────────────────────────────────────────────────────────────── │
│ [Cancel]                                                          [Import]   │
└──────────────────────────────────────────────────────────────────────────────┘
```

* The Accounts section appears only in Monarch mode with ≥1 unmapped account. The Categories and Payees sections appear only
  when non-empty. If all three are empty, show "All values are already mapped! Ready to import."
* **Accounts row:** source (200 px) → a ComboBox with "Skip (don't import)" (value "") plus live accounts by name → a
  **New…** button that opens the *Create New Account* sub-dialog prefilled with the source as the name.
* **Create New Account** sub-dialog: fields Account Name, Institution, Type (AccountType::ALL, capitalised), Currency
  (default "USD"), "External Account" checkbox; Cancel / **Create** (disabled when the name is empty). Create calls
  `commands::accounts::create_account(cx.engine, AccountInput { … })` right away (its own undo step) and sets the matching
  `choices.accounts` entry to the new name.
* **Categories row:** three radio options, laid out horizontally:
  `Unmapped`; `Existing:` + a ComboBox ("Select…" plus all live categories by full name, case-insensitive sort; choosing one
  selects this option); `New:` + a name field (default = source) + a parent ComboBox ("None" plus top-level categories by name).
  Show "⚠" with hover text `This category already exists — it will be mapped to the existing one.` when the new name
  (trimmed, lowercased) equals an existing category's name under the same parent (or both top level).
* **Create Parent Category…** opens a sub-dialog: title "Create Parent Category", text `Create a top-level category to use
  as a parent when mapping CSV categories below.`, field "Category Name", a red warning
  `A category named "{name}" already exists.` when a category with that name exists (case-insensitive); Cancel /
  **Create** (disabled when empty or duplicate). Create calls `commands::categories::save_category(cx.engine, None, CategoryInput { name, category_type: Expense, parent: None, .. })` right away.
* **Payees row:** source (250 px, hover = full text) → a text field (default = source).
* **Import:** build a `CsvImportRequest` (T10c: `is_monarch, target_account, rows, mapping, new_accounts, mapping_plan`)
  and call `run_csv_import(cx.engine, req)` **once**. Set `mapping_plan = plan_mappings(db, &headers, &mapping, &choices, target, is_monarch)`
  (T06) unchanged. `new_accounts` stays empty, because the sub-dialog already
  created them. The engine creates the new categories
  (R-CSV-16), saves the translation mappings, replaces this source's "column" mappings with the current assignments
  (skipping "(Skip)"), saves the negate setting (bank mode), and imports with `allow_unmapped_payees = !is_monarch`
  (Swift `CSVImportFlowView.performImport`). It is one undo step "Import CSV" (R-CSV-14). Then go to step 3.
  `choices` rules (applied by plan_mappings): accounts with target ≠ ""; `CategoryChoice::MapToExisting(full)` (non-empty),
  `CreateNew { name, parent }` (trimmed non-empty), `Unmapped` ⇒ skip; payees with target ≠ source and target ≠ "".
  Then `step = Summary(result)`; the summary lines come from `result.summary_lines()`.
* **Cancel** closes; nothing is imported (accounts and parents already created by the sub-dialogs stay).

## Step 3: summary ("Import Complete")

```
        ✔ (green, large)
     Import Complete
 ✅ 12 transactions imported
 ⏭️ 3 duplicates skipped
 ⚠️ 2 skipped (unmapped account)
          [Done]
```
Each line appears only when its count > 0. Singular forms: "1 transaction imported", "1 duplicate skipped".
**Done** closes (Enter triggers it as well).

## Steps

1. Write the tests with small CSV strings embedded in the test file: write each to a `tempfile` path,
   `FakeDialogs::push_answer(Some(path))`, then click "Import CSV…" on the account screen (bank) or "Import from Monarch CSV" in
   Settings (Monarch), using `PfApp::new_for_test_with_files(engine, fake)` and T13's `tests/common::harness`.
2. Implement the wizard state and step 1; then step 2 and the sub-dialogs; then step 3.
3. Wire the entry points: the Account detail "Import CSV…" button and the Settings Monarch button (replace T21's stub).
4. Run Done-when.

## Tests (required)

`crates/pf-ui/tests/ui_csv_wizard.rs` (assert on the engine via `app.engine().lock()` after `drop(harness)`).

| Test name | Covers | What it checks |
|---|---|---|
| `ui_csv_01_bank_wizard_end_to_end` | UI-CSV-01, R-CSV-14 | Bank CSV with Date/Description/Amount ⇒ Continue ⇒ Import ⇒ "Import Complete" shows "✅ 2 transactions imported"; the engine has 2 new rows in the account; one Cmd-Z removes both |
| `ui_csv_missing_required_alert` | R-CSV-3 | Set the Amount column to "(Skip)" ⇒ Continue shows "Missing Required Columns" listing "Amount" |
| `ui_csv_saved_column_mapping_restored` | R-CSV-15 | After one import with a manual mapping, reopening with the same headers preselects the same fields |
| `ui_csv_negate_toggle_flips_sign` | R-CSV-7 | With negate on, an amount of "45.10" imports as -45.10 |
| `ui_csv_duplicates_summary` | R-CSV-11 | Importing the same CSV twice ⇒ the second summary shows "⏭️ 2 duplicates skipped" and no "imported" line |
| `ui_csv_monarch_account_section` | R-CSV-5, R-CSV-8 | Monarch CSV with an unknown account shows the Accounts section; leaving "Skip" ⇒ the summary shows "skipped (unmapped account)" |
| `ui_csv_new_account_subdialog` | R-CSV-16 | "New…" ⇒ Create ⇒ the account exists and its row's ComboBox shows it; the import then succeeds |
| `ui_csv_category_create_new_default` | R-CSV-16 | An unknown category defaults to "New:" and the import creates it and assigns it to the rows |
| `ui_csv_category_duplicate_warning` | — | Typing an existing category name in "New:" shows the ⚠ hover text |
| `ui_csv_payee_rename_creates_mapping` | R-CSV-10 | Renaming a payee in step 2 stores a "payee" mapping and the imported row uses the new payee |
| `ui_csv_all_mapped_message` | — | When nothing is unmapped, step 2 shows "All values are already mapped! Ready to import." |

## Pitfalls

- **Resolved:** `CsvImportRequest.mapping_plan` takes T06's `MappingPlan` as-is (translation, column and setting rows plus
  removals and new categories). Do not add or rebuild mapping rows in the UI.
- In Swift the bank wizard **allows** unmapped payees and Monarch **requires** them (`allowUnmappedPayees: !isMonarch`).
  The summary line "skipped (unmapped account)" is also shown for payee skips (Swift wording). Keep the text.
- ComboBoxes inside loops need unique id salts: `("csvcol", i)`, `("csvacct", source)`, and so on.
- Do not run CSV parsing every frame. Parse once in `open`, and keep the rows in the wizard state.
- Swift clears the undo stack after an import. We make it one undoable step instead (R-CSV-14 fix).

## Done when

```bash
cargo test -p pf-ui --test ui_csv_wizard
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules
```
Manual: import `../PFApp/HSATransactionsAsOf_03022026.csv` into a test account.

## Out of scope

- CSV parsing and matching logic (T06), the engine command (T10c), the headless API import (T12).
