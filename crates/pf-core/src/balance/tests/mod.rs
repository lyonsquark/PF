//! Unit tests for `balance` (and the `balance`-side use of `filter` and `obligation`).

use super::*;
use crate::filter::{SearchState, account_transactions, account_view};
use pf_model::testing::{TestDb, d};
use proptest::prelude::*;
use rust_decimal_macros::dec;

#[test]
fn r_scn_1_reality_sees_only_reality() {
    // Covers: R-SCN-1
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let s = t.scenario("Plan A");
    let real = t.tx(chk, "2026-01-05", "10", "real").done();
    let scen = t.tx(chk, "2026-01-06", "20", "scen").scenario(s).done();
    let gone = t.tx(chk, "2026-01-07", "30", "gone").deleted().done();
    let db = t.build();
    let tx = |id| db.transaction(id).expect("inserted");
    assert!(is_visible(tx(real), None));
    assert!(!is_visible(tx(scen), None));
    assert!(!is_visible(tx(gone), None));
}

#[test]
fn r_scn_1_scenario_sees_reality_plus_own() {
    // Covers: R-SCN-1
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let s = t.scenario("Plan A");
    let other = t.scenario("Plan B");
    let real = t.tx(chk, "2026-01-05", "10", "real").done();
    let mine = t.tx(chk, "2026-01-06", "20", "mine").scenario(s).done();
    let theirs = t
        .tx(chk, "2026-01-07", "30", "theirs")
        .scenario(other)
        .done();
    let db = t.build();
    let tx = |id| db.transaction(id).expect("inserted");
    assert!(is_visible(tx(real), Some(s)));
    assert!(is_visible(tx(mine), Some(s)));
    assert!(!is_visible(tx(theirs), Some(s)));
}

#[test]
fn r_scn_2_toggle_same_turns_off() {
    // Covers: R-SCN-2
    let mut t = TestDb::new();
    let s = t.scenario("Plan A");
    let db = t.build();
    assert!(db.scenario(s).is_some());
    assert_eq!(toggle_scenario(Some(s), Some(s)), None);
}

#[test]
fn r_scn_2_toggle_other_switches() {
    // Covers: R-SCN-2
    let mut t = TestDb::new();
    let s = t.scenario("Plan A");
    let other = t.scenario("Plan B");
    assert_eq!(toggle_scenario(Some(s), Some(other)), Some(other));
    assert_eq!(toggle_scenario(None, Some(s)), Some(s));
    assert_eq!(toggle_scenario(Some(s), None), None);
}

#[test]
fn r_bal_1_balance_sums_visible_all_statuses() {
    // Covers: R-BAL-1
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let s = t.scenario("Plan A");
    t.tx(chk, "2026-01-01", "100", "a")
        .status(TransactionStatus::Pending)
        .done();
    t.tx(chk, "2026-01-02", "-30", "b")
        .status(TransactionStatus::Cleared)
        .done();
    t.tx(chk, "2026-01-03", "-5", "c")
        .status(TransactionStatus::Reconciled)
        .done();
    t.tx(chk, "2026-02-01", "-50", "d")
        .status(TransactionStatus::Future)
        .done();
    t.tx(chk, "2026-03-01", "-200", "e")
        .scenario(s)
        .status(TransactionStatus::Future)
        .done();
    let db = t.build();
    assert_eq!(account_balance(&db, chk, None), dec!(15));
    assert_eq!(account_balance(&db, chk, Some(s)), dec!(-185));
}

#[test]
fn r_bal_1_balance_excludes_other_accounts_and_deleted() {
    // Covers: R-BAL-1
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let sav = t.account("Savings", AccountType::Savings);
    t.tx(chk, "2026-01-01", "10", "keep").done();
    t.tx(chk, "2026-01-02", "99", "deleted").deleted().done();
    t.tx(sav, "2026-01-03", "500", "other account").done();
    t.orphan_tx("2026-01-04", "1000", "orphan").done();
    let db = t.build();
    assert_eq!(account_balance(&db, chk, None), dec!(10));
}

#[test]
fn r_bal_2_cleared_balance_only_cleared_reconciled() {
    // Covers: R-BAL-2
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let s = t.scenario("Plan A");
    t.tx(chk, "2026-01-01", "-100", "pending")
        .status(TransactionStatus::Pending)
        .done();
    t.tx(chk, "2026-01-02", "20", "cleared")
        .status(TransactionStatus::Cleared)
        .done();
    t.tx(chk, "2026-01-03", "5", "reconciled")
        .status(TransactionStatus::Reconciled)
        .done();
    t.tx(chk, "2026-01-04", "-40", "future")
        .status(TransactionStatus::Future)
        .done();
    t.tx(chk, "2026-03-01", "7", "scen")
        .scenario(s)
        .status(TransactionStatus::Cleared)
        .done();
    let db = t.build();
    assert_eq!(account_cleared_balance(&db, chk, None), dec!(25));
    assert_eq!(account_cleared_balance(&db, chk, Some(s)), dec!(32));
}

#[test]
fn r_bal_3_reality_maps_ignore_active_scenario() {
    // Covers: R-BAL-3
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let s = t.scenario("Plan A");
    t.tx(chk, "2026-01-01", "10", "real").done();
    t.tx(chk, "2026-03-01", "4", "scen").scenario(s).done();
    let db = t.build();
    let maps = balance_maps(&db, Some(s));
    assert_eq!(maps.all[&chk], dec!(14));
    assert_eq!(maps.cleared[&chk], dec!(14));
    assert_eq!(maps.reality_all[&chk], dec!(10));
    assert_eq!(maps.reality_cleared[&chk], dec!(10));
}

#[test]
fn r_bal_4_orphans_contribute_nothing() {
    // Covers: R-BAL-4
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    t.orphan_tx("2026-01-01", "500", "orphan").done();
    t.tx(chk, "2026-01-02", "1", "real").done();
    let db = t.build();
    let maps = balance_maps(&db, None);
    assert_eq!(maps.all.len(), 1);
    assert_eq!(maps.cleared.len(), 1);
    assert_eq!(maps.reality_all.len(), 1);
    assert_eq!(maps.reality_cleared.len(), 1);
    assert_eq!(maps.all[&chk], dec!(1));
}

/// One cleared reality transaction per account type: assets 1310, credit -300, loan -5000.
fn typed_accounts(t: &mut TestDb) {
    let specs: [(&str, AccountType, &str); 6] = [
        ("Chk", AccountType::Checking, "100"),
        ("Sav", AccountType::Savings, "200"),
        ("Cash", AccountType::Cash, "10"),
        ("Inv", AccountType::Investment, "1000"),
        ("Card", AccountType::Credit, "-300"),
        ("Loan", AccountType::Loan, "-5000"),
    ];
    for (name, kind, amount) in specs {
        let acc = t.account(name, kind);
        t.tx(acc, "2026-01-01", amount, name)
            .status(TransactionStatus::Cleared)
            .done();
    }
}

#[test]
fn r_bal_5_totals_by_account_type() {
    // Covers: R-BAL-5
    let mut t = TestDb::new();
    typed_accounts(&mut t);
    let db = t.build();
    let maps = balance_maps(&db, None);
    let (cleared, _) = displayed_balances(&db, &maps, false);
    let totals = dashboard_totals(&db, &cleared);
    assert_eq!(totals.assets, dec!(1310));
    assert_eq!(totals.credit_debt, dec!(-300));
    assert_eq!(totals.loan_debt, dec!(-5000));
    assert_eq!(totals.net_worth, dec!(-3990));
}

#[test]
fn r_bal_5_external_accounts_excluded() {
    // Covers: R-BAL-5
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let ext = t.external_account("External checking", AccountType::Checking);
    let ext_card = t.external_account("External card", AccountType::Credit);
    t.tx(chk, "2026-01-01", "100", "a").done();
    t.tx(ext, "2026-01-01", "500", "b").done();
    t.tx(ext_card, "2026-01-01", "-40", "c").done();
    let db = t.build();
    let maps = balance_maps(&db, None);
    let (cleared, _) = displayed_balances(&db, &maps, false);
    let totals = dashboard_totals(&db, &cleared);
    assert_eq!(totals.assets, dec!(100));
    assert_eq!(totals.credit_debt, dec!(0));
    assert_eq!(totals.net_worth, dec!(100));
}

#[test]
fn r_bal_6_available_subtracts_reserved() {
    // Covers: R-BAL-6
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let ctx = t.context("Car");
    t.obligation("Car")
        .account(chk)
        .context(ctx)
        .balance("300")
        .done();
    t.tx(chk, "2026-01-01", "1000", "pay")
        .status(TransactionStatus::Cleared)
        .done();
    let db = t.build();
    let maps = balance_maps(&db, None);
    let (cleared, all) = displayed_balances(&db, &maps, true);
    assert_eq!(cleared[&chk], dec!(700));
    assert_eq!(all[&chk], dec!(700));
}

#[test]
fn r_bal_6_available_off_is_identity() {
    // Covers: R-BAL-6
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let ctx = t.context("Car");
    t.obligation("Car")
        .account(chk)
        .context(ctx)
        .balance("300")
        .done();
    t.tx(chk, "2026-01-01", "1000", "pay").done();
    let db = t.build();
    let maps = balance_maps(&db, None);
    let (cleared, all) = displayed_balances(&db, &maps, false);
    assert_eq!(cleared, maps.cleared);
    assert_eq!(all, maps.all);
}

#[test]
fn r_bal_6_reserved_uses_reality() {
    // Covers: R-BAL-6
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let s = t.scenario("Plan A");
    let ctx = t.context("Car");
    t.obligation("Car")
        .account(chk)
        .context(ctx)
        .balance("300")
        .done();
    t.tx(chk, "2026-01-01", "1000", "pay").done();
    t.tx(chk, "2026-01-02", "-50", "ctx real")
        .contexts(&[ctx])
        .done();
    t.tx(chk, "2026-02-01", "2000", "scen ctx")
        .contexts(&[ctx])
        .scenario(s)
        .done();
    let db = t.build();
    assert_eq!(obligation::total_reserved(&db, chk, None), dec!(250));
    let maps = balance_maps(&db, Some(s));
    assert_eq!(maps.all[&chk], dec!(2950));
    let (_, all) = displayed_balances(&db, &maps, true);
    assert_eq!(all[&chk], dec!(2700));
}

#[test]
fn r_bal_7_running_balance_oldest_to_newest() {
    // Covers: R-BAL-7
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let a = t.tx(chk, "2026-01-01", "10", "a").done();
    let b = t.tx(chk, "2026-01-05", "5", "b").done();
    let c = t.tx(chk, "2026-01-10", "-3", "c").done();
    let db = t.build();
    let txs = account_transactions(&db, chk, None);
    let rb = running_balances(&txs);
    assert_eq!(rb[&a], dec!(10));
    assert_eq!(rb[&b], dec!(15));
    assert_eq!(rb[&c], dec!(12));
}

#[test]
fn r_bal_7_running_balance_same_date_by_id() {
    // Covers: R-BAL-7
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let a = t.tx(chk, "2026-01-05", "1", "a").done();
    let b = t.tx(chk, "2026-01-05", "2", "b").done();
    let db = t.build();
    // Newest first, as the list passes them: b (higher id) before a.
    let txs = vec![
        db.transaction(b).expect("inserted"),
        db.transaction(a).expect("inserted"),
    ];
    let rb = running_balances(&txs);
    assert_eq!(rb[&a], dec!(1));
    assert_eq!(rb[&b], dec!(3));
}

#[test]
fn r_bal_7_running_balance_independent_of_window() {
    // Covers: R-BAL-7
    let today = d("2026-03-15");
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let old = t.tx(chk, "2025-06-01", "10", "old").done();
    let recent = t.tx(chk, "2026-03-01", "5", "recent").done();
    let db = t.build();
    let search = SearchState::new(today);
    let month = account_view(&db, chk, None, TimePeriod::PastMonth, &search, today);
    let all = account_view(&db, chk, None, TimePeriod::All, &search, today);
    assert!(
        !month.rows.contains(&old),
        "old row is outside the month window"
    );
    assert_eq!(month.running[&recent], dec!(15));
    assert_eq!(month.running[&recent], all.running[&recent]);
    assert_eq!(month.running[&old], all.running[&old]);
}

#[test]
fn r_bal_7_running_balance_per_account() {
    // Covers: R-BAL-7
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let sav = t.account("Savings", AccountType::Savings);
    let a = t.tx(chk, "2026-01-01", "10", "a").done();
    let b = t.tx(sav, "2026-01-02", "100", "b").done();
    let c = t.tx(chk, "2026-01-03", "5", "c").done();
    let db = t.build();
    let all: Vec<&Transaction> = db.live_transactions().collect();
    let rb = running_balances(&all);
    assert_eq!(rb[&a], dec!(10));
    assert_eq!(rb[&b], dec!(100));
    assert_eq!(rb[&c], dec!(15));
}

#[test]
fn r_bal_8_search_running_only_filtered() {
    // Covers: R-BAL-8
    let today = d("2026-03-15");
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let c1 = t.tx(chk, "2026-01-01", "-4", "Coffee shop").done();
    let rent = t.tx(chk, "2026-01-02", "-20", "Rent").done();
    let c2 = t.tx(chk, "2026-01-03", "-6", "Coffee beans").done();
    let db = t.build();
    let mut search = SearchState::new(today);
    search.open = true;
    search.text = "coffee".to_string();
    let view = account_view(&db, chk, None, TimePeriod::All, &search, today);
    let srb = view.search_running.expect("search is active");
    assert_eq!(srb[&c1], dec!(-4));
    assert_eq!(srb[&c2], dec!(-10));
    assert!(!srb.contains_key(&rent));

    let inactive = SearchState::new(today);
    let view = account_view(&db, chk, None, TimePeriod::All, &inactive, today);
    assert!(view.search_running.is_none());
}

#[test]
fn r_bal_9_groups_order_and_membership() {
    // Covers: R-BAL-9
    let mut t = TestDb::new();
    let zebra = t.account("Zebra", AccountType::Checking);
    let alpha = t.account("Alpha", AccountType::Checking);
    let wallet = t.account("Wallet", AccountType::Cash);
    let joint = t.account("Joint Savings", AccountType::Savings);
    t.account("Visa", AccountType::Credit);
    t.account("Brokerage", AccountType::Investment);
    let db = t.build();
    let groups = dashboard_groups(&db);
    let titles: Vec<&str> = groups.iter().map(|g| g.title).collect();
    assert_eq!(
        titles,
        vec!["Cash & Checking", "Savings", "Credit Cards", "Investments"]
    );
    assert_eq!(groups[0].accounts, vec![alpha, wallet, zebra]);
    assert_eq!(groups[1].accounts, vec![joint]);
}

proptest! {
    /// Covers: R-BAL-1
    #[test]
    fn prop_balances_sum_matches(
        rows in proptest::collection::vec(
            (0usize..3, -100_000i64..100_000, 0usize..4, any::<bool>(), any::<bool>(), any::<bool>()),
            0..60,
        )
    ) {
        let mut t = TestDb::new();
        let accs = [
            t.account("A", AccountType::Checking),
            t.account("B", AccountType::Savings),
            t.account("C", AccountType::Cash),
        ];
        let s = t.scenario("S");
        let mut expected = Decimal::ZERO;
        for (ai, cents, st, in_scenario, deleted, orphan) in rows {
            let amount = Decimal::new(cents, 2).to_string();
            let status = TransactionStatus::ALL[st];
            let builder = if orphan {
                t.orphan_tx("2026-01-01", &amount, "p")
            } else {
                t.tx(accs[ai], "2026-01-01", &amount, "p")
            };
            let builder = builder.status(status);
            let builder = if in_scenario { builder.scenario(s) } else { builder };
            let builder = if deleted { builder.deleted() } else { builder };
            builder.done();
            // Visible under Reality: live, not scenario, with an account.
            if !orphan && !deleted && !in_scenario {
                expected += Decimal::new(cents, 2);
            }
        }
        let db = t.build();
        let maps = balance_maps(&db, None);
        let sum: Decimal = accs.iter().map(|a| maps.all.get(a).copied().unwrap_or(Decimal::ZERO)).sum();
        prop_assert_eq!(sum, expected);
        let per_account: Decimal = accs.iter().map(|a| account_balance(&db, *a, None)).sum();
        prop_assert_eq!(per_account, expected);
    }
}
