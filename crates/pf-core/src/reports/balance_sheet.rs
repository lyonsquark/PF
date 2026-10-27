//! Balance sheet as of a date (R-RPT-4 .. R-RPT-4d).

use std::collections::BTreeMap;

use jiff::civil::Date;
use pf_model::{AccountId, Db, ObligationId, ObligationStatus, ScenarioId, TransactionStatus};
use rust_decimal::Decimal;

use crate::balance::is_visible;

/// One non-external account in the balance sheet (R-RPT-4a).
#[derive(Debug, Clone, PartialEq)]
pub struct BsAccountRow {
    /// Account id.
    pub account_id: AccountId,
    /// Account name.
    pub name: String,
    /// Institution, possibly "" (shown under the name when non-empty).
    pub institution: String,
    /// Sum of visible amounts dated on or before the as-of date.
    pub total: Decimal,
    /// The part of `total` whose status is Pending or Future.
    pub pending: Decimal,
}

/// One active obligation (R-RPT-4c).
#[derive(Debug, Clone, PartialEq)]
pub struct BsObligationRow {
    /// Obligation id.
    pub obligation_id: ObligationId,
    /// Obligation name.
    pub name: String,
    /// The stored `current_balance` (not the computed balance of R-OBL-1).
    pub balance: Decimal,
    /// True for Funding (shown in normal colour); Spending is shown red.
    pub is_funding: bool,
}

/// The balance sheet as of one date.
#[derive(Debug, Clone, PartialEq)]
pub struct BalanceSheet {
    /// The as-of date.
    pub as_of: Date,
    /// Non-external live accounts, sorted by name.
    pub accounts: Vec<BsAccountRow>,
    /// Σ account totals (R-RPT-4b).
    pub total: Decimal,
    /// Σ account pending amounts.
    pub pending_total: Decimal,
    /// Active obligations in name order. Empty means the section is hidden.
    pub obligations: Vec<BsObligationRow>,
    /// Σ obligation balances.
    pub total_obligations: Decimal,
    /// Net worth: equal to `total` (R-RPT-4d).
    pub net_worth: Decimal,
    /// Net worth − total obligations (R-RPT-4d).
    pub available: Decimal,
}

/// R-RPT-4 .. 4d: the balance sheet as of `as_of` (the start date is not used). Orphan
/// transactions are not in any account, so they are left out.
pub fn balance_sheet(db: &Db, as_of: Date, scenario: Option<ScenarioId>) -> BalanceSheet {
    // R-RPT-4a: one pass over visible transactions dated on or before `as_of`.
    let mut sums: BTreeMap<AccountId, (Decimal, Decimal)> = BTreeMap::new();
    for tx in db.live_transactions() {
        if !is_visible(tx, scenario) || tx.date > as_of {
            continue;
        }
        let Some(account) = tx.account_id else {
            continue;
        };
        let entry = sums
            .entry(account)
            .or_insert((Decimal::ZERO, Decimal::ZERO));
        entry.0 += tx.amount;
        if matches!(
            tx.status,
            TransactionStatus::Pending | TransactionStatus::Future
        ) {
            entry.1 += tx.amount;
        }
    }

    // R-RPT-4: non-external accounts sorted by name.
    let accounts: Vec<BsAccountRow> = db
        .accounts_by_name()
        .into_iter()
        .filter(|a| !a.is_external)
        .map(|a| {
            let (total, pending) = sums
                .get(&a.id)
                .copied()
                .unwrap_or((Decimal::ZERO, Decimal::ZERO));
            BsAccountRow {
                account_id: a.id,
                name: a.name.clone(),
                institution: a.institution.clone(),
                total,
                pending,
            }
        })
        .collect();
    // R-RPT-4b: totals are sums over the listed accounts.
    let total: Decimal = accounts.iter().map(|a| a.total).sum();
    let pending_total: Decimal = accounts.iter().map(|a| a.pending).sum();

    // R-RPT-4c: active obligations (Funding or Spending), name order, stored balance.
    let obligations: Vec<BsObligationRow> = db
        .obligations_by_name()
        .into_iter()
        .filter(|o| {
            matches!(
                o.status,
                ObligationStatus::Funding | ObligationStatus::Spending
            )
        })
        .map(|o| BsObligationRow {
            obligation_id: o.id,
            name: o.name.clone(),
            balance: o.current_balance,
            is_funding: o.status == ObligationStatus::Funding,
        })
        .collect();
    let total_obligations: Decimal = obligations.iter().map(|o| o.balance).sum();

    // R-RPT-4d: net worth and available.
    BalanceSheet {
        as_of,
        accounts,
        total,
        pending_total,
        obligations,
        total_obligations,
        net_worth: total,
        available: total - total_obligations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::testing::{TestDb, d};
    use pf_model::{AccountType, ObligationStatus};
    use rust_decimal_macros::dec;

    #[test]
    fn r_rpt_4_orphans_external_and_cutoff() {
        // Covers: R-RPT-4, R-RPT-4a
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let ext = t.external_account("Brokerage", AccountType::Investment);
        t.tx(chk, "2026-03-15", "100", "on cutoff").done();
        t.tx(chk, "2026-03-16", "1000", "after cutoff").done();
        t.tx(ext, "2026-01-01", "5000", "external").done();
        t.orphan_tx("2026-01-01", "77", "orphan").done();
        t.tx(chk, "2026-01-01", "50", "future pending")
            .status(TransactionStatus::Future)
            .done();
        t.tx(chk, "2026-01-02", "1", "pending")
            .status(TransactionStatus::Pending)
            .done();
        t.obligation("Fund")
            .status(ObligationStatus::Spending)
            .balance("20")
            .done();
        let db = t.build();

        let sheet = balance_sheet(&db, d("2026-03-15"), None);
        assert_eq!(sheet.as_of, d("2026-03-15"));
        assert_eq!(sheet.accounts.len(), 1);
        assert_eq!(sheet.accounts[0].name, "Checking");
        assert_eq!(sheet.accounts[0].total, dec!(151));
        assert_eq!(sheet.accounts[0].pending, dec!(51));
        assert_eq!(sheet.total, dec!(151));
        assert_eq!(sheet.total_obligations, dec!(20));
        assert_eq!(sheet.available, dec!(131));
        assert!(!sheet.obligations[0].is_funding);
    }
}
