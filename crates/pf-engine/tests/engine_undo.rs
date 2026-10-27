//! L1/L2 engine tests for undo, redo, persistence and state (T09: R-UND, R-SCN-2/3, R-PRG-1).

use std::path::PathBuf;
use std::time::Instant;

use jiff::civil::Date;
use pf_core::balance::account_cleared_balance;
use pf_core::obligation::{computed_balance, total_reserved};
use pf_engine::testing::{TEST_TODAY, World};
use pf_engine::{Engine, EngineError, Record, RowKey};
use pf_model::testing::TestDb;
use pf_model::{
    Account, AccountId, AccountType, Category, CategoryId, CategoryType, Db, Scenario, ScenarioId,
    TimePeriod, Transaction, TransactionId,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tempfile::TempDir;

/// A file-backed engine in a fresh temp dir. The `TempDir` must outlive the engine.
fn file_engine() -> (TempDir, PathBuf, Engine) {
    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("pf.sqlite");
    let engine = Engine::open(&path).expect("open");
    (dir, path, engine)
}

fn day(s: &str) -> Date {
    s.parse().expect("date")
}

/// Contents of a Db. Decision (1): `Db` equality includes the private id counter, so these
/// tests compare the rows (`all_records`, which includes soft-deleted rows and settings).
fn rows(db: &Db) -> Vec<Record> {
    db.all_records()
}

fn add_account(e: &mut Engine, name: &str) -> AccountId {
    e.apply("Add Account", |w| {
        Ok(w.insert_account(Account::new(
            AccountId(0),
            name,
            AccountType::Checking,
            "Bank",
        )))
    })
    .expect("add account")
}

fn add_scenario(e: &mut Engine, name: &str) -> ScenarioId {
    e.apply("Add Scenario", |w| {
        Ok(w.insert_scenario(Scenario::new(ScenarioId(0), name)))
    })
    .expect("add scenario")
}

#[test]
/// Covers: R-UND-1
fn r_und_1_apply_creates_one_named_step() {
    let mut e = Engine::open_in_memory().expect("open");
    e.apply("Add Things", |w| {
        w.insert_account(Account::new(
            AccountId(0),
            "Checking",
            AccountType::Checking,
            "",
        ));
        w.insert_category(Category::new(CategoryId(0), "Food", CategoryType::Expense));
        w.insert_transaction(Transaction::new(
            TransactionId(0),
            day("2026-01-05"),
            dec!(-3),
            "Cafe",
        ));
        Ok(())
    })
    .expect("apply");
    assert_eq!(e.undo_name(), Some("Add Things"));
    assert_eq!(e.undo().expect("undo"), Some("Add Things".to_string()));
    assert!(!e.can_undo(), "the three inserts were one step");
    assert!(rows(e.db()).is_empty());
}

#[test]
/// Covers: R-UND-2
fn r_und_2_undo_restores_exact_prior_state() {
    let mut e = Engine::open_in_memory().expect("open");
    let chk = add_account(&mut e, "Checking");
    let tx = e
        .apply("Add Tx", |w| {
            Ok(w.insert_transaction(Transaction::new(
                TransactionId(0),
                day("2026-01-05"),
                dec!(-10),
                "Coffee",
            )))
        })
        .expect("tx");
    let snapshot = e.db().clone();
    e.apply("Edit Everything", |w| {
        w.update_account(chk, |a| a.name = "Renamed".into())?;
        w.set_deleted(RowKey::Transaction(tx), true)?;
        w.insert_category(Category::new(CategoryId(0), "New", CategoryType::Income));
        Ok(())
    })
    .expect("edit");
    e.undo().expect("undo");
    assert_eq!(rows(e.db()), rows(&snapshot));
}

#[test]
/// Covers: R-UND-2
fn r_und_2_redo_reapplies_and_ping_pong_is_stable() {
    let mut e = Engine::open_in_memory().expect("open");
    let chk = add_account(&mut e, "Checking");
    let before = e.db().clone();
    e.apply("Rename", |w| {
        w.update_account(chk, |a| a.name = "New".into())
    })
    .expect("rename");
    let after = rows(e.db());
    for _ in 0..2 {
        e.undo().expect("undo");
        assert_eq!(rows(e.db()), rows(&before));
        e.redo().expect("redo");
        assert_eq!(rows(e.db()), after);
    }
}

#[test]
/// Covers: R-UND-3
fn r_und_3_new_edit_clears_redo() {
    let mut e = Engine::open_in_memory().expect("open");
    add_account(&mut e, "A");
    e.undo().expect("undo");
    assert!(e.can_redo());
    add_account(&mut e, "B");
    assert!(!e.can_redo());
    assert_eq!(e.redo_name(), None);
}

#[test]
/// Covers: R-UND-4
fn r_und_4_history_empty_after_open() {
    let (_dir, path, mut e) = file_engine();
    add_account(&mut e, "Persisted");
    drop(e);
    let reopened = Engine::open(&path).expect("reopen");
    assert!(!reopened.can_undo());
    assert_eq!(reopened.db().live_accounts().count(), 1);
}

#[test]
/// Covers: R-UND-5
fn r_und_5_undo_redo_names() {
    let mut e = Engine::open_in_memory().expect("open");
    assert_eq!(e.undo_name(), None);
    assert_eq!(e.redo_name(), None);
    e.apply("A", |w| {
        w.insert_account(Account::new(AccountId(0), "x", AccountType::Cash, ""));
        Ok(())
    })
    .expect("a");
    e.apply("B", |w| {
        w.insert_account(Account::new(AccountId(0), "y", AccountType::Cash, ""));
        Ok(())
    })
    .expect("b");
    assert_eq!(e.undo_name(), Some("B"));
    assert_eq!(e.redo_name(), None);
    assert_eq!(e.undo().expect("undo"), Some("B".to_string()));
    assert_eq!(e.undo_name(), Some("A"));
    assert_eq!(e.redo_name(), Some("B"));
    assert_eq!(e.redo().expect("redo"), Some("B".to_string()));
    assert_eq!(e.redo_name(), None);
    e.clear_history();
    assert_eq!(e.undo_name(), None);
    assert_eq!(e.redo_name(), None);
    assert_eq!(e.undo().expect("undo on empty"), None);
}

#[test]
/// Covers: R-UND-6
fn r_und_6_apply_persists_before_return() {
    let (_dir, path, mut e) = file_engine();
    add_account(&mut e, "Persisted");
    let second = Engine::open(&path).expect("second engine");
    assert_eq!(second.db().live_accounts().count(), 1);
}

#[test]
/// Covers: R-UND-6
fn r_und_6_undo_is_persisted() {
    let (_dir, path, mut e) = file_engine();
    add_account(&mut e, "Gone soon");
    e.undo().expect("undo");
    drop(e);
    let reopened = Engine::open(&path).expect("reopen");
    assert!(reopened.db().accounts.is_empty());
}

#[test]
fn apply_with_no_changes_creates_no_step() {
    let mut e = Engine::open_in_memory().expect("open");
    let chk = add_account(&mut e, "Checking");
    let rev = e.revision();
    e.apply("Set Same Name", |w| {
        w.update_account(chk, |a| a.name = "Checking".into())
    })
    .expect("noop");
    assert_eq!(e.revision(), rev);
    assert_eq!(e.undo_name(), Some("Add Account"));
}

#[test]
fn apply_error_rolls_back_memory() {
    let mut e = Engine::open_in_memory().expect("open");
    let snapshot = e.db().clone();
    let rev = e.revision();
    let result: Result<(), EngineError> = e.apply("Broken", |w| {
        w.insert_account(Account::new(AccountId(0), "Ghost", AccountType::Cash, ""));
        Err(EngineError::Validation("nope".into()))
    });
    assert_eq!(result, Err(EngineError::Validation("nope".into())));
    assert_eq!(e.db(), &snapshot);
    assert!(!e.can_undo());
    assert_eq!(e.revision(), rev);
}

#[test]
/// Covers: R-UND-6
fn store_failure_rolls_back_memory() {
    let (_dir, path, mut e) = file_engine();
    let chk = add_account(&mut e, "Checking");
    let snapshot = e.db().clone();
    let rev = e.revision();
    // Make the next account insert fail inside SQLite. A second connection installs the trigger.
    let raw = rusqlite::Connection::open(&path).expect("raw connection");
    raw.execute_batch(
        "CREATE TRIGGER boom BEFORE INSERT ON accounts BEGIN SELECT RAISE(ABORT, 'boom'); END;",
    )
    .expect("trigger");
    let result = e.apply("Add Blocked", |w| {
        w.insert_account(Account::new(AccountId(0), "Blocked", AccountType::Cash, ""));
        w.update_account(chk, |a| a.name = "Renamed".into())?;
        Ok(())
    });
    assert!(matches!(result, Err(EngineError::Store(_))), "{result:?}");
    assert_eq!(
        e.db(),
        &snapshot,
        "memory restored, including the id counter"
    );
    assert_eq!(e.revision(), rev);
    assert_eq!(e.undo_name(), Some("Add Account"), "no new step");
}

#[test]
fn revision_bumps_on_apply_undo_redo() {
    let mut e = Engine::open_in_memory().expect("open");
    let r0 = e.revision();
    add_account(&mut e, "A");
    assert_eq!(e.revision(), r0 + 1);
    e.undo().expect("undo");
    assert_eq!(e.revision(), r0 + 2);
    e.redo().expect("redo");
    assert_eq!(e.revision(), r0 + 3);
}

#[test]
/// Covers: R-SCN-2
fn r_scn_2_toggle_same_scenario_returns_to_reality() {
    let mut e = Engine::open_in_memory().expect("open");
    let a = add_scenario(&mut e, "Plan A");
    let b = add_scenario(&mut e, "Plan B");
    e.toggle_scenario(Some(a));
    assert_eq!(e.state.active_scenario, Some(a));
    e.toggle_scenario(Some(a));
    assert_eq!(e.state.active_scenario, None);
    e.toggle_scenario(Some(a));
    e.toggle_scenario(Some(b));
    assert_eq!(e.state.active_scenario, Some(b));
    e.toggle_scenario(None);
    assert_eq!(e.state.active_scenario, None);
}

#[test]
/// Covers: R-SCN-3
fn fix_state_clears_deleted_active_scenario() {
    let mut e = Engine::open_in_memory().expect("open");
    let s = add_scenario(&mut e, "Plan A");
    e.set_active_scenario(Some(s));
    e.apply("Delete Scenario", |w| {
        w.set_deleted(RowKey::Scenario(s), true)
    })
    .expect("delete");
    assert_eq!(e.state.active_scenario, None);
    // Undo brings the scenario back, but the active selection stays cleared.
    e.undo().expect("undo");
    assert_eq!(e.state.active_scenario, None);
}

#[test]
fn set_time_period_persists_without_undo() {
    let (_dir, path, mut e) = file_engine();
    e.set_time_period(TimePeriod::PastYear).expect("set");
    assert!(!e.can_undo());
    drop(e);
    let reopened = Engine::open(&path).expect("reopen");
    assert_eq!(reopened.state.time_period, TimePeriod::PastYear);
    assert!(!reopened.can_undo());
}

#[test]
fn set_setting_direct_persists_without_undo() {
    let (_dir, path, mut e) = file_engine();
    let rev = e.revision();
    e.set_setting_direct("backupFolder", "/Users/example/Backups")
        .expect("set");
    assert!(!e.can_undo());
    assert_eq!(e.revision(), rev + 1);
    drop(e);
    let reopened = Engine::open(&path).expect("reopen");
    assert_eq!(
        reopened.setting("backupFolder"),
        Some("/Users/example/Backups")
    );
    assert!(!reopened.can_undo());
}

#[test]
fn bulk_apply_touching_10k_rows_is_fast() {
    let mut e = Engine::open_in_memory().expect("open");
    let start = Instant::now();
    e.apply("Import 10k", |w| {
        for i in 0..10_000 {
            w.insert_transaction(Transaction::new(
                TransactionId(0),
                day("2026-01-05"),
                Decimal::from(i),
                "bulk",
            ));
        }
        Ok(())
    })
    .expect("bulk");
    let elapsed = start.elapsed();
    assert_eq!(e.db().transactions.len(), 10_000);
    assert!(elapsed.as_secs_f64() < 1.0, "took {elapsed:?}");
}

#[test]
fn world_fixture_facts() {
    let mut w = World::with_fixture("world");
    let db = w.engine.db();
    assert_eq!(db.live_accounts().count(), 5);
    assert_eq!(db.live_categories().count(), 6);
    assert_eq!(db.live_contexts().count(), 2);
    assert_eq!(db.live_scenarios().count(), 1);
    assert_eq!(db.live_obligations().count(), 2);
    assert_eq!(db.live_budget_items().count(), 5);
    assert_eq!(w.live_tx_count(), 14);
    let plan_a = w.scenario("Plan A");
    assert_eq!(db.transactions_for_scenario(plan_a).count(), 1);

    let checking = w.account("Checking");
    let savings = w.account("Savings");
    let visa = w.account("Visa");
    let mortgage = w.account("Mortgage");
    let k401 = w.account("External 401k");
    assert!(db.account(k401).expect("401k").is_external);
    assert_eq!(w.balance(checking), dec!(2051.95));
    assert_eq!(w.cleared_balance(checking), dec!(2194.25));
    assert_eq!(w.reality_balance(checking), dec!(2051.95));
    assert_eq!(w.balance(savings), dec!(10500));
    assert_eq!(w.balance(visa), dec!(-668.40));
    assert_eq!(w.balance(k401), dec!(1000));
    assert_eq!(w.balance(mortgage), dec!(0));

    // Plan A active: only the scenario row moves Checking; cleared and Reality stay put.
    w.activate_scenario("Plan A");
    assert_eq!(w.balance(checking), dec!(551.95));
    assert_eq!(w.cleared_balance(checking), dec!(2194.25));
    assert_eq!(w.reality_balance(checking), dec!(2051.95));
    w.activate_reality();

    // Cleared, non-external totals (R-BAL-5).
    let db = w.engine.db();
    let cleared = |a| account_cleared_balance(db, a, None);
    assert_eq!(
        cleared(checking) + cleared(savings) + cleared(visa) + cleared(mortgage),
        dec!(12025.85)
    );
    assert_eq!(
        cleared(checking) + cleared(savings),
        dec!(12694.25),
        "assets"
    );
    assert_eq!(cleared(visa), dec!(-668.40), "credit debt");
    assert_eq!(cleared(mortgage), dec!(0), "loan debt");

    // Obligations: Property Tax reserves 700; Vacation Fund is Completed, so it reserves nothing.
    let prop = db.obligation(w.obligation("Property Tax")).expect("prop");
    let vac = db.obligation(w.obligation("Vacation Fund")).expect("vac");
    assert_eq!(computed_balance(db, prop, None), dec!(700));
    assert_eq!(computed_balance(db, vac, None), dec!(1350));
    assert_eq!(total_reserved(db, checking, None), dec!(700));
    assert_eq!(total_reserved(db, savings, None), dec!(0));

    // Uncategorized: 6 live rows. "Cafe Luna" is one of them and matches Dining's "Cafe".
    let uncategorized = db
        .live_transactions()
        .filter(|t| t.category_id.is_none())
        .count();
    assert_eq!(uncategorized, 6);
    let dining = db.category_by_name("Dining").expect("dining");
    assert!(dining.match_patterns.iter().any(|p| p == "Cafe"));
}

#[test]
fn world_reload_keeps_data_and_clock() {
    let mut w = World::with_fixture("world");
    w.reload();
    assert_eq!(w.engine.today(), TEST_TODAY);
    assert_eq!(w.live_tx_count(), 14);
}

#[test]
fn replace_db_swaps_everything_and_clears_history() {
    let (_dir, path, mut e) = file_engine();
    let s = add_scenario(&mut e, "Plan A");
    add_account(&mut e, "Old");
    e.set_active_scenario(Some(s));
    let mut t = TestDb::new();
    t.account("Only", AccountType::Savings);
    e.replace_db(t.build()).expect("replace");
    assert!(!e.can_undo());
    assert_eq!(e.state.active_scenario, None);
    drop(e);
    let reopened = Engine::open(&path).expect("reopen");
    let names: Vec<&str> = reopened
        .db()
        .live_accounts()
        .map(|a| a.name.as_str())
        .collect();
    assert_eq!(names, vec!["Only"]);
    assert_eq!(reopened.db().live_scenarios().count(), 0);
}
