//! The in-memory database: one `BTreeMap` per table, plus id allocation and query helpers.
//! Query helpers skip soft-deleted rows unless the method name says otherwise.
//! Business logic (balances, filters, projections) does not belong here.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::entities::{
    Account, BudgetItem, Category, Context, ImportMapping, Obligation, Scenario, Transaction,
};
use crate::ids::Id;
use crate::ids::{
    AccountId, BudgetItemId, CategoryId, ContextId, ImportMappingId, ObligationId, ScenarioId,
    TransactionId,
};
use crate::record::{Record, RowKey};

/// The numeric id of a row, or `None` for settings (keyed by string).
fn numeric_id(key: &RowKey) -> Option<Id> {
    match key {
        RowKey::Account(id) => Some(id.raw()),
        RowKey::Transaction(id) => Some(id.raw()),
        RowKey::Category(id) => Some(id.raw()),
        RowKey::Context(id) => Some(id.raw()),
        RowKey::Scenario(id) => Some(id.raw()),
        RowKey::Obligation(id) => Some(id.raw()),
        RowKey::BudgetItem(id) => Some(id.raw()),
        RowKey::ImportMapping(id) => Some(id.raw()),
        RowKey::Setting(_) => None,
    }
}

/// The whole data set, in memory. Every table is keyed by its id newtype.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Db {
    /// Accounts by id.
    pub accounts: BTreeMap<AccountId, Account>,
    /// Transactions by id.
    pub transactions: BTreeMap<TransactionId, Transaction>,
    /// Categories by id.
    pub categories: BTreeMap<CategoryId, Category>,
    /// Contexts by id.
    pub contexts: BTreeMap<ContextId, Context>,
    /// Scenarios by id.
    pub scenarios: BTreeMap<ScenarioId, Scenario>,
    /// Obligations by id.
    pub obligations: BTreeMap<ObligationId, Obligation>,
    /// Budget items by id.
    pub budget_items: BTreeMap<BudgetItemId, BudgetItem>,
    /// Import mappings by id.
    pub import_mappings: BTreeMap<ImportMappingId, ImportMapping>,
    /// Settings by key.
    pub settings: BTreeMap<String, String>,
    /// Next id to hand out; 0 means "not yet allocated" (first id is 1).
    next_id: Id,
}

impl Db {
    /// An empty database.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the next id and advances the shared sequence.
    pub fn alloc_id(&mut self) -> Id {
        let id = self.next_id();
        self.next_id = id + 1;
        id
    }

    /// The value the next [`Db::alloc_id`] call would return.
    pub fn next_id(&self) -> Id {
        self.next_id.max(1)
    }

    /// Sets the sequence position (used by the store after loading).
    pub fn set_next_id(&mut self, id: Id) {
        self.next_id = id;
    }

    /// The largest id present in any table, or 0 when empty.
    pub fn max_id(&self) -> Id {
        let ids = self
            .accounts
            .keys()
            .map(|k| k.raw())
            .chain(self.transactions.keys().map(|k| k.raw()))
            .chain(self.categories.keys().map(|k| k.raw()))
            .chain(self.contexts.keys().map(|k| k.raw()))
            .chain(self.scenarios.keys().map(|k| k.raw()))
            .chain(self.obligations.keys().map(|k| k.raw()))
            .chain(self.budget_items.keys().map(|k| k.raw()))
            .chain(self.import_mappings.keys().map(|k| k.raw()));
        ids.max().unwrap_or(0)
    }

    /// Clones the row identified by `key`, if it exists.
    pub fn get(&self, key: &RowKey) -> Option<Record> {
        match key {
            RowKey::Account(id) => self.accounts.get(id).cloned().map(Record::Account),
            RowKey::Transaction(id) => self.transactions.get(id).cloned().map(Record::Transaction),
            RowKey::Category(id) => self.categories.get(id).cloned().map(Record::Category),
            RowKey::Context(id) => self.contexts.get(id).cloned().map(Record::Context),
            RowKey::Scenario(id) => self.scenarios.get(id).cloned().map(Record::Scenario),
            RowKey::Obligation(id) => self.obligations.get(id).cloned().map(Record::Obligation),
            RowKey::BudgetItem(id) => self.budget_items.get(id).cloned().map(Record::BudgetItem),
            RowKey::ImportMapping(id) => self
                .import_mappings
                .get(id)
                .cloned()
                .map(Record::ImportMapping),
            RowKey::Setting(k) => self.settings.get(k).map(|v| Record::Setting {
                key: k.clone(),
                value: v.clone(),
            }),
        }
    }

    /// Inserts or replaces a row. Keeps the id sequence ahead of any id put in,
    /// so a later [`Db::alloc_id`] cannot collide with it.
    pub fn put(&mut self, record: Record) {
        if let Some(raw) = numeric_id(&record.key()) {
            self.next_id = self.next_id.max(raw + 1);
        }
        match record {
            Record::Account(r) => {
                self.accounts.insert(r.id, r);
            }
            Record::Transaction(r) => {
                self.transactions.insert(r.id, r);
            }
            Record::Category(r) => {
                self.categories.insert(r.id, r);
            }
            Record::Context(r) => {
                self.contexts.insert(r.id, r);
            }
            Record::Scenario(r) => {
                self.scenarios.insert(r.id, r);
            }
            Record::Obligation(r) => {
                self.obligations.insert(r.id, r);
            }
            Record::BudgetItem(r) => {
                self.budget_items.insert(r.id, r);
            }
            Record::ImportMapping(r) => {
                self.import_mappings.insert(r.id, r);
            }
            Record::Setting { key, value } => {
                self.settings.insert(key, value);
            }
        }
    }

    /// Hard-removes the row identified by `key`. No-op if it is absent.
    pub fn remove(&mut self, key: &RowKey) {
        match key {
            RowKey::Account(id) => {
                self.accounts.remove(id);
            }
            RowKey::Transaction(id) => {
                self.transactions.remove(id);
            }
            RowKey::Category(id) => {
                self.categories.remove(id);
            }
            RowKey::Context(id) => {
                self.contexts.remove(id);
            }
            RowKey::Scenario(id) => {
                self.scenarios.remove(id);
            }
            RowKey::Obligation(id) => {
                self.obligations.remove(id);
            }
            RowKey::BudgetItem(id) => {
                self.budget_items.remove(id);
            }
            RowKey::ImportMapping(id) => {
                self.import_mappings.remove(id);
            }
            RowKey::Setting(k) => {
                self.settings.remove(k);
            }
        }
    }

    /// Every row in every table, including soft-deleted rows and settings.
    pub fn all_records(&self) -> Vec<Record> {
        let mut out = Vec::new();
        out.extend(self.accounts.values().cloned().map(Record::Account));
        out.extend(self.transactions.values().cloned().map(Record::Transaction));
        out.extend(self.categories.values().cloned().map(Record::Category));
        out.extend(self.contexts.values().cloned().map(Record::Context));
        out.extend(self.scenarios.values().cloned().map(Record::Scenario));
        out.extend(self.obligations.values().cloned().map(Record::Obligation));
        out.extend(self.budget_items.values().cloned().map(Record::BudgetItem));
        out.extend(
            self.import_mappings
                .values()
                .cloned()
                .map(Record::ImportMapping),
        );
        out.extend(self.settings.iter().map(|(k, v)| Record::Setting {
            key: k.clone(),
            value: v.clone(),
        }));
        out
    }

    /// Account by id, including soft-deleted.
    pub fn account(&self, id: AccountId) -> Option<&Account> {
        self.accounts.get(&id)
    }

    /// Transaction by id, including soft-deleted.
    pub fn transaction(&self, id: TransactionId) -> Option<&Transaction> {
        self.transactions.get(&id)
    }

    /// Category by id, including soft-deleted.
    pub fn category(&self, id: CategoryId) -> Option<&Category> {
        self.categories.get(&id)
    }

    /// Context by id, including soft-deleted.
    pub fn context(&self, id: ContextId) -> Option<&Context> {
        self.contexts.get(&id)
    }

    /// Scenario by id, including soft-deleted.
    pub fn scenario(&self, id: ScenarioId) -> Option<&Scenario> {
        self.scenarios.get(&id)
    }

    /// Obligation by id, including soft-deleted.
    pub fn obligation(&self, id: ObligationId) -> Option<&Obligation> {
        self.obligations.get(&id)
    }

    /// Budget item by id, including soft-deleted.
    pub fn budget_item(&self, id: BudgetItemId) -> Option<&BudgetItem> {
        self.budget_items.get(&id)
    }

    /// Live accounts in id order.
    pub fn live_accounts(&self) -> impl Iterator<Item = &Account> {
        self.accounts.values().filter(|r| !r.is_deleted)
    }

    /// Live transactions in id order.
    pub fn live_transactions(&self) -> impl Iterator<Item = &Transaction> {
        self.transactions.values().filter(|r| !r.is_deleted)
    }

    /// Live categories in id order.
    pub fn live_categories(&self) -> impl Iterator<Item = &Category> {
        self.categories.values().filter(|r| !r.is_deleted)
    }

    /// Live contexts in id order.
    pub fn live_contexts(&self) -> impl Iterator<Item = &Context> {
        self.contexts.values().filter(|r| !r.is_deleted)
    }

    /// Live scenarios in id order.
    pub fn live_scenarios(&self) -> impl Iterator<Item = &Scenario> {
        self.scenarios.values().filter(|r| !r.is_deleted)
    }

    /// Live obligations in id order.
    pub fn live_obligations(&self) -> impl Iterator<Item = &Obligation> {
        self.obligations.values().filter(|r| !r.is_deleted)
    }

    /// Live budget items in id order.
    pub fn live_budget_items(&self) -> impl Iterator<Item = &BudgetItem> {
        self.budget_items.values().filter(|r| !r.is_deleted)
    }

    /// Live accounts sorted by name. Byte-wise compare (not locale-aware), ties broken by id.
    /// SwiftData's sort may be locale-aware for non-ASCII names; this is a deliberate difference.
    pub fn accounts_by_name(&self) -> Vec<&Account> {
        let mut v: Vec<&Account> = self.live_accounts().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        v
    }

    /// Live categories sorted by name (byte-wise, ties by id).
    pub fn categories_by_name(&self) -> Vec<&Category> {
        let mut v: Vec<&Category> = self.live_categories().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        v
    }

    /// Live contexts sorted by name (byte-wise, ties by id).
    pub fn contexts_by_name(&self) -> Vec<&Context> {
        let mut v: Vec<&Context> = self.live_contexts().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        v
    }

    /// Live scenarios sorted by name (byte-wise, ties by id).
    pub fn scenarios_by_name(&self) -> Vec<&Scenario> {
        let mut v: Vec<&Scenario> = self.live_scenarios().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        v
    }

    /// Live obligations sorted by name (byte-wise, ties by id).
    pub fn obligations_by_name(&self) -> Vec<&Obligation> {
        let mut v: Vec<&Obligation> = self.live_obligations().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        v
    }

    /// Live budget items sorted by name (byte-wise, ties by id).
    pub fn budget_items_by_name(&self) -> Vec<&BudgetItem> {
        let mut v: Vec<&BudgetItem> = self.live_budget_items().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        v
    }

    /// First live account whose name equals `name` ignoring case.
    pub fn account_by_name_ci(&self, name: &str) -> Option<&Account> {
        let want = name.to_lowercase();
        self.live_accounts().find(|a| a.name.to_lowercase() == want)
    }

    /// First live scenario whose name equals `name` ignoring case.
    pub fn scenario_by_name_ci(&self, name: &str) -> Option<&Scenario> {
        let want = name.to_lowercase();
        self.live_scenarios()
            .find(|s| s.name.to_lowercase() == want)
    }

    /// First live context whose name equals `name` ignoring case.
    pub fn context_by_name_ci(&self, name: &str) -> Option<&Context> {
        let want = name.to_lowercase();
        self.live_contexts().find(|c| c.name.to_lowercase() == want)
    }

    /// First live account with exactly this name.
    pub fn account_by_name(&self, name: &str) -> Option<&Account> {
        self.live_accounts().find(|a| a.name == name)
    }

    /// First live category with exactly this name (lowest id).
    pub fn category_by_name(&self, name: &str) -> Option<&Category> {
        self.live_categories().find(|c| c.name == name)
    }

    /// First live context with exactly this name.
    pub fn context_by_name(&self, name: &str) -> Option<&Context> {
        self.live_contexts().find(|c| c.name == name)
    }

    /// First live scenario with exactly this name.
    pub fn scenario_by_name(&self, name: &str) -> Option<&Scenario> {
        self.live_scenarios().find(|s| s.name == name)
    }

    /// R-CAT-4: `"Parent/Child"` if the category has a parent (the parent is looked up even if
    /// deleted), else the plain name. An unknown id gives `""`.
    pub fn category_full_name(&self, id: CategoryId) -> String {
        let Some(cat) = self.categories.get(&id) else {
            return String::new();
        };
        match cat.parent_id.and_then(|p| self.categories.get(&p)) {
            Some(parent) => format!("{}/{}", parent.name, cat.name),
            None => cat.name.clone(),
        }
    }

    /// Live direct children of `parent`, sorted by name (byte-wise, ties by id).
    pub fn subcategories(&self, parent: CategoryId) -> Vec<&Category> {
        let mut v: Vec<&Category> = self
            .live_categories()
            .filter(|c| c.parent_id == Some(parent))
            .collect();
        v.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        v
    }

    /// Live transactions whose account is `account`.
    pub fn transactions_for_account(
        &self,
        account: AccountId,
    ) -> impl Iterator<Item = &Transaction> {
        self.live_transactions()
            .filter(move |t| t.account_id == Some(account))
    }

    /// Live transactions belonging to `scenario` (not Reality).
    pub fn transactions_for_scenario(
        &self,
        scenario: ScenarioId,
    ) -> impl Iterator<Item = &Transaction> {
        self.live_transactions()
            .filter(move |t| t.scenario_id == Some(scenario))
    }

    /// Live transactions tagged with `ctx`.
    pub fn transactions_with_context(&self, ctx: ContextId) -> impl Iterator<Item = &Transaction> {
        self.live_transactions()
            .filter(move |t| t.context_ids.contains(&ctx))
    }

    /// Live obligations linked to `account`.
    pub fn obligations_for_account(&self, account: AccountId) -> impl Iterator<Item = &Obligation> {
        self.live_obligations()
            .filter(move |o| o.account_id == Some(account))
    }

    /// Raw value of a setting, if set.
    pub fn setting(&self, key: &str) -> Option<&str> {
        self.settings.get(key).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enums::{AccountType, CategoryType, TransactionStatus};
    use crate::testing::{TestDb, d, m};

    #[test]
    fn alloc_id_is_monotonic_and_shared() {
        let mut db = Db::new();
        assert_eq!(db.next_id(), 1);
        assert_eq!(db.alloc_id(), 1);
        assert_eq!(db.alloc_id(), 2);
        let id3 = db.alloc_id();
        assert_eq!(id3, 3);
        // Ids from one sequence: an account gets id 4 after the transactions' ids above.
        db.put(Record::Account(Account::new(
            AccountId(id3),
            "A",
            AccountType::Cash,
            "",
        )));
        assert_eq!(db.alloc_id(), 4);
        // Putting a row with a larger id moves the sequence past it, so ids never collide.
        db.put(Record::Account(Account::new(
            AccountId(10),
            "B",
            AccountType::Cash,
            "",
        )));
        assert_eq!(db.alloc_id(), 11);
        db.set_next_id(50);
        assert_eq!(db.alloc_id(), 50);
        assert_eq!(db.max_id(), 10);
    }

    #[test]
    fn get_put_remove_every_table() {
        let records = vec![
            Record::Account(Account::new(AccountId(1), "A", AccountType::Cash, "")),
            Record::Transaction(Transaction::new(
                TransactionId(2),
                d("2026-01-05"),
                m("1"),
                "x",
            )),
            Record::Category(Category::new(CategoryId(3), "C", CategoryType::Income)),
            Record::Context(Context::new(ContextId(4), "Ctx")),
            Record::Scenario(Scenario::new(ScenarioId(5), "S")),
            Record::Obligation(Obligation::new(ObligationId(6), "O")),
            Record::BudgetItem(BudgetItem::new(
                BudgetItemId(7),
                crate::enums::BudgetItemType::Bill,
                "B",
                m("9"),
                d("2026-01-01"),
                crate::enums::BudgetFrequency::Monthly,
            )),
            Record::ImportMapping(ImportMapping::new(
                ImportMappingId(8),
                "account",
                "src",
                "tgt",
                None,
            )),
            Record::Setting {
                key: "k".into(),
                value: "v".into(),
            },
        ];
        let mut db = Db::new();
        for r in &records {
            let key = r.key();
            db.put(r.clone());
            assert_eq!(db.get(&key).as_ref(), Some(r));
            db.remove(&key);
            assert_eq!(db.get(&key), None);
            db.remove(&key); // removing an absent row is a no-op
        }
        assert_eq!(db.all_records(), Vec::new());
    }

    #[test]
    fn r_cat_4_full_name_with_parent() {
        // Covers: R-CAT-4
        let mut t = TestDb::new();
        let housing = t.category("Housing", CategoryType::Expense);
        let utilities = t.child_category(housing, "Utilities");
        let db = t.build();
        assert_eq!(db.category_full_name(utilities), "Housing/Utilities");
    }

    #[test]
    fn r_cat_4_full_name_without_parent() {
        // Covers: R-CAT-4
        let mut t = TestDb::new();
        let housing = t.category("Housing", CategoryType::Expense);
        let db = t.build();
        assert_eq!(db.category_full_name(housing), "Housing");
        assert_eq!(db.category_full_name(CategoryId(999)), "");
    }

    #[test]
    fn live_iterators_skip_deleted() {
        let mut t = TestDb::new();
        let a = t.account("Keep", AccountType::Checking);
        let gone = t.account("Gone", AccountType::Savings);
        let tx_keep = t.tx(a, "2026-01-01", "1", "keep").done();
        let _tx_gone = t.tx(a, "2026-01-02", "2", "gone").deleted().done();
        let cat = t.category("Old", CategoryType::Expense);
        t.delete(RowKey::Account(gone));
        t.delete(RowKey::Category(cat));
        let db = t.build();
        let accounts: Vec<AccountId> = db.live_accounts().map(|x| x.id).collect();
        assert_eq!(accounts, vec![a]);
        let txs: Vec<TransactionId> = db.live_transactions().map(|x| x.id).collect();
        assert_eq!(txs, vec![tx_keep]);
        assert_eq!(db.live_categories().count(), 0);
        assert!(
            db.account(gone).is_some(),
            "by-id lookup still sees deleted rows"
        );
    }

    #[test]
    fn by_name_ci_matches_case_insensitive_live_only() {
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let old = t.account("Old Card", AccountType::Credit);
        t.delete(RowKey::Account(old));
        let db = t.build();
        assert_eq!(db.account_by_name_ci("checking").map(|a| a.id), Some(chk));
        assert!(db.account_by_name_ci("old card").is_none());
        assert!(
            db.account_by_name("checking").is_none(),
            "exact match is case-sensitive"
        );
    }

    #[test]
    fn sorted_helpers_sort_by_name_then_id() {
        let mut t = TestDb::new();
        let b = t.account("B", AccountType::Checking);
        let a2 = t.account("A", AccountType::Checking);
        let a3 = t.account("A", AccountType::Savings);
        let db = t.build();
        let order: Vec<AccountId> = db.accounts_by_name().iter().map(|a| a.id).collect();
        assert_eq!(order, vec![a2, a3, b]);
    }

    #[test]
    fn transactions_for_account_live_only() {
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let sav = t.account("Savings", AccountType::Savings);
        let keep = t
            .tx(chk, "2026-01-01", "-5", "keep")
            .status(TransactionStatus::Pending)
            .done();
        let _gone = t.tx(chk, "2026-01-02", "-6", "gone").deleted().done();
        let _other = t.tx(sav, "2026-01-03", "-7", "other").done();
        let db = t.build();
        let ids: Vec<TransactionId> = db.transactions_for_account(chk).map(|x| x.id).collect();
        assert_eq!(ids, vec![keep]);
    }
}
