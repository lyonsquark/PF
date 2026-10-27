//! L3 behaviour and rule tests for projections into scenarios and Reproject (T10c: BEH-BUD-01..03,
//! R-PRJ-6..9). Driven through the public engine API on the `world` fixture, as the UI would call it.

use jiff::civil::Date;
use pf_engine::EngineError;
use pf_engine::commands::budget::{
    BudgetItemInput, ProjectionRequest, ProjectionTarget, generate_projection, last_projection,
    reproject_item,
};
use pf_engine::testing::{World, d, m};
use pf_model::{BudgetItemId, Db, ScenarioId, TransactionStatus};
use rust_decimal::Decimal;

/// The editor values of an existing item, as the Budget editor would hold them.
fn input_of(db: &Db, id: BudgetItemId) -> BudgetItemInput {
    let b = db.budget_item(id).expect("budget item exists");
    BudgetItemInput {
        item_type: b.item_type,
        name: b.name.clone(),
        amount: b.amount,
        start_date: b.start_date,
        end_date: b.end_date,
        frequency: b.frequency,
        specific_months: b.specific_months.clone(),
        notes: b.notes.clone().unwrap_or_default(),
        category: b.category_id,
        account: b.account_id,
    }
}

/// The five budget items of the world fixture, in name order.
fn all_five(w: &World) -> Vec<BudgetItemId> {
    [
        "Acme Payroll",
        "Car Insurance",
        "Groceries",
        "Landlord",
        "Property Tax",
    ]
    .iter()
    .map(|n| w.budget_item(n))
    .collect()
}

/// `(date, amount)` of the rows in `scenario` whose payee is `payee`, sorted by date.
fn rows_for(w: &World, scenario: ScenarioId, payee: &str) -> Vec<(Date, Decimal)> {
    let mut rows: Vec<(Date, Decimal)> = w
        .engine
        .db()
        .transactions_for_scenario(scenario)
        .filter(|t| t.payee == payee)
        .map(|t| (t.date, t.amount))
        .collect();
    rows.sort();
    rows
}

/// Dates from literal `YYYY-MM-DD` strings.
fn dates(list: &[&str]) -> Vec<Date> {
    list.iter().map(|s| d(s)).collect()
}

/// Runs the BEH-BUD-01 projection and returns the new scenario id.
fn generate_next_year(w: &mut World) -> ScenarioId {
    let items = all_five(w);
    let res = generate_projection(
        &mut w.engine,
        ProjectionRequest {
            target: ProjectionTarget::New("Next Year".into()),
            start: d("2026-03-15"),
            end: d("2027-03-14"),
            items,
        },
    )
    .expect("generate projection");
    assert_eq!(res.count, 42, "5 items generate 42 rows in that window");
    res.scenario
}

#[test]
/// R-PRJ-6 — Covers: R-PRJ-6
fn r_prj_6_amount_and_payee_by_type() {
    // Given: one Bill, one Expense and one Income item, all monthly from the 15th.
    let mut w = World::with_fixture("world");
    let landlord = w.budget_item("Landlord");
    let groceries = w.budget_item("Groceries");
    let payroll = w.budget_item("Acme Payroll");
    let checking = w.account("Checking");
    // When
    let res = generate_projection(
        &mut w.engine,
        ProjectionRequest {
            target: ProjectionTarget::New("Six".into()),
            start: d("2026-03-15"),
            end: d("2026-04-30"),
            items: vec![landlord, groceries, payroll],
        },
    )
    .expect("generate");
    // Then
    assert_eq!(res.count, 5, "Landlord 1, Groceries 2, Payroll 2");
    let rows: Vec<_> = w
        .engine
        .db()
        .transactions_for_scenario(res.scenario)
        .collect();
    assert_eq!(rows.len(), 5);
    assert_eq!(
        rows_for(&w, res.scenario, "Landlord"),
        vec![(d("2026-04-01"), m("-1500"))]
    );
    assert_eq!(
        rows_for(&w, res.scenario, "Expense: Groceries"),
        vec![(d("2026-03-31"), m("-600")), (d("2026-04-30"), m("-600"))]
    );
    assert_eq!(
        rows_for(&w, res.scenario, "Acme Payroll"),
        vec![(d("2026-03-15"), m("4000")), (d("2026-04-15"), m("4000"))]
    );
    for t in &rows {
        assert_eq!(t.memo.as_deref(), Some("Auto-generated from Budget"));
        assert_eq!(t.status, TransactionStatus::Future);
        assert_eq!(t.scenario_id, Some(res.scenario));
        assert_eq!(t.account_id, Some(checking));
    }
    // Groceries carries its category.
    let groceries_row = rows.iter().find(|t| t.payee == "Expense: Groceries");
    assert_eq!(
        groceries_row.and_then(|t| t.category_id),
        Some(w.category("Groceries"))
    );
}

#[test]
/// R-PRJ-7 — Covers: R-PRJ-7
fn r_prj_7_new_scenario_created_in_same_step() {
    // Given
    let mut w = World::with_fixture("world");
    let landlord = w.budget_item("Landlord");
    // When: the name has surrounding spaces.
    let res = generate_projection(
        &mut w.engine,
        ProjectionRequest {
            target: ProjectionTarget::New("  Q2 Plan ".into()),
            start: d("2026-04-01"),
            end: d("2026-06-30"),
            items: vec![landlord],
        },
    )
    .expect("generate");
    // Then: the scenario is named trimmed.
    assert_eq!(w.scenario("Q2 Plan"), res.scenario);
    assert_eq!(w.engine.undo_name(), Some("Generate Projection"));
    // One undo removes the scenario and its rows.
    w.engine.undo().expect("undo");
    assert!(w.engine.db().scenario_by_name("Q2 Plan").is_none());
    assert_eq!(
        w.engine
            .db()
            .live_transactions()
            .filter(|t| t.scenario_id == Some(res.scenario))
            .count(),
        0
    );
}

#[test]
/// R-PRJ-7 — Covers: R-PRJ-7
fn r_prj_7_empty_new_name_rejected() {
    // Given
    let mut w = World::with_fixture("world");
    let landlord = w.budget_item("Landlord");
    // When
    let err = generate_projection(
        &mut w.engine,
        ProjectionRequest {
            target: ProjectionTarget::New("   ".into()),
            start: d("2026-04-01"),
            end: d("2026-06-30"),
            items: vec![landlord],
        },
    )
    .expect_err("empty name");
    // Then
    assert_eq!(
        err,
        EngineError::Validation("Scenario name is required".into())
    );
    assert!(!w.engine.can_undo());
}

#[test]
/// R-PRJ-8 — Covers: R-PRJ-8
fn r_prj_8_settings_saved_and_prefilled() {
    // Given: nothing projected yet.
    let mut w = World::with_fixture("world");
    assert_eq!(last_projection(w.engine.db()), (None, None, None));
    let plan_a = w.scenario("Plan A");
    let landlord = w.budget_item("Landlord");
    // When
    generate_projection(
        &mut w.engine,
        ProjectionRequest {
            target: ProjectionTarget::Existing(plan_a),
            start: d("2026-05-01"),
            end: d("2026-07-31"),
            items: vec![landlord],
        },
    )
    .expect("generate");
    // Then
    let expected = (Some(plan_a), Some(d("2026-05-01")), Some(d("2026-07-31")));
    assert_eq!(last_projection(w.engine.db()), expected);
    w.reload();
    assert_eq!(last_projection(w.engine.db()), expected);
}

#[test]
/// BEH-BUD-01 — Covers: BEH-BUD-01, R-PRJ-3, R-PRJ-4
fn beh_bud_01_projection_exact_dates() {
    // Given
    let mut w = World::with_fixture("world");
    let checking = w.account("Checking");
    let before = w.reality_balance(checking);
    assert_eq!(before, m("2051.95"));
    // When
    let next_year = generate_next_year(&mut w);
    // Then: exact dates per item.
    assert_eq!(
        rows_for(&w, next_year, "Landlord")
            .iter()
            .map(|r| r.0)
            .collect::<Vec<_>>(),
        dates(&[
            "2026-04-01",
            "2026-05-01",
            "2026-06-01",
            "2026-07-01",
            "2026-08-01",
            "2026-09-01",
            "2026-10-01",
            "2026-11-01",
            "2026-12-01",
            "2027-01-01",
            "2027-02-01",
            "2027-03-01",
        ])
    );
    assert!(
        rows_for(&w, next_year, "Landlord")
            .iter()
            .all(|r| r.1 == m("-1500"))
    );
    assert_eq!(
        rows_for(&w, next_year, "Acme Payroll")
            .iter()
            .map(|r| r.0)
            .collect::<Vec<_>>(),
        dates(&[
            "2026-03-15",
            "2026-04-15",
            "2026-05-15",
            "2026-06-15",
            "2026-07-15",
            "2026-08-15",
            "2026-09-15",
            "2026-10-15",
            "2026-11-15",
            "2026-12-15",
            "2027-01-15",
            "2027-02-15",
        ])
    );
    assert!(
        rows_for(&w, next_year, "Acme Payroll")
            .iter()
            .all(|r| r.1 == m("4000"))
    );
    assert_eq!(
        rows_for(&w, next_year, "Expense: Groceries")
            .iter()
            .map(|r| r.0)
            .collect::<Vec<_>>(),
        dates(&[
            "2026-03-31",
            "2026-04-30",
            "2026-05-31",
            "2026-06-30",
            "2026-07-31",
            "2026-08-31",
            "2026-09-30",
            "2026-10-31",
            "2026-11-30",
            "2026-12-31",
            "2027-01-31",
            "2027-02-28",
        ])
    );
    assert_eq!(
        rows_for(&w, next_year, "Car Insurance")
            .iter()
            .map(|r| r.0)
            .collect::<Vec<_>>(),
        dates(&["2026-05-15", "2026-08-15", "2026-11-15", "2027-02-15"])
    );
    assert_eq!(
        rows_for(&w, next_year, "Property Tax")
            .iter()
            .map(|r| r.0)
            .collect::<Vec<_>>(),
        dates(&["2026-04-30", "2026-10-30"])
    );
    // Reality is untouched.
    assert_eq!(w.reality_balance(checking), before);
}

#[test]
/// BEH-BUD-02 — Covers: BEH-BUD-02, R-PRJ-8
fn beh_bud_02_projection_settings_prefill() {
    // Given
    let mut w = World::with_fixture("world");
    let next_year = generate_next_year(&mut w);
    // Then
    let expected = (
        Some(next_year),
        Some(d("2026-03-15")),
        Some(d("2027-03-14")),
    );
    assert_eq!(last_projection(w.engine.db()), expected);
    w.reload();
    assert_eq!(last_projection(w.engine.db()), expected);
}

#[test]
/// BEH-BUD-03 — Covers: BEH-BUD-03, R-PRJ-9
fn beh_bud_03_reproject_one_item_others_untouched() {
    // Given
    let mut w = World::with_fixture("world");
    let next_year = generate_next_year(&mut w);
    let landlord = w.budget_item("Landlord");
    let groceries_before = rows_for(&w, next_year, "Expense: Groceries");
    let old_landlord = rows_for(&w, next_year, "Landlord");
    // When: edit the amount to 1550 and reproject.
    let mut input = input_of(w.engine.db(), landlord);
    input.amount = m("1550");
    let toast = reproject_item(&mut w.engine, landlord, input).expect("reproject");
    // Then: additive, 12 new rows at -1550 beside the 12 at -1500.
    assert_eq!(toast, "Added 12 transactions to Next Year");
    assert_eq!(w.engine.undo_name(), Some("Reproject"));
    let landlord_rows = rows_for(&w, next_year, "Landlord");
    assert_eq!(landlord_rows.len(), 24);
    assert_eq!(
        landlord_rows.iter().filter(|r| r.1 == m("-1550")).count(),
        12
    );
    assert_eq!(
        landlord_rows.iter().filter(|r| r.1 == m("-1500")).count(),
        12
    );
    assert_eq!(
        rows_for(&w, next_year, "Expense: Groceries"),
        groceries_before,
        "other items are unchanged"
    );
    // One undo removes only the 12 new rows and restores the amount.
    w.engine.undo().expect("undo");
    assert_eq!(rows_for(&w, next_year, "Landlord"), old_landlord);
    assert_eq!(
        w.engine.db().budget_item(landlord).map(|b| b.amount),
        Some(m("1500"))
    );
}

#[test]
/// R-PRJ-9 — Covers: R-PRJ-9
fn r_prj_9_reproject_adds_rows_and_saves_edits() {
    // Given: a projection of Landlord for April and May.
    let mut w = World::with_fixture("world");
    let plan_a = w.scenario("Plan A");
    let landlord = w.budget_item("Landlord");
    generate_projection(
        &mut w.engine,
        ProjectionRequest {
            target: ProjectionTarget::Existing(plan_a),
            start: d("2026-04-01"),
            end: d("2026-05-31"),
            items: vec![landlord],
        },
    )
    .expect("generate");
    // The world already has one Landlord row in Plan A on 2026-04-01 at -1500.
    let old = rows_for(&w, plan_a, "Landlord");
    // When: edit the amount to 1600, then reproject.
    let mut input = input_of(w.engine.db(), landlord);
    input.amount = m("1600");
    reproject_item(&mut w.engine, landlord, input).expect("reproject");
    // Then
    assert_eq!(
        w.engine.db().budget_item(landlord).map(|b| b.amount),
        Some(m("1600"))
    );
    let now = rows_for(&w, plan_a, "Landlord");
    assert!(old.iter().all(|r| now.contains(r)), "old rows are kept");
    assert_eq!(
        now.iter().filter(|r| r.1 == m("-1600")).count(),
        2,
        "April and May get new rows at -1600"
    );
}

#[test]
/// R-PRJ-9 — Covers: R-PRJ-9
fn r_prj_9_reproject_without_prior_projection_rejected() {
    // Given: no projection has been run.
    let mut w = World::with_fixture("world");
    let landlord = w.budget_item("Landlord");
    let input = input_of(w.engine.db(), landlord);
    // When
    let err = reproject_item(&mut w.engine, landlord, input).expect_err("no projection");
    // Then
    assert_eq!(
        err,
        EngineError::Validation("Run a projection first".into())
    );
    assert!(!w.engine.can_undo());
}

#[test]
/// R-PRJ-9 — Covers: R-PRJ-9
fn r_prj_9_toast_singular_plural() {
    // Given
    let mut w = World::with_fixture("world");
    let plan_a = w.scenario("Plan A");
    let landlord = w.budget_item("Landlord");
    // When: a range with exactly one Landlord date (April 1).
    generate_projection(
        &mut w.engine,
        ProjectionRequest {
            target: ProjectionTarget::Existing(plan_a),
            start: d("2026-04-01"),
            end: d("2026-04-30"),
            items: vec![landlord],
        },
    )
    .expect("generate");
    let input = input_of(w.engine.db(), landlord);
    let one = reproject_item(&mut w.engine, landlord, input.clone()).expect("reproject");
    // Then
    assert_eq!(one, "Added 1 transaction to Plan A");
    // When: a range with two dates (April and May).
    generate_projection(
        &mut w.engine,
        ProjectionRequest {
            target: ProjectionTarget::Existing(plan_a),
            start: d("2026-04-01"),
            end: d("2026-05-31"),
            items: vec![landlord],
        },
    )
    .expect("generate");
    let two = reproject_item(&mut w.engine, landlord, input).expect("reproject");
    // Then
    assert_eq!(two, "Added 2 transactions to Plan A");
}
