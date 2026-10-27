//! Account and dashboard balances and running balances: pure reads over `Db`.

use std::collections::HashMap;

use pf_model::*;
use rust_decimal::Decimal;

use crate::obligation;

/// R-SCN-1: is `tx` visible under `active` (None = Reality)? Deleted rows are never visible.
pub fn is_visible(tx: &Transaction, active: Option<ScenarioId>) -> bool {
    if tx.is_deleted {
        return false;
    }
    match tx.scenario_id {
        None => true,
        Some(s) => active == Some(s),
    }
}

/// R-SCN-2: new active scenario after the user picks `clicked` (None = "Reality") while `current` is active.
/// Same id gives None; otherwise clicked.
pub fn toggle_scenario(
    current: Option<ScenarioId>,
    clicked: Option<ScenarioId>,
) -> Option<ScenarioId> {
    if current == clicked { None } else { clicked }
}

/// `Cleared` or `Reconciled`.
pub fn is_cleared(status: TransactionStatus) -> bool {
    matches!(
        status,
        TransactionStatus::Cleared | TransactionStatus::Reconciled
    )
}

/// R-BAL-1: sum of visible amounts for the account (all statuses).
pub fn account_balance(db: &Db, account: AccountId, active: Option<ScenarioId>) -> Decimal {
    db.transactions_for_account(account)
        .filter(|tx| is_visible(tx, active))
        .map(|tx| tx.amount)
        .sum()
}

/// R-BAL-2: same, cleared/reconciled only.
pub fn account_cleared_balance(db: &Db, account: AccountId, active: Option<ScenarioId>) -> Decimal {
    db.transactions_for_account(account)
        .filter(|tx| is_visible(tx, active) && is_cleared(tx.status))
        .map(|tx| tx.amount)
        .sum()
}

/// R-BAL-1..4: everything the dashboard needs in ONE pass over live transactions.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BalanceMaps {
    /// Visible under active (R-BAL-1).
    pub all: HashMap<AccountId, Decimal>,
    /// Visible and cleared (R-BAL-2).
    pub cleared: HashMap<AccountId, Decimal>,
    /// scenario_id == None (R-BAL-3).
    pub reality_all: HashMap<AccountId, Decimal>,
    /// Reality and cleared (R-BAL-3).
    pub reality_cleared: HashMap<AccountId, Decimal>,
}

/// Adds `amount` to the account's entry, creating it at zero.
fn add_to(map: &mut HashMap<AccountId, Decimal>, account: AccountId, amount: Decimal) {
    *map.entry(account).or_insert(Decimal::ZERO) += amount;
}

/// R-BAL-4: account None is skipped. One pass over `db.live_transactions()`.
pub fn balance_maps(db: &Db, active: Option<ScenarioId>) -> BalanceMaps {
    let mut maps = BalanceMaps::default();
    for tx in db.live_transactions() {
        // R-BAL-4: orphans contribute to no balance
        let Some(account) = tx.account_id else {
            continue;
        };
        let cleared = is_cleared(tx.status);
        // R-BAL-3: reality maps ignore the active scenario
        if tx.scenario_id.is_none() {
            add_to(&mut maps.reality_all, account, tx.amount);
            if cleared {
                add_to(&mut maps.reality_cleared, account, tx.amount);
            }
        }
        // R-BAL-1, R-BAL-2
        if is_visible(tx, active) {
            add_to(&mut maps.all, account, tx.amount);
            if cleared {
                add_to(&mut maps.cleared, account, tx.amount);
            }
        }
    }
    maps
}

/// R-BAL-6: the maps the dashboard displays, as (cleared, all). With `show_available`, subtracts
/// `obligation::total_reserved(db, account, None)` from both maps for every live account.
pub fn displayed_balances(
    db: &Db,
    maps: &BalanceMaps,
    show_available: bool,
) -> (HashMap<AccountId, Decimal>, HashMap<AccountId, Decimal>) {
    let mut cleared = maps.cleared.clone();
    let mut all = maps.all.clone();
    if show_available {
        for account in db.live_accounts() {
            // R-BAL-6: reserved is always computed under Reality
            let reserved = obligation::total_reserved(db, account.id, None);
            add_to(&mut cleared, account.id, -reserved);
            add_to(&mut all, account.id, -reserved);
        }
    }
    (cleared, all)
}

/// R-BAL-5: summary cards, computed from a cleared map.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DashboardTotals {
    /// All non-external accounts.
    pub net_worth: Decimal,
    /// Non-external checking, savings, cash and investment.
    pub assets: Decimal,
    /// Non-external credit.
    pub credit_debt: Decimal,
    /// Non-external loan.
    pub loan_debt: Decimal,
}

/// R-BAL-5: summary cards from a cleared map. External accounts are excluded throughout.
pub fn dashboard_totals(db: &Db, cleared: &HashMap<AccountId, Decimal>) -> DashboardTotals {
    let mut totals = DashboardTotals::default();
    for account in db.live_accounts().filter(|a| !a.is_external) {
        let value = cleared.get(&account.id).copied().unwrap_or(Decimal::ZERO);
        totals.net_worth += value;
        match account.account_type {
            AccountType::Checking
            | AccountType::Savings
            | AccountType::Cash
            | AccountType::Investment => {
                totals.assets += value;
            }
            AccountType::Credit => totals.credit_debt += value,
            AccountType::Loan => totals.loan_debt += value,
        }
    }
    totals
}

/// R-BAL-9: one dashboard group.
#[derive(Debug, Clone, PartialEq)]
pub struct AccountGroup {
    /// Group heading.
    pub title: &'static str,
    /// Accounts in the group, sorted by name.
    pub accounts: Vec<AccountId>,
}

/// R-BAL-9: dashboard account groups, in order, empty groups omitted.
pub fn dashboard_groups(db: &Db) -> Vec<AccountGroup> {
    const GROUPS: [(&str, &[AccountType]); 5] = [
        (
            "Cash & Checking",
            &[AccountType::Checking, AccountType::Cash],
        ),
        ("Savings", &[AccountType::Savings]),
        ("Credit Cards", &[AccountType::Credit]),
        ("Loans", &[AccountType::Loan]),
        ("Investments", &[AccountType::Investment]),
    ];
    let sorted = db.accounts_by_name();
    GROUPS
        .iter()
        .filter_map(|&(title, types)| {
            let accounts: Vec<AccountId> = sorted
                .iter()
                .filter(|a| types.contains(&a.account_type))
                .map(|a| a.id)
                .collect();
            (!accounts.is_empty()).then_some(AccountGroup { title, accounts })
        })
        .collect()
}

/// The rows in (date, id) ascending order, skipping deleted rows.
fn live_in_date_order<'a>(txs: &[&'a Transaction]) -> Vec<&'a Transaction> {
    let mut ordered: Vec<&Transaction> = txs.iter().copied().filter(|t| !t.is_deleted).collect();
    ordered.sort_by_key(|t| (t.date, t.id));
    ordered
}

/// R-BAL-7: running balances, computed oldest to newest by (date, id), per account.
pub fn running_balances(txs: &[&Transaction]) -> HashMap<TransactionId, Decimal> {
    let mut by_account: HashMap<AccountId, Decimal> = HashMap::new();
    let mut out = HashMap::new();
    for tx in live_in_date_order(txs) {
        let Some(account) = tx.account_id else {
            continue;
        };
        let running = by_account.entry(account).or_insert(Decimal::ZERO);
        *running += tx.amount;
        out.insert(tx.id, *running);
    }
    out
}

/// R-BAL-8: cumulative sum over `txs` in (date, id) order, ignoring accounts.
pub fn search_running_balances(txs: &[&Transaction]) -> HashMap<TransactionId, Decimal> {
    let mut running = Decimal::ZERO;
    let mut out = HashMap::new();
    for tx in live_in_date_order(txs) {
        running += tx.amount;
        out.insert(tx.id, running);
    }
    out
}

#[cfg(test)]
mod tests;
