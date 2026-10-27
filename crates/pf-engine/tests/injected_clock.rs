//! Time-zone and clock injection (T10c follow-up). Every engine read of "now" and of the local
//! time zone comes from the engine, so these tests pin both and check the outputs. They must pass
//! under any machine `TZ`. Driven through the public engine API.

use jiff::tz::TimeZone;
use jiff::{Timestamp, Zoned};
use pf_engine::commands::accounts::{AccountInput, create_account};
use pf_engine::commands::backup::{export_markdown, import_markdown_replacing, perform_backup};
use pf_engine::commands::obligations::{
    ContextChoice, NewObligation, ObligationField, create_obligation, edit_obligation,
};
use pf_engine::testing::{TEST_TODAY, World, m, test_now, test_tz};
use pf_model::{AccountType, ObligationStatus, TransactionStatus};
use tempfile::TempDir;

/// Reads `fixtures/world.md`, the Markdown backup the world fixture is built from.
fn world_markdown() -> String {
    let p = format!("{}/../../fixtures/world.md", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("fixture {p}: {e}"))
}

/// A zone with no daylight saving and a large positive offset (UTC+9).
fn tokyo() -> TimeZone {
    TimeZone::get("Asia/Tokyo").expect("system tzdb has Asia/Tokyo")
}

/// A zone a day behind New York for the Markdown dates (UTC-7 in March 2026).
fn los_angeles() -> TimeZone {
    TimeZone::get("America/Los_Angeles").expect("system tzdb has America/Los_Angeles")
}

/// A timestamp literal in tests.
fn ts(s: &str) -> Timestamp {
    s.parse().expect("timestamp literal")
}

/// Form values for a new obligation on `account` with no target and no initial funds.
fn new_ob(name: &str, account: pf_model::AccountId) -> NewObligation {
    NewObligation {
        name: name.into(),
        target_amount: None,
        current_balance: m("0"),
        account,
        context: ContextChoice::None,
    }
}

#[test]
/// Covers: injected clock (obligation created and updated dates come from `Engine::now`)
fn obligation_dates_come_from_injected_clock() {
    // Given: a world whose clock is pinned to `test_now()`, and a checking account.
    let mut w = World::new();
    let chk = create_account(
        &mut w.engine,
        AccountInput {
            name: "Checking".into(),
            account_type: AccountType::Checking,
            institution: "Bank".into(),
            currency: String::new(),
            is_external: false,
        },
    )
    .expect("create account");
    // When: an obligation is created.
    let id = create_obligation(&mut w.engine, new_ob("Repairs", chk)).expect("create");
    // Then: both stamps are the injected instant.
    let created = w.engine.db().obligation(id).expect("row").clone();
    assert_eq!(created.created_date, Some(test_now()));
    assert_eq!(created.last_updated_date, Some(test_now()));
    // When: the clock moves on and the status changes.
    let later = ts("2030-06-01T12:34:56Z");
    w.engine.set_now_clock(Box::new(move || later));
    edit_obligation(
        &mut w.engine,
        id,
        ObligationField::Status(ObligationStatus::Completed),
    )
    .expect("edit");
    // Then: only the update stamp moves, to the injected instant.
    let edited = w.engine.db().obligation(id).expect("row");
    assert_eq!(edited.created_date, Some(test_now()));
    assert_eq!(edited.last_updated_date, Some(later));
}

#[test]
/// R-MD-7 — Covers: backup file name from the injected clock and time zone
fn backup_file_name_comes_from_injected_clock_and_tz() {
    // Given: Tokyo time zone, and the instant 14:05:07 UTC on the test day (23:05:07 in Tokyo).
    let mut w = World::new();
    w.engine.set_tz(tokyo());
    w.engine
        .set_now_clock(Box::new(|| ts("2026-03-15T14:05:07Z")));
    let dir = TempDir::new().expect("temp dir");
    // When
    let path = perform_backup(&w.engine, dir.path()).expect("backup");
    // Then: the name uses Tokyo wall-clock time, not the machine's zone.
    assert_eq!(
        path.file_name().and_then(|n| n.to_str()),
        Some("PFBackup_2026-03-15_23-05-07.md")
    );
}

#[test]
/// R-MD-1 — Covers: Markdown export dates use the injected time zone
fn markdown_export_uses_injected_tz() {
    // Given: a cleared deposit dated 2026-03-15 on a new account.
    let mut w = World::new();
    let acc = create_account(
        &mut w.engine,
        AccountInput {
            name: "Wallet".into(),
            account_type: AccountType::Checking,
            institution: "Bank".into(),
            currency: String::new(),
            is_external: false,
        },
    )
    .expect("create account");
    w.add_deposit(acc, "2026-03-15", "10", "Tz Probe");
    let probe_line = |md: &str| -> String {
        md.lines()
            .find(|l| l.contains("Tz Probe"))
            .expect("probe row in export")
            .to_string()
    };
    // Then: New York midnight is 04:00 UTC (EDT), the test zone.
    let ny = probe_line(&export_markdown(&w.engine));
    assert!(ny.contains("2026-03-15T04:00:00Z"), "{ny}");
    // When: the engine is moved to Tokyo.
    w.engine.set_tz(tokyo());
    // Then: Tokyo midnight is 15:00 UTC on the previous day.
    let jp = probe_line(&export_markdown(&w.engine));
    assert!(jp.contains("2026-03-14T15:00:00Z"), "{jp}");
    assert!(!jp.contains("2026-03-15T04:00:00Z"), "{jp}");
}

#[test]
/// R-MD-2 — Covers: Markdown import reads dates in the injected time zone
fn markdown_import_uses_injected_tz() {
    // The fixture's "Landlord" future row is `2026-04-01T04:00:00Z`: April 1 in New York,
    // March 31 in Los Angeles. The engine's zone decides which day it lands on.
    let landlord_future = |w: &World| {
        w.engine
            .db()
            .live_transactions()
            .find(|t| t.payee == "Landlord" && t.status == TransactionStatus::Future)
            .map(|t| t.date)
            .expect("future landlord row")
    };
    // Given: the engine in New York (the test zone).
    let mut ny = World::new();
    import_markdown_replacing(&mut ny.engine, &world_markdown(), &mut |_| {}).expect("import");
    // Then
    assert_eq!(landlord_future(&ny), "2026-04-01".parse().expect("date"));
    // Given: the engine in Los Angeles.
    let mut la = World::new();
    la.engine.set_tz(los_angeles());
    import_markdown_replacing(&mut la.engine, &world_markdown(), &mut |_| {}).expect("import");
    // Then
    assert_eq!(landlord_future(&la), "2026-03-31".parse().expect("date"));
}

#[test]
/// R-MD-2 — Covers: import fills a missing obligation stamp with noon of today in the injected zone
fn markdown_import_fallback_stamp_uses_injected_tz() {
    // Given: the fixture with the Vacation Fund's created and updated dates removed.
    let text = world_markdown();
    let old = "| 2026-01-01T05:00:00Z | 2026-01-01T05:00:00Z |  | Savings | Vacation |";
    assert_eq!(text.matches(old).count(), 1, "fixture row changed");
    let text = text.replace(old, "|  |  |  | Savings | Vacation |");
    let mut w = World::new();
    w.engine.set_tz(los_angeles());
    // When
    import_markdown_replacing(&mut w.engine, &text, &mut |_| {}).expect("import");
    // Then: noon on TEST_TODAY in Los Angeles (UTC-7) is 19:00 UTC.
    let vacation = w
        .engine
        .db()
        .live_obligations()
        .find(|o| o.name == "Vacation Fund")
        .expect("vacation fund");
    let want = TEST_TODAY
        .at(12, 0, 0, 0)
        .to_zoned(los_angeles())
        .expect("noon")
        .timestamp();
    assert_eq!(vacation.created_date, Some(want));
    assert_eq!(vacation.last_updated_date, Some(want));
    assert_eq!(want, ts("2026-03-15T19:00:00Z"));
}

#[test]
/// Covers: test world pins the zone and clock the tests rely on
fn world_pins_test_zone_and_clock() {
    let w = World::new();
    assert_eq!(w.engine.tz(), &test_tz());
    assert_eq!(w.engine.now(), test_now());
    assert_eq!(test_now(), ts("2026-03-15T13:05:07Z"));
    // The engine's clock and zone give the same local day as `today()`.
    let local: Zoned = w.engine.now().to_zoned(w.engine.tz().clone());
    assert_eq!(local.date(), w.engine.today());
}
