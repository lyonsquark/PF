//! Tests for the shared chart helpers in `charts/mod.rs` (R-CHT-0, 0a, 0b, 0c, 10).

use std::collections::BTreeSet;

use pf_model::testing::{TestDb, d};
use pf_model::{AccountType, CategoryType, TimePeriod};

use super::fixtures::{payees, small_db, today};
use super::*;

#[test]
/// Covers: R-CHT-0
fn r_cht_0_filter_rules() {
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let card = t.account("Card", AccountType::Credit);
    let plan = t.scenario("Plan A");
    let xfer = t.category("Credit Card Payment", CategoryType::Transfer);
    let food = t.category("Food", CategoryType::Expense);
    // Inserted out of date order, so the sort is visible.
    let b = t.tx(chk, "2026-03-02", "-5", "B").category(food).done();
    let a = t.tx(chk, "2026-03-01", "-1", "A").done();
    t.tx(card, "2026-03-05", "300", "Pay").category(xfer).done();
    t.tx(chk, "2026-03-08", "-50", "Transfer out")
        .category(xfer)
        .done();
    t.tx(chk, "2026-03-03", "-7", "Plan").scenario(plan).done();
    t.tx(chk, "2026-03-04", "-9", "Deleted").deleted().done();
    t.orphan_tx("2026-03-06", "-2", "Orphan").done();
    t.tx(card, "2026-03-07", "-3", "Unselected").done();
    let db = t.build();
    let only_chk = ChartFilter {
        accounts: BTreeSet::from([chk]),
        scenario: None,
        today: today(),
    };

    // F without transfers: A, B only (orphan, deleted, scenario and unselected rows excluded).
    let spend = filtered_transactions(&db, &only_chk, false);
    assert_eq!(payees(&spend), ["A", "B"]);
    assert_eq!(spend[0].id, a);
    assert_eq!(spend[1].id, b);
    // F with transfers: the Transfer-category row joins, still sorted by date.
    let all = filtered_transactions(&db, &only_chk, true);
    assert_eq!(payees(&all), ["A", "B", "Transfer out"]);
    // Under Plan A, the scenario row is visible on top of Reality.
    let plan_filter = ChartFilter {
        scenario: Some(plan),
        ..only_chk.clone()
    };
    assert_eq!(
        payees(&filtered_transactions(&db, &plan_filter, false)),
        ["A", "B", "Plan"]
    );
    // Selecting the card shows its own rows only when the card is selected.
    let card_filter = ChartFilter {
        accounts: BTreeSet::from([card]),
        ..only_chk
    };
    assert_eq!(
        payees(&filtered_transactions(&db, &card_filter, true)),
        ["Pay", "Unselected"]
    );
}

#[test]
/// Covers: R-CHT-0a
fn r_cht_0a_default_selection_and_labels() {
    let db = small_db();
    let chk = super::fixtures::account_id(&db, "Checking");
    let card = super::fixtures::account_id(&db, "Rewards Card");
    let brokerage = super::fixtures::account_id(&db, "Brokerage");
    assert_eq!(default_selection(&db), BTreeSet::from([chk, card]));

    let all = BTreeSet::from([chk, card, brokerage]);
    assert_eq!(selection_label(&db, &all), "All Accounts");
    assert_eq!(selection_label(&db, &BTreeSet::new()), "No Accounts");
    assert_eq!(selection_label(&db, &BTreeSet::from([chk])), "Checking");
    assert_eq!(
        selection_label(&db, &BTreeSet::from([chk, card])),
        "2 Accounts"
    );
    assert_eq!(select_all_button_label(&db, &all), "Deselect All");
    assert_eq!(
        select_all_button_label(&db, &BTreeSet::from([chk, card])),
        "Select All"
    );
}

#[test]
/// Covers: R-CHT-0b
fn r_cht_0b_month_helpers() {
    assert_eq!(
        month_bounds(d("2024-02-10")),
        (d("2024-02-01"), d("2024-02-29"))
    );
    assert_eq!(month_label_short(d("2024-02-10")), "Feb 24");
    assert_eq!(month_label_long(d("2024-02-10")), "February 2024");
    assert_eq!(spending::step_month(d("2026-03-31"), -1), d("2026-02-28"));
    assert_eq!(spending::step_month(d("2026-01-31"), 1), d("2026-02-28"));
    assert_eq!(
        period_window(TimePeriod::PastMonth, today()),
        (Some(d("2026-02-15")), today())
    );
    assert_eq!(period_window(TimePeriod::All, today()), (None, today()));
}

#[test]
/// Covers: R-CHT-0c
fn r_cht_0c_titles_and_messages() {
    assert_eq!(
        CHART_TITLES,
        [
            "Account Balances Over Time",
            "Cumulative Spending (Month vs Month)",
            "Budget Pacing vs Actuals",
            "Profit & Loss (Cashflow)",
            "Categorical Breakdown",
            "Monthly Cashflow Waterfall",
            "Expense Distribution",
            "Income vs Expense Trend",
            "Money Flow",
        ]
    );
    assert_eq!(NOT_ENOUGH_DATA, "Not enough data");
    assert_eq!(NO_BUDGETS, "No budgets set for this month.");
}

#[test]
/// Covers: R-CHT-10
fn r_cht_10_category_hex_goldens() {
    assert_eq!(category_hex("Groceries"), "#92e544");
    assert_eq!(category_hex("Dining"), "#4744e5");
    assert_eq!(category_hex("Housing"), "#6744e5");
    assert_eq!(category_hex("Utilities"), "#44e58a");
    assert_eq!(category_hex("Shopping"), "#e54444");
    assert_eq!(category_hex("Income"), "#e59d44");
    assert_eq!(category_hex("Other"), "#8e8e93");
    assert_eq!(category_hex("Uncategorized"), "#8e8e93");
    assert_eq!(
        category_hex("A very long category name that overflows the hash"),
        "#44e557"
    );
}

#[test]
/// Covers: R-CHT-10
fn r_cht_10_hex_parses_with_t04() {
    let mut colours: Vec<&str> = vec![
        COLOR_INCOME,
        COLOR_EXPENSE,
        COLOR_BLUE,
        COLOR_GREY,
        COLOR_SAVINGS,
    ];
    colours.extend(waterfall::EXPENSE_CYCLE);
    for c in colours {
        assert!(crate::color::parse_hex(c).is_some(), "{c} does not parse");
    }
    for name in ["Groceries", "Housing", "Income"] {
        let hex = category_hex(name);
        assert!(
            crate::color::parse_hex(&hex).is_some(),
            "{hex} does not parse"
        );
    }
}
