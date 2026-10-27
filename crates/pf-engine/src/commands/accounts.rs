//! Account commands (T10a): create, edit, delete and the starting-balance section.
//! Rules R-ACC-1..3; the Swift source is `Views/ContentView.swift` (`EditAccountView`).

use jiff::civil::Date;
use pf_model::{
    Account, AccountId, AccountType, Db, ObligationId, RowKey, Transaction, TransactionId,
    TransactionStatus,
};
use rust_decimal::Decimal;

use crate::commands::transactions::is_locked;
use crate::engine::Engine;
use crate::error::{EngineError, Result};

/// Payee of an account's opening balance row (R-ACC-2).
const STARTING_BALANCE_PAYEE: &str = "Starting Balance";
/// Memo written on a newly created starting-balance row (R-ACC-2).
const STARTING_BALANCE_MEMO: &str = "Initial account balance";
/// Currency used when the form leaves the field empty.
const DEFAULT_CURRENCY: &str = "USD";

/// Form values from Add/Edit Account.
#[derive(Debug, Clone, PartialEq)]
pub struct AccountInput {
    /// Display name; trimmed before saving.
    pub name: String,
    /// Kind of account.
    pub account_type: AccountType,
    /// Institution name, stored as typed.
    pub institution: String,
    /// ISO currency code; empty means USD.
    pub currency: String,
    /// External accounts are excluded from totals.
    pub is_external: bool,
}

/// Read-only data for the Edit Account "Starting Balance" section (R-ACC-2).
#[derive(Debug, Clone, PartialEq)]
pub struct StartingBalanceInfo {
    /// The live reality row with payee "Starting Balance" (first by id), if any.
    pub existing: Option<TransactionId>,
    /// Amount of that row, or zero.
    pub amount: Decimal,
    /// Earliest date of the other live reality rows, if any.
    pub earliest_other: Option<Date>,
    /// The day before `earliest_other`, or the day before today when there are none.
    pub balance_date: Date,
    /// Sum of the other live reality rows (the starting-balance row excluded).
    pub other_sum: Decimal,
}

/// Trims the name and fills the default currency. `Validation` for an empty name.
fn normalize(input: AccountInput) -> Result<AccountInput> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(EngineError::Validation("Account name is required".into()));
    }
    let currency = input.currency.trim();
    let currency = if currency.is_empty() {
        DEFAULT_CURRENCY.to_string()
    } else {
        currency.to_string()
    };
    Ok(AccountInput {
        name,
        currency,
        ..input
    })
}

/// Builds an account row from normalized form values.
fn account_from(id: AccountId, input: &AccountInput) -> Account {
    Account {
        currency: input.currency.clone(),
        is_external: input.is_external,
        institution: input.institution.clone(),
        ..Account::new(id, &input.name, input.account_type, &input.institution)
    }
}

/// The error for a missing or soft-deleted account.
fn account_not_found(id: AccountId) -> EngineError {
    EngineError::NotFound(format!("account {}", id.raw()))
}

/// The day before `d`. Falls back to `d` at the calendar limit, where no earlier day exists.
fn day_before(d: Date) -> Date {
    d.yesterday().unwrap_or(d)
}

/// "Add Account". The name is trimmed; an empty name gives `Validation("Account name is required")`;
/// an empty currency becomes "USD".
pub fn create_account(e: &mut Engine, input: AccountInput) -> Result<AccountId> {
    let input = normalize(input)?;
    e.apply("Add Account", |w| {
        Ok(w.insert_account(account_from(AccountId(0), &input)))
    })
}

/// "Edit Account". Same validation as `create_account`. `NotFound` if the id is unknown.
pub fn update_account(e: &mut Engine, id: AccountId, input: AccountInput) -> Result<()> {
    let input = normalize(input)?;
    e.apply("Edit Account", |w| {
        w.update_account(id, |a| {
            a.name = input.name.clone();
            a.account_type = input.account_type;
            a.institution = input.institution.clone();
            a.currency = input.currency.clone();
            a.is_external = input.is_external;
        })
    })
}

/// "Delete Account". R-ACC-1: soft-deletes the account, its live transactions and its live
/// obligations as one step. `NotFound` if the account is missing or already deleted.
pub fn delete_account(e: &mut Engine, id: AccountId) -> Result<()> {
    let db = e.db();
    if db.account(id).is_none_or(|a| a.is_deleted) {
        return Err(account_not_found(id));
    }
    // Collect the children first: the apply closure borrows the writer.
    let transactions: Vec<TransactionId> = db.transactions_for_account(id).map(|t| t.id).collect();
    let obligations: Vec<ObligationId> = db.obligations_for_account(id).map(|o| o.id).collect();
    e.apply("Delete Account", |w| {
        w.set_deleted(RowKey::Account(id), true)?;
        for t in transactions {
            w.set_deleted(RowKey::Transaction(t), true)?;
        }
        for o in obligations {
            w.set_deleted(RowKey::Obligation(o), true)?;
        }
        Ok(())
    })
}

/// Read-only data for the Edit Account "Starting Balance" section. R-ACC-2: "other" rows are the
/// live reality rows (no scenario) whose payee is not "Starting Balance".
pub fn starting_balance_info(db: &Db, id: AccountId, today: Date) -> StartingBalanceInfo {
    let reality = || {
        db.transactions_for_account(id)
            .filter(|t| t.scenario_id.is_none())
    };
    let existing = reality()
        .filter(|t| t.payee == STARTING_BALANCE_PAYEE)
        .min_by_key(|t| t.id);
    let others: Vec<&Transaction> = reality()
        .filter(|t| t.payee != STARTING_BALANCE_PAYEE)
        .collect();
    let earliest_other = others.iter().map(|t| t.date).min();
    let other_sum = others.iter().map(|t| t.amount).sum();
    let balance_date = day_before(earliest_other.unwrap_or(today));
    StartingBalanceInfo {
        existing: existing.map(|t| t.id),
        amount: existing.map_or(Decimal::ZERO, |t| t.amount),
        earliest_other,
        balance_date,
        other_sum,
    }
}

/// R-ACC-3: starting = current − other_sum (the "Calculate" button).
pub fn calculate_starting_balance(current: Decimal, other_sum: Decimal) -> Decimal {
    current - other_sum
}

/// "Set Starting Balance". R-ACC-2: an existing row gets the new amount and date; otherwise a
/// non-zero amount creates the row (Cleared, memo "Initial account balance"). With no other rows
/// and a zero amount there is nothing to do. A locked existing row is refused (R-TXN-1).
pub fn set_starting_balance(e: &mut Engine, id: AccountId, amount: Decimal) -> Result<()> {
    if e.db().account(id).is_none_or(|a| a.is_deleted) {
        return Err(account_not_found(id));
    }
    let info = starting_balance_info(e.db(), id, e.today());
    // R-ACC-2 (Swift `guard`): nothing to do without other rows and with a zero amount.
    if info.earliest_other.is_none() && amount.is_zero() {
        return Ok(());
    }
    if info
        .existing
        .is_some_and(|row| is_locked(e.db(), &e.state, row))
    {
        return Err(EngineError::Locked);
    }
    e.apply("Set Starting Balance", |w| {
        match info.existing {
            Some(row) => w.update_transaction(row, |t| {
                t.amount = amount;
                t.date = info.balance_date;
            })?,
            None if !amount.is_zero() => {
                let mut t = Transaction::new(
                    TransactionId(0),
                    info.balance_date,
                    amount,
                    STARTING_BALANCE_PAYEE,
                );
                t.memo = Some(STARTING_BALANCE_MEMO.into());
                t.status = TransactionStatus::Cleared;
                t.account_id = Some(id);
                w.insert_transaction(t);
            }
            None => {}
        }
        Ok(())
    })
}
