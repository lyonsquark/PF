# T01: pf-model (ids, enums, entities, `Db`, `Record`/`Change`, `TestDb` builder)

**Phase:** 1 · **Deps:** T00 · **Crate(s):** `pf-model` · **Size:** medium (about 700 lines including tests)

## Goal

Create the data types every other crate uses: the entity structs, the string-backed
enums, the in-memory `Db` with query helpers, the generic row types `Record` / `RowKey` /
`Change` (used by pf-store and pf-engine for persistence and undo), and the `TestDb` builder
that every test in the workspace uses to create data.

## Read first

- `docs/data-model.md` (all of it: this is the spec)
- `docs/testing.md` → L1 (the `TestDb` example)
- Swift: `../PFApp/Sources/PFApp/Models/*.swift` (all files, especially the enum raw values and defaults)
- Rules: R-CAT-4 (full name)

## Deliverables

```
crates/pf-model/
├── Cargo.toml          add: [features] test-support = ["dep:rust_decimal_macros"], and make
│                       rust_decimal_macros an optional normal dependency (keep it in dev-deps too)
└── src/
    ├── lib.rs          module declarations + re-exports (pub use ids::*, enums::*, …)
    ├── ids.rs          Id + newtype ids
    ├── enums.rs        all enums
    ├── entities.rs     entity structs
    ├── record.rs       Record, RowKey, Change
    ├── db.rs           Db + query helpers
    └── testing.rs      TestDb builder  (#[cfg(any(test, feature = "test-support"))])
```

`Cargo.toml` changes (exact):
```toml
[features]
test-support = ["dep:rust_decimal_macros"]

[dependencies]
rust_decimal = { workspace = true }
rust_decimal_macros = { workspace = true, optional = true }
jiff = { workspace = true }
serde = { workspace = true }
thiserror = { workspace = true }
```
Other crates use the builder by adding, under their `[dev-dependencies]`:
`pf-model = { workspace = true, features = ["test-support"] }`. **Do that now for pf-core, pf-store,
pf-engine, pf-api and pf-ui** (dev-dependencies only).

### ids.rs
```rust
pub type Id = i64;

/// Generates a Copy newtype id: Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash,
/// Serialize, Deserialize, plus `impl From<Id>`, `fn raw(self) -> Id`, and Display (prints the number).
macro_rules! id_type { ($name:ident) => { /* … */ } }

id_type!(AccountId); id_type!(TransactionId); id_type!(CategoryId); id_type!(ContextId);
id_type!(ScenarioId); id_type!(ObligationId); id_type!(BudgetItemId); id_type!(ImportMappingId);
```
Fields are `pub struct AccountId(pub Id);`.

### enums.rs
For **each** enum: `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]`,
`pub const ALL: [Self; N]` (in the order below, which is Swift's `allCases` order),
`pub fn as_str(self) -> &'static str` (the stored or exported string), `impl Default`,
`impl std::str::FromStr` (**exact** match on `as_str`; error `ParseEnumError`), and `impl Display` (prints `label()`).

| Enum | Variants → `as_str` | Default | `label()` |
|---|---|---|---|
| `AccountType` | Checking "checking", Savings "savings", Credit "credit", Loan "loan", Investment "investment", Cash "cash" | Checking | Capitalised ("Checking") |
| `CategoryType` | Income "income", Expense "expense", Transfer "transfer" | Expense | Capitalised |
| `TransactionStatus` | Pending "pending", Cleared "cleared", Reconciled "reconciled", Future "future" | Cleared | "Pending" "Cleared" "Reconciled" "Future" |
| `ObligationStatus` | Funding "Funding", Spending "Spending", Future "Future", Deferred "Deferred", Completed "Completed" | Funding | same as as_str |
| `BudgetItemType` | Bill "Bill", Expense "Expense", Income "Income" | Expense | same |
| `BudgetFrequency` | Weekly "Weekly", Monthly "Monthly", EveryOtherMonth "Every Other Month", Quarterly "Quarterly", Yearly "Yearly", SpecificMonths "Specific Months" | Monthly | same |
| `TimePeriod` | PastMonth "Past Month", Past60Days "Past 60 Days", YearToDate "Year to Date", PastYear "Past Year", Archival "Archival", All "All" | PastMonth | same |

Extra:
```rust
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid {kind}: {value:?}")]
pub struct ParseEnumError { pub kind: &'static str, pub value: String }

impl BudgetItemType  { /// Case-insensitive; also matches with spaces removed. Used by the Markdown importer.
                       pub fn parse_lenient(s: &str) -> Option<Self>; }
impl BudgetFrequency { pub fn parse_lenient(s: &str) -> Option<Self>; }  // "every other month", "everyothermonth", "EVERY OTHER MONTH" → EveryOtherMonth
impl <every enum>    { /// FromStr or Default::default() (Swift's raw-value fallback).
                       pub fn parse_or_default(s: &str) -> Self; }
```
Swift lenient rule (`MarkdownDataService.importBudgetItem`), copied exactly:
```rust
let input = s.to_lowercase();
let squeezed = input.replace(' ', "");
Self::ALL.into_iter().find(|v| { let raw = v.as_str().to_lowercase(); raw == input || raw == squeezed })
```
Spaces are removed from the **input only**, so `"everyothermonth"` does **not** match
`"every other month"` (this contradicts `docs/data-model.md`, and Swift wins). It only helps inputs like
`"Mon thly"`. A test pins this (`every_other_month_lenient_quirk`): `"every other month"` → Some(EveryOtherMonth),
`"EVERY OTHER MONTH"` → Some, `"everyothermonth"` → None, `"Mon thly"` → Some(Monthly), `"MONTHLY"` → Some(Monthly).

### entities.rs
Exactly the structs in `docs/data-model.md` → "Entities" (same field names and types), with
`#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]`. Use `rust_decimal::Decimal`,
`jiff::civil::Date` and `jiff::Timestamp`. No methods besides constructors with defaults:
```rust
impl Account     { pub fn new(id: AccountId, name: &str, account_type: AccountType, institution: &str) -> Self; } // currency "USD", not external, live
impl Transaction { pub fn new(id: TransactionId, date: Date, amount: Decimal, payee: &str) -> Self; }   // status Cleared, everything else None/empty
impl Category    { pub fn new(id: CategoryId, name: &str, category_type: CategoryType) -> Self; }
impl Context     { pub fn new(id: ContextId, name: &str) -> Self; }
impl Scenario    { pub fn new(id: ScenarioId, name: &str) -> Self; }
impl Obligation  { pub fn new(id: ObligationId, name: &str) -> Self; }                                 // status Funding, current_balance 0
impl BudgetItem  { pub fn new(id: BudgetItemId, item_type: BudgetItemType, name: &str, amount: Decimal, start_date: Date, frequency: BudgetFrequency) -> Self; }
impl ImportMapping { pub fn new(id: ImportMappingId, mapping_type: &str, source: &str, target: &str, account_id: Option<AccountId>) -> Self; }
```

### record.rs: owned by pf-model (pf-engine re-exports them)

`PLAN.md` shows `Record` / `RowKey` / `Change` in its pf-engine snippet. **They live here in
pf-model** so `pf-store` can persist them without depending on `pf-engine`. pf-engine does
`pub use pf_model::{Record, RowKey, Change};`.

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Record {
    Account(Account), Transaction(Transaction), Category(Category), Context(Context),
    Scenario(Scenario), Obligation(Obligation), BudgetItem(BudgetItem),
    ImportMapping(ImportMapping), Setting { key: String, value: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RowKey {
    Account(AccountId), Transaction(TransactionId), Category(CategoryId), Context(ContextId),
    Scenario(ScenarioId), Obligation(ObligationId), BudgetItem(BudgetItemId),
    ImportMapping(ImportMappingId), Setting(String),
}

/// One row's before/after image. `None` = the row does not exist (insert has before=None,
/// hard delete has after=None). A soft delete is an update (`is_deleted` flips).
#[derive(Debug, Clone, PartialEq)]
pub struct Change { pub key: RowKey, pub before: Option<Record>, pub after: Option<Record> }

impl Record { pub fn key(&self) -> RowKey; }
impl Change { /// The inverse change (swap before/after) — used by undo.
              pub fn inverted(&self) -> Change; }
```

### db.rs
```rust
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
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
    next_id: Id,                                 // private; 0 means "not yet allocated" → first id is 1
}

impl Db {
    pub fn new() -> Self;
    // ---- id allocation (one sequence shared by all tables) ----
    pub fn alloc_id(&mut self) -> Id;            // returns max(next_id,1), then increments
    pub fn next_id(&self) -> Id;                 // the value alloc_id would return next
    pub fn set_next_id(&mut self, id: Id);       // used by the store after loading
    pub fn max_id(&self) -> Id;                  // largest id present in any table (0 if empty)

    // ---- generic row access (used by engine undo + store) ----
    pub fn get(&self, key: &RowKey) -> Option<Record>;   // clones the row
    pub fn put(&mut self, record: Record);               // insert or replace
    pub fn remove(&mut self, key: &RowKey);              // hard remove (no-op if absent)
    pub fn all_records(&self) -> Vec<Record>;            // every row incl. settings, deleted rows too

    // ---- lookups by id (include soft-deleted rows) ----
    pub fn account(&self, id: AccountId) -> Option<&Account>;
    pub fn transaction(&self, id: TransactionId) -> Option<&Transaction>;
    pub fn category(&self, id: CategoryId) -> Option<&Category>;
    pub fn context(&self, id: ContextId) -> Option<&Context>;
    pub fn scenario(&self, id: ScenarioId) -> Option<&Scenario>;
    pub fn obligation(&self, id: ObligationId) -> Option<&Obligation>;
    pub fn budget_item(&self, id: BudgetItemId) -> Option<&BudgetItem>;

    // ---- live (not deleted) iteration, in id order ----
    pub fn live_accounts(&self) -> impl Iterator<Item = &Account>;
    pub fn live_transactions(&self) -> impl Iterator<Item = &Transaction>;
    pub fn live_categories(&self) -> impl Iterator<Item = &Category>;
    pub fn live_contexts(&self) -> impl Iterator<Item = &Context>;
    pub fn live_scenarios(&self) -> impl Iterator<Item = &Scenario>;
    pub fn live_obligations(&self) -> impl Iterator<Item = &Obligation>;
    pub fn live_budget_items(&self) -> impl Iterator<Item = &BudgetItem>;

    // ---- sorted helpers (live only; sort by name, byte-wise, ties by id) ----
    pub fn accounts_by_name(&self) -> Vec<&Account>;
    pub fn categories_by_name(&self) -> Vec<&Category>;
    pub fn contexts_by_name(&self) -> Vec<&Context>;
    pub fn scenarios_by_name(&self) -> Vec<&Scenario>;
    pub fn obligations_by_name(&self) -> Vec<&Obligation>;
    pub fn budget_items_by_name(&self) -> Vec<&BudgetItem>;

    // ---- name lookups (live only) ----
    pub fn account_by_name_ci(&self, name: &str) -> Option<&Account>;     // case-insensitive (to_lowercase ==)
    pub fn scenario_by_name_ci(&self, name: &str) -> Option<&Scenario>;
    pub fn context_by_name_ci(&self, name: &str) -> Option<&Context>;
    pub fn account_by_name(&self, name: &str) -> Option<&Account>;        // exact (Markdown import)
    pub fn category_by_name(&self, name: &str) -> Option<&Category>;      // exact, first by id
    pub fn context_by_name(&self, name: &str) -> Option<&Context>;
    pub fn scenario_by_name(&self, name: &str) -> Option<&Scenario>;

    // ---- relationships (live only) ----
    /// R-CAT-4: "Parent/Child" if the category has a parent (parent looked up even if deleted), else name.
    pub fn category_full_name(&self, id: CategoryId) -> String;           // unknown id → ""
    pub fn subcategories(&self, parent: CategoryId) -> Vec<&Category>;    // sorted by name
    pub fn transactions_for_account(&self, account: AccountId) -> impl Iterator<Item = &Transaction>;
    pub fn transactions_for_scenario(&self, scenario: ScenarioId) -> impl Iterator<Item = &Transaction>;
    pub fn transactions_with_context(&self, ctx: ContextId) -> impl Iterator<Item = &Transaction>;
    pub fn obligations_for_account(&self, account: AccountId) -> impl Iterator<Item = &Obligation>;

    // ---- settings ----
    pub fn setting(&self, key: &str) -> Option<&str>;
}
```

### testing.rs: the `TestDb` builder

Compiled under `#[cfg(any(test, feature = "test-support"))]`, re-exported as `pf_model::testing`.
**Panics are fine here** (this is test code): invalid date/amount strings panic with a clear message.

```rust
/// "2026-01-05" → Date. Panics on bad input.
pub fn d(s: &str) -> jiff::civil::Date;
/// "-12.50" → Decimal (exact, keeps scale). Panics on bad input.
pub fn m(s: &str) -> rust_decimal::Decimal;

pub struct TestDb { db: Db }

impl TestDb {
    pub fn new() -> Self;
    /// Live account, institution "Test Bank", currency USD, not external.
    pub fn account(&mut self, name: &str, t: AccountType) -> AccountId;
    pub fn external_account(&mut self, name: &str, t: AccountType) -> AccountId;
    pub fn scenario(&mut self, name: &str) -> ScenarioId;
    pub fn category(&mut self, name: &str, t: CategoryType) -> CategoryId;
    pub fn child_category(&mut self, parent: CategoryId, name: &str) -> CategoryId;   // same type as parent
    pub fn patterns(&mut self, cat: CategoryId, patterns: &[&str]);                    // replaces match_patterns
    pub fn context(&mut self, name: &str) -> ContextId;
    pub fn setting(&mut self, key: &str, value: &str);
    pub fn import_mapping(&mut self, mapping_type: &str, source: &str, target: &str, account: Option<AccountId>) -> ImportMappingId;

    /// Start a transaction: status Cleared, reality, no category/contexts/memo.
    pub fn tx(&mut self, account: AccountId, date: &str, amount: &str, payee: &str) -> TxBuilder<'_>;
    /// Transaction with account_id = None (orphan).
    pub fn orphan_tx(&mut self, date: &str, amount: &str, payee: &str) -> TxBuilder<'_>;

    pub fn obligation(&mut self, name: &str) -> ObligationBuilder<'_>;
    pub fn budget_item(&mut self, t: BudgetItemType, name: &str, amount: &str, start: &str, f: BudgetFrequency) -> BudgetItemBuilder<'_>;

    /// Soft-delete any row (sets is_deleted = true). Panics for ImportMapping / Setting.
    pub fn delete(&mut self, key: RowKey);
    pub fn db(&self) -> &Db;
    pub fn build(self) -> Db;
}

pub struct TxBuilder<'a> { /* &mut TestDb + Transaction */ }
impl<'a> TxBuilder<'a> {
    pub fn status(self, s: TransactionStatus) -> Self;
    pub fn scenario(self, s: ScenarioId) -> Self;
    pub fn category(self, c: CategoryId) -> Self;
    pub fn contexts(self, c: &[ContextId]) -> Self;
    pub fn memo(self, memo: &str) -> Self;
    pub fn attachment(self, path: &str) -> Self;
    pub fn deleted(self) -> Self;
    pub fn done(self) -> TransactionId;          // inserts the row
}

pub struct ObligationBuilder<'a> { /* … */ }
impl<'a> ObligationBuilder<'a> {
    pub fn account(self, a: AccountId) -> Self;
    pub fn context(self, c: ContextId) -> Self;
    pub fn status(self, s: ObligationStatus) -> Self;
    pub fn balance(self, amount: &str) -> Self;      // current_balance
    pub fn target(self, amount: &str) -> Self;
    pub fn due(self, date: &str) -> Self;
    pub fn done(self) -> ObligationId;
}

pub struct BudgetItemBuilder<'a> { /* … */ }
impl<'a> BudgetItemBuilder<'a> {
    pub fn end(self, date: &str) -> Self;
    pub fn months(self, months: &[u32]) -> Self;
    pub fn category(self, c: CategoryId) -> Self;
    pub fn account(self, a: AccountId) -> Self;
    pub fn notes(self, n: &str) -> Self;
    pub fn done(self) -> BudgetItemId;
}
```

Usage (this exact snippet must compile in a test):
```rust
use pf_model::{*, testing::*};
use TransactionStatus::*;
let mut t = TestDb::new();
let chk = t.account("Checking", AccountType::Checking);
let s   = t.scenario("Plan A");
t.tx(chk, "2026-01-05", "-12.50", "Coffee").status(Cleared).done();
t.tx(chk, "2026-02-01", "-100", "Rent").scenario(s).status(Future).done();
let db = t.build();
assert_eq!(db.transactions.len(), 2);
```

## Steps

1. Edit `crates/pf-model/Cargo.toml` (features), then add the `test-support` dev-dependency line to the other five library crates.
2. Write `ids.rs` and `enums.rs`, and their tests.
3. Write `entities.rs` and `record.rs`.
4. Write `db.rs`. `get`/`put`/`remove` are a `match` on `RowKey`/`Record` over the 9 tables.
   `alloc_id`: `let id = self.next_id.max(1); self.next_id = id + 1; id`.
5. Write `testing.rs`; every builder method allocates ids with `db.alloc_id()`.
6. Re-export from `lib.rs`: `pub use ids::*; pub use enums::*; pub use entities::*; pub use record::*; pub use db::Db;`
   and `#[cfg(any(test, feature = "test-support"))] pub mod testing;`.
7. Delete the placeholder `scaffold_compiles` test.

## Tests (required)

| Test name | Covers | What it checks |
|---|---|---|
| `enum_strings_round_trip` | – | for every enum and variant: `parse(as_str()) == variant` |
| `enum_defaults_match_swift` | – | defaults: Checking, Expense, Cleared, Funding, Expense (BudgetItemType), Monthly, PastMonth |
| `enum_parse_is_exact` | – | `"Checking".parse::<AccountType>()` is Err, and `"funding".parse::<ObligationStatus>()` is Err |
| `parse_or_default_falls_back` | – | `AccountType::parse_or_default("bogus") == Checking`; `TransactionStatus::parse_or_default("") == Cleared` |
| `every_other_month_lenient_quirk` | – | the lenient cases listed above |
| `budget_item_type_lenient` | – | `"bill"`, `"BILL"`, `"Income"` parse; `"bills"` → None |
| `labels_match_swift` | – | `TransactionStatus::Reconciled.label() == "Reconciled"`, `BudgetFrequency::EveryOtherMonth.as_str() == "Every Other Month"`, `TimePeriod::YearToDate.as_str() == "Year to Date"` |
| `alloc_id_is_monotonic_and_shared` | – | allocating across tables gives 1,2,3…; `set_next_id(50)` then gives 50 |
| `get_put_remove_every_table` | – | for each `Record` variant: put → get returns an equal record; remove → get is None |
| `change_inverted_swaps` | – | `inverted()` swaps before and after, and the key is unchanged |
| `r_cat_4_full_name_with_parent` | R-CAT-4 | child of "Housing" named "Utilities" gives `"Housing/Utilities"` |
| `r_cat_4_full_name_without_parent` | R-CAT-4 | a top-level category's full name is its plain name; an unknown id gives `""` |
| `live_iterators_skip_deleted` | – | a deleted account/transaction/category is not yielded |
| `by_name_ci_matches_case_insensitive_live_only` | – | `account_by_name_ci("checking")` finds "Checking"; a deleted one is not found |
| `sorted_helpers_sort_by_name_then_id` | – | two accounts "B", "A", then "A" again: order A(id2), A(id3), B |
| `transactions_for_account_live_only` | – | excludes deleted and other accounts' rows |
| `testdb_example_compiles_and_counts` | – | the usage snippet above |
| `testdb_builders_set_every_field` | – | tx with memo/contexts/category/scenario/attachment/deleted; obligation; budget item; mapping; setting: all fields as given |

## Pitfalls

- `jiff::civil::Date` parses with `"2026-01-05".parse::<jiff::civil::Date>()`.
- `Decimal::from_str("-12.50")` keeps scale 2. That is fine and expected; do not normalise here.
- Swift byte-wise vs localized sort: we use **byte-wise** (`a.name.cmp(&b.name)`) with id tie-break.
  Document it with a comment, because SwiftData's sort may be locale-aware for non-ASCII names.
- `impl Iterator` returns borrow `self`; give them explicit lifetimes if the compiler asks (`+ '_`).
- Do not put business logic (balances, filters) here. That is pf-core (T03).
- `#![cfg]` gating: the `testing` module must compile both in pf-model's own tests and when another crate enables `test-support`.

## Done when

```bash
cargo test -p pf-model
cargo test --workspace            # other crates still build with the new dev-dependency
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules | grep R-CAT-4  # no output: R-CAT-4 is covered (if T00b is done)
```

## Out of scope

- SQLite (T07), Markdown (T05), any computation of balances (T03).
