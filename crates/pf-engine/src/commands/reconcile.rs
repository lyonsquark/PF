//! Statement reconciliation commands (T29, R-REC-14 … R-REC-16). New feature with no Swift source.
//! Reconciliation is recorded only as the Reconciled status (plus aliases and a remembered reading).

use pf_core::categorize::auto_categorize;
use pf_core::reconcile::{
    Choice, Reading, Report, Statement, aliases_for, normalize_description, payee_score,
};
use pf_model::{
    AccountId, Db, ImportMapping, ImportMappingId, Transaction, TransactionId, TransactionStatus,
};
use rust_decimal::Decimal;

use crate::engine::Engine;
use crate::error::{EngineError, Result};

/// What to apply: the owner's decisions about a report.
pub struct Apply<'a> {
    /// The account being reconciled.
    pub account: AccountId,
    /// The statement the report was made from.
    pub statement: &'a Statement,
    /// The report (its `amounts` are already under the chosen reading).
    pub report: &'a Report,
    /// Accepted pairs, lines to add and the adjustment.
    pub choice: &'a Choice,
    /// Also mark every older, unreconciled transaction of the account (first reconciliation).
    pub mark_before: bool,
}

/// R-REC-16: the setting that remembers an account's reading.
fn reading_key(account: AccountId) -> String {
    format!("reconcileReading.{}", account.raw())
}

/// R-REC-11: the aliases used to score payees for this account.
pub fn payee_aliases(db: &Db, account: AccountId) -> Vec<(String, String)> {
    aliases_for(db, account)
}

/// R-REC-16: the remembered reading for the account, if any.
pub fn preferred_reading(db: &Db, account: AccountId) -> Option<Reading> {
    match db.setting(&reading_key(account)) {
        Some("asPrinted") => Some(Reading::AsPrinted),
        Some("flipped") => Some(Reading::Flipped),
        _ => None,
    }
}

/// A new transaction for a statement line that PF lacks.
struct NewLine {
    date: jiff::civil::Date,
    amount: Decimal,
    payee: String,
    memo: Option<String>,
    category: Option<pf_model::CategoryId>,
}

fn statement_amount(a: &Apply<'_>, line: usize) -> Result<Decimal> {
    a.report
        .amounts
        .get(line)
        .copied()
        .ok_or_else(|| EngineError::Validation(format!("Statement line {} not found.", line + 1)))
}

/// R-REC-14 … R-REC-16: "Reconcile Statement", one undo step. Returns how many transactions
/// became Reconciled (pairs + added lines + marked older ones).
pub fn apply_reconciliation(e: &mut Engine, a: &Apply<'_>) -> Result<usize> {
    if e.state.active_scenario.is_some() {
        return Err(EngineError::Validation(
            "Switch to Reality to reconcile.".into(),
        ));
    }
    // Validate everything before `apply`, so a refusal creates no step.
    for (_, id, _) in &a.choice.pairs {
        let ok = e.db().transaction(*id).is_some_and(|t| {
            !t.is_deleted && t.scenario_id.is_none() && t.account_id == Some(a.account)
        });
        if !ok {
            return Err(EngineError::Validation(format!(
                "Transaction {id} cannot be reconciled."
            )));
        }
    }
    let aliases = aliases_for(e.db(), a.account);
    let mut new_lines = Vec::new();
    for &line in &a.choice.add_lines {
        let amount = statement_amount(a, line)?;
        let st_line = a.statement.lines.get(line).ok_or_else(|| {
            EngineError::Validation(format!("Statement line {} not found.", line + 1))
        })?;
        let desc = st_line.description.trim().to_string();
        let norm = normalize_description(&desc);
        let alias = aliases.iter().find(|(s, _)| !norm.is_empty() && *s == norm);
        let payee = match alias {
            Some((_, target)) => target.clone(),
            None if desc.is_empty() => "Statement line".to_string(),
            None => desc.clone(),
        };
        let memo = (!desc.is_empty() && payee != desc).then(|| desc.clone());
        new_lines.push(NewLine {
            date: st_line.date,
            amount,
            category: auto_categorize(e.db(), &payee),
            payee,
            memo,
        });
    }
    let mut pair_amounts = Vec::new();
    for &(line, id, use_statement) in &a.choice.pairs {
        let amount = use_statement
            .then(|| statement_amount(a, line))
            .transpose()?;
        pair_amounts.push((line, id, amount));
    }
    let account = a.account;
    let (start, end) = (a.statement.period_start, a.statement.period_end);
    let reading = match a.report.reading {
        Reading::AsPrinted => "asPrinted",
        Reading::Flipped => "flipped",
    };

    e.apply("Reconcile Statement", |w| {
        let mut count = 0;
        let mut paired: Vec<TransactionId> = Vec::new();
        for (line, id, amount) in &pair_amounts {
            w.update_transaction(*id, |t| {
                t.status = TransactionStatus::Reconciled;
                if let Some(x) = amount {
                    t.amount = *x;
                }
            })?;
            paired.push(*id);
            count += 1;
            save_alias(w, a, &aliases, *line, *id);
        }
        for n in new_lines {
            let mut t = Transaction::new(TransactionId(0), n.date, n.amount, &n.payee);
            t.memo = n.memo;
            t.category_id = n.category;
            t.status = TransactionStatus::Reconciled;
            t.account_id = Some(account);
            w.insert_transaction(t);
            count += 1;
        }
        if let Some(adj) = a.choice.adjustment.filter(|x| !x.is_zero()) {
            let mut t = Transaction::new(TransactionId(0), end, adj, "Reconciliation adjustment");
            t.memo = Some(format!("Statement {start} to {end}"));
            t.status = TransactionStatus::Reconciled;
            t.account_id = Some(account);
            w.insert_transaction(t);
        }
        if a.mark_before {
            let older: Vec<TransactionId> = w
                .db()
                .transactions_for_account(account)
                .filter(|t| {
                    t.scenario_id.is_none()
                        && t.date < start
                        && t.status != TransactionStatus::Reconciled
                        && !paired.contains(&t.id)
                })
                .map(|t| t.id)
                .collect();
            for id in older {
                w.update_transaction(id, |t| t.status = TransactionStatus::Reconciled)?;
                count += 1;
            }
        }
        w.set_setting(&reading_key(account), reading);
        Ok(count)
    })
}

/// R-REC-15: remember how the statement names this payee, unless it already scored 1.0.
fn save_alias(
    w: &mut crate::writer::Writer<'_>,
    a: &Apply<'_>,
    aliases: &[(String, String)],
    line: usize,
    tx: TransactionId,
) {
    let Some(st_line) = a.statement.lines.get(line) else {
        return;
    };
    let Some(payee) = w.db().transaction(tx).map(|t| t.payee.trim().to_string()) else {
        return;
    };
    let source = normalize_description(&st_line.description);
    if source.is_empty()
        || payee.is_empty()
        || payee_score(&st_line.description, &payee, aliases) >= 1.0
    {
        return;
    }
    let existing = w
        .db()
        .import_mappings
        .values()
        .find(|m| {
            m.mapping_type == "statementPayee"
                && m.account_id == Some(a.account)
                && m.source_value == source
        })
        .map(|m| m.id);
    match existing {
        Some(id) => {
            let _ = w.update_import_mapping(id, |m| m.target_value = payee.clone());
        }
        None => {
            let _: ImportMappingId = w.insert_import_mapping(ImportMapping::new(
                ImportMappingId(0),
                "statementPayee",
                &source,
                &payee,
                Some(a.account),
            ));
        }
    }
}
