//! CSV import commands (T10c): the wizard's import as one undo step, and the headless import used
//! by the API. Rules R-CSV-8..16 and R-API-8; the Swift sources are `Views/CSVImportFlowView.swift`
//! (`onContinue`, `performImport`) and `Services/APIHandlers.swift` (`handleImportCSV`).

use jiff::tz::TimeZone;
use pf_core::csv_import::{
    ColumnMapping, ContextRef, ImportRequest, MappingPlan, detect_columns, parse_csv, plan_import,
    saved_column_assignments,
};
use pf_model::{
    AccountId, Category, CategoryId, CategoryType, Context, ContextId, ImportMapping,
    ImportMappingId, RowKey, Transaction, TransactionId, TransactionStatus,
};

pub use pf_core::csv_import::ImportResult;

use crate::commands::accounts::{AccountInput, account_from, normalize};
use crate::engine::Engine;
use crate::error::{EngineError, Result};
use crate::writer::Writer;

/// Everything the CSV wizard collected, applied as one step (R-CSV-14, R-CSV-15, R-CSV-16).
#[derive(Debug, Clone, PartialEq)]
pub struct CsvImportRequest {
    /// Monarch layout (an Account column is mapped). Bank layouts need a target account.
    pub is_monarch: bool,
    /// Bank mode: the account every row goes to. Required unless `is_monarch`.
    pub target_account: Option<AccountId>,
    /// Data rows, headers excluded.
    pub rows: Vec<Vec<String>>,
    /// Final column assignments and the negate flag.
    pub mapping: ColumnMapping,
    /// Accounts the wizard asked to create (Monarch "New…").
    pub new_accounts: Vec<AccountInput>,
    /// Translation and column rows, from `pf_core::csv_import::plan_mappings`.
    pub mapping_plan: MappingPlan,
}

/// Inserts the transactions of `rows` planned against the current data. Returns the counts.
/// R-CSV-14: rows are Cleared and in Reality; contexts are created for names the plan lists.
fn import_rows(
    w: &mut Writer<'_>,
    rows: &[Vec<String>],
    mapping: &ColumnMapping,
    target: Option<AccountId>,
    allow_unmapped_payees: bool,
) -> ImportResult {
    let tz = TimeZone::system();
    let plan = plan_import(
        w.db(),
        &ImportRequest {
            rows,
            mapping,
            target_account: target,
            allow_unmapped_payees,
            tz: &tz,
        },
    );
    let context_ids: Vec<ContextId> = plan
        .new_contexts
        .iter()
        .map(|name| w.insert_context(Context::new(ContextId(0), name)))
        .collect();
    for p in &plan.transactions {
        let contexts: Vec<ContextId> = p
            .contexts
            .iter()
            .filter_map(|c| match c {
                ContextRef::Existing(id) => Some(*id),
                ContextRef::New(i) => context_ids.get(*i).copied(),
            })
            .collect();
        w.insert_transaction(Transaction {
            memo: p.memo.clone(),
            status: TransactionStatus::Cleared,
            account_id: Some(p.account_id),
            category_id: p.category_id,
            context_ids: contexts,
            ..Transaction::new(TransactionId(0), p.date, p.amount, &p.payee)
        });
    }
    plan.result
}

/// Applies the mapping plan's categories and mappings, then runs `import_rows`. Shared by the
/// wizard path and the headless path so each is one step.
fn import_step(
    w: &mut Writer<'_>,
    plan: &MappingPlan,
    rows: &[Vec<String>],
    mapping: &ColumnMapping,
    target: Option<AccountId>,
    allow_unmapped_payees: bool,
) -> Result<ImportResult> {
    for nc in &plan.new_categories {
        let mut c = Category::new(CategoryId(0), &nc.name, CategoryType::Expense);
        c.parent_id = nc.parent_id;
        w.insert_category(c);
    }
    for id in &plan.remove_mapping_ids {
        // Skip rows that are already gone; the plan only names rows that were live when it was made.
        if w.db().get(&RowKey::ImportMapping(*id)).is_some() {
            w.delete_hard(RowKey::ImportMapping(*id))?;
        }
    }
    for nm in &plan.new_mappings {
        let mut m = ImportMapping::new(
            ImportMappingId(0),
            &nm.kind,
            &nm.source,
            &nm.target,
            nm.account_id,
        );
        m.negate_amounts = nm.negate;
        w.insert_import_mapping(m);
    }
    Ok(import_rows(w, rows, mapping, target, allow_unmapped_payees))
}

/// "Import CSV" (R-CSV-14..16). One undo step, in this order: create the new accounts; insert the
/// new categories; hard-delete the replaced mapping rows and insert the new ones; plan the rows over
/// the updated data (`allow_unmapped_payees = !is_monarch`); insert the transactions.
/// A bank import without a target account is `Validation("Choose an account")`.
pub fn run_csv_import(e: &mut Engine, req: CsvImportRequest) -> Result<ImportResult> {
    if !req.is_monarch && req.target_account.is_none() {
        return Err(EngineError::Validation("Choose an account".into()));
    }
    let accounts = req
        .new_accounts
        .iter()
        .cloned()
        .map(normalize)
        .collect::<Result<Vec<AccountInput>>>()?;
    e.apply("Import CSV", |w| {
        for input in &accounts {
            w.insert_account(account_from(AccountId(0), input));
        }
        import_step(
            w,
            &req.mapping_plan,
            &req.rows,
            &req.mapping,
            req.target_account,
            !req.is_monarch,
        )
    })
}

/// Headless import used by the API (R-API-8). Port of `APIHandlers.handleImportCSV`:
/// * no "column" mapping rows for the account ⇒ `Validation` naming the account;
/// * an empty CSV (no headers or no rows) ⇒ `Validation("CSV is empty or could not be parsed")`;
/// * detect the columns, then apply the saved assignments matched by trimmed header text;
/// * plan with `allow_unmapped_payees = true`; the saved negate flag is not applied (Swift quirk);
/// * one step "Import CSV".
pub fn import_csv_with_saved_mappings(
    e: &mut Engine,
    account: AccountId,
    csv: &str,
) -> Result<ImportResult> {
    let db = e.db();
    let acc = db
        .account(account)
        .filter(|a| !a.is_deleted)
        .ok_or_else(|| EngineError::NotFound(format!("account {}", account.raw())))?;
    let has_columns = db
        .import_mappings
        .values()
        .any(|m| m.mapping_type == "column" && m.account_id == Some(account));
    if !has_columns {
        return Err(EngineError::Validation(format!(
            "No saved column mappings for account '{}'. Please import via the UI first to set up column mappings.",
            acc.name
        )));
    }
    let parsed = parse_csv(csv);
    if parsed.headers.is_empty() || parsed.rows.is_empty() {
        return Err(EngineError::Validation(
            "CSV is empty or could not be parsed".into(),
        ));
    }
    let mut mapping = detect_columns(&parsed.headers);
    let saved = saved_column_assignments(db, &parsed.headers, Some(account), false);
    mapping.apply_assignments(&saved);
    e.apply("Import CSV", |w| {
        Ok(import_rows(w, &parsed.rows, &mapping, Some(account), true))
    })
}
