# Data Model (Rust + SQLite)

This is the **authoritative** definition of the PFRust data model. It mirrors the
Swift SwiftData models in `PFApp/Sources/PFApp/Models/*.swift`. If this file and the
Swift code disagree on *behaviour*, the Swift code wins (and fix this file). If they
disagree on *representation* (types, ids, storage), this file wins.

## Conventions

| Concern | Swift | Rust |
|---|---|---|
| Identity | `PersistentIdentifier` | `type Id = i64` (SQLite `INTEGER PRIMARY KEY`). Newtype wrappers per entity: `AccountId(Id)`, etc. |
| Money | `Decimal` | `rust_decimal::Decimal`. **Never `f64` for money.** `f64` only when handing values to a chart. |
| Calendar dates (transaction date, budget start/end, due date) | `Date` (timestamp, logic uses `startOfDay`) | `jiff::civil::Date` (no time, no zone). egui_extras 0.37 DatePickerButton uses this exact type. |
| Timestamps (created / last updated) | `Date` | `jiff::Timestamp` |
| Enums stored as raw strings | `typeRawValue: String?` with fallback | Rust `enum` with `as_str()` / `parse()`; unknown/missing values fall back to the same default Swift uses (listed below) |
| Soft delete | `isDeleted: Bool` | `is_deleted: bool` on every entity that has it in Swift |
| Relationships | object references | foreign-key ids: `Option<AccountId>` etc. Many-to-many via `Vec<ContextId>` in memory, join table on disk |

All entity structs derive `Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize`.

## Enums (exact string values — these are written to the Markdown backup)

```rust
pub enum AccountType { Checking, Savings, Credit, Loan, Investment, Cash }
// strings: "checking" "savings" "credit" "loan" "investment" "cash"   default: Checking

pub enum CategoryType { Income, Expense, Transfer }
// strings: "income" "expense" "transfer"                              default: Expense

pub enum TransactionStatus { Pending, Cleared, Reconciled, Future }
// strings: "pending" "cleared" "reconciled" "future"                  default: Cleared
// display labels: "Pending" "Cleared" "Reconciled" "Future"
// colors: cleared = normal text, pending = green, reconciled = rgb(0.55,0.27,0.07) brown, future = red

pub enum ObligationStatus { Funding, Spending, Future, Deferred, Completed }
// strings: "Funding" "Spending" "Future" "Deferred" "Completed"  (Capitalised!)  default: Funding

pub enum BudgetItemType { Bill, Expense, Income }
// strings: "Bill" "Expense" "Income"  (Capitalised)                  default: Expense

pub enum BudgetFrequency { Weekly, Monthly, EveryOtherMonth, Quarterly, Yearly, SpecificMonths }
// strings: "Weekly" "Monthly" "Every Other Month" "Quarterly" "Yearly" "Specific Months"   default: Monthly
// parse(): case-insensitive match of the raw value, OR of the input with its spaces removed against the raw value
//          (matches Swift: "every other month" ok, "Monthly" ok, "everyothermonth" does NOT match "Every Other Month")

pub enum TimePeriod { PastMonth, Past60Days, YearToDate, PastYear, Archival, All }
// labels: "Past Month" "Past 60 Days" "Year to Date" "Past Year" "Archival" "All"
// (UI-only; not persisted in entities, but persisted as a setting)
```

Each enum gets: `pub const ALL: [Self; N]`, `fn as_str(&self) -> &'static str`,
`fn label(&self) -> &'static str` (if different), `impl FromStr`, and
`impl Default` returning the default above.

## Entities

```rust
pub struct Account {
    pub id: AccountId,
    pub name: String,
    pub account_type: AccountType,
    pub institution: String,
    pub currency: String,        // default "USD"
    pub is_external: bool,       // external accounts are excluded from net worth / totals
    pub is_deleted: bool,
}

pub struct Transaction {
    pub id: TransactionId,
    pub date: Date,
    pub amount: Decimal,         // + = money in, - = money out
    pub payee: String,
    pub memo: Option<String>,
    pub attachment_path: Option<String>,
    pub status: TransactionStatus,
    pub is_deleted: bool,
    pub account_id: Option<AccountId>,     // None = "orphaned" transaction
    pub scenario_id: Option<ScenarioId>,   // None = Reality
    pub category_id: Option<CategoryId>,
    pub context_ids: Vec<ContextId>,       // many-to-many, order preserved
}

pub struct Category {
    pub id: CategoryId,
    pub name: String,
    pub icon: Option<String>,              // SF Symbol name or emoji; Rust UI shows emoji, ignores SF names
    pub notes: Option<String>,
    pub match_patterns: Vec<String>,       // Swift: [String]? — empty Vec == nil
    pub category_type: CategoryType,
    pub parent_id: Option<CategoryId>,
    pub is_deleted: bool,
}
// full_name(): "Parent/Child" if parent exists, else name. (Needs a lookup of the parent.)

pub struct Context {          // a.k.a. "tag"; shown in UI as "#name"
    pub id: ContextId,
    pub name: String,
    pub color_hex: Option<String>,         // "#RRGGBB" or "#RRGGBBAA"
    pub notes: Option<String>,
    pub is_archived: bool,                 // Swift: Bool? — nil == false
    pub is_deleted: bool,
}

pub struct Scenario {
    pub id: ScenarioId,
    pub name: String,
    pub color_hex: Option<String>,
    pub notes: Option<String>,
    pub is_deleted: bool,
}

pub struct Obligation {
    pub id: ObligationId,
    pub name: String,
    pub target_amount: Option<Decimal>,
    pub current_balance: Decimal,          // manually funded starting amount
    pub due_date: Option<Date>,
    pub notes: Option<String>,
    pub status: ObligationStatus,
    pub created_date: Option<Timestamp>,
    pub last_updated_date: Option<Timestamp>,
    pub account_id: Option<AccountId>,
    pub context_id: Option<ContextId>,
    pub is_deleted: bool,
}

pub struct BudgetItem {
    pub id: BudgetItemId,
    pub item_type: BudgetItemType,
    pub name: String,                      // payee for Bill, category-ish name for Expense, source for Income
    pub amount: Decimal,
    pub start_date: Date,
    pub end_date: Option<Date>,
    pub frequency: BudgetFrequency,
    pub specific_months: Vec<u32>,         // 1..=12, only meaningful for SpecificMonths
    pub notes: Option<String>,
    pub category_id: Option<CategoryId>,
    pub account_id: Option<AccountId>,
    pub is_deleted: bool,
}

pub struct ImportMapping {
    pub id: ImportMappingId,
    pub mapping_type: String,              // "account" | "category" | "payee" | "column" | "setting"
    pub source_value: String,
    pub target_value: String,
    pub negate_amounts: bool,
    pub account_id: Option<AccountId>,     // None = global (Monarch); Some = per-bank-account
    // NOTE: ImportMapping has no is_deleted in Swift. Deletes are hard deletes.
}
```

`"column"` mappings: `source_value` = CSV header text, `target_value` = app field
name ("Date", "Payee", "Amount", "Category", "Memo", "Account", "Tags", "Original Statement").
They remember a bank's column layout so the API/CLI import can run headless.

`"setting"` mappings: per-account import settings. Currently only `source_value = "negateAmounts"`,
`target_value = "true"|"false"` (and `negate_amounts` set the same). See `CSVImportFlowView.swift` lines 70–105.

## The in-memory database

```rust
pub struct Db {
    pub accounts: BTreeMap<AccountId, Account>,
    pub transactions: BTreeMap<TransactionId, Transaction>,
    pub categories: BTreeMap<CategoryId, Category>,
    pub contexts: BTreeMap<ContextId, Context>,
    pub scenarios: BTreeMap<ScenarioId, Scenario>,
    pub obligations: BTreeMap<ObligationId, Obligation>,
    pub budget_items: BTreeMap<BudgetItemId, BudgetItem>,
    pub import_mappings: BTreeMap<ImportMappingId, ImportMapping>,
    pub settings: BTreeMap<String, String>,
    next_id: Id,   // one id sequence shared by all tables; simplifies undo and tests
}
```

Helper query methods live on `Db` (in `pf-model`), always skipping `is_deleted` rows
unless the method name says `_including_deleted`:
`live_accounts()`, `live_transactions()`, `account_by_name_ci(&str)`,
`scenario_by_name_ci(&str)`, `category_full_name(CategoryId)`, `transactions_for_account(AccountId)`, etc.

## SQLite schema (`pf-store`)

File: `~/Library/Application Support/PF/PFRust.sqlite` (use `directories` crate or
`$HOME/Library/Application Support`). **Never open or modify the Swift app's `PF.store`.**

```sql
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = OFF;   -- integrity is enforced by the engine; soft deletes keep rows around

CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
-- meta('schema_version','1'), meta('next_id', '…')

CREATE TABLE IF NOT EXISTS accounts (
  id INTEGER PRIMARY KEY, name TEXT NOT NULL, account_type TEXT NOT NULL,
  institution TEXT NOT NULL, currency TEXT NOT NULL, is_external INTEGER NOT NULL,
  is_deleted INTEGER NOT NULL DEFAULT 0);

CREATE TABLE IF NOT EXISTS transactions (
  id INTEGER PRIMARY KEY, date TEXT NOT NULL /* YYYY-MM-DD */, amount TEXT NOT NULL /* Decimal string */,
  payee TEXT NOT NULL, memo TEXT, attachment_path TEXT, status TEXT NOT NULL,
  is_deleted INTEGER NOT NULL DEFAULT 0,
  account_id INTEGER, scenario_id INTEGER, category_id INTEGER);
CREATE INDEX IF NOT EXISTS ix_tx_account ON transactions(account_id);
CREATE INDEX IF NOT EXISTS ix_tx_date ON transactions(date);

CREATE TABLE IF NOT EXISTS transaction_contexts (
  transaction_id INTEGER NOT NULL, context_id INTEGER NOT NULL, position INTEGER NOT NULL,
  PRIMARY KEY (transaction_id, context_id));

CREATE TABLE IF NOT EXISTS categories (
  id INTEGER PRIMARY KEY, name TEXT NOT NULL, icon TEXT, notes TEXT,
  match_patterns TEXT NOT NULL /* JSON array of strings */, category_type TEXT NOT NULL,
  parent_id INTEGER, is_deleted INTEGER NOT NULL DEFAULT 0);

CREATE TABLE IF NOT EXISTS contexts (
  id INTEGER PRIMARY KEY, name TEXT NOT NULL, color_hex TEXT, notes TEXT,
  is_archived INTEGER NOT NULL DEFAULT 0, is_deleted INTEGER NOT NULL DEFAULT 0);

CREATE TABLE IF NOT EXISTS scenarios (
  id INTEGER PRIMARY KEY, name TEXT NOT NULL, color_hex TEXT, notes TEXT,
  is_deleted INTEGER NOT NULL DEFAULT 0);

CREATE TABLE IF NOT EXISTS obligations (
  id INTEGER PRIMARY KEY, name TEXT NOT NULL, target_amount TEXT, current_balance TEXT NOT NULL,
  due_date TEXT, notes TEXT, status TEXT NOT NULL, created_date TEXT, last_updated_date TEXT,
  account_id INTEGER, context_id INTEGER, is_deleted INTEGER NOT NULL DEFAULT 0);

CREATE TABLE IF NOT EXISTS budget_items (
  id INTEGER PRIMARY KEY, item_type TEXT NOT NULL, name TEXT NOT NULL, amount TEXT NOT NULL,
  start_date TEXT NOT NULL, end_date TEXT, frequency TEXT NOT NULL,
  specific_months TEXT NOT NULL /* JSON array of ints */, notes TEXT,
  category_id INTEGER, account_id INTEGER, is_deleted INTEGER NOT NULL DEFAULT 0);

CREATE TABLE IF NOT EXISTS import_mappings (
  id INTEGER PRIMARY KEY, mapping_type TEXT NOT NULL, source_value TEXT NOT NULL,
  target_value TEXT NOT NULL, negate_amounts INTEGER NOT NULL, account_id INTEGER);

CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
```

Timestamps are stored as RFC 3339 strings (`Timestamp::to_string()`, e.g. `2026-02-01T05:00:00Z`).

## Settings keys

| Key | Value | Used by |
|---|---|---|
| `lastProjectionScenario` | scenario name | Budget projection dialog prefill; Markdown export `## Settings` |
| `lastProjectionStartDate` | `YYYY-MM-DD` | same |
| `lastProjectionEndDate` | `YYYY-MM-DD` | same |
| `backupFolder` | absolute folder path | Cmd-S backup (Swift used a macOS bookmark; we store a plain path) |
| `apiPort` | integer, default `8989` | API server |
| `sidebarGrouping` | `A-Z` \| `Institution` \| `Type` | Sidebar (Rust addition; Swift kept it as view state) |
| `selectedTimePeriod` | TimePeriod label | Account detail |
