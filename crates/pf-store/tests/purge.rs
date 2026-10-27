//! L2 purge tests (R-PRG-1): soft-deleted rows, dangling links, nulled references, orphan mappings.

mod common;

use pf_model::testing::TestDb;
use pf_model::{AccountType, BudgetFrequency, BudgetItemType, CategoryType, RowKey};
use pf_store::{PurgeReport, Store};

use common::{file_path, inserts};

#[test]
/// Covers: R-PRG-1
fn r_prg_1_purge_removes_soft_deleted() {
    let (_dir, path) = file_path();
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let old_acc = t.account("Old", AccountType::Savings);
    let old_cat = t.category("Old", CategoryType::Expense);
    let old_ctx = t.context("Old");
    let old_scn = t.scenario("Old");
    let old_ob = t.obligation("Old").done();
    let old_bi = t
        .budget_item(
            BudgetItemType::Bill,
            "Old",
            "1",
            "2026-01-01",
            BudgetFrequency::Monthly,
        )
        .done();
    let old_tx = t.tx(chk, "2026-01-05", "-1", "Old").done();
    let live_tx = t.tx(chk, "2026-01-06", "-2", "Live").done();
    t.delete(RowKey::Account(old_acc));
    t.delete(RowKey::Category(old_cat));
    t.delete(RowKey::Context(old_ctx));
    t.delete(RowKey::Scenario(old_scn));
    t.delete(RowKey::Obligation(old_ob));
    t.delete(RowKey::BudgetItem(old_bi));
    t.delete(RowKey::Transaction(old_tx));
    let original = t.build();

    let mut store = Store::open(&path).expect("open");
    store
        .apply_changes(&inserts(&original), original.next_id())
        .expect("apply");
    let report = store.purge_soft_deleted().expect("purge");
    assert_eq!(
        report,
        PurgeReport {
            transactions: 1,
            accounts: 1,
            categories: 1,
            contexts: 1,
            scenarios: 1,
            obligations: 1,
            budget_items: 1,
            ..Default::default()
        }
    );
    let loaded = store.load_all().expect("load");
    assert_eq!(
        loaded.accounts.keys().copied().collect::<Vec<_>>(),
        vec![chk]
    );
    assert_eq!(
        loaded.transactions.keys().copied().collect::<Vec<_>>(),
        vec![live_tx]
    );
    assert_eq!(loaded.transaction(live_tx), original.transaction(live_tx));
    assert!(loaded.categories.is_empty());
    assert!(loaded.contexts.is_empty());
    assert!(loaded.scenarios.is_empty());
    assert!(loaded.obligations.is_empty());
    assert!(loaded.budget_items.is_empty());
}

#[test]
/// Covers: R-PRG-1
fn r_prg_1_purge_removes_dangling_links() {
    let (_dir, path) = file_path();
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let live_ctx = t.context("Live");
    let dead_ctx = t.context("Dead");
    t.delete(RowKey::Context(dead_ctx));
    let tid = t
        .tx(chk, "2026-01-05", "-1", "Tagged")
        .contexts(&[dead_ctx, live_ctx])
        .done();
    let db = t.build();
    let mut store = Store::open(&path).expect("open");
    store
        .apply_changes(&inserts(&db), db.next_id())
        .expect("apply");

    let report = store.purge_soft_deleted().expect("purge");
    assert_eq!(
        report,
        PurgeReport {
            contexts: 1,
            dangling_links: 1,
            ..Default::default()
        }
    );
    let loaded = store.load_all().expect("load");
    assert_eq!(
        loaded.transaction(tid).expect("live tx").context_ids,
        vec![live_ctx]
    );
}

#[test]
/// Covers: R-PRG-1
fn r_prg_1_purge_nullifies_references() {
    let (_dir, path) = file_path();
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let gone_acc = t.account("Gone", AccountType::Savings);
    let gone_ctx = t.context("Gone");
    let gone_cat = t.category("Gone", CategoryType::Expense);
    let gone_parent = t.category("Parent", CategoryType::Expense);
    let kid = t.child_category(gone_parent, "Kid");
    let tid = t
        .tx(chk, "2026-01-05", "-1", "Cat")
        .category(gone_cat)
        .done();
    let bi = t
        .budget_item(
            BudgetItemType::Bill,
            "Gym",
            "40",
            "2026-01-01",
            BudgetFrequency::Monthly,
        )
        .category(gone_cat)
        .account(gone_acc)
        .done();
    let ob = t
        .obligation("Car")
        .account(gone_acc)
        .context(gone_ctx)
        .done();
    t.delete(RowKey::Account(gone_acc));
    t.delete(RowKey::Context(gone_ctx));
    t.delete(RowKey::Category(gone_cat));
    t.delete(RowKey::Category(gone_parent));
    let db = t.build();
    let mut store = Store::open(&path).expect("open");
    store
        .apply_changes(&inserts(&db), db.next_id())
        .expect("apply");

    let report = store.purge_soft_deleted().expect("purge");
    assert_eq!(
        report,
        PurgeReport {
            accounts: 1,
            contexts: 1,
            categories: 2,
            nullified_refs: 6,
            ..Default::default()
        }
    );
    let loaded = store.load_all().expect("load");
    assert_eq!(loaded.transaction(tid).expect("tx").category_id, None);
    let item = loaded.budget_item(bi).expect("budget item");
    assert_eq!(item.category_id, None);
    assert_eq!(item.account_id, None);
    let ob = loaded.obligation(ob).expect("obligation");
    assert_eq!(ob.account_id, None);
    assert_eq!(ob.context_id, None);
    assert_eq!(loaded.category(kid).expect("child").parent_id, None);
}

#[test]
/// Covers: R-PRG-1
fn r_prg_1_purge_drops_orphan_bank_mappings() {
    let (_dir, path) = file_path();
    let mut t = TestDb::new();
    let live = t.account("Live", AccountType::Checking);
    let gone = t.account("Gone", AccountType::Checking);
    let orphan = t.import_mapping("column", "Date", "Date", Some(gone));
    let global = t.import_mapping("account", "MONARCH", "Checking", None);
    let kept = t.import_mapping("column", "Payee", "Payee", Some(live));
    t.delete(RowKey::Account(gone));
    let db = t.build();
    let mut store = Store::open(&path).expect("open");
    store
        .apply_changes(&inserts(&db), db.next_id())
        .expect("apply");

    let report = store.purge_soft_deleted().expect("purge");
    assert_eq!(
        report,
        PurgeReport {
            accounts: 1,
            orphan_mappings: 1,
            ..Default::default()
        }
    );
    let loaded = store.load_all().expect("load");
    assert!(!loaded.import_mappings.contains_key(&orphan));
    assert!(loaded.import_mappings.contains_key(&global));
    assert!(loaded.import_mappings.contains_key(&kept));
}
