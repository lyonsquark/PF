//! The `World` test helper: an engine on a temp SQLite file, with lookups by name, balances and
//! reloads. Shared by the behaviour tests of every crate. This is test support: lookups panic
//! with a message when a name is missing, which is the failure mode a test wants.

use std::path::{Path, PathBuf};

use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use pf_core::markdown::import_markdown;
use pf_model::{
    AccountId, BudgetItemId, CategoryId, ContextId, ObligationId, ScenarioId, TransactionId,
};
use rust_decimal::Decimal;
use tempfile::TempDir;

use crate::engine::Engine;

/// The day every `World` is pinned to.
pub const TEST_TODAY: Date = jiff::civil::date(2026, 3, 15);

/// Time zone for fixture imports and conversions. Decision (3): tests pin America/Chicago.
pub fn test_tz() -> TimeZone {
    TimeZone::get("America/Chicago").expect("system tzdb has America/Chicago")
}

/// Parses a `YYYY-MM-DD` literal in tests.
pub fn d(s: &str) -> Date {
    s.parse().expect("date literal")
}

/// Parses a decimal literal in tests.
pub fn m(s: &str) -> Decimal {
    s.parse().expect("decimal literal")
}

/// Fallback timestamp for fixture imports (fills missing obligation timestamps).
fn import_now() -> Timestamp {
    "2026-03-15T12:00:00Z".parse().expect("timestamp literal")
}

/// An engine on a temp SQLite file, with the clock fixed to `TEST_TODAY`.
pub struct World {
    /// The engine under test. Tests call it directly, as the UI would.
    pub engine: Engine,
    /// Keeps the SQLite file alive until the world is dropped.
    dir: TempDir,
    path: PathBuf,
}

/// Opens an engine on `path` with the test clock.
fn open_fixed(path: &Path) -> Engine {
    let mut engine = Engine::open(path).expect("open engine on temp file");
    engine.set_clock(Box::new(|| TEST_TODAY));
    engine
}

/// Reads `fixtures/{name}.md` from the repo-level fixtures directory.
fn read_fixture(name: &str) -> String {
    let p = format!("{}/../../fixtures/{name}.md", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("fixture {p}: {e}"))
}

impl World {
    /// An empty database in a temp file, clock fixed to `TEST_TODAY`.
    pub fn new() -> World {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("pf.sqlite");
        let engine = open_fixed(&path);
        World { engine, dir, path }
    }

    /// Loads `fixtures/{name}.md` through the Markdown importer, in `test_tz()`. Clears history.
    pub fn with_fixture(name: &str) -> World {
        let mut w = World::new();
        let text = read_fixture(name);
        let imported = import_markdown(&text, &test_tz(), import_now(), &mut |_| {});
        assert_eq!(imported.skipped_rows, 0, "fixture {name} has unparsed rows");
        w.engine
            .replace_db(imported.db)
            .expect("write fixture to the temp file");
        w.engine.clear_history();
        w
    }

    /// Restarts the engine on the same file (purge and load run again). Re-fixes the clock.
    pub fn reload(&mut self) {
        self.engine = open_fixed(&self.path);
    }

    /// Hands the engine to the caller. The temp dir is leaked on purpose (test-only) so the
    /// SQLite file outlives the returned engine.
    pub fn into_engine(self) -> Engine {
        let World { engine, dir, .. } = self;
        std::mem::forget(dir);
        engine
    }

    /// The SQLite file this world uses.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Id of the live account with exactly this name. Panics if missing.
    pub fn account(&self, name: &str) -> AccountId {
        self.engine
            .db()
            .account_by_name(name)
            .map(|a| a.id)
            .unwrap_or_else(|| panic!("no live account named {name:?}"))
    }

    /// Id of the live category with exactly this plain name. Panics if missing.
    pub fn category(&self, name: &str) -> CategoryId {
        self.engine
            .db()
            .category_by_name(name)
            .map(|c| c.id)
            .unwrap_or_else(|| panic!("no live category named {name:?}"))
    }

    /// Id of the live context with exactly this name. Panics if missing.
    pub fn context(&self, name: &str) -> ContextId {
        self.engine
            .db()
            .context_by_name(name)
            .map(|c| c.id)
            .unwrap_or_else(|| panic!("no live context named {name:?}"))
    }

    /// Id of the live scenario with exactly this name. Panics if missing.
    pub fn scenario(&self, name: &str) -> ScenarioId {
        self.engine
            .db()
            .scenario_by_name(name)
            .map(|s| s.id)
            .unwrap_or_else(|| panic!("no live scenario named {name:?}"))
    }

    /// Id of the live obligation with exactly this name. Panics if missing.
    pub fn obligation(&self, name: &str) -> ObligationId {
        self.engine
            .db()
            .live_obligations()
            .find(|o| o.name == name)
            .map(|o| o.id)
            .unwrap_or_else(|| panic!("no live obligation named {name:?}"))
    }

    /// Id of the live budget item with exactly this name. Panics if missing.
    pub fn budget_item(&self, name: &str) -> BudgetItemId {
        self.engine
            .db()
            .live_budget_items()
            .find(|b| b.name == name)
            .map(|b| b.id)
            .unwrap_or_else(|| panic!("no live budget item named {name:?}"))
    }

    /// Id of the one live transaction with this payee. Panics if there are none or several.
    pub fn tx_by_payee(&self, payee: &str) -> TransactionId {
        let ids: Vec<TransactionId> = self
            .engine
            .db()
            .live_transactions()
            .filter(|t| t.payee == payee)
            .map(|t| t.id)
            .collect();
        match ids.as_slice() {
            [one] => *one,
            [] => panic!("no live transaction with payee {payee:?}"),
            many => panic!("{} live transactions with payee {payee:?}", many.len()),
        }
    }

    /// Number of live (not soft-deleted) transactions.
    pub fn live_tx_count(&self) -> usize {
        self.engine.db().live_transactions().count()
    }

    /// Makes the named scenario the active one.
    pub fn activate_scenario(&mut self, name: &str) {
        let id = self.scenario(name);
        self.engine.set_active_scenario(Some(id));
    }

    /// Returns to Reality (no active scenario).
    pub fn activate_reality(&mut self) {
        self.engine.set_active_scenario(None);
    }

    /// R-BAL-1: all statuses, visible under the active scenario.
    pub fn balance(&self, a: AccountId) -> Decimal {
        pf_core::balance::account_balance(self.engine.db(), a, self.engine.state.active_scenario)
    }

    /// R-BAL-2: cleared and reconciled only, visible under the active scenario.
    pub fn cleared_balance(&self, a: AccountId) -> Decimal {
        pf_core::balance::account_cleared_balance(
            self.engine.db(),
            a,
            self.engine.state.active_scenario,
        )
    }

    /// R-BAL-3: all statuses in Reality, ignoring the active scenario.
    pub fn reality_balance(&self, a: AccountId) -> Decimal {
        pf_core::balance::account_balance(self.engine.db(), a, None)
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}
