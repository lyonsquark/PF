//! L3 behaviour and rule tests for obligations (T10b: BEH-OBL-01..03, R-OBL-1..6).
//! Driven through the public engine API on the `world` fixture, as the UI would call it.

use pf_core::obligation::{available_balance, computed_balance, total_reserved};
use pf_engine::EngineError;
use pf_engine::commands::contexts::{ContextInput, create_context};
use pf_engine::commands::obligations::{
    ContextChoice, NewObligation, ObligationField, create_obligation, delete_obligations,
    display_balance, edit_obligation, obligations_by_status, suggest_context_name,
};
use pf_engine::commands::transactions::{
    NewTransaction, TxField, TxKind, add_transaction, edit_transaction,
};
use pf_engine::testing::{World, d, m};
use pf_model::testing::TestDb;
use pf_model::{
    AccountId, Db, ObligationId, ObligationStatus, RowKey, TransactionId, TransactionStatus,
};

/// Form values for a new obligation on `account` with no target and no initial funds.
fn new_ob(name: &str, account: AccountId, context: ContextChoice) -> NewObligation {
    NewObligation {
        name: name.into(),
        target_amount: None,
        current_balance: m("0"),
        account,
        context,
    }
}

/// The live reality row with this payee on this date. Panics if there is none.
fn tx_on(w: &World, payee: &str, date: &str) -> TransactionId {
    w.engine
        .db()
        .live_transactions()
        .find(|t| t.payee == payee && t.date == d(date) && t.scenario_id.is_none())
        .map(|t| t.id)
        .expect("fixture row")
}

/// Adds a Plan A withdrawal of 100 tagged Taxes on Checking (scenario must be active).
fn add_plan_a_tax(w: &mut World) {
    let chk = w.account("Checking");
    let taxes = w.context("Taxes");
    let form = NewTransaction {
        kind: TxKind::Withdrawal,
        account: chk,
        date: d("2026-03-01"),
        amount: m("100"),
        payee: "Plan A tax".into(),
        memo: String::new(),
        status: TransactionStatus::Pending,
        category: None,
        contexts: vec![taxes],
    };
    add_transaction(&mut w.engine, form).expect("add scenario transaction");
}

/// The Obligation id for a name in the fixture (live only).
fn ob_id(w: &World, name: &str) -> ObligationId {
    w.obligation(name)
}

#[test]
/// R-OBL-1 — Covers: R-OBL-1
fn r_obl_1_computed_balance_world() {
    // Given
    let w = World::with_fixture("world");
    let ob = w
        .engine
        .db()
        .obligation(ob_id(&w, "Property Tax"))
        .expect("row");
    // Then: 1000 funded, minus the 300 County Tax Office row tagged Taxes
    assert_eq!(computed_balance(w.engine.db(), ob, None), m("700"));
}

#[test]
/// R-OBL-1 — Covers: R-OBL-1
fn r_obl_1_scenario_tagged_tx_counts_only_when_active() {
    // Given
    let mut w = World::with_fixture("world");
    let plan = w.scenario("Plan A");
    let ob_row = ob_id(&w, "Property Tax");
    // When: a Plan A transaction tagged Taxes (-100) is added while Plan A is active
    w.activate_scenario("Plan A");
    add_plan_a_tax(&mut w);
    // Then
    let db = w.engine.db();
    let ob = db.obligation(ob_row).expect("row");
    assert_eq!(computed_balance(db, ob, None), m("700"));
    assert_eq!(computed_balance(db, ob, Some(plan)), m("600"));
}

#[test]
/// R-OBL-6 — Covers: R-OBL-6
fn r_obl_6_list_balance_uses_reality() {
    // Given: Plan A active with a scenario row tagged to the obligation's context
    let mut w = World::with_fixture("world");
    let ob_row = ob_id(&w, "Property Tax");
    w.activate_scenario("Plan A");
    add_plan_a_tax(&mut w);
    // Then: the list/detail balance is the Reality computed balance
    assert_eq!(display_balance(w.engine.db(), ob_row), m("700"));
    // And the scenario-aware balance really does differ
    let plan = w.scenario("Plan A");
    let ob = w.engine.db().obligation(ob_row).expect("row");
    assert_eq!(computed_balance(w.engine.db(), ob, Some(plan)), m("600"));
}

#[test]
/// R-OBL-2 — Covers: R-OBL-2
fn r_obl_2_only_funding_and_spending_reserved() {
    // Given
    let mut w = World::with_fixture("world");
    let chk = w.account("Checking");
    let sav = w.account("Savings");
    assert_eq!(total_reserved(w.engine.db(), chk, None), m("700"));
    // When: Vacation Fund (Completed, on Savings) becomes Spending
    let vacation_fund = ob_id(&w, "Vacation Fund");
    edit_obligation(
        &mut w.engine,
        vacation_fund,
        ObligationField::Status(ObligationStatus::Spending),
    )
    .expect("edit status");
    // Then: 2000 funded minus Hotel Maui 650 is 1350
    assert_eq!(total_reserved(w.engine.db(), sav, None), m("1350"));
}

#[test]
/// R-OBL-3 — Covers: R-OBL-3
fn r_obl_3_available_balance() {
    // Given
    let w = World::with_fixture("world");
    let chk = w.account("Checking");
    // When
    let total = w.balance(chk);
    let reserved = total_reserved(w.engine.db(), chk, None);
    // Then
    assert_eq!(total, m("2051.95"));
    assert_eq!(available_balance(total, reserved), m("1351.95"));
}

#[test]
/// R-OBL-4 — Covers: R-OBL-4
fn r_obl_4_suggest_context_name_dedupes() {
    // Given
    let mut w = World::with_fixture("world");
    // Then: no clash for "Property Tax"; "Taxes" clashes with the existing context
    assert_eq!(
        suggest_context_name(w.engine.db(), "Property Tax"),
        "property-tax"
    );
    assert_eq!(suggest_context_name(w.engine.db(), "Taxes"), "taxes-2");
    // When: a context "property-tax" exists
    create_context(
        &mut w.engine,
        ContextInput {
            name: "property-tax".into(),
            color_hex: None,
            notes: None,
        },
    )
    .expect("create context");
    // Then
    assert_eq!(
        suggest_context_name(w.engine.db(), "Property Tax"),
        "property-tax-2"
    );
}

#[test]
/// R-OBL-5 — Covers: R-OBL-5
fn r_obl_5_grouped_in_status_order() {
    // Given
    let w = World::with_fixture("world");
    // When
    let groups = obligations_by_status(w.engine.db());
    // Then: Funding (Property Tax) before Completed (Vacation Fund)
    let statuses: Vec<ObligationStatus> = groups.iter().map(|(s, _)| *s).collect();
    assert_eq!(
        statuses,
        vec![ObligationStatus::Funding, ObligationStatus::Completed]
    );
    assert_eq!(groups[0].1, vec![ob_id(&w, "Property Tax")]);
    assert_eq!(groups[1].1, vec![ob_id(&w, "Vacation Fund")]);
}

#[test]
/// BEH-OBL-01 — Covers: BEH-OBL-01, R-OBL-1, R-OBL-2, R-OBL-3
fn beh_obl_01_reserved_reduces_available() {
    // Given
    let mut w = World::with_fixture("world");
    let chk = w.account("Checking");
    let taxes = w.context("Taxes");
    // When: the 2026-02-01 Landlord rent (-1500) is tagged Taxes
    let landlord = tx_on(&w, "Landlord", "2026-02-01");
    edit_transaction(&mut w.engine, landlord, TxField::Contexts(vec![taxes])).expect("tag");
    // Then: computed 1000 - 300 - 1500 = -800; reserved is that; available grows
    let ob = w
        .engine
        .db()
        .obligation(ob_id(&w, "Property Tax"))
        .expect("row");
    assert_eq!(computed_balance(w.engine.db(), ob, None), m("-800"));
    let reserved = total_reserved(w.engine.db(), chk, None);
    assert_eq!(reserved, m("-800"));
    assert_eq!(available_balance(w.balance(chk), reserved), m("2851.95"));
}

#[test]
/// BEH-OBL-02 — Covers: BEH-OBL-02, R-OBL-2
fn beh_obl_02_completed_and_deferred_not_reserved() {
    // Given
    let mut w = World::with_fixture("world");
    let chk = w.account("Checking");
    let tax = ob_id(&w, "Property Tax");
    let before = w
        .engine
        .db()
        .obligation(tax)
        .and_then(|o| o.last_updated_date);
    // When: Deferred
    edit_obligation(
        &mut w.engine,
        tax,
        ObligationField::Status(ObligationStatus::Deferred),
    )
    .expect("defer");
    // Then: not reserved, and the last-updated stamp moved on
    assert_eq!(total_reserved(w.engine.db(), chk, None), m("0"));
    let after = w
        .engine
        .db()
        .obligation(tax)
        .and_then(|o| o.last_updated_date);
    assert_ne!(before, after);
    assert!(after.is_some());
    // When: Completed
    edit_obligation(
        &mut w.engine,
        tax,
        ObligationField::Status(ObligationStatus::Completed),
    )
    .expect("complete");
    assert_eq!(total_reserved(w.engine.db(), chk, None), m("0"));
    // When: Spending
    edit_obligation(
        &mut w.engine,
        tax,
        ObligationField::Status(ObligationStatus::Spending),
    )
    .expect("spend");
    assert_eq!(total_reserved(w.engine.db(), chk, None), m("700"));
}

#[test]
/// BEH-OBL-03 — Covers: BEH-OBL-03, R-OBL-4
fn beh_obl_03_new_obligation_creates_unique_context() {
    // Given
    let mut w = World::with_fixture("world");
    let chk = w.account("Checking");
    let name = suggest_context_name(w.engine.db(), "Taxes");
    // When: a new obligation "Taxes" with a new context named by the suggestion
    let ob = create_obligation(
        &mut w.engine,
        new_ob("Taxes", chk, ContextChoice::New(name)),
    )
    .expect("create");
    // Then: the context is "taxes-2" with the Swift notes
    let ctx = w
        .engine
        .db()
        .context_by_name("taxes-2")
        .expect("context created");
    assert_eq!(ctx.notes.as_deref(), Some("Taxes obligation"));
    let row = w.engine.db().obligation(ob).expect("obligation");
    assert_eq!(row.context_id, Some(ctx.id));
    assert_eq!(row.status, ObligationStatus::Funding);
    // When: undo removes both
    assert_eq!(
        w.engine.undo().expect("undo"),
        Some("Add Obligation".into())
    );
    assert!(w.engine.db().context_by_name("taxes-2").is_none());
    assert!(w.engine.db().obligation(ob).is_none());
}

#[test]
/// Covers: BEH-OBL-03 (create validation and an unchanged edit makes no undo step)
fn obligation_name_required_and_unchanged_edit_is_no_step() {
    // Given
    let mut w = World::with_fixture("world");
    let chk = w.account("Checking");
    // Then: blank name refused
    let err = create_obligation(&mut w.engine, new_ob("  ", chk, ContextChoice::None))
        .expect_err("blank");
    assert_eq!(
        err,
        EngineError::Validation("Obligation name is required".into())
    );
    // When: the same status is set again
    let tax = ob_id(&w, "Property Tax");
    edit_obligation(
        &mut w.engine,
        tax,
        ObligationField::Status(ObligationStatus::Funding),
    )
    .expect("no-op edit");
    // Then: no undo step
    assert!(!w.engine.can_undo());
}

#[test]
/// Covers: BEH-OBL-03 (delete obligations: plural name and one undo step)
fn delete_obligations_plural_name_and_undo() {
    // Given
    let mut w = World::with_fixture("world");
    let ids = vec![ob_id(&w, "Property Tax"), ob_id(&w, "Vacation Fund")];
    // When
    delete_obligations(&mut w.engine, &ids).expect("delete");
    // Then
    assert!(w.engine.db().live_obligations().next().is_none());
    assert_eq!(w.engine.undo_name(), Some("Delete Obligations"));
    // When: undo
    w.engine.undo().expect("undo");
    assert_eq!(w.engine.db().live_obligations().count(), 2);
}

#[test]
/// Unit: pure helpers over TestDb (R-OBL-4 and R-OBL-5 wrappers).
fn helpers_over_test_db() {
    // Given: a live "Taxes" context, a deleted "Car" context
    let mut t = TestDb::new();
    let chk = t.account("Checking", pf_model::AccountType::Checking);
    t.context("Taxes");
    let car = t.context("Car");
    t.delete(RowKey::Context(car));
    let done = t
        .obligation("Done")
        .status(ObligationStatus::Completed)
        .done();
    let amy = t.obligation("Amy").account(chk).done();
    let db: Db = t.build();
    // Then: the deleted context's name is free again
    assert_eq!(suggest_context_name(&db, "Car"), "car");
    assert_eq!(suggest_context_name(&db, "taxes"), "taxes-2");
    // Then: grouping order
    let groups = obligations_by_status(&db);
    assert_eq!(
        groups,
        vec![
            (ObligationStatus::Funding, vec![amy]),
            (ObligationStatus::Completed, vec![done])
        ]
    );
}
