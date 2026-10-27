# T05: Markdown backup import / export

**Phase:** 1 · **Deps:** T01 · **Crate(s):** `pf-core` (plus `fixtures/`) · **Size:** large (≈700 lines of code + tests)

## Goal

After this task PFRust can read every Markdown backup the Swift app has produced and write
backups in the exact Swift format. This is the **migration bridge**: users move their data from
the Swift app to PFRust through this code, so it is golden-tested against real fixture files.
The task also creates the shared test fixtures (`fixtures/small.md` and friends) that later
tasks load with `World::with_fixture("small")`.

## Read first

- Docs: `docs/markdown-format.md` (all of it: it is the spec), `docs/data-model.md` (entities, enums, settings keys), `docs/testing.md` (L1, golden tests)
- Swift: `../PFApp/Sources/PFApp/Services/MarkdownDataService.swift` (whole file: `export()`, `import(from:)`, every `import*` helper, `esc`/`unesc`, `stringToDate`)
- Swift test: `../PFApp/Tests/PFAppTests/MarkdownDataServiceTests.swift`
- Rules: R-MD-1, R-MD-2, R-MD-3, R-MD-4, R-MD-6; behaviour IDs BEH-MD-01, BEH-MD-02 (`docs/testing.md`)
- Code: `crates/pf-model/src/` from T01: build the result with `Db::new()`, `db.alloc_id()`, the entity `::new(...)` constructors and `db.put(Record::…)`; settings via `db.settings`. Parse BudgetItem type/frequency with T01's `parse_lenient`. Do not add a second way to insert rows.

## Deliverables

```text
fixtures/SampleData.md              (copied, byte-identical)
fixtures/SampleData2.md             (copied)
fixtures/SampleData3.md             (copied)
fixtures/LargeSampleData.md         (copied)
fixtures/PersonalFinance_Backup.md  (copied)
fixtures/small.md                   (new, content below)
fixtures/expected_counts.json       (new, content below)
crates/pf-core/src/lib.rs           (add `pub mod markdown;`)
crates/pf-core/src/markdown/mod.rs      re-exports + MarkdownImport
crates/pf-core/src/markdown/escape.rs   esc / unesc
crates/pf-core/src/markdown/values.rs   decimal / bool / date encode + decode
crates/pf-core/src/markdown/export.rs   export_markdown
crates/pf-core/src/markdown/import.rs   import_markdown
crates/pf-core/src/markdown/snapshots/  (insta snapshots, committed)
crates/pf-core/Cargo.toml           (add `serde_json = { workspace = true }` to [dev-dependencies])
```

Public API (exact names; later tasks call these):

```rust
// crates/pf-core/src/markdown/mod.rs
use jiff::{Timestamp, tz::TimeZone};
use pf_model::Db;

/// Result of parsing a backup. The Swift importer never fails; it skips bad rows.
#[derive(Debug, Clone)]
pub struct MarkdownImport {
    pub db: Db,               // a brand-new Db holding only the imported rows
    pub skipped_rows: usize,  // data rows that were dropped (too few columns, bad enum/number/date)
}

/// R-MD-1: write `db` (live rows only) in the exact Swift backup format.
/// Calendar dates are written as local midnight in `tz`, converted to UTC.
pub fn export_markdown(db: &Db, tz: &TimeZone) -> String;

/// R-MD-2: parse a backup into a fresh Db. `now` fills missing obligation timestamps.
/// `progress` receives "Importing {section}...", "Importing Transactions... {n}/{total}"
/// (every 50 transaction rows) and finally "Finalizing...".
pub fn import_markdown(text: &str, tz: &TimeZone, now: Timestamp,
                       progress: &mut dyn FnMut(&str)) -> MarkdownImport;

// crates/pf-core/src/markdown/escape.rs
pub fn esc(value: &str) -> String;     // "|" -> "｜" (U+FF5C)   R-MD-4
pub fn unesc(value: &str) -> String;   // "｜" -> "|"

// crates/pf-core/src/markdown/values.rs
pub fn fmt_decimal(d: Decimal) -> String;                     // Swift Decimal.description: d.normalize().to_string()
pub fn parse_decimal(s: &str) -> Option<Decimal>;             // strict; "" -> None
pub fn swift_bool(s: &str) -> bool;                            // NSString.boolValue
pub fn fmt_date(d: Date, tz: &TimeZone) -> String;            // local midnight -> "2026-02-01T06:00:00Z"
pub fn fmt_timestamp(t: Timestamp) -> String;                 // whole seconds, "Z", no fraction
pub fn parse_date(s: &str, tz: &TimeZone) -> Option<Date>;    // ISO timestamp -> local date; or "YYYY-MM-DD"
pub fn parse_timestamp(s: &str, tz: &TimeZone) -> Option<Timestamp>; // ISO; or "YYYY-MM-DD" = local midnight
```

There is deliberately no "import into an existing Db". The Swift app always wipes and then imports
(`SettingsView.importMarkdown`), and its name caches start empty, so lookups only ever see rows
created by the same import. The engine (T10c) replaces its `Db` with `MarkdownImport.db`.

## Steps

1. **Copy fixtures.** From the repo root (`..` relative to `PFRust/`), copy byte-identical:
   ```bash
   cd PFRust
   cp ../SampleData.md ../SampleData2.md ../SampleData3.md ../LargeSampleData.md ../PersonalFinance_Backup.md fixtures/
   ```
   Never edit these copies; they are real Swift-era files with format variants (listed under Pitfalls).
2. **Create `fixtures/small.md`** with exactly the content in the section "fixtures/small.md" below.
   It is the hand-made fixture every later behaviour test uses.
3. **Create `fixtures/expected_counts.json`** with exactly the content in the section below.
4. **escape.rs**: `esc`, `unesc` + tests.
5. **values.rs**: write the tests first (table below), then implement:
   - `fmt_decimal`: `d.normalize().to_string()` (gives `12.5`, `-1200`, `0`, never an exponent).
   - `parse_decimal`: trim; `""` ⇒ None; `Decimal::from_str(s).ok()` (accepts `12.50`, `-5`, `+5`).
   - `swift_bool`: trim; then true iff the first char is one of `t T y Y 1-9`; an optional leading `+`/`-` followed by zeros then a non-zero digit is also true (`NSString.boolValue`). `"true"`, `"YES"`, `"1"` ⇒ true; `"false"`, `""`, `"0"`, `"no"` ⇒ false.
   - `fmt_date`: `d.to_zoned(tz.clone())` (handles DST via the compatible strategy) `.timestamp()` then `fmt_timestamp`.
   - `fmt_timestamp`: `t.strftime("%Y-%m-%dT%H:%M:%SZ")` on the UTC timestamp (drops sub-seconds, which Swift's ISO8601 formatter never writes).
   - `parse_date`: `""` ⇒ None. Try `s.parse::<Timestamp>()` ⇒ `.to_zoned(tz.clone()).date()`; else try `s.parse::<jiff::civil::Date>()` (`YYYY-MM-DD`); else None.
   - `parse_timestamp`: Timestamp parse; else a civil date at local midnight in `tz`; else None.
6. **export.rs**: build the string exactly as Swift's `export()` does. Follow `docs/markdown-format.md` → "Layout" and "Sort orders". Exact byte layout:
   - Start with `"# Personal Finance Data\n\n"`.
   - Contexts / Categories / Scenarios / Accounts / Obligations / BudgetItems: `"## {Name}\n"`, header line, `"| --- | … |\n"` separator, one row per live entity, then `"\n"`.
   - Row format: `"| " + cells.join(" | ") + " |\n"`, so an empty cell renders as `|  |` (two spaces).
   - Strings pass through `esc`. Enum cells use `as_str()` (category type lowercase; obligation status, budget type and budget frequency capitalised, e.g. `Every Other Month`). Booleans are `true`/`false`.
   - The Categories "Parent" cell is the parent's **name** (not its full name); "Match Patterns" is the patterns joined with `;`.
   - BudgetItems always use the **new** 10-column layout (with End Date). Specific months are joined with `,`.
   - **ImportMappings:** `"## ImportMappings\n"`. Then, if any global mappings (`account_id == None`) exist, `"### Monarch (Global)\n"` + header + separator + rows (id order) + `"\n"`. Then for each account name sorted ascending (look the name up including deleted accounts), `"### {name}\n"` + header + separator + rows (id order) + `"\n"`. With no mappings at all, the next line is immediately `"## Transactions\n"`: there is **no** blank line (Swift quirk).
   - **Transactions:** `"## Transactions\n"`. Group live transactions by account name (account looked up including deleted; `None` ⇒ `"Uncategorized"`). Groups are sorted by name; within a group, rows are sorted by date descending, then id descending. Each group: `"### {name}\n"` + header + separator + rows + `"\n"`. Cells: date, payee, category **name** (not full name), amount, memo, scenario name, context names joined with `,`, status, attachment.
   - **Settings:** only if setting `lastProjectionScenario` is non-empty **or** `backupFolder` is non-empty: `"## Settings\n| Key | Value |\n| --- | --- |\n"`, then the rows that are set, in this order: `lastProjectionScenario`, `lastProjectionStartDate` (`fmt_date` of the stored `YYYY-MM-DD`), `lastProjectionEndDate`, and `backupFolderBookmark` (whose value is the Rust `backupFolder` path), then `"\n"`.
   - Only live (`!is_deleted`) rows are exported, for every entity type that has the flag.
7. **import.rs**: implement `docs/markdown-format.md` → "Import parsing algorithm" literally:
   - Iterate `text.lines()`. Keep `section: Option<String>` and `subsection: Option<Sub>` (`Sub::Global` or `Sub::Account(String)`).
   - Keep four `HashMap<String, Id>` name caches (contexts, categories, scenarios, accounts) filled **only** by rows created in this import. Lookups are exact and case-sensitive; an empty name ⇒ None.
   - Split a table row into columns: `line.split('|')`, drop the first piece and the last piece, then `unesc(piece.trim())`. Do **not** filter empty pieces.
   - Implement one function per section (`import_context(cols, …)`, etc.) with the minimum column counts and per-field rules from the spec. Every rejected data row increments `skipped_rows`.
   - Contexts in a transaction row: `cols[6].split(',')` **without trimming**, dropping empty pieces; unknown names are ignored. Specific months: `split(',')`, dropping pieces that do not parse as an integer in `1..=12` (no trim, matching Swift's `Int("3")`).
   - Category patterns: `split(';')`, dropping empty pieces.
   - Settings: map `lastProjectionScenario` → setting `lastProjectionScenario`; `lastProjectionStartDate`/`EndDate` → `parse_date` then store `YYYY-MM-DD`; `backupFolderBookmark` → store as `backupFolder` **only** if the value starts with `/`. Ignore other keys.
   - Progress: call `progress(&format!("Importing {section}..."))` on every `## ` line, `progress(&format!("Importing Transactions... {i}/{total}"))` every 50th transaction row (pre-count data rows per section the way Swift does: lines starting `|` without `---` and without the header markers), and `progress("Finalizing...")` at the end.
8. **Golden and round-trip tests** (below). Run `cargo insta test -p pf-core` and then `cargo insta review`. Inspect **every** new snapshot by eye against `docs/markdown-format.md` before accepting it.
9. Remove the `scaffold_compiles` placeholder test from `pf-core` if it is still there.

### Test helpers (in `crates/pf-core/src/markdown/mod.rs` under `#[cfg(test)]`)

```rust
fn fixture(name: &str) -> String {
    let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/");
    std::fs::read_to_string(format!("{p}{name}")).expect("fixture exists")
}
fn chicago() -> TimeZone { TimeZone::get("America/Chicago").expect("tzdb") }
fn new_york() -> TimeZone { TimeZone::get("America/New_York").expect("tzdb") }
fn tokyo() -> TimeZone { TimeZone::get("Asia/Tokyo").expect("tzdb") }
fn now() -> Timestamp { "2026-03-15T12:00:00Z".parse().expect("ts") }
fn import(text: &str, tz: &TimeZone) -> MarkdownImport { import_markdown(text, tz, now(), &mut |_| {}) }
```

The fixtures' timestamps (`…T06:00:00Z`) were written in **America/Chicago**, so golden tests use `chicago()`.

### fixtures/small.md

Create the file with exactly this content (the final newline included). It uses date-only values,
so it imports the same in every time zone. It deliberately contains the Swift quirks:
the `| column | Date | Date | false |` mapping row is **dropped** by the header-row check
(it contains `| Date |`); `A｜B Hardware` unescapes to `A|B Hardware`; the "Old Bank" group
yields an orphan transaction (no such account); Brokerage is external.

````markdown
# Personal Finance Data

## Contexts
| Name | Color | Notes |
| --- | --- | --- |
| home-repair | #FFA500 | Fixes around the house |
| vacation | #008080 | Hawaii 2026 |

## Categories
| Name | Type | Icon | Notes | Parent | Match Patterns |
| --- | --- | --- | --- | --- | --- |
| Credit Card Payment | transfer | 🏦 | Paying off cards |  |  |
| Dining | expense | 🍔 |  |  | ^star.*;bistro |
| Groceries | expense | 🛒 |  |  | whole foods;kroger |
| Housing | expense | 🏠 | Mortgage and repairs |  | mortgage |
| Income | income | 💵 |  |  | employer;payroll |
| Utilities | expense | ⚡️ |  | Housing | power;water |

## Scenarios
| Name | Color | Notes |
| --- | --- | --- |
| Plan A | #00FF00 | Projected budget |

## Accounts
| Name | Type | Institution | Currency | External |
| --- | --- | --- | --- | --- |
| Brokerage | investment | Fidelity | USD | true |
| Checking | checking | Chase | USD | false |
| Rewards Card | credit | Amex | USD | false |

## Obligations
| Name | Status | Target | Balance | Due Date | Created Date | Updated Date | Notes | Account | Context |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Roof Repair | Completed | 1200 | 1200 |  | 2025-10-01 | 2025-12-01 |  | Checking | home-repair |
| Vacation Fund | Funding | 3000 | 500 | 2026-07-01 | 2026-01-01 | 2026-02-01 | Hawaii trip | Checking | vacation |

## BudgetItems
| Name | Type | Amount | Start Date | End Date | Frequency | Specific Months | Notes | Category | Account |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Groceries | Expense | 150 | 2026-01-05 |  | Weekly |  | Weekly estimate | Groceries | Rewards Card |
| Gym | Bill | 40 | 2026-01-10 | 2026-06-30 | Monthly |  | Cancel in July | Housing | Checking |
| Insurance | Bill | 600 | 2026-02-15 |  | Quarterly |  |  | Housing | Checking |
| Mortgage | Bill | 1500 | 2026-01-31 |  | Monthly |  |  | Housing | Checking |
| Salary | Income | 4000 | 2026-01-25 |  | Monthly |  | Payroll | Income | Checking |
| Water Bill | Bill | 90 | 2026-01-17 |  | Specific Months | 1,3,5,7,9,11 | Bi-monthly | Utilities | Checking |

## ImportMappings
### Monarch (Global)
| Type | Source Value | Target Value | Negate Amounts |
| --- | --- | --- | --- |
| account | chase checking | Checking | false |

### Checking
| Type | Source Value | Target Value | Negate Amounts |
| --- | --- | --- | --- |
| column | Date | Date | false |
| column | Description | Payee | false |
| column | Amount | Amount | false |
| payee | starbucks store 123 | Starbucks | false |

## Transactions
### Brokerage
| Date | Payee | Category | Amount | Memo | Scenario | Contexts | Status | Attachment |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-01-02 | Opening deposit |  | 10000 |  |  |  | cleared |  |

### Checking
| Date | Payee | Category | Amount | Memo | Scenario | Contexts | Status | Attachment |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-04-01 | Mortgage | Housing | -1500 | Auto-generated from Budget | Plan A |  | future |  |
| 2026-03-25 | Employer Payroll | Income | 4000 | Auto-generated from Budget | Plan A |  | future |  |
| 2026-03-10 | Hawaii flights |  | -450 |  |  | vacation | cleared |  |
| 2026-03-05 | Transfer to Rewards Card | Credit Card Payment | -300 |  |  |  | cleared |  |
| 2026-03-01 | City Power | Utilities | -120.45 |  |  |  | pending |  |
| 2026-02-28 | Mortgage | Housing | -1500 |  |  |  | reconciled |  |
| 2026-02-25 | Employer Payroll | Income | 4000 |  |  |  | reconciled |  |
| 2026-02-17 | City Water | Utilities | -88.10 |  |  |  | cleared |  |
| 2026-02-03 | A｜B Hardware | Housing | -64.99 | faucet｜washer |  | home-repair | cleared | /Users/Shared/receipt.pdf |
| 2026-01-31 | Mortgage | Housing | -1500 |  |  |  | reconciled |  |
| 2026-01-25 | Employer Payroll | Income | 4000 |  |  |  | reconciled |  |
| 2026-01-24 | Starting Balance |  | 2500 | Initial account balance |  |  | cleared |  |

### Old Bank
| Date | Payee | Category | Amount | Memo | Scenario | Contexts | Status | Attachment |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2025-11-01 | Closed account fee |  | -5 |  |  |  | cleared |  |

### Rewards Card
| Date | Payee | Category | Amount | Memo | Scenario | Contexts | Status | Attachment |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-04-05 | Expense: Groceries | Groceries | -150 | Auto-generated from Budget | Plan A |  | future |  |
| 2026-03-12 | Starbucks | Dining | -6.75 |  |  |  | pending |  |
| 2026-03-05 | Transfer from Checking | Credit Card Payment | 300 |  |  |  | cleared |  |
| 2026-03-02 | Whole Foods | Groceries | -142.30 |  |  | vacation,home-repair | cleared |  |
| 2026-02-14 | Bistro Nights | Dining | -88 | Valentine's dinner |  |  | cleared |  |

## Settings
| Key | Value |
| --- | --- |
| lastProjectionScenario | Plan A |
| lastProjectionStartDate | 2026-03-15 |
| lastProjectionEndDate | 2026-12-31 |
````

Facts about `small.md` that later tasks rely on (assert them in `small_fixture_facts`):

| Fact | Value |
|---|---|
| Accounts | Brokerage (investment, external), Checking (checking), Rewards Card (credit) |
| Reality balance Checking (all statuses) | 2500 + 4000 − 1500 − 64.99 + 4000 − 88.10 − 1500 − 120.45 − 300 − 450 = **6476.46** |
| Reality cleared balance Checking (cleared + reconciled) | 6476.46 + 120.45 = **6596.91** |
| Plan A balance Checking (reality + Plan A rows) | 6476.46 + 4000 − 1500 = **8976.46** |
| Reality balance Rewards Card | −88 − 142.30 + 300 − 6.75 = **62.95** (cleared **69.70**; Plan A **−87.05**) |
| Orphan transactions | 1 (`Closed account fee`, −5) |
| Uncategorized live transactions | 4 (Opening deposit, Hawaii flights, Starting Balance, Closed account fee) |
| Utilities full name | `Housing/Utilities` |
| Transaction contexts | `Hawaii flights` → [vacation]; `Whole Foods` 2026-03-02 → [vacation, home-repair] (in that order) |
| Settings | lastProjectionScenario = `Plan A`, lastProjectionStartDate = `2026-03-15`, lastProjectionEndDate = `2026-12-31` |

### fixtures/expected_counts.json

Counts were computed by emulating the Swift importer on each file. Note the two legacy files
(`SampleData.md`, `LargeSampleData.md`) use an old transaction layout
(`| Date | Amount | Payee | Memo | Account | … |` with no `###` account groups). The Swift importer
reads column 3 (a memo such as "Rent") as the amount, fails, and **skips every transaction row**.
That is the parity behaviour, so their transaction count is 0.

```json
{
  "SampleData.md":             {"contexts": 3, "categories": 8, "scenarios": 2, "accounts": 4, "obligations": 3, "budget_items": 8,  "transactions": 0,   "orphans": 0, "import_mappings": 0, "settings": 0},
  "SampleData2.md":            {"contexts": 2, "categories": 7, "scenarios": 1, "accounts": 2, "obligations": 0, "budget_items": 10, "transactions": 278, "orphans": 0, "import_mappings": 0, "settings": 0},
  "SampleData3.md":            {"contexts": 2, "categories": 7, "scenarios": 1, "accounts": 2, "obligations": 0, "budget_items": 10, "transactions": 278, "orphans": 0, "import_mappings": 0, "settings": 0},
  "LargeSampleData.md":        {"contexts": 3, "categories": 8, "scenarios": 1, "accounts": 3, "obligations": 0, "budget_items": 2,  "transactions": 0,   "orphans": 0, "import_mappings": 0, "settings": 0},
  "PersonalFinance_Backup.md": {"contexts": 2, "categories": 7, "scenarios": 1, "accounts": 2, "obligations": 0, "budget_items": 0,  "transactions": 279, "orphans": 0, "import_mappings": 0, "settings": 0},
  "small.md":                  {"contexts": 2, "categories": 6, "scenarios": 1, "accounts": 3, "obligations": 2, "budget_items": 6,  "transactions": 19,  "orphans": 1, "import_mappings": 4, "settings": 3}
}
```

How to sanity-check a count by hand: data rows are lines starting with `| ` under a section,
minus the separator line and header lines (any line containing `| Name |`, `| Date |`, `| Type |`
or `| Key |`), minus rows whose enum, number or date fails to parse. Examples:
`awk '/^## Transactions/{t=1} t && /^\| 20/' fixtures/SampleData2.md | wc -l` ⇒ 278;
LargeSampleData's single obligation has status `active`, which is not a valid `ObligationStatus`, so 0 are imported.
If your importer disagrees with this file, **your importer is wrong**. Re-read the Swift code;
do not edit the JSON.

## Tests (required)

| Test name | Covers | What it checks |
|---|---|---|
| `r_md_4_esc_round_trips_pipes` | R-MD-4 | `esc("a|b") == "a｜b"`, `unesc(esc(s)) == s` for s without `｜` |
| `fmt_decimal_matches_swift_description` | R-MD-1 | `12.50→"12.5"`, `-1200.00→"-1200"`, `0.00→"0"`, `219.10→"219.1"`, `-0.5→"-0.5"` |
| `parse_decimal_strict` | R-MD-2 | `"12.50"`, `"-5"`, `"+5"` parse; `""`, `"Rent"`, `"1,000"` ⇒ None |
| `swift_bool_matches_nsstring` | R-MD-2 | the true/false table in step 5 |
| `fmt_date_is_local_midnight_utc` | R-MD-1 | 2026-02-01 in New York ⇒ `2026-02-01T05:00:00Z`; in Chicago ⇒ `…T06:00:00Z`; in Tokyo ⇒ `2026-01-31T15:00:00Z`; 2026-07-01 New York ⇒ `…T04:00:00Z` (DST) |
| `parse_date_converts_to_local_day` | R-MD-2 | `2026-03-13T04:59:06Z` in Chicago ⇒ 2026-03-12; in Tokyo ⇒ 2026-03-13; `2025-01-25` ⇒ 2025-01-25 in any zone; `""`, `"garbage"` ⇒ None |
| `fmt_timestamp_drops_fraction` | R-MD-1 | `2026-02-01T05:00:00.123Z` ⇒ `2026-02-01T05:00:00Z` |
| `beh_md_01_fixture_counts_match_expected` | BEH-MD-01, R-MD-2 | for each of the 6 fixtures, entity counts == `fixtures/expected_counts.json` (parse with `serde_json::Value`) |
| `r_md_2_old_budget_items_format` | R-MD-2 | `SampleData2.md` (9-column budget items, `Budget Items` heading, date-only start dates): 10 items, `Water` is SpecificMonths `[1,3,5,7,9,11]`, `end_date == None` |
| `r_md_2_new_budget_items_format` | R-MD-2 | `small.md`: `Gym` has `end_date == 2026-06-30`, frequency Monthly |
| `r_md_2_categories_without_patterns` | R-MD-2 | `SampleData.md`: 5-column categories import with empty `match_patterns`; `Bonus` parent is `Salary` |
| `r_md_2_swapped_context_columns_quirk` | R-MD-2 | `SampleData2.md` header `Name | Notes | Color`: `vacation-2026` gets `color_hex == "saving for hawaii"` and `notes == "#008080"` (Swift reads by position; keep) |
| `r_md_2_header_marker_rows_are_skipped` | R-MD-2 | in `small.md` the `column | Date | Date` mapping is dropped (4 mappings, none with source `Date`); in an inline test document, a transaction row whose payee cell is exactly `Name` is skipped, and so is a row with `---` inside a memo |
| `r_md_2_legacy_transaction_layout_imports_nothing` | R-MD-2 | `LargeSampleData.md`: 0 transactions; `skipped_rows >= 675` |
| `r_md_2_missing_trailing_pipe_drops_last_cell` | R-MD-2 | `"| Ctx | #fff | note"` under Contexts ⇒ a context `Ctx` with color `#fff` and notes None |
| `r_md_6_name_lookups_exact_and_first_wins` | R-MD-6 | duplicate context/category/scenario/account names: second row ignored; a transaction category `groceries` (wrong case) ⇒ None |
| `r_md_6_orphans_and_unknown_subsections` | R-MD-6 | `small.md`: `Closed account fee` has `account_id == None`; `### Monarch (Global)` mapping has `account_id == None`; `### Checking` mappings point at Checking |
| `small_fixture_facts` | R-MD-2 | every row of the "Facts" table (compute balances directly from the Db in the test; do not depend on T03) |
| `import_progress_messages` | R-MD-2 | progress receives `Importing Contexts...` … `Importing Settings...` in order, then ends with `Finalizing...`; `SampleData2.md` also yields `Importing Transactions... 50/278` |
| `beh_md_02_golden_export_small` | BEH-MD-02, R-MD-1 | `insta::assert_snapshot!(export_markdown(&import(small, &chicago()).db, &chicago()))` |
| `golden_export_each_fixture` | R-MD-1 | one snapshot per real fixture after import (Chicago) |
| `export_is_idempotent_on_fixtures` | R-MD-1, R-MD-3 | for every fixture: `export(import(export(import(f)))) == export(import(f))` |
| `r_md_1_empty_db_layout` | R-MD-1 | export of an empty Db equals the literal string: title, the six table sections with header and separator lines only, `## ImportMappings\n## Transactions\n` with no blank line between, and no Settings section |
| `r_md_1_settings_section_rules` | R-MD-1 | no section when neither `lastProjectionScenario` nor `backupFolder` is set; dates written via `fmt_date`; `backupFolder` written as key `backupFolderBookmark` |
| `r_md_1_only_live_rows_exported` | R-MD-1 | soft-deleted rows of every entity type are absent |
| `r_md_1_sort_orders` | R-MD-1 | children categories after all top-level ones; transaction groups sorted; same-date rows by id descending |
| `r_md_3_round_trip_property` | R-MD-3 | proptest (see below), run in New York and Tokyo |
| `r_md_4_escape_property` | R-MD-4 | proptest: for any string without `｜` (`"\\PC*"` minus U+FF5C), `unesc(esc(s)) == s` and `esc(s)` contains no `|` |

**Round-trip property (`r_md_3_round_trip_property`).** Write a proptest strategy that builds a
`Db` respecting the guarantee in `docs/markdown-format.md` → "Round-trip guarantee":
- names from `"[A-Za-z][A-Za-z0-9 ]{0,10}[A-Za-z0-9]"`, unique **per entity type and across categories**,
  never equal to `Name`, `Date`, `Type`, `Key`, `Uncategorized` or `Monarch (Global)`;
- free-text fields (payee, memo, notes) from `"[A-Za-z0-9 |.,'!-]{0,20}"` with `---` removed, trimmed,
  and `None` when empty (so `|` escaping is exercised);
- context names must not contain `,`; patterns must not contain `;` and must be non-empty;
- one level of categories (parents, then children whose parent is a top-level category);
- amounts are `Decimal::new(cents, 2)` with cents in ±10_000_000; dates between 2020-01-01 and 2030-12-31;
  timestamps are whole seconds;
- mapping cells must not be exactly `Date`, `Name`, `Type` or `Key`;
- no soft-deleted rows.

Then `let back = import(&export_markdown(&db, &tz), &tz).db;` and compare **normalised** forms:
write a test-only `fn normalise(db: &Db) -> Vec<String>` that renders every entity with ids replaced by
names (account name, category name, scenario name, sorted context names) and sorts the list.
Assert `normalise(&back) == normalise(&db)`. Decimals compare by value (`12.5 == 12.50`).
Use `ProptestConfig::with_cases(64)` to keep the run under a few seconds.

## Pitfalls

- **Real-fixture variants.** `SampleData.md` and `LargeSampleData.md` use the legacy transaction layout (0 imported); `SampleData2.md` uses a `## Budget Items` heading (with a space), 9-column budget items, date-only start dates, and a `Name | Notes | Color` context header (values read by position); `SampleData3.md` transaction rows have 9 cells with an empty Attachment; `PersonalFinance_Backup.md` repeats header lines inside the Contexts and Scenarios tables (they are skipped because they contain `| Name |`).
- **Time zone.** Never call `TimeZone::system()` in `pf-core`; the zone is a parameter. Chicago-written timestamps like `2026-03-13T04:59:06Z` are **March 12** in Chicago. A wrong zone shifts every date by one day.
- **jiff, not chrono.** `Timestamp` parsing handles `Z`; `Date` parsing handles `YYYY-MM-DD`. Don't hand-roll regexes.
- **Trim before unescape**, and trim each cell, not the whole line. `str::trim` is fine (lines contain no `\n`).
- Swift's `Decimal(string:)` accepts a numeric prefix (`"12abc"` ⇒ 12). We parse strictly. No fixture depends on this; it is a deliberate, documented difference.
- `ObligationStatus`, `AccountType`, `CategoryType` and `TransactionStatus` parse **case-sensitively** (`"active"` and `"Checking"` fail). Only BudgetItem type and frequency are case-insensitive (also with spaces removed).
- The export header strings must match the spec byte for byte, including `| Name | Type | Icon | Notes | Parent | Match Patterns |` and `| Date | Payee | Category | Amount | Memo | Scenario | Contexts | Status | Attachment |`.
- Snapshots: never run `cargo insta accept` blindly. A wrong snapshot becomes the "truth" for every later task.

## Done when

```bash
cd PFRust
cargo test -p pf-core markdown
cargo insta test -p pf-core --check        # no pending snapshots
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules                          # (after T00b) R-MD-1..4, R-MD-6, BEH-MD-01/02 covered
```

## Out of scope

- Wiping the database, the confirmation dialog, clearing undo, and the Cmd-S backup file (T10c / T21). R-MD-5 and R-MD-7 belong to those tasks.
- Reading the SwiftData store. Writing files. Anything touching the clock (`now` and `tz` are parameters).
