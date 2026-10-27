//! L5 UI tests for the Settings screen (T21): backup folder and Cmd-S, Markdown export and import,
//! Monarch CSV entry, orphaned transactions, and Delete All Data. Headless (egui_kittest) on temp
//! databases and temp folders. Dialogs are the fake `FakeDialogs`, so no native window opens.
//! Assertions are made on engine state, on files in the temp folder, or on the labels the screen draws.

mod common;

use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::kittest::{NodeT, Queryable};
use jiff::civil::date;
use pf_engine::commands::{backup, settings, transactions};
use pf_engine::testing::World;
use pf_model::testing::TestDb;
use pf_model::{AccountType, Db, TransactionStatus};
use pf_ui::PfApp;
use pf_ui::files::FakeDialogs;
use pf_ui::screen::Screen;
use rust_decimal_macros::dec;

use common::{empty_engine, harness};

/// Engine holding `db`, on a temp file (never the app's database).
fn engine_with(db: Db) -> pf_engine::Engine {
    let mut e = empty_engine();
    e.replace_db(db)
        .expect("write the test database to the temp file");
    e
}

/// Number of live transactions in the engine.
fn live_tx_count(app: &PfApp) -> usize {
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    e.db().live_transactions().count()
}

/// Names of the `PFBackup_*.md` files in `dir`.
fn backup_files(dir: &std::path::Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .expect("read backup folder")
        .map(|entry| {
            entry
                .expect("dir entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.starts_with("PFBackup_"))
        .collect()
}

#[test]
/// Choosing a font in the Settings combo saves `uiFont` through the engine.
fn settings_font_combo_saves_choice() {
    let mut app = PfApp::new_for_test(empty_engine());
    app.set_screen(Screen::Settings);
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_role_and_label(Role::ComboBox, "Font").click();
        h.run();
        h.get_by_label("Arial").click();
        h.run();
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    assert_eq!(settings::ui_font(&e), Some("arial"));
}

#[test]
/// A saved choice is shown in the combo when Settings opens.
fn settings_font_combo_shows_saved_choice() {
    let mut engine = empty_engine();
    settings::set_ui_font(&mut engine, "verdana").expect("set");
    let mut app = PfApp::new_for_test(engine);
    app.set_screen(Screen::Settings);
    let mut h = harness(&mut app);
    h.run();
    let combo = h.get_by_role_and_label(Role::ComboBox, "Font");
    assert_eq!(combo.value().as_deref(), Some("Verdana"));
}

#[test]
/// UI-SET-01, R-MD-5: export to a file, add a transaction, import the file (Import). The added
/// transaction is gone, the exported data is back, undo is unavailable, and the success alert shows.
fn ui_set_01_export_then_import_round_trip() {
    let dir = tempfile::tempdir().expect("temp dir");
    let export_path = dir.path().join("export.md");
    let fake = FakeDialogs::default();
    let world = World::with_fixture("world");
    let checking = world.account("Checking");
    let mut app = PfApp::new_for_test_with_files(world.into_engine(), fake.clone());
    app.set_screen(Screen::Settings);
    let original = live_tx_count(&app);

    fake.push_answer(Some(export_path.clone()));
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Export to Markdown").click();
        h.run();
    }
    assert!(export_path.exists(), "export written to the chosen file");

    {
        let shared = app.engine();
        let mut e = shared.lock().expect("engine lock");
        transactions::add_transaction(
            &mut e,
            transactions::NewTransaction {
                kind: transactions::TxKind::Withdrawal,
                account: checking,
                date: date(2026, 3, 1),
                amount: dec!(7.25),
                payee: "Round Trip Payee".into(),
                memo: String::new(),
                status: TransactionStatus::Cleared,
                category: None,
                contexts: Vec::new(),
            },
        )
        .expect("add transaction");
    }
    assert_eq!(live_tx_count(&app), original + 1);

    fake.push_answer(Some(export_path));
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Import from Markdown (Wipes current data)")
            .click();
        h.run();
        h.get_by_label("Import").click();
        h.run();
        assert!(
            h.query_by_label("Data successfully imported from Markdown.")
                .is_some(),
            "success alert shown"
        );
    }
    assert_eq!(live_tx_count(&app), original, "import replaced the data");
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    assert!(
        e.db()
            .live_transactions()
            .all(|t| t.payee != "Round Trip Payee"),
        "the added transaction is gone"
    );
    assert!(!e.can_undo(), "import clears the undo stack (R-MD-5)");
}

#[test]
/// R-MD-7, BEH-BAK-01: with a folder set, Backup Now writes one PFBackup_*.md and toasts its path.
fn ui_set_backup_now_writes_file_and_toasts() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut engine = empty_engine();
    backup::set_backup_folder(&mut engine, dir.path()).expect("set folder");
    let mut app = PfApp::new_for_test(engine);
    app.set_screen(Screen::Settings);
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Backup Now (Cmd-S)").click();
        h.run();
        assert!(
            h.query_by_label_contains("Backup saved to:").is_some(),
            "toast shown"
        );
    }
    let files = backup_files(dir.path());
    assert_eq!(files.len(), 1, "one backup written: {files:?}");
    assert!(files[0].ends_with(".md"));
}

#[test]
/// R-MD-7: with no folder set, Cmd-S asks for one; a cancelled pick writes nothing.
fn ui_set_backup_prompts_for_folder_when_unset() {
    let fake = FakeDialogs::default();
    fake.push_answer(None);
    let mut app = PfApp::new_for_test_with_files(empty_engine(), fake.clone());
    {
        let mut h = harness(&mut app);
        h.run();
        h.key_press_modifiers(Modifiers::COMMAND, Key::S);
        h.run();
    }
    assert!(
        fake.calls()
            .contains(&"pick_folder:Set Backup Folder".to_string()),
        "calls: {:?}",
        fake.calls()
    );
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    assert!(backup::backup_folder(&e).is_none(), "no folder saved");
}

#[test]
/// R-MD-7: a folder that no longer exists raises "Backup Error" and clears the setting.
fn ui_set_backup_missing_folder_alerts() {
    let dir = tempfile::tempdir().expect("temp dir");
    let gone = dir.path().join("gone");
    std::fs::create_dir(&gone).expect("create folder");
    let mut engine = empty_engine();
    backup::set_backup_folder(&mut engine, &gone).expect("set folder");
    std::fs::remove_dir(&gone).expect("remove folder");
    let mut app = PfApp::new_for_test(engine);
    app.set_screen(Screen::Settings);
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Backup Now (Cmd-S)").click();
        h.run();
        assert!(
            h.query_by_label("Backup folder access lost. Please choose the folder again.")
                .is_some(),
            "backup error alert shown"
        );
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    assert!(
        backup::backup_folder(&e).is_none_or(|p| p.as_os_str().is_empty()),
        "setting cleared"
    );
}

#[test]
/// The folder row says "Not Set" until a folder is chosen, then "Configured".
fn ui_set_folder_status_label() {
    let dir = tempfile::tempdir().expect("temp dir");
    let fake = FakeDialogs::default();
    let mut app = PfApp::new_for_test_with_files(empty_engine(), fake.clone());
    app.set_screen(Screen::Settings);
    {
        let mut h = harness(&mut app);
        h.run();
        assert!(h.query_by_label("Not Set").is_some());
        assert!(h.query_by_label("Configured").is_none());
        fake.push_answer(Some(dir.path().to_path_buf()));
        h.get_by_label("Automatic Backup Folder").click();
        h.run();
        assert!(h.query_by_label("Configured").is_some());
        assert!(h.query_by_label("Not Set").is_none());
    }
}

#[test]
/// Choosing a file, then Cancel on "Wipe and Import?", changes nothing.
fn ui_set_import_cancel_keeps_data() {
    let dir = tempfile::tempdir().expect("temp dir");
    let file = dir.path().join("other.md");
    std::fs::write(&file, "not a backup").expect("write file");
    let fake = FakeDialogs::default();
    let world = World::with_fixture("world");
    let mut app = PfApp::new_for_test_with_files(world.into_engine(), fake.clone());
    app.set_screen(Screen::Settings);
    let before = live_tx_count(&app);
    fake.push_answer(Some(file));
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Import from Markdown (Wipes current data)")
            .click();
        h.run();
        assert!(h.query_by_label("Wipe and Import?").is_some());
        h.get_by_label("Cancel").click();
        h.run();
    }
    assert_eq!(live_tx_count(&app), before);
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    assert!(e.db().live_accounts().count() > 0, "accounts kept");
}

#[test]
/// Delete All Data, confirmed, leaves no accounts or transactions and shows the alert.
fn ui_set_delete_all_data() {
    let world = World::with_fixture("world");
    let mut app = PfApp::new_for_test(world.into_engine());
    app.set_screen(Screen::Settings);
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Delete All Data").click();
        h.run();
        assert!(h.query_by_label("Delete All Data?").is_some());
        h.get_by_label("Delete").click();
        h.run();
        assert!(h.query_by_label("All data has been deleted.").is_some());
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    assert_eq!(e.db().live_accounts().count(), 0);
    assert_eq!(e.db().live_transactions().count(), 0);
}

#[test]
/// R-TXN-15: with no orphaned transactions the Find button is disabled and says "(0)".
fn ui_set_orphaned_button_disabled_when_zero() {
    let mut app = PfApp::new_for_test(empty_engine());
    app.set_screen(Screen::Settings);
    let mut h = harness(&mut app);
    h.run();
    let button = h.get_by_label("Find Orphaned Transactions (0)");
    assert!(button.accesskit_node().is_disabled(), "button disabled");
}

#[test]
/// R-TXN-15: two orphans; Find, then Delete All removes both; Cmd-Z brings them back.
fn ui_set_orphaned_delete_all_undoable() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    t.orphan_tx("2026-02-01", "-4.50", "Coffee").done();
    t.orphan_tx("2026-01-10", "-12", "Lunch").done();
    t.tx(chk, "2026-01-05", "1000", "Pay")
        .status(TransactionStatus::Cleared)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Settings);
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Find Orphaned Transactions (2)").click();
        h.run();
        h.get_by_label("Delete All").click();
        h.run();
    }
    let orphans = |app: &PfApp| {
        let shared = app.engine();
        let e = shared.lock().expect("engine lock");
        transactions::orphaned_transactions(e.db()).len()
    };
    assert_eq!(orphans(&app), 0);
    {
        let mut h = harness(&mut app);
        h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
        h.run();
    }
    assert_eq!(orphans(&app), 2, "one undo step restores both");
}

#[test]
/// Import from Monarch CSV: the chosen file goes to the CSV wizard in Monarch mode.
fn ui_set_monarch_opens_wizard() {
    let dir = tempfile::tempdir().expect("temp dir");
    let csv = dir.path().join("monarch.csv");
    std::fs::write(&csv, "Date,Merchant,Amount\n").expect("write csv");
    let fake = FakeDialogs::default();
    fake.push_answer(Some(csv));
    let mut app = PfApp::new_for_test_with_files(empty_engine(), fake.clone());
    app.set_screen(Screen::Settings);
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Import from Monarch CSV").click();
        h.run();
        assert!(
            h.query_by_label("Map CSV Columns").is_some(),
            "the CSV wizard opened at its first step"
        );
    }
    assert!(
        fake.calls()
            .contains(&"pick_file:Import from Monarch CSV".to_string())
    );
}
