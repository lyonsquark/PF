//! Entity structs mirroring `docs/data-model.md`. Constructors only; no business logic.

use jiff::Timestamp;
use jiff::civil::Date;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::enums::{
    AccountType, BudgetFrequency, BudgetItemType, CategoryType, ObligationStatus, TransactionStatus,
};
use crate::ids::{
    AccountId, BudgetItemId, CategoryId, ContextId, ImportMappingId, ObligationId, ScenarioId,
    TransactionId,
};

/// A bank, card, loan or cash account.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Account {
    /// Primary key.
    pub id: AccountId,
    /// Display name.
    pub name: String,
    /// Kind of account.
    pub account_type: AccountType,
    /// Institution name.
    pub institution: String,
    /// ISO currency code, default "USD".
    pub currency: String,
    /// External accounts are excluded from net worth and totals.
    pub is_external: bool,
    /// Soft-delete flag.
    pub is_deleted: bool,
}

impl Account {
    /// New live, internal USD account.
    pub fn new(id: AccountId, name: &str, account_type: AccountType, institution: &str) -> Self {
        Self {
            id,
            name: name.to_string(),
            account_type,
            institution: institution.to_string(),
            currency: "USD".to_string(),
            is_external: false,
            is_deleted: false,
        }
    }
}

/// A money movement. Positive amounts are money in, negative are money out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transaction {
    /// Primary key.
    pub id: TransactionId,
    /// Calendar date of the movement.
    pub date: Date,
    /// Signed amount.
    pub amount: Decimal,
    /// Payee text.
    pub payee: String,
    /// Optional memo.
    pub memo: Option<String>,
    /// Optional attachment path.
    pub attachment_path: Option<String>,
    /// Reconciliation status.
    pub status: TransactionStatus,
    /// Soft-delete flag.
    pub is_deleted: bool,
    /// Owning account; `None` marks an orphaned transaction.
    pub account_id: Option<AccountId>,
    /// Owning scenario; `None` means Reality.
    pub scenario_id: Option<ScenarioId>,
    /// Category, if any.
    pub category_id: Option<CategoryId>,
    /// Contexts (tags), order preserved.
    pub context_ids: Vec<ContextId>,
}

impl Transaction {
    /// New live Cleared transaction in Reality with no account, category or contexts.
    pub fn new(id: TransactionId, date: Date, amount: Decimal, payee: &str) -> Self {
        Self {
            id,
            date,
            amount,
            payee: payee.to_string(),
            memo: None,
            attachment_path: None,
            status: TransactionStatus::Cleared,
            is_deleted: false,
            account_id: None,
            scenario_id: None,
            category_id: None,
            context_ids: Vec::new(),
        }
    }
}

/// A category in the income / expense / transfer tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Category {
    /// Primary key.
    pub id: CategoryId,
    /// Name (without parent prefix).
    pub name: String,
    /// SF Symbol name or emoji.
    pub icon: Option<String>,
    /// Free-form notes.
    pub notes: Option<String>,
    /// Auto-match patterns; an empty Vec means Swift's `nil`.
    pub match_patterns: Vec<String>,
    /// Income, expense or transfer.
    pub category_type: CategoryType,
    /// Parent category, if this is a subcategory.
    pub parent_id: Option<CategoryId>,
    /// Soft-delete flag.
    pub is_deleted: bool,
}

impl Category {
    /// New live top-level category with no patterns.
    pub fn new(id: CategoryId, name: &str, category_type: CategoryType) -> Self {
        Self {
            id,
            name: name.to_string(),
            icon: None,
            notes: None,
            match_patterns: Vec::new(),
            category_type,
            parent_id: None,
            is_deleted: false,
        }
    }
}

/// A context (tag), shown in the UI as `#name`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Context {
    /// Primary key.
    pub id: ContextId,
    /// Name.
    pub name: String,
    /// `#RRGGBB` or `#RRGGBBAA`.
    pub color_hex: Option<String>,
    /// Free-form notes.
    pub notes: Option<String>,
    /// Archived contexts are listed in their own section. Swift `nil` means false.
    pub is_archived: bool,
    /// Soft-delete flag.
    pub is_deleted: bool,
}

impl Context {
    /// New live, unarchived context.
    pub fn new(id: ContextId, name: &str) -> Self {
        Self {
            id,
            name: name.to_string(),
            color_hex: None,
            notes: None,
            is_archived: false,
            is_deleted: false,
        }
    }
}

/// A what-if scenario. Transactions with a scenario id belong to it, not to Reality.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scenario {
    /// Primary key.
    pub id: ScenarioId,
    /// Name.
    pub name: String,
    /// `#RRGGBB` or `#RRGGBBAA`.
    pub color_hex: Option<String>,
    /// Free-form notes.
    pub notes: Option<String>,
    /// Soft-delete flag.
    pub is_deleted: bool,
}

impl Scenario {
    /// New live scenario with no colour or notes.
    pub fn new(id: ScenarioId, name: &str) -> Self {
        Self {
            id,
            name: name.to_string(),
            color_hex: None,
            notes: None,
            is_deleted: false,
        }
    }
}

/// A saved-up amount or payment owed, optionally linked to an account and context.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Obligation {
    /// Primary key.
    pub id: ObligationId,
    /// Name.
    pub name: String,
    /// Optional target amount.
    pub target_amount: Option<Decimal>,
    /// Manually funded starting amount.
    pub current_balance: Decimal,
    /// Optional due date.
    pub due_date: Option<Date>,
    /// Free-form notes.
    pub notes: Option<String>,
    /// Lifecycle status.
    pub status: ObligationStatus,
    /// Creation timestamp, if known.
    pub created_date: Option<Timestamp>,
    /// Last update timestamp, if known.
    pub last_updated_date: Option<Timestamp>,
    /// Linked account.
    pub account_id: Option<AccountId>,
    /// Linked context.
    pub context_id: Option<ContextId>,
    /// Soft-delete flag.
    pub is_deleted: bool,
}

impl Obligation {
    /// New live obligation in status Funding with a zero balance.
    pub fn new(id: ObligationId, name: &str) -> Self {
        Self {
            id,
            name: name.to_string(),
            target_amount: None,
            current_balance: Decimal::ZERO,
            due_date: None,
            notes: None,
            status: ObligationStatus::Funding,
            created_date: None,
            last_updated_date: None,
            account_id: None,
            context_id: None,
            is_deleted: false,
        }
    }
}

/// A planned bill, expense or income that projects into scenarios.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BudgetItem {
    /// Primary key.
    pub id: BudgetItemId,
    /// Bill, expense or income.
    pub item_type: BudgetItemType,
    /// Payee for a bill, category-ish name for an expense, source for income.
    pub name: String,
    /// Amount per occurrence.
    pub amount: Decimal,
    /// First occurrence.
    pub start_date: Date,
    /// Optional last date to project.
    pub end_date: Option<Date>,
    /// How often it recurs.
    pub frequency: BudgetFrequency,
    /// Months 1..=12, used only for `SpecificMonths`.
    pub specific_months: Vec<u32>,
    /// Free-form notes.
    pub notes: Option<String>,
    /// Category for analytical tracking.
    pub category_id: Option<CategoryId>,
    /// Originating account.
    pub account_id: Option<AccountId>,
    /// Soft-delete flag.
    pub is_deleted: bool,
}

impl BudgetItem {
    /// New live budget item with no end date, months, notes, category or account.
    pub fn new(
        id: BudgetItemId,
        item_type: BudgetItemType,
        name: &str,
        amount: Decimal,
        start_date: Date,
        frequency: BudgetFrequency,
    ) -> Self {
        Self {
            id,
            item_type,
            name: name.to_string(),
            amount,
            start_date,
            end_date: None,
            frequency,
            specific_months: Vec::new(),
            notes: None,
            category_id: None,
            account_id: None,
            is_deleted: false,
        }
    }
}

/// A CSV import mapping: column layouts, account names, category and payee translations, or settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportMapping {
    /// Primary key.
    pub id: ImportMappingId,
    /// One of "account", "category", "payee", "column", "setting", "statementPayee".
    pub mapping_type: String,
    /// Raw source value (CSV header or bank value).
    pub source_value: String,
    /// Mapped app value.
    pub target_value: String,
    /// Per-account sign inversion.
    pub negate_amounts: bool,
    /// `None` = global mapping; `Some` = per-bank-account mapping.
    pub account_id: Option<AccountId>,
}

impl ImportMapping {
    /// New mapping with `negate_amounts` false.
    pub fn new(
        id: ImportMappingId,
        mapping_type: &str,
        source: &str,
        target: &str,
        account_id: Option<AccountId>,
    ) -> Self {
        Self {
            id,
            mapping_type: mapping_type.to_string(),
            source_value: source.to_string(),
            target_value: target.to_string(),
            negate_amounts: false,
            account_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_set_documented_defaults() {
        let a = Account::new(AccountId(1), "Checking", AccountType::Checking, "Bank");
        assert_eq!(a.currency, "USD");
        assert!(!a.is_external && !a.is_deleted);
        let c = Category::new(CategoryId(2), "Food", CategoryType::Expense);
        assert!(c.match_patterns.is_empty() && c.parent_id.is_none());
        let o = Obligation::new(ObligationId(3), "Car");
        assert_eq!(o.status, ObligationStatus::Funding);
        assert_eq!(o.current_balance, Decimal::ZERO);
        let ctx = Context::new(ContextId(4), "Trip");
        assert!(!ctx.is_archived);
    }
}
