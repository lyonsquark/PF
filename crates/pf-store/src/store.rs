//! `Store`: one SQLite file holding the whole `Db`. Writes are single SQL transactions (R-UND-6);
//! purge is R-PRG-1. The store knows no business rules beyond those two.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use pf_model::{Change, ContextId, Db, Id, TransactionId};
use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::codec;
use crate::rows::{
    delete_row, read_account, read_budget_item, read_category, read_context, read_import_mapping,
    read_obligation, read_scenario, read_transaction, write_record,
};
use crate::schema;

/// Errors from the SQLite store.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// Any failure reported by SQLite.
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// A stored value that cannot be decoded.
    #[error("corrupt value in {table}.{column}: {value:?}")]
    Corrupt {
        /// Table holding the bad value.
        table: &'static str,
        /// Column holding the bad value.
        column: &'static str,
        /// The raw stored text.
        value: String,
    },
    /// A file-system failure, e.g. creating the parent folder.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Rows removed or fixed by one purge (R-PRG-1).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PurgeReport {
    /// Soft-deleted transactions removed.
    pub transactions: usize,
    /// Soft-deleted accounts removed.
    pub accounts: usize,
    /// Soft-deleted categories removed.
    pub categories: usize,
    /// Soft-deleted contexts removed.
    pub contexts: usize,
    /// Soft-deleted scenarios removed.
    pub scenarios: usize,
    /// Soft-deleted obligations removed.
    pub obligations: usize,
    /// Soft-deleted budget items removed.
    pub budget_items: usize,
    /// Context links whose transaction or context no longer exists.
    pub dangling_links: usize,
    /// Foreign-key references set to NULL because their target was purged.
    pub nullified_refs: usize,
    /// Bank-specific import mappings removed because their account was purged.
    pub orphan_mappings: usize,
}

/// The SQLite database file (or an in-memory database) behind the engine.
pub struct Store {
    conn: Connection,
    path: Option<PathBuf>,
}

/// Table names purged by soft delete, in the order R-PRG-1 lists them.
const SOFT_DELETED_TABLES: [(&str, fn(&mut PurgeReport) -> &mut usize); 7] = [
    ("transactions", |r| &mut r.transactions),
    ("budget_items", |r| &mut r.budget_items),
    ("obligations", |r| &mut r.obligations),
    ("accounts", |r| &mut r.accounts),
    ("categories", |r| &mut r.categories),
    ("contexts", |r| &mut r.contexts),
    ("scenarios", |r| &mut r.scenarios),
];

/// Reference clean-ups from R-PRG-1 step 3 (Swift `.nullify` delete rules).
const NULLIFY_SQL: [&str; 6] = [
    "UPDATE transactions SET category_id = NULL WHERE category_id IS NOT NULL AND category_id NOT IN (SELECT id FROM categories)",
    "UPDATE budget_items SET category_id = NULL WHERE category_id IS NOT NULL AND category_id NOT IN (SELECT id FROM categories)",
    "UPDATE budget_items SET account_id = NULL WHERE account_id IS NOT NULL AND account_id NOT IN (SELECT id FROM accounts)",
    "UPDATE obligations SET account_id = NULL WHERE account_id IS NOT NULL AND account_id NOT IN (SELECT id FROM accounts)",
    "UPDATE obligations SET context_id = NULL WHERE context_id IS NOT NULL AND context_id NOT IN (SELECT id FROM contexts)",
    "UPDATE categories SET parent_id = NULL WHERE parent_id IS NOT NULL AND parent_id NOT IN (SELECT id FROM categories)",
];

/// Every data table, for wiping. `meta` is deliberately absent.
const DATA_TABLES: [&str; 10] = [
    "transaction_contexts",
    "transactions",
    "accounts",
    "categories",
    "contexts",
    "scenarios",
    "obligations",
    "budget_items",
    "import_mappings",
    "settings",
];

impl Store {
    /// Opens the file (creating parent folders and the file if needed), sets PRAGMAs, and
    /// creates or checks the schema. Fails with `Corrupt` on an unknown schema version.
    pub fn open(path: &Path) -> Result<Store, StoreError> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get::<_, String>(0))?;
        configure(&conn)?;
        schema::migrate(&conn)?;
        Ok(Store {
            conn,
            path: Some(path.to_path_buf()),
        })
    }

    /// An in-memory database, for tests and `Engine::open_in_memory`.
    pub fn open_in_memory() -> Result<Store, StoreError> {
        let conn = Connection::open_in_memory()?;
        configure(&conn)?;
        schema::migrate(&conn)?;
        Ok(Store { conn, path: None })
    }

    /// The file path, or `None` for an in-memory database.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Reads every table into a `Db`. The id sequence becomes max(stored next_id, max id + 1).
    pub fn load_all(&self) -> Result<Db, StoreError> {
        let c = &self.conn;
        let mut db = Db::new();
        for a in query_all(
            c,
            "SELECT id, name, account_type, institution, currency, is_external, is_deleted FROM accounts",
            read_account,
        )? {
            db.accounts.insert(a.id, a);
        }
        let mut links = load_context_links(c)?;
        for mut t in query_all(c, TRANSACTION_SQL, read_transaction)? {
            if let Some(ids) = links.remove(&t.id) {
                t.context_ids = ids;
            }
            db.transactions.insert(t.id, t);
        }
        for r in query_all(
            c,
            "SELECT id, name, icon, notes, match_patterns, category_type, parent_id, is_deleted FROM categories",
            read_category,
        )? {
            db.categories.insert(r.id, r);
        }
        for r in query_all(
            c,
            "SELECT id, name, color_hex, notes, is_archived, is_deleted FROM contexts",
            read_context,
        )? {
            db.contexts.insert(r.id, r);
        }
        for r in query_all(
            c,
            "SELECT id, name, color_hex, notes, is_deleted FROM scenarios",
            read_scenario,
        )? {
            db.scenarios.insert(r.id, r);
        }
        for r in query_all(c, OBLIGATION_SQL, read_obligation)? {
            db.obligations.insert(r.id, r);
        }
        for r in query_all(c, BUDGET_ITEM_SQL, read_budget_item)? {
            db.budget_items.insert(r.id, r);
        }
        for r in query_all(
            c,
            "SELECT id, mapping_type, source_value, target_value, negate_amounts, account_id FROM import_mappings",
            read_import_mapping,
        )? {
            db.import_mappings.insert(r.id, r);
        }
        for (key, value) in query_all(c, "SELECT key, value FROM settings", read_setting)? {
            db.settings.insert(key, value);
        }
        let stored = read_next_id(c)?.unwrap_or(0);
        db.set_next_id(stored.max(db.max_id() + 1));
        Ok(db)
    }

    /// Applies all changes in ONE SQL transaction and stores `next_id` in meta (R-UND-6).
    /// `after = Some(r)` upserts `r`; `after = None` deletes the row with `change.key`.
    /// `before` is ignored. If any change fails, nothing is written.
    pub fn apply_changes(&mut self, changes: &[Change], next_id: Id) -> Result<(), StoreError> {
        let tx = self.conn.transaction()?;
        for change in changes {
            match &change.after {
                Some(record) => {
                    if record.key() != change.key {
                        return Err(codec::corrupt(
                            "changes",
                            "key",
                            &format!("{:?}", change.key),
                        ));
                    }
                    write_record(&tx, record)?;
                }
                None => delete_row(&tx, &change.key)?,
            }
        }
        write_next_id(&tx, next_id)?;
        tx.commit()?;
        Ok(())
    }

    /// R-PRG-1: hard-deletes soft-deleted rows, then removes dangling context links, nulls
    /// references to purged rows, and drops bank mappings whose account is gone. One transaction.
    /// Transactions whose account or scenario is gone are left alone (R-TXN-15 handles them).
    pub fn purge_soft_deleted(&mut self) -> Result<PurgeReport, StoreError> {
        let tx = self.conn.transaction()?;
        let mut report = PurgeReport::default();
        for (table, field) in SOFT_DELETED_TABLES {
            let sql = format!("DELETE FROM {table} WHERE is_deleted = 1");
            *field(&mut report) = tx.execute(&sql, [])?;
        }
        report.dangling_links = tx.execute(
            "DELETE FROM transaction_contexts
             WHERE transaction_id NOT IN (SELECT id FROM transactions)
                OR context_id NOT IN (SELECT id FROM contexts)",
            [],
        )?;
        for sql in NULLIFY_SQL {
            report.nullified_refs += tx.execute(sql, [])?;
        }
        // (fix) Swift would nullify these, which turns a bank's mappings into global ones.
        report.orphan_mappings = tx.execute(
            "DELETE FROM import_mappings
             WHERE account_id IS NOT NULL AND account_id NOT IN (SELECT id FROM accounts)",
            [],
        )?;
        tx.commit()?;
        Ok(report)
    }

    /// Hard-deletes every data row. `meta` (schema version and next id) is kept.
    pub fn wipe_all(&mut self) -> Result<(), StoreError> {
        let tx = self.conn.transaction()?;
        wipe_tables(&tx)?;
        tx.commit()?;
        Ok(())
    }

    /// Replaces the whole database with `db` in ONE SQL transaction. On error nothing changes.
    pub fn replace_all(&mut self, db: &Db) -> Result<(), StoreError> {
        let tx = self.conn.transaction()?;
        wipe_tables(&tx)?;
        for record in db.all_records() {
            write_record(&tx, &record)?;
        }
        write_next_id(&tx, db.next_id())?;
        tx.commit()?;
        Ok(())
    }

    /// The schema version stored in meta (1 for this build).
    pub fn schema_version(&self) -> Result<i64, StoreError> {
        schema::read_schema_version(&self.conn)
    }

    /// The journal mode: "wal" for a file database, "memory" for an in-memory one.
    pub fn journal_mode(&self) -> Result<String, StoreError> {
        Ok(self
            .conn
            .query_row("PRAGMA journal_mode", [], |r| r.get::<_, String>(0))?)
    }

    /// Test hook: runs raw SQL (for example to install a failing trigger). Not for production code.
    #[doc(hidden)]
    pub fn execute_batch_for_test(&self, sql: &str) -> Result<(), StoreError> {
        Ok(self.conn.execute_batch(sql)?)
    }
}

/// The app's database file: `$HOME/Library/Application Support/PF/PF.sqlite`.
/// Never the Swift app's `PF.store`. Creates nothing.
pub fn default_db_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join("Library")
        .join("Application Support")
        .join("PF")
        .join("PF.sqlite")
}

/// The database's name before the app was renamed from PFRust to PF.
const LEGACY_DB_FILE: &str = "PFRust.sqlite";

/// The database under its old name next to `path`, when it still has to be moved to `path`: the old
/// file exists and `path` does not. An existing `path` is never replaced.
pub fn legacy_db_to_move(path: &Path) -> Option<PathBuf> {
    let legacy = path.parent()?.join(LEGACY_DB_FILE);
    (legacy.exists() && !path.exists()).then_some(legacy)
}

/// Moves the database from its old name to `path`, with its `-wal` and `-shm` files. Returns whether it
/// moved. The main file goes last, so an interrupted move simply continues on the next start.
pub fn move_legacy_db(path: &Path) -> std::io::Result<bool> {
    let Some(legacy) = legacy_db_to_move(path) else {
        return Ok(false);
    };
    for suffix in ["-wal", "-shm"] {
        let from = with_suffix(&legacy, suffix);
        if from.exists() {
            std::fs::rename(from, with_suffix(path, suffix))?;
        }
    }
    std::fs::rename(legacy, path)?;
    Ok(true)
}

/// `path` with `suffix` added to its file name.
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Connection PRAGMAs shared by file and in-memory databases.
fn configure(conn: &Connection) -> Result<(), StoreError> {
    conn.execute_batch("PRAGMA foreign_keys = OFF; PRAGMA synchronous = NORMAL;")?;
    Ok(())
}

/// Deletes every row of every data table, inside the caller's transaction.
fn wipe_tables(conn: &Connection) -> Result<(), StoreError> {
    for table in DATA_TABLES {
        conn.execute(&format!("DELETE FROM {table}"), [])?;
    }
    Ok(())
}

/// Stores the id sequence position in meta.
fn write_next_id(conn: &Connection, next_id: Id) -> Result<(), StoreError> {
    let text = next_id.to_string();
    conn.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES ('next_id', ?1)",
        params![text],
    )?;
    Ok(())
}

/// The stored id sequence position, if any.
fn read_next_id(conn: &Connection) -> Result<Option<Id>, StoreError> {
    let text: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key = 'next_id'", [], |r| {
            r.get(0)
        })
        .optional()?;
    text.map(|t| codec::int(&t, "meta", "next_id")).transpose()
}

/// Reads all rows of one query with a row reader.
fn query_all<T>(
    conn: &Connection,
    sql: &str,
    read: fn(&Row<'_>) -> Result<T, StoreError>,
) -> Result<Vec<T>, StoreError> {
    let mut stmt = conn.prepare_cached(sql)?;
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(read(row)?);
    }
    Ok(out)
}

/// Reads the whole `transaction_contexts` table in one query, grouped by transaction in list order.
fn load_context_links(
    conn: &Connection,
) -> Result<HashMap<TransactionId, Vec<ContextId>>, StoreError> {
    let mut stmt = conn.prepare_cached(
        "SELECT transaction_id, context_id FROM transaction_contexts ORDER BY transaction_id, position",
    )?;
    let mut rows = stmt.query([])?;
    let mut links: HashMap<TransactionId, Vec<ContextId>> = HashMap::new();
    while let Some(row) = rows.next()? {
        let tx: i64 = row.get(0)?;
        let ctx: i64 = row.get(1)?;
        links
            .entry(TransactionId(tx))
            .or_default()
            .push(ContextId(ctx));
    }
    Ok(links)
}

/// Reads one settings row.
fn read_setting(row: &Row<'_>) -> Result<(String, String), StoreError> {
    Ok((row.get(0)?, row.get(1)?))
}

const TRANSACTION_SQL: &str =
    "SELECT id, date, amount, payee, memo, attachment_path, status, is_deleted,
    account_id, scenario_id, category_id FROM transactions";

const OBLIGATION_SQL: &str =
    "SELECT id, name, target_amount, current_balance, due_date, notes, status,
    created_date, last_updated_date, account_id, context_id, is_deleted FROM obligations";

const BUDGET_ITEM_SQL: &str = "SELECT id, item_type, name, amount, start_date, end_date, frequency,
    specific_months, notes, category_id, account_id, is_deleted FROM budget_items";
