# T06: CSV import core (parse, detect, map, plan)

**Phase:** 1 · **Deps:** T01, T04 · **Crate(s):** `pf-core` (plus `fixtures/csv/`) · **Size:** large (≈600 lines of code + tests)

## Goal

All the *logic* of bank and Monarch CSV import, as pure functions: parse the CSV the way Swift
does, auto-detect columns, apply saved column assignments, find unmapped values, turn the user's
translation choices into new categories and mappings, and compute an **import plan** (rows to
insert, contexts to create, skip counts). Nothing is written here. The engine applies the plan
as one undo step in T10c, the CSV wizard UI (T22) drives it, and the HTTP API (T12) reuses it.

## Read first

- Docs: `docs/business-rules.md` §11 (R-CSV-1 … R-CSV-16), `docs/data-model.md` (`ImportMapping`), `docs/testing.md` (L1)
- Swift (read fully; this task ports them):
  - `../PFApp/Sources/PFApp/Services/CSVImportService.swift`: `CSVColumnMapping` (`assignments`, `applyAssignments`), `parseCSV`, `splitCSVLine`, `detectColumns`, `lookupMapping`, `collectUnmappedValues`, `importTransactions`
  - `../PFApp/Sources/PFApp/Views/CSVImportFlowView.swift`: `savedColumnAssignments`, the `onImport` closure (saving column, negate and translation mappings), `performImport`, the summary texts
  - `../PFApp/Sources/PFApp/Views/ColumnMappingView.swift`: the required-field validation message, and the `onAppear` that applies saved assignments
  - `../PFApp/Sources/PFApp/Views/TranslationMappingView.swift`: `CategoryMode`, the `onAppear` defaults, `createNewCategoriesAndImport`, `buildMappings`
  - `../PFApp/Sources/PFApp/Services/APIHandlers.swift`: `handleImportCSV` (the headless path)
- Code: T01 `pf-model` (`Db`, `ImportMapping`, `TestDb`); T04 `pf-core::categorize` (you do **not** auto-categorize here; Swift doesn't during CSV import).

## Deliverables

```text
crates/pf-core/src/lib.rs                    add `pub mod csv_import;`
crates/pf-core/src/csv_import/mod.rs         re-exports
crates/pf-core/src/csv_import/parse.rs       split_csv_line, parse_csv
crates/pf-core/src/csv_import/columns.rs     ImportField, ColumnMapping, detect_columns, saved assignments
crates/pf-core/src/csv_import/lookup.rs      lookup_mapping
crates/pf-core/src/csv_import/unmapped.rs    UnmappedValues, collect_unmapped
crates/pf-core/src/csv_import/translate.rs   choices -> MappingPlan
crates/pf-core/src/csv_import/plan.rs        ImportRequest, ImportPlan, plan_import, value parsers
fixtures/csv/bank_basic.csv                  (content below)
fixtures/csv/monarch_basic.csv               (content below)
```

```rust
// parse.rs: R-CSV-1
pub struct ParsedCsv { pub headers: Vec<String>, pub rows: Vec<Vec<String>> }
pub fn split_csv_line(line: &str) -> Vec<String>;
pub fn parse_csv(text: &str) -> ParsedCsv;

// columns.rs: R-CSV-2, R-CSV-3, R-CSV-15
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportField { Date, Payee, Amount, Category, Memo, Account, Tags, OriginalStatement, Skip }
impl ImportField {
    /// Picker order, exactly Swift's `allFields` (no OriginalStatement!):
    pub const PICKER: [ImportField; 8] = [Date, Payee, Amount, Category, Memo, Account, Tags, Skip];
    pub fn as_str(self) -> &'static str;   // "Date" … "Tags", "Original Statement", "(Skip)"
    pub fn parse(s: &str) -> Option<ImportField>;
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ColumnMapping {
    pub date: Option<usize>, pub payee: Option<usize>, pub amount: Option<usize>,
    pub category: Option<usize>, pub memo: Option<usize>, pub account: Option<usize>,
    pub tags: Option<usize>, pub original_statement: Option<usize>,
    pub negate_amounts: bool,
    pub extra_memo: Vec<usize>, pub extra_payee: Vec<usize>,
}
impl ColumnMapping {
    pub fn all_payee_columns(&self) -> Vec<usize>;          // primary + extras, sorted
    pub fn all_memo_columns(&self) -> Vec<usize>;
    pub fn is_monarch(&self) -> bool;                       // account.is_some()
    /// One entry per header: (index, header, field). Swift quirk: OriginalStatement shows as Skip.
    pub fn assignments(&self, headers: &[String]) -> Vec<(usize, String, ImportField)>;
    /// Resets every column (NOT negate_amounts), then applies; second Payee/Memo become extras.
    pub fn apply_assignments(&mut self, assignments: &[(usize, ImportField)]);
}
pub fn detect_columns(headers: &[String]) -> ColumnMapping;
/// "Required columns not mapped: Date, Payee. Every import needs at least Date, Payee, and Amount."
pub fn validate_required(assignments: &[(usize, String, ImportField)]) -> Result<(), String>;
/// Saved "column" mappings for this source (Monarch: account None; bank: that account), matched
/// against headers by exact equality, in mapping id order.
pub fn saved_column_assignments(db: &Db, headers: &[String], account: Option<AccountId>, is_monarch: bool)
    -> Vec<(usize, ImportField)>;
/// Saved negate flag: mapping {type "setting", source "negateAmounts", account}.
pub fn saved_negate(db: &Db, account: AccountId) -> Option<bool>;

// lookup.rs: R-CSV-4
pub fn lookup_mapping<'a>(mappings: &'a [ImportMapping], kind: &str, source: &str,
                          account: Option<AccountId>) -> Option<&'a str>;

// unmapped.rs: R-CSV-5
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnmappedValues { pub accounts: Vec<String>, pub categories: Vec<String>, pub payees: Vec<String> }
pub fn collect_unmapped(db: &Db, rows: &[Vec<String>], mapping: &ColumnMapping,
                        account: Option<AccountId>, is_monarch: bool) -> UnmappedValues;

// translate.rs: R-CSV-15, R-CSV-16
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CategoryChoice { Unmapped, MapToExisting(String /* full name */), CreateNew { name: String, parent: Option<String> } }
#[derive(Debug, Clone, Default)]
pub struct TranslationChoices {
    pub accounts: Vec<(String, String)>,            // source -> existing account name ("" = skip)
    pub categories: Vec<(String, CategoryChoice)>,
    pub payees: Vec<(String, String)>,              // source -> cleaned payee
}
impl TranslationChoices {
    /// Swift `onAppear` defaults: payee -> itself, category -> CreateNew(source, None), accounts -> skip.
    pub fn defaults(unmapped: &UnmappedValues) -> TranslationChoices;
}
#[derive(Debug, Clone, PartialEq)]
pub struct NewCategory { pub name: String, pub parent_id: Option<CategoryId> }   // type Expense, no icon
#[derive(Debug, Clone, PartialEq)]
pub struct NewMapping { pub kind: String, pub source: String, pub target: String,
                        pub account_id: Option<AccountId>, pub negate: bool }
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MappingPlan {
    pub new_categories: Vec<NewCategory>,
    pub new_mappings: Vec<NewMapping>,           // translation + "column" + "setting" rows
    pub remove_mapping_ids: Vec<ImportMappingId>, // old "column" (and "setting") rows being replaced
}
pub fn plan_mappings(db: &Db, headers: &[String], mapping: &ColumnMapping, choices: &TranslationChoices,
                     target_account: Option<AccountId>, is_monarch: bool) -> MappingPlan;

// plan.rs: R-CSV-6 … R-CSV-14
pub struct ImportRequest<'a> {
    pub rows: &'a [Vec<String>],
    pub mapping: &'a ColumnMapping,
    pub target_account: Option<AccountId>,   // None for Monarch
    pub allow_unmapped_payees: bool,         // UI bank: true; UI Monarch: false; API: true
    pub tz: &'a jiff::tz::TimeZone,          // for ISO-8601 timestamps in the date column
}
#[derive(Debug, Clone, PartialEq)]
pub enum ContextRef { Existing(ContextId), New(usize) /* index into ImportPlan::new_contexts */ }
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedTransaction {
    pub date: Date, pub amount: Decimal, pub payee: String, pub memo: Option<String>,
    pub account_id: AccountId, pub category_id: Option<CategoryId>, pub contexts: Vec<ContextRef>,
    // status is always Cleared and scenario always None (R-CSV-14); the engine sets them.
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImportResult { pub imported: usize, pub skipped_duplicates: usize, pub skipped_unmapped: usize }
impl ImportResult {
    /// Swift summary lines, omitting zero counts:
    /// "✅ 3 transactions imported" / "✅ 1 transaction imported",
    /// "⏭️ 2 duplicates skipped" / "⏭️ 1 duplicate skipped", "⚠️ 4 skipped (unmapped account)".
    pub fn summary_lines(&self) -> Vec<String>;
}
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportPlan { pub transactions: Vec<PlannedTransaction>, pub new_contexts: Vec<String>, pub result: ImportResult }
pub fn parse_csv_date(s: &str, tz: &jiff::tz::TimeZone) -> Option<Date>;   // R-CSV-6
pub fn parse_csv_amount(s: &str) -> Option<Decimal>;                          // R-CSV-7 (before negation)
pub fn plan_import(db: &Db, req: &ImportRequest) -> ImportPlan;
```

**How the engine (T10c) will use this, so design for it:** inside one `Engine::apply("Import CSV", |w| …)`
it (1) applies a `MappingPlan` (inserts the new categories first, then the mappings, replacing a
`NewCategory` target with its full name), then (2) calls `plan_import(w.db(), …)` on the updated
data, (3) inserts `new_contexts`, then every `PlannedTransaction`. That is why `plan_import` reads
mappings from `db.import_mappings` only, and why new categories are resolved by name.

## Steps

1. Create the two CSV fixtures (content below).
2. **parse.rs**, R-CSV-1. Port `splitCSVLine` character by character: a `"` toggles `in_quotes` and is
   dropped; a `,` outside quotes ends the field; every field is `trim()`med. `parse_csv`: split the text into lines
   (`str::lines()`, which also strips `\r`), drop lines that are empty after trimming, use the first as headers, and
   keep every other line (`split_csv_line` always returns ≥ 1 field). Do **not** use the `csv` crate:
   its quoting rules differ, and we must match Swift exactly.
3. **columns.rs**:
   - `detect_columns`: lower-case and trim each header, then for each header run the **if/else-if chain** from
     R-CSV-2 in that order, setting the field only if it is still `None` (`mapping.date = mapping.date.or(Some(i))`).
   - `assignments`: a header's field is chosen in this order: Date, Amount, Category, Account, Tags (exact
     index match); else Payee if the index is in `all_payee_columns`; else Memo if in `all_memo_columns`; else **Skip**.
     `original_statement` therefore displays as Skip (Swift quirk: it is not in the picker). Keep it.
   - `apply_assignments`: reset all column fields and the extras (keep `negate_amounts`), then for each
     `(i, field)`: Date/Amount/Category/Account/Tags/OriginalStatement set that column (later wins);
     Payee/Memo: the first one goes to the primary slot, later ones are pushed to the extras; Skip does nothing.
   - `validate_required`: list the missing fields among Date, Payee, Amount, in that order, using the exact message above.
   - `saved_column_assignments` / `saved_negate`, per the signatures.
4. **lookup.rs**, R-CSV-4: iterate `mappings` in order; match when `m.mapping_type == kind`,
   `m.source_value.to_lowercase() == source.trim().to_lowercase()` and `m.account_id == account`.
   Return the first `target_value`.
5. **unmapped.rs**, R-CSV-5: port `collectUnmappedValues` exactly. Use live accounts (name, lower-cased)
   and live categories (**full name** only, lower-cased). The payee column is the Monarch original-statement
   column (falling back to the payee column), or for banks the **primary** payee column only. Return each list
   deduplicated and sorted ascending.
6. **translate.rs**, R-CSV-15/16. Port `createNewCategoriesAndImport` + `buildMappings` + the save logic in
   `CSVImportFlowView.onImport`:
   - Category `CreateNew{name, parent}`: trim the name; skip it if empty. If a live category matches
     (full name or name, case-insensitive), treat it as `MapToExisting(that.full_name)`. Otherwise plan a `NewCategory`
     whose parent is the live **top-level** category whose name matches `parent` case-insensitively (None if not found),
     and map the source to the new full name (`"{parent}/{name}"` or `name`). If two sources create the same new name,
     plan it only once (fix: Swift created duplicates).
   - Mappings: accounts with a non-empty target ⇒ `{kind "account", account_id None}`; categories
     `MapToExisting(t)` with non-empty t ⇒ `{kind "category", account_id: target_account}`; payees whose target is
     non-empty and ≠ source ⇒ `{kind "payee", account_id: target_account}`.
   - Bank only (`target_account` is Some): replace the `setting`/`negateAmounts` row for the account
     (`target_value` = `"true"`/`"false"`, `negate` = the flag).
   - Column rows: remove the existing `"column"` mappings for this source (Monarch: account None; bank: that account),
     and add one `{kind "column", source: header, target: field.as_str()}` per header whose `assignments()` field is not Skip.
   - Emit `new_mappings` in a stable order: accounts, categories, payees (each in `choices` order), then setting, then columns (header order).
7. **plan.rs**, R-CSV-6 … R-CSV-14. Port `importTransactions` row by row, in Swift's order of checks:
   date → amount → negate → account → payee-mapping requirement → payee value → duplicate → category → memo → tags.
   - `parse_csv_date`: trim; try `M/D/YYYY` (1–2 digit month/day, exactly 4-digit year, valid date), then
     `YYYY-MM-DD` (`jiff::civil::Date` parse), then an ISO-8601 timestamp (`jiff::Timestamp` parse ⇒ `.to_zoned(tz).date()`).
   - `parse_csv_amount`: remove every `$` and `,`, trim, then `Decimal::from_str`.
   - Rows with no parseable date or amount (or a column index beyond the row) are skipped **without** being counted.
   - Account (Monarch): raw value of the account column ⇒ `lookup_mapping("account", raw, None)` or raw ⇒ live account
     with that name (case-insensitive) ⇒ else `skipped_unmapped += 1`. Bank: `target_account`, which must be a live account, else skipped_unmapped.
   - Payee parts: the non-empty trimmed values of `all_payee_columns()`, joined with `" | "`.
   - Payee-mapping requirement (when `!allow_unmapped_payees`): the source is the Monarch original-statement column
     (or the payee column, or column 0) **raw cell** (use `""` if the index is out of range; Swift would crash), or for banks the joined payee;
     looked up with `account = req.target_account`. No mapping ⇒ skipped_unmapped.
   - Payee value: Monarch with an OS column ⇒ the OS mapping (account None) or the joined payee; otherwise
     `lookup_mapping("payee", joined, target_account)` or the joined payee.
   - Duplicate (R-CSV-11): any **live** transaction already in `db` with the same account, same date, equal amount, and
     payee equal case-insensitively ⇒ `skipped_duplicates += 1`. Do **not** check rows planned earlier in this batch (Swift doesn't).
   - Category: the raw cell, if non-empty ⇒ `lookup_mapping("category", raw, if monarch {None} else {target_account})` or raw ⇒ the first live
     category (name order, then id) whose full name or name equals it case-insensitively.
   - Memo: the non-empty trimmed values of `all_memo_columns()` joined with `" | "`, or None.
   - Tags: split the cell on `,`, trim, drop empties; match a live context by name case-insensitively ⇒ `Existing`; else
     reuse or append in `new_contexts` (dedupe case-insensitively within the plan: fix, Swift created one per row) ⇒ `New(idx)`.
8. Write all tests (table), then make them pass.

### fixtures/csv/bank_basic.csv

```csv
Transaction Date,Description,Amount,Type,Balance
03/01/2026,"STARBUCKS STORE 123, SEATTLE",-6.75,DEBIT,1000.00
03/02/2026,PAYROLL EMPLOYER INC,"4,000.00",CREDIT,5000.00
2026-03-03,Whole Foods,$-142.30,DEBIT,4857.70
13/45/2026,Bad Date,-1.00,DEBIT,0
03/04/2026,Bad Amount,abc,DEBIT,0

3/5/2026,  City Power  ,-120.45,DEBIT,4737.25
```

Expected: headers = 5; rows = 6 (the blank line is dropped). `detect_columns` ⇒ date 0, payee 1, amount 2, nothing else.
Row 1 payee is `STARBUCKS STORE 123, SEATTLE` (the comma is inside quotes, and the quotes are removed). Planned
against an empty account: 4 transactions; `Bad Date` and `Bad Amount` are silently skipped (counted nowhere).

### fixtures/csv/monarch_basic.csv

```csv
Date,Merchant,Category,Account,Original Statement,Notes,Amount,Tags
2026-03-01,Starbucks,Coffee Shops,Chase Checking,STARBUCKS STORE 123,,-6.75,"vacation,work"
2026-03-02,Employer,Paychecks,Chase Checking,PAYROLL EMPLOYER INC,March pay,4000.00,
2026-03-03,Whole Foods,Groceries,Amex Gold,WHOLEFDS MKT 10,,-142.30,
2026-03-04,Unknown,,Mystery Bank,SOMETHING,,-1.00,
```

Expected `detect_columns`: date 0, payee 1, category 2, account 3, original_statement 4, memo 5, amount 6, tags 7.

## Tests (required)

Build databases with `TestDb` (T01). If `TestDb` lacks a helper for categories with parents, contexts or
import mappings, add one to `pf-model`'s `TestDb` (an additive change; note it in STATUS.md).
Read fixtures with `concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/csv/…")`. Use `America/Chicago` for `tz`.

| Test name | Covers | What it checks |
|---|---|---|
| `r_csv_1_split_respects_quotes` | R-CSV-1 | `a,"b,c",d` ⇒ `[a, "b,c", d]`; quotes removed; fields trimmed; `""` ⇒ `[""]`; `a,,b` ⇒ `[a, "", b]` |
| `r_csv_1_escaped_quotes_not_supported` | R-CSV-1 | `"say ""hi""",x` ⇒ `["say hi", "x"]` (each `"` toggles; Swift parity) |
| `r_csv_1_parse_drops_blank_lines` | R-CSV-1 | `bank_basic.csv` ⇒ 5 headers, 6 rows; CRLF input gives the same result |
| `r_csv_1_split_property` | R-CSV-1 | proptest: never panics; for input without `"`, equals `line.split(',').map(str::trim)` |
| `r_csv_2_detect_bank_headers` | R-CSV-2 | `bank_basic.csv` mapping as stated above |
| `r_csv_2_detect_monarch_headers` | R-CSV-2 | `monarch_basic.csv` mapping as stated above |
| `r_csv_2_first_match_wins_and_chain_order` | R-CSV-2 | `["Name","Payee"]` ⇒ payee 0 (first match wins); `["Original Amount"]` ⇒ amount None (needs an exact `amount`), original_statement 0 (contains `original`); `["Transaction Date","Date"]` ⇒ date 0 |
| `r_csv_3_multiple_payee_and_memo_columns_join` | R-CSV-3 | assigning Payee to columns 1 and 3 and Memo to 4 and 5 ⇒ extras set; planned payee `"A | B"`; empty parts omitted |
| `r_csv_3_assignments_round_trip` | R-CSV-3 | `apply_assignments(assignments(h))` reproduces the mapping except `original_statement` (shown as Skip: quirk) |
| `r_csv_3_apply_resets_but_keeps_negate` | R-CSV-3 | apply on a mapping with negate = true keeps negate, clears the old columns |
| `r_csv_3_validate_required_message` | R-CSV-3 | missing Date+Amount ⇒ `"Required columns not mapped: Date, Amount. Every import needs at least Date, Payee, and Amount."` |
| `r_csv_4_lookup_case_and_account_scope` | R-CSV-4 | source `"  Starbucks "` matches mapping source `STARBUCKS`; the account must match exactly (a global mapping is not found for Some(acct)); the first match wins |
| `r_csv_5_unmapped_monarch` | R-CSV-5 | Db with account "Checking", global mapping `account: chase checking → Checking`, category "Groceries" ⇒ accounts `["Amex Gold", "Mystery Bank"]`, categories `["Coffee Shops", "Paychecks"]`, payees = the 4 original statements, sorted |
| `r_csv_5_unmapped_uses_full_name_only` | R-CSV-5 | category "Housing/Utilities" exists; CSV value "Utilities" is reported as unmapped (Swift quirk) |
| `r_csv_5_unmapped_bank_uses_primary_payee_only` | R-CSV-5 | with extra payee columns, only the primary column's values are listed |
| `r_csv_6_date_formats` | R-CSV-6 | `03/01/2026`, `3/5/2026`, `2026-03-03`, `2026-03-13T04:59:06Z` (⇒ 2026-03-12 in Chicago) parse; `13/45/2026`, `03/01/26`, `""` ⇒ None |
| `r_csv_6_bad_rows_skipped_uncounted` | R-CSV-6 | `bank_basic.csv` ⇒ imported 4, both skip counts 0 |
| `r_csv_7_amount_cleanup_and_negate` | R-CSV-7 | `"$-142.30"` ⇒ −142.30; `"4,000.00"` ⇒ 4000; with `negate_amounts` the signs flip |
| `r_csv_8_monarch_account_resolution` | R-CSV-8 | Chase Checking resolves via the mapping; `Mystery Bank` ⇒ skipped_unmapped; `amex gold` matches account "Amex Gold" case-insensitively |
| `r_csv_8_bank_requires_target_account` | R-CSV-8 | target None ⇒ every row skipped_unmapped; a deleted target account ⇒ skipped_unmapped |
| `r_csv_9_unmapped_payees_skipped_unless_allowed` | R-CSV-9 | Monarch, allow = false: only rows whose payee source has a mapping are imported; allow = true: all |
| `r_csv_10_payee_value_monarch_and_bank` | R-CSV-10 | Monarch with OS mapping `STARBUCKS STORE 123 → Starbucks Coffee` ⇒ payee `Starbucks Coffee`; without one ⇒ the merchant; bank with mapping (account-scoped) ⇒ the mapped value |
| `r_csv_11_duplicates_live_only_not_in_batch` | R-CSV-11 | an existing live tx (same account/day/amount, payee differs only in case) ⇒ skipped_duplicates 1; a soft-deleted twin does not block (fix); two identical CSV rows are both imported |
| `r_csv_12_category_mapping_and_matching` | R-CSV-12 | mapping `Coffee Shops → Dining` ⇒ Dining's id; raw `housing/utilities` matches the full name; raw `utilities` matches the name; scope: a bank mapping for another account is ignored |
| `r_csv_13_tags_become_contexts` | R-CSV-13 | `"vacation,work"` with existing context `Vacation` ⇒ `[Existing(id), New(0)]`, `new_contexts == ["work"]`; a second row with `WORK` reuses `New(0)` |
| `r_csv_14_plan_counts_and_defaults` | R-CSV-14 | Monarch fixture; Db has accounts Checking and Amex Gold, a global mapping `chase checking → Checking`, and payee mappings for all four original statements; allow_unmapped_payees = false ⇒ result `{imported 3, skipped_duplicates 0, skipped_unmapped 1}`; `summary_lines()` exact strings, including the singular/plural forms |
| `r_csv_15_plan_mappings_columns_and_negate` | R-CSV-15 | bank: old column rows listed in `remove_mapping_ids`; new column rows for Date/Payee/Amount only (Type and Balance are Skip); setting row `negateAmounts` = `"true"`; `saved_column_assignments` on the resulting mappings reproduces the assignments |
| `r_csv_15_saved_assignments_exact_header_match` | R-CSV-15 | saved source `Description` matches header `Description` but not `description`; Monarch uses account None |
| `r_csv_16_translation_defaults` | R-CSV-16 | `defaults()` ⇒ payees map to themselves (no mapping planned), categories CreateNew(source, None), accounts skip |
| `r_csv_16_create_new_category_with_parent` | R-CSV-16 | CreateNew{`Coffee`, parent `dining`} with top-level `Dining` ⇒ NewCategory(Coffee, Dining id) + mapping target `Dining/Coffee`; an existing name ⇒ no new category, mapped to the existing full name; the same new name twice ⇒ one NewCategory |
| `r_csv_16_build_mappings_filters` | R-CSV-16 | empty account target ⇒ none; payee target == source ⇒ none; category Unmapped ⇒ none; scopes: account None, category/payee `target_account` |

## Pitfalls

- **Original Statement disappears in the UI flow (Swift quirk, keep).** `assignments()` shows it as Skip, so after the
  column-mapping step `apply_assignments` drops it, and Monarch payee mapping then falls back to the merchant. `plan_import`
  must still honour `original_statement` when it is set (detect-only paths such as tests and the API keep it).
- **Order of checks matters for the counts.** A row with an unknown account counts as skipped_unmapped even if it would
  also be a duplicate.
- `str::lines()` already handles `\r\n`. Do not split on `,` before handling quotes.
- `to_lowercase()` everywhere Swift uses `lowercased()`; do not use ASCII-only lowering.
- Never mutate `db` here; all functions take `&Db`. Ids for new rows do not exist yet, which is why `ContextRef::New` and `NewCategory` exist.
- Do not auto-categorize imported rows (Swift does not). Status is always Cleared, and the scenario is always None.
- The `"setting"` mapping kind (`negateAmounts`) is not listed in `docs/data-model.md`, but it exists in Swift; support it.

## Done when

```bash
cd PFRust
cargo test -p pf-core csv_import
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules          # (after T00b) R-CSV-1 … R-CSV-16 covered
```

## Out of scope

- Applying plans, undo, and persistence (T10c); BEH-CSV-01/02 are T10c's behaviour tests and build on this.
- The wizard UI, file dialogs, creating accounts or parent categories from the "New…" buttons (T22 calls ordinary engine commands for those).
- The HTTP endpoint (T12).
