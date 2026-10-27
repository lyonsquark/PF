//! L3 behaviour and rule tests for scenarios (T10b: BEH-SCN-01..03, R-SCN-1..4, R-SCN-3 deletes).
//! Driven through the public engine API on the `world` fixture, as the UI would call it.

use pf_engine::EngineError;
use pf_engine::commands::contexts::{ContextInput, create_context};
use pf_engine::commands::scenarios::{
    ScenarioInput, create_scenario, delete_scenario, update_scenario,
};
use pf_engine::testing::{World, m};
use pf_model::TransactionId;

/// Form values for a scenario.
fn input(name: &str, color: Option<&str>, notes: Option<&str>) -> ScenarioInput {
    ScenarioInput {
        name: name.into(),
        color_hex: color.map(Into::into),
        notes: notes.map(Into::into),
    }
}

/// The 2026-04-01 "Landlord" row of the fixture that belongs to Plan A.
fn plan_a_landlord(w: &World) -> TransactionId {
    let plan = w.scenario("Plan A");
    w.engine
        .db()
        .live_transactions()
        .find(|t| t.payee == "Landlord" && t.scenario_id == Some(plan))
        .map(|t| t.id)
        .expect("fixture has the Plan A landlord row")
}

#[test]
/// R-SCN-3 — Covers: R-SCN-3
fn r_scn_3_delete_active_scenario_deactivates() {
    // Given
    let mut w = World::with_fixture("world");
    let plan = w.scenario("Plan A");
    w.engine.set_active_scenario(Some(plan));
    // When
    delete_scenario(&mut w.engine, plan).expect("delete");
    // Then
    assert_eq!(w.engine.state.active_scenario, None);
}

#[test]
/// R-SCN-3 — Covers: R-SCN-3
fn r_scn_3_delete_scenario_hides_its_transactions() {
    // Given
    let mut w = World::with_fixture("world");
    let row = plan_a_landlord(&w);
    let plan = w.scenario("Plan A");
    // When
    delete_scenario(&mut w.engine, plan).expect("delete");
    // Then
    let landlord = w
        .engine
        .db()
        .transaction(row)
        .expect("row kept until restart");
    assert!(landlord.is_deleted);
    assert!(w.engine.db().scenario(plan).is_some_and(|s| s.is_deleted));
}

#[test]
/// R-SCN-4 — Covers: R-SCN-4
fn r_scn_4_create_scenario_trims_name_and_empty_notes_none() {
    // Given
    let mut w = World::with_fixture("world");
    // When
    let id = create_scenario(&mut w.engine, input("  Plan B ", None, Some(""))).expect("create");
    // Then
    let row = w.engine.db().scenario(id).expect("created scenario");
    assert_eq!(row.name, "Plan B");
    assert_eq!(row.notes, None);
    // When the name is blank
    let live_before = w.engine.db().live_scenarios().count();
    let err = create_scenario(&mut w.engine, input("   ", None, None)).expect_err("blank name");
    // Then
    assert_eq!(
        err,
        EngineError::Validation("Scenario name is required".into())
    );
    assert_eq!(w.engine.db().live_scenarios().count(), live_before);
}

#[test]
/// R-SCN-4 — Covers: R-SCN-4
fn edit_scenario_requires_a_name_and_trims() {
    // Given
    let mut w = World::with_fixture("world");
    let plan = w.scenario("Plan A");
    // Then: a blank name is refused and nothing changes
    let err = update_scenario(&mut w.engine, plan, input(" ", None, None)).expect_err("blank");
    assert_eq!(
        err,
        EngineError::Validation("Scenario name is required".into())
    );
    // When: a valid edit
    update_scenario(
        &mut w.engine,
        plan,
        input(" Plan A2 ", Some("#112233"), Some(" x ")),
    )
    .expect("edit");
    // Then
    let row = w.engine.db().scenario(plan).expect("row");
    assert_eq!(row.name, "Plan A2");
    assert_eq!(row.color_hex.as_deref(), Some("#112233"));
    assert_eq!(row.notes.as_deref(), Some("x"));
}

#[test]
/// Defaults: a scenario gets #800080 and a context gets #0000FF when no colour is given (Swift
/// `Scenario` and `ContextsView` defaults). Covers: R-SCN-4, R-CTX-4
fn defaults_colors() {
    // Given
    let mut w = World::with_fixture("world");
    // When
    let s = create_scenario(&mut w.engine, input("Plan C", None, None)).expect("create");
    let c = create_context(
        &mut w.engine,
        ContextInput {
            name: "Work".into(),
            color_hex: None,
            notes: None,
        },
    )
    .expect("create context");
    // Then
    let scenario_color = w.engine.db().scenario(s).and_then(|r| r.color_hex.clone());
    assert_eq!(scenario_color.as_deref(), Some("#800080"));
    let context_color = w.engine.db().context(c).and_then(|r| r.color_hex.clone());
    assert_eq!(context_color.as_deref(), Some("#0000FF"));
}

#[test]
/// BEH-SCN-01 — Covers: BEH-SCN-01, R-SCN-2
fn beh_scn_01_toggle_twice_back_to_reality() {
    // Given
    let mut w = World::with_fixture("world");
    let plan = w.scenario("Plan A");
    // When
    w.engine.toggle_scenario(Some(plan));
    w.engine.toggle_scenario(Some(plan));
    // Then
    assert_eq!(w.engine.state.active_scenario, None);
    let chk = w.account("Checking");
    assert_eq!(w.balance(chk), m("2051.95"));
}

#[test]
/// BEH-SCN-02 — Covers: BEH-SCN-02, R-SCN-1, R-BAL-1, R-BAL-3
fn beh_scn_02_scenario_does_not_change_reality() {
    // Given
    let mut w = World::with_fixture("world");
    let chk = w.account("Checking");
    let reality = w.reality_balance(chk);
    assert_eq!(reality, m("2051.95"));
    // When: Plan A active, withdrawal 500 on 2026-04-01
    w.activate_scenario("Plan A");
    w.add_withdrawal(chk, "2026-04-01", "500", "Vacation");
    // Then: the scenario view includes the Plan A landlord row (-1500) and the new withdrawal
    assert_eq!(w.balance(chk), m("51.95"));
    assert_eq!(w.reality_balance(chk), m("2051.95"));
    // Reality ignores the scenario
    w.activate_reality();
    assert_eq!(w.balance(chk), m("2051.95"));
    // Persisted across a restart
    w.reload();
    assert_eq!(w.balance(chk), m("2051.95"));
}

#[test]
/// BEH-SCN-03 — Covers: BEH-SCN-03, R-SCN-3
fn beh_scn_03_delete_active_scenario_undo() {
    // Given: Plan A is active
    let mut w = World::with_fixture("world");
    let chk = w.account("Checking");
    let plan = w.scenario("Plan A");
    let row = plan_a_landlord(&w);
    w.activate_scenario("Plan A");
    // When
    delete_scenario(&mut w.engine, plan).expect("delete");
    // Then: deactivated, balance is reality, the row is hidden
    assert_eq!(w.engine.state.active_scenario, None);
    assert_eq!(w.balance(chk), m("2051.95"));
    assert!(w.engine.db().transaction(row).is_some_and(|t| t.is_deleted));
    // When: undo
    assert_eq!(
        w.engine.undo().expect("undo"),
        Some("Delete Scenario".into())
    );
    // Then: the scenario and its row are live again; still in Reality
    assert!(w.engine.db().scenario(plan).is_some_and(|s| !s.is_deleted));
    assert!(
        w.engine
            .db()
            .transaction(row)
            .is_some_and(|t| !t.is_deleted)
    );
    assert_eq!(w.engine.state.active_scenario, None);
}
