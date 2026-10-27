# T10c: Commands: budget items, projection, reproject, add-to-budget, CSV import, Markdown import, backup

**Phase:** 2 · **Deps:** T09, T02, T05, T06 · **Crate(s):** `pf-engine` · **Size:** large (about 500 lines of code plus 550 lines of tests)

## Goal

The remaining user actions: budget CRUD and projection into scenarios, applying CSV imports
(with their saved mappings), replacing all data from a Markdown backup, and Cmd-S backups.
This task also owns the undo property test (BEH-UND-01), which exercises commands from T10a–c together.

## Read first

- `tasks/T09-engine-core.md` (Engine, Writer, World, `set_setting_direct`, world fixture facts)
- `docs/business-rules.md`: §8 (R-PRJ-*, R-BUD-*), R-TXN-11, §11 (R-CSV-*), §12 (R-MD-5, R-MD-7)
- `docs/markdown-format.md` (import entry point and tz handling)
- `docs/testing.md`: BEH-BUD-*, BEH-CSV-*, BEH-MD-03, BEH-BAK-01, BEH-UND-01
- Swift:
  - `Views/BudgetView.swift`, `Views/BudgetItemEditorView.swift` (`init` defaults, `save`, the "Reproject to Scenario" section, `reprojectItem`)
  - `Views/BudgetProjectionView.swift` (`generateProjection`, `prefillFromLastProjection`)
  - `Views/TransactionListView.swift` `addToBudget`
  - `Views/CSVImportFlowView.swift` (the `onContinue` closure that saves "setting"/"column"/translation mappings, `savedColumnAssignments`, `performImport`)
  - `Services/CSVImportService.swift`, `Services/APIHandlers.swift` `handleImportCSV`
  - `Views/SettingsView.swift` (`importMarkdown`, the Danger Zone wipe), `Services/BackupManager.swift`
- pf-core from T02 (`generate_dates`), T05 (Markdown import/export), T06 (CSV parse/detect/plan)

## Deliverables

`crates/pf-engine/src/commands/{budget,import,backup}.rs` and tests
`crates/pf-engine/tests/behaviour_{budget,csv,backup,undo_property}.rs`.

### budget.rs

```rust
pub struct BudgetItemInput {
    pub item_type: BudgetItemType, pub name: String, pub amount: Decimal,
    pub start_date: Date, pub end_date: Option<Date>, pub frequency: BudgetFrequency,
    pub specific_months: Vec<u32>, pub notes: String,
    pub category: Option<CategoryId>, pub account: Option<AccountId>,
}
/// Editor defaults (Swift init): start = Jan 1 of today's year; end = today + 1 year;
/// frequency Monthly; type = the tab's type.
pub fn default_budget_input(item_type: BudgetItemType, today: Date) -> BudgetItemInput;
/// Same normalisation as Swift `save()`: name trimmed (empty ⇒ Validation("Name is required"));
/// specific_months kept (sorted, deduped) only when frequency == SpecificMonths, else empty;
/// notes: whitespace-only ⇒ None, otherwise kept as typed (Swift does not trim kept notes);
/// end_date < start_date ⇒ Validation("End date cannot be before start date") (R-BUD-2).
/// Undo names: "Add Budget Item" / "Edit Budget Item".
pub fn save_budget_item(e: &mut Engine, id: Option<BudgetItemId>, input: BudgetItemInput) -> Result<BudgetItemId>;
/// "Delete Budget Item" / "Delete Budget Items" (soft delete).
pub fn delete_budget_items(e: &mut Engine, ids: &[BudgetItemId]) -> Result<()>;
/// R-BUD-1: live items of a type, sorted by name.
pub fn budget_items_by_type(db: &Db, t: BudgetItemType) -> Vec<BudgetItemId>;

/// R-TXN-11. "Add to Budget". Returns the toast text `Added "{payee}" to Bills`.
pub fn add_to_budget(e: &mut Engine, tx: TransactionId) -> Result<String>;

pub enum ProjectionTarget { Existing(ScenarioId), New(String) }
pub struct ProjectionRequest { pub target: ProjectionTarget, pub start: Date, pub end: Date, pub items: Vec<BudgetItemId> }
pub struct ProjectionResult { pub scenario: ScenarioId, pub count: usize }
/// R-PRJ-6/7/8. One step "Generate Projection" that (a) creates the scenario if New (name trimmed;
/// empty ⇒ Validation("Scenario name is required")), (b) inserts one Future transaction per
/// generated date per item (amount: −|amt| for Bill/Expense, +|amt| for Income; payee: name for
/// Bill/Income, "Expense: {name}" for Expense; memo "Auto-generated from Budget"; account and
/// category from the item; scenario = target), (c) sets settings lastProjectionScenario (name),
/// lastProjectionStartDate, lastProjectionEndDate (YYYY-MM-DD) through the Writer.
/// Unknown/deleted item ids are skipped. Existing target that is deleted ⇒ NotFound.
pub fn generate_projection(e: &mut Engine, req: ProjectionRequest) -> Result<ProjectionResult>;
/// R-PRJ-8 prefill: (scenario id if a live scenario has that exact name, start, end).
pub fn last_projection(db: &Db) -> (Option<ScenarioId>, Option<Date>, Option<Date>);

/// R-PRJ-9. Port of `BudgetItemEditorView.reprojectItem`, which does exactly this:
///   1. Saves the editor's pending input into the item (same normalisation as save_budget_item).
///   2. Generates dates for the item over [lastProjectionStartDate, lastProjectionEndDate].
///   3. Inserts one Future transaction per date into the scenario named lastProjectionScenario,
///      built exactly as in generate_projection.
///   It does NOT delete or match existing rows (Swift has no match criteria: it only adds).
/// One step "Reproject". Returns the toast `Added {n} transaction{s} to {scenario}`
/// ("transaction" when n == 1, "transactions" otherwise).
/// Precondition (the UI hides the button otherwise): the item exists AND all three
/// lastProjection* settings exist AND a live scenario has that name. Otherwise
/// Validation("Run a projection first").
pub fn reproject_item(e: &mut Engine, id: BudgetItemId, input: BudgetItemInput) -> Result<String>;
```

> **Resolved:** R-PRJ-9 and BEH-BUD-03 now say "additive", matching Swift. Do not remove existing rows.
> (If replacement is ever wanted, the match criteria would be: same scenario, memo "Auto-generated from Budget",
> same account, category and generated payee, date in range, status Future.)

### import.rs

T06 provides the pure CSV logic. This module turns T06's result into one undoable step.
**Adapt the names below to T06's actual API**; the shapes matter, not the spelling.

```rust
// Expected from pf-core::csv_import (T06):
//   parse_csv(&str) -> (Vec<String> /*headers*/, Vec<Vec<String>> /*rows*/)
//   detect_columns(&[String]) -> ColumnMapping
//   ColumnMapping::apply_assignments(&mut self, &[(usize, String)])
//   plan_import(db: &Db, rows, &ColumnMapping, mappings: &[ImportMapping], target: Option<AccountId>,
//               allow_unmapped_payees: bool) -> ImportPlan
//   ImportPlan { transactions: Vec<PlannedTx>, new_context_names: Vec<String>,
//                skipped_duplicates: usize, skipped_unmapped: usize }
//   PlannedTx  { date, amount, payee, memo, account: AccountId, category: Option<CategoryId>,
//                contexts: Vec<ContextRef> }   // ContextRef::Existing(ContextId) | ContextRef::New(name)

pub struct ImportResult { pub imported: usize, pub skipped_duplicates: usize, pub skipped_unmapped: usize }

/// Everything the CSV wizard (T22) collected, applied as ONE step "Import CSV" (R-CSV-14, R-CSV-15, R-CSV-16):
pub struct CsvImportRequest {
    pub is_monarch: bool,
    pub target_account: Option<AccountId>,          // bank mode: required (else Validation("Choose an account"))
    pub rows: Vec<Vec<String>>,
    pub mapping: ColumnMapping,                      // final column assignments + negate flag
    pub new_accounts: Vec<AccountInput>,             // accounts the wizard asked to create (Monarch "New…")
    /// From `pf_core::csv_import::plan_mappings` (T06). It already contains the translation rows
    /// (account/category/payee), the rebuilt "column" rows and the bank "setting"/negateAmounts row,
    /// the old rows to remove, and the categories to create. T10c does NOT rebuild any of these itself.
    pub mapping_plan: MappingPlan,
}
/// Order inside the single step, mirroring Swift `onContinue` + `performImport`:
///  1. create `new_accounts` (mapping targets refer to accounts by name, so order vs. the plan is safe);
///  2. insert `mapping_plan.new_categories` (type Expense, parent_id as given);
///  3. hard-delete `mapping_plan.remove_mapping_ids`, then insert every `mapping_plan.new_mappings` row;
///  4. plan_import over the updated Db (`w.db()`) with allow_unmapped_payees = !is_monarch (Swift performImport);
///  5. create contexts for ContextRef::New (deduped, case-insensitive) and insert the planned transactions
///     (status Cleared, reality).
pub fn run_csv_import(e: &mut Engine, req: CsvImportRequest) -> Result<ImportResult>;

// Saved column assignments are NOT defined here: use `pf_core::csv_import::saved_column_assignments`
// (T06, signature `(db, headers, account, is_monarch) -> Vec<(usize, ImportField)>`).

/// Headless import used by the API (R-API-8) and CLI. Port of APIHandlers.handleImportCSV:
///  * no "column" mappings for the account ⇒ Validation(exact):
///    "No saved column mappings for account '{name}'. Please import via the UI first to set up column mappings."
///  * empty CSV ⇒ Validation("CSV is empty or could not be parsed")
///  * detect_columns, then apply saved assignments matched by header.trim() == source_value
///  * plan with allow_unmapped_payees = true; the saved negate setting is NOT applied (Swift quirk; keep it)
///  * one step "Import CSV"
pub fn import_csv_with_saved_mappings(e: &mut Engine, account: AccountId, csv: &str) -> Result<ImportResult>;
```

### backup.rs

```rust
/// Markdown export of the current Db (pf-core T05, TimeZone::system()).
pub fn export_markdown(e: &Engine) -> String;
/// R-MD-5. Builds a fresh Db by importing `text` into an EMPTY Db (pf-core T05, TimeZone::system(),
/// forwarding progress strings), then `e.replace_db(db)`: wipes old data, clears undo/redo,
/// active scenario None. Returns per-entity counts for the UI message.
pub fn import_markdown_replacing(e: &mut Engine, text: &str, progress: &mut dyn FnMut(&str)) -> Result<ImportCounts>;
pub struct ImportCounts { pub accounts: usize, pub transactions: usize, pub categories: usize,
    pub contexts: usize, pub scenarios: usize, pub obligations: usize, pub budget_items: usize, pub import_mappings: usize }
/// Danger Zone "Delete All Data": replace_db(Db::default()). Not undoable (clears history).
pub fn wipe_all_data(e: &mut Engine) -> Result<()>;

/// R-MD-7 file name: "PFBackup_yyyy-MM-dd_HH-mm-ss.md" from local wall-clock time, e.g.
/// PFBackup_2026-03-15_09-05-07.md. jiff: now.strftime("PFBackup_%Y-%m-%d_%H-%M-%S.md").
pub fn backup_filename(now: &jiff::Zoned) -> String;
/// Writes export_markdown(e) to folder/backup_filename(now) atomically (write "<name>.tmp", then rename).
/// Errors ⇒ Store("Failed to create backup: {io error}"). Returns the full path for the toast
/// `Backup saved to: {path}`.
pub fn perform_backup(e: &Engine, folder: &Path, now: &jiff::Zoned) -> Result<PathBuf>;
/// Setting "backupFolder" via e.set_setting_direct (not undoable). None ⇒ the UI must prompt first.
pub fn backup_folder(e: &Engine) -> Option<PathBuf>;
pub fn set_backup_folder(e: &mut Engine, folder: &Path) -> Result<()>;
```

## Steps

1. Write the behaviour tests (tables below). CSV inputs are inline string constants in the test file.
2. Implement budget.rs (pure helpers + commands), then import.rs, then backup.rs.
3. Write the undo property test last; it uses commands from T10a and T10b too.

## Tests (required)

| Test name | Covers | What it checks |
|---|---|---|
| `r_prj_6_amount_and_payee_by_type` | R-PRJ-6 | Bill −, Expense − with "Expense: " prefix, Income +; memo, status Future, scenario set |
| `r_prj_7_new_scenario_created_in_same_step` | R-PRJ-7 | New("  Q2 Plan ") ⇒ scenario "Q2 Plan"; one undo removes scenario AND rows |
| `r_prj_7_empty_new_name_rejected` | R-PRJ-7 | "Scenario name is required" |
| `r_prj_8_settings_saved_and_prefilled` | R-PRJ-8 | `last_projection` returns what was generated |
| `r_prj_9_reproject_adds_rows_and_saves_edits` | R-PRJ-9 | editing amount to 1600 then reproject ⇒ item amount 1600, new rows −1600, old rows kept |
| `r_prj_9_reproject_without_prior_projection_rejected` | R-PRJ-9 | "Run a projection first" |
| `r_prj_9_toast_singular_plural` | R-PRJ-9 | 1 ⇒ "Added 1 transaction to Plan A"; 2 ⇒ "transactions" |
| `r_bud_1_items_by_type_sorted` | R-BUD-1 | Bills of world: Car Insurance, Landlord, Property Tax |
| `r_bud_2_end_before_start_rejected` | R-BUD-2 | exact message |
| `budget_save_normalises_months_and_notes` | – | Monthly + months [4,1] ⇒ months []; SpecificMonths [10,4,4] ⇒ [4,10]; notes "  " ⇒ None |
| `default_budget_input_values` | – | start 2026-01-01, end 2027-03-15 |
| `r_txn_11_add_to_budget` | R-TXN-11 | Starbucks ⇒ Bill "Starbucks" −5.75, monthly, start 2026-01-20, Dining, Checking; toast text |
| `r_csv_14_import_is_one_undo_step` | R-CSV-14 | after run_csv_import, undo ⇒ no new tx, no new mappings, no new contexts |
| `r_csv_15_column_mappings_saved_and_reused` | R-CSV-15 | saved_column_assignments returns the wizard's choices for the same headers; "(Skip)" not saved |
| `r_csv_15_negate_setting_replaced_not_duplicated` | R-CSV-15 | importing twice leaves exactly one "setting"/negateAmounts mapping for the account |
| `r_api_8_no_column_mappings_message` | R-API-8 | exact message from import_csv_with_saved_mappings |
| `r_md_5_import_replaces_and_clears_history` | R-MD-5 | old account gone, fixture accounts present, `can_undo()` false |
| `r_md_7_backup_filename_format` | R-MD-7 | 2026-03-15 09:05:07 New York ⇒ `PFBackup_2026-03-15_09-05-07.md` |
| `wipe_all_data_empties_db` | – | all maps empty; can_undo false |

Behaviour:

| Test name | ID | Given / When / Then |
|---|---|---|
| `beh_bud_01_projection_exact_dates` | BEH-BUD-01, R-PRJ-3, R-PRJ-4 | **Given** world · **When** generate New("Next Year") from 2026-03-15 to 2027-03-14 with all 5 items · **Then** count 42: Landlord 12 (2026-04-01…2027-03-01), Acme Payroll 12 (2026-03-15…2027-02-15, +4000), Groceries 12 (2026-03-31, 04-30, 05-31, 06-30, 07-31, 08-31, 09-30, 10-31, 11-30, 12-31, 2027-01-31, 2027-02-28; payee "Expense: Groceries"), Car Insurance 4 (05-15, 08-15, 11-15, 2027-02-15), Property Tax 2 (04-30, 10-30) · reality Checking balance still 2051.95 |
| `beh_bud_02_projection_settings_prefill` | BEH-BUD-02 | after BEH-BUD-01's generate, `last_projection` == (Some(Next Year), 2026-03-15, 2027-03-14); still true after reload |
| `beh_bud_03_reproject_one_item_others_untouched` | BEH-BUD-03 | **Given** BEH-BUD-01 state · **When** reproject Landlord with amount 1550 · **Then** 12 new Landlord rows at −1550 exist alongside the 12 at −1500 (additive, per R-PRJ-9); rows of other items unchanged; one undo removes only the 12 new rows and restores amount 1500 |
| `beh_bud_04_add_to_budget_from_transaction` | BEH-BUD-04 | Hotel Maui ⇒ Bill "Hotel Maui", amount −650, start 2026-02-14, category None, account Visa; appears in `budget_items_by_type(Bill)` |
| `beh_csv_01_bank_import_then_duplicate_skip_then_undo` | BEH-CSV-01, R-CSV-11 | **Given** world and CSV `Date,Description,Amount` / `03/01/2026,Corner Deli,-12.00` / `03/02/2026,"ACME, INC PAYROLL",4000.00` · **When** run_csv_import (bank, Checking, detected mapping) · **Then** imported 2, payee `ACME, INC PAYROLL` · **When** import_csv_with_saved_mappings with the same text · **Then** imported 0, skipped_duplicates 2, and NO undo step was added (nothing changed) · **When** undo once · **Then** live tx count back to 14 and `can_undo()` is false |
| `beh_csv_02_monarch_mappings_tags_unmapped_payees` | BEH-CSV-02, R-CSV-8, R-CSV-9, R-CSV-13 | **Given** world and CSV `Date,Merchant,Category,Account,Original Statement,Notes,Amount,Tags` / `2026-03-01,Blue Bottle,Coffee Shops,Chase Checking,BLUE BOTTLE #123,,-4.50,"Vacation,Work"` / `2026-03-02,Unknown Shop,Shopping,Chase Checking,UNKNOWN 999,,-20.00,` with a `mapping_plan` from `plan_mappings` whose choices map (global) account `Chase Checking`→Checking, category `Coffee Shops`→Dining, payee `BLUE BOTTLE #123`→Blue Bottle · **When** run_csv_import (Monarch) · **Then** imported 1 (payee "Blue Bottle", Dining, Checking, contexts [Vacation, Work] with "Work" auto-created), skipped_unmapped 1 |
| `beh_md_03_settings_import_wipes_and_clears_undo` | BEH-MD-03, R-MD-5 | **Given** World::new with an account "Old" created via command · **When** import_markdown_replacing(fixtures/world.md) · **Then** "Old" is gone, 5 accounts, 14 tx, `can_undo()` false; reload ⇒ same |
| `beh_bak_01_backup_writes_export` | BEH-BAK-01, R-MD-7 | **Given** world and a tempdir set as backup folder · **When** perform_backup(now = 2026-03-15T09:05:07 New York) · **Then** file `PFBackup_2026-03-15_09-05-07.md` exists and its content == export_markdown(e); no `.tmp` file left |
| `beh_und_01_random_edits_undo_all_redo_all` | BEH-UND-01, R-UND-2 | proptest (64 cases, 20 ops each) over world: ops chosen from {add_withdrawal on a random account, edit payee, change_status, delete random tx, duplicate, create context, delete context, save category, set_starting_balance, generate_projection on 1 item, toggle archive}; ops returning Err are skipped (they make no step). Record `db0`, apply ops, record `db1` and the number of steps `n` (`undo` stack growth). Undo n times ⇒ `db == db0`; redo n times ⇒ `db == db1`. Compare Dbs ignoring `next_id`. Also check after `reload()` that `db1` is what is on disk. |

The ImportMapping source values in BEH-CSV-02 are lowercase because lookups lowercase the
source (R-CSV-4), and Swift stores `sourceValue` lowercased for matching (see ImportMapping.swift comment).

## Pitfalls

- `generate_projection` can insert thousands of rows; it must be **one** `apply` (T09 guarantees bulk speed).
- In `run_csv_import`, the plan must be computed **after** steps 1–4 so that new accounts, categories and
  mappings are visible. Compute it inside the apply closure from `w.db()`.
- Hard-deleting mappings inside a step is still undoable (`Writer::delete_hard` keeps the before-image).
- `import_markdown_replacing` must not partially replace: build the whole new Db first, then call
  `replace_db`. If the import errors, the old data stays.
- Backups: never overwrite an existing file. If the name exists (two backups within a second), append `-2`.
- proptest + temp SQLite: use `World::new()`-style temp dirs per case and keep cases at 64 so the
  suite stays under about 10 s in debug.

## Done when

```bash
cargo test -p pf-engine
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules      # R-PRJ-6..9, R-BUD-*, R-TXN-11, R-CSV-14/15, R-MD-5/7, BEH-BUD/CSV/MD-03/BAK/UND-01 present
```

## Out of scope

- The CSV wizard UI (T22), the Settings UI (T21), the projection dialog UI (T17).
- The pure CSV logic itself (T06) and the Markdown format (T05): only call them.
