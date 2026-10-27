//! L3 behaviour and rule tests for CSV import (T10c: BEH-CSV-01..02, R-CSV-14..15, R-API-8).
//! Driven through the public engine API on the `world` fixture, as the UI and the API would call it.

use pf_core::csv_import::{
    CategoryChoice, ColumnMapping, ImportField, TranslationChoices, detect_columns, parse_csv,
    plan_mappings, saved_column_assignments,
};
use pf_engine::EngineError;
use pf_engine::commands::accounts::AccountInput;
use pf_engine::commands::import::{
    CsvImportRequest, import_csv_with_saved_mappings, run_csv_import,
};
use pf_engine::testing::{World, m};
use pf_model::{AccountType, ImportMapping};

/// Bank export with a plain Date, Description and Amount layout.
const BANK_CSV: &str = "Date,Description,Amount\n03/01/2026,Corner Deli,-12.00\n03/02/2026,\"ACME, INC PAYROLL\",4000.00\n";

/// Monarch export: account, tags and original statement columns.
const MONARCH_CSV: &str = "Date,Merchant,Category,Account,Original Statement,Notes,Amount,Tags\n2026-03-01,Blue Bottle,Coffee Shops,Chase Checking,BLUE BOTTLE #123,,-4.50,\"Vacation,Work\"\n2026-03-02,Unknown Shop,Shopping,Chase Checking,UNKNOWN 999,,-20.00,\n";

/// Headers and rows of a CSV text, as the wizard holds them.
fn parsed(text: &str) -> (Vec<String>, Vec<Vec<String>>) {
    let p = parse_csv(text);
    (p.headers, p.rows)
}

/// A bank request for `text`, with the detected mapping and no translation choices.
fn bank_request(w: &World, text: &str) -> CsvImportRequest {
    let (headers, rows) = parsed(text);
    let mapping = detect_columns(&headers);
    let checking = w.account("Checking");
    let mapping_plan = plan_mappings(
        w.engine.db(),
        &headers,
        &mapping,
        &TranslationChoices::default(),
        Some(checking),
        false,
    );
    CsvImportRequest {
        is_monarch: false,
        target_account: Some(checking),
        rows,
        mapping,
        new_accounts: Vec::new(),
        mapping_plan,
    }
}

#[test]
/// BEH-CSV-01 — Covers: BEH-CSV-01, R-CSV-11, R-CSV-14
fn beh_csv_01_bank_import_then_duplicate_skip_then_undo() {
    // Given
    let mut w = World::with_fixture("world");
    assert_eq!(w.live_tx_count(), 14);
    let checking = w.account("Checking");
    // When: first import with the detected mapping.
    let req = bank_request(&w, BANK_CSV);
    let first = run_csv_import(&mut w.engine, req).expect("import");
    // Then
    assert_eq!(first.imported, 2);
    let payroll = w.tx_by_payee("ACME, INC PAYROLL");
    assert_eq!(
        w.engine.db().transaction(payroll).map(|t| t.amount),
        Some(m("4000.00"))
    );
    assert_eq!(w.live_tx_count(), 16);
    // When: the same text through the saved mappings.
    let second =
        import_csv_with_saved_mappings(&mut w.engine, checking, BANK_CSV).expect("second import");
    // Then: both rows are duplicates and nothing is stored.
    assert_eq!(second.imported, 0);
    assert_eq!(second.skipped_duplicates, 2);
    assert_eq!(w.live_tx_count(), 16);
    // When: one undo removes the first batch and no second step exists.
    w.engine.undo().expect("undo");
    // Then
    assert_eq!(w.live_tx_count(), 14);
    assert!(!w.engine.can_undo());
}

#[test]
/// BEH-CSV-02 — Covers: BEH-CSV-02, R-CSV-8, R-CSV-9, R-CSV-13
fn beh_csv_02_monarch_mappings_tags_unmapped_payees() {
    // Given: Monarch mappings for the account, category and one payee.
    let mut w = World::with_fixture("world");
    let (headers, rows) = parsed(MONARCH_CSV);
    let mapping = detect_columns(&headers);
    let choices = TranslationChoices {
        accounts: vec![("Chase Checking".into(), "Checking".into())],
        categories: vec![(
            "Coffee Shops".into(),
            CategoryChoice::MapToExisting("Dining".into()),
        )],
        payees: vec![("BLUE BOTTLE #123".into(), "Blue Bottle".into())],
    };
    let mapping_plan = plan_mappings(w.engine.db(), &headers, &mapping, &choices, None, true);
    // When
    let req = CsvImportRequest {
        is_monarch: true,
        target_account: None,
        rows,
        mapping,
        new_accounts: Vec::new(),
        mapping_plan,
    };
    let res = run_csv_import(&mut w.engine, req).expect("monarch import");
    // Then: one row imported, the unmapped payee skipped.
    assert_eq!(res.imported, 1);
    assert_eq!(res.skipped_unmapped, 1);
    let tx = w.tx_by_payee("Blue Bottle");
    let t = w.engine.db().transaction(tx).cloned().expect("row");
    assert_eq!(t.category_id, Some(w.category("Dining")));
    assert_eq!(t.account_id, Some(w.account("Checking")));
    let work = w.context("Work");
    assert_eq!(t.context_ids, vec![w.context("Vacation"), work]);
    assert_eq!(
        w.engine.db().context(work).map(|c| c.name.clone()),
        Some("Work".to_string()),
        "Work was created by the import"
    );
}

#[test]
/// R-CSV-14 — Covers: R-CSV-14
fn r_csv_14_import_is_one_undo_step() {
    // Given: a Monarch import that creates an account, a category, a context and mappings.
    let mut w = World::with_fixture("world");
    let before = w.engine.db().all_records();
    let (headers, rows) = parsed(MONARCH_CSV);
    let mapping = detect_columns(&headers);
    let choices = TranslationChoices {
        accounts: vec![("Chase Checking".into(), "Checking".into())],
        categories: vec![(
            "Coffee Shops".into(),
            CategoryChoice::CreateNew {
                name: "Coffee Shops".into(),
                parent: None,
            },
        )],
        payees: vec![("BLUE BOTTLE #123".into(), "Blue Bottle".into())],
    };
    let mapping_plan = plan_mappings(w.engine.db(), &headers, &mapping, &choices, None, true);
    let req = CsvImportRequest {
        is_monarch: true,
        target_account: None,
        rows,
        mapping,
        new_accounts: vec![AccountInput {
            name: "Amex".into(),
            account_type: AccountType::Credit,
            institution: "Amex".into(),
            currency: String::new(),
            is_external: false,
        }],
        mapping_plan,
    };
    // When
    run_csv_import(&mut w.engine, req).expect("import");
    assert_eq!(w.engine.undo_name(), Some("Import CSV"));
    assert!(w.engine.db().context_by_name("Work").is_some());
    // When: one undo.
    w.engine.undo().expect("undo");
    // Then: the database is back to its rows, and nothing else is left.
    assert_eq!(w.engine.db().all_records(), before);
    assert!(w.engine.db().context_by_name("Work").is_none());
    assert!(w.engine.db().category_by_name("Coffee Shops").is_none());
    assert!(w.engine.db().account_by_name("Amex").is_none());
    assert!(!w.engine.can_undo());
}

#[test]
/// R-CSV-15 — Covers: R-CSV-15
fn r_csv_15_column_mappings_saved_and_reused() {
    // Given: the wizard maps When→Date, Memo Text→Payee, Value→Amount and skips Junk.
    let mut w = World::with_fixture("world");
    let text = "When,Memo Text,Value,Junk\n03/05/2026,Groceries run,-42.10,x\n";
    let (headers, rows) = parsed(text);
    let mut mapping = ColumnMapping::default();
    mapping.apply_assignments(&[
        (0, ImportField::Date),
        (1, ImportField::Payee),
        (2, ImportField::Amount),
        (3, ImportField::Skip),
    ]);
    let checking = w.account("Checking");
    let mapping_plan = plan_mappings(
        w.engine.db(),
        &headers,
        &mapping,
        &TranslationChoices::default(),
        Some(checking),
        false,
    );
    let req = CsvImportRequest {
        is_monarch: false,
        target_account: Some(checking),
        rows,
        mapping,
        new_accounts: Vec::new(),
        mapping_plan,
    };
    run_csv_import(&mut w.engine, req).expect("import");
    // When: the same headers are looked up again.
    let saved = saved_column_assignments(w.engine.db(), &headers, Some(checking), false);
    // Then: the three wizard choices come back; Skip is not stored.
    assert_eq!(saved.len(), 3);
    for expected in [
        (0, ImportField::Date),
        (1, ImportField::Payee),
        (2, ImportField::Amount),
    ] {
        assert!(saved.contains(&expected), "missing {expected:?}");
    }
}

#[test]
/// R-CSV-15 — Covers: R-CSV-15
fn r_csv_15_negate_setting_replaced_not_duplicated() {
    // Given: the same wizard layout imported twice, negate first on and then off.
    let mut w = World::with_fixture("world");
    let checking = w.account("Checking");
    let layout = |negate: bool| {
        let mut m = ColumnMapping::default();
        m.apply_assignments(&[
            (0, ImportField::Date),
            (1, ImportField::Payee),
            (2, ImportField::Amount),
        ]);
        m.negate_amounts = negate;
        m
    };
    let headers: Vec<String> = ["Date", "Payee", "Amount"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    for (negate, text) in [
        (true, "03/05/2026,Cafe One,-3.00\n"),
        (false, "03/06/2026,Cafe Two,-4.00\n"),
    ] {
        let mapping = layout(negate);
        let rows = vec![text.trim().split(',').map(str::to_string).collect()];
        let mapping_plan = plan_mappings(
            w.engine.db(),
            &headers,
            &mapping,
            &TranslationChoices::default(),
            Some(checking),
            false,
        );
        let req = CsvImportRequest {
            is_monarch: false,
            target_account: Some(checking),
            rows,
            mapping,
            new_accounts: Vec::new(),
            mapping_plan,
        };
        run_csv_import(&mut w.engine, req).expect("import");
    }
    // Then: exactly one negate setting for Checking, holding the last value.
    let settings: Vec<&ImportMapping> = w
        .engine
        .db()
        .import_mappings
        .values()
        .filter(|m| m.mapping_type == "setting" && m.account_id == Some(checking))
        .collect();
    assert_eq!(settings.len(), 1);
    assert!(!settings[0].negate_amounts);
    let columns = w
        .engine
        .db()
        .import_mappings
        .values()
        .filter(|m| m.mapping_type == "column" && m.account_id == Some(checking))
        .count();
    assert_eq!(columns, 3, "column rows are replaced, not added to");
}

#[test]
/// R-API-8 — Covers: R-API-8
fn r_api_8_no_column_mappings_message() {
    // Given: Checking has no saved column mappings in the world fixture.
    let mut w = World::with_fixture("world");
    let checking = w.account("Checking");
    // When
    let err =
        import_csv_with_saved_mappings(&mut w.engine, checking, BANK_CSV).expect_err("no mappings");
    // Then
    assert_eq!(
        err,
        EngineError::Validation(
            "No saved column mappings for account 'Checking'. Please import via the UI first to set up column mappings.".into()
        )
    );
    assert!(!w.engine.can_undo());
}
