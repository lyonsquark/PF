//! Generic row images used by persistence (`pf-store`) and undo (`pf-engine`).

use serde::{Deserialize, Serialize};

use crate::entities::{
    Account, BudgetItem, Category, Context, ImportMapping, Obligation, Scenario, Transaction,
};
use crate::ids::{
    AccountId, BudgetItemId, CategoryId, ContextId, ImportMappingId, ObligationId, ScenarioId,
    TransactionId,
};

/// One row of any table, carried by value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Record {
    /// An account row.
    Account(Account),
    /// A transaction row.
    Transaction(Transaction),
    /// A category row.
    Category(Category),
    /// A context row.
    Context(Context),
    /// A scenario row.
    Scenario(Scenario),
    /// An obligation row.
    Obligation(Obligation),
    /// A budget item row.
    BudgetItem(BudgetItem),
    /// An import mapping row.
    ImportMapping(ImportMapping),
    /// A key/value setting.
    Setting {
        /// Setting key.
        key: String,
        /// Setting value.
        value: String,
    },
}

/// Identity of one row across all tables.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RowKey {
    /// Account row key.
    Account(AccountId),
    /// Transaction row key.
    Transaction(TransactionId),
    /// Category row key.
    Category(CategoryId),
    /// Context row key.
    Context(ContextId),
    /// Scenario row key.
    Scenario(ScenarioId),
    /// Obligation row key.
    Obligation(ObligationId),
    /// Budget item row key.
    BudgetItem(BudgetItemId),
    /// Import mapping row key.
    ImportMapping(ImportMappingId),
    /// Setting key.
    Setting(String),
}

/// One row's before/after image. `None` = the row does not exist (insert has before=None,
/// hard delete has after=None). A soft delete is an update (`is_deleted` flips).
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    /// The row that changed.
    pub key: RowKey,
    /// Row before the change, `None` if it did not exist.
    pub before: Option<Record>,
    /// Row after the change, `None` if it no longer exists.
    pub after: Option<Record>,
}

impl Record {
    /// The identity of this row.
    pub fn key(&self) -> RowKey {
        match self {
            Record::Account(r) => RowKey::Account(r.id),
            Record::Transaction(r) => RowKey::Transaction(r.id),
            Record::Category(r) => RowKey::Category(r.id),
            Record::Context(r) => RowKey::Context(r.id),
            Record::Scenario(r) => RowKey::Scenario(r.id),
            Record::Obligation(r) => RowKey::Obligation(r.id),
            Record::BudgetItem(r) => RowKey::BudgetItem(r.id),
            Record::ImportMapping(r) => RowKey::ImportMapping(r.id),
            Record::Setting { key, .. } => RowKey::Setting(key.clone()),
        }
    }
}

impl Change {
    /// The inverse change (swap before and after), used by undo.
    pub fn inverted(&self) -> Change {
        Change {
            key: self.key.clone(),
            before: self.after.clone(),
            after: self.before.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enums::{CategoryType, TransactionStatus};
    use crate::testing::d;
    use rust_decimal::Decimal;

    #[test]
    fn change_inverted_swaps() {
        let before = Record::Setting {
            key: "k".into(),
            value: "1".into(),
        };
        let after = Record::Setting {
            key: "k".into(),
            value: "2".into(),
        };
        let change = Change {
            key: RowKey::Setting("k".into()),
            before: Some(before.clone()),
            after: Some(after.clone()),
        };
        let inv = change.inverted();
        assert_eq!(inv.before, Some(after));
        assert_eq!(inv.after, Some(before));
        assert_eq!(inv.key, change.key);
        assert_eq!(inv.inverted(), change);
    }

    #[test]
    fn record_key_matches_row_identity() {
        let c = Category::new(CategoryId(9), "Food", CategoryType::Expense);
        assert_eq!(Record::Category(c).key(), RowKey::Category(CategoryId(9)));
        let mut t = Transaction::new(TransactionId(3), d("2026-01-05"), Decimal::ONE, "x");
        t.status = TransactionStatus::Future;
        assert_eq!(
            Record::Transaction(t).key(),
            RowKey::Transaction(TransactionId(3))
        );
    }
}
