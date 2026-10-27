//! R-CSV-15 (mapping plan for saved columns and negate) and R-CSV-16 (translation choices).

use pf_model::testing::TestDb;
use pf_model::{AccountId, AccountType, CategoryType, Db, ImportMapping, ImportMappingId};

use super::strings;
use crate::csv_import::{
    CategoryChoice, ColumnMapping, ImportField, MappingPlan, NewCategory, NewMapping,
    TranslationChoices, UnmappedValues, detect_columns, plan_mappings, saved_column_assignments,
    saved_negate,
};

/// Applies a mapping plan to a Db the way the engine will: remove, then insert.
fn apply_plan(db: &mut Db, plan: &MappingPlan) {
    for id in &plan.remove_mapping_ids {
        db.import_mappings.remove(id);
    }
    for nm in &plan.new_mappings {
        let id = ImportMappingId(db.alloc_id());
        let mut m = ImportMapping::new(id, &nm.kind, &nm.source, &nm.target, nm.account_id);
        m.negate_amounts = nm.negate;
        db.import_mappings.insert(id, m);
    }
}

#[test]
fn r_csv_15_plan_mappings_columns_and_negate() {
    let mut b = TestDb::new();
    let acct = b.account("Checking", AccountType::Checking);
    let other = b.account("Savings", AccountType::Checking);
    let old_col = b.import_mapping("column", "Description", "Payee", Some(acct));
    b.import_mapping("column", "Date", "Date", None);
    let old_setting = b.import_mapping("setting", "negateAmounts", "false", Some(acct));
    b.import_mapping("setting", "negateAmounts", "true", Some(other));
    let mut db = b.build();

    let headers = strings(&[
        "Transaction Date",
        "Description",
        "Amount",
        "Type",
        "Balance",
    ]);
    let mut mapping = detect_columns(&headers);
    mapping.negate_amounts = true;
    let plan = plan_mappings(
        &db,
        &headers,
        &mapping,
        &TranslationChoices::default(),
        Some(acct),
        false,
    );

    let mut removed = plan.remove_mapping_ids.clone();
    removed.sort();
    assert_eq!(removed, vec![old_col, old_setting]);

    let columns: Vec<(&str, &str)> = plan
        .new_mappings
        .iter()
        .filter(|m| m.kind == "column")
        .map(|m| (m.source.as_str(), m.target.as_str()))
        .collect();
    assert_eq!(
        columns,
        vec![
            ("Transaction Date", "Date"),
            ("Description", "Payee"),
            ("Amount", "Amount")
        ]
    );
    let setting = plan
        .new_mappings
        .iter()
        .find(|m| m.kind == "setting")
        .expect("setting row");
    assert_eq!(
        setting,
        &NewMapping {
            kind: "setting".into(),
            source: "negateAmounts".into(),
            target: "true".into(),
            account_id: Some(acct),
            negate: true,
        }
    );

    apply_plan(&mut db, &plan);
    assert_eq!(
        saved_column_assignments(&db, &headers, Some(acct), false),
        vec![
            (0, ImportField::Date),
            (1, ImportField::Payee),
            (2, ImportField::Amount)
        ]
    );
    assert_eq!(saved_negate(&db, acct), Some(true));
    assert!(
        db.import_mappings
            .values()
            .any(|m| m.account_id == Some(other))
    );
    assert!(
        db.import_mappings
            .values()
            .any(|m| m.account_id.is_none() && m.source_value == "Date")
    );
}

#[test]
fn r_csv_16_translation_defaults() {
    let unmapped = UnmappedValues {
        accounts: strings(&["Mystery Bank"]),
        categories: strings(&["Coffee"]),
        payees: strings(&["STARBUCKS"]),
    };
    let choices = TranslationChoices::defaults(&unmapped);
    assert_eq!(
        choices.accounts,
        vec![("Mystery Bank".to_string(), String::new())]
    );
    assert_eq!(
        choices.categories,
        vec![(
            "Coffee".to_string(),
            CategoryChoice::CreateNew {
                name: "Coffee".into(),
                parent: None
            }
        )]
    );
    assert_eq!(
        choices.payees,
        vec![("STARBUCKS".to_string(), "STARBUCKS".to_string())]
    );

    let db = TestDb::new().build();
    let plan = plan_mappings(&db, &[], &ColumnMapping::default(), &choices, None, true);
    assert!(
        plan.new_mappings
            .iter()
            .all(|m| m.kind != "payee" && m.kind != "account")
    );
    assert_eq!(
        plan.new_categories,
        vec![NewCategory {
            name: "Coffee".into(),
            parent_id: None
        }]
    );
}

#[test]
fn r_csv_16_create_new_category_with_parent() {
    let mut b = TestDb::new();
    let dining = b.category("Dining", CategoryType::Expense);
    let db = b.build();
    let choices = TranslationChoices {
        categories: vec![(
            "Coffee".into(),
            CategoryChoice::CreateNew {
                name: "Coffee".into(),
                parent: Some("dining".into()),
            },
        )],
        ..Default::default()
    };
    let plan = plan_mappings(&db, &[], &ColumnMapping::default(), &choices, None, true);
    assert_eq!(
        plan.new_categories,
        vec![NewCategory {
            name: "Coffee".into(),
            parent_id: Some(dining)
        }]
    );
    assert_eq!(
        plan.new_mappings,
        vec![NewMapping {
            kind: "category".into(),
            source: "Coffee".into(),
            target: "Dining/Coffee".into(),
            account_id: None,
            negate: false,
        }]
    );

    // An existing category is mapped to, not created.
    let mut b = TestDb::new();
    let dining = b.category("Dining", CategoryType::Expense);
    b.child_category(dining, "Coffee");
    let db = b.build();
    let choices = TranslationChoices {
        categories: vec![(
            "Coffee".into(),
            CategoryChoice::CreateNew {
                name: "coffee".into(),
                parent: None,
            },
        )],
        ..Default::default()
    };
    let plan = plan_mappings(&db, &[], &ColumnMapping::default(), &choices, None, true);
    assert!(plan.new_categories.is_empty());
    assert_eq!(plan.new_mappings[0].target, "Dining/Coffee");

    // The same new name from two sources is planned once.
    let mut b = TestDb::new();
    b.category("Dining", CategoryType::Expense);
    let db = b.build();
    let choices = TranslationChoices {
        categories: vec![
            (
                "Coffee".into(),
                CategoryChoice::CreateNew {
                    name: "Coffee".into(),
                    parent: Some("Dining".into()),
                },
            ),
            (
                "Java".into(),
                CategoryChoice::CreateNew {
                    name: " coffee ".into(),
                    parent: Some("dining".into()),
                },
            ),
        ],
        ..Default::default()
    };
    let plan = plan_mappings(&db, &[], &ColumnMapping::default(), &choices, None, true);
    assert_eq!(plan.new_categories.len(), 1);
    assert_eq!(plan.new_mappings.len(), 2);
    assert!(
        plan.new_mappings
            .iter()
            .all(|m| m.target == "Dining/Coffee")
    );
}

#[test]
fn r_csv_16_build_mappings_filters() {
    let mut b = TestDb::new();
    let acct = b.account("Checking", AccountType::Checking);
    let db = b.build();
    let choices = TranslationChoices {
        accounts: vec![
            ("Mystery Bank".into(), String::new()),
            ("Chase".into(), "Checking".into()),
        ],
        categories: vec![
            ("Coffee".into(), CategoryChoice::Unmapped),
            ("Groc".into(), CategoryChoice::MapToExisting(String::new())),
            (
                "Food".into(),
                CategoryChoice::MapToExisting("Groceries".into()),
            ),
        ],
        payees: vec![
            ("STARBUCKS".into(), "STARBUCKS".into()),
            ("WHOLE".into(), String::new()),
            ("SB".into(), "Starbucks".into()),
        ],
    };

    let bank = plan_mappings(
        &db,
        &[],
        &ColumnMapping::default(),
        &choices,
        Some(acct),
        false,
    );
    let rows: Vec<(&str, &str, &str, Option<AccountId>)> = bank
        .new_mappings
        .iter()
        .filter(|m| m.kind != "setting")
        .map(|m| {
            (
                m.kind.as_str(),
                m.source.as_str(),
                m.target.as_str(),
                m.account_id,
            )
        })
        .collect();
    assert_eq!(
        rows,
        vec![
            ("account", "Chase", "Checking", None),
            ("category", "Food", "Groceries", Some(acct)),
            ("payee", "SB", "Starbucks", Some(acct)),
        ]
    );

    let monarch = plan_mappings(&db, &[], &ColumnMapping::default(), &choices, None, true);
    assert!(monarch.new_mappings.iter().all(|m| m.kind != "setting"));
    assert!(
        monarch
            .new_mappings
            .iter()
            .filter(|m| m.kind != "account")
            .all(|m| m.account_id.is_none())
    );
}
