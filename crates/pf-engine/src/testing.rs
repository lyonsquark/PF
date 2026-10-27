//! The `World` test helper: an engine on a temp SQLite file, with lookups by name, balances and
//! reloads. Shared by the behaviour tests of every crate. This is test support: lookups panic
//! with a message when a name is missing, which is the failure mode a test wants.

use std::path::{Path, PathBuf};

use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use pf_core::markdown::import_markdown;
use pf_model::{
    Account, AccountId, AccountType, BudgetItemId, Category, CategoryId, CategoryType, Context,
    ContextId, Db, ObligationId, Record, Scenario, ScenarioId, Transaction, TransactionId,
};
use rust_decimal::Decimal;
use tempfile::TempDir;

use crate::commands::transactions::{NewTransaction, TxKind, add_transaction};
use crate::engine::Engine;
use pf_model::TransactionStatus;

/// The day every `World` is pinned to.
pub const TEST_TODAY: Date = jiff::civil::date(2026, 3, 15);

/// Time zone for fixture imports and conversions. `fixtures/world.md` is New York-authored.
pub fn test_tz() -> TimeZone {
    TimeZone::get("America/New_York").expect("system tzdb has America/New_York")
}

/// The instant every `World` is pinned to: 09:05:07 on `TEST_TODAY` in `test_tz()`.
pub fn test_now() -> Timestamp {
    // Invariant: a fixed civil time in a bundled zone always resolves, so this cannot fail.
    TEST_TODAY
        .at(9, 5, 7, 0)
        .to_zoned(test_tz())
        .expect("09:05:07 on TEST_TODAY exists in test_tz")
        .timestamp()
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

/// Opens an engine on `path` with the test clock, instant and zone.
fn open_fixed(path: &Path) -> Engine {
    let mut engine = Engine::open(path).expect("open engine on temp file");
    engine.set_clock(Box::new(|| TEST_TODAY));
    engine.set_now_clock(Box::new(test_now));
    engine.set_tz(test_tz());
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

    /// Adds a Cleared deposit through `add_transaction` (R-TXN-8). `date` is `YYYY-MM-DD`.
    pub fn add_deposit(
        &mut self,
        acc: AccountId,
        date: &str,
        amount: &str,
        payee: &str,
    ) -> TransactionId {
        self.add_simple(TxKind::Deposit, acc, date, amount, payee)
    }

    /// Adds a Cleared withdrawal through `add_transaction` (R-TXN-8). The sign is forced.
    pub fn add_withdrawal(
        &mut self,
        acc: AccountId,
        date: &str,
        amount: &str,
        payee: &str,
    ) -> TransactionId {
        self.add_simple(TxKind::Withdrawal, acc, date, amount, payee)
    }

    /// Adds a Cleared transfer through `add_transaction`. Returns `(from_row, to_row)`.
    pub fn add_transfer(
        &mut self,
        from: AccountId,
        to: AccountId,
        date: &str,
        amount: &str,
    ) -> (TransactionId, TransactionId) {
        let ids = self.add(TxKind::Transfer { to }, from, date, amount, "");
        match ids.as_slice() {
            [out, into] => (*out, *into),
            other => panic!("transfer made {} rows, expected 2", other.len()),
        }
    }

    /// One Cleared, uncategorized row with the given kind.
    fn add_simple(
        &mut self,
        kind: TxKind,
        acc: AccountId,
        date: &str,
        amount: &str,
        payee: &str,
    ) -> TransactionId {
        let ids = self.add(kind, acc, date, amount, payee);
        match ids.as_slice() {
            [one] => *one,
            other => panic!("expected one new row, got {}", other.len()),
        }
    }

    /// Runs `add_transaction` for a Cleared, uncategorized, memo-less form.
    fn add(
        &mut self,
        kind: TxKind,
        account: AccountId,
        date: &str,
        amount: &str,
        payee: &str,
    ) -> Vec<TransactionId> {
        let form = NewTransaction {
            kind,
            account,
            date: d(date),
            amount: m(amount),
            payee: payee.to_string(),
            memo: String::new(),
            status: TransactionStatus::Cleared,
            category: None,
            contexts: Vec::new(),
        };
        add_transaction(&mut self.engine, form).expect("add transaction in World")
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

/// Shape of the generated large data set (T12b). `Default` is the 100k-row perf dataset.
#[derive(Debug, Clone, PartialEq)]
pub struct LargeDbSpec {
    /// LCG seed; the same seed gives the same data.
    pub seed: u64,
    /// Number of accounts.
    pub accounts: usize,
    /// Number of transactions (reality and scenario rows together).
    pub transactions: usize,
    /// Years back from `TEST_TODAY` that dates are spread over.
    pub years: i16,
    /// Number of categories.
    pub categories: usize,
    /// Number of contexts.
    pub contexts: usize,
    /// Number of scenarios.
    pub scenarios: usize,
}

impl Default for LargeDbSpec {
    fn default() -> Self {
        Self {
            seed: 42,
            accounts: 10,
            transactions: 100_000,
            years: 5,
            categories: 40,
            contexts: 20,
            scenarios: 3,
        }
    }
}

/// Deterministic 64-bit LCG. No RNG crate is needed for test data.
struct Lcg(u64);

impl Lcg {
    /// Next value: the high 31 bits of the state.
    fn step(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    /// A value in `0..n`. `n` must be non-zero.
    fn below(&mut self, n: u64) -> u64 {
        self.step() % n
    }
}

/// Payee names for generated rows: 200 names built from eight stems.
const PAYEE_STEMS: [&str; 8] = [
    "Abbey", "Crest", "Harbor", "Lumen", "Maple", "Orbit", "Pine", "River",
];

/// Deterministic large Db (T12b). Built directly, not through engine commands, so the
/// generator is fast. Mix: 90% reality and 10% scenario rows. Half of the scenario rows are
/// Future; the rest, and all reality rows, are 80% Cleared, 10% Pending and 10% Reconciled
/// (so about 5% Future overall). Amounts are −500.00…+3000.00. About 80% of rows have a category
/// and 20% have one or two contexts. Dates are spread over `years` years ending at `TEST_TODAY`.
pub fn generate_large_db(spec: &LargeDbSpec) -> Db {
    assert!(spec.accounts > 0, "a large db needs at least one account");
    let mut db = Db::new();
    let mut rng = Lcg(spec.seed);

    let accounts: Vec<AccountId> = (0..spec.accounts)
        .map(|i| {
            let id = AccountId::from(db.alloc_id());
            let kind = AccountType::ALL[i % AccountType::ALL.len()];
            let acc = Account::new(id, &format!("Account {i:02}"), kind, &format!("Bank {i}"));
            db.put(Record::Account(acc));
            id
        })
        .collect();

    let categories: Vec<CategoryId> = (0..spec.categories)
        .map(|i| {
            let id = CategoryId::from(db.alloc_id());
            let kind = CategoryType::ALL[i % CategoryType::ALL.len()];
            let mut cat = Category::new(id, &format!("Category {i:02}"), kind);
            let patterns = 1 + rng.below(3);
            cat.match_patterns = (0..patterns).map(|j| format!("merchant{i}-{j}")).collect();
            db.put(Record::Category(cat));
            id
        })
        .collect();

    let contexts: Vec<ContextId> = (0..spec.contexts)
        .map(|i| {
            let id = ContextId::from(db.alloc_id());
            db.put(Record::Context(Context::new(
                id,
                &format!("Context {i:02}"),
            )));
            id
        })
        .collect();

    let scenarios: Vec<ScenarioId> = (0..spec.scenarios)
        .map(|i| {
            let id = ScenarioId::from(db.alloc_id());
            db.put(Record::Scenario(Scenario::new(
                id,
                &format!("Scenario {i:02}"),
            )));
            id
        })
        .collect();

    let payees: Vec<String> = (0..200)
        .map(|k| format!("{} {k:03}", PAYEE_STEMS[k % PAYEE_STEMS.len()]))
        .collect();
    let span_days = u64::from(spec.years.unsigned_abs()) * 365;

    for _ in 0..spec.transactions {
        let id = TransactionId::from(db.alloc_id());
        let offset = i64::try_from(rng.below(span_days.max(1))).unwrap_or(0);
        let date = TEST_TODAY
            .checked_sub(jiff::Span::new().days(offset))
            .unwrap_or(TEST_TODAY);
        let cents = i64::try_from(rng.below(350_001)).unwrap_or(0) - 50_000;
        let amount = Decimal::new(cents, 2);
        let payee = &payees[usize::try_from(rng.below(200)).unwrap_or(0)];
        let mut tx = Transaction::new(id, date, amount, payee);

        let account = accounts[usize::try_from(rng.below(accounts.len() as u64)).unwrap_or(0)];
        tx.account_id = Some(account);
        let in_scenario = !scenarios.is_empty() && rng.below(10) == 0;
        if in_scenario {
            let pick = usize::try_from(rng.below(scenarios.len() as u64)).unwrap_or(0);
            tx.scenario_id = Some(scenarios[pick]);
        }
        tx.status = if in_scenario && rng.below(2) == 0 {
            TransactionStatus::Future
        } else {
            match rng.below(100) {
                0..80 => TransactionStatus::Cleared,
                80..90 => TransactionStatus::Pending,
                _ => TransactionStatus::Reconciled,
            }
        };
        if !categories.is_empty() && rng.below(10) < 8 {
            let pick = usize::try_from(rng.below(categories.len() as u64)).unwrap_or(0);
            tx.category_id = Some(categories[pick]);
        }
        if !contexts.is_empty() && rng.below(5) == 0 {
            for _ in 0..=rng.below(2) {
                let pick = usize::try_from(rng.below(contexts.len() as u64)).unwrap_or(0);
                if !tx.context_ids.contains(&contexts[pick]) {
                    tx.context_ids.push(contexts[pick]);
                }
            }
        }
        db.put(Record::Transaction(tx));
    }
    db
}

/// A `World` whose file holds the large data set from `generate_large_db(spec)`.
pub fn large_world(spec: &LargeDbSpec) -> World {
    let mut w = World::new();
    w.engine
        .replace_db(generate_large_db(spec))
        .expect("write large db to the temp file");
    w
}
