//! R-CSV-2, R-CSV-3, R-CSV-15: detection, manual assignment, saved column layouts.

use pf_model::AccountType;
use pf_model::testing::TestDb;

use super::{mapping_from, pairs, parsed, strings};
use crate::csv_import::{
    ColumnMapping, ImportField, detect_columns, saved_column_assignments, validate_required,
};

fn assign(i: usize, header: &str, field: ImportField) -> (usize, String, ImportField) {
    (i, header.to_string(), field)
}

#[test]
fn r_csv_2_detect_bank_headers() {
    let p = parsed("bank_basic.csv");
    assert_eq!(
        detect_columns(&p.headers),
        ColumnMapping {
            date: Some(0),
            payee: Some(1),
            amount: Some(2),
            ..Default::default()
        }
    );
}

#[test]
fn r_csv_2_detect_monarch_headers() {
    let p = parsed("monarch_basic.csv");
    assert_eq!(
        detect_columns(&p.headers),
        ColumnMapping {
            date: Some(0),
            payee: Some(1),
            category: Some(2),
            account: Some(3),
            original_statement: Some(4),
            memo: Some(5),
            amount: Some(6),
            tags: Some(7),
            ..Default::default()
        }
    );
}

#[test]
fn r_csv_2_first_match_wins_and_chain_order() {
    assert_eq!(detect_columns(&strings(&["Name", "Payee"])).payee, Some(0));
    let original = detect_columns(&strings(&["Original Amount"]));
    assert_eq!(original.amount, None);
    assert_eq!(original.original_statement, Some(0));
    assert_eq!(
        detect_columns(&strings(&["Transaction Date", "Date"])).date,
        Some(0)
    );
}

#[test]
fn r_csv_3_assignments_round_trip() {
    for file in ["bank_basic.csv", "monarch_basic.csv"] {
        let p = parsed(file);
        let detected = detect_columns(&p.headers);
        let rebuilt = mapping_from(&pairs(&detected.assignments(&p.headers)));
        // R-CSV-3 quirk: Original Statement is shown as Skip, so it does not survive.
        let expected = ColumnMapping {
            original_statement: None,
            ..detected.clone()
        };
        assert_eq!(rebuilt, expected, "{file}");
    }
    let headers = strings(&["Date", "A", "Amount", "B", "M1", "M2"]);
    let with_extras = mapping_from(&[
        (0, ImportField::Date),
        (1, ImportField::Payee),
        (2, ImportField::Amount),
        (3, ImportField::Payee),
        (4, ImportField::Memo),
        (5, ImportField::Memo),
    ]);
    assert_eq!(
        mapping_from(&pairs(&with_extras.assignments(&headers))),
        with_extras
    );
}

#[test]
fn r_csv_3_apply_resets_but_keeps_negate() {
    let mut m = detect_columns(&strings(&["Date", "Payee", "Amount"]));
    m.negate_amounts = true;
    m.apply_assignments(&[(2, ImportField::Amount)]);
    assert_eq!(
        m,
        ColumnMapping {
            amount: Some(2),
            negate_amounts: true,
            ..Default::default()
        }
    );
}

#[test]
fn r_csv_3_validate_required_message() {
    assert_eq!(
        validate_required(&[assign(1, "Payee", ImportField::Payee)]),
        Err("Required columns not mapped: Date, Amount. Every import needs at least Date, Payee, and Amount.".to_string())
    );
    let all = [
        assign(0, "Date", ImportField::Date),
        assign(1, "Payee", ImportField::Payee),
        assign(2, "Amount", ImportField::Amount),
    ];
    assert_eq!(validate_required(&all), Ok(()));
}

#[test]
fn r_csv_15_saved_assignments_exact_header_match() {
    let mut b = TestDb::new();
    let acct = b.account("Checking", AccountType::Checking);
    b.import_mapping("column", "Description", "Payee", Some(acct));
    b.import_mapping("column", "Date", "Date", None);
    let db = b.build();
    let headers = strings(&["Date", "Description", "Amount"]);
    assert_eq!(
        saved_column_assignments(&db, &headers, Some(acct), false),
        vec![(1, ImportField::Payee)]
    );
    let lower = strings(&["Date", "description", "Amount"]);
    assert_eq!(
        saved_column_assignments(&db, &lower, Some(acct), false),
        vec![]
    );
    // Monarch reads the global (account None) rows whatever account is passed.
    assert_eq!(
        saved_column_assignments(&db, &headers, Some(acct), true),
        vec![(0, ImportField::Date)]
    );
    assert_eq!(
        saved_column_assignments(&db, &headers, None, true),
        vec![(0, ImportField::Date)]
    );
}
