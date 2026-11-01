//! R-REC-7, R-REC-13: the entry point, the reading choice and the balance checks.

use std::collections::BTreeMap;

use pf_model::{AccountId, Db, TransactionId, TransactionStatus};
use rust_decimal::Decimal;

use super::check::{
    BalanceStyle, Reading, check_totals, line_amount, pf_balance, self_check_for, style_for_account,
};
use super::matching::{Pair, candidates, run_matching};
use super::payee::aliases_for;
use super::statement::Statement;

/// What to reconcile.
pub struct ReconcileInput<'a> {
    /// The data.
    pub db: &'a Db,
    /// The account being reconciled.
    pub account: AccountId,
    /// The parsed statement.
    pub statement: &'a Statement,
    /// The reading remembered for this account; only breaks a tie.
    pub preferred: Option<Reading>,
}

/// The result of comparing a statement with an account.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// The chosen reading.
    pub reading: Reading,
    /// The chosen balance style.
    pub style: BalanceStyle,
    /// Lines with a same-amount candidate within 10 days, read as printed.
    pub matches_as_printed: usize,
    /// The same count with every line flipped.
    pub matches_flipped: usize,
    /// Each line's amount under the chosen reading.
    pub amounts: Vec<Decimal>,
    /// Statement opening balance, PF sign.
    pub opening: Decimal,
    /// Statement closing balance, PF sign.
    pub closing: Decimal,
    /// Pass 1 and 2 pairs.
    pub matched: Vec<Pair>,
    /// Pass 3 pairs.
    pub amount_differs: Vec<Pair>,
    /// Unpaired lines that a Reconciled transaction already explains.
    pub already_reconciled: Vec<(usize, TransactionId)>,
    /// Lines with no transaction.
    pub statement_only: Vec<usize>,
    /// Transactions in the period with no line.
    pub pf_only_in_period: Vec<TransactionId>,
    /// Transactions near but outside the period with no line.
    pub pf_only_outside: Vec<TransactionId>,
    /// Sum of the account's Reconciled transactions.
    pub reconciled_balance: Decimal,
    /// Statement opening minus the reconciled balance.
    pub opening_difference: Decimal,
    /// True when the account has no Reconciled transaction yet.
    pub first_time: bool,
    /// Amounts of the candidate transactions, so `difference` needs no database.
    pub tx_amounts: BTreeMap<TransactionId, Decimal>,
}

/// The owner's (or the default) decisions about a report.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    /// (line index, transaction, use the statement amount).
    pub pairs: Vec<(usize, TransactionId, bool)>,
    /// Statement-only lines to add as new transactions.
    pub add_lines: Vec<usize>,
    /// An adjustment transaction, if wanted.
    pub adjustment: Option<Decimal>,
}

impl Choice {
    /// R-REC-13: accept every pair (PF amounts) and add every statement-only line.
    pub fn default_for(report: &Report) -> Choice {
        Choice {
            pairs: report
                .matched
                .iter()
                .chain(&report.amount_differs)
                .map(|p| (p.line, p.transaction, false))
                .collect(),
            add_lines: report.statement_only.clone(),
            adjustment: None,
        }
    }
}

/// R-REC-13: Σ amounts of the account's live Reality Reconciled transactions.
pub fn reconciled_balance(db: &Db, account: AccountId) -> Decimal {
    db.transactions_for_account(account)
        .filter(|t| t.scenario_id.is_none() && t.status == TransactionStatus::Reconciled)
        .map(|t| t.amount)
        .sum()
}

/// R-REC-13: closing − (opening + accepted pairs + added lines + adjustment).
pub fn difference(report: &Report, choice: &Choice) -> Decimal {
    let pairs: Decimal = choice
        .pairs
        .iter()
        .map(|(line, tx, use_statement)| {
            if *use_statement {
                report.amounts.get(*line).copied().unwrap_or_default()
            } else {
                report.tx_amounts.get(tx).copied().unwrap_or_default()
            }
        })
        .sum();
    let added: Decimal = choice
        .add_lines
        .iter()
        .map(|i| report.amounts.get(*i).copied().unwrap_or_default())
        .sum();
    let adjustment = choice.adjustment.unwrap_or_default();
    report.closing - (report.opening + pairs + added + adjustment)
}

/// R-REC-7: lines with some candidate of exactly the same amount within 10 days.
fn count_matches(db: &Db, account: AccountId, st: &Statement, reading: Reading) -> usize {
    let (open, _) = candidates(db, account, st);
    st.lines
        .iter()
        .filter(|l| {
            let amount = line_amount(l, reading);
            open.iter().any(|tx| {
                let near = |d: jiff::civil::Date| (d - tx.date).get_days().abs() <= 10;
                tx.amount == amount && (near(l.date) || l.post_date.is_some_and(near))
            })
        })
        .count()
}

/// R-REC-3 … R-REC-13: checks the statement, chooses the reading and matches.
pub fn reconcile(input: &ReconcileInput<'_>) -> Result<Report, String> {
    reconcile_with(input, None)
}

/// Like [`reconcile`], but `forced` overrides the reading choice (the Flip signs button, T30).
pub fn reconcile_with(
    input: &ReconcileInput<'_>,
    forced: Option<Reading>,
) -> Result<Report, String> {
    let (db, account, st) = (input.db, input.account, input.statement);
    let Some(acct) = db.account(account) else {
        return Err("Account not found.".to_string());
    };
    let readings = self_check_for(st, acct.account_type)?;
    check_totals(st)?;

    let matches_as_printed = count_matches(db, account, st, Reading::AsPrinted);
    let matches_flipped = count_matches(db, account, st, Reading::Flipped);
    let chosen = if let Some(f) = forced {
        usize::from(f == Reading::Flipped)
    } else if matches_as_printed != matches_flipped {
        if matches_as_printed > matches_flipped {
            0
        } else {
            1
        }
    } else if let Some(pref) = input.preferred {
        usize::from(pref == Reading::Flipped)
    } else {
        let fit = style_for_account(acct.account_type);
        if readings[0].1 != readings[1].1 && readings[1].1 == fit {
            1
        } else {
            0
        }
    };
    let (reading, style) = readings[chosen];

    let amounts: Vec<Decimal> = st.lines.iter().map(|l| line_amount(l, reading)).collect();
    let aliases = aliases_for(db, account);
    let m = run_matching(db, account, st, &amounts, &aliases);
    let (open, _) = candidates(db, account, st);
    let opening = pf_balance(st.opening_balance, style);
    let closing = pf_balance(st.closing_balance, style);
    let reconciled = reconciled_balance(db, account);
    let first_time = !db
        .transactions_for_account(account)
        .any(|t| t.scenario_id.is_none() && t.status == TransactionStatus::Reconciled);
    Ok(Report {
        reading,
        style,
        matches_as_printed,
        matches_flipped,
        amounts,
        opening,
        closing,
        matched: m.matched,
        amount_differs: m.amount_differs,
        already_reconciled: m.already_reconciled,
        statement_only: m.statement_only,
        pf_only_in_period: m.pf_only_in_period,
        pf_only_outside: m.pf_only_outside,
        reconciled_balance: reconciled,
        opening_difference: opening - reconciled,
        first_time,
        tx_amounts: open.iter().map(|t| (t.id, t.amount)).collect(),
    })
}

/// R-REC-12 headings, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    /// Pass 1 and 2 pairs.
    Matched,
    /// Pass 3 pairs.
    AmountDiffers,
    /// Lines only on the statement.
    StatementOnly,
    /// Transactions in the period only in PF.
    PfOnlyInPeriod,
    /// Lines a Reconciled transaction explains.
    AlreadyReconciled,
    /// Transactions outside the period only in PF.
    PfOnlyOutside,
}
