//! L2 store tests (T07): SQLite round-trips, schema, atomic writes, wipe and replace.

mod common;

use std::path::Path;
use std::time::Instant;

use common::{file_path, inserts, remove, upsert};
use jiff::Timestamp;
use pf_model::testing::{TestDb, d, m};
use pf_model::{
    Account, AccountId, AccountType, BudgetFrequency, BudgetItemType, CategoryType, Change, Db,
    ObligationStatus, Record, RowKey, Scenario, ScenarioId, Transaction, TransactionId,
    TransactionStatus,
};
use pf_store::{Store, StoreError, default_db_path};

/// Unwraps an error result without requiring `Debug` on the success type.
fn expect_err<T>(r: Result<T, StoreError>) -> StoreError {
    match r {
        Ok(_) => panic!("expected an error"),
        Err(e) => e,
    }
}

/// Runs one SQL count query on a second connection to the same file.
fn raw_count(path: &Path, sql: &str) -> i64 {
    let conn = rusqlite::Connection::open(path).expect("open raw connection");
    conn.query_row(sql, [], |r| r.get(0)).expect("count query")
}

/// A database with every entity type, optional fields both set and unset, and context order [c2, c1].
fn full_db() -> Db {
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let ext = t.external_account("Brokerage", AccountType::Investment);
    let s = t.scenario("Plan A");
    let food = t.category("Food", CategoryType::Expense);
    let coffee = t.child_category(food, "Coffee");
    t.patterns(coffee, &["starbucks", "blue bottle"]);
    let c1 = t.context("Trip");
    let c2 = t.context("Work");
    t.tx(chk, "2026-03-04", "-4.25", "Blue Bottle")
        .status(TransactionStatus::Reconciled)
        .scenario(s)
        .category(coffee)
        .contexts(&[c2, c1])
        .memo("latte")
        .attachment("/receipts/1.pdf")
        .done();
    t.tx(chk, "2026-03-05", "1000.00", "Pay")
        .status(TransactionStatus::Pending)
        .done();
    t.orphan_tx("2026-03-06", "-0.10", "Orphan")
        .deleted()
        .done();
    let car = t
        .obligation("Car")
        .account(chk)
        .context(c1)
        .status(ObligationStatus::Spending)
        .balance("150.00")
        .target("900")
        .due("2026-12-31")
        .done();
    t.obligation("Bare").done();
    t.budget_item(
        BudgetItemType::Bill,
        "Gym",
        "40",
        "2026-01-01",
        BudgetFrequency::SpecificMonths,
    )
    .end("2026-12-31")
    .months(&[1, 6, 12])
    .category(food)
    .account(ext)
    .notes("monthly")
    .done();
    t.budget_item(
        BudgetItemType::Income,
        "Salary",
        "2000.5",
        "2026-01-15",
        BudgetFrequency::Monthly,
    )
    .done();
    t.import_mapping("account", "CHK-1234", "Checking", Some(chk));
    t.import_mapping("payee", "PAY", "Payee", None);
    t.setting("apiPort", "8989");
    t.setting("lastProjectionScenario", "Plan A");
    let mut db = t.build();

    // Fields the builder does not set: colours, icons, notes, timestamps, archive, currency, negate.
    let scenario = db.scenarios.get_mut(&s).expect("scenario");
    scenario.color_hex = Some("#00FF00".into());
    scenario.notes = Some("what if".into());
    let cat = db.categories.get_mut(&food).expect("category");
    cat.icon = Some("🍔".into());
    cat.notes = Some("eat".into());
    let ctx = db.contexts.get_mut(&c1).expect("context");
    ctx.color_hex = Some("#FF8800CC".into());
    ctx.notes = Some("holiday".into());
    ctx.is_archived = true;
    let ob = db.obligations.get_mut(&car).expect("obligation");
    ob.created_date = Some("2026-02-01T05:00:00Z".parse::<Timestamp>().expect("ts"));
    ob.last_updated_date = Some("2026-02-02T05:00:00.5Z".parse::<Timestamp>().expect("ts"));
    ob.notes = Some("saving".into());
    db.accounts.get_mut(&ext).expect("account").currency = "EUR".into();
    let mapping = db.import_mappings.values_mut().next().expect("mapping");
    mapping.negate_amounts = true;
    db
}

#[test]
/// Covers: R-UND-6
fn save_then_load_round_trips_every_entity_type() {
    let (_dir, path) = file_path();
    let original = full_db();
    {
        let mut store = Store::open(&path).expect("open");
        store
            .apply_changes(&inserts(&original), original.next_id())
            .expect("apply");
    }
    let store = Store::open(&path).expect("reopen");
    let loaded = store.load_all().expect("load");
    assert_eq!(loaded, original);
    let tx = loaded
        .transactions
        .values()
        .find(|t| t.payee == "Blue Bottle")
        .expect("tagged transaction");
    assert_eq!(tx.amount.to_string(), "-4.25");
    assert_eq!(tx.context_ids.len(), 2);
}

#[test]
fn schema_creation_is_idempotent() {
    let (_dir, path) = file_path();
    let first = Store::open(&path).expect("first open");
    assert_eq!(first.schema_version().expect("version"), 1);
    drop(first);
    let second = Store::open(&path).expect("second open");
    assert_eq!(second.schema_version().expect("version"), 1);
}

#[test]
fn unknown_schema_version_errors() {
    let (_dir, path) = file_path();
    let store = Store::open(&path).expect("open");
    store
        .execute_batch_for_test("UPDATE meta SET value = '99' WHERE key = 'schema_version'")
        .expect("set version");
    drop(store);
    let err = expect_err(Store::open(&path));
    assert!(matches!(
        err,
        StoreError::Corrupt {
            table: "meta",
            column: "schema_version",
            ..
        }
    ));
}

#[test]
/// Covers: R-UND-6
fn apply_changes_is_atomic() {
    let (_dir, path) = file_path();
    let mut store = Store::open(&path).expect("open");
    store
        .execute_batch_for_test(
            "CREATE TRIGGER boom BEFORE INSERT ON scenarios BEGIN SELECT RAISE(ABORT,'boom'); END;",
        )
        .expect("install trigger");
    let changes = vec![
        upsert(Record::Account(Account::new(
            AccountId(1),
            "Checking",
            AccountType::Checking,
            "Bank",
        ))),
        upsert(Record::Scenario(Scenario::new(ScenarioId(2), "Plan A"))),
    ];
    assert!(store.apply_changes(&changes, 3).is_err());
    let db = store.load_all().expect("load");
    assert!(db.accounts.is_empty());
    assert!(db.scenarios.is_empty());
}

#[test]
fn apply_changes_rejects_key_mismatch() {
    let (_dir, path) = file_path();
    let mut store = Store::open(&path).expect("open");
    let bad = Change {
        key: RowKey::Account(AccountId(9)),
        before: None,
        after: Some(Record::Scenario(Scenario::new(ScenarioId(1), "Plan A"))),
    };
    assert!(store.apply_changes(&[bad], 2).is_err());
    assert!(store.load_all().expect("load").scenarios.is_empty());
}

#[test]
fn apply_changes_delete_removes_row_and_links() {
    let (_dir, path) = file_path();
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let c1 = t.context("Trip");
    let c2 = t.context("Work");
    let tid = t
        .tx(chk, "2026-01-05", "-12.50", "Coffee")
        .contexts(&[c1, c2])
        .done();
    let db = t.build();
    let mut store = Store::open(&path).expect("open");
    store
        .apply_changes(&inserts(&db), db.next_id())
        .expect("insert");
    assert_eq!(
        raw_count(&path, "SELECT COUNT(*) FROM transaction_contexts"),
        2
    );
    store
        .apply_changes(&[remove(RowKey::Transaction(tid))], db.next_id())
        .expect("delete");
    assert!(store.load_all().expect("load").transactions.is_empty());
    assert_eq!(
        raw_count(&path, "SELECT COUNT(*) FROM transaction_contexts"),
        0
    );
}

#[test]
fn update_replaces_contexts() {
    let (_dir, path) = file_path();
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let a = t.context("A");
    let b = t.context("B");
    let tid = t.tx(chk, "2026-01-05", "-1", "X").contexts(&[a, b]).done();
    let db = t.build();
    let mut store = Store::open(&path).expect("open");
    store
        .apply_changes(&inserts(&db), db.next_id())
        .expect("insert");

    let mut tx = db.transaction(tid).expect("tx").clone();
    tx.context_ids = vec![b];
    store
        .apply_changes(&[upsert(Record::Transaction(tx))], db.next_id())
        .expect("update");
    let loaded = store.load_all().expect("load");
    assert_eq!(loaded.transaction(tid).expect("tx").context_ids, vec![b]);
    assert_eq!(
        raw_count(&path, "SELECT COUNT(*) FROM transaction_contexts"),
        1
    );
}

#[test]
fn settings_round_trip_and_delete() {
    let (_dir, path) = file_path();
    let mut store = Store::open(&path).expect("open");
    let set = Record::Setting {
        key: "backupFolder".into(),
        value: "/Users/me/Backups".into(),
    };
    store.apply_changes(&[upsert(set)], 1).expect("set");
    let loaded = store.load_all().expect("load");
    assert_eq!(loaded.setting("backupFolder"), Some("/Users/me/Backups"));

    store
        .apply_changes(&[remove(RowKey::Setting("backupFolder".into()))], 1)
        .expect("delete");
    assert_eq!(
        store.load_all().expect("load").setting("backupFolder"),
        None
    );
}

#[test]
/// Covers: R-UND-6
fn next_id_survives_restart() {
    let (_dir, path) = file_path();
    {
        let mut store = Store::open(&path).expect("open");
        store.apply_changes(&[], 42).expect("apply");
    }
    let store = Store::open(&path).expect("reopen");
    assert_eq!(store.load_all().expect("load").next_id(), 42);
    drop(store);

    let mut store = Store::open(&path).expect("open again");
    let big = Record::Account(Account::new(
        AccountId(100),
        "Big",
        AccountType::Savings,
        "Bank",
    ));
    store.apply_changes(&[upsert(big)], 42).expect("apply");
    drop(store);
    let store = Store::open(&path).expect("reopen again");
    assert_eq!(store.load_all().expect("load").next_id(), 101);
}

#[test]
fn wal_mode_enabled() {
    let (_dir, path) = file_path();
    let store = Store::open(&path).expect("open");
    assert_eq!(store.journal_mode().expect("journal mode"), "wal");
}

#[test]
fn wipe_all_empties_tables() {
    let (_dir, path) = file_path();
    let mut store = Store::open(&path).expect("open");
    store
        .apply_changes(&inserts(&full_db()), 500)
        .expect("apply");
    store.wipe_all().expect("wipe");
    let loaded = store.load_all().expect("load");
    assert!(loaded.all_records().is_empty());
    assert_eq!(store.schema_version().expect("version"), 1);
}

#[test]
/// Covers: R-MD-5
fn replace_all_is_atomic_and_complete() {
    let (_dir, path) = file_path();
    let mut store = Store::open(&path).expect("open");
    let mut old = TestDb::new();
    old.account("Old", AccountType::Cash);
    store
        .apply_changes(&inserts(&old.build()), 10)
        .expect("seed old data");

    let new = full_db();
    store.replace_all(&new).expect("replace");
    assert_eq!(store.load_all().expect("load"), new);

    store
        .execute_batch_for_test(
            "CREATE TRIGGER boom BEFORE INSERT ON scenarios BEGIN SELECT RAISE(ABORT,'boom'); END;",
        )
        .expect("install trigger");
    let mut failing = TestDb::new();
    failing.account("Other", AccountType::Cash);
    failing.scenario("Blocked");
    assert!(store.replace_all(&failing.build()).is_err());
    assert_eq!(store.load_all().expect("load"), new);
}

#[test]
fn default_db_path_is_pf_sqlite() {
    let path = default_db_path();
    assert!(path.ends_with("Library/Application Support/PF/PF.sqlite"));
    assert!(!path.to_string_lossy().contains("PF.store"));
}

#[test]
#[ignore = "perf: run with cargo test --release -p pf-store -- --ignored load_100k"]
fn load_100k_transactions_fast() {
    let (_dir, path) = file_path();
    let acc = Account::new(AccountId(1), "Checking", AccountType::Checking, "Bank");
    let mut changes = vec![upsert(Record::Account(acc))];
    for i in 0..100_000_i64 {
        let mut tx = Transaction::new(
            TransactionId(i + 2),
            d(&format!("2026-01-{:02}", 1 + i % 28)),
            m("-12.50"),
            "Payee",
        );
        tx.account_id = Some(AccountId(1));
        changes.push(upsert(Record::Transaction(tx)));
    }
    let mut store = Store::open(&path).expect("open");
    store.apply_changes(&changes, 100_002).expect("apply");
    drop(store);

    let store = Store::open(&path).expect("reopen");
    let start = Instant::now();
    let loaded = store.load_all().expect("load");
    let elapsed = start.elapsed();
    assert_eq!(loaded.transactions.len(), 100_000);
    assert!(elapsed.as_millis() < 500, "load took {elapsed:?}");
}
