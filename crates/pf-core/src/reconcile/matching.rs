//! R-REC-8 … R-REC-10, R-REC-12: candidates, the three passes and the sections.

use std::collections::BTreeMap;

use jiff::ToSpan;
use jiff::civil::Date;
use pf_model::{AccountId, Db, Transaction, TransactionId, TransactionStatus};
use rust_decimal::Decimal;

use super::payee::payee_score;
use super::statement::Statement;

/// How sure a pairing is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// Equal amount within 3 days.
    High,
    /// Equal amount within 10 days.
    Medium,
    /// Amounts differ a little (tip, fee).
    AmountDiffers,
}

/// A statement line paired with a PFRust transaction.
#[derive(Debug, Clone, PartialEq)]
pub struct Pair {
    /// Index into `statement.lines`.
    pub line: usize,
    /// The paired transaction.
    pub transaction: TransactionId,
    /// How the pair was found.
    pub confidence: Confidence,
    /// Days between the line (or its post date) and the transaction.
    pub day_distance: i32,
    /// R-REC-11 payee score.
    pub payee_score: f64,
}

/// The result of matching, before balances.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Matching {
    /// Pass 1 and 2 pairs.
    pub matched: Vec<Pair>,
    /// Pass 3 pairs.
    pub amount_differs: Vec<Pair>,
    /// Unpaired lines that already have a Reconciled transaction.
    pub already_reconciled: Vec<(usize, TransactionId)>,
    /// Other unpaired lines.
    pub statement_only: Vec<usize>,
    /// Unpaired candidates dated inside the period.
    pub pf_only_in_period: Vec<TransactionId>,
    /// Unpaired candidates dated outside the period.
    pub pf_only_outside: Vec<TransactionId>,
}

/// R-REC-8: (unreconciled candidates, reconciled ones) of the account near the period.
pub fn candidates<'a>(
    db: &'a Db,
    account: AccountId,
    st: &Statement,
) -> (Vec<&'a Transaction>, Vec<&'a Transaction>) {
    let from = st
        .period_start
        .checked_sub(7.days())
        .unwrap_or(st.period_start);
    let to = st.period_end.checked_add(7.days()).unwrap_or(st.period_end);
    let mut open = Vec::new();
    let mut done = Vec::new();
    for tx in db.transactions_for_account(account) {
        if tx.scenario_id.is_some() || tx.date < from || tx.date > to {
            continue;
        }
        if tx.status == TransactionStatus::Reconciled {
            done.push(tx);
        } else {
            open.push(tx);
        }
    }
    (open, done)
}

fn days_between(a: Date, b: Date) -> i32 {
    (a - b).get_days().abs()
}

/// R-REC-9: the smaller of the line date and post date distance to the transaction date.
fn day_distance(st: &Statement, line: usize, tx: &Transaction) -> i32 {
    let l = &st.lines[line];
    let d = days_between(l.date, tx.date);
    match l.post_date {
        Some(p) => d.min(days_between(p, tx.date)),
        None => d,
    }
}

struct Edge {
    line: usize,
    tx: usize,
    dist: i32,
    score: f64,
    diff: Decimal,
}

/// Takes `options` greedily in sorted order while both sides are free.
fn take(
    mut options: Vec<Edge>,
    txs: &[&Transaction],
    line_free: &mut [bool],
    tx_free: &mut [bool],
    confidence: Confidence,
    out: &mut Vec<Pair>,
    by_diff: bool,
) {
    options.sort_by(|a, b| {
        a.dist
            .cmp(&b.dist)
            .then_with(|| {
                if by_diff {
                    a.diff.cmp(&b.diff)
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .then_with(|| b.score.total_cmp(&a.score))
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| txs[a.tx].date.cmp(&txs[b.tx].date))
            .then_with(|| txs[a.tx].id.cmp(&txs[b.tx].id))
    });
    for o in options {
        if line_free[o.line] && tx_free[o.tx] {
            line_free[o.line] = false;
            tx_free[o.tx] = false;
            out.push(Pair {
                line: o.line,
                transaction: txs[o.tx].id,
                confidence,
                day_distance: o.dist,
                payee_score: o.score,
            });
        }
    }
}

/// R-REC-9, R-REC-10, R-REC-12: pairs lines (with `amounts` under the chosen reading) to transactions.
pub fn run_matching(
    db: &Db,
    account: AccountId,
    st: &Statement,
    amounts: &[Decimal],
    aliases: &[(String, String)],
) -> Matching {
    let (open, done) = candidates(db, account, st);
    let mut line_free = vec![true; st.lines.len()];
    let mut tx_free = vec![true; open.len()];
    let mut out = Matching::default();

    let score = |line: usize, tx: &Transaction| {
        payee_score(&st.lines[line].description, &tx.payee, aliases)
    };
    for (max_days, confidence) in [(3, Confidence::High), (10, Confidence::Medium)] {
        let mut options = Vec::new();
        for (i, free) in line_free.iter().enumerate() {
            for (j, tx) in open.iter().enumerate() {
                if !*free || !tx_free[j] || amounts[i] != tx.amount {
                    continue;
                }
                let dist = day_distance(st, i, tx);
                if dist <= max_days {
                    options.push(Edge {
                        line: i,
                        tx: j,
                        dist,
                        score: score(i, tx),
                        diff: Decimal::ZERO,
                    });
                }
            }
        }
        take(
            options,
            &open,
            &mut line_free,
            &mut tx_free,
            confidence,
            &mut out.matched,
            false,
        );
    }

    let mut options = Vec::new();
    for (i, free) in line_free.iter().enumerate() {
        for (j, tx) in open.iter().enumerate() {
            if !*free || !tx_free[j] {
                continue;
            }
            let line_amount = amounts[i];
            if line_amount.is_zero()
                || line_amount.is_sign_negative() != tx.amount.is_sign_negative()
            {
                continue;
            }
            let diff = (line_amount - tx.amount).abs();
            let dist = day_distance(st, i, tx);
            let s = score(i, tx);
            if dist <= 3 && s >= 0.5 && diff * Decimal::from(4) <= line_amount.abs() {
                options.push(Edge {
                    line: i,
                    tx: j,
                    dist,
                    score: s,
                    diff,
                });
            }
        }
    }
    take(
        options,
        &open,
        &mut line_free,
        &mut tx_free,
        Confidence::AmountDiffers,
        &mut out.amount_differs,
        true,
    );

    // R-REC-12: leftovers. Each Reconciled transaction explains at most one line.
    let mut done_free = vec![true; done.len()];
    for (i, free) in line_free.iter().enumerate() {
        if !*free {
            continue;
        }
        let best = done
            .iter()
            .enumerate()
            .filter(|(j, tx)| done_free[*j] && tx.amount == amounts[i])
            .map(|(j, tx)| (day_distance(st, i, tx), tx.id, j))
            .filter(|(d, _, _)| *d <= 3)
            .min();
        match best {
            Some((_, id, j)) => {
                done_free[j] = false;
                out.already_reconciled.push((i, id));
            }
            None => out.statement_only.push(i),
        }
    }
    for (j, tx) in open.iter().enumerate() {
        if !tx_free[j] {
            continue;
        }
        if tx.date >= st.period_start && tx.date <= st.period_end {
            out.pf_only_in_period.push(tx.id);
        } else {
            out.pf_only_outside.push(tx.id);
        }
    }

    let line_date = |i: usize| st.lines[i].date;
    out.matched.sort_by_key(|p| (line_date(p.line), p.line));
    out.amount_differs
        .sort_by_key(|p| (line_date(p.line), p.line));
    out.already_reconciled
        .sort_by_key(|(i, _)| (line_date(*i), *i));
    out.statement_only.sort_by_key(|i| (line_date(*i), *i));
    let by_tx: BTreeMap<TransactionId, Date> = open.iter().map(|t| (t.id, t.date)).collect();
    out.pf_only_in_period.sort_by_key(|id| (by_tx[id], *id));
    out.pf_only_outside.sort_by_key(|id| (by_tx[id], *id));
    out
}
