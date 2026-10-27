//! Table-style tests for the API and CLI output formats (T11, R-API-3..8).

use super::*;
use pf_model::testing::TestDb;
use pf_model::{AccountType, CategoryType, Db, TransactionStatus};
use pretty_assertions::assert_eq;
use rust_decimal_macros::dec;
use serde_json::Value;

/// Sets the institution of every account called `name`.
fn set_institution(db: &mut Db, name: &str, institution: &str) {
    for a in db.accounts.values_mut().filter(|a| a.name == name) {
        a.institution = institution.to_string();
    }
}

/// Checking (Chase), External 401k (Fidelity, external), Visa (Amex, negative balance).
/// Balances: 2051.95, 1000.00, -668.40. Total excludes the external account: 1383.55.
fn balances_db() -> Db {
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let ext = t.external_account("External 401k", AccountType::Investment);
    let visa = t.account("Visa", AccountType::Credit);
    t.tx(chk, "2026-03-01", "2051.95", "Pay").done();
    t.tx(ext, "2026-03-01", "1000.00", "Contribution").done();
    t.tx(visa, "2026-03-02", "-668.40", "Groceries")
        .status(TransactionStatus::Pending)
        .done();
    let mut db = t.build();
    set_institution(&mut db, "Checking", "Chase");
    set_institution(&mut db, "External 401k", "Fidelity");
    set_institution(&mut db, "Visa", "Amex");
    db
}

/// One checking account with four rows, and the scenario "Plan A".
/// Rows (id order): Opening -500000.00 (03-10, Cleared); Coffee -12.50 (03-11, Pending, category
/// Dining, contexts Taxes and Home, memo "latte"); Rent -100.00 (03-11, Future, Plan A);
/// Bonus 1000000.00 (03-12, Cleared).
fn ledger() -> (Db, AccountId, ScenarioId) {
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let plan = t.scenario("Plan A");
    let dining = t.category("Dining", CategoryType::Expense);
    let taxes = t.context("Taxes");
    let home = t.context("Home");
    t.tx(chk, "2026-03-10", "-500000.00", "Opening").done();
    t.tx(chk, "2026-03-11", "-12.50", "Coffee")
        .status(TransactionStatus::Pending)
        .category(dining)
        .contexts(&[taxes, home])
        .memo("latte")
        .done();
    t.tx(chk, "2026-03-11", "-100.00", "Rent")
        .status(TransactionStatus::Future)
        .scenario(plan)
        .done();
    t.tx(chk, "2026-03-12", "1000000.00", "Bonus").done();
    (t.build(), chk, plan)
}

#[test]
/// R-API-3: format names are case-insensitive; anything unknown is JSON.
fn r_api_3_parse_format() {
    assert_eq!(parse_format("TEXT"), Format::Text);
    assert_eq!(parse_format("md"), Format::Markdown);
    assert_eq!(parse_format("Markdown"), Format::Markdown);
    assert_eq!(parse_format("json"), Format::Json);
    assert_eq!(parse_format("xml"), Format::Json);
    assert_eq!(parse_format(""), Format::Json);
}

#[test]
/// R-API-4: missing, empty or "reality" is Reality; a live name (any case) is its id; else an error.
/// A soft-deleted scenario is not found.
fn r_api_4_resolve_scenario() {
    let mut t = TestDb::new();
    let plan = t.scenario("Plan A");
    let old = t.scenario("Old");
    let mut db = t.build();
    db.scenarios.get_mut(&old).expect("Old exists").is_deleted = true;

    assert_eq!(resolve_scenario(&db, None), Ok(None));
    assert_eq!(resolve_scenario(&db, Some("")), Ok(None));
    assert_eq!(resolve_scenario(&db, Some("Reality")), Ok(None));
    assert_eq!(resolve_scenario(&db, Some("reality")), Ok(None));
    assert_eq!(resolve_scenario(&db, Some("plan a")), Ok(Some(plan)));
    assert_eq!(
        resolve_scenario(&db, Some("Nope")),
        Err("Scenario 'Nope' not found.".to_string())
    );
    assert_eq!(
        resolve_scenario(&db, Some("Old")),
        Err("Scenario 'Old' not found.".to_string())
    );
    assert_eq!(
        error_body("Scenario 'Nope' not found.", Format::Json),
        r#"{"error":"Scenario 'Nope' not found."}"#
    );
    assert_eq!(
        error_body("Scenario 'Nope' not found.", Format::Text),
        "Error: Scenario 'Nope' not found."
    );
}

#[test]
/// R-API-6: text lists each account, an empty line, then the total (external excluded).
fn r_api_6_balances_text_exact() {
    let db = balances_db();
    let entries = balance_entries(&db, None);
    let expected = "Checking (Chase): $2,051.95\n\
                    External 401k (Fidelity) [external]: $1,000.00\n\
                    Visa (Amex): -$668.40\n\
                    \n\
                    Total: $1,383.55";
    assert_eq!(format_balances(&entries, Format::Text), expected);
}

#[test]
/// R-API-6: the external row keeps its trailing space after "*(ext)*"; the total row is bold.
fn r_api_6_balances_markdown_exact() {
    let db = balances_db();
    let entries = balance_entries(&db, None);
    let expected = "| Account | Institution | Type | Balance |\n\
                    | --- | --- | --- | ---: |\n\
                    | Checking | Chase | checking | $2,051.95 |\n\
                    | External 401k *(ext)*  | Fidelity | investment | $1,000.00 |\n\
                    | Visa | Amex | credit | -$668.40 |\n\
                    | **Total** | | | **$1,383.55** |";
    assert_eq!(format_balances(&entries, Format::Markdown), expected);
}

#[test]
/// R-API-6: sorted keys, numbers as JSON numbers, and the total leaves out external accounts.
fn r_api_6_balances_json_shape() {
    let db = balances_db();
    let entries = balance_entries(&db, None);
    let text = format_balances(&entries, Format::Json);
    let v: Value = serde_json::from_str(&text).expect("valid JSON");
    assert_eq!(v["total"].as_f64(), Some(1383.55));
    assert_eq!(v["accounts"].as_array().map(Vec::len), Some(3));
    assert_eq!(v["accounts"][1]["external"], Value::Bool(true));
    assert_eq!(v["accounts"][2]["balance"].as_f64(), Some(-668.40));
    let pos = |key: &str| text.find(key).expect("key present");
    assert!(pos("\"accounts\" :") < pos("\"total\" :"));
    assert!(pos("\"balance\" :") < pos("\"external\" :"));
    assert!(pos("\"external\" :") < pos("\"institution\" :"));
    assert!(pos("\"institution\" :") < pos("\"name\" :"));
    assert!(pos("\"name\" :") < pos("\"type\" :"));
}

#[test]
/// R-API-7, R-BAL-7: with n = 2 only two rows are returned, but their running balances are
/// computed over every visible row (oldest first). Deleted rows never count.
fn r_api_7_running_balance_over_all_visible() {
    let (mut db, chk, plan) = ledger();
    let ghost = TransactionId(9_000);
    let mut deleted = pf_model::Transaction::new(
        ghost,
        "2026-03-01".parse().expect("date"),
        dec!(777),
        "Ghost",
    );
    deleted.account_id = Some(chk);
    deleted.is_deleted = true;
    db.transactions.insert(ghost, deleted);

    let lines = transaction_lines(&db, chk, Some(plan), 2);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].payee, "Bonus");
    assert_eq!(lines[0].running_balance, dec!(499887.50));
    assert_eq!(lines[1].payee, "Rent");
    assert_eq!(lines[1].running_balance, dec!(-500112.50));
}

#[test]
/// R-API-7: exact text layout, 77-dash rule, truncation to 12 and 10 characters, and the
/// category, contexts, memo and scenario suffixes.
fn r_api_7_transactions_text_exact() {
    let (db, chk, plan) = ledger();
    let lines = transaction_lines(&db, chk, Some(plan), 10);
    let header = "Date        Amount        Balance       Status      Payee";
    assert_eq!(header.chars().count(), 57);
    let expected = format!(
        "Checking — Last 4 transactions\n\
         \n\
         {header}\n\
         {rule}\n\
         2026-03-12  $1,000,000.0  $499,887.50   Cleared     Bonus\n\
         2026-03-11  -$100.00      -$500,112.50  Future      Rent  (Plan A)\n\
         2026-03-11  -$12.50       -$500,012.50  Pending     Coffee  [Dining]  {{#Taxes, #Home}}  // latte\n\
         2026-03-10  -$500,000.00  -$500,000.00  Cleared     Opening",
        rule = "-".repeat(77),
    );
    assert_eq!(
        format_transactions("Checking", &lines, Format::Text),
        expected
    );
}

#[test]
/// R-API-7: scenario in the payee cell as `*(Plan A)*`; contexts as code spans.
fn r_api_7_transactions_markdown_exact() {
    let (db, chk, plan) = ledger();
    let lines = transaction_lines(&db, chk, Some(plan), 10);
    let expected = "## Checking — Last 4 Transactions\n\
         \n\
         | Date | Payee | Amount | Balance | Status | Category | Contexts | Memo |\n\
         | --- | --- | ---: | ---: | --- | --- | --- | --- |\n\
         | 2026-03-12 | Bonus | $1,000,000.00 | $499,887.50 | Cleared |  |  |  |\n\
         | 2026-03-11 | Rent *(Plan A)* | -$100.00 | -$500,112.50 | Future |  |  |  |\n\
         | 2026-03-11 | Coffee | -$12.50 | -$500,012.50 | Pending | Dining | `#Taxes` `#Home` | latte |\n\
         | 2026-03-10 | Opening | -$500,000.00 | -$500,000.00 | Cleared |  |  |  |";
    assert_eq!(
        format_transactions("Checking", &lines, Format::Markdown),
        expected
    );
}

#[test]
/// R-API-7: JSON omits memo, contexts, category and scenario when they are absent or empty.
fn r_api_7_transactions_json_optional_keys() {
    let (db, chk, plan) = ledger();
    let lines = transaction_lines(&db, chk, Some(plan), 10);
    let text = format_transactions("Checking", &lines, Format::Json);
    let v: Value = serde_json::from_str(&text).expect("valid JSON");
    assert_eq!(v["account"], "Checking");
    assert_eq!(v["count"].as_u64(), Some(4));
    let txs = &v["transactions"];

    let bonus = &txs[0];
    assert_eq!(bonus["date"], "2026-03-12");
    assert_eq!(bonus["status"], "cleared");
    assert_eq!(bonus["runningBalance"].as_f64(), Some(499887.5));
    for key in ["memo", "contexts", "category", "scenario"] {
        assert!(bonus.get(key).is_none(), "{key} must be absent");
    }

    let rent = &txs[1];
    assert_eq!(rent["scenario"], "Plan A");
    assert!(rent.get("memo").is_none());
    assert!(rent.get("contexts").is_none());

    let coffee = &txs[2];
    assert_eq!(coffee["status"], "pending");
    assert_eq!(coffee["memo"], "latte");
    assert_eq!(coffee["category"], "Dining");
    assert_eq!(coffee["contexts"][0], "Taxes");
    assert_eq!(coffee["contexts"][1], "Home");
    assert!(coffee.get("scenario").is_none());
}

#[test]
/// Scenario lists: "(None)" when empty, otherwise one entry per live scenario, sorted by name.
fn scenarios_all_formats() {
    let empty = Db::new();
    assert_eq!(
        format_scenarios(&empty, Format::Text),
        "Available Scenarios:\n(None)"
    );
    assert_eq!(
        format_scenarios(&empty, Format::Markdown),
        "## Available Scenarios\n\n*(None)*"
    );
    let v: Value = serde_json::from_str(&format_scenarios(&empty, Format::Json)).expect("JSON");
    assert_eq!(v["scenarios"].as_array().map(Vec::len), Some(0));

    let mut t = TestDb::new();
    let a = t.scenario("Plan A");
    t.scenario("Plan B");
    let mut db = t.build();
    let plan_a = db.scenarios.get_mut(&a).expect("Plan A exists");
    plan_a.color_hex = Some("#00AA00".into());
    plan_a.notes = Some("Test plan".into());

    assert_eq!(
        format_scenarios(&db, Format::Text),
        "Available Scenarios:\n- Plan A\n- Plan B"
    );
    assert_eq!(
        format_scenarios(&db, Format::Markdown),
        "## Available Scenarios\n\n- **Plan A**\n- **Plan B**"
    );
    let v: Value = serde_json::from_str(&format_scenarios(&db, Format::Json)).expect("JSON");
    assert_eq!(v["scenarios"][0]["name"], "Plan A");
    assert_eq!(v["scenarios"][0]["colorHex"], "#00AA00");
    assert_eq!(v["scenarios"][0]["notes"], "Test plan");
    assert_eq!(v["scenarios"][1]["name"], "Plan B");
    assert!(v["scenarios"][1]["colorHex"].is_null());
    assert!(v["scenarios"][1]["notes"].is_null());
}

#[test]
/// R-API-8: the import summary text block, and the JSON keys.
fn import_result_formats() {
    let expected = "Import Complete for Checking:\n\
                    Added:   3\n\
                    Skipped: 1 (duplicates)\n\
                    Skipped: 2 (unmapped payees)";
    assert_eq!(
        format_import_result("Checking", 3, 1, 2, Format::Text),
        expected
    );
    assert_eq!(
        format_import_result("Checking", 3, 1, 2, Format::Markdown),
        expected
    );
    let json = format_import_result("Checking", 3, 1, 2, Format::Json);
    let v: Value = serde_json::from_str(&json).expect("JSON");
    assert_eq!(v["account"], "Checking");
    assert_eq!(v["imported"].as_u64(), Some(3));
    assert_eq!(v["skippedDuplicates"].as_u64(), Some(1));
    assert_eq!(v["skippedUnmapped"].as_u64(), Some(2));
}

#[test]
/// Apple JSON style: "key" : value when pretty, slashes escaped, integers without a decimal point,
/// empty containers on one line, keys sorted when asked.
fn apple_json_escapes_slash_and_pretty_style() {
    let v = JsonValue::Obj(vec![("a/b".into(), JsonValue::Str("c/d".into()))]);
    // Keys are strings too, so the slash in the key is escaped as well.
    assert_eq!(
        to_apple_json(&v, true, true),
        "{\n  \"a\\/b\" : \"c\\/d\"\n}"
    );
    assert_eq!(to_apple_json(&v, false, true), r#"{"a\/b":"c\/d"}"#);

    let nested = JsonValue::Obj(vec![
        ("y".into(), JsonValue::Arr(vec![])),
        (
            "x".into(),
            JsonValue::Arr(vec![
                JsonValue::Num(4000.0),
                JsonValue::Num(2051.95),
                JsonValue::Obj(vec![]),
            ]),
        ),
    ]);
    assert_eq!(
        to_apple_json(&nested, true, true),
        "{\n  \"x\" : [\n    4000,\n    2051.95,\n    {}\n  ],\n  \"y\" : []\n}"
    );
    assert_eq!(
        to_apple_json(&nested, false, false),
        r#"{"y":[],"x":[4000,2051.95,{}]}"#
    );

    let escaped = JsonValue::Str("q\"\\\n\t\u{1}".into());
    assert_eq!(
        to_apple_json(&escaped, false, false),
        r#""q\"\\\n\t\u0001""#
    );
    assert_eq!(to_apple_json(&JsonValue::Null, false, false), "null");
    assert_eq!(to_apple_json(&JsonValue::Bool(true), false, false), "true");
}
