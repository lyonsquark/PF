//! L3 behaviour and rule tests for budget items and Add to Budget (T10c: BEH-BUD-04, R-BUD-1..2,
//! R-TXN-11). Projections are in `behaviour_projection.rs`. Driven through the public engine API on
//! the `world` fixture, as the UI would call it.

use pf_engine::EngineError;
use pf_engine::commands::budget::{
    BudgetItemInput, add_to_budget, budget_items_by_type, default_budget_input, save_budget_item,
};
use pf_engine::testing::{TEST_TODAY, World, d, m};
use pf_model::{BudgetFrequency, BudgetItem, BudgetItemType, TransactionId};
use rust_decimal::Decimal;

/// A plain Monthly bill input with no end date, months, notes, category or account.
fn bill(name: &str, amount: &str, start: &str) -> BudgetItemInput {
    BudgetItemInput {
        item_type: BudgetItemType::Bill,
        name: name.into(),
        amount: m(amount),
        start_date: d(start),
        end_date: None,
        frequency: BudgetFrequency::Monthly,
        specific_months: Vec::new(),
        notes: String::new(),
        category: None,
        account: None,
    }
}

#[test]
/// R-BUD-1 — Covers: R-BUD-1
fn r_bud_1_items_by_type_sorted() {
    // Given
    let w = World::with_fixture("world");
    // When
    let bills = budget_items_by_type(w.engine.db(), BudgetItemType::Bill);
    // Then
    let names: Vec<String> = bills
        .iter()
        .map(|id| {
            w.engine
                .db()
                .budget_item(*id)
                .map(|b| b.name.clone())
                .unwrap_or_default()
        })
        .collect();
    assert_eq!(names, vec!["Car Insurance", "Landlord", "Property Tax"]);
}

#[test]
/// R-BUD-2 — Covers: R-BUD-2
fn r_bud_2_end_before_start_rejected() {
    // Given
    let mut w = World::with_fixture("world");
    let mut input = bill("Gym", "30", "2026-03-01");
    input.end_date = Some(d("2026-02-01"));
    // When
    let err = save_budget_item(&mut w.engine, None, input).expect_err("end before start");
    // Then
    assert_eq!(
        err,
        EngineError::Validation("End date cannot be before start date".into())
    );
    assert!(!w.engine.can_undo());
}

#[test]
/// Covers: R-BUD-2 (Swift `save()` normalisation)
fn budget_save_normalises_months_and_notes() {
    // Given: a monthly item with months set (they must be dropped).
    let mut w = World::with_fixture("world");
    let mut monthly = bill("Monthly Thing", "10", "2026-03-01");
    monthly.specific_months = vec![4, 1];
    let id = save_budget_item(&mut w.engine, None, monthly).expect("save monthly");
    assert!(
        w.engine
            .db()
            .budget_item(id)
            .is_some_and(|b| b.specific_months.is_empty())
    );
    // When: SpecificMonths with duplicates and out-of-order months.
    let mut specific = bill("Seasonal", "10", "2026-03-01");
    specific.frequency = BudgetFrequency::SpecificMonths;
    specific.specific_months = vec![10, 4, 4];
    let sid = save_budget_item(&mut w.engine, None, specific).expect("save specific");
    // Then: sorted and deduplicated.
    let stored = w
        .engine
        .db()
        .budget_item(sid)
        .map(|b| b.specific_months.clone());
    assert_eq!(stored, Some(vec![4, 10]));
    // Blank notes are stored as None; the kept notes are not trimmed.
    let mut blank = bill("Blank Notes", "5", "2026-03-01");
    blank.notes = "  ".into();
    let bid = save_budget_item(&mut w.engine, None, blank).expect("save blank notes");
    assert_eq!(
        w.engine.db().budget_item(bid).and_then(|b| b.notes.clone()),
        None
    );
    let mut kept = bill("Kept Notes", "5", "2026-03-01");
    kept.notes = " rent  ".into();
    let kid = save_budget_item(&mut w.engine, None, kept).expect("save notes");
    assert_eq!(
        w.engine.db().budget_item(kid).and_then(|b| b.notes.clone()),
        Some(" rent  ".to_string())
    );
}

#[test]
/// Covers: BUD editor defaults (Swift `init`)
fn default_budget_input_values() {
    // When
    let input = default_budget_input(BudgetItemType::Bill, TEST_TODAY);
    // Then
    assert_eq!(input.item_type, BudgetItemType::Bill);
    assert_eq!(input.start_date, d("2026-01-01"));
    assert_eq!(input.end_date, Some(d("2027-03-15")));
    assert_eq!(input.frequency, BudgetFrequency::Monthly);
    assert_eq!(input.amount, Decimal::ZERO);
    assert!(input.name.is_empty() && input.specific_months.is_empty());
}

#[test]
/// R-TXN-11 — Covers: R-TXN-11
fn r_txn_11_add_to_budget() {
    // Given: Starbucks on 2026-01-20 in Checking, categorised as Dining.
    let mut w = World::with_fixture("world");
    let starbucks = w.tx_by_payee("Starbucks");
    // When
    let toast = add_to_budget(&mut w.engine, starbucks).expect("add to budget");
    // Then
    assert_eq!(toast, "Added \"Starbucks\" to Bills");
    let id = w.budget_item("Starbucks");
    let item: BudgetItem = w.engine.db().budget_item(id).cloned().expect("item");
    assert_eq!(item.item_type, BudgetItemType::Bill);
    assert_eq!(item.amount, m("-5.75"));
    assert_eq!(item.frequency, BudgetFrequency::Monthly);
    assert_eq!(item.start_date, d("2026-01-20"));
    assert_eq!(item.category_id, Some(w.category("Dining")));
    assert_eq!(item.account_id, Some(w.account("Checking")));
}

#[test]
/// BEH-BUD-04 — Covers: BEH-BUD-04, R-TXN-11
fn beh_bud_04_add_to_budget_from_transaction() {
    // Given: Hotel Maui on 2026-02-14, -650, in Visa, no category.
    let mut w = World::with_fixture("world");
    let hotel: TransactionId = w.tx_by_payee("Hotel Maui");
    // When
    add_to_budget(&mut w.engine, hotel).expect("add to budget");
    // Then
    let bills = budget_items_by_type(w.engine.db(), BudgetItemType::Bill);
    let hotel_item = bills
        .iter()
        .filter_map(|id| w.engine.db().budget_item(*id))
        .find(|b| b.name == "Hotel Maui")
        .cloned()
        .expect("Hotel Maui is a bill");
    assert_eq!(hotel_item.amount, m("-650"));
    assert_eq!(hotel_item.start_date, d("2026-02-14"));
    assert_eq!(hotel_item.category_id, None);
    assert_eq!(hotel_item.account_id, Some(w.account("Visa")));
}
