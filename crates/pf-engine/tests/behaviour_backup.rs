//! L3 behaviour and rule tests for the Markdown backup, Cmd-S backup and the Danger Zone wipe
//! (T10c: BEH-MD-03, BEH-BAK-01, R-MD-5, R-MD-7). Driven through the public engine API.

use jiff::Zoned;
use pf_engine::commands::accounts::{AccountInput, create_account};
use pf_engine::commands::backup::{
    backup_filename, backup_folder, export_markdown, import_markdown_replacing, perform_backup,
    set_backup_folder, wipe_all_data,
};
use pf_engine::testing::World;
use pf_model::AccountType;
use tempfile::TempDir;

/// Reads `fixtures/world.md`, the Markdown backup the world fixture is built from.
fn world_markdown() -> String {
    let p = format!("{}/../../fixtures/world.md", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("fixture {p}: {e}"))
}

/// A checking-style account input with the given name.
fn account(name: &str) -> AccountInput {
    AccountInput {
        name: name.into(),
        account_type: AccountType::Checking,
        institution: "Bank".into(),
        currency: String::new(),
        is_external: false,
    }
}

/// 2026-03-15 09:05:07 in New York, the instant the backup tests use.
fn nine_oh_five() -> Zoned {
    "2026-03-15T09:05:07[America/New_York]"
        .parse()
        .expect("zoned literal")
}

#[test]
/// R-MD-5 — Covers: R-MD-5
fn r_md_5_import_replaces_and_clears_history() {
    // Given: an account created by a command, so there is an undo step.
    let mut w = World::with_fixture("world");
    create_account(&mut w.engine, account("Old")).expect("create");
    assert!(w.engine.can_undo());
    // When
    let mut progress = Vec::new();
    import_markdown_replacing(&mut w.engine, &world_markdown(), &mut |s| {
        progress.push(s.to_string())
    })
    .expect("import");
    // Then
    assert!(w.engine.db().account_by_name("Old").is_none());
    assert!(w.engine.db().account_by_name("Checking").is_some());
    assert!(w.engine.db().account_by_name("Visa").is_some());
    assert!(!w.engine.can_undo());
    assert!(progress.iter().any(|s| s == "Finalizing..."));
}

#[test]
/// BEH-MD-03 — Covers: BEH-MD-03, R-MD-5
fn beh_md_03_settings_import_wipes_and_clears_undo() {
    // Given: a world with an extra account created by a command.
    let mut w = World::new();
    create_account(&mut w.engine, account("Old")).expect("create");
    // When
    let counts =
        import_markdown_replacing(&mut w.engine, &world_markdown(), &mut |_| {}).expect("import");
    // Then
    assert!(w.engine.db().account_by_name("Old").is_none());
    assert_eq!(w.engine.db().live_accounts().count(), 5);
    assert_eq!(w.live_tx_count(), 14);
    assert!(!w.engine.can_undo());
    assert_eq!(counts.accounts, 5);
    assert_eq!(counts.transactions, 14);
    // Reload: the same data is on disk.
    w.reload();
    assert!(w.engine.db().account_by_name("Old").is_none());
    assert_eq!(w.engine.db().live_accounts().count(), 5);
    assert_eq!(w.live_tx_count(), 14);
}

#[test]
/// R-MD-7 — Covers: R-MD-7
fn r_md_7_backup_filename_format() {
    assert_eq!(
        backup_filename(&nine_oh_five()),
        "PFBackup_2026-03-15_09-05-07.md"
    );
}

#[test]
/// BEH-BAK-01 — Covers: BEH-BAK-01, R-MD-7
fn beh_bak_01_backup_writes_export() {
    // Given: a world and a temp folder set as the backup folder.
    let mut w = World::with_fixture("world");
    let dir = TempDir::new().expect("temp dir");
    set_backup_folder(&mut w.engine, dir.path()).expect("set folder");
    assert_eq!(backup_folder(&w.engine).as_deref(), Some(dir.path()));
    // When
    let path = perform_backup(&w.engine, dir.path()).expect("backup");
    // Then
    assert_eq!(
        path.file_name().and_then(|n| n.to_str()),
        Some("PFBackup_2026-03-15_09-05-07.md")
    );
    let written = std::fs::read_to_string(&path).expect("read backup");
    assert_eq!(written, export_markdown(&w.engine));
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .expect("list folder")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "no .tmp left: {leftovers:?}");
}

#[test]
/// Covers: R-MD-7 (never overwrite; a second backup in the same second gets "-2")
fn backup_never_overwrites_existing_file() {
    // Given: a backup already written for this second.
    let w = World::with_fixture("world");
    let dir = TempDir::new().expect("temp dir");
    let first = perform_backup(&w.engine, dir.path()).expect("first");
    std::fs::write(&first, "older contents").expect("mark first");
    // When
    let second = perform_backup(&w.engine, dir.path()).expect("second");
    // Then
    assert_eq!(
        second.file_name().and_then(|n| n.to_str()),
        Some("PFBackup_2026-03-15_09-05-07-2.md")
    );
    assert_eq!(
        std::fs::read_to_string(&first).expect("first still there"),
        "older contents"
    );
}

#[test]
/// Covers: R-MD-7 (a folder that cannot be written gives the Swift-style error)
fn backup_into_missing_folder_reports_store_error() {
    let w = World::with_fixture("world");
    let dir = TempDir::new().expect("temp dir");
    let missing = dir.path().join("no-such-folder");
    let err = perform_backup(&w.engine, &missing).expect_err("no folder");
    match err {
        pf_engine::EngineError::Store(msg) => {
            assert!(msg.starts_with("Failed to create backup: "), "{msg}");
        }
        other => panic!("expected Store error, got {other:?}"),
    }
}

#[test]
/// Covers: DZ "Delete All Data" (wipe_all_data)
fn wipe_all_data_empties_db() {
    // Given
    let mut w = World::with_fixture("world");
    create_account(&mut w.engine, account("Extra")).expect("create");
    // When
    wipe_all_data(&mut w.engine).expect("wipe");
    // Then
    assert!(w.engine.db().all_records().is_empty());
    assert!(!w.engine.can_undo());
    w.reload();
    assert!(w.engine.db().all_records().is_empty());
}
