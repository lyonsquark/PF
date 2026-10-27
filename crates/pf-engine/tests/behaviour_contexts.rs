//! L3 behaviour and rule tests for contexts (T10b: BEH-CTX-01..02, R-CTX-1..3).
//! Driven through the public engine API on the `world` fixture, as the UI would call it.

use pf_engine::EngineError;
use pf_engine::commands::contexts::{
    ContextInput, contexts_by_section, create_context, delete_context, set_context_archived,
    update_context,
};
use pf_engine::commands::obligations::delete_obligations;
use pf_engine::testing::World;
use pf_model::testing::TestDb;
use pf_model::{ObligationStatus, RowKey};

/// Form values for a context with no colour and no notes.
fn input(name: &str) -> ContextInput {
    ContextInput {
        name: name.into(),
        color_hex: None,
        notes: None,
    }
}

/// The exact R-CTX-1 message for a context and the obligation that blocks it.
fn blocked_message(ctx: &str, ob: &str) -> EngineError {
    EngineError::Validation(format!(
        "Cannot delete context '#{ctx}' because it is currently assigned to the obligation '{ob}'. Please change the obligation's context or delete the obligation first."
    ))
}

#[test]
/// R-CTX-1 — Covers: R-CTX-1
fn r_ctx_1_context_used_by_obligation_cannot_be_deleted() {
    // Given
    let mut w = World::with_fixture("world");
    let taxes = w.context("Taxes");
    let revision = w.engine.revision();
    // When / Then
    let err = delete_context(&mut w.engine, taxes).expect_err("blocked");
    assert_eq!(err, blocked_message("Taxes", "Property Tax"));
    assert!(w.engine.db().context(taxes).is_some_and(|c| !c.is_deleted));
    assert_eq!(w.engine.revision(), revision);
}

#[test]
/// R-CTX-1 — Covers: R-CTX-1 (a Completed obligation is still a live obligation)
fn r_ctx_1_completed_but_live_obligation_still_blocks() {
    // Given: Vacation Fund is Completed and still live
    let mut w = World::with_fixture("world");
    let vacation = w.context("Vacation");
    assert_eq!(
        w.engine
            .db()
            .obligation(w.obligation("Vacation Fund"))
            .map(|o| o.status),
        Some(ObligationStatus::Completed)
    );
    // When / Then
    let err = delete_context(&mut w.engine, vacation).expect_err("blocked");
    assert_eq!(err, blocked_message("Vacation", "Vacation Fund"));
}

#[test]
/// R-CTX-2 — Covers: R-CTX-2
fn r_ctx_2_delete_removes_from_transactions() {
    // Given: the obligation that uses Vacation is deleted first
    let mut w = World::with_fixture("world");
    let fund = w.obligation("Vacation Fund");
    delete_obligations(&mut w.engine, &[fund]).expect("delete obligation");
    let vacation = w.context("Vacation");
    let hotel = w.tx_by_payee("Hotel Maui");
    assert_eq!(
        w.engine
            .db()
            .transaction(hotel)
            .map(|t| t.context_ids.clone()),
        Some(vec![vacation])
    );
    // When
    delete_context(&mut w.engine, vacation).expect("delete context");
    // Then
    assert!(
        w.engine
            .db()
            .transaction(hotel)
            .is_some_and(|t| t.context_ids.is_empty())
    );
    assert!(
        w.engine
            .db()
            .context(vacation)
            .is_some_and(|c| c.is_deleted)
    );
}

#[test]
/// R-CTX-3 — Covers: R-CTX-3
fn r_ctx_3_sections() {
    // Given
    let mut w = World::with_fixture("world");
    let taxes = w.context("Taxes");
    let vacation = w.context("Vacation");
    // When
    set_context_archived(&mut w.engine, taxes, true).expect("archive");
    // Then
    let (active, archived) = contexts_by_section(w.engine.db());
    assert_eq!(archived, vec![taxes]);
    assert_eq!(active, vec![vacation]);
    // When: unarchived again
    set_context_archived(&mut w.engine, taxes, false).expect("unarchive");
    let (active, archived) = contexts_by_section(w.engine.db());
    assert!(archived.is_empty());
    assert_eq!(active, vec![taxes, vacation]);
}

#[test]
/// BEH-CTX-01 — Covers: BEH-CTX-01, R-CTX-1
fn beh_ctx_01_delete_context_used_by_obligation_refused() {
    // Given
    let mut w = World::with_fixture("world");
    let taxes = w.context("Taxes");
    let revision = w.engine.revision();
    // When
    let err = delete_context(&mut w.engine, taxes).expect_err("refused");
    // Then: exact message, nothing changed, no undo step
    assert_eq!(err, blocked_message("Taxes", "Property Tax"));
    assert_eq!(w.engine.revision(), revision);
    assert!(!w.engine.can_undo());
    assert!(w.engine.db().context(taxes).is_some_and(|c| !c.is_deleted));
}

#[test]
/// BEH-CTX-02 — Covers: BEH-CTX-02, R-CTX-2
fn beh_ctx_02_delete_context_undo_restores_links() {
    // Given: Vacation Fund deleted, so Vacation is free to delete
    let mut w = World::with_fixture("world");
    let fund = w.obligation("Vacation Fund");
    delete_obligations(&mut w.engine, &[fund]).expect("delete obligation");
    let vacation = w.context("Vacation");
    let hotel = w.tx_by_payee("Hotel Maui");
    // When
    delete_context(&mut w.engine, vacation).expect("delete context");
    // Then
    assert!(
        w.engine
            .db()
            .transaction(hotel)
            .is_some_and(|t| t.context_ids.is_empty())
    );
    // When: undo
    assert_eq!(
        w.engine.undo().expect("undo"),
        Some("Delete Context".into())
    );
    // Then: the context and its link are back on the same transaction
    assert!(
        w.engine
            .db()
            .context(vacation)
            .is_some_and(|c| !c.is_deleted)
    );
    assert_eq!(
        w.engine
            .db()
            .transaction(hotel)
            .map(|t| t.context_ids.clone()),
        Some(vec![vacation])
    );
}

#[test]
/// Covers: R-CTX-4 (name required, trimmed; edit keeps the same validation)
fn context_name_required_and_trimmed() {
    // Given
    let mut w = World::with_fixture("world");
    // Then: a blank name is refused
    let err = create_context(&mut w.engine, input("  ")).expect_err("blank");
    assert_eq!(
        err,
        EngineError::Validation("Context name is required".into())
    );
    // When: a valid create and edit
    let id = create_context(&mut w.engine, input("  Work  ")).expect("create");
    update_context(
        &mut w.engine,
        id,
        ContextInput {
            name: " Office ".into(),
            color_hex: Some("#123456".into()),
            notes: Some("  ".into()),
        },
    )
    .expect("edit");
    // Then
    let row = w.engine.db().context(id).expect("row");
    assert_eq!(row.name, "Office");
    assert_eq!(row.color_hex.as_deref(), Some("#123456"));
    assert_eq!(row.notes, None);
    let err = update_context(&mut w.engine, id, input("")).expect_err("blank edit");
    assert_eq!(
        err,
        EngineError::Validation("Context name is required".into())
    );
}

#[test]
/// Unit: contexts_by_section over TestDb (R-CTX-3): live only, split by archive flag, by name.
fn contexts_by_section_splits_and_sorts() {
    // Given
    let mut t = TestDb::new();
    let beta = t.context("beta");
    let alpha = t.context("Alpha");
    let zeta = t.context("Zeta");
    let gone = t.context("Gone");
    t.delete(RowKey::Context(gone));
    let mut db = t.build();
    db.contexts.get_mut(&zeta).expect("zeta").is_archived = true;
    // When
    let (active, archived) = contexts_by_section(&db);
    // Then
    assert_eq!(active, vec![alpha, beta]);
    assert_eq!(archived, vec![zeta]);
}
