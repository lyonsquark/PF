//! The engine: one in-memory `Db`, one SQLite store, and the undo/redo stacks. `apply` is the
//! only way to change data (R-UND-1); every applied step is persisted before it returns (R-UND-6).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use jiff::civil::Date;
use pf_model::{Change, Db, Record, RowKey, ScenarioId, TimePeriod};
use pf_store::Store;

use crate::error::Result;
use crate::state::AppState;
use crate::writer::{Writer, dedupe_context_ids, set_image};

/// Setting that stores the account-detail time window (Swift `selectedTimePeriod`).
const KEY_TIME_PERIOD: &str = "selectedTimePeriod";

/// One undoable user action: a human name and the row images it changed (R-UND-1, R-UND-2).
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    /// Name shown in the Edit menu, e.g. "Delete Account".
    pub name: String,
    /// Before/after images in touch order.
    pub changes: Vec<Change>,
}

/// The engine behind a mutex, shared by the UI and the API thread.
pub type SharedEngine = Arc<Mutex<Engine>>;

/// The application core: data, persistence, undo history and UI-level state.
pub struct Engine {
    db: Db,
    store: Store,
    undo: Vec<Step>,
    redo: Vec<Step>,
    /// Active scenario and time window. Not undoable.
    pub state: AppState,
    /// Bumps on every change; UI caches key on it.
    revision: u64,
    /// Injected local date. Tests fix it.
    clock: Box<dyn Fn() -> Date + Send>,
}

impl Engine {
    /// Opens the SQLite file, purges soft-deleted rows (R-PRG-1), loads everything and starts
    /// with an empty undo history (R-UND-4).
    pub fn open(path: &Path) -> Result<Engine> {
        Self::from_store(Store::open(path)?)
    }

    /// An in-memory engine, for tests.
    pub fn open_in_memory() -> Result<Engine> {
        Self::from_store(Store::open_in_memory()?)
    }

    /// Purge, load, and restore the time window from its setting.
    fn from_store(mut store: Store) -> Result<Engine> {
        // R-PRG-1: runs once per launch, before any undo step exists.
        let report = store.purge_soft_deleted()?;
        log::info!("purged soft-deleted rows: {report:?}");
        let db = store.load_all()?;
        let time_period = db
            .setting(KEY_TIME_PERIOD)
            .map(TimePeriod::parse_or_default)
            .unwrap_or_default();
        Ok(Engine {
            db,
            store,
            undo: Vec::new(),
            redo: Vec::new(),
            state: AppState {
                active_scenario: None,
                time_period,
            },
            revision: 0,
            clock: Box::new(|| jiff::Zoned::now().date()),
        })
    }

    /// The default database file (`~/Library/Application Support/PF/PFRust.sqlite`).
    pub fn default_db_path() -> PathBuf {
        pf_store::default_db_path()
    }

    /// The SQLite file, or `None` for an in-memory engine.
    pub fn db_path(&self) -> Option<&Path> {
        self.store.path()
    }

    /// Read access to all data, including soft-deleted rows.
    pub fn db(&self) -> &Db {
        &self.db
    }

    /// Changes on every apply, undo, redo and state change; UI caches key on it.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The local date from the injected clock.
    pub fn today(&self) -> Date {
        (self.clock)()
    }

    /// Replaces the clock. Tests use it to fix the day.
    pub fn set_clock(&mut self, clock: Box<dyn Fn() -> Date + Send>) {
        self.clock = clock;
    }

    /// Wraps the engine for sharing between the UI and the API thread.
    pub fn shared(self) -> SharedEngine {
        Arc::new(Mutex::new(self))
    }

    /// Runs `f` against a write handle as one undo step (R-UND-1).
    ///
    /// On `Err`, every touched row is restored and nothing is stored. On `Ok` with no real
    /// changes, there is no step and no revision bump (R-TXN-5). Otherwise the diff is persisted
    /// in one SQL transaction (R-UND-6) and pushed as a step; redo is cleared (R-UND-3).
    /// If the store fails, memory is restored, so the Db never diverges from disk.
    pub fn apply<T>(
        &mut self,
        name: &str,
        f: impl FnOnce(&mut Writer<'_>) -> Result<T>,
    ) -> Result<T> {
        let today = self.today();
        let start_next_id = self.db.next_id();
        let mut w = Writer::new(&mut self.db, today);
        let value = match f(&mut w) {
            Ok(v) => v,
            Err(e) => {
                w.rollback();
                return Err(e);
            }
        };
        let changes = w.finish();
        if changes.is_empty() {
            self.db.set_next_id(start_next_id);
            return Ok(value);
        }
        if let Err(e) = self.store.apply_changes(&changes, self.db.next_id()) {
            restore_before(&mut self.db, &changes);
            self.db.set_next_id(start_next_id);
            return Err(e.into());
        }
        self.undo.push(Step {
            name: name.to_string(),
            changes,
        });
        self.redo.clear();
        self.bump();
        Ok(value)
    }

    /// Reverts the newest step (R-UND-2). Returns its name, or `None` when there is nothing to undo.
    /// The inverse is persisted in one transaction; if that fails, memory is put back and the
    /// step stays on the undo stack.
    pub fn undo(&mut self) -> Result<Option<String>> {
        let Some(step) = self.undo.pop() else {
            return Ok(None);
        };
        // Reverse order, before/after swapped, so a child is removed before its parent.
        let inverse: Vec<Change> = step.changes.iter().rev().map(Change::inverted).collect();
        restore_before(&mut self.db, &step.changes);
        if let Err(e) = self.store.apply_changes(&inverse, self.db.next_id()) {
            for c in &step.changes {
                set_image(&mut self.db, &c.key, &c.after);
            }
            self.undo.push(step);
            return Err(e.into());
        }
        let name = step.name.clone();
        self.redo.push(step);
        self.bump();
        Ok(Some(name))
    }

    /// Re-applies the newest undone step (R-UND-2). Returns its name, or `None` when empty.
    pub fn redo(&mut self) -> Result<Option<String>> {
        let Some(step) = self.redo.pop() else {
            return Ok(None);
        };
        for c in &step.changes {
            set_image(&mut self.db, &c.key, &c.after);
        }
        if let Err(e) = self.store.apply_changes(&step.changes, self.db.next_id()) {
            restore_before(&mut self.db, &step.changes);
            self.redo.push(step);
            return Err(e.into());
        }
        let name = step.name.clone();
        self.undo.push(step);
        self.bump();
        Ok(Some(name))
    }

    /// Name of the step that `undo` would revert.
    pub fn undo_name(&self) -> Option<&str> {
        self.undo.last().map(|s| s.name.as_str())
    }

    /// Name of the step that `redo` would re-apply.
    pub fn redo_name(&self) -> Option<&str> {
        self.redo.last().map(|s| s.name.as_str())
    }

    /// Whether `undo` has a step to revert.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether `redo` has a step to re-apply.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Forgets all undo and redo steps. Rows are not changed.
    pub fn clear_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    /// Replaces the whole Db (Markdown import, Danger Zone wipe). Persists in one SQL transaction
    /// through `Store::replace_all`. Clears history and the active scenario. On error, the old
    /// Db stays in memory and on disk.
    pub fn replace_db(&mut self, mut db: Db) -> Result<()> {
        for t in db.transactions.values_mut() {
            dedupe_context_ids(&mut t.context_ids);
        }
        let next = db.next_id().max(db.max_id() + 1);
        db.set_next_id(next);
        self.store.replace_all(&db)?;
        self.db = db;
        self.clear_history();
        self.state.active_scenario = None;
        self.bump();
        Ok(())
    }

    /// R-SCN-2: the same id turns the scenario off (Reality); another id switches to it.
    /// Not undoable. Bumps the revision.
    pub fn toggle_scenario(&mut self, id: Option<ScenarioId>) {
        self.state.active_scenario =
            pf_core::balance::toggle_scenario(self.state.active_scenario, id);
        self.revision += 1;
    }

    /// Sets the active scenario directly (`None` = Reality). Not undoable. Bumps the revision.
    pub fn set_active_scenario(&mut self, id: Option<ScenarioId>) {
        self.state.active_scenario = id;
        self.revision += 1;
    }

    /// Persists the account-detail time window as a setting. Not an undo step.
    pub fn set_time_period(&mut self, p: TimePeriod) -> Result<()> {
        self.set_setting_direct(KEY_TIME_PERIOD, p.as_str())?;
        self.state.time_period = p;
        self.revision += 1;
        Ok(())
    }

    /// Writes a UI or preference setting to Db and SQLite. Not an undo step (use
    /// `Writer::set_setting` when the setting must undo with a user action). Bumps the revision.
    pub fn set_setting_direct(&mut self, key: &str, value: &str) -> Result<()> {
        let row = RowKey::Setting(key.to_string());
        let change = Change {
            key: row.clone(),
            before: self.db.get(&row),
            after: Some(Record::Setting {
                key: key.to_string(),
                value: value.to_string(),
            }),
        };
        self.store
            .apply_changes(std::slice::from_ref(&change), self.db.next_id())?;
        set_image(&mut self.db, &change.key, &change.after);
        self.revision += 1;
        Ok(())
    }

    /// The raw value of a setting, if set.
    pub fn setting(&self, key: &str) -> Option<&str> {
        self.db.setting(key)
    }

    /// Counts a successful state change and clears any state that no longer exists.
    fn bump(&mut self) {
        self.revision += 1;
        self.fix_state();
    }

    /// If the active scenario is missing or soft-deleted, go back to Reality (R-SCN-3).
    fn fix_state(&mut self) {
        if let Some(id) = self.state.active_scenario {
            let gone = self.db.scenario(id).is_none_or(|s| s.is_deleted);
            if gone {
                self.state.active_scenario = None;
            }
        }
    }
}

/// Puts back the before-images of `changes`, last first (used after a store failure).
fn restore_before(db: &mut Db, changes: &[Change]) {
    for c in changes.iter().rev() {
        set_image(db, &c.key, &c.before);
    }
}
