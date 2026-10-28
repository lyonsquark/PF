//! L5 UI behaviour tests for the app shell (T13): sidebar navigation, scenario menu, Edit menu and
//! shortcuts, confirmations, grouping and backup. Headless (egui_kittest); assertions are made on
//! engine state after each interaction, never on pixels.

mod common;

use egui::{Key, Modifiers};
use egui_kittest::kittest::{NodeT, Queryable};
use pf_engine::commands::{accounts, backup};
use pf_model::AccountType;
use pf_ui::PfApp;
use pf_ui::files::FakeDialogs;
use pf_ui::screen::Screen;

use common::{empty_engine, engine_with_fixture, harness};

/// Adds a plain checking account named `name` through the engine and returns its id.
fn add_account(app: &PfApp, name: &str, institution: &str) -> pf_model::AccountId {
    let shared = app.engine();
    let mut e = shared.lock().expect("engine lock");
    accounts::create_account(
        &mut e,
        accounts::AccountInput {
            name: name.to_string(),
            account_type: AccountType::Checking,
            institution: institution.to_string(),
            currency: String::new(),
            is_external: false,
        },
    )
    .expect("create account")
}

#[test]
/// UI-NAV-01: every sidebar entry opens its screen.
fn ui_nav_01_every_sidebar_entry_opens_its_screen() {
    let mut app = PfApp::new_for_test(engine_with_fixture());
    // Each label is clicked while another screen is showing: the current screen's title is
    // also a label, so clicking the entry for the screen already shown would be ambiguous.
    let steps = [
        (Screen::Budget, "Budget"),
        (Screen::Obligations, "Obligations"),
        (Screen::Scenarios, "Scenarios"),
        (Screen::Charts, "Charts"),
        (Screen::Reports, "Reports"),
        (Screen::Categories, "Categories"),
        (Screen::Contexts, "Contexts"),
        (Screen::Settings, "Settings"),
        (Screen::Dashboard, "Dashboard"),
    ];
    for (target, label) in steps {
        {
            let mut h = harness(&mut app);
            h.run();
            h.get_by_label(label).click();
            h.run();
        }
        assert_eq!(app.screen(), target, "clicking {label:?}");
    }
}

#[test]
/// UI-NAV-01: clicking an account in the sidebar opens `Screen::Account(id)`.
fn ui_nav_01_account_entry_opens_account_screen() {
    let mut app = PfApp::new_for_test(engine_with_fixture());
    let shared = app.engine();
    let id = {
        let e = shared.lock().expect("engine lock");
        e.db()
            .account_by_name("Savings")
            .expect("fixture account")
            .id
    };
    {
        let mut h = harness(&mut app);
        h.run();
        h.query_all_by_label("Savings")
            .next()
            .expect("sidebar entry")
            .click();
        h.run();
    }
    assert_eq!(app.screen(), Screen::Account(id));
}

#[test]
/// UI-SCN-01: the scenario menu switches to a scenario, and picking it again returns to Reality.
fn ui_scn_01_scenario_menu_switches_and_toggles() {
    let mut app = PfApp::new_for_test(engine_with_fixture());
    let shared = app.engine();
    let plan = {
        let e = shared.lock().expect("engine lock");
        e.db()
            .scenario_by_name("Plan A")
            .expect("fixture scenario")
            .id
    };
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Reality").click();
        h.run();
        h.get_by_label("Plan A").click();
        h.run();
    }
    assert_eq!(
        shared.lock().expect("lock").state.active_scenario,
        Some(plan)
    );
    {
        let mut h = harness(&mut app);
        h.run();
        // The menu button now shows the scenario name; the item shows a check mark.
        h.get_by_label("Plan A").click();
        h.run();
        h.get_by_label("• Plan A").click();
        h.run();
    }
    assert_eq!(shared.lock().expect("lock").state.active_scenario, None);
}

#[test]
/// UI-SCN-01: the scenario menu is hidden on Budget and shown on Dashboard.
fn ui_scn_01_scenario_menu_hidden_on_budget() {
    let mut app = PfApp::new_for_test(engine_with_fixture());
    {
        let mut h = harness(&mut app);
        h.run();
        assert!(
            h.query_by_label("Reality").is_some(),
            "menu shown on Dashboard"
        );
        h.get_by_label("Budget").click();
        h.run();
        assert!(
            h.query_by_label("Reality").is_none(),
            "menu hidden on Budget"
        );
    }
}

#[test]
/// R-UND-5: the Edit menu shows "Undo {name}" and runs it; with no history Undo is disabled.
fn ui_shell_edit_menu_undo_label_and_action() {
    let mut app = PfApp::new_for_test(engine_with_fixture());
    let shared = app.engine();
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Edit").click();
        h.run();
        assert!(
            h.get_by_label("Undo").accesskit_node().is_disabled(),
            "Undo is disabled with an empty history"
        );
    }
    let id = add_account(&app, "Temp Account", "");
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Edit").click();
        h.run();
        h.get_by_label("Undo Add Account").click();
        h.run();
    }
    assert!(shared.lock().expect("lock").db().account(id).is_none());
}

#[test]
/// R-ACC-1: right-click → Delete asks for confirmation; Delete soft-deletes; Undo restores.
fn ui_shell_delete_account_from_sidebar_confirms_and_undoes() {
    let mut app = PfApp::new_for_test(engine_with_fixture());
    let shared = app.engine();
    let visa = {
        let e = shared.lock().expect("engine lock");
        e.db().account_by_name("Visa").expect("fixture account").id
    };
    {
        let mut h = harness(&mut app);
        h.run();
        h.query_all_by_label("Visa")
            .next()
            .expect("sidebar entry")
            .click_secondary();
        h.run();
        h.get_by_label("Delete").click();
        h.run();
        assert!(
            h.query_by_label("Delete Account?").is_some(),
            "confirm shown"
        );
        h.get_by_label("Delete").click();
        h.run();
        assert!(h.query_by_label("Visa").is_none(), "gone from the sidebar");
    }
    {
        let e = shared.lock().expect("lock");
        assert!(
            e.db().account(visa).is_some_and(|a| a.is_deleted),
            "soft-deleted"
        );
    }
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Edit").click();
        h.run();
        h.get_by_label("Undo Delete Account").click();
        h.run();
        assert!(
            h.query_all_by_label("Visa").next().is_some(),
            "back in the sidebar"
        );
    }
    assert!(
        shared
            .lock()
            .expect("lock")
            .db()
            .account(visa)
            .is_some_and(|a| !a.is_deleted)
    );
}

#[test]
/// R-UND-5: Cmd-Z undoes the newest step and Shift-Cmd-Z redoes it.
fn ui_shell_cmd_z_undoes_and_shift_cmd_z_redoes() {
    let mut app = PfApp::new_for_test(engine_with_fixture());
    let shared = app.engine();
    let id = add_account(&app, "Shortcut Account", "");
    {
        let mut h = harness(&mut app);
        h.run();
        h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
        h.run();
    }
    assert!(
        shared.lock().expect("lock").db().account(id).is_none(),
        "undone"
    );
    {
        let mut h = harness(&mut app);
        h.run();
        h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
        h.run();
    }
    assert!(
        shared.lock().expect("lock").db().account(id).is_some(),
        "redone"
    );
}

#[test]
/// R-UND-5: Cmd-Z does nothing while a dialog is open.
fn ui_shell_cmd_z_ignored_while_dialog_open() {
    let mut app = PfApp::new_for_test(engine_with_fixture());
    let shared = app.engine();
    let id = add_account(&app, "Keeper", "");
    {
        // Open the Delete confirmation through the sidebar, but do not accept it.
        let mut h = harness(&mut app);
        h.run();
        h.query_all_by_label("Keeper")
            .next()
            .expect("sidebar entry")
            .click_secondary();
        h.run();
        h.get_by_label("Delete").click();
        h.run();
        assert!(h.query_by_label("Delete Account?").is_some(), "dialog open");
        h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
        h.run();
        assert!(
            h.query_by_label("Delete Account?").is_some(),
            "dialog still open"
        );
    }
    let e = shared.lock().expect("lock");
    assert!(
        e.db().account(id).is_some(),
        "the Add Account step was not undone"
    );
    assert!(e.can_undo(), "history untouched");
}

#[test]
/// Deleting the selected account through the engine returns the screen to the dashboard.
fn ui_shell_deleted_selected_account_returns_to_dashboard() {
    let mut app = PfApp::new_for_test(engine_with_fixture());
    let shared = app.engine();
    let mortgage = {
        let e = shared.lock().expect("engine lock");
        e.db()
            .account_by_name("Mortgage")
            .expect("fixture account")
            .id
    };
    app.set_screen(Screen::Account(mortgage));
    {
        let mut e = shared.lock().expect("engine lock");
        accounts::delete_account(&mut e, mortgage).expect("delete account");
    }
    {
        let mut h = harness(&mut app);
        h.run();
    }
    assert_eq!(app.screen(), Screen::Dashboard);
}

#[test]
/// R-ACC-4: with the Institution grouping, an account with no institution is listed under "Other".
fn ui_shell_sidebar_grouping_institution_uses_other() {
    let mut app = PfApp::new_for_test(empty_engine());
    add_account(&app, "Loose Cash", "");
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Grouping").click();
        h.run();
        h.get_by_label("Institution").click();
        h.run();
        assert!(h.query_by_label("Other").is_some(), "Other header shown");
        assert!(
            h.query_all_by_label("Loose Cash").next().is_some(),
            "account under it"
        );
    }
}

#[test]
/// R-MD-7: with no folder set, Backup Data asks for one, then writes a PFBackup_*.md file there.
fn ui_shell_backup_prompts_for_folder_then_writes() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let files = FakeDialogs::default();
    files.push_answer(Some(dir.path().to_path_buf()));
    let mut app = PfApp::new_for_test_with_files(engine_with_fixture(), files.clone());
    let shared = app.engine();
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("File").click();
        h.run();
        h.get_by_label("Backup Data").click();
        h.run();
        assert!(
            h.query_by_label_contains("Backup saved to:").is_some(),
            "toast shown"
        );
    }
    assert_eq!(files.calls(), vec!["pick_folder:Set Backup Folder"]);
    let written: Vec<String> = std::fs::read_dir(dir.path())
        .expect("read backup folder")
        .filter_map(|entry| {
            entry
                .ok()
                .map(|e| e.file_name().to_string_lossy().into_owned())
        })
        .filter(|name| name.starts_with("PFBackup_") && name.ends_with(".md"))
        .collect();
    assert_eq!(written.len(), 1, "one backup file: {written:?}");
    let e = shared.lock().expect("lock");
    assert_eq!(
        backup::backup_folder(&e).as_deref(),
        Some(dir.path()),
        "folder remembered"
    );
    // The fixture has a live account, so the backup contains its name.
    let content = std::fs::read_to_string(dir.path().join(&written[0])).expect("read backup");
    assert!(content.contains("Checking"));
}

#[test]
/// R-MD-7: cancelling the folder prompt stores nothing, writes nothing and shows no alert.
fn ui_shell_backup_cancelled_folder_does_nothing() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let files = FakeDialogs::default();
    files.push_answer(None);
    let mut app = PfApp::new_for_test_with_files(engine_with_fixture(), files.clone());
    let shared = app.engine();
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("File").click();
        h.run();
        h.get_by_label("Backup Data").click();
        h.run();
        assert!(h.query_by_label("Backup Error").is_none(), "no alert");
    }
    assert_eq!(files.calls(), vec!["pick_folder:Set Backup Folder"]);
    assert!(
        std::fs::read_dir(dir.path())
            .expect("read")
            .next()
            .is_none()
    );
    let e = shared.lock().expect("lock");
    assert!(backup::backup_folder(&e).is_none(), "no folder stored");
}

#[test]
/// Cmd-\ hides the sidebar and shows it again; the View menu item says which one it will do.
fn ui_shell_cmd_backslash_toggles_sidebar() {
    let mut t = pf_model::testing::TestDb::new();
    t.account("Everyday", pf_model::AccountType::Checking);
    let mut e = common::empty_engine();
    e.replace_db(t.build()).expect("write the test database");
    let mut app = PfApp::new_for_test(e);
    let mut h = harness(&mut app);
    h.run();
    assert!(
        h.query_all_by_value("ACCOUNTS").count() > 0,
        "sidebar shown at first"
    );
    h.key_press_modifiers(Modifiers::COMMAND, Key::Backslash);
    h.run_steps(60);
    assert!(
        h.query_all_by_value("ACCOUNTS").count() == 0,
        "sidebar hidden"
    );
    h.key_press_modifiers(Modifiers::COMMAND, Key::Backslash);
    h.run_steps(60);
    assert!(
        h.query_all_by_value("ACCOUNTS").count() > 0,
        "sidebar shown again"
    );
}
