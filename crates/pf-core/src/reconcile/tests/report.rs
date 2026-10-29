//! R-REC-7, R-REC-13: reading choice and balance checks.

use pf_model::testing::{TestDb, m};
use pf_model::{AccountType, TransactionStatus};

use super::{line, statement};
use crate::reconcile::{
    Choice, Reading, ReconcileInput, Statement, difference, reconcile, reconciled_balance,
};

fn input<'a>(
    db: &'a pf_model::Db,
    a: pf_model::AccountId,
    st: &'a Statement,
    p: Option<Reading>,
) -> ReconcileInput<'a> {
    ReconcileInput {
        db,
        account: a,
        statement: st,
        preferred: p,
    }
}

/// A one-line statement that adds up both ways; PFRust has -30 or +30 depending on the case.
fn tie_case(tx_amount: &str, t: AccountType) -> (pf_model::Db, pf_model::AccountId) {
    let mut db = TestDb::new();
    let a = db.account("A", t);
    db.tx(a, "2026-02-10", tx_amount, "X").done();
    (db.build(), a)
}

/// Covers: R-REC-7
#[test]
fn r_rec_7_reading_by_match_count() {
    // Statement prints a charge as +30; PFRust has -30 => the flipped reading matches.
    let st = statement(
        "2026-02-01",
        "2026-02-28",
        "100",
        "130",
        vec![line("2026-02-10", "X", "30")],
    );
    let (db, a) = tie_case("-30", AccountType::Checking);
    let r = reconcile(&input(&db, a, &st, None)).expect("a value");
    assert_eq!(
        (r.reading, r.matches_as_printed, r.matches_flipped),
        (Reading::Flipped, 0, 1)
    );
    let r = reconcile(&input(&db, a, &st, Some(Reading::AsPrinted))).expect("a value");
    assert_eq!(r.reading, Reading::Flipped, "match count beats preferred");

    // Tie (no matches either way): preferred, then account type.
    let none = statement(
        "2026-02-01",
        "2026-02-28",
        "100",
        "130",
        vec![line("2026-02-10", "X", "30")],
    );
    let (db, a) = tie_case("-99", AccountType::Checking);
    assert_eq!(
        reconcile(&input(&db, a, &none, None))
            .expect("a value")
            .reading,
        Reading::AsPrinted
    );
    assert_eq!(
        reconcile(&input(&db, a, &none, Some(Reading::Flipped)))
            .expect("a value")
            .reading,
        Reading::Flipped
    );
    let (db, a) = tie_case("-99", AccountType::Credit);
    // As printed the statement is asset style; a card fits owed, which is the flipped reading.
    let r = reconcile(&input(&db, a, &none, None)).expect("a value");
    assert_eq!(
        (r.reading, r.style),
        (Reading::Flipped, crate::reconcile::BalanceStyle::Owed)
    );
}

/// Covers: R-REC-13
#[test]
fn r_rec_13_balance_checks() {
    let mut t = TestDb::new();
    let a = t.account("Chk", AccountType::Checking);
    let b = t.account("Other", AccountType::Checking);
    t.tx(a, "2026-01-20", "1000", "Old")
        .status(TransactionStatus::Reconciled)
        .done();
    t.tx(b, "2026-01-20", "55", "Elsewhere")
        .status(TransactionStatus::Reconciled)
        .done();
    let p1 = t.tx(a, "2026-02-03", "-5", "Coffee").done();
    t.tx(a, "2026-02-05", "-7", "Lunch").done();
    let db = t.build();
    assert_eq!(reconciled_balance(&db, a), m("1000"));
    let st = statement(
        "2026-02-01",
        "2026-02-28",
        "1000",
        "980",
        vec![
            line("2026-02-03", "COFFEE", "-5"),
            line("2026-02-05", "LUNCH", "-7"),
            line("2026-02-27", "FEE", "-8"),
        ],
    );
    let r = reconcile(&input(&db, a, &st, None)).expect("a value");
    assert_eq!(r.reconciled_balance, m("1000"));
    assert_eq!(r.opening_difference, m("0"));
    assert!(!r.first_time);
    let mut c = Choice::default_for(&r);
    assert_eq!(c.pairs.len(), 2);
    assert_eq!(c.add_lines, [2]);
    assert_eq!(difference(&r, &c), m("0"));
    c.pairs.retain(|p| p.1 != p1);
    assert_eq!(difference(&r, &c), m("-5"));
    c.adjustment = Some(m("-5"));
    assert_eq!(difference(&r, &c), m("0"));
    // Use the statement amount for a pair whose PFRust amount differs.
    assert_eq!(
        reconcile(&input(&db, b, &st, None))
            .expect("a value")
            .opening_difference,
        m("945")
    );
    assert!(
        !reconcile(&input(&db, b, &st, None))
            .expect("a value")
            .first_time
    );

    let fresh_db = {
        let mut t = TestDb::new();
        let a = t.account("New", AccountType::Checking);
        t.tx(a, "2026-02-03", "-5", "Coffee").done();
        (t.build(), a)
    };
    let r = reconcile(&input(&fresh_db.0, fresh_db.1, &st, None)).expect("a value");
    assert!(r.first_time);
    assert_eq!(r.opening_difference, m("1000"));
}

/// Covers: R-REC-13
#[test]
fn r_rec_13_unknown_account_and_bad_statement() {
    let db = TestDb::new().build();
    let st = statement("2026-02-01", "2026-02-28", "0", "0", vec![]);
    assert_eq!(
        reconcile(&input(&db, 99.into(), &st, None)).expect_err("an error"),
        "Account not found."
    );
}

/// Covers: R-REC-7
#[test]
fn r_rec_7_forced_reading_overrides_counts() {
    let st = statement(
        "2026-02-01",
        "2026-02-28",
        "100",
        "130",
        vec![line("2026-02-10", "X", "30")],
    );
    let (db, a) = tie_case("-30", AccountType::Checking);
    let forced =
        crate::reconcile::reconcile_with(&input(&db, a, &st, None), Some(Reading::AsPrinted))
            .expect("report");
    assert_eq!(forced.reading, Reading::AsPrinted);
    assert_eq!(forced.matches_flipped, 1);
}
