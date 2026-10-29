//! R-REC-3 … R-REC-6: self-check, totals, readings, balance signs.

use pf_model::AccountType;
use pf_model::testing::m;

use super::{line, statement};
use crate::reconcile::{
    BalanceStyle::{Asset, Owed},
    Reading::{AsPrinted, Flipped},
    check_totals, pf_balance, self_check, self_check_for,
};

/// Covers: R-REC-3
#[test]
fn r_rec_3_asset_and_owed_styles() {
    let checking = statement(
        "2026-02-01",
        "2026-02-28",
        "100",
        "70",
        vec![line("2026-02-02", "A", "-30")],
    );
    assert_eq!(
        self_check(&checking).expect("a value")[0],
        (AsPrinted, Asset)
    );
    let card = statement(
        "2026-02-01",
        "2026-02-28",
        "100",
        "130",
        vec![line("2026-02-02", "A", "-30")],
    );
    assert_eq!(self_check(&card).expect("a value")[0], (AsPrinted, Owed));
}

/// Covers: R-REC-3
#[test]
fn r_rec_3_does_not_add_up_with_hint() {
    let mut a = line("2026-02-02", "A", "-30");
    a.section = "Purchases".into();
    let mut b = line("2026-02-03", "B", "50");
    b.section = "Credits".into();
    let mut f = line("2026-02-04", "C", "-5");
    f.section = "Fees".into();
    // Only negating Credits makes the lines sum to -85, so asset style closes at 15.
    let lines = vec![a, b, f];
    let st = statement("2026-02-01", "2026-02-28", "100", "15", lines.clone());
    assert_eq!(
        self_check(&st).expect_err("an error"),
        "Statement does not add up: opening $100.00, lines $15.00, closing $15.00. Off by $70.00. \
         Flipping the signs under 'Credits' would make it add up."
    );
    // No section helps: no hint.
    let st = statement("2026-02-01", "2026-02-28", "100", "7", lines);
    let e = self_check(&st).expect_err("an error");
    assert!(e.ends_with("Off by $78.00."), "{e}");
    // No sections at all.
    let st = statement(
        "2026-02-01",
        "2026-02-28",
        "100",
        "7",
        vec![line("2026-02-02", "A", "-30")],
    );
    assert!(!self_check(&st).expect_err("an error").contains("Flipping"));
}

/// Covers: R-REC-4
#[test]
fn r_rec_4_totals_either_order() {
    let mut st = statement(
        "2026-02-01",
        "2026-02-28",
        "0",
        "20",
        vec![
            line("2026-02-02", "A", "50"),
            line("2026-02-03", "B", "-30"),
        ],
    );
    assert!(check_totals(&st).is_ok());
    st.total_credits = Some(m("50"));
    assert!(check_totals(&st).is_ok(), "only one total present");
    st.total_debits = Some(m("30"));
    assert!(check_totals(&st).is_ok());
    st.total_credits = Some(m("30"));
    st.total_debits = Some(m("50"));
    assert!(check_totals(&st).is_ok(), "either order");
    st.total_credits = Some(m("51"));
    assert_eq!(
        check_totals(&st).expect_err("an error"),
        "Statement totals do not match: credits $51.00 and debits $50.00 listed, lines give $50.00 and $30.00."
    );
}

/// Covers: R-REC-5
#[test]
fn r_rec_5_two_readings() {
    let st = statement(
        "2026-02-01",
        "2026-02-28",
        "100",
        "70",
        vec![line("2026-02-02", "A", "-30")],
    );
    assert_eq!(
        self_check(&st).expect("a value"),
        [(AsPrinted, Asset), (Flipped, Owed)]
    );
    let st = statement(
        "2026-02-01",
        "2026-02-28",
        "100",
        "130",
        vec![line("2026-02-02", "A", "-30")],
    );
    assert_eq!(
        self_check(&st).expect("a value"),
        [(AsPrinted, Owed), (Flipped, Asset)]
    );
    let zero = statement(
        "2026-02-01",
        "2026-02-28",
        "100",
        "100",
        vec![
            line("2026-02-02", "A", "-30"),
            line("2026-02-03", "B", "30"),
        ],
    );
    assert_eq!(
        self_check_for(&zero, AccountType::Credit).expect("a value"),
        [(AsPrinted, Owed), (Flipped, Owed)]
    );
    assert_eq!(
        self_check_for(&zero, AccountType::Savings).expect("a value"),
        [(AsPrinted, Asset), (Flipped, Asset)]
    );
    let none = statement("2026-02-01", "2026-02-28", "100", "100", vec![]);
    assert!(self_check(&none).is_ok());
}

/// Covers: R-REC-6
#[test]
fn r_rec_6_owed_balance_is_negative() {
    assert_eq!(pf_balance(m("1764.30"), Owed), m("-1764.30"));
    assert_eq!(pf_balance(m("1764.30"), Asset), m("1764.30"));
}
