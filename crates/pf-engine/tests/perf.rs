//! Performance budgets from `docs/testing.md` ("Performance tests"). The timing tests are ignored
//! by default. Run them with:
//!
//! `cargo test --release -p pf-engine --test perf -- --ignored --nocapture`
//!
//! Budgets are asserted only in release builds; debug timings mean nothing here.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use pf_core::api_format::transaction_lines;
use pf_core::balance::{balance_maps, displayed_balances};
use pf_core::filter::{SearchScope, SearchState, account_view, filter_transactions};
use pf_engine::Engine;
use pf_engine::commands::backup::{export_markdown, import_markdown_replacing};
use pf_engine::commands::budget::{ProjectionRequest, ProjectionTarget, generate_projection};
use pf_engine::commands::transactions::{TxField, edit_transaction};
use pf_engine::testing::{LargeDbSpec, TEST_TODAY, World, d, generate_large_db};
use pf_model::{
    AccountId, BudgetFrequency, BudgetItem, BudgetItemId, BudgetItemType, Db, Record, TimePeriod,
    Transaction, TransactionId,
};
use rust_decimal::Decimal;

/// The 100k-row data set plus 40 monthly budget items, built once. Used as the start point of
/// every timing test, so each test gets its own engine and its own writes.
fn perf_db() -> &'static Db {
    static DB: OnceLock<Db> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = generate_large_db(&LargeDbSpec::default());
        let account = busiest_account(&db);
        let start = d("2021-03-01");
        for i in 0..40i64 {
            let id = BudgetItemId::from(db.alloc_id());
            let name = format!("Bill {i:02}");
            let mut item = BudgetItem::new(
                id,
                BudgetItemType::Bill,
                &name,
                Decimal::new(12_345 + i, 2),
                start,
                BudgetFrequency::Monthly,
            );
            item.account_id = Some(account);
            db.put(Record::BudgetItem(item));
        }
        db
    })
}

/// The account with the most transactions. Ties go to the higher id.
fn busiest_account(db: &Db) -> AccountId {
    db.live_accounts()
        .map(|a| a.id)
        .max_by_key(|id| db.transactions_for_account(*id).count())
        .expect("generated data has accounts")
}

/// A fresh engine on a temp file holding a copy of `db`.
fn world_from(db: &Db) -> World {
    let mut w = World::new();
    w.engine
        .replace_db(db.clone())
        .expect("write perf data to the temp file");
    w
}

/// Runs `op` and returns how long it took. The result is dropped after the clock stops, so
/// teardown is not timed.
fn timed<T>(op: impl FnOnce() -> T) -> Duration {
    let start = Instant::now();
    let value = op();
    let elapsed = start.elapsed();
    std::hint::black_box(&value);
    drop(value);
    elapsed
}

/// One warm-up run, then the best of five. Prints the result, and asserts the budget in release.
/// `run` performs one timed iteration and returns its duration.
fn check_budget(name: &str, budget_ms: f64, mut run: impl FnMut() -> Duration) {
    let _ = run();
    let best = (0..5).map(|_| run()).min().unwrap_or_default();
    let ms = best.as_secs_f64() * 1000.0;
    eprintln!("perf {name}: {ms:.2} ms (budget {budget_ms} ms)");
    if !cfg!(debug_assertions) {
        assert!(
            ms < budget_ms,
            "perf {name}: {ms:.2} ms is over the {budget_ms} ms budget"
        );
    }
}

#[test]
fn large_db_generator_is_deterministic() {
    let spec = LargeDbSpec {
        transactions: 2_000,
        ..LargeDbSpec::default()
    };
    let a = generate_large_db(&spec);
    let b = generate_large_db(&spec);
    assert_eq!(a, b, "the same seed must give the same data");
    assert_eq!(a.transactions.len(), 2_000);
}

#[test]
#[ignore = "perf: run with --release -- --ignored"]
fn perf_load_from_sqlite() {
    let w = world_from(perf_db());
    let path = w.path().to_path_buf();
    check_budget("perf_load_from_sqlite", 500.0, || {
        timed(|| Engine::open(&path).expect("open the 100k file"))
    });
}

#[test]
#[ignore = "perf: run with --release -- --ignored"]
fn perf_account_view() {
    let w = world_from(perf_db());
    let db = w.engine.db();
    let acc = busiest_account(db);
    let search = SearchState::new(TEST_TODAY);
    check_budget("perf_account_view", 15.0, || {
        timed(|| account_view(db, acc, None, TimePeriod::PastYear, &search, TEST_TODAY))
    });
}

#[test]
#[ignore = "perf: run with --release -- --ignored"]
fn perf_account_view_scenario_all() {
    let w = world_from(perf_db());
    let db = w.engine.db();
    let acc = busiest_account(db);
    let scenario = w.scenario("Scenario 00");
    let search = SearchState::new(TEST_TODAY);
    check_budget("perf_account_view_scenario_all", 15.0, || {
        timed(|| {
            account_view(
                db,
                acc,
                Some(scenario),
                TimePeriod::All,
                &search,
                TEST_TODAY,
            )
        })
    });
}

#[test]
#[ignore = "perf: run with --release -- --ignored"]
fn perf_dashboard_balances() {
    let w = world_from(perf_db());
    let db = w.engine.db();
    check_budget("perf_dashboard_balances", 10.0, || {
        timed(|| {
            let maps = balance_maps(db, None);
            displayed_balances(db, &maps, true)
        })
    });
}

/// The first categorized Reality row of the busiest account, which an edit can change alone.
fn categorized_reality_row(w: &World, acc: AccountId) -> TransactionId {
    w.engine
        .db()
        .transactions_for_account(acc)
        .find(|t| t.category_id.is_some() && t.scenario_id.is_none())
        .map(|t| t.id)
        .expect("a categorized reality row exists")
}

#[test]
#[ignore = "perf: run with --release -- --ignored"]
fn perf_single_edit_apply() {
    let mut w = world_from(perf_db());
    let acc = busiest_account(w.engine.db());
    let tx = categorized_reality_row(&w, acc);
    let mut n = 0;
    check_budget("perf_single_edit_apply", 5.0, || {
        n += 1;
        let payee = format!("Edited {n}");
        timed(|| edit_transaction(&mut w.engine, tx, TxField::Payee(payee)).expect("edit applies"))
    });
}

#[test]
#[ignore = "perf: run with --release -- --ignored"]
fn perf_undo_redo_single() {
    let mut w = world_from(perf_db());
    let acc = busiest_account(w.engine.db());
    let tx = categorized_reality_row(&w, acc);
    let mut n = 0;
    check_budget("perf_undo_redo_single", 5.0, || {
        n += 1;
        edit_transaction(&mut w.engine, tx, TxField::Payee(format!("Undo {n}")))
            .expect("edit applies");
        timed(|| {
            w.engine.undo().expect("undo");
            w.engine.redo().expect("redo");
        })
    });
}

#[test]
#[ignore = "perf: run with --release -- --ignored"]
fn perf_bulk_projection() {
    let mut w = world_from(perf_db());
    let scenario = w.scenario("Scenario 00");
    let items: Vec<BudgetItemId> = w.engine.db().live_budget_items().map(|b| b.id).collect();
    check_budget("perf_bulk_projection", 200.0, || {
        let req = ProjectionRequest {
            target: ProjectionTarget::Existing(scenario),
            start: d("2021-03-01"),
            end: TEST_TODAY,
            items: items.clone(),
        };
        timed(|| generate_projection(&mut w.engine, req).expect("projection runs"))
    });
}

#[test]
#[ignore = "perf: run with --release -- --ignored"]
fn perf_search_payee() {
    let w = world_from(perf_db());
    let db = w.engine.db();
    let search = SearchState {
        open: true,
        text: "ab".into(),
        scope: SearchScope::Payee,
        ..SearchState::new(TEST_TODAY)
    };
    check_budget("perf_search_payee", 30.0, || {
        timed(|| {
            let all: Vec<&Transaction> = db.live_transactions().collect();
            filter_transactions(db, &all, TimePeriod::All, &search, TEST_TODAY)
        })
    });
}

#[test]
#[ignore = "perf: run with --release -- --ignored"]
fn perf_markdown_export() {
    let w = world_from(perf_db());
    check_budget("perf_markdown_export", 1000.0, || {
        timed(|| export_markdown(&w.engine))
    });
}

#[test]
#[ignore = "perf: run with --release -- --ignored"]
fn perf_markdown_import() {
    let mut w = world_from(perf_db());
    let text = export_markdown(&w.engine);
    check_budget("perf_markdown_import", 3000.0, || {
        timed(|| {
            import_markdown_replacing(&mut w.engine, &text, &mut |_: &str| {})
                .expect("import the export")
        })
    });
}

#[test]
#[ignore = "perf: run with --release -- --ignored"]
fn perf_api_transactions() {
    let w = world_from(perf_db());
    let db = w.engine.db();
    let acc = busiest_account(db);
    check_budget("perf_api_transactions", 15.0, || {
        timed(|| transaction_lines(db, acc, None, 10))
    });
}
