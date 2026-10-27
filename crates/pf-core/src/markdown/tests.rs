//! L1 tests for Markdown import and export: fixtures, goldens, quirks and rules R-MD-1..6.

use std::collections::HashMap;

use jiff::civil::Date;
use pf_model::testing::TestDb;
use pf_model::{AccountId, AccountType, CategoryType, Db, RowKey, TransactionStatus};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use super::helpers::{chicago, fixture, import, new_york, now};
use super::{export_markdown, import_markdown};

/// The six fixture files and the names used in `fixtures/expected_counts.json`.
#[cfg(test)]
const FIXTURES: [&str; 6] = [
    "SampleData.md",
    "SampleData2.md",
    "SampleData3.md",
    "LargeSampleData.md",
    "PersonalFinance_Backup.md",
    "small.md",
];

/// Counts live rows the way `expected_counts.json` does.
fn counts(db: &Db) -> HashMap<&'static str, usize> {
    HashMap::from([
        ("contexts", db.contexts.len()),
        ("categories", db.categories.len()),
        ("scenarios", db.scenarios.len()),
        ("accounts", db.accounts.len()),
        ("obligations", db.obligations.len()),
        ("budget_items", db.budget_items.len()),
        ("transactions", db.transactions.len()),
        (
            "orphans",
            db.transactions
                .values()
                .filter(|t| t.account_id.is_none())
                .count(),
        ),
        ("import_mappings", db.import_mappings.len()),
        ("settings", db.settings.len()),
    ])
}

/// Sum of live transaction amounts for `account` that pass `keep`.
fn balance(db: &Db, account: AccountId, keep: impl Fn(&pf_model::Transaction) -> bool) -> Decimal {
    db.live_transactions()
        .filter(|t| t.account_id == Some(account) && keep(t))
        .map(|t| t.amount)
        .sum()
}

/// Splits `fixtures/expected_counts.json` into per-file expectations.
fn expected_counts() -> serde_json::Value {
    let text = fixture("expected_counts.json");
    serde_json::from_str(&text).expect("valid json")
}

/// Calls `import_markdown` and records every progress message.
fn import_with_progress(text: &str) -> (super::MarkdownImport, Vec<String>) {
    let mut messages = Vec::new();
    let result = import_markdown(text, &chicago(), now(), &mut |m| {
        messages.push(m.to_string())
    });
    (result, messages)
}

/// Covers: BEH-MD-01, R-MD-2
#[test]
fn beh_md_01_fixture_counts_match_expected() {
    let expected = expected_counts();
    for name in FIXTURES {
        let db = import(&fixture(name), &chicago()).db;
        let got = counts(&db);
        for (key, value) in expected[name].as_object().expect("object") {
            let want = value.as_u64().expect("count") as usize;
            assert_eq!(got[key.as_str()], want, "{name}: {key}");
        }
    }
}

/// Covers: R-MD-2
#[test]
fn r_md_2_old_budget_items_format() {
    let db = import(&fixture("SampleData2.md"), &chicago()).db;
    assert_eq!(db.budget_items.len(), 10);
    let water = db
        .budget_items
        .values()
        .find(|b| b.name == "Water")
        .expect("Water item");
    assert_eq!(water.specific_months, vec![1, 3, 5, 7, 9, 11]);
    assert_eq!(water.end_date, None);
}

/// Covers: R-MD-2
#[test]
fn r_md_2_new_budget_items_format() {
    let db = import(&fixture("small.md"), &chicago()).db;
    let gym = db
        .budget_items
        .values()
        .find(|b| b.name == "Gym")
        .expect("Gym");
    let june: Date = "2026-06-30".parse().expect("date");
    assert_eq!(gym.end_date, Some(june));
    assert_eq!(gym.frequency, pf_model::BudgetFrequency::Monthly);
    assert_eq!(gym.item_type, pf_model::BudgetItemType::Bill);
}

/// Covers: R-MD-2
#[test]
fn r_md_2_categories_without_patterns() {
    let db = import(&fixture("SampleData.md"), &chicago()).db;
    assert!(db.categories.values().all(|c| c.match_patterns.is_empty()));
    let salary = db.category_by_name("Salary").expect("Salary").id;
    let bonus = db.category_by_name("Bonus").expect("Bonus");
    assert_eq!(bonus.parent_id, Some(salary));
}

/// Covers: R-MD-2
/// SampleData2 uses the header `Name | Notes | Color`; values are read by position (Swift quirk).
#[test]
fn r_md_2_swapped_context_columns_quirk() {
    let db = import(&fixture("SampleData2.md"), &chicago()).db;
    let ctx = db.context_by_name("vacation-2026").expect("context");
    assert_eq!(ctx.color_hex.as_deref(), Some("saving for hawaii"));
    assert_eq!(ctx.notes.as_deref(), Some("#008080"));
}

/// Covers: R-MD-2
/// Header-like rows and rows with `---` are skipped, as Swift does.
#[test]
fn r_md_2_header_marker_rows_are_skipped() {
    let small = import(&fixture("small.md"), &chicago()).db;
    assert_eq!(small.import_mappings.len(), 4);
    assert!(
        small
            .import_mappings
            .values()
            .all(|m| m.source_value != "Date")
    );

    let doc = r#"## Accounts
| Name | Type | Institution | Currency | External |
| --- | --- | --- | --- | --- |
| Checking | checking | Chase | USD | false |

## Transactions
### Checking
| Date | Payee | Category | Amount | Memo | Scenario | Contexts | Status | Attachment |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-01-02 | Name |  | 5 |  |  |  | cleared |  |
| 2026-01-03 | Dashes | | 6 | a --- b |  |  | cleared |  |
| 2026-01-04 | Kept two |  | 7 |  |  |  | cleared |  |
"#;
    let result = import(doc, &chicago());
    assert_eq!(result.db.transactions.len(), 1);
    let kept = result.db.transactions.values().next().expect("one tx");
    assert_eq!(kept.payee, "Kept two");
    assert_eq!(result.skipped_rows, 0, "header-marker rows are not counted");
}

/// Covers: R-MD-2
/// LargeSampleData uses the legacy transaction layout: every transaction row is skipped.
#[test]
fn r_md_2_legacy_transaction_layout_imports_nothing() {
    let result = import(&fixture("LargeSampleData.md"), &chicago());
    assert_eq!(result.db.transactions.len(), 0);
    assert!(
        result.skipped_rows >= 675,
        "skipped {}",
        result.skipped_rows
    );
}

/// Covers: R-MD-2
/// Swift requires 3 columns for a context. Without a trailing pipe the last cell is dropped, so
/// `| Ctx | #fff | note` has only 2 columns and is rejected. The spec expected it to import.
#[test]
fn r_md_2_missing_trailing_pipe_drops_last_cell() {
    let doc = "## Contexts\n| Name | Color | Notes |\n| --- | --- | --- |\n| Ctx | #fff | note\n| Ctx2 | #abc | note | dropped\n";
    let result = import(doc, &chicago());
    assert!(result.db.context_by_name("Ctx").is_none());
    assert_eq!(result.skipped_rows, 1);
    let ctx2 = result.db.context_by_name("Ctx2").expect("Ctx2 imports");
    assert_eq!(ctx2.color_hex.as_deref(), Some("#abc"));
    assert_eq!(ctx2.notes.as_deref(), Some("note"));
}

/// Covers: R-MD-6
#[test]
fn r_md_6_name_lookups_exact_and_first_wins() {
    let doc = r#"## Contexts
| Name | Color | Notes |
| --- | --- | --- |
| Trip | #111111 | first |
| Trip | #222222 | second |

## Scenarios
| Name | Color | Notes |
| --- | --- | --- |
| Plan | | first |
| Plan | | second |

## Accounts
| Name | Type | Institution | Currency | External |
| --- | --- | --- | --- | --- |
| Checking | checking | A | USD | false |
| Checking | savings | B | USD | true |

## Categories
| Name | Type | Icon | Notes | Parent | Match Patterns |
| --- | --- | --- | --- | --- | --- |
| Groceries | expense |  |  |  |  |
| Groceries | income |  |  |  |  |

## Transactions
### Checking
| Date | Payee | Category | Amount | Memo | Scenario | Contexts | Status | Attachment |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-01-02 | Lower | groceries | -5 |  |  | trip | cleared |  |
| 2026-01-03 | Proper | Groceries | -6 |  | Plan | Trip | cleared |  |
"#;
    let result = import(doc, &chicago());
    let db = &result.db;
    assert_eq!(db.contexts.len(), 1);
    assert_eq!(
        db.context_by_name("Trip").expect("ctx").notes.as_deref(),
        Some("first")
    );
    assert_eq!(db.scenarios.len(), 1);
    assert_eq!(db.accounts.len(), 1);
    let acc = db.account_by_name("Checking").expect("account");
    assert_eq!(acc.account_type, AccountType::Checking);
    let groceries = db.category_by_name("Groceries").expect("category");
    assert_eq!(groceries.category_type, CategoryType::Expense);
    assert_eq!(db.categories.len(), 1);
    assert_eq!(result.skipped_rows, 4, "four duplicate rows are dropped");

    let lower = db
        .transactions
        .values()
        .find(|t| t.payee == "Lower")
        .expect("tx");
    assert_eq!(lower.category_id, None, "wrong case does not match");
    assert!(
        lower.context_ids.is_empty(),
        "lowercase `trip` does not match `Trip`"
    );
    let proper = db
        .transactions
        .values()
        .find(|t| t.payee == "Proper")
        .expect("tx");
    assert_eq!(proper.category_id, Some(groceries.id));
    assert_eq!(
        proper.scenario_id,
        db.scenario_by_name("Plan").map(|s| s.id)
    );
    assert_eq!(
        proper.context_ids,
        vec![db.context_by_name("Trip").expect("Trip").id]
    );
}

/// Covers: R-MD-6, R-MD-2
#[test]
fn r_md_6_orphans_and_unknown_subsections() {
    let db = import(&fixture("small.md"), &chicago()).db;
    let fee = db
        .transactions
        .values()
        .find(|t| t.payee == "Closed account fee")
        .expect("fee");
    assert_eq!(fee.account_id, None);
    let global = db
        .import_mappings
        .values()
        .find(|m| m.source_value == "chase checking")
        .expect("global mapping");
    assert_eq!(global.account_id, None);
    let checking = db.account_by_name("Checking").expect("Checking").id;
    let payee_map = db
        .import_mappings
        .values()
        .find(|m| m.source_value == "starbucks store 123")
        .expect("checking mapping");
    assert_eq!(payee_map.account_id, Some(checking));
}

/// Covers: R-MD-2
/// Facts from the `small.md` table in T05. Balances are computed directly from the Db.
#[test]
fn small_fixture_facts() {
    let db = import(&fixture("small.md"), &chicago()).db;
    let accounts = db.accounts_by_name();
    let names: Vec<(&str, AccountType, bool)> = accounts
        .iter()
        .map(|a| (a.name.as_str(), a.account_type, a.is_external))
        .collect();
    assert_eq!(
        names,
        vec![
            ("Brokerage", AccountType::Investment, true),
            ("Checking", AccountType::Checking, false),
            ("Rewards Card", AccountType::Credit, false),
        ]
    );
    let checking = db.account_by_name("Checking").expect("Checking").id;
    let rewards = db.account_by_name("Rewards Card").expect("Rewards").id;
    let plan = db.scenario_by_name("Plan A").expect("Plan A").id;
    let reality = |t: &pf_model::Transaction| t.scenario_id.is_none();
    let cleared = |t: &pf_model::Transaction| {
        matches!(
            t.status,
            TransactionStatus::Cleared | TransactionStatus::Reconciled
        )
    };
    let plan_a = |t: &pf_model::Transaction| t.scenario_id.is_none() || t.scenario_id == Some(plan);

    assert_eq!(balance(&db, checking, reality), dec!(6476.46));
    assert_eq!(
        balance(&db, checking, |t| reality(t) && cleared(t)),
        dec!(6596.91)
    );
    assert_eq!(balance(&db, checking, plan_a), dec!(8976.46));
    assert_eq!(balance(&db, rewards, reality), dec!(62.95));
    assert_eq!(
        balance(&db, rewards, |t| reality(t) && cleared(t)),
        dec!(69.70)
    );
    assert_eq!(balance(&db, rewards, plan_a), dec!(-87.05));

    let orphans: Vec<&str> = db
        .live_transactions()
        .filter(|t| t.account_id.is_none())
        .map(|t| t.payee.as_str())
        .collect();
    assert_eq!(orphans, vec!["Closed account fee"]);
    let uncategorized = db
        .live_transactions()
        .filter(|t| t.category_id.is_none())
        .count();
    assert_eq!(uncategorized, 4);

    let utilities = db.category_by_name("Utilities").expect("Utilities").id;
    assert_eq!(db.category_full_name(utilities), "Housing/Utilities");

    let context_names = |payee: &str, date: &str| -> Vec<String> {
        let date: Date = date.parse().expect("date");
        let tx = db
            .live_transactions()
            .find(|t| t.payee == payee && t.date == date)
            .expect("transaction");
        tx.context_ids
            .iter()
            .map(|c| db.context(*c).expect("context").name.clone())
            .collect()
    };
    assert_eq!(
        context_names("Hawaii flights", "2026-03-10"),
        vec!["vacation"]
    );
    assert_eq!(
        context_names("Whole Foods", "2026-03-02"),
        vec!["vacation", "home-repair"]
    );

    assert_eq!(db.setting("lastProjectionScenario"), Some("Plan A"));
    assert_eq!(db.setting("lastProjectionStartDate"), Some("2026-03-15"));
    assert_eq!(db.setting("lastProjectionEndDate"), Some("2026-12-31"));
}

/// Covers: R-MD-2
#[test]
fn import_progress_messages() {
    let (_, messages) = import_with_progress(&fixture("small.md"));
    let sections: Vec<&str> = messages
        .iter()
        .map(String::as_str)
        .filter(|m| m.starts_with("Importing ") && m.ends_with("..."))
        .collect();
    assert_eq!(
        sections,
        vec![
            "Importing Contexts...",
            "Importing Categories...",
            "Importing Scenarios...",
            "Importing Accounts...",
            "Importing Obligations...",
            "Importing BudgetItems...",
            "Importing ImportMappings...",
            "Importing Transactions...",
            "Importing Settings...",
        ]
    );
    assert_eq!(messages.last().map(String::as_str), Some("Finalizing..."));

    let (_, messages) = import_with_progress(&fixture("SampleData2.md"));
    assert!(
        messages
            .iter()
            .any(|m| m == "Importing Transactions... 50/278")
    );
}

/// Covers: BEH-MD-02, R-MD-1
#[test]
fn beh_md_02_golden_export_small() {
    let text = export_markdown(&import(&fixture("small.md"), &chicago()).db, &chicago());
    insta::assert_snapshot!(text);
}

/// Covers: R-MD-1
#[test]
fn golden_export_each_fixture() {
    for name in FIXTURES.iter().filter(|n| **n != "small.md") {
        let text = export_markdown(&import(&fixture(name), &chicago()).db, &chicago());
        let stem = name.trim_end_matches(".md");
        insta::assert_snapshot!(format!("golden_export_{stem}"), text);
    }
}

/// Covers: R-MD-1, R-MD-3
#[test]
fn export_is_idempotent_on_fixtures() {
    for name in FIXTURES {
        let first = export_markdown(&import(&fixture(name), &chicago()).db, &chicago());
        let second = export_markdown(&import(&first, &chicago()).db, &chicago());
        assert_eq!(second, first, "{name}");
    }
}

/// Covers: R-MD-1
#[test]
fn r_md_1_empty_db_layout() {
    let out = export_markdown(&Db::new(), &chicago());
    let expected = "# Personal Finance Data\n\n\
## Contexts\n| Name | Color | Notes |\n| --- | --- | --- |\n\n\
## Categories\n| Name | Type | Icon | Notes | Parent | Match Patterns |\n| --- | --- | --- | --- | --- | --- |\n\n\
## Scenarios\n| Name | Color | Notes |\n| --- | --- | --- |\n\n\
## Accounts\n| Name | Type | Institution | Currency | External |\n| --- | --- | --- | --- | --- |\n\n\
## Obligations\n| Name | Status | Target | Balance | Due Date | Created Date | Updated Date | Notes | Account | Context |\n| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |\n\n\
## BudgetItems\n| Name | Type | Amount | Start Date | End Date | Frequency | Specific Months | Notes | Category | Account |\n| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |\n\n\
## ImportMappings\n## Transactions\n";
    assert_eq!(out, expected);
}

/// Covers: R-MD-1
#[test]
fn r_md_1_settings_section_rules() {
    let mut t = TestDb::new();
    t.setting("lastProjectionStartDate", "2026-03-15");
    let only_date = export_markdown(&t.build(), &new_york());
    assert!(!only_date.contains("## Settings"), "{only_date}");

    let mut t = TestDb::new();
    t.setting("lastProjectionScenario", "Plan A");
    t.setting("lastProjectionStartDate", "2026-03-15");
    t.setting("lastProjectionEndDate", "2026-12-31");
    t.setting("backupFolder", "/Users/me/Backups");
    let out = export_markdown(&t.build(), &new_york());
    assert!(
        out.ends_with(
            "## Settings\n| Key | Value |\n| --- | --- |\n\
| lastProjectionScenario | Plan A |\n\
| lastProjectionStartDate | 2026-03-15T04:00:00Z |\n\
| lastProjectionEndDate | 2026-12-31T05:00:00Z |\n\
| backupFolderBookmark | /Users/me/Backups |\n\n"
        ),
        "{out}"
    );
}

/// Covers: R-MD-1
#[test]
fn r_md_1_only_live_rows_exported() {
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let gone_acc = t.account("ZZGoneAcc", AccountType::Savings);
    let gone_ctx = t.context("ZZGoneCtx");
    let gone_scn = t.scenario("ZZGoneScn");
    let gone_cat = t.category("ZZGoneCat", CategoryType::Expense);
    let gone_obl = t.obligation("ZZGoneObl").done();
    let gone_bud = t
        .budget_item(
            pf_model::BudgetItemType::Bill,
            "ZZGoneBud",
            "5",
            "2026-01-01",
            pf_model::BudgetFrequency::Monthly,
        )
        .done();
    t.tx(chk, "2026-01-02", "-1", "kept").done();
    t.tx(chk, "2026-01-03", "-2", "ZZGonePayee")
        .deleted()
        .done();
    t.delete(RowKey::Account(gone_acc));
    t.delete(RowKey::Context(gone_ctx));
    t.delete(RowKey::Scenario(gone_scn));
    t.delete(RowKey::Category(gone_cat));
    t.delete(RowKey::Obligation(gone_obl));
    t.delete(RowKey::BudgetItem(gone_bud));
    let out = export_markdown(&t.build(), &chicago());
    assert!(!out.contains("ZZGone"), "deleted row leaked: {out}");
    assert!(out.contains("| kept |"));
}

/// Covers: R-MD-1
#[test]
fn r_md_1_sort_orders() {
    let mut t = TestDb::new();
    let zed = t.category("Zed", CategoryType::Expense);
    t.category("Alpha", CategoryType::Expense);
    t.child_category(zed, "Aardvark");
    let chk = t.account("Checking", AccountType::Checking);
    let sav = t.account("Savings", AccountType::Savings);
    t.tx(chk, "2026-01-05", "-1", "first").done();
    t.tx(chk, "2026-01-05", "-2", "second").done();
    t.tx(chk, "2026-02-01", "-3", "later").done();
    t.tx(sav, "2026-01-01", "-4", "savings one").done();
    let out = export_markdown(&t.build(), &chicago());

    let pos = |needle: &str| {
        out.find(needle)
            .unwrap_or_else(|| panic!("missing {needle}"))
    };
    assert!(pos("| Alpha |") < pos("| Zed |"));
    assert!(
        pos("| Zed |") < pos("| Aardvark |"),
        "children after top-level"
    );
    assert!(pos("### Checking") < pos("### Savings"));
    assert!(pos("| later |") < pos("| second |"), "newer date first");
    assert!(
        pos("| second |") < pos("| first |"),
        "same date: higher id first"
    );
}
