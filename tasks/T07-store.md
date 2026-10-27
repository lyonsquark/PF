# T07: pf-store (SQLite schema, load, atomic apply, purge)

**Phase:** 1 · **Deps:** T01 · **Crate(s):** `pf-store` · **Size:** medium (about 550 lines of code, 350 of tests)

## Goal

Persist the in-memory `Db` to one SQLite file. The engine (T09) loads everything at startup with
`load_all`, writes every undo step with `apply_changes` (one SQL transaction per step), and purges
soft-deleted rows at launch. The store knows nothing about business rules.

## Read first

- `docs/data-model.md` → "SQLite schema" and "Settings keys" (the schema is the spec: copy it exactly)
- `PLAN.md` → "Engine (pf-engine): the core contract"
- `crates/pf-model/src/record.rs` (`Record`, `RowKey`, `Change`, from T01). **These types are owned by
  pf-model**, so pf-store uses them directly; pf-engine re-exports them.
- Rules: R-PRG-1, R-UND-6
- `docs/testing.md` → L2
- Swift: `Services/UndoService.swift` → `purgeSoftDeleted`; `PFApp.swift` (fixed DB path idea)

## Deliverables

```
crates/pf-store/src/
├── lib.rs        pub use store::{Store, StoreError, PurgeReport, default_db_path};
├── schema.rs     const SCHEMA_V1: &str (the CREATE statements from data-model.md) + migrate()
├── store.rs      Store
├── rows.rs       row ↔ struct conversion per table (read_account(row), write_account(tx, &Account), …)
└── codec.rs      Decimal/Date/Timestamp/JSON/bool helpers
crates/pf-store/tests/store.rs
```

```rust
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")] Sqlite(#[from] rusqlite::Error),
    #[error("corrupt value in {table}.{column}: {value:?}")] Corrupt { table: &'static str, column: &'static str, value: String },
    #[error("io error: {0}")] Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PurgeReport { pub transactions: usize, pub accounts: usize, pub categories: usize, pub contexts: usize,
                         pub scenarios: usize, pub obligations: usize, pub budget_items: usize,
                         pub dangling_links: usize, pub nullified_refs: usize, pub orphan_mappings: usize }

pub struct Store { /* conn: rusqlite::Connection, path: Option<PathBuf> */ }

impl Store {
    /// Open (creating parent dirs and the file if needed), set PRAGMAs, create/migrate the schema.
    pub fn open(path: &Path) -> Result<Store, StoreError>;
    /// In-memory database (tests, `Engine::open_in_memory`).
    pub fn open_in_memory() -> Result<Store, StoreError>;
    pub fn path(&self) -> Option<&Path>;
    /// Read every table into a Db. Sets db.next_id = max(meta.next_id, db.max_id() + 1).
    pub fn load_all(&self) -> Result<Db, StoreError>;
    /// R-UND-6: apply all changes in ONE SQL transaction and store `next_id` in meta.
    /// For each change: after = Some(r) → upsert r; after = None → delete the row with that key.
    /// (`before` is ignored by the store; it exists for undo.) On any error the transaction rolls back.
    pub fn apply_changes(&mut self, changes: &[Change], next_id: Id) -> Result<(), StoreError>;
    /// R-PRG-1: hard-delete soft-deleted rows, clean up references (see below). One SQL transaction.
    pub fn purge_soft_deleted(&mut self) -> Result<PurgeReport, StoreError>;
    /// Hard-delete everything in every table (Settings → Import Backup wipes first, R-MD-5). Keeps meta.
    pub fn wipe_all(&mut self) -> Result<(), StoreError>;
    /// Replace the whole database with `db` (wipe + insert every row + meta next_id) in ONE SQL
    /// transaction. Backs `Engine::replace_db` (Markdown import, R-MD-5). On error nothing changes.
    pub fn replace_all(&mut self, db: &Db) -> Result<(), StoreError>;
    pub fn schema_version(&self) -> Result<i64, StoreError>;
    pub fn journal_mode(&self) -> Result<String, StoreError>;
    /// Test hook: run raw SQL (e.g. to install a failing trigger). Not for production code.
    #[doc(hidden)]
    pub fn execute_batch_for_test(&self, sql: &str) -> Result<(), StoreError>;
}

/// `$HOME/Library/Application Support/PF/PFRust.sqlite`. Uses the HOME env var (no extra crates).
/// NEVER returns the Swift app's PF.store path.
pub fn default_db_path() -> PathBuf;
```

## Encoding (codec.rs)

| Rust | SQLite |
|---|---|
| `Decimal` | TEXT via `to_string()` (keeps scale, e.g. "-12.50"); read with `Decimal::from_str` |
| `Option<Decimal>` | TEXT or NULL |
| `jiff::civil::Date` | TEXT `YYYY-MM-DD` (`Date::to_string()`); read with `.parse::<Date>()` |
| `jiff::Timestamp` | TEXT (`Timestamp::to_string()`); read with `.parse::<Timestamp>()` |
| `bool` | INTEGER 0/1 |
| enums | TEXT `as_str()`; read with `parse_or_default` (never fail on an unknown enum value) |
| `Vec<String>` (match_patterns), `Vec<u32>` (specific_months) | TEXT JSON (`serde_json`) |
| ids | INTEGER |
| `Transaction.context_ids` | rows in `transaction_contexts (transaction_id, context_id, position)` |

A value that cannot be decoded (bad Decimal or date text) returns `StoreError::Corrupt` naming table and column.

## Behaviour details

* **PRAGMAs** on open: `journal_mode = WAL` (file DBs only; in-memory returns "memory"), `foreign_keys = OFF`,
  `synchronous = NORMAL`.
* **Schema:** `meta(schema_version)` = 1 is written on first open. Opening an existing v1 DB is a no-op. An unknown
  higher version → `Corrupt { table: "meta", column: "schema_version", … }`.
* **Upsert** = `INSERT OR REPLACE` with every column. For a `Transaction`, also
  `DELETE FROM transaction_contexts WHERE transaction_id = ?` then insert one row per context id with `position` = index.
  Deleting a transaction row also deletes its `transaction_contexts`.
* **Settings** use the `settings` table; `RowKey::Setting(key)`.
* **load_all** reads `transaction_contexts ORDER BY transaction_id, position` in one query and attaches the ids.
  Use `prepare_cached`; do not run one query per transaction (perf target: 100k transactions in < 500 ms in release, T12b).
* **next_id:** `apply_changes` writes `meta('next_id', next_id)` in the same transaction.
* **Purge (R-PRG-1)**, in one transaction, in this order:
  1. `DELETE FROM <table> WHERE is_deleted = 1` for transactions, budget_items, obligations, accounts, categories, contexts, scenarios.
  2. `DELETE FROM transaction_contexts` where the transaction or context no longer exists → `dangling_links`.
  3. Nullify dangling references (Swift `.nullify` delete rules) → `nullified_refs`:
     `transactions.category_id`, `budget_items.category_id`, `budget_items.account_id`,
     `obligations.account_id`, `obligations.context_id`, `categories.parent_id`.
  4. `DELETE FROM import_mappings WHERE account_id IS NOT NULL AND account_id NOT IN (SELECT id FROM accounts)` → `orphan_mappings`.
     This is **(fix)**: Swift would nullify, which silently turns a bank's mappings into global Monarch mappings.
  Transactions whose account or scenario is gone are left alone. They were soft-deleted together with their
  parent and so are already purged in step 1; any that remain are "orphans" and the UI handles them (R-TXN-15).

## Steps

1. `codec.rs` + unit tests (round-trip each type, corrupt-value errors).
2. `schema.rs`: paste the SQL from `docs/data-model.md`, plus `INSERT OR IGNORE INTO meta VALUES('schema_version','1')`.
3. `rows.rs`: one `read_*`/`write_*` pair per table. Keep the column order identical to the CREATE statement.
4. `store.rs`: `open`, `load_all`, `apply_changes`, `purge_soft_deleted`, `wipe_all`.
5. `tests/store.rs`: integration tests below, using `tempfile::tempdir()` and `pf_model::testing::TestDb`.
6. Delete the placeholder test.

## Tests (required)

| Test name | Covers | What it checks |
|---|---|---|
| `codec_decimal_keeps_scale` | – | "-12.50" round-trips as "-12.50" |
| `codec_date_and_timestamp_round_trip` | – | |
| `codec_corrupt_decimal_errors` | – | "abc" → `StoreError::Corrupt` with table and column |
| `save_then_load_round_trips_every_entity_type` | R-UND-6 | build a TestDb with every entity type, all optional fields set AND unset, context order [c2, c1]; apply all rows as inserts; reopen the file; `load_all() == original` (compare the whole `Db`) |
| `schema_creation_is_idempotent` | – | open twice on the same file; version 1; no error |
| `unknown_schema_version_errors` | – | set meta to 99 via the test hook → open fails with Corrupt |
| `apply_changes_is_atomic` | R-UND-6 | install `CREATE TRIGGER boom BEFORE INSERT ON scenarios BEGIN SELECT RAISE(ABORT,'boom'); END;`; apply [insert account, insert scenario] → Err; reload → no account |
| `apply_changes_delete_removes_row_and_links` | – | a change with after=None removes the transaction and its transaction_contexts rows |
| `update_replaces_contexts` | – | update tx contexts [a,b] → [b]; reload → [b] |
| `settings_round_trip_and_delete` | – | |
| `next_id_survives_restart` | – | apply with next_id 42 → reopen → `db.next_id() == 42`; also max(id)+1 wins if larger |
| `wal_mode_enabled` | – | `journal_mode() == "wal"` for a file DB |
| `r_prg_1_purge_removes_soft_deleted` | R-PRG-1 | deleted rows of every type gone after purge; live rows untouched; report counts correct |
| `r_prg_1_purge_removes_dangling_links` | R-PRG-1 | a live tx tagged with a deleted context loses that link |
| `r_prg_1_purge_nullifies_references` | R-PRG-1 | a live tx whose category was deleted gets category_id None; same for budget item category/account, obligation account/context, child category parent |
| `r_prg_1_purge_drops_orphan_bank_mappings` | R-PRG-1 | a mapping for a purged account is deleted; global mappings are kept |
| `wipe_all_empties_tables` | – | |
| `replace_all_is_atomic_and_complete` | R-MD-5 | replace with a fixture Db ⇒ load_all equals it; with an injected failing trigger ⇒ old data intact |
| `default_db_path_is_pfrust_sqlite` | – | ends with `Library/Application Support/PF/PFRust.sqlite` and never `PF.store` |
| `load_100k_transactions_fast` | – | `#[ignore]` perf: insert 100k rows, `load_all` < 500 ms (release) |

## Pitfalls

- `rusqlite` 0.41: use `conn.transaction()?` (needs `&mut self`) and `tx.commit()?`; parameters with `rusqlite::params![…]`.
- `journal_mode` PRAGMA returns a row: `conn.query_row("PRAGMA journal_mode=WAL", [], |r| r.get::<_, String>(0))`.
- `INSERT OR REPLACE` deletes and reinserts, which is fine because foreign keys are off.
- Booleans: read with `r.get::<_, i64>(i)? != 0`.
- Never panic on bad data in the file; return `StoreError`.
- `default_db_path()` must not create anything; `open` creates the parent directory.
- Do not depend on pf-engine (it is above pf-store in the dependency graph).

## Done when

```bash
cargo test -p pf-store
cargo test --release -p pf-store -- --ignored load_100k       # < 500 ms
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules | grep R-PRG-1                                # no output
```

## Out of scope

- Undo stacks, diffing, `Writer` (T09). Markdown (T05). Migrating from the Swift store (never).
