//! R-REC-8 … R-REC-10, R-REC-12: candidates, passes and sections.

use pf_model::testing::{TestDb, d, m};
use pf_model::{AccountId, AccountType, TransactionStatus};
use proptest::prelude::*;

use super::{line, statement};
use crate::reconcile::{Confidence, Statement, candidates, run_matching};

fn run(db: &pf_model::Db, acct: AccountId, st: &Statement) -> crate::reconcile::Matching {
    let amounts: Vec<_> = st.lines.iter().map(|l| l.amount).collect();
    run_matching(db, acct, st, &amounts, &[])
}

fn feb(lines: Vec<crate::reconcile::StatementLine>) -> Statement {
    statement("2026-02-01", "2026-02-28", "0", "0", lines)
}

/// Covers: R-REC-8
#[test]
fn r_rec_8_candidates_window() {
    let mut t = TestDb::new();
    let a = t.account("Chk", AccountType::Checking);
    let other = t.account("Other", AccountType::Checking);
    let scen = t.scenario("S");
    let ins = ["2026-01-25", "2026-02-15", "2026-03-07"];
    for date in ins {
        t.tx(a, date, "-1", "In").done();
    }
    for date in ["2026-01-24", "2026-03-08"] {
        t.tx(a, date, "-1", "Out").done();
    }
    t.tx(a, "2026-02-10", "-1", "Scenario")
        .scenario(scen)
        .done();
    t.tx(a, "2026-02-10", "-1", "Deleted").deleted().done();
    t.tx(other, "2026-02-10", "-1", "Other").done();
    t.tx(a, "2026-02-10", "-1", "Rec")
        .status(TransactionStatus::Reconciled)
        .done();
    let db = t.build();
    let (open, done) = candidates(&db, a, &feb(vec![]));
    let mut payees: Vec<_> = open.iter().map(|t| t.payee.as_str()).collect();
    payees.sort();
    assert_eq!(payees, ["In", "In", "In"]);
    assert_eq!(done.len(), 1);
}

/// Covers: R-REC-9
#[test]
fn r_rec_9_exact_amount_passes() {
    let mut t = TestDb::new();
    let a = t.account("Chk", AccountType::Checking);
    let near = t.tx(a, "2026-02-05", "-10", "A").done();
    let mid = t.tx(a, "2026-02-17", "-20", "B").done();
    let far = t.tx(a, "2026-02-28", "-30", "C").done();
    let post = t.tx(a, "2026-02-21", "-40", "D").done();
    let db = t.build();
    let mut with_post = line("2026-02-10", "D", "-40");
    with_post.post_date = Some(d("2026-02-20"));
    let st = feb(vec![
        line("2026-02-08", "A", "-10"),
        line("2026-02-10", "B", "-20"),
        line("2026-02-10", "C", "-30"),
        with_post,
    ]);
    let r = run(&db, a, &st);
    let find = |id| r.matched.iter().find(|p| p.transaction == id);
    assert_eq!(find(near).expect("a value").confidence, Confidence::High);
    assert_eq!(find(near).expect("a value").day_distance, 3);
    assert_eq!(find(mid).expect("a value").confidence, Confidence::Medium);
    assert!(find(far).is_none());
    assert_eq!(find(post).expect("a value").day_distance, 1);
    assert!(r.statement_only.contains(&2));
}

/// Covers: R-REC-9
#[test]
fn r_rec_9_day_distance_across_month_and_year_end() {
    let mut t = TestDb::new();
    let a = t.account("Chk", AccountType::Checking);
    let id = t.tx(a, "2026-01-02", "-5", "A").done();
    let db = t.build();
    let st = statement(
        "2025-12-01",
        "2025-12-31",
        "0",
        "0",
        vec![line("2025-12-31", "A", "-5")],
    );
    let r = run(&db, a, &st);
    assert_eq!(r.matched[0].transaction, id);
    assert_eq!(r.matched[0].day_distance, 2);
}

/// Covers: R-REC-9
#[test]
fn r_rec_9_payee_breaks_ties() {
    let mut t = TestDb::new();
    let a = t.account("Chk", AccountType::Checking);
    let shell = t.tx(a, "2026-02-10", "-20", "Shell").done();
    let cafe = t.tx(a, "2026-02-10", "-20", "Blue Bottle").done();
    let db = t.build();
    let st = feb(vec![
        line("2026-02-10", "SQ *BLUE BOTTLE 12", "-20"),
        line("2026-02-10", "SHELL OIL 55", "-20"),
    ]);
    let r = run(&db, a, &st);
    let for_line = |i| {
        r.matched
            .iter()
            .find(|p| p.line == i)
            .expect("a value")
            .transaction
    };
    assert_eq!(for_line(0), cafe);
    assert_eq!(for_line(1), shell);
}

proptest! {
    /// Covers: R-REC-9
    #[test]
    fn r_rec_9_same_day_order_does_not_matter(
        amounts in proptest::collection::vec(1u32..4, 1..6),
        seed in 0u64..1000,
    ) {
        let payees = ["Alpha", "Bravo", "Charlie"];
        let mk = |order: &[usize]| {
            let mut t = TestDb::new();
            let a = t.account("Chk", AccountType::Checking);
            for &i in order {
                t.tx(a, "2026-02-10", &format!("-{}", amounts[i]), payees[amounts[i] as usize - 1]).done();
            }
            (t.build(), a)
        };
        let n = amounts.len();
        let straight: Vec<usize> = (0..n).collect();
        let mut shuffled = straight.clone();
        let mut s = seed;
        for i in (1..n).rev() {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            shuffled.swap(i, (s >> 33) as usize % (i + 1));
        }
        let lines = |order: &[usize]| feb(order.iter().map(|&i| {
            line("2026-02-10", payees[amounts[i] as usize - 1], &format!("-{}", amounts[i]))
        }).collect());
        let summary = |order: &[usize], tx_order: &[usize]| {
            let (db, a) = mk(tx_order);
            let st = lines(order);
            let r = run(&db, a, &st);
            let mut v: Vec<_> = r.matched.iter().map(|p| {
                let tx = db.transaction(p.transaction).expect("a value");
                (st.lines[p.line].amount, tx.payee.clone(), tx.date)
            }).collect();
            v.sort();
            (v, r.statement_only.len(), r.pf_only_in_period.len())
        };
        prop_assert_eq!(summary(&straight, &straight), summary(&shuffled, &shuffled));
    }
}

/// Covers: R-REC-10
#[test]
fn r_rec_10_amount_differs() {
    let mut t = TestDb::new();
    let a = t.account("Chk", AccountType::Checking);
    let tip = t.tx(a, "2026-02-10", "-41.17", "Pizza Place").done();
    t.tx(a, "2026-02-12", "-100", "Hardware").done(); // 25%+ off
    t.tx(a, "2026-02-14", "50", "Refund Shop").done(); // other sign
    t.tx(a, "2026-02-16", "-20", "Unrelated").done(); // score < 0.5
    let db = t.build();
    let st = feb(vec![
        line("2026-02-10", "PIZZA PLACE", "-43.17"),
        line("2026-02-12", "HARDWARE", "-140"),
        line("2026-02-14", "REFUND SHOP", "-52"),
        line("2026-02-16", "SOMETHING ELSE", "-21"),
    ]);
    let r = run(&db, a, &st);
    assert_eq!(r.amount_differs.len(), 1);
    assert_eq!(r.amount_differs[0].transaction, tip);
    assert_eq!(r.amount_differs[0].confidence, Confidence::AmountDiffers);
    assert!(r.matched.is_empty());
}

/// Covers: R-REC-12
#[test]
fn r_rec_12_sections() {
    let mut t = TestDb::new();
    let a = t.account("Chk", AccountType::Checking);
    let pair = t.tx(a, "2026-02-03", "-5", "Coffee").done();
    let tip = t.tx(a, "2026-02-05", "-40", "Pizza").done();
    let rec = t
        .tx(a, "2026-02-07", "-9", "Old")
        .status(TransactionStatus::Reconciled)
        .done();
    let inside = t.tx(a, "2026-02-20", "-3", "Extra").done();
    let outside = t.tx(a, "2026-03-03", "-8", "Next month").done();
    let db = t.build();
    let st = feb(vec![
        line("2026-02-28", "FEE", "-12"),
        line("2026-02-03", "COFFEE", "-5"),
        line("2026-02-05", "PIZZA", "-42"),
        line("2026-02-07", "OLD", "-9"),
    ]);
    let r = run(&db, a, &st);
    assert_eq!(
        r.matched.iter().map(|p| p.transaction).collect::<Vec<_>>(),
        [pair]
    );
    assert_eq!(
        r.amount_differs
            .iter()
            .map(|p| p.transaction)
            .collect::<Vec<_>>(),
        [tip]
    );
    assert_eq!(r.already_reconciled, [(3, rec)]);
    assert_eq!(r.statement_only, [0]);
    assert_eq!(r.pf_only_in_period, [inside]);
    assert_eq!(r.pf_only_outside, [outside]);
    let _ = m("0");
}
