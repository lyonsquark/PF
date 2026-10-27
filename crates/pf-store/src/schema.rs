//! The v1 SQLite schema (copied from `docs/data-model.md`) and the version check run on every open.

use rusqlite::{Connection, params};

use crate::codec;
use crate::store::StoreError;

/// The schema version this build reads and writes.
pub(crate) const SCHEMA_VERSION: i64 = 1;

/// The v1 CREATE statements. Keep this identical to `docs/data-model.md`.
pub const SCHEMA_V1: &str = "
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);

CREATE TABLE IF NOT EXISTS accounts (
  id INTEGER PRIMARY KEY, name TEXT NOT NULL, account_type TEXT NOT NULL,
  institution TEXT NOT NULL, currency TEXT NOT NULL, is_external INTEGER NOT NULL,
  is_deleted INTEGER NOT NULL DEFAULT 0);

CREATE TABLE IF NOT EXISTS transactions (
  id INTEGER PRIMARY KEY, date TEXT NOT NULL, amount TEXT NOT NULL,
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
  match_patterns TEXT NOT NULL, category_type TEXT NOT NULL,
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
  specific_months TEXT NOT NULL, notes TEXT,
  category_id INTEGER, account_id INTEGER, is_deleted INTEGER NOT NULL DEFAULT 0);

CREATE TABLE IF NOT EXISTS import_mappings (
  id INTEGER PRIMARY KEY, mapping_type TEXT NOT NULL, source_value TEXT NOT NULL,
  target_value TEXT NOT NULL, negate_amounts INTEGER NOT NULL, account_id INTEGER);

CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
";

/// Creates the schema if it is missing, records version 1 on first open, and rejects any
/// other stored version (R-PRG-1 is not involved here; this is open-time migration only).
pub(crate) fn migrate(conn: &Connection) -> Result<(), StoreError> {
    conn.execute_batch(SCHEMA_V1)?;
    conn.execute(
        "INSERT OR IGNORE INTO meta (key, value) VALUES ('schema_version', ?1)",
        params![SCHEMA_VERSION.to_string()],
    )?;
    let stored = read_schema_version(conn)?;
    if stored != SCHEMA_VERSION {
        return Err(StoreError::Corrupt {
            table: "meta",
            column: "schema_version",
            value: stored.to_string(),
        });
    }
    Ok(())
}

/// The version stored in `meta`.
pub(crate) fn read_schema_version(conn: &Connection) -> Result<i64, StoreError> {
    let text: String = conn.query_row(
        "SELECT value FROM meta WHERE key = 'schema_version'",
        [],
        |r| r.get(0),
    )?;
    codec::int(&text, "meta", "schema_version")
}
