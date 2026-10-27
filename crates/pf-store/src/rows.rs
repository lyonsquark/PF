//! Row <-> struct conversion, one `read_*` / `write_*` pair per table. Column order matches
//! the CREATE statements in `schema.rs`. Enum columns decode with `parse_or_default`, so an
//! unknown value never fails a load.

use pf_model::{
    Account, AccountId, AccountType, BudgetFrequency, BudgetItem, BudgetItemId, BudgetItemType,
    Category, CategoryId, CategoryType, Context, ContextId, ImportMapping, ImportMappingId,
    Obligation, ObligationId, ObligationStatus, Record, RowKey, Scenario, ScenarioId, Transaction,
    TransactionId, TransactionStatus,
};
use rusqlite::{Connection, Row, params};

use crate::codec::{self, flag, json_strings, json_u32s, strings_json, u32s_json};
use crate::store::StoreError;

/// Reads a 0/1 column.
fn read_flag(row: &Row<'_>, idx: usize) -> Result<bool, StoreError> {
    Ok(flag(row.get(idx)?))
}

/// Writes a bool as INTEGER 0/1.
fn bool_int(value: bool) -> i64 {
    i64::from(value)
}

/// Reads an accounts row (`SELECT id, name, account_type, institution, currency, is_external, is_deleted`).
pub(crate) fn read_account(row: &Row<'_>) -> Result<Account, StoreError> {
    Ok(Account {
        id: AccountId(row.get(0)?),
        name: row.get(1)?,
        account_type: AccountType::parse_or_default(&row.get::<_, String>(2)?),
        institution: row.get(3)?,
        currency: row.get(4)?,
        is_external: read_flag(row, 5)?,
        is_deleted: read_flag(row, 6)?,
    })
}

/// Upserts an accounts row.
pub(crate) fn write_account(conn: &Connection, a: &Account) -> Result<(), StoreError> {
    conn.prepare_cached(
        "INSERT OR REPLACE INTO accounts (id, name, account_type, institution, currency, is_external, is_deleted)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?
    .execute(params![
        a.id.raw(),
        a.name,
        a.account_type.as_str(),
        a.institution,
        a.currency,
        bool_int(a.is_external),
        bool_int(a.is_deleted)
    ])?;
    Ok(())
}

/// Reads a transactions row. `context_ids` is left empty; `load_all` attaches the join rows.
pub(crate) fn read_transaction(row: &Row<'_>) -> Result<Transaction, StoreError> {
    Ok(Transaction {
        id: TransactionId(row.get(0)?),
        date: codec::date(&row.get::<_, String>(1)?, "transactions", "date")?,
        amount: codec::decimal(&row.get::<_, String>(2)?, "transactions", "amount")?,
        payee: row.get(3)?,
        memo: row.get(4)?,
        attachment_path: row.get(5)?,
        status: TransactionStatus::parse_or_default(&row.get::<_, String>(6)?),
        is_deleted: read_flag(row, 7)?,
        account_id: row.get::<_, Option<i64>>(8)?.map(AccountId),
        scenario_id: row.get::<_, Option<i64>>(9)?.map(ScenarioId),
        category_id: row.get::<_, Option<i64>>(10)?.map(CategoryId),
        context_ids: Vec::new(),
    })
}

/// Upserts a transactions row and replaces its `transaction_contexts` rows, keeping list order
/// in `position`.
pub(crate) fn write_transaction(conn: &Connection, t: &Transaction) -> Result<(), StoreError> {
    conn.prepare_cached(
        "INSERT OR REPLACE INTO transactions (id, date, amount, payee, memo, attachment_path, status,
           is_deleted, account_id, scenario_id, category_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
    )?
    .execute(params![
        t.id.raw(),
        t.date.to_string(),
        t.amount.to_string(),
        t.payee,
        t.memo,
        t.attachment_path,
        t.status.as_str(),
        bool_int(t.is_deleted),
        t.account_id.map(AccountId::raw),
        t.scenario_id.map(ScenarioId::raw),
        t.category_id.map(CategoryId::raw)
    ])?;
    conn.prepare_cached("DELETE FROM transaction_contexts WHERE transaction_id = ?1")?
        .execute(params![t.id.raw()])?;
    let mut link = conn.prepare_cached(
        "INSERT INTO transaction_contexts (transaction_id, context_id, position) VALUES (?1, ?2, ?3)",
    )?;
    for (ctx, position) in t.context_ids.iter().zip(0_i64..) {
        link.execute(params![t.id.raw(), ctx.raw(), position])?;
    }
    Ok(())
}

/// Reads a categories row (`match_patterns` is a JSON array).
pub(crate) fn read_category(row: &Row<'_>) -> Result<Category, StoreError> {
    Ok(Category {
        id: CategoryId(row.get(0)?),
        name: row.get(1)?,
        icon: row.get(2)?,
        notes: row.get(3)?,
        match_patterns: json_strings(&row.get::<_, String>(4)?, "categories", "match_patterns")?,
        category_type: CategoryType::parse_or_default(&row.get::<_, String>(5)?),
        parent_id: row.get::<_, Option<i64>>(6)?.map(CategoryId),
        is_deleted: read_flag(row, 7)?,
    })
}

/// Upserts a categories row.
pub(crate) fn write_category(conn: &Connection, c: &Category) -> Result<(), StoreError> {
    conn.prepare_cached(
        "INSERT OR REPLACE INTO categories (id, name, icon, notes, match_patterns, category_type, parent_id, is_deleted)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?
    .execute(params![
        c.id.raw(),
        c.name,
        c.icon,
        c.notes,
        strings_json(&c.match_patterns),
        c.category_type.as_str(),
        c.parent_id.map(CategoryId::raw),
        bool_int(c.is_deleted)
    ])?;
    Ok(())
}

/// Reads a contexts row.
pub(crate) fn read_context(row: &Row<'_>) -> Result<Context, StoreError> {
    Ok(Context {
        id: ContextId(row.get(0)?),
        name: row.get(1)?,
        color_hex: row.get(2)?,
        notes: row.get(3)?,
        is_archived: read_flag(row, 4)?,
        is_deleted: read_flag(row, 5)?,
    })
}

/// Upserts a contexts row.
pub(crate) fn write_context(conn: &Connection, c: &Context) -> Result<(), StoreError> {
    conn.prepare_cached(
        "INSERT OR REPLACE INTO contexts (id, name, color_hex, notes, is_archived, is_deleted)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?
    .execute(params![
        c.id.raw(),
        c.name,
        c.color_hex,
        c.notes,
        bool_int(c.is_archived),
        bool_int(c.is_deleted)
    ])?;
    Ok(())
}

/// Reads a scenarios row.
pub(crate) fn read_scenario(row: &Row<'_>) -> Result<Scenario, StoreError> {
    Ok(Scenario {
        id: ScenarioId(row.get(0)?),
        name: row.get(1)?,
        color_hex: row.get(2)?,
        notes: row.get(3)?,
        is_deleted: read_flag(row, 4)?,
    })
}

/// Upserts a scenarios row.
pub(crate) fn write_scenario(conn: &Connection, s: &Scenario) -> Result<(), StoreError> {
    conn.prepare_cached(
        "INSERT OR REPLACE INTO scenarios (id, name, color_hex, notes, is_deleted) VALUES (?1, ?2, ?3, ?4, ?5)",
    )?
    .execute(params![
        s.id.raw(),
        s.name,
        s.color_hex,
        s.notes,
        bool_int(s.is_deleted)
    ])?;
    Ok(())
}

/// Reads an obligations row.
pub(crate) fn read_obligation(row: &Row<'_>) -> Result<Obligation, StoreError> {
    const T: &str = "obligations";
    Ok(Obligation {
        id: ObligationId(row.get(0)?),
        name: row.get(1)?,
        target_amount: row
            .get::<_, Option<String>>(2)?
            .map(|v| codec::decimal(&v, T, "target_amount"))
            .transpose()?,
        current_balance: codec::decimal(&row.get::<_, String>(3)?, T, "current_balance")?,
        due_date: row
            .get::<_, Option<String>>(4)?
            .map(|v| codec::date(&v, T, "due_date"))
            .transpose()?,
        notes: row.get(5)?,
        status: ObligationStatus::parse_or_default(&row.get::<_, String>(6)?),
        created_date: row
            .get::<_, Option<String>>(7)?
            .map(|v| codec::timestamp(&v, T, "created_date"))
            .transpose()?,
        last_updated_date: row
            .get::<_, Option<String>>(8)?
            .map(|v| codec::timestamp(&v, T, "last_updated_date"))
            .transpose()?,
        account_id: row.get::<_, Option<i64>>(9)?.map(AccountId),
        context_id: row.get::<_, Option<i64>>(10)?.map(ContextId),
        is_deleted: read_flag(row, 11)?,
    })
}

/// Upserts an obligations row.
pub(crate) fn write_obligation(conn: &Connection, o: &Obligation) -> Result<(), StoreError> {
    conn.prepare_cached(
        "INSERT OR REPLACE INTO obligations (id, name, target_amount, current_balance, due_date, notes,
           status, created_date, last_updated_date, account_id, context_id, is_deleted)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
    )?
    .execute(params![
        o.id.raw(),
        o.name,
        o.target_amount.map(|d| d.to_string()),
        o.current_balance.to_string(),
        o.due_date.map(|d| d.to_string()),
        o.notes,
        o.status.as_str(),
        o.created_date.map(|t| t.to_string()),
        o.last_updated_date.map(|t| t.to_string()),
        o.account_id.map(AccountId::raw),
        o.context_id.map(ContextId::raw),
        bool_int(o.is_deleted)
    ])?;
    Ok(())
}

/// Reads a budget_items row (`specific_months` is a JSON array).
pub(crate) fn read_budget_item(row: &Row<'_>) -> Result<BudgetItem, StoreError> {
    const T: &str = "budget_items";
    Ok(BudgetItem {
        id: BudgetItemId(row.get(0)?),
        item_type: BudgetItemType::parse_or_default(&row.get::<_, String>(1)?),
        name: row.get(2)?,
        amount: codec::decimal(&row.get::<_, String>(3)?, T, "amount")?,
        start_date: codec::date(&row.get::<_, String>(4)?, T, "start_date")?,
        end_date: row
            .get::<_, Option<String>>(5)?
            .map(|v| codec::date(&v, T, "end_date"))
            .transpose()?,
        frequency: BudgetFrequency::parse_or_default(&row.get::<_, String>(6)?),
        specific_months: json_u32s(&row.get::<_, String>(7)?, T, "specific_months")?,
        notes: row.get(8)?,
        category_id: row.get::<_, Option<i64>>(9)?.map(CategoryId),
        account_id: row.get::<_, Option<i64>>(10)?.map(AccountId),
        is_deleted: read_flag(row, 11)?,
    })
}

/// Upserts a budget_items row.
pub(crate) fn write_budget_item(conn: &Connection, b: &BudgetItem) -> Result<(), StoreError> {
    conn.prepare_cached(
        "INSERT OR REPLACE INTO budget_items (id, item_type, name, amount, start_date, end_date, frequency,
           specific_months, notes, category_id, account_id, is_deleted)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
    )?
    .execute(params![
        b.id.raw(),
        b.item_type.as_str(),
        b.name,
        b.amount.to_string(),
        b.start_date.to_string(),
        b.end_date.map(|d| d.to_string()),
        b.frequency.as_str(),
        u32s_json(&b.specific_months),
        b.notes,
        b.category_id.map(CategoryId::raw),
        b.account_id.map(AccountId::raw),
        bool_int(b.is_deleted)
    ])?;
    Ok(())
}

/// Reads an import_mappings row.
pub(crate) fn read_import_mapping(row: &Row<'_>) -> Result<ImportMapping, StoreError> {
    Ok(ImportMapping {
        id: ImportMappingId(row.get(0)?),
        mapping_type: row.get(1)?,
        source_value: row.get(2)?,
        target_value: row.get(3)?,
        negate_amounts: read_flag(row, 4)?,
        account_id: row.get::<_, Option<i64>>(5)?.map(AccountId),
    })
}

/// Upserts an import_mappings row.
pub(crate) fn write_import_mapping(conn: &Connection, m: &ImportMapping) -> Result<(), StoreError> {
    conn.prepare_cached(
        "INSERT OR REPLACE INTO import_mappings (id, mapping_type, source_value, target_value, negate_amounts, account_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?
    .execute(params![
        m.id.raw(),
        m.mapping_type,
        m.source_value,
        m.target_value,
        bool_int(m.negate_amounts),
        m.account_id.map(AccountId::raw)
    ])?;
    Ok(())
}

/// Upserts a settings row.
pub(crate) fn write_setting(conn: &Connection, key: &str, value: &str) -> Result<(), StoreError> {
    conn.prepare_cached("INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)")?
        .execute(params![key, value])?;
    Ok(())
}

/// Upserts any record into its table.
pub(crate) fn write_record(conn: &Connection, record: &Record) -> Result<(), StoreError> {
    match record {
        Record::Account(r) => write_account(conn, r),
        Record::Transaction(r) => write_transaction(conn, r),
        Record::Category(r) => write_category(conn, r),
        Record::Context(r) => write_context(conn, r),
        Record::Scenario(r) => write_scenario(conn, r),
        Record::Obligation(r) => write_obligation(conn, r),
        Record::BudgetItem(r) => write_budget_item(conn, r),
        Record::ImportMapping(r) => write_import_mapping(conn, r),
        Record::Setting { key, value } => write_setting(conn, key, value),
    }
}

/// Hard-deletes the row with `key`. A transaction's context links go with it.
pub(crate) fn delete_row(conn: &Connection, key: &RowKey) -> Result<(), StoreError> {
    let (sql, id): (&str, Option<i64>) = match key {
        RowKey::Account(id) => ("DELETE FROM accounts WHERE id = ?1", Some(id.raw())),
        RowKey::Transaction(id) => {
            conn.prepare_cached("DELETE FROM transaction_contexts WHERE transaction_id = ?1")?
                .execute(params![id.raw()])?;
            ("DELETE FROM transactions WHERE id = ?1", Some(id.raw()))
        }
        RowKey::Category(id) => ("DELETE FROM categories WHERE id = ?1", Some(id.raw())),
        RowKey::Context(id) => ("DELETE FROM contexts WHERE id = ?1", Some(id.raw())),
        RowKey::Scenario(id) => ("DELETE FROM scenarios WHERE id = ?1", Some(id.raw())),
        RowKey::Obligation(id) => ("DELETE FROM obligations WHERE id = ?1", Some(id.raw())),
        RowKey::BudgetItem(id) => ("DELETE FROM budget_items WHERE id = ?1", Some(id.raw())),
        RowKey::ImportMapping(id) => ("DELETE FROM import_mappings WHERE id = ?1", Some(id.raw())),
        RowKey::Setting(k) => {
            conn.prepare_cached("DELETE FROM settings WHERE key = ?1")?
                .execute(params![k])?;
            return Ok(());
        }
    };
    conn.prepare_cached(sql)?.execute(params![id])?;
    Ok(())
}
