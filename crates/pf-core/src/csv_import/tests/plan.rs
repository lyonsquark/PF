//! R-CSV-6 … R-CSV-14: date and amount parsing, the row-by-row import plan.

use jiff::tz::TimeZone;
use pf_model::testing::{TestDb, d};
use pf_model::{AccountId, AccountType, CategoryType, Db, RowKey};
use rust_decimal_macros::dec;

use super::{chicago, mapping_from, parsed, strings};
use crate::csv_import::{
    ColumnMapping, ContextRef, ImportField, ImportPlan, ImportRequest, ImportResult,
    detect_columns, parse_csv_amount, parse_csv_date, plan_import,
};

/// Plans an import with the test time zone.
fn run(
    db: &Db,
    rows: &[Vec<String>],
    mapping: &ColumnMapping,
    target: Option<AccountId>,
    allow_unmapped_payees: bool,
) -> ImportPlan {
    let tz: TimeZone = chicago();
    plan_import(
        db,
        &ImportRequest {
            rows,
            mapping,
            target_account: target,
            allow_unmapped_payees,
            tz: &tz,
        },
    )
}

/// A Db with one live bank account and its id.
fn bank_db() -> (Db, AccountId) {
    let mut b = TestDb::new();
    let acct = b.account("Checking", AccountType::Checking);
    (b.build(), acct)
}

#[test]
fn r_csv_6_date_formats() {
    let tz = chicago();
    assert_eq!(parse_csv_date("03/01/2026", &tz), Some(d("2026-03-01")));
    assert_eq!(parse_csv_date("3/5/2026", &tz), Some(d("2026-03-05")));
    assert_eq!(parse_csv_date("2026-03-03", &tz), Some(d("2026-03-03")));
    // 04:59 UTC is 23:59 on the previous day in Chicago (CDT).
    assert_eq!(
        parse_csv_date("2026-03-13T04:59:06Z", &tz),
        Some(d("2026-03-12"))
    );
    for bad in ["13/45/2026", "03/01/26", ""] {
        assert_eq!(parse_csv_date(bad, &tz), None, "{bad}");
    }
}

#[test]
fn r_csv_6_bad_rows_skipped_uncounted() {
    let (db, acct) = bank_db();
    let p = parsed("bank_basic.csv");
    let plan = run(&db, &p.rows, &detect_columns(&p.headers), Some(acct), true);
    assert_eq!(
        plan.result,
        ImportResult {
            imported: 4,
            skipped_duplicates: 0,
            skipped_unmapped: 0
        }
    );
}

#[test]
fn r_csv_7_amount_cleanup_and_negate() {
    assert_eq!(parse_csv_amount("$-142.30"), Some(dec!(-142.30)));
    assert_eq!(parse_csv_amount("4,000.00"), Some(dec!(4000)));
    assert_eq!(parse_csv_amount(" $1,234.5 "), Some(dec!(1234.5)));
    assert_eq!(parse_csv_amount("abc"), None);
    assert_eq!(parse_csv_amount(""), None);

    let (db, acct) = bank_db();
    let p = parsed("bank_basic.csv");
    let mut mapping = detect_columns(&p.headers);
    let plain = run(&db, &p.rows, &mapping, Some(acct), true);
    mapping.negate_amounts = true;
    let negated = run(&db, &p.rows, &mapping, Some(acct), true);
    assert_eq!(plain.transactions[0].amount, dec!(-6.75));
    assert_eq!(negated.transactions[0].amount, dec!(6.75));
    assert_eq!(plain.transactions[1].amount, dec!(4000));
    assert_eq!(negated.transactions[1].amount, dec!(-4000));
}

#[test]
fn r_csv_8_monarch_account_resolution() {
    let mut b = TestDb::new();
    let checking = b.account("Checking", AccountType::Checking);
    let amex = b.account("Amex gold", AccountType::Credit);
    b.import_mapping("account", "chase checking", "Checking", None);
    let db = b.build();
    let p = parsed("monarch_basic.csv");
    let plan = run(&db, &p.rows, &detect_columns(&p.headers), None, true);
    assert_eq!(
        plan.result,
        ImportResult {
            imported: 3,
            skipped_duplicates: 0,
            skipped_unmapped: 1
        }
    );
    assert_eq!(plan.transactions[0].account_id, checking);
    assert_eq!(plan.transactions[1].account_id, checking);
    assert_eq!(plan.transactions[2].account_id, amex);
}

#[test]
fn r_csv_8_bank_requires_target_account() {
    let p = parsed("bank_basic.csv");
    let mapping = detect_columns(&p.headers);
    let (db, _) = bank_db();
    let none = run(&db, &p.rows, &mapping, None, true);
    assert_eq!(
        none.result,
        ImportResult {
            imported: 0,
            skipped_duplicates: 0,
            skipped_unmapped: 4
        }
    );

    let mut b = TestDb::new();
    let gone = b.account("Gone", AccountType::Checking);
    b.delete(RowKey::Account(gone));
    let db = b.build();
    let deleted = run(&db, &p.rows, &mapping, Some(gone), true);
    assert_eq!(
        deleted.result,
        ImportResult {
            imported: 0,
            skipped_duplicates: 0,
            skipped_unmapped: 4
        }
    );
}

#[test]
fn r_csv_9_unmapped_payees_skipped_unless_allowed() {
    let mut b = TestDb::new();
    b.account("Checking", AccountType::Checking);
    b.account("Amex Gold", AccountType::Credit);
    b.import_mapping("account", "chase checking", "Checking", None);
    b.import_mapping("payee", "STARBUCKS STORE 123", "Starbucks", None);
    let db = b.build();
    let p = parsed("monarch_basic.csv");
    let mapping = detect_columns(&p.headers);
    let strict = run(&db, &p.rows, &mapping, None, false);
    assert_eq!(
        strict.result,
        ImportResult {
            imported: 1,
            skipped_duplicates: 0,
            skipped_unmapped: 3
        }
    );
    let open = run(&db, &p.rows, &mapping, None, true);
    assert_eq!(
        open.result,
        ImportResult {
            imported: 3,
            skipped_duplicates: 0,
            skipped_unmapped: 1
        }
    );
}

#[test]
fn r_csv_10_payee_value_monarch_and_bank() {
    let p = parsed("monarch_basic.csv");
    let mapping = detect_columns(&p.headers);

    let mut b = TestDb::new();
    b.account("Checking", AccountType::Checking);
    b.import_mapping("account", "chase checking", "Checking", None);
    b.import_mapping("payee", "STARBUCKS STORE 123", "Starbucks Coffee", None);
    let db = b.build();
    let mapped = run(&db, &p.rows, &mapping, None, true);
    assert_eq!(mapped.transactions[0].payee, "Starbucks Coffee");
    assert_eq!(mapped.transactions[1].payee, "Employer");

    let mut b = TestDb::new();
    b.account("Checking", AccountType::Checking);
    b.import_mapping("account", "chase checking", "Checking", None);
    let db = b.build();
    let unmapped = run(&db, &p.rows, &mapping, None, true);
    assert_eq!(unmapped.transactions[0].payee, "Starbucks");

    let mut b = TestDb::new();
    let acct = b.account("Checking", AccountType::Checking);
    b.import_mapping(
        "payee",
        "STARBUCKS STORE 123, SEATTLE",
        "Starbucks Seattle",
        Some(acct),
    );
    b.import_mapping("payee", "PAYROLL EMPLOYER INC", "Payroll", None);
    let db = b.build();
    let bank = parsed("bank_basic.csv");
    let plan = run(
        &db,
        &bank.rows,
        &detect_columns(&bank.headers),
        Some(acct),
        true,
    );
    assert_eq!(plan.transactions[0].payee, "Starbucks Seattle");
    assert_eq!(plan.transactions[1].payee, "PAYROLL EMPLOYER INC");
}

#[test]
fn r_csv_11_duplicates_live_only_not_in_batch() {
    let mut b = TestDb::new();
    let acct = b.account("Checking", AccountType::Checking);
    b.tx(acct, "2026-03-01", "-6.75", "starbucks store 123, seattle")
        .done();
    b.tx(acct, "2026-03-02", "4000", "PAYROLL EMPLOYER INC")
        .deleted()
        .done();
    let db = b.build();
    let p = parsed("bank_basic.csv");
    let mapping = detect_columns(&p.headers);
    let plan = run(&db, &p.rows, &mapping, Some(acct), true);
    assert_eq!(
        plan.result,
        ImportResult {
            imported: 3,
            skipped_duplicates: 1,
            skipped_unmapped: 0
        }
    );

    // Two identical rows in one batch are both imported.
    let rows = vec![
        strings(&["03/09/2026", "Coffee", "-3.00"]),
        strings(&["03/09/2026", "Coffee", "-3.00"]),
    ];
    let twins = detect_columns(&strings(&["Date", "Payee", "Amount"]));
    let plan = run(&db, &rows, &twins, Some(acct), true);
    assert_eq!(
        plan.result,
        ImportResult {
            imported: 2,
            skipped_duplicates: 0,
            skipped_unmapped: 0
        }
    );
}

#[test]
fn r_csv_12_category_mapping_and_matching() {
    let mut b = TestDb::new();
    let acct = b.account("Checking", AccountType::Checking);
    let other = b.account("Savings", AccountType::Checking);
    let dining = b.category("Dining", CategoryType::Expense);
    let groceries = b.category("Groceries", CategoryType::Expense);
    let housing = b.category("Housing", CategoryType::Expense);
    let utilities = b.child_category(housing, "Utilities");
    b.import_mapping("category", "Coffee Shops", "Dining", Some(acct));
    b.import_mapping("category", "Groceries", "Dining", Some(other));
    let db = b.build();
    let headers = strings(&["Date", "Payee", "Amount", "Category"]);
    let rows = vec![
        strings(&["03/01/2026", "x", "-1", "Coffee Shops"]),
        strings(&["03/02/2026", "x", "-2", "housing/utilities"]),
        strings(&["03/03/2026", "x", "-3", "utilities"]),
        strings(&["03/04/2026", "x", "-4", "Groceries"]),
    ];
    let plan = run(&db, &rows, &detect_columns(&headers), Some(acct), true);
    let cats: Vec<_> = plan.transactions.iter().map(|t| t.category_id).collect();
    assert_eq!(
        cats,
        vec![
            Some(dining),
            Some(utilities),
            Some(utilities),
            Some(groceries)
        ]
    );
}

#[test]
fn r_csv_13_tags_become_contexts() {
    let mut b = TestDb::new();
    let acct = b.account("Checking", AccountType::Checking);
    let vacation = b.context("Vacation");
    let db = b.build();
    let headers = strings(&["Date", "Payee", "Amount", "Tags"]);
    let rows = vec![
        strings(&["03/01/2026", "x", "-1", "vacation,work"]),
        strings(&["03/02/2026", "y", "-2", "WORK"]),
    ];
    let plan = run(&db, &rows, &detect_columns(&headers), Some(acct), true);
    assert_eq!(
        plan.transactions[0].contexts,
        vec![ContextRef::Existing(vacation), ContextRef::New(0)]
    );
    assert_eq!(plan.transactions[1].contexts, vec![ContextRef::New(0)]);
    assert_eq!(plan.new_contexts, strings(&["work"]));
}

#[test]
fn r_csv_14_plan_counts_and_defaults() {
    let mut b = TestDb::new();
    b.account("Checking", AccountType::Checking);
    b.account("Amex Gold", AccountType::Credit);
    b.import_mapping("account", "chase checking", "Checking", None);
    for (src, tgt) in [
        ("STARBUCKS STORE 123", "Starbucks"),
        ("PAYROLL EMPLOYER INC", "Payroll"),
        ("WHOLEFDS MKT 10", "Whole Foods"),
        ("SOMETHING", "Something"),
    ] {
        b.import_mapping("payee", src, tgt, None);
    }
    let db = b.build();
    let p = parsed("monarch_basic.csv");
    let plan = run(&db, &p.rows, &detect_columns(&p.headers), None, false);
    assert_eq!(
        plan.result,
        ImportResult {
            imported: 3,
            skipped_duplicates: 0,
            skipped_unmapped: 1
        }
    );
    assert_eq!(
        plan.result.summary_lines(),
        vec![
            "\u{2705} 3 transactions imported",
            "\u{26A0}\u{FE0F} 1 skipped (unmapped account)"
        ]
    );
    assert_eq!(
        ImportResult {
            imported: 1,
            skipped_duplicates: 1,
            skipped_unmapped: 0
        }
        .summary_lines(),
        vec![
            "\u{2705} 1 transaction imported",
            "\u{23ED}\u{FE0F} 1 duplicate skipped"
        ]
    );
    assert_eq!(
        ImportResult {
            imported: 0,
            skipped_duplicates: 2,
            skipped_unmapped: 0
        }
        .summary_lines(),
        vec!["\u{23ED}\u{FE0F} 2 duplicates skipped"]
    );
    assert!(ImportResult::default().summary_lines().is_empty());
}

#[test]
fn r_csv_3_multiple_payee_and_memo_columns_join() {
    let mapping = mapping_from(&[
        (0, ImportField::Date),
        (1, ImportField::Payee),
        (2, ImportField::Amount),
        (3, ImportField::Payee),
        (4, ImportField::Memo),
        (5, ImportField::Memo),
    ]);
    assert_eq!(mapping.extra_payee, vec![3]);
    assert_eq!(mapping.all_payee_columns(), vec![1, 3]);
    assert_eq!(mapping.all_memo_columns(), vec![4, 5]);

    let (db, acct) = bank_db();
    let rows = vec![
        strings(&["03/01/2026", "A", "-1.00", "B", "x", ""]),
        strings(&["03/02/2026", "", "-2.00", "B", "", "y"]),
        strings(&["03/03/2026", "A", "-3.00", "", "", ""]),
    ];
    let plan = run(&db, &rows, &mapping, Some(acct), true);
    assert_eq!(plan.transactions[0].payee, "A | B");
    assert_eq!(plan.transactions[0].memo.as_deref(), Some("x"));
    assert_eq!(plan.transactions[1].payee, "B");
    assert_eq!(plan.transactions[1].memo.as_deref(), Some("y"));
    assert_eq!(plan.transactions[2].payee, "A");
    assert_eq!(plan.transactions[2].memo, None);
}
