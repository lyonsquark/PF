//! `TestDb`: a builder for test data. Compiled for pf-model's own tests and whenever another
//! crate enables the `test-support` feature. Panics on bad input are intended here.

use std::fmt::Display;
use std::str::FromStr;

use jiff::civil::Date;
use rust_decimal::Decimal;

use crate::db::Db;
use crate::entities::{
    Account, BudgetItem, Category, Context, ImportMapping, Obligation, Scenario, Transaction,
};
use crate::enums::{
    AccountType, BudgetFrequency, BudgetItemType, CategoryType, ObligationStatus, TransactionStatus,
};
use crate::ids::{
    AccountId, BudgetItemId, CategoryId, ContextId, ImportMappingId, ObligationId, ScenarioId,
    TransactionId,
};
use crate::record::RowKey;

/// `"2026-01-05"` to a [`Date`]. Panics on bad input.
pub fn d(s: &str) -> Date {
    s.parse::<Date>()
        .unwrap_or_else(|e| panic!("bad test date {s:?}: {e}"))
}

/// `"-12.50"` to an exact [`Decimal`] (keeps its scale). Panics on bad input.
pub fn m(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap_or_else(|e| panic!("bad test amount {s:?}: {e}"))
}

/// Sets `is_deleted` through a flag reference, panicking when the row does not exist.
fn mark_deleted(flag: Option<&mut bool>, what: &str, id: impl Display) {
    match flag {
        Some(f) => *f = true,
        None => panic!("delete: no {what} with id {id}"),
    }
}

/// Builds a [`Db`] for tests. Every builder method allocates ids from the Db's sequence.
#[derive(Debug, Default)]
pub struct TestDb {
    db: Db,
}

impl TestDb {
    /// An empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Live account, institution "Test Bank", currency USD, not external.
    pub fn account(&mut self, name: &str, t: AccountType) -> AccountId {
        let id = AccountId(self.db.alloc_id());
        self.db
            .accounts
            .insert(id, Account::new(id, name, t, "Test Bank"));
        id
    }

    /// Like [`TestDb::account`], but external (excluded from net worth).
    pub fn external_account(&mut self, name: &str, t: AccountType) -> AccountId {
        let id = AccountId(self.db.alloc_id());
        let mut a = Account::new(id, name, t, "Test Bank");
        a.is_external = true;
        self.db.accounts.insert(id, a);
        id
    }

    /// Live scenario.
    pub fn scenario(&mut self, name: &str) -> ScenarioId {
        let id = ScenarioId(self.db.alloc_id());
        self.db.scenarios.insert(id, Scenario::new(id, name));
        id
    }

    /// Live top-level category.
    pub fn category(&mut self, name: &str, t: CategoryType) -> CategoryId {
        let id = CategoryId(self.db.alloc_id());
        self.db.categories.insert(id, Category::new(id, name, t));
        id
    }

    /// Live subcategory with the same type as `parent`. Panics if the parent does not exist.
    pub fn child_category(&mut self, parent: CategoryId, name: &str) -> CategoryId {
        let t = match self.db.categories.get(&parent) {
            Some(p) => p.category_type,
            None => panic!("child_category: no parent with id {parent}"),
        };
        let id = CategoryId(self.db.alloc_id());
        let mut c = Category::new(id, name, t);
        c.parent_id = Some(parent);
        self.db.categories.insert(id, c);
        id
    }

    /// Replaces the category's match patterns. Panics if the category does not exist.
    pub fn patterns(&mut self, cat: CategoryId, patterns: &[&str]) {
        let c = self
            .db
            .categories
            .get_mut(&cat)
            .unwrap_or_else(|| panic!("patterns: no category with id {cat}"));
        c.match_patterns = patterns.iter().map(|p| p.to_string()).collect();
    }

    /// Live context.
    pub fn context(&mut self, name: &str) -> ContextId {
        let id = ContextId(self.db.alloc_id());
        self.db.contexts.insert(id, Context::new(id, name));
        id
    }

    /// Sets a setting.
    pub fn setting(&mut self, key: &str, value: &str) {
        self.db.settings.insert(key.to_string(), value.to_string());
    }

    /// Adds an import mapping.
    pub fn import_mapping(
        &mut self,
        mapping_type: &str,
        source: &str,
        target: &str,
        account: Option<AccountId>,
    ) -> ImportMappingId {
        let id = ImportMappingId(self.db.alloc_id());
        self.db.import_mappings.insert(
            id,
            ImportMapping::new(id, mapping_type, source, target, account),
        );
        id
    }

    /// Starts a transaction in Reality, status Cleared, with no category, contexts or memo.
    pub fn tx(
        &mut self,
        account: AccountId,
        date: &str,
        amount: &str,
        payee: &str,
    ) -> TxBuilder<'_> {
        let mut tx = self.new_tx(date, amount, payee);
        tx.account_id = Some(account);
        TxBuilder { db: self, tx }
    }

    /// Like [`TestDb::tx`], but with `account_id = None` (an orphan).
    pub fn orphan_tx(&mut self, date: &str, amount: &str, payee: &str) -> TxBuilder<'_> {
        let tx = self.new_tx(date, amount, payee);
        TxBuilder { db: self, tx }
    }

    fn new_tx(&mut self, date: &str, amount: &str, payee: &str) -> Transaction {
        let id = TransactionId(self.db.alloc_id());
        Transaction::new(id, d(date), m(amount), payee)
    }

    /// Starts an obligation in status Funding with a zero balance.
    pub fn obligation(&mut self, name: &str) -> ObligationBuilder<'_> {
        let id = ObligationId(self.db.alloc_id());
        ObligationBuilder {
            db: self,
            ob: Obligation::new(id, name),
        }
    }

    /// Starts a budget item with no end date, months, notes, category or account.
    pub fn budget_item(
        &mut self,
        t: BudgetItemType,
        name: &str,
        amount: &str,
        start: &str,
        f: BudgetFrequency,
    ) -> BudgetItemBuilder<'_> {
        let id = BudgetItemId(self.db.alloc_id());
        let item = BudgetItem::new(id, t, name, m(amount), d(start), f);
        BudgetItemBuilder { db: self, item }
    }

    /// Soft-deletes a row (sets `is_deleted`). Panics for import mappings and settings,
    /// which have no soft delete, and for rows that do not exist.
    pub fn delete(&mut self, key: RowKey) {
        let db = &mut self.db;
        match key {
            RowKey::Account(id) => mark_deleted(
                db.accounts.get_mut(&id).map(|r| &mut r.is_deleted),
                "account",
                id,
            ),
            RowKey::Transaction(id) => mark_deleted(
                db.transactions.get_mut(&id).map(|r| &mut r.is_deleted),
                "transaction",
                id,
            ),
            RowKey::Category(id) => mark_deleted(
                db.categories.get_mut(&id).map(|r| &mut r.is_deleted),
                "category",
                id,
            ),
            RowKey::Context(id) => mark_deleted(
                db.contexts.get_mut(&id).map(|r| &mut r.is_deleted),
                "context",
                id,
            ),
            RowKey::Scenario(id) => mark_deleted(
                db.scenarios.get_mut(&id).map(|r| &mut r.is_deleted),
                "scenario",
                id,
            ),
            RowKey::Obligation(id) => mark_deleted(
                db.obligations.get_mut(&id).map(|r| &mut r.is_deleted),
                "obligation",
                id,
            ),
            RowKey::BudgetItem(id) => mark_deleted(
                db.budget_items.get_mut(&id).map(|r| &mut r.is_deleted),
                "budget item",
                id,
            ),
            RowKey::ImportMapping(_) | RowKey::Setting(_) => {
                panic!("delete: import mappings and settings have no soft delete")
            }
        }
    }

    /// Read access to the Db built so far.
    pub fn db(&self) -> &Db {
        &self.db
    }

    /// Consumes the builder and returns the Db.
    pub fn build(self) -> Db {
        self.db
    }
}

/// Builds one transaction; [`TxBuilder::done`] inserts it.
pub struct TxBuilder<'a> {
    db: &'a mut TestDb,
    tx: Transaction,
}

impl TxBuilder<'_> {
    /// Sets the status.
    pub fn status(mut self, s: TransactionStatus) -> Self {
        self.tx.status = s;
        self
    }

    /// Puts the transaction in a scenario.
    pub fn scenario(mut self, s: ScenarioId) -> Self {
        self.tx.scenario_id = Some(s);
        self
    }

    /// Sets the category.
    pub fn category(mut self, c: CategoryId) -> Self {
        self.tx.category_id = Some(c);
        self
    }

    /// Sets the contexts, keeping their order.
    pub fn contexts(mut self, c: &[ContextId]) -> Self {
        self.tx.context_ids = c.to_vec();
        self
    }

    /// Sets the memo.
    pub fn memo(mut self, memo: &str) -> Self {
        self.tx.memo = Some(memo.to_string());
        self
    }

    /// Sets the attachment path.
    pub fn attachment(mut self, path: &str) -> Self {
        self.tx.attachment_path = Some(path.to_string());
        self
    }

    /// Marks the transaction soft-deleted.
    pub fn deleted(mut self) -> Self {
        self.tx.is_deleted = true;
        self
    }

    /// Inserts the transaction and returns its id.
    pub fn done(self) -> TransactionId {
        let TxBuilder { db, tx } = self;
        let id = tx.id;
        db.db.transactions.insert(id, tx);
        id
    }
}

/// Builds one obligation; [`ObligationBuilder::done`] inserts it.
pub struct ObligationBuilder<'a> {
    db: &'a mut TestDb,
    ob: Obligation,
}

impl ObligationBuilder<'_> {
    /// Links the account.
    pub fn account(mut self, a: AccountId) -> Self {
        self.ob.account_id = Some(a);
        self
    }

    /// Links the context.
    pub fn context(mut self, c: ContextId) -> Self {
        self.ob.context_id = Some(c);
        self
    }

    /// Sets the status.
    pub fn status(mut self, s: ObligationStatus) -> Self {
        self.ob.status = s;
        self
    }

    /// Sets the current balance from a decimal string.
    pub fn balance(mut self, amount: &str) -> Self {
        self.ob.current_balance = m(amount);
        self
    }

    /// Sets the target amount from a decimal string.
    pub fn target(mut self, amount: &str) -> Self {
        self.ob.target_amount = Some(m(amount));
        self
    }

    /// Sets the due date from a `YYYY-MM-DD` string.
    pub fn due(mut self, date: &str) -> Self {
        self.ob.due_date = Some(d(date));
        self
    }

    /// Inserts the obligation and returns its id.
    pub fn done(self) -> ObligationId {
        let ObligationBuilder { db, ob } = self;
        let id = ob.id;
        db.db.obligations.insert(id, ob);
        id
    }
}

/// Builds one budget item; [`BudgetItemBuilder::done`] inserts it.
pub struct BudgetItemBuilder<'a> {
    db: &'a mut TestDb,
    item: BudgetItem,
}

impl BudgetItemBuilder<'_> {
    /// Sets the end date from a `YYYY-MM-DD` string.
    pub fn end(mut self, date: &str) -> Self {
        self.item.end_date = Some(d(date));
        self
    }

    /// Sets the specific months (1..=12).
    pub fn months(mut self, months: &[u32]) -> Self {
        self.item.specific_months = months.to_vec();
        self
    }

    /// Sets the category.
    pub fn category(mut self, c: CategoryId) -> Self {
        self.item.category_id = Some(c);
        self
    }

    /// Sets the originating account.
    pub fn account(mut self, a: AccountId) -> Self {
        self.item.account_id = Some(a);
        self
    }

    /// Sets the notes.
    pub fn notes(mut self, n: &str) -> Self {
        self.item.notes = Some(n.to_string());
        self
    }

    /// Inserts the budget item and returns its id.
    pub fn done(self) -> BudgetItemId {
        let BudgetItemBuilder { db, item } = self;
        let id = item.id;
        db.db.budget_items.insert(id, item);
        id
    }
}

#[cfg(test)]
mod tests {
    use crate::*;

    use crate::testing::*;

    #[test]
    fn testdb_example_compiles_and_counts() {
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let s = t.scenario("Plan A");
        t.tx(chk, "2026-01-05", "-12.50", "Coffee")
            .status(TransactionStatus::Cleared)
            .done();
        t.tx(chk, "2026-02-01", "-100", "Rent")
            .scenario(s)
            .status(TransactionStatus::Future)
            .done();
        let db = t.build();
        assert_eq!(db.transactions.len(), 2);
    }

    #[test]
    fn testdb_builders_set_every_field() {
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let ext = t.external_account("Brokerage", AccountType::Investment);
        let s = t.scenario("Plan A");
        let cat = t.category("Food", CategoryType::Expense);
        let sub = t.child_category(cat, "Coffee");
        t.patterns(sub, &["starbucks", "blue bottle"]);
        let ctx_a = t.context("Trip");
        let ctx_b = t.context("Work");
        let tid = t
            .tx(chk, "2026-03-04", "-4.25", "Blue Bottle")
            .status(TransactionStatus::Reconciled)
            .scenario(s)
            .category(sub)
            .contexts(&[ctx_b, ctx_a])
            .memo("latte")
            .attachment("/receipts/1.pdf")
            .deleted()
            .done();
        let ob = t
            .obligation("Car")
            .account(chk)
            .context(ctx_a)
            .status(ObligationStatus::Spending)
            .balance("150.00")
            .target("900")
            .due("2026-12-31")
            .done();
        let bi = t
            .budget_item(
                BudgetItemType::Bill,
                "Gym",
                "40",
                "2026-01-01",
                BudgetFrequency::SpecificMonths,
            )
            .end("2026-12-31")
            .months(&[1, 6, 12])
            .category(cat)
            .account(ext)
            .notes("monthly")
            .done();
        let mapping = t.import_mapping("account", "CHK-1234", "Checking", Some(chk));
        t.setting("apiPort", "8989");
        let db = t.build();

        let tx = db.transaction(tid).expect("transaction inserted");
        assert_eq!(tx.date, d("2026-03-04"));
        assert_eq!(tx.amount, m("-4.25"));
        assert_eq!(tx.payee, "Blue Bottle");
        assert_eq!(tx.status, TransactionStatus::Reconciled);
        assert_eq!(tx.scenario_id, Some(s));
        assert_eq!(tx.category_id, Some(sub));
        assert_eq!(tx.context_ids, vec![ctx_b, ctx_a]);
        assert_eq!(tx.memo.as_deref(), Some("latte"));
        assert_eq!(tx.attachment_path.as_deref(), Some("/receipts/1.pdf"));
        assert!(tx.is_deleted);
        assert_eq!(tx.account_id, Some(chk));

        assert_eq!(
            db.category(sub).map(|c| c.match_patterns.clone()),
            Some(vec!["starbucks".to_string(), "blue bottle".to_string()])
        );
        assert_eq!(db.category(sub).and_then(|c| c.parent_id), Some(cat));
        assert!(db.account(ext).is_some_and(|a| a.is_external));

        let ob = db.obligation(ob).expect("obligation inserted");
        assert_eq!(ob.account_id, Some(chk));
        assert_eq!(ob.context_id, Some(ctx_a));
        assert_eq!(ob.status, ObligationStatus::Spending);
        assert_eq!(ob.current_balance, m("150.00"));
        assert_eq!(ob.target_amount, Some(m("900")));
        assert_eq!(ob.due_date, Some(d("2026-12-31")));

        let bi = db.budget_item(bi).expect("budget item inserted");
        assert_eq!(bi.item_type, BudgetItemType::Bill);
        assert_eq!(bi.frequency, BudgetFrequency::SpecificMonths);
        assert_eq!(bi.end_date, Some(d("2026-12-31")));
        assert_eq!(bi.specific_months, vec![1, 6, 12]);
        assert_eq!(bi.category_id, Some(cat));
        assert_eq!(bi.account_id, Some(ext));
        assert_eq!(bi.notes.as_deref(), Some("monthly"));

        let mp = db.import_mappings.get(&mapping).expect("mapping inserted");
        assert_eq!(mp.source_value, "CHK-1234");
        assert_eq!(mp.account_id, Some(chk));
        assert_eq!(db.setting("apiPort"), Some("8989"));
    }
}
