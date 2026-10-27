# Markdown Backup Format

The Markdown backup is the **migration bridge**: the user exports from the Swift app
(Settings → Export, or `curl http://localhost:8989/api/export/markdown > backup.md`)
and imports into PFRust. PFRust's export must be readable by the Swift importer and
vice-versa. Source of truth: `PFApp/Sources/PFApp/Services/MarkdownDataService.swift`.

Real examples to use as fixtures (copy into `PFRust/fixtures/`, never edit originals):
`SampleData.md`, `SampleData2.md`, `SampleData3.md`, `LargeSampleData.md`,
`PersonalFinance_Backup.md` (all in the repo root). Note `SampleData.md` is an
*older* format (Categories without "Match Patterns", BudgetItems without "End Date").

## Layout (export order is fixed)

```
# Personal Finance Data

## Contexts
| Name | Color | Notes |
| --- | --- | --- |
| {name} | {colorHex or ""} | {notes or ""} |

## Categories
| Name | Type | Icon | Notes | Parent | Match Patterns |
| --- | --- | --- | --- | --- | --- |
| {name} | {income|expense|transfer} | {icon} | {notes} | {parent NAME, not full name} | {patterns joined with ";"} |

## Scenarios
| Name | Color | Notes |
| --- | --- | --- |

## Accounts
| Name | Type | Institution | Currency | External |
| --- | --- | --- | --- | --- |
| {name} | {checking|…} | {institution} | {USD} | {true|false} |

## Obligations
| Name | Status | Target | Balance | Due Date | Created Date | Updated Date | Notes | Account | Context |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |

## BudgetItems
| Name | Type | Amount | Start Date | End Date | Frequency | Specific Months | Notes | Category | Account |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| {name} | {Bill|Expense|Income} | {amount} | {date} | {date or ""} | {Monthly|…} | {1,4,7} | {notes} | {category NAME} | {account name} |

## ImportMappings
### Monarch (Global)                      <- only if any global mappings exist
| Type | Source Value | Target Value | Negate Amounts |
| --- | --- | --- | --- |
| {type} | {source} | {target} | {true|false} |

### {Account Name}                         <- one per account with mappings, sorted by name
| Type | Source Value | Target Value | Negate Amounts |
| --- | --- | --- | --- |

## Transactions
### {Account Name}                         <- grouped by account name, sorted; orphans under "Uncategorized"
| Date | Payee | Category | Amount | Memo | Scenario | Contexts | Status | Attachment |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| {date} | {payee} | {category NAME} | {amount} | {memo} | {scenario name} | {ctx1,ctx2} | {pending|cleared|…} | {path} |

## Settings                                <- only if any setting below is set
| Key | Value |
| --- | --- |
| lastProjectionScenario | {name} |
| lastProjectionStartDate | {date} |
| lastProjectionEndDate | {date} |
| backupFolderBookmark | {value} |
```

Each section ends with one blank line. Every line ends with `\n`.

## Sort orders on export

* Contexts, Scenarios, Accounts, Obligations, BudgetItems: by `name` (byte-wise, as Swift `<`) — live rows only.
* Categories: top-level categories first, then children; within each group by name.
* ImportMappings: Global group first (store order), then accounts sorted by name (store order within).
* Transactions: account groups sorted by name; within a group date descending (ties: id descending).

## Value encoding

| Kind | Export | Import |
|---|---|---|
| String | value with `|` replaced by `｜` (U+FF5C) | trim, then replace `｜` with `|` |
| Optional string | `""` when None | `""` ⇒ None |
| Decimal | Swift `Decimal.description`: no trailing zeros, no exponent (`12.5`, `-1200`, `0`). In Rust: `d.normalize().to_string()` | `Decimal::from_str` (accepts `12.50`) |
| Optional decimal (Target) | `""` when None | `""` or invalid ⇒ None |
| Bool | `true` / `false` | Swift `NSString.boolValue`: after trimming, true iff it starts with `t`, `T`, `y`, `Y`, or a digit 1–9 |
| Calendar date | **local midnight of that date, converted to UTC**, ISO-8601 with `Z`, no fraction: e.g. `2026-02-01T05:00:00Z` in New York. jiff: `date.to_zoned(TimeZone::system())?.timestamp().to_string()` | parse full ISO-8601 timestamp ⇒ convert to `TimeZone::system()` ⇒ `.date()`. Fallback: `yyyy-MM-dd`. Empty ⇒ None |
| Timestamp (created/updated) | `Timestamp` Display (`2026-02-01T05:00:00Z`) | same parsing, keep as Timestamp; a bare `yyyy-MM-dd` becomes local midnight |
| Settings dates | as calendar date | as calendar date |
| Specific months | ints joined by `,` | split `,`, parse ints, ignore junk |
| Contexts list | names joined by `,` | split `,` (no trim, matching Swift); unknown names ignored |
| Patterns | joined by `;` | split `;`; empty string ⇒ empty Vec |

Why local midnight: Swift stores transaction dates as timestamps in the user's zone.
A Swift export of "Feb 1" in New York reads `2026-02-01T05:00:00Z`. Converting to
the system zone and taking the date gives Feb 1 back. Tests must pin the zone with
`TimeZone::get("America/New_York")` — make the zone a parameter of
`export_with_tz` / `import_with_tz`, and have the public `export`/`import` pass
`TimeZone::system()`.

## Import parsing algorithm (match exactly, quirks included)

1. Split into lines (`\n`, tolerate `\r\n`).
2. Line starts with `## ` ⇒ `section = rest.trim()`, reset account subsection. Report progress `Importing {section}...`.
3. Line starts with `### `:
   * if section is `Transactions`, `ImportMappings` or `Import Mappings`: subsection = rest.trim(); `Monarch (Global)` ⇒ global.
   * otherwise ignore.
4. Line starts with `|` and does **not contain** `---`:
   * skip if it contains `| Name |`, `| Date |`, `| Type |` or `| Key |` (header rows). **Quirk:** a data row whose cell is exactly `Name`/`Date`/`Type`/`Key`, or which contains `---` anywhere, is also skipped. Keep the quirk; test it (R-MD-2).
   * columns = split on `|`, drop the first and last piece, trim each, unescape.
   * dispatch on section: `Contexts`, `Categories`, `Scenarios`, `Accounts`, `Obligations`, `BudgetItems`|`Budget Items`, `Transactions`, `ImportMappings`|`Import Mappings`, `Settings`; any other section is ignored.
5. Per-row rules (minimum column counts; rows with fewer columns are skipped):
   * Context ≥3: skip if a context with that exact name already exists.
   * Category ≥5 (6th = patterns, optional): type must parse else skip; parent looked up by **name** among categories created so far; skip if name exists.
   * Scenario ≥3; Account ≥5 (type must parse else skip; skip if name exists).
   * Obligation ≥10: status must parse else skip; created/updated default to now when empty.
   * BudgetItem ≥9: type and frequency parsed case-insensitively and also with spaces removed; amount and start date must parse. **≥10 columns ⇒ new format** (End Date at index 4), else old format (no End Date).
   * Transaction ≥8: date, amount and status must parse else skip; category looked up by **name**; account from the `###` subsection (None if missing/unknown); attachment = column 8 if present and non-empty.
   * ImportMapping ≥4: account from subsection unless global; old 5-column format uses column 4 as the account name.
   * Setting ≥2: `lastProjectionScenario`, `lastProjectionStartDate`, `lastProjectionEndDate`, `backupFolderBookmark` (stored as the Rust setting `backupFolder` only if it looks like an absolute path; Swift bookmark blobs are ignored).
6. Name lookups use the **exact** string (case-sensitive) and only see rows created **by this import** (the lookup caches start empty; the Settings screen always wipes first).

## Round-trip guarantee (R-MD-3)

For any `Db` whose names are unique per entity type and contain no leading/trailing
whitespace or `---`: `import(export(db))` yields a `Db` equal to the original after
mapping ids by name, with these known lossy points:
* Transactions' category is matched by *name*: two categories with the same name under different parents collapse to the first.
* Soft-deleted rows are not exported.
* Transaction ordering within the same date is by id.
