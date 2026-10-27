//! L3 behaviour tests for undo and restart (T09: BEH-UND-02, BEH-PRG-01). Driven through the public
//! engine API on the `world` fixture, as the UI would call it.

use pf_engine::RowKey;
use pf_engine::testing::World;

#[test]
/// BEH-UND-02 — Covers: BEH-UND-02, R-UND-3
fn beh_und_02_new_edit_after_undo_clears_redo() {
    // Given
    let mut w = World::with_fixture("world");
    let starbucks = w.tx_by_payee("Starbucks");
    // When: soft-delete "Starbucks", undo, then rename the Landlord rows.
    w.engine
        .apply("Delete Transaction", |e| {
            e.set_deleted(RowKey::Transaction(starbucks), true)
        })
        .expect("delete");
    w.engine.undo().expect("undo");
    let landlords: Vec<_> = w
        .engine
        .db()
        .live_transactions()
        .filter(|t| t.payee == "Landlord")
        .map(|t| t.id)
        .collect();
    assert_eq!(landlords.len(), 3, "world fixture has three Landlord rows");
    w.engine
        .apply("Change Payee", |e| {
            for id in &landlords {
                e.update_transaction(*id, |t| t.payee = "Rent".into())?;
            }
            Ok(())
        })
        .expect("rename");
    // Then
    assert!(!w.engine.can_redo());
    assert_eq!(w.tx_by_payee("Starbucks"), starbucks);
}

#[test]
/// BEH-PRG-01 — Covers: BEH-PRG-01, R-PRG-1
fn beh_prg_01_soft_deleted_rows_purged_on_restart() {
    // Given: "Hotel Maui" carries a context link to Vacation.
    let mut w = World::with_fixture("world");
    let hotel = w.tx_by_payee("Hotel Maui");
    assert_eq!(link_count(&w, Some(hotel.raw())), 1);
    assert_eq!(
        link_count(&w, None),
        2,
        "County Tax Office and Hotel Maui are tagged"
    );
    // When: soft-delete it and restart.
    w.engine
        .apply("Delete Transaction", |e| {
            e.set_deleted(RowKey::Transaction(hotel), true)
        })
        .expect("delete");
    w.reload();
    // Then: the row is gone, not just flagged; no dangling link; nothing to undo.
    assert!(w.engine.db().transaction(hotel).is_none());
    assert_eq!(link_count(&w, Some(hotel.raw())), 0);
    assert_eq!(link_count(&w, None), 1, "only the County Tax link remains");
    assert_eq!(dangling_links(&w), 0);
    assert!(!w.engine.can_undo());
}

/// Counts `transaction_contexts` rows, for one transaction or for all. A raw query helper,
/// because the engine exposes no SQL: the test reads the SQLite file directly.
fn link_count(w: &World, transaction: Option<i64>) -> i64 {
    let conn = rusqlite::Connection::open(w.path()).expect("open sqlite file");
    match transaction {
        Some(id) => conn.query_row(
            "SELECT COUNT(*) FROM transaction_contexts WHERE transaction_id = ?1",
            [id],
            |r| r.get(0),
        ),
        None => conn.query_row("SELECT COUNT(*) FROM transaction_contexts", [], |r| {
            r.get(0)
        }),
    }
    .expect("count links")
}

/// Links whose transaction no longer exists (R-PRG-1 must leave none).
fn dangling_links(w: &World) -> i64 {
    let conn = rusqlite::Connection::open(w.path()).expect("open sqlite file");
    conn.query_row(
        "SELECT COUNT(*) FROM transaction_contexts
         WHERE transaction_id NOT IN (SELECT id FROM transactions)",
        [],
        |r| r.get(0),
    )
    .expect("count dangling")
}
