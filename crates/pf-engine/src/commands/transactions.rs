//! Transaction commands (T10a): add, inline edit, status, duplicate, delete, orphans, attachment
//! check and the Rec button. Rules R-TXN-1..15 and R-CAT-5; the Swift source is
//! `Views/TransactionListView.swift` and `Views/AddTransactionView.swift`.

use std::path::Path;

use jiff::civil::Date;
use pf_core::balance::is_cleared;
use pf_core::categorize::{auto_categorize, rec_disabled, rec_message};
use pf_model::{
    AccountId, CategoryId, ContextId, Db, RowKey, Transaction, TransactionId, TransactionStatus,
};
use rust_decimal::Decimal;

use crate::engine::Engine;
use crate::error::{EngineError, Result};
use crate::state::AppState;
use crate::writer::dedupe_context_ids;

/// File extensions that may be opened from a transaction (R-TXN-14), compared lower-case.
const SAFE_EXTENSIONS: &[&str] = &[
    "pdf", "png", "jpg", "jpeg", "gif", "heic", "tiff", "webp", "txt", "md", "rtf", "csv", "xls",
    "xlsx", "numbers", "ppt", "pptx", "key",
];

/// The kind of a new transaction (R-TXN-8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxKind {
    /// Money in: the amount is forced positive.
    Deposit,
    /// Money out: the amount is forced negative.
    Withdrawal,
    /// Two rows: out of `account`, into `to`.
    Transfer {
        /// Destination account.
        to: AccountId,
    },
}

/// Add Transaction dialog values.
#[derive(Debug, Clone, PartialEq)]
pub struct NewTransaction {
    /// Deposit, withdrawal or transfer.
    pub kind: TxKind,
    /// Source account (the account of a deposit or withdrawal).
    pub account: AccountId,
    /// Calendar date of the movement.
    pub date: Date,
    /// Sign ignored; the absolute value is used (R-TXN-8).
    pub amount: Decimal,
    /// Payee; ignored for transfers, whose payees are generated.
    pub payee: String,
    /// Memo; an empty string is stored as `None`.
    pub memo: String,
    /// Status of the new row(s).
    pub status: TransactionStatus,
    /// Category, if any; shared by both rows of a transfer.
    pub category: Option<CategoryId>,
    /// Contexts; duplicates are dropped.
    pub contexts: Vec<ContextId>,
}

/// One inline edit of a transaction field (R-TXN-12).
#[derive(Debug, Clone, PartialEq)]
pub enum TxField {
    /// Payee text.
    Payee(String),
    /// Memo; `Some("")` is stored as `None`.
    Memo(Option<String>),
    /// Signed amount, stored as given.
    Amount(Decimal),
    /// Calendar date (R-TXN-9 applies).
    Date(Date),
    /// Category, or none.
    Category(Option<CategoryId>),
    /// Full context list.
    Contexts(Vec<ContextId>),
    /// Attachment path, or none.
    Attachment(Option<String>),
}

impl TxField {
    /// The undo name for this field, e.g. "Change Payee".
    fn undo_name(&self) -> &'static str {
        match self {
            TxField::Payee(_) => "Change Payee",
            TxField::Memo(_) => "Change Memo",
            TxField::Amount(_) => "Change Amount",
            TxField::Date(_) => "Change Date",
            TxField::Category(_) => "Change Category",
            TxField::Contexts(_) => "Change Contexts",
            TxField::Attachment(_) => "Change Attachment",
        }
    }

    /// Writes this field's value into `t`.
    fn apply_to(&self, t: &mut Transaction) {
        match self {
            TxField::Payee(p) => t.payee = p.clone(),
            TxField::Memo(m) => t.memo = m.clone().filter(|s| !s.is_empty()),
            TxField::Amount(a) => t.amount = *a,
            TxField::Date(d) => t.date = *d,
            TxField::Category(c) => t.category_id = *c,
            TxField::Contexts(ids) => {
                t.context_ids = ids.clone();
                dedupe_context_ids(&mut t.context_ids);
            }
            TxField::Attachment(p) => t.attachment_path = p.clone(),
        }
    }
}

/// R-TXN-1: deleted rows are locked; in scenario mode the reality rows are locked. A row that does
/// not exist counts as locked, so the UI disables it. Uses `state.active_scenario`.
pub fn is_locked(db: &Db, state: &AppState, id: TransactionId) -> bool {
    match db.transaction(id) {
        None => true,
        Some(t) if t.is_deleted => true,
        Some(t) => state.active_scenario.is_some() && t.scenario_id.is_none(),
    }
}

/// R-TXN-9: a reality row cannot be cleared or reconciled after `today`.
fn check_cleared_not_future(
    reality: bool,
    status: TransactionStatus,
    date: Date,
    today: Date,
) -> Result<()> {
    if reality && is_cleared(status) && date > today {
        return Err(EngineError::Validation(
            "Cleared transactions cannot be dated in the future".into(),
        ));
    }
    Ok(())
}

/// A live transaction, or `NotFound`.
fn live_transaction(db: &Db, id: TransactionId) -> Result<&Transaction> {
    db.transaction(id)
        .filter(|t| !t.is_deleted)
        .ok_or_else(|| EngineError::NotFound(format!("transaction {}", id.raw())))
}

/// A live account, or `NotFound`.
fn live_account(db: &Db, id: AccountId) -> Result<()> {
    match db.account(id) {
        Some(a) if !a.is_deleted => Ok(()),
        _ => Err(EngineError::NotFound(format!("account {}", id.raw()))),
    }
}

/// A copy of `template` on `account`, with the given signed amount and payee.
fn row_on(template: &Transaction, account: AccountId, amount: Decimal, payee: &str) -> Transaction {
    Transaction {
        account_id: Some(account),
        amount,
        payee: payee.to_string(),
        ..template.clone()
    }
}

/// "Add Transaction" (or "Add Transfer"). R-TXN-8: the new rows get the active scenario and the
/// signs are forced. R-TXN-9 applies to reality rows. Returns the new ids: one, or `[from, to]`.
pub fn add_transaction(e: &mut Engine, t: NewTransaction) -> Result<Vec<TransactionId>> {
    if let TxKind::Transfer { to } = t.kind
        && to == t.account
    {
        return Err(EngineError::Validation(
            "Choose a different destination account".into(),
        ));
    }
    let scenario = e.state.active_scenario;
    check_cleared_not_future(scenario.is_none(), t.status, t.date, e.today())?;

    let amount = t.amount.abs();
    let mut contexts = t.contexts.clone();
    dedupe_context_ids(&mut contexts);
    let template = Transaction {
        memo: (!t.memo.is_empty()).then(|| t.memo.clone()),
        status: t.status,
        scenario_id: scenario,
        category_id: t.category,
        context_ids: contexts,
        ..Transaction::new(TransactionId(0), t.date, amount, "")
    };
    let db = e.db();
    let (step, rows) = match t.kind {
        TxKind::Deposit => {
            live_account(db, t.account)?;
            (
                "Add Transaction",
                vec![row_on(&template, t.account, amount, &t.payee)],
            )
        }
        TxKind::Withdrawal => {
            live_account(db, t.account)?;
            (
                "Add Transaction",
                vec![row_on(&template, t.account, -amount, &t.payee)],
            )
        }
        TxKind::Transfer { to } => {
            let from_name = named_live_account(db, t.account)?;
            let to_name = named_live_account(db, to)?;
            (
                "Add Transfer",
                vec![
                    row_on(
                        &template,
                        t.account,
                        -amount,
                        &format!("Transfer to {to_name}"),
                    ),
                    row_on(&template, to, amount, &format!("Transfer from {from_name}")),
                ],
            )
        }
    };
    e.apply(step, |w| {
        Ok(rows.into_iter().map(|r| w.insert_transaction(r)).collect())
    })
}

/// The name of a live account, or `NotFound`.
fn named_live_account(db: &Db, id: AccountId) -> Result<String> {
    live_account(db, id)?;
    Ok(db.account(id).map(|a| a.name.clone()).unwrap_or_default())
}

/// One inline edit (R-TXN-12). Undo names: "Change Payee", "Change Memo", "Change Amount",
/// "Change Date", "Change Category", "Change Contexts", "Change Attachment". Locked ⇒ `Locked`;
/// an unchanged value makes no step. A date that is cleared-and-future on a reality row is refused.
/// After a changed payee on an uncategorized, unlocked row, R-TXN-13 runs auto-categorize (R-CAT-1)
/// as a second step "Auto-categorize". Returns whether that step fired.
pub fn edit_transaction(e: &mut Engine, id: TransactionId, field: TxField) -> Result<bool> {
    let today = e.today();
    let db = e.db();
    let current = db
        .transaction(id)
        .ok_or_else(|| EngineError::NotFound(format!("transaction {}", id.raw())))?;
    if is_locked(db, &e.state, id) {
        return Err(EngineError::Locked);
    }
    if let TxField::Date(date) = &field
        && *date != current.date
    {
        check_cleared_not_future(current.scenario_id.is_none(), current.status, *date, today)?;
    }
    let payee_changed = matches!(&field, TxField::Payee(p) if *p != current.payee);
    e.apply(field.undo_name(), |w| {
        w.update_transaction(id, |t| field.apply_to(t))
    })?;
    if payee_changed {
        return auto_categorize_after_payee(e, id);
    }
    Ok(false)
}

/// R-TXN-13: sets the category of an uncategorized, unlocked row whose payee matches R-CAT-1.
fn auto_categorize_after_payee(e: &mut Engine, id: TransactionId) -> Result<bool> {
    let db = e.db();
    let Some(row) = db.transaction(id) else {
        return Ok(false);
    };
    if row.category_id.is_some() || is_locked(db, &e.state, id) {
        return Ok(false);
    }
    let Some(category) = auto_categorize(db, &row.payee) else {
        return Ok(false);
    };
    e.apply("Auto-categorize", |w| {
        w.update_transaction(id, |t| t.category_id = Some(category))
    })?;
    Ok(true)
}

/// "Change Status". R-TXN-2..5 and R-TXN-4 (promotion). Messages (exact):
/// "Future status is only for scenario transactions" and
/// "Cannot clear a future-dated scenario transaction". Locked ⇒ `Locked`. Same status ⇒ no step.
pub fn change_status(e: &mut Engine, id: TransactionId, status: TransactionStatus) -> Result<()> {
    let today = e.today();
    let db = e.db();
    let current = db
        .transaction(id)
        .ok_or_else(|| EngineError::NotFound(format!("transaction {}", id.raw())))?;
    if is_locked(db, &e.state, id) {
        return Err(EngineError::Locked);
    }
    let reality = current.scenario_id.is_none();
    // R-TXN-2
    if reality && status == TransactionStatus::Future {
        return Err(EngineError::Validation(
            "Future status is only for scenario transactions".into(),
        ));
    }
    // R-TXN-5
    if status == current.status {
        return Ok(());
    }
    // R-TXN-3
    let clearing = matches!(
        status,
        TransactionStatus::Cleared | TransactionStatus::Reconciled
    );
    if !reality && clearing && current.date > today {
        return Err(EngineError::Validation(
            "Cannot clear a future-dated scenario transaction".into(),
        ));
    }
    // R-TXN-4: a scenario row that leaves the future state becomes reality.
    let promote = !reality
        && matches!(
            status,
            TransactionStatus::Pending | TransactionStatus::Cleared | TransactionStatus::Reconciled
        );
    e.apply("Change Status", |w| {
        w.update_transaction(id, |t| {
            t.status = status;
            if promote {
                t.scenario_id = None;
            }
        })
    })
}

/// "Duplicate Transaction" (1) / "Duplicate Transactions" (>1). R-TXN-6: each copy has every field
/// of its source except that `date` is today. Locked sources are allowed. Returns the new ids in
/// input order. A missing or deleted source is `NotFound`.
pub fn duplicate_transactions(e: &mut Engine, ids: &[TransactionId]) -> Result<Vec<TransactionId>> {
    let today = e.today();
    let db = e.db();
    let sources: Vec<Transaction> = ids
        .iter()
        .map(|id| live_transaction(db, *id).cloned())
        .collect::<Result<_>>()?;
    let step = if sources.len() > 1 {
        "Duplicate Transactions"
    } else {
        "Duplicate Transaction"
    };
    e.apply(step, |w| {
        Ok(sources
            .into_iter()
            .map(|src| {
                let copy = Transaction {
                    id: TransactionId(0),
                    date: today,
                    ..src
                };
                w.insert_transaction(copy)
            })
            .collect())
    })
}

/// "Delete Transaction" / "Delete Transactions". R-TXN-7: soft-deletes all of them in one step.
/// An empty slice makes no step. A locked row ⇒ `Locked` with nothing deleted (fix: Swift let the
/// context menu delete locked rows). A missing row ⇒ `NotFound`.
pub fn delete_transactions(e: &mut Engine, ids: &[TransactionId]) -> Result<()> {
    let mut unique: Vec<TransactionId> = Vec::new();
    for id in ids {
        if !unique.contains(id) {
            unique.push(*id);
        }
    }
    if unique.is_empty() {
        return Ok(());
    }
    let db = e.db();
    for id in &unique {
        if db.transaction(*id).is_none() {
            return Err(EngineError::NotFound(format!("transaction {}", id.raw())));
        }
        if is_locked(db, &e.state, *id) {
            return Err(EngineError::Locked);
        }
    }
    let step = if unique.len() > 1 {
        "Delete Transactions"
    } else {
        "Delete Transaction"
    };
    e.apply(step, |w| {
        for id in unique {
            w.set_deleted(RowKey::Transaction(id), true)?;
        }
        Ok(())
    })
}

/// R-TXN-15: live transactions with no account, newest first (date desc, then id desc).
pub fn orphaned_transactions(db: &Db) -> Vec<TransactionId> {
    let mut rows: Vec<&Transaction> = db
        .live_transactions()
        .filter(|t| t.account_id.is_none())
        .collect();
    rows.sort_by(|a, b| b.date.cmp(&a.date).then(b.id.cmp(&a.id)));
    rows.into_iter().map(|t| t.id).collect()
}

/// "Delete Orphaned Transactions". R-TXN-15 (fix): soft-deletes every orphan in one undoable step.
/// Returns the number deleted.
pub fn delete_orphaned(e: &mut Engine) -> Result<usize> {
    let ids = orphaned_transactions(e.db());
    let count = ids.len();
    e.apply("Delete Orphaned Transactions", |w| {
        for id in ids {
            w.set_deleted(RowKey::Transaction(id), true)?;
        }
        Ok(count)
    })
}

/// R-TXN-14: true if the file extension (case-insensitive) is on the allow-list. Pure; the UI
/// opens the file.
pub fn is_safe_attachment(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            let lower = ext.to_lowercase();
            SAFE_EXTENSIONS.contains(&lower.as_str())
        })
}

/// R-CAT-5: whether the Rec button is enabled for `category` and `payee`. Disabled when the
/// category is missing or deleted, the trimmed payee is empty, or it is already a pattern.
pub fn can_add_payee_pattern(db: &Db, category: CategoryId, payee: &str) -> bool {
    db.category(category).is_some_and(|c| !c.is_deleted) && !rec_disabled(db, Some(category), payee)
}

/// "Add Match Pattern". R-CAT-5: appends the trimmed payee to the category's patterns and returns
/// the toast text `Added "{payee}" to {fullName} auto-match`. `Validation` when the button would be
/// disabled.
pub fn add_payee_pattern(e: &mut Engine, category: CategoryId, payee: &str) -> Result<String> {
    if !can_add_payee_pattern(e.db(), category, payee) {
        return Err(EngineError::Validation(
            "This payee cannot be added as a pattern".into(),
        ));
    }
    let toast = rec_message(e.db(), category, payee);
    let trimmed = payee.trim().to_string();
    e.apply("Add Match Pattern", |w| {
        w.update_category(category, |c| c.match_patterns.push(trimmed.clone()))
    })?;
    Ok(toast)
}
