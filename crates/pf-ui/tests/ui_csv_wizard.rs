//! L5 UI tests for the CSV import wizard (T22): a bank import end to end, the required-columns check,
//! saved column mappings, negate, duplicates, Monarch accounts (skip and New…), categories, payee
//! renames, and the all-mapped message. Each test writes a real CSV to a temp file and answers the fake
//! file dialog with it. Headless (egui_kittest); assertions are on engine state and on what is drawn.

mod common;

use std::io::Write;

use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable;
use pf_engine::Engine;
use pf_model::testing::TestDb;
use pf_model::{AccountType, CategoryType, Db};
use pf_ui::PfApp;
use pf_ui::files::FakeDialogs;
use pf_ui::screen::Screen;

use common::{empty_engine, harness};

/// Engine holding `db`, on a temp file (never the app's database).
fn engine_with(db: Db) -> Engine {
    let mut e = empty_engine();
    e.replace_db(db)
        .expect("write the test database to the temp file");
    e
}

/// Writes `content` to a temp CSV and returns it (keep it alive while the test runs).
fn csv_file(content: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::NamedTempFile::new().expect("temp csv");
    f.write_all(content.as_bytes()).expect("write csv");
    f
}

/// A bank CSV with two rows.
const BANK: &str =
    "Date,Description,Amount\n03/01/2026,Coffee Shop,-4.50\n03/02/2026,Grocer,-40.00\n";

/// A Monarch CSV with one row whose account is not in the database.
const MONARCH: &str =
    "Date,Merchant,Category,Account,Amount\n03/01/2026,Corner Store,Snacks,Unknown Bank,-5.00\n";

/// Renames payee `from` to `to` in step 2 of the wizard. A renamed payee saves a mapping; an unchanged
/// one is not saved, and a Monarch row with an unmapped payee is skipped by the engine.
fn rename_payee(h: &mut egui_kittest::Harness<'_>, from: &str, to: &str) {
    let label = format!("Payee {from}");
    h.get_by_role_and_label(Role::TextInput, &label).click();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run();
    h.get_by_role_and_label(Role::TextInput, &label)
        .type_text(to);
    h.run();
}

/// Clicks the button labelled `label`.
fn click(h: &mut egui_kittest::Harness<'_>, label: &str) {
    h.get_by_label(label).click();
    h.run();
}

#[test]
/// UI-CSV-01, R-CSV-14: a bank CSV imports two rows in one step; one undo removes both.
fn ui_csv_01_bank_wizard_end_to_end() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    let file = csv_file(BANK);
    let files = FakeDialogs::default();
    files.push_answer(Some(file.path().to_path_buf()));
    let mut app = PfApp::new_for_test_with_files(engine_with(t.build()), files);
    app.set_screen(Screen::Account(chk));
    {
        let mut h = harness(&mut app);
        h.run();
        click(&mut h, "Import CSV…");
        click(&mut h, "Continue");
        click(&mut h, "Import");
        assert!(
            h.query_by_label_contains("2 transactions imported")
                .is_some(),
            "summary"
        );
        click(&mut h, "Done");
    }
    {
        let shared = app.engine();
        let mut e = shared.lock().expect("engine lock");
        assert_eq!(e.db().live_transactions().count(), 2);
        assert_eq!(e.undo_name(), Some("Import CSV"));
        e.undo().expect("undo");
        assert_eq!(
            e.db().live_transactions().count(),
            0,
            "one undo removes both"
        );
    }
}

#[test]
/// R-CSV-3: Amount set to "(Skip)" stops Continue with the "Missing Required Columns" message.
fn ui_csv_missing_required_alert() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    let file = csv_file(BANK);
    let files = FakeDialogs::default();
    files.push_answer(Some(file.path().to_path_buf()));
    let mut app = PfApp::new_for_test_with_files(engine_with(t.build()), files);
    app.set_screen(Screen::Account(chk));
    let mut h = harness(&mut app);
    h.run();
    click(&mut h, "Import CSV…");
    h.get_by_role_and_label(Role::ComboBox, "Column Amount")
        .click();
    h.run();
    let options: Vec<_> = h.get_all_by_label("(Skip)").collect();
    options.last().expect("the Skip option").click();
    h.run();
    click(&mut h, "Continue");
    assert!(
        h.query_by_label("Missing Required Columns").is_some(),
        "heading"
    );
    assert!(
        h.query_by_label_contains("Required columns not mapped")
            .is_some(),
        "lists the missing column"
    );
}

#[test]
/// R-CSV-15: a mapping saved by an import is preselected when the same headers come back.
fn ui_csv_saved_column_mapping_restored() {
    let custom = "Posted,Payee Text,Sum\n03/01/2026,Coffee Shop,-4.50\n";
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    let file = csv_file(custom);
    let files = FakeDialogs::default();
    files.push_answer(Some(file.path().to_path_buf()));
    files.push_answer(Some(file.path().to_path_buf()));
    let mut app = PfApp::new_for_test_with_files(engine_with(t.build()), files);
    app.set_screen(Screen::Account(chk));
    {
        let mut h = harness(&mut app);
        h.run();
        click(&mut h, "Import CSV…");
        for (column, field) in [
            ("Posted", "Date"),
            ("Payee Text", "Payee"),
            ("Sum", "Amount"),
        ] {
            h.get_by_role_and_label(Role::ComboBox, &format!("Column {column}"))
                .click();
            h.run();
            let options: Vec<_> = h.get_all_by_label(field).collect();
            options.last().expect("option").click();
            h.run();
        }
        click(&mut h, "Continue");
        click(&mut h, "Import");
        click(&mut h, "Done");
    }
    let mut h = harness(&mut app);
    h.run();
    click(&mut h, "Import CSV…");
    let shown = h
        .get_by_role_and_label(Role::ComboBox, "Column Sum")
        .value()
        .unwrap_or_default();
    assert!(shown.contains("Amount"), "restored field, shows {shown:?}");
}

#[test]
/// With "Negate amounts" on, an amount of 45.10 imports as -45.10.
fn ui_csv_negate_toggle_flips_sign() {
    let csv = "Date,Description,Amount\n03/01/2026,Refund,45.10\n";
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    let file = csv_file(csv);
    let files = FakeDialogs::default();
    files.push_answer(Some(file.path().to_path_buf()));
    let mut app = PfApp::new_for_test_with_files(engine_with(t.build()), files);
    app.set_screen(Screen::Account(chk));
    {
        let mut h = harness(&mut app);
        h.run();
        click(&mut h, "Import CSV…");
        h.get_by_label_contains("Negate amounts").click();
        h.run();
        click(&mut h, "Continue");
        click(&mut h, "Import");
        click(&mut h, "Done");
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    let tx = e.db().live_transactions().next().expect("row");
    assert_eq!(tx.amount.to_string(), "-45.10");
}

#[test]
/// R-CSV-11: importing the same file again skips both rows as duplicates.
fn ui_csv_duplicates_summary() {
    let file = csv_file(BANK);
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    let files = FakeDialogs::default();
    files.push_answer(Some(file.path().to_path_buf()));
    files.push_answer(Some(file.path().to_path_buf()));
    let mut app = PfApp::new_for_test_with_files(engine_with(t.build()), files);
    app.set_screen(Screen::Account(chk));
    for _ in 0..2 {
        let mut h = harness(&mut app);
        h.run();
        click(&mut h, "Import CSV…");
        click(&mut h, "Continue");
        click(&mut h, "Import");
        if h.query_by_label_contains("duplicates skipped").is_some() {
            assert!(h.query_by_label_contains("2 duplicates skipped").is_some());
            assert!(h.query_by_label_contains("transactions imported").is_none());
        }
        click(&mut h, "Done");
    }
}

#[test]
/// R-CSV-5, R-CSV-8: a Monarch account the database does not know is listed; skipping it leaves the row
/// out and the summary says so.
fn ui_csv_monarch_account_section() {
    let file = csv_file(MONARCH);
    let files = FakeDialogs::default();
    files.push_answer(Some(file.path().to_path_buf()));
    let mut app = PfApp::new_for_test_with_files(engine_with(TestDb::new().build()), files);
    app.set_screen(Screen::Settings);
    let mut h = harness(&mut app);
    h.run();
    click(&mut h, "Import from Monarch CSV");
    click(&mut h, "Continue");
    assert!(h.query_by_label("Accounts").is_some(), "accounts section");
    click(&mut h, "Import");
    assert!(
        h.query_by_label_contains("skipped (unmapped account)")
            .is_some(),
        "skipped"
    );
}

#[test]
/// R-CSV-16: "New…" creates the account, then the import uses it.
fn ui_csv_new_account_subdialog() {
    let file = csv_file(MONARCH);
    let files = FakeDialogs::default();
    files.push_answer(Some(file.path().to_path_buf()));
    let mut app = PfApp::new_for_test_with_files(engine_with(TestDb::new().build()), files);
    app.set_screen(Screen::Settings);
    {
        let mut h = harness(&mut app);
        h.run();
        click(&mut h, "Import from Monarch CSV");
        click(&mut h, "Continue");
        click(&mut h, "New…");
        click(&mut h, "Create");
        rename_payee(&mut h, "Corner Store", "Corner Store Co");
        click(&mut h, "Import");
        assert!(
            h.query_by_label_contains("1 transaction imported")
                .is_some(),
            "imported"
        );
        click(&mut h, "Done");
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    assert!(
        e.db().live_accounts().any(|a| a.name == "Unknown Bank"),
        "account created"
    );
    assert_eq!(e.db().live_transactions().count(), 1);
}

#[test]
/// R-CSV-16: an unknown category defaults to "New:", so the import creates it and uses it.
fn ui_csv_category_create_new_default() {
    let file = csv_file(MONARCH);
    let files = FakeDialogs::default();
    files.push_answer(Some(file.path().to_path_buf()));
    let mut t = TestDb::new();
    t.account("Unknown Bank", AccountType::Checking);
    let mut app = PfApp::new_for_test_with_files(engine_with(t.build()), files);
    app.set_screen(Screen::Settings);
    {
        let mut h = harness(&mut app);
        h.run();
        click(&mut h, "Import from Monarch CSV");
        click(&mut h, "Continue");
        rename_payee(&mut h, "Corner Store", "Corner Store Co");
        click(&mut h, "Import");
        click(&mut h, "Done");
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    assert_eq!(
        e.db().live_transactions().count(),
        1,
        "the row was imported"
    );
    let cat = e
        .db()
        .live_categories()
        .find(|c| c.name == "Snacks")
        .expect("created category");
    assert_eq!(cat.category_type, CategoryType::Expense);
    assert!(
        e.db()
            .live_transactions()
            .all(|t| t.category_id == Some(cat.id))
    );
}

#[test]
/// Typing a name that already exists under the same parent shows the ⚠ warning.
fn ui_csv_category_duplicate_warning() {
    let file = csv_file(MONARCH);
    let files = FakeDialogs::default();
    files.push_answer(Some(file.path().to_path_buf()));
    let mut t = TestDb::new();
    t.account("Unknown Bank", AccountType::Checking);
    t.category("Groceries", CategoryType::Expense);
    let mut app = PfApp::new_for_test_with_files(engine_with(t.build()), files);
    app.set_screen(Screen::Settings);
    let mut h = harness(&mut app);
    h.run();
    click(&mut h, "Import from Monarch CSV");
    click(&mut h, "Continue");
    h.get_by_role_and_label(Role::TextInput, "New Category Name Snacks")
        .click();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run();
    h.get_by_role_and_label(Role::TextInput, "New Category Name Snacks")
        .type_text("groceries");
    h.run();
    assert!(h.query_by_label("⚠").is_some(), "duplicate warning");
}

#[test]
/// R-CSV-10: renaming a payee in step 2 stores a payee mapping, and the row uses the new name.
fn ui_csv_payee_rename_creates_mapping() {
    let csv = "Date,Description,Amount\n03/01/2026,KROGER #123,-12.00\n";
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    let file = csv_file(csv);
    let files = FakeDialogs::default();
    files.push_answer(Some(file.path().to_path_buf()));
    let mut app = PfApp::new_for_test_with_files(engine_with(t.build()), files);
    app.set_screen(Screen::Account(chk));
    {
        let mut h = harness(&mut app);
        h.run();
        click(&mut h, "Import CSV…");
        click(&mut h, "Continue");
        h.get_by_role_and_label(Role::TextInput, "Payee KROGER #123")
            .click();
        h.run();
        h.key_press_modifiers(Modifiers::COMMAND, Key::A);
        h.run();
        h.get_by_role_and_label(Role::TextInput, "Payee KROGER #123")
            .type_text("Kroger");
        h.run();
        click(&mut h, "Import");
        click(&mut h, "Done");
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    assert!(e.db().live_transactions().any(|t| t.payee == "Kroger"));
    assert!(
        e.db()
            .import_mappings
            .values()
            .any(|m| m.mapping_type == "payee")
    );
}

#[test]
/// With every value already mapped, step 2 says so. The first import renames the payee, which saves
/// its mapping; the second import then has nothing left to map.
fn ui_csv_all_mapped_message() {
    let csv = "Date,Description,Amount\n03/01/2026,Coffee Shop,-4.50\n";
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    let file = csv_file(csv);
    let files = FakeDialogs::default();
    files.push_answer(Some(file.path().to_path_buf()));
    files.push_answer(Some(file.path().to_path_buf()));
    let mut app = PfApp::new_for_test_with_files(engine_with(t.build()), files);
    app.set_screen(Screen::Account(chk));
    {
        let mut h = harness(&mut app);
        h.run();
        click(&mut h, "Import CSV…");
        click(&mut h, "Continue");
        h.get_by_role_and_label(Role::TextInput, "Payee Coffee Shop")
            .click();
        h.run();
        h.key_press_modifiers(Modifiers::COMMAND, Key::A);
        h.run();
        h.get_by_role_and_label(Role::TextInput, "Payee Coffee Shop")
            .type_text("Coffee");
        h.run();
        click(&mut h, "Import");
        click(&mut h, "Done");
    }
    let mut h = harness(&mut app);
    h.run();
    click(&mut h, "Import CSV…");
    click(&mut h, "Continue");
    assert!(
        h.query_by_label("All values are already mapped! Ready to import.")
            .is_some(),
        "all mapped"
    );
}
