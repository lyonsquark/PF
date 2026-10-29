//! Golden snapshots of whole reconciliations over `fixtures/statements/`.

use pf_model::testing::TestDb;
use pf_model::{AccountType, TransactionStatus};

use super::fixture;
use crate::reconcile::{ReconcileInput, parse_statement_json, reconcile};

fn report(file: &str, db: &pf_model::Db, a: pf_model::AccountId) -> String {
    let st = parse_statement_json(&fixture(file)).expect("a value");
    let r = reconcile(&ReconcileInput {
        db,
        account: a,
        statement: &st,
        preferred: None,
    })
    .expect("a value");
    format!("{r:#?}")
}

/// Covers: R-REC-3, R-REC-9, R-REC-12
#[test]
fn golden_checking_statement() {
    let mut t = TestDb::new();
    let a = t.account("Checking", AccountType::Checking);
    t.tx(a, "2026-01-31", "1000", "Opening")
        .status(TransactionStatus::Reconciled)
        .done();
    t.tx(a, "2026-02-02", "2000", "Acme Payroll").done();
    t.tx(a, "2026-02-03", "-6.50", "Blue Bottle Coffee").done();
    t.tx(a, "2026-02-06", "-84.20", "Trader Joe's").done();
    t.tx(a, "2026-02-12", "-23.99", "Amazon").done();
    t.tx(a, "2026-02-15", "-4.00", "Vending").done();
    t.tx(a, "2026-03-03", "-500", "Rent").done();
    insta::assert_snapshot!(report("checking.json", &t.build(), a));
}

/// Covers: R-REC-5, R-REC-6, R-REC-7
#[test]
fn golden_card_separate_tables() {
    let mut t = TestDb::new();
    let a = t.account("Visa", AccountType::Credit);
    t.tx(a, "2026-02-05", "500", "Payment").done();
    t.tx(a, "2026-02-10", "-60", "Blue Bottle").done();
    t.tx(a, "2026-02-21", "-40", "Shell").done();
    insta::assert_snapshot!(report("card-separate.json", &t.build(), a));
}

/// Covers: R-REC-7, R-REC-10
#[test]
fn golden_card_single_table() {
    let mut t = TestDb::new();
    let a = t.account("Mastercard", AccountType::Credit);
    t.tx(a, "2026-02-06", "-65.50", "Joe's Pizza").done();
    t.tx(a, "2026-02-14", "-40", "Shell").done();
    t.tx(a, "2026-02-25", "300", "Payment").done();
    insta::assert_snapshot!(report("card-single.json", &t.build(), a));
}
