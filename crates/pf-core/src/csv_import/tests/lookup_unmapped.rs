//! R-CSV-4 (translation lookup) and R-CSV-5 (unmapped values).

use pf_model::testing::TestDb;
use pf_model::{AccountType, CategoryType, ImportMapping};

use super::{parsed, strings};
use crate::csv_import::{
    ColumnMapping, ImportField, collect_unmapped, detect_columns, lookup_mapping,
};

#[test]
fn r_csv_4_lookup_case_and_account_scope() {
    let mut b = TestDb::new();
    let acct = b.account("Checking", AccountType::Checking);
    let other = b.account("Savings", AccountType::Checking);
    b.import_mapping("payee", "STARBUCKS", "Starbucks", None);
    b.import_mapping("payee", "Coffee", "Coffee Acct", Some(acct));
    b.import_mapping("payee", "STARBUCKS", "First", None);
    b.import_mapping("payee", "Starbucks", "Second", None);
    let maps: Vec<ImportMapping> = b.build().import_mappings.values().cloned().collect();

    assert_eq!(
        lookup_mapping(&maps, "payee", "  Starbucks ", None),
        Some("Starbucks")
    );
    // A global mapping is not found for a specific account.
    assert_eq!(
        lookup_mapping(&maps, "payee", "STARBUCKS", Some(acct)),
        None
    );
    assert_eq!(
        lookup_mapping(&maps, "payee", "coffee", Some(acct)),
        Some("Coffee Acct")
    );
    assert_eq!(lookup_mapping(&maps, "payee", "coffee", None), None);
    assert_eq!(lookup_mapping(&maps, "payee", "coffee", Some(other)), None);
    // First match wins (lowest id).
    assert_eq!(
        lookup_mapping(&maps, "payee", "starbucks", None),
        Some("Starbucks")
    );
    // The type must match too.
    assert_eq!(lookup_mapping(&maps, "category", "STARBUCKS", None), None);
}

#[test]
fn r_csv_5_unmapped_monarch() {
    let mut b = TestDb::new();
    b.account("Checking", AccountType::Checking);
    b.category("Groceries", CategoryType::Expense);
    b.import_mapping("account", "chase checking", "Checking", None);
    let db = b.build();
    let p = parsed("monarch_basic.csv");
    let mapping = detect_columns(&p.headers);
    let got = collect_unmapped(&db, &p.rows, &mapping, None, true);
    assert_eq!(got.accounts, strings(&["Amex Gold", "Mystery Bank"]));
    assert_eq!(got.categories, strings(&["Coffee Shops", "Paychecks"]));
    assert_eq!(
        got.payees,
        strings(&[
            "PAYROLL EMPLOYER INC",
            "SOMETHING",
            "STARBUCKS STORE 123",
            "WHOLEFDS MKT 10"
        ])
    );
}

#[test]
fn r_csv_5_unmapped_uses_full_name_only() {
    let mut b = TestDb::new();
    let housing = b.category("Housing", CategoryType::Expense);
    b.child_category(housing, "Utilities");
    let db = b.build();
    let headers = strings(&["Date", "Payee", "Amount", "Category"]);
    let rows = vec![
        strings(&["03/01/2026", "X", "-1", "Utilities"]),
        strings(&["03/02/2026", "X", "-1", "housing/utilities"]),
    ];
    let got = collect_unmapped(&db, &rows, &detect_columns(&headers), None, false);
    assert_eq!(got.categories, strings(&["Utilities"]));
}

#[test]
fn r_csv_5_unmapped_bank_uses_primary_payee_only() {
    let db = TestDb::new().build();
    let mut mapping = ColumnMapping::default();
    mapping.apply_assignments(&[
        (0, ImportField::Date),
        (1, ImportField::Payee),
        (2, ImportField::Amount),
        (3, ImportField::Payee),
    ]);
    let rows = vec![strings(&["03/01/2026", "Primary A", "-1", "Extra B"])];
    let got = collect_unmapped(&db, &rows, &mapping, None, false);
    assert_eq!(got.payees, strings(&["Primary A"]));
}
