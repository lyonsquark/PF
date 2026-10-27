//! Report values on `fixtures/small.md`. The expected numbers were computed by hand from the
//! fixture rows (and cross-checked against the Swift rules), not copied from the code.

use pf_model::testing::d;
use pf_model::{Transaction, TransactionId};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use super::golden::fixture_db;
use super::*;

#[cfg(test)]
/// (label, budgeted, actual) of each row, for comparing whole reports.
fn triples(rows: &[PlRow]) -> Vec<(&str, Decimal, Decimal)> {
    rows.iter()
        .map(|r| (r.label.as_str(), r.budgeted, r.actual))
        .collect()
}

#[test]
fn small_pnl_q1_reality() {
    // Covers: R-RPT-1, R-RPT-2, R-RPT-3
    let db = fixture_db("small.md");
    let rows = profit_and_loss(&db, d("2026-01-01"), d("2026-03-31"), None);
    assert_eq!(
        triples(&rows),
        vec![
            ("Dining", dec!(0), dec!(-94.75)),
            ("Groceries", dec!(-1950), dec!(-142.30)),
            ("Housing", dec!(-5220), dec!(-3064.99)),
            ("Housing/Utilities", dec!(-180), dec!(-208.55)),
            ("Income", dec!(12000), dec!(8000)),
            ("Uncategorized", dec!(0), dec!(2050)),
            ("Total", dec!(4650), dec!(6539.41)),
        ]
    );
    assert_eq!(rows.last().expect("total").difference(), dec!(1889.41));
}

#[test]
fn small_pnl_q1_plan_a() {
    // Covers: R-RPT-2
    let db = fixture_db("small.md");
    let plan_a = db.scenario_by_name("Plan A").expect("Plan A").id;
    let rows = profit_and_loss(&db, d("2026-01-01"), d("2026-03-31"), Some(plan_a));
    assert_eq!(
        triples(&rows),
        vec![
            ("Dining", dec!(0), dec!(-94.75)),
            ("Groceries", dec!(-1950), dec!(-142.30)),
            ("Housing", dec!(-5220), dec!(-3064.99)),
            ("Housing/Utilities", dec!(-180), dec!(-208.55)),
            ("Income", dec!(12000), dec!(12000)),
            ("Uncategorized", dec!(0), dec!(2050)),
            ("Total", dec!(4650), dec!(10539.41)),
        ]
    );
}

#[test]
fn r_rpt_4a_balance_sheet_accounts() {
    // Covers: R-RPT-4, R-RPT-4a
    let db = fixture_db("small.md");
    let bs = balance_sheet(&db, d("2026-03-15"), None);
    let accounts: Vec<(&str, &str, Decimal, Decimal)> = bs
        .accounts
        .iter()
        .map(|a| (a.name.as_str(), a.institution.as_str(), a.total, a.pending))
        .collect();
    assert_eq!(
        accounts,
        vec![
            ("Checking", "Chase", dec!(6476.46), dec!(-120.45)),
            ("Rewards Card", "Amex", dec!(62.95), dec!(-6.75)),
        ]
    );
}

#[test]
fn r_rpt_4b_balance_sheet_totals() {
    // Covers: R-RPT-4b
    let db = fixture_db("small.md");
    let bs = balance_sheet(&db, d("2026-03-15"), None);
    assert_eq!(bs.total, dec!(6539.41));
    assert_eq!(bs.pending_total, dec!(-127.20));
}

#[test]
fn r_rpt_4c_obligations_use_stored_balance() {
    // Covers: R-RPT-4c
    let db = fixture_db("small.md");
    let bs = balance_sheet(&db, d("2026-03-15"), None);
    let obligations: Vec<(&str, Decimal, bool)> = bs
        .obligations
        .iter()
        .map(|o| (o.name.as_str(), o.balance, o.is_funding))
        .collect();
    // Vacation Fund is tagged with "Hawaii flights" (−450), yet its stored 500 is shown.
    assert_eq!(obligations, vec![("Vacation Fund", dec!(500), true)]);
    assert_eq!(bs.total_obligations, dec!(500));
}

#[test]
fn r_rpt_4d_net_worth_and_available() {
    // Covers: R-RPT-4d
    let db = fixture_db("small.md");
    let bs = balance_sheet(&db, d("2026-03-15"), None);
    assert_eq!(bs.net_worth, dec!(6539.41));
    assert_eq!(bs.available, dec!(6039.41));
}

#[test]
fn r_rpt_4_scenario_and_future_pending() {
    // Covers: R-RPT-4a, R-SCN-1
    let db = fixture_db("small.md");
    let plan_a = db.scenario_by_name("Plan A").expect("Plan A").id;
    let bs = balance_sheet(&db, d("2026-12-31"), Some(plan_a));
    let checking = bs
        .accounts
        .iter()
        .find(|a| a.name == "Checking")
        .expect("checking");
    assert_eq!(checking.total, dec!(8976.46));
    // Future rows (Plan A, 2500 net) count as pending.
    assert_eq!(checking.pending, dec!(2379.55));
    let card = bs
        .accounts
        .iter()
        .find(|a| a.name == "Rewards Card")
        .expect("card");
    assert_eq!(card.total, dec!(-87.05));
    assert_eq!(card.pending, dec!(-156.75));
    assert_eq!(bs.total, dec!(8889.41));
}

#[test]
fn r_rpt_5a_matrix_actuals() {
    // Covers: R-RPT-5a
    let mut db = fixture_db("small.md");
    let m = monthly_matrix(&db, 2026, None);
    assert_eq!(row_of(&m, "Housing").actual[0], dec!(-1500));
    assert_eq!(row_of(&m, "Housing").actual[1], dec!(-1564.99));
    assert_eq!(row_of(&m, "Dining").actual[1], dec!(-88));
    assert_eq!(row_of(&m, "Dining").actual[2], dec!(-6.75));
    assert_eq!(row_of(&m, "Uncategorized").actual[0], dec!(2500));
    assert_eq!(row_of(&m, "Uncategorized").actual[2], dec!(-450));
    // Brokerage (external) holds a 2026-01-02 deposit: it is not in NET.
    assert_eq!(m.net_actual[0], dec!(5000));

    // A Dec 31 transaction is included.
    let checking = db.account_by_name("Checking").expect("checking").id;
    let id = TransactionId(db.alloc_id());
    let mut tx = Transaction::new(id, d("2026-12-31"), dec!(-1), "Year end");
    tx.account_id = Some(checking);
    db.transactions.insert(id, tx);
    let m = monthly_matrix(&db, 2026, None);
    assert_eq!(row_of(&m, "Uncategorized").actual[11], dec!(-1));
}

#[test]
fn r_rpt_5b_matrix_budget() {
    // Covers: R-RPT-5b
    let db = fixture_db("small.md");
    let m = monthly_matrix(&db, 2026, None);
    let housing = row_of(&m, "Housing");
    assert_eq!(housing.budget[0], dec!(-1540));
    assert_eq!(housing.budget[1], dec!(-2140));
    assert_eq!(housing.budget[6], dec!(-1500));
    assert_eq!(housing.budget[7], dec!(-2100));
    let groceries = row_of(&m, "Groceries");
    assert_eq!(groceries.budget[2], dec!(-750));
    assert_eq!(groceries.budget[3], dec!(-600));
    let utilities = row_of(&m, "Housing/Utilities");
    assert_eq!(utilities.budget[0], dec!(-90));
    assert_eq!(utilities.budget[1], dec!(0));
    let income = row_of(&m, "Income");
    assert!(income.budget.iter().all(|v| *v == dec!(4000)));
}

fn row_of(m: &MonthlyMatrix, label: &str) -> MatrixRow {
    m.rows
        .iter()
        .find(|r| r.label == label)
        .unwrap_or_else(|| panic!("row {label}"))
        .clone()
}

#[test]
fn r_rpt_5c_matrix_rows_and_net() {
    // Covers: R-RPT-5c
    let db = fixture_db("small.md");
    let m = monthly_matrix(&db, 2026, None);
    let labels: Vec<&str> = m.rows.iter().map(|r| r.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Dining",
            "Groceries",
            "Housing",
            "Housing/Utilities",
            "Income",
            "Uncategorized"
        ]
    );
    let zero = Decimal::ZERO;
    assert_eq!(
        m.net_actual,
        [
            dec!(5000),
            dec!(2258.91),
            dec!(-719.50),
            zero,
            zero,
            zero,
            zero,
            zero,
            zero,
            zero,
            zero,
            zero
        ]
    );
    assert_eq!(m.net_actual.iter().sum::<Decimal>(), dec!(6539.41));
    assert_eq!(
        m.net_budget,
        [
            dec!(1770),
            dec!(1260),
            dec!(1620),
            dec!(1860),
            dec!(1170),
            dec!(1710),
            dec!(1810),
            dec!(1150),
            dec!(1810),
            dec!(1900),
            dec!(1060),
            dec!(1900)
        ]
    );
    assert_eq!(m.net_budget.iter().sum::<Decimal>(), dec!(19020));
}

#[test]
fn r_rpt_5e_matrix_drill_and_title() {
    // Covers: R-RPT-5e
    let db = fixture_db("small.md");
    let m = monthly_matrix(&db, 2026, None);
    let housing = m
        .rows
        .iter()
        .find(|r| r.label == "Housing")
        .and_then(|r| r.category_id);
    let feb = matrix_drill(&db, 2026, None, housing, Some(2));
    let dates: Vec<Date> = feb
        .iter()
        .filter_map(|id| db.transaction(*id))
        .map(|t| t.date)
        .collect();
    assert_eq!(dates, [d("2026-02-03"), d("2026-02-28")]);

    let uncat = matrix_drill(&db, 2026, None, None, None);
    assert_eq!(uncat.len(), 2);
    assert_eq!(
        matrix_drill_title("Housing", 2026, Some(2)),
        "Housing — Feb 2026"
    );
    assert_eq!(matrix_drill_title("Housing", 2026, None), "Housing 2026");
}
