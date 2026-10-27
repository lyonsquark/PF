//! The write handle used inside `Engine::apply`. It remembers the first before-image of every
//! row it touches, so the engine can persist a diff and undo it (R-UND-2).

use std::collections::{HashMap, HashSet};

use jiff::civil::Date;
use pf_model::{
    Account, AccountId, BudgetItem, BudgetItemId, Category, CategoryId, Change, Context, ContextId,
    Db, Id, ImportMapping, ImportMappingId, Obligation, ObligationId, Record, RowKey, Scenario,
    ScenarioId, Transaction, TransactionId,
};

use crate::error::{EngineError, Result};

/// Write handle over the `Db`. Every mutation first records the row's before-image.
pub struct Writer<'a> {
    db: &'a mut Db,
    /// Rows in first-touch order, each with its before-image (`None` if it did not exist).
    touched: Vec<(RowKey, Option<Record>)>,
    /// Position of each touched row in `touched` (keeps bulk commands linear).
    index: HashMap<RowKey, usize>,
    today: Date,
    /// The id counter when the writer was created, restored on rollback.
    start_next_id: Id,
}

impl<'a> Writer<'a> {
    /// A writer over `db`; `today` is the injected local date (no clock reads here).
    pub(crate) fn new(db: &'a mut Db, today: Date) -> Self {
        let start_next_id = db.next_id();
        Self {
            db,
            touched: Vec::new(),
            index: HashMap::new(),
            today,
            start_next_id,
        }
    }

    /// Read access to the data inside a command.
    pub fn db(&self) -> &Db {
        self.db
    }

    /// The injected local date.
    pub fn today(&self) -> Date {
        self.today
    }

    /// Records the before-image of `key` the first time it is touched.
    fn touch(&mut self, key: RowKey) {
        if self.index.contains_key(&key) {
            return;
        }
        let before = self.db.get(&key);
        self.index.insert(key.clone(), self.touched.len());
        self.touched.push((key, before));
    }

    /// Inserts an account. The `id` of `a` is ignored; a fresh id is allocated.
    pub fn insert_account(&mut self, mut a: Account) -> AccountId {
        let id = AccountId(self.db.alloc_id());
        a.id = id;
        self.touch(RowKey::Account(id));
        self.db.put(Record::Account(a));
        id
    }

    /// Inserts a transaction. The `id` is ignored; context links are de-duplicated (decision 2).
    pub fn insert_transaction(&mut self, mut t: Transaction) -> TransactionId {
        let id = TransactionId(self.db.alloc_id());
        t.id = id;
        dedupe_context_ids(&mut t.context_ids);
        self.touch(RowKey::Transaction(id));
        self.db.put(Record::Transaction(t));
        id
    }

    /// Inserts a category. The `id` is ignored.
    pub fn insert_category(&mut self, mut c: Category) -> CategoryId {
        let id = CategoryId(self.db.alloc_id());
        c.id = id;
        self.touch(RowKey::Category(id));
        self.db.put(Record::Category(c));
        id
    }

    /// Inserts a context. The `id` is ignored.
    pub fn insert_context(&mut self, mut c: Context) -> ContextId {
        let id = ContextId(self.db.alloc_id());
        c.id = id;
        self.touch(RowKey::Context(id));
        self.db.put(Record::Context(c));
        id
    }

    /// Inserts a scenario. The `id` is ignored.
    pub fn insert_scenario(&mut self, mut s: Scenario) -> ScenarioId {
        let id = ScenarioId(self.db.alloc_id());
        s.id = id;
        self.touch(RowKey::Scenario(id));
        self.db.put(Record::Scenario(s));
        id
    }

    /// Inserts an obligation. The `id` is ignored.
    pub fn insert_obligation(&mut self, mut o: Obligation) -> ObligationId {
        let id = ObligationId(self.db.alloc_id());
        o.id = id;
        self.touch(RowKey::Obligation(id));
        self.db.put(Record::Obligation(o));
        id
    }

    /// Inserts a budget item. The `id` is ignored.
    pub fn insert_budget_item(&mut self, mut b: BudgetItem) -> BudgetItemId {
        let id = BudgetItemId(self.db.alloc_id());
        b.id = id;
        self.touch(RowKey::BudgetItem(id));
        self.db.put(Record::BudgetItem(b));
        id
    }

    /// Inserts an import mapping. The `id` is ignored.
    pub fn insert_import_mapping(&mut self, mut m: ImportMapping) -> ImportMappingId {
        let id = ImportMappingId(self.db.alloc_id());
        m.id = id;
        self.touch(RowKey::ImportMapping(id));
        self.db.put(Record::ImportMapping(m));
        id
    }

    /// Updates an account. Deleted rows can be updated. `NotFound` if the id is unknown.
    pub fn update_account(&mut self, id: AccountId, f: impl FnOnce(&mut Account)) -> Result<()> {
        let mut row = self
            .db
            .account(id)
            .cloned()
            .ok_or_else(|| not_found("account", id.raw()))?;
        self.touch(RowKey::Account(id));
        f(&mut row);
        row.id = id;
        self.db.put(Record::Account(row));
        Ok(())
    }

    /// Updates a transaction; context links are de-duplicated afterwards (decision 2).
    pub fn update_transaction(
        &mut self,
        id: TransactionId,
        f: impl FnOnce(&mut Transaction),
    ) -> Result<()> {
        let mut row = self
            .db
            .transaction(id)
            .cloned()
            .ok_or_else(|| not_found("transaction", id.raw()))?;
        self.touch(RowKey::Transaction(id));
        f(&mut row);
        row.id = id;
        dedupe_context_ids(&mut row.context_ids);
        self.db.put(Record::Transaction(row));
        Ok(())
    }

    /// Updates a category.
    pub fn update_category(&mut self, id: CategoryId, f: impl FnOnce(&mut Category)) -> Result<()> {
        let mut row = self
            .db
            .category(id)
            .cloned()
            .ok_or_else(|| not_found("category", id.raw()))?;
        self.touch(RowKey::Category(id));
        f(&mut row);
        row.id = id;
        self.db.put(Record::Category(row));
        Ok(())
    }

    /// Updates a context.
    pub fn update_context(&mut self, id: ContextId, f: impl FnOnce(&mut Context)) -> Result<()> {
        let mut row = self
            .db
            .context(id)
            .cloned()
            .ok_or_else(|| not_found("context", id.raw()))?;
        self.touch(RowKey::Context(id));
        f(&mut row);
        row.id = id;
        self.db.put(Record::Context(row));
        Ok(())
    }

    /// Updates a scenario.
    pub fn update_scenario(&mut self, id: ScenarioId, f: impl FnOnce(&mut Scenario)) -> Result<()> {
        let mut row = self
            .db
            .scenario(id)
            .cloned()
            .ok_or_else(|| not_found("scenario", id.raw()))?;
        self.touch(RowKey::Scenario(id));
        f(&mut row);
        row.id = id;
        self.db.put(Record::Scenario(row));
        Ok(())
    }

    /// Updates an obligation.
    pub fn update_obligation(
        &mut self,
        id: ObligationId,
        f: impl FnOnce(&mut Obligation),
    ) -> Result<()> {
        let mut row = self
            .db
            .obligation(id)
            .cloned()
            .ok_or_else(|| not_found("obligation", id.raw()))?;
        self.touch(RowKey::Obligation(id));
        f(&mut row);
        row.id = id;
        self.db.put(Record::Obligation(row));
        Ok(())
    }

    /// Updates a budget item.
    pub fn update_budget_item(
        &mut self,
        id: BudgetItemId,
        f: impl FnOnce(&mut BudgetItem),
    ) -> Result<()> {
        let mut row = self
            .db
            .budget_item(id)
            .cloned()
            .ok_or_else(|| not_found("budget item", id.raw()))?;
        self.touch(RowKey::BudgetItem(id));
        f(&mut row);
        row.id = id;
        self.db.put(Record::BudgetItem(row));
        Ok(())
    }

    /// Updates an import mapping.
    pub fn update_import_mapping(
        &mut self,
        id: ImportMappingId,
        f: impl FnOnce(&mut ImportMapping),
    ) -> Result<()> {
        let mut row = self
            .db
            .import_mappings
            .get(&id)
            .cloned()
            .ok_or_else(|| not_found("import mapping", id.raw()))?;
        self.touch(RowKey::ImportMapping(id));
        f(&mut row);
        row.id = id;
        self.db.put(Record::ImportMapping(row));
        Ok(())
    }

    /// Soft delete or restore (R-PRG-1 keeps the row until restart). `Validation` for import
    /// mappings and settings, which have no flag.
    pub fn set_deleted(&mut self, key: RowKey, deleted: bool) -> Result<()> {
        match key {
            RowKey::Account(id) => self.update_account(id, |r| r.is_deleted = deleted),
            RowKey::Transaction(id) => self.update_transaction(id, |r| r.is_deleted = deleted),
            RowKey::Category(id) => self.update_category(id, |r| r.is_deleted = deleted),
            RowKey::Context(id) => self.update_context(id, |r| r.is_deleted = deleted),
            RowKey::Scenario(id) => self.update_scenario(id, |r| r.is_deleted = deleted),
            RowKey::Obligation(id) => self.update_obligation(id, |r| r.is_deleted = deleted),
            RowKey::BudgetItem(id) => self.update_budget_item(id, |r| r.is_deleted = deleted),
            RowKey::ImportMapping(_) | RowKey::Setting(_) => Err(EngineError::Validation(
                "This item has no deleted flag; remove it instead".into(),
            )),
        }
    }

    /// Removes the row entirely. Undo re-inserts it with the same id. `NotFound` if absent.
    pub fn delete_hard(&mut self, key: RowKey) -> Result<()> {
        if self.db.get(&key).is_none() {
            return Err(EngineError::NotFound(format!("{key:?}")));
        }
        self.touch(key.clone());
        self.db.remove(&key);
        Ok(())
    }

    /// Writes a setting as part of this undo step.
    pub fn set_setting(&mut self, key: &str, value: &str) {
        self.touch(RowKey::Setting(key.to_string()));
        self.db.put(Record::Setting {
            key: key.to_string(),
            value: value.to_string(),
        });
    }

    /// Removes a setting as part of this undo step.
    pub fn remove_setting(&mut self, key: &str) {
        let row = RowKey::Setting(key.to_string());
        self.touch(row.clone());
        self.db.remove(&row);
    }

    /// Consumes the writer and returns the changes where before != after, in touch order.
    pub(crate) fn finish(self) -> Vec<Change> {
        let db = self.db;
        self.touched
            .into_iter()
            .filter_map(|(key, before)| {
                let after = db.get(&key);
                (before != after).then_some(Change { key, before, after })
            })
            .collect()
    }

    /// Restores every touched row to its before-image, last touch first, and rewinds the id
    /// counter. Used when the closure returns an error.
    pub(crate) fn rollback(self) {
        let Writer {
            db,
            touched,
            start_next_id,
            ..
        } = self;
        for (key, before) in touched.iter().rev() {
            set_image(db, key, before);
        }
        db.set_next_id(start_next_id);
    }
}

/// Puts a row image into the `Db`: `Some` upserts, `None` removes.
pub(crate) fn set_image(db: &mut Db, key: &RowKey, image: &Option<Record>) {
    match image {
        Some(record) => db.put(record.clone()),
        None => db.remove(key),
    }
}

/// Context links are unique per transaction in SQLite (`PRIMARY KEY (transaction_id,
/// context_id)`), so duplicates are dropped and the first occurrence keeps its place (decision 2).
pub(crate) fn dedupe_context_ids(ids: &mut Vec<ContextId>) {
    let mut seen = HashSet::new();
    ids.retain(|id| seen.insert(*id));
}

/// The error for a missing row, e.g. `NotFound("transaction 42")`.
fn not_found(kind: &str, id: Id) -> EngineError {
    EngineError::NotFound(format!("{kind} {id}"))
}
