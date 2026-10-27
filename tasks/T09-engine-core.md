# T09: Engine core: `Engine`, `Writer`, undo/redo, persistence, `World` test helper

**Phase:** 2 · **Deps:** T03, T07 · **Crate(s):** `pf-engine` · **Size:** large (about 500 lines of code plus 400 lines of tests)

## Goal

Build the one and only way to change data: `Engine::apply(name, |w| …)`. It records
before-images of every row touched, persists the diff to SQLite in one SQL transaction, and
pushes one undo `Step`. Undo/redo replay those images. After this task every later task
(commands, CLI, API, UI) has a working, tested, persistent, undoable core, plus the
`World` helper that all behaviour tests use.

## Read first

- `PLAN.md`: "Engine (pf-engine): the core contract", plus the Threading notes
- `docs/data-model.md`: the whole file (the `Db`, ids, settings keys)
- `docs/business-rules.md` §7 (R-UND-1…6), R-PRG-1, R-SCN-2
- `docs/testing.md`: L2, L3, the BEH-UND/BEH-PRG rows
- Swift: `../PFApp/Sources/PFApp/Services/UndoService.swift` (the whole file: why undo is
  "before/after images" and why purge only happens at launch), `Managers/ScenarioManager.swift`
- Code from earlier tasks: `crates/pf-model/src/` (T01: `Db`, `Record`, `RowKey`, `Change`, ids, `TestDb`),
  `crates/pf-store/src/` (T07: `Store`), `crates/pf-core/src/markdown*` (T05: import), `crates/pf-core/src/balance*` (T03)

> **Name drift:** this file uses the names given in PLAN.md and data-model.md. If T01/T05/T07
> chose slightly different names (e.g. `Store::load_all` rather than `load`),
> **use theirs** and do not rename their code. If something you need is missing (e.g.
> a `Store::replace_all`), add it to that crate as a small, additive, tested change and note it in STATUS.md.

## Deliverables

```
crates/pf-engine/src/
  lib.rs          pub mod engine; pub mod writer; pub mod state; pub mod error; pub mod commands; pub mod testing;
                  pub use engine::*; pub use writer::Writer; pub use state::AppState; pub use error::*;
                  pub use pf_model::{Record, RowKey, Change};          // re-export
  error.rs
  state.rs
  writer.rs
  engine.rs
  commands/mod.rs  (empty modules for T10a–c: accounts, transactions, scenarios, obligations,
                    contexts, categories, budget, import, backup; each file holds only `//! T10x`)
  testing.rs       World helper (always compiled, used by pf-api, pf-ui and pf-cli tests too)
crates/pf-engine/tests/
  engine_undo.rs
  behaviour_undo.rs   (BEH-UND-02, BEH-PRG-01; BEH-UND-01 is in T10c)
fixtures/world.md      (create it if it does not exist yet, content below)
```

Add `tempfile = { workspace = true }` to pf-engine `[dependencies]` (not only dev-deps):
`testing::World` uses it and other crates' tests depend on it. It is already a workspace dependency.

### error.rs

```rust
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EngineError {
    #[error("This transaction is locked in the current scenario")]
    Locked,
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("{0}")]
    Validation(String),          // the message is shown to the user verbatim
    #[error("Storage error: {0}")]
    Store(String),
}
pub type Result<T> = std::result::Result<T, EngineError>;
// impl From<pf_store::StoreError> for EngineError  -> Store(e.to_string())
```

### state.rs

```rust
/// UI-level state. Not undoable, not part of Db.
#[derive(Debug, Clone, PartialEq)]
pub struct AppState {
    pub active_scenario: Option<ScenarioId>,  // None = Reality
    pub time_period: TimePeriod,              // default PastMonth (Swift default)
}
impl Default for AppState { /* None, TimePeriod::PastMonth */ }
```

### writer.rs

`Writer` is a write handle over `&mut Db` that remembers the **first** before-image of every row it touches.

```rust
pub struct Writer<'a> {
    db: &'a mut Db,
    touched: Vec<(RowKey, Option<Record>)>,   // insertion order; first touch only
    today: Date,
}
impl<'a> Writer<'a> {
    pub(crate) fn new(db: &'a mut Db, today: Date) -> Self;
    pub fn db(&self) -> &Db;                  // read access inside a command
    pub fn today(&self) -> Date;

    // inserts: the `id` field of the passed struct is ignored; a fresh id is allocated (Db::alloc_id)
    pub fn insert_account(&mut self, a: Account) -> AccountId;
    pub fn insert_transaction(&mut self, t: Transaction) -> TransactionId;
    pub fn insert_category(&mut self, c: Category) -> CategoryId;
    pub fn insert_context(&mut self, c: Context) -> ContextId;
    pub fn insert_scenario(&mut self, s: Scenario) -> ScenarioId;
    pub fn insert_obligation(&mut self, o: Obligation) -> ObligationId;
    pub fn insert_budget_item(&mut self, b: BudgetItem) -> BudgetItemId;
    pub fn insert_import_mapping(&mut self, m: ImportMapping) -> ImportMappingId;

    // updates: Err(NotFound("transaction 42")) if the id does not exist (deleted rows CAN be updated)
    pub fn update_account(&mut self, id: AccountId, f: impl FnOnce(&mut Account)) -> Result<()>;
    pub fn update_transaction(&mut self, id: TransactionId, f: impl FnOnce(&mut Transaction)) -> Result<()>;
    pub fn update_category(&mut self, id: CategoryId, f: impl FnOnce(&mut Category)) -> Result<()>;
    pub fn update_context(&mut self, id: ContextId, f: impl FnOnce(&mut Context)) -> Result<()>;
    pub fn update_scenario(&mut self, id: ScenarioId, f: impl FnOnce(&mut Scenario)) -> Result<()>;
    pub fn update_obligation(&mut self, id: ObligationId, f: impl FnOnce(&mut Obligation)) -> Result<()>;
    pub fn update_budget_item(&mut self, id: BudgetItemId, f: impl FnOnce(&mut BudgetItem)) -> Result<()>;
    pub fn update_import_mapping(&mut self, id: ImportMappingId, f: impl FnOnce(&mut ImportMapping)) -> Result<()>;

    /// Soft delete / restore. Err(Validation) for ImportMapping and Setting keys (they have no flag).
    pub fn set_deleted(&mut self, key: RowKey, deleted: bool) -> Result<()>;
    /// Remove the row entirely (ImportMappings, wipe). Undo re-inserts it with the same id.
    pub fn delete_hard(&mut self, key: RowKey) -> Result<()>;
    pub fn set_setting(&mut self, key: &str, value: &str);
    pub fn remove_setting(&mut self, key: &str);

    /// Consumes the writer: returns the changes where before != after, in touch order.
    pub(crate) fn finish(self) -> Vec<Change>;
    /// Restores every touched row to its before-image (used when the closure returns Err).
    pub(crate) fn rollback(self);
}
```

Implementation: every mutating method first calls `self.touch(key)`. If `key` is not yet in
`touched`, it pushes `(key.clone(), self.db.get(&key))`. `Db::get(&RowKey) -> Option<Record>`, `Db::put(Record)` and
`Db::remove(&RowKey)` come from T01 (`crates/pf-model/src/db.rs`); ids come from `Db::alloc_id()`. `finish()` builds
`Change { key, before, after: db.get(&key) }` and drops entries where `before == after`.
`touched` lookups may be linear (commands touch few rows) **except** in bulk commands. Use a
`HashMap<RowKey, usize>` index next to the Vec so that projections touching 10k rows stay O(n).
`RowKey` derives `Hash, Eq` (T01).

### engine.rs

```rust
pub struct Step { pub name: String, pub changes: Vec<Change> }

pub struct Engine {
    db: Db,
    store: Store,
    undo: Vec<Step>,
    redo: Vec<Step>,
    pub state: AppState,
    revision: u64,
    clock: Box<dyn Fn() -> Date + Send>,
}

pub type SharedEngine = std::sync::Arc<std::sync::Mutex<Engine>>;

impl Engine {
    /// `Store::open` → `store.purge_soft_deleted()` (R-PRG-1, log the PurgeReport) → `store.load_all()`;
    /// restore AppState.time_period
    /// from setting `selectedTimePeriod`. Undo history is empty (R-UND-4).
    pub fn open(path: &Path) -> Result<Engine>;
    pub fn open_in_memory() -> Result<Engine>;
    /// Delegates to `pf_store::default_db_path()` (~/Library/Application Support/PF/PFRust.sqlite).
    pub fn default_db_path() -> std::path::PathBuf;
    /// The SQLite file path (None for in-memory). Used by `/api/db-path` and `pf-cli db-path`.
    pub fn db_path(&self) -> Option<&Path>;
    pub fn db(&self) -> &Db;
    pub fn revision(&self) -> u64;
    pub fn today(&self) -> Date;
    pub fn set_clock(&mut self, clock: Box<dyn Fn() -> Date + Send>);   // tests: fixed day
    pub fn shared(self) -> SharedEngine;                                // Arc::new(Mutex::new(self))

    pub fn apply<T>(&mut self, name: &str, f: impl FnOnce(&mut Writer) -> Result<T>) -> Result<T>;
    pub fn undo(&mut self) -> Result<Option<String>>;
    pub fn redo(&mut self) -> Result<Option<String>>;
    pub fn undo_name(&self) -> Option<&str>;
    pub fn redo_name(&self) -> Option<&str>;
    pub fn can_undo(&self) -> bool;
    pub fn can_redo(&self) -> bool;
    pub fn clear_history(&mut self);

    /// Replace the whole Db (Markdown wipe+import, Danger Zone wipe). Persists via `Store::replace_all`
    /// (ADD it to pf-store in this task: wipe every table and insert every record of `db` plus next_id,
    /// in ONE SQL transaction, tested by `replace_all_is_atomic` in pf-store), clears history,
    /// resets active_scenario to None, bumps revision. On error the old Db stays in memory and on disk.
    pub fn replace_db(&mut self, db: Db) -> Result<()>;

    /// R-SCN-2: same id ⇒ Reality; different/None ⇒ switch. Not undoable. Bumps revision.
    pub fn toggle_scenario(&mut self, id: Option<ScenarioId>);
    pub fn set_active_scenario(&mut self, id: Option<ScenarioId>);
    /// Persists setting `selectedTimePeriod` directly (NOT an undo step).
    pub fn set_time_period(&mut self, p: TimePeriod) -> Result<()>;
    /// Writes a UI/preference setting (backupFolder, sidebarGrouping, apiPort…) to Db + SQLite
    /// directly: NOT an undo step, and bumps revision. Use `Writer::set_setting` instead when the
    /// setting is part of a user action that must undo together (e.g. lastProjection*).
    pub fn set_setting_direct(&mut self, key: &str, value: &str) -> Result<()>;
    pub fn setting(&self, key: &str) -> Option<&str>;
}
```

`apply` algorithm (R-UND-1, R-UND-3, R-UND-6):

1. `let mut w = Writer::new(&mut self.db, self.today());`
2. `match f(&mut w)`:
   * `Err(e)` ⇒ `w.rollback()`; return `Err(e)`. No step, no revision bump.
   * `Ok(v)` ⇒ `let changes = w.finish();`
3. If `changes.is_empty()` ⇒ return `Ok(v)`: no step and no revision bump (R-TXN-5 relies on this).
4. `self.store.apply_changes(&changes, self.db.next_id())` (T07 signature). On `Err`: restore every change's
   `before` into `self.db` (reverse order) and return `Err(Store)`. The in-memory Db must never
   diverge from disk.
5. Push `Step { name, changes }`, `self.redo.clear()`, `self.revision += 1`, run `fix_state()`.

`undo()`: pop the step (None ⇒ `Ok(None)`). For each change in **reverse**, put `before`
(or remove if None). Persist the inverted changes (`before`/`after` swapped) in one SQL
transaction; on store error, re-apply `after` and return Err without moving the step. Push to `redo`.
Bump revision, `fix_state()`, return the name. `redo()` is the mirror: forward order, `after`.

`fix_state()`: if `state.active_scenario` points to a scenario that is missing or deleted,
set it to `None` (covers deleting the active scenario and undoing its creation).

## Steps

1. Write `error.rs`, `state.rs`, `commands/mod.rs` with empty modules.
2. Write `tests/engine_undo.rs` first (table below) using `Engine::open_in_memory()` and raw
   `apply` closures. They fail to compile; that is expected.
3. Implement `writer.rs`, then `engine.rs`. Get the tests green.
4. Create `fixtures/world.md` with exactly the content below. (It is separate from T05's `fixtures/small.md`
   on purpose: every number in the engine, CLI and API tests is computed from `world.md`. Do not merge them.)
5. Implement `testing.rs` (World). Write `tests/behaviour_undo.rs`.
6. Run the Done-when commands.

### testing.rs: `World` (used by every behaviour, API, CLI and UI test)

```rust
pub const TEST_TODAY: Date = jiff::civil::date(2026, 3, 15);
pub fn test_tz() -> jiff::tz::TimeZone;            // TimeZone::get("America/New_York")

pub struct World {
    pub engine: Engine,
    dir: tempfile::TempDir,                         // keeps the SQLite file alive
    path: PathBuf,
}
impl World {
    pub fn new() -> World;                          // empty Db, temp file, clock fixed to TEST_TODAY
    /// Loads PFRust/fixtures/{name}.md using pf-core's Markdown import with test_tz().
    /// Locate fixtures via env!("CARGO_MANIFEST_DIR") + "/../../fixtures". Clears undo history afterwards.
    pub fn with_fixture(name: &str) -> World;
    pub fn reload(&mut self);                       // drop Engine, Engine::open(same path), re-fix the clock
    pub fn into_engine(self) -> Engine;             // NOTE: leaks the TempDir on purpose (std::mem::forget) so the file lives on
    pub fn path(&self) -> &Path;

    // lookups (panic with a clear message if missing; this is test code)
    pub fn account(&self, name: &str) -> AccountId;
    pub fn category(&self, name: &str) -> CategoryId;   // by plain name
    pub fn context(&self, name: &str) -> ContextId;
    pub fn scenario(&self, name: &str) -> ScenarioId;
    pub fn obligation(&self, name: &str) -> ObligationId;
    pub fn budget_item(&self, name: &str) -> BudgetItemId;
    pub fn tx_by_payee(&self, payee: &str) -> TransactionId;   // first live match, panics if 0 or >1
    pub fn live_tx_count(&self) -> usize;

    // state
    pub fn activate_scenario(&mut self, name: &str);  // engine.set_active_scenario(Some(..))
    pub fn activate_reality(&mut self);

    // balances (call the T03 pf-core functions with engine.state.active_scenario)
    pub fn balance(&self, a: AccountId) -> Decimal;          // R-BAL-1
    pub fn cleared_balance(&self, a: AccountId) -> Decimal;  // R-BAL-2
    pub fn reality_balance(&self, a: AccountId) -> Decimal;  // R-BAL-3

    // thin wrappers that later tasks add as commands land; T09 only needs the above.
    // T10a adds (in this file): add_deposit, add_withdrawal, add_transfer(from,to,date,amount)
    //   with signature (acc, "YYYY-MM-DD", "amount", "payee") -> TransactionId
}
```

Helper: `pub fn d(s: &str) -> Date` and `pub fn m(s: &str) -> Decimal` in `testing` for terse tests.

### fixtures/world.md (exact content)

Dates are New York local midnight in UTC (EST = `T05:00:00Z` before 2026-03-08, EDT = `T04:00:00Z` from 2026-03-08).

```markdown
# Personal Finance Data

## Contexts
| Name | Color | Notes |
| --- | --- | --- |
| Taxes | #FF4500 | Property tax |
| Vacation | #FFD700 | Hawaii |

## Categories
| Name | Type | Icon | Notes | Parent | Match Patterns |
| --- | --- | --- | --- | --- | --- |
| Dining | expense | 🍽️ |  |  | Starbucks;Cafe |
| Groceries | expense |  |  |  | Whole Foods |
| Rent | expense |  |  |  |  |
| Salary | income |  |  |  | Acme Payroll |
| Transfer | transfer |  |  |  |  |
| Utilities | expense |  |  | Rent |  |

## Scenarios
| Name | Color | Notes |
| --- | --- | --- |
| Plan A | #00AA00 | Test plan |

## Accounts
| Name | Type | Institution | Currency | External |
| --- | --- | --- | --- | --- |
| Checking | checking | Chase | USD | false |
| External 401k | investment | Fidelity | USD | true |
| Mortgage | loan | Rocket | USD | false |
| Savings | savings | Ally | USD | false |
| Visa | credit | Chase | USD | false |

## Obligations
| Name | Status | Target | Balance | Due Date | Created Date | Updated Date | Notes | Account | Context |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Property Tax | Funding | 2400 | 1000 | 2026-10-15T04:00:00Z | 2026-01-01T05:00:00Z | 2026-01-01T05:00:00Z |  | Checking | Taxes |
| Vacation Fund | Completed | 2000 | 2000 |  | 2026-01-01T05:00:00Z | 2026-01-01T05:00:00Z |  | Savings | Vacation |

## BudgetItems
| Name | Type | Amount | Start Date | End Date | Frequency | Specific Months | Notes | Category | Account |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Acme Payroll | Income | 4000 | 2026-01-15T05:00:00Z |  | Monthly |  |  | Salary | Checking |
| Car Insurance | Bill | 450 | 2026-02-15T05:00:00Z |  | Quarterly |  |  |  | Checking |
| Groceries | Expense | 600 | 2026-01-31T05:00:00Z |  | Monthly |  |  | Groceries | Checking |
| Landlord | Bill | 1500 | 2026-01-01T05:00:00Z |  | Monthly |  |  | Rent | Checking |
| Property Tax | Bill | 1200 | 2026-04-30T04:00:00Z |  | Specific Months | 4,10 |  |  | Checking |

## ImportMappings

## Transactions
### Checking
| Date | Payee | Category | Amount | Memo | Scenario | Contexts | Status | Attachment |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-04-01T04:00:00Z | Landlord | Rent | -1500 | Auto-generated from Budget | Plan A |  | future |  |
| 2026-03-12T04:00:00Z | Transfer to Savings | Transfer | -500 |  |  |  | cleared |  |
| 2026-03-10T04:00:00Z | Whole Foods | Groceries | -142.3 |  |  |  | pending |  |
| 2026-02-10T05:00:00Z | County Tax Office |  | -300 |  |  | Taxes | cleared |  |
| 2026-02-01T05:00:00Z | Landlord | Rent | -1500 |  |  |  | cleared |  |
| 2026-01-20T05:00:00Z | Starbucks | Dining | -5.75 |  |  |  | cleared |  |
| 2026-01-15T05:00:00Z | Acme Payroll | Salary | 4000 |  |  |  | cleared |  |
| 2026-01-02T05:00:00Z | Landlord | Rent | -1500 |  |  |  | reconciled |  |
| 2025-12-31T05:00:00Z | Starting Balance |  | 2000 | Initial account balance |  |  | cleared |  |

### External 401k
| Date | Payee | Category | Amount | Memo | Scenario | Contexts | Status | Attachment |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-01-31T05:00:00Z | Contribution |  | 1000 |  |  |  | cleared |  |

### Savings
| Date | Payee | Category | Amount | Memo | Scenario | Contexts | Status | Attachment |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-03-12T04:00:00Z | Transfer from Checking | Transfer | 500 |  |  |  | cleared |  |
| 2026-01-01T05:00:00Z | Starting Balance |  | 10000 | Initial account balance |  |  | cleared |  |

### Visa
| Date | Payee | Category | Amount | Memo | Scenario | Contexts | Status | Attachment |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-02-14T05:00:00Z | Hotel Maui |  | -650 |  |  | Vacation | cleared |  |
| 2026-01-25T05:00:00Z | Cafe Luna |  | -18.4 |  |  |  | cleared |  |

```

### World fixture facts (use these numbers in tests; they were computed by hand, so re-check by hand if one disagrees)

| Fact | Value |
|---|---|
| Accounts / categories / contexts / scenarios / obligations / budget items | 5 / 6 / 2 / 1 / 2 / 5 |
| Live transactions | 14 (1 of them in Plan A) |
| Checking balance, Reality, all statuses | 2051.95 |
| Checking cleared balance, Reality | 2194.25 (excludes the pending −142.30) |
| Checking balance with Plan A active | 551.95 (cleared stays 2194.25) |
| Savings | 10500 · Visa −668.40 · External 401k 1000 (external) · Mortgage 0 |
| Net worth (cleared, non-external) | 12025.85 |
| Assets / Credit debt / Loan debt (cleared) | 12694.25 / −668.40 / 0 |
| Property Tax computed balance | 1000 + (−300) = 700; Checking reserved = 700 |
| Vacation Fund computed balance | 2000 + (−650) = 1350, but Completed ⇒ not reserved |
| Uncategorized live tx | County Tax Office, Starting Balance ×2, Contribution, Hotel Maui, Cafe Luna (6); "Cafe Luna" matches Dining's `Cafe` |

## Tests (required)

`tests/engine_undo.rs`:

| Test name | Covers | What it checks |
|---|---|---|
| `r_und_1_apply_creates_one_named_step` | R-UND-1 | an apply inserting 3 rows ⇒ `undo_name() == Some("Add Things")`, one step |
| `r_und_2_undo_restores_exact_prior_state` | R-UND-2 | snapshot `db.clone()`, apply an update + soft delete + insert, undo ⇒ `db == snapshot` (compare with `next_id` ignored) |
| `r_und_2_redo_reapplies_and_ping_pong_is_stable` | R-UND-2 | apply, undo, redo, undo, redo ⇒ final == after-apply state each time |
| `r_und_3_new_edit_clears_redo` | R-UND-3 | apply A, undo, apply B ⇒ `can_redo() == false` |
| `r_und_4_history_empty_after_open` | R-UND-4 | apply, reopen from same file ⇒ `can_undo() == false`, data still there |
| `r_und_5_undo_redo_names` | R-UND-5 | names reported for both stacks; None when empty |
| `r_und_6_apply_persists_before_return` | R-UND-6 | apply, then open a *second* Engine on the same file ⇒ sees the row |
| `r_und_6_undo_is_persisted` | R-UND-6 | apply, undo, reopen ⇒ row absent |
| `apply_with_no_changes_creates_no_step` | R-TXN-5 support | closure that sets a field to its current value ⇒ no step, revision unchanged |
| `apply_error_rolls_back_memory` | – | closure inserts a row then returns `Err(Validation)` ⇒ Db unchanged, no step |
| `store_failure_rolls_back_memory` | R-UND-6 | make the store fail (e.g. a test-only `Store::fail_next_write()` hook or a read-only file) ⇒ Err(Store), Db unchanged |
| `revision_bumps_on_apply_undo_redo` | – | +1 each |
| `r_scn_2_toggle_same_scenario_returns_to_reality` | R-SCN-2 | toggle(Some(s)) twice ⇒ None; toggle(Some(a)) then toggle(Some(b)) ⇒ b |
| `fix_state_clears_deleted_active_scenario` | R-SCN-3 support | active = s, apply set_deleted(s) ⇒ active None |
| `set_time_period_persists_without_undo` | – | set, reopen ⇒ same period; `can_undo()` false |
| `set_setting_direct_persists_without_undo` | – | set `backupFolder`, reopen ⇒ `setting("backupFolder")` same; `can_undo()` false |
| `bulk_apply_touching_10k_rows_is_fast` | – | 10k inserts in one apply complete in < 1 s even in debug |
| `world_fixture_facts` | – | every number in the "world fixture facts" table |
| `world_reload_keeps_data_and_clock` | – | after reload, `today() == TEST_TODAY` |
| `replace_db_swaps_everything_and_clears_history` | – | replace with a 1-account Db ⇒ reopen shows only it; `can_undo()` false; active scenario None |

`tests/behaviour_undo.rs`:

| Test name | Covers | Given / When / Then |
|---|---|---|
| `beh_und_02_new_edit_after_undo_clears_redo` | BEH-UND-02, R-UND-3 | **Given** world fixture · **When** soft-delete "Starbucks", undo, then rename "Landlord" rows via update · **Then** `can_redo()` is false and "Starbucks" is live |
| `beh_prg_01_soft_deleted_rows_purged_on_restart` | BEH-PRG-01, R-PRG-1 | **Given** world fixture · **When** soft-delete "Hotel Maui" (its context link to Vacation exists), `reload()` · **Then** the row is gone from Db (not just flagged), `transaction_contexts` has no dangling row (ask the store or count via a raw query helper), and `can_undo()` is false |

## Pitfalls

- **Never** let the in-memory Db and SQLite diverge: on any store error, roll memory back.
- `finish()` must compare `before != after` per row. A touched-but-unchanged row is not a change.
- Undo must apply changes in **reverse** order (insert of a child after its parent, etc.).
- `next_id` is never decremented on undo. Ids are never reused, so redo re-inserts the same id.
- `TimeZone::get("America/New_York")` needs the system tzdb, which is fine on macOS. Do not use `TimeZone::system()` in tests.
- Do not hold `MutexGuard<Engine>` in `World`; World owns the Engine directly.
- `into_engine` intentionally leaks the temp dir; it is test-only, so add a comment saying so.

## Done when

```bash
cargo test -p pf-engine
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules      # R-UND-*, R-SCN-2, BEH-UND-02, BEH-PRG-01 present
```

## Out of scope

- Business commands (T10a–c). Only raw `apply` closures in tests here.
- Threading and API wiring (T12/T13), beyond providing `SharedEngine`.
