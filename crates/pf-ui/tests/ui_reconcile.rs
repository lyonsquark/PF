//! L5 UI tests for the Reconcile dialog and the reconciled-edit warning (T30): UI-REC-01..04, the
//! CSV path, Flip signs, the first-time checkbox and the Reality-only button. Headless (egui_kittest).

mod common;

use std::io::Write;

use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable;
use pf_engine::Engine;
use pf_model::testing::TestDb;
use pf_model::{AccountId, AccountType, Db, TransactionId, TransactionStatus};
use pf_ui::PfApp;
use pf_ui::files::FakeDialogs;
use pf_ui::screen::Screen;

use common::{empty_engine, harness};

fn engine_with(db: Db) -> Engine {
    let mut e = empty_engine();
    e.replace_db(db)
        .expect("write the test database to the temp file");
    e
}

/// Copies `fixtures/statements/{name}` to a temp file with the same extension.
fn statement_file(name: &str) -> tempfile::NamedTempFile {
    let base = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/statements/");
    let text = std::fs::read_to_string(format!("{base}{name}")).expect("fixture");
    temp_file(&text, ".json")
}

fn temp_file(text: &str, suffix: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::Builder::new()
        .suffix(suffix)
        .tempfile()
        .expect("temp file");
    f.write_all(text.as_bytes()).expect("write");
    f
}

/// The checking account of the golden test (all four lines match; the fee is missing).
fn checking_db() -> (Db, AccountId) {
    let mut t = TestDb::new();
    let a = t.account("Checking", AccountType::Checking);
    t.tx(a, "2026-01-31", "1000", "Opening")
        .status(TransactionStatus::Reconciled)
        .done();
    t.tx(a, "2026-02-02", "2000", "Acme Payroll").done();
    t.tx(a, "2026-02-03", "-6.50", "Blue Bottle Coffee").done();
    t.tx(a, "2026-02-06", "-84.20", "Trader Joe's").done();
    t.tx(a, "2026-02-12", "-23.99", "Amazon").done();
    (t.build(), a)
}

fn open_with(db: Db, account: AccountId, file: &tempfile::NamedTempFile) -> PfApp {
    let files = FakeDialogs::default();
    files.push_answer(Some(file.path().to_path_buf()));
    let mut app = PfApp::new_for_test_with_files(engine_with(db), files);
    app.set_screen(Screen::Account(account));
    app
}

fn click(h: &mut egui_kittest::Harness<'_>, label: &str) {
    h.get_by_label(label).click();
    h.run();
    // Grids lay out over two frames; settle so the next click lands on the right row.
    h.run();
    h.run();
}

fn has(h: &egui_kittest::Harness<'_>, text: &str) -> bool {
    h.query_by_label_contains(text).is_some()
}

fn status_count(app: &PfApp, s: TransactionStatus) -> usize {
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    e.db().live_transactions().filter(|t| t.status == s).count()
}

#[test]
/// UI-REC-01, R-REC-14: load a JSON statement; Difference is $0.00; Finish is one undo step.
fn ui_rec_01_load_review_finish() {
    let (db, a) = checking_db();
    let file = statement_file("checking.json");
    let mut app = open_with(db, a, &file);
    {
        let mut h = harness(&mut app);
        h.run();
        click(&mut h, "Reconcile…");
        click(&mut h, "Choose File…");
        assert!(has(&h, "Statement 2026-02-01 to 2026-02-28"));
        assert!(has(&h, "Difference: $0.00"));
        assert!(has(&h, "Matched (4)"));
        assert!(has(&h, "On statement only (1)"));
        click(&mut h, "Finish");
        assert!(!has(&h, "Difference:"), "dialog closed");
    }
    // 1 opening + 4 pairs + 1 added fee.
    assert_eq!(status_count(&app, TransactionStatus::Reconciled), 6);
    let shared = app.engine();
    let mut e = shared.lock().expect("engine lock");
    assert_eq!(e.undo_name(), Some("Reconcile Statement"));
    e.undo().expect("undo");
    assert_eq!(e.db().live_transactions().count(), 5);
}

#[test]
/// UI-REC-02, R-REC-13: unticking a pair changes the Difference and the Finish button.
fn ui_rec_02_untick_pair_changes_difference() {
    let (db, a) = checking_db();
    let file = statement_file("checking.json");
    let mut app = open_with(db, a, &file);
    let mut h = harness(&mut app);
    h.run();
    click(&mut h, "Reconcile…");
    click(&mut h, "Choose File…");
    click(&mut h, "Accept SQ *BLUE BOTTLE 8812");
    assert!(has(&h, "Difference: -$6.50"));
    assert!(h.query_by_label("Finish with Adjustment…").is_some());
    click(&mut h, "Accept SQ *BLUE BOTTLE 8812");
    assert!(has(&h, "Difference: $0.00"));
    assert!(h.query_by_label("Finish").is_some());
}

#[test]
/// UI-REC-03: pair an "In PFRust only" transaction with an "On statement only" line by hand.
fn ui_rec_03_manual_pair() {
    let mut t = TestDb::new();
    let a = t.account("Checking", AccountType::Checking);
    t.tx(a, "2026-02-20", "-9.00", "Mystery").done();
    let db = t.build();
    let json = r#"{"period_start":"2026-02-01","period_end":"2026-02-28","opening_balance":"100","closing_balance":"90.50",
      "lines":[{"date":"2026-02-27","description":"FEE","amount":"-9.50"}]}"#;
    let file = temp_file(json, ".json");
    let mut app = open_with(db, a, &file);
    let mut h = harness(&mut app);
    h.run();
    click(&mut h, "Reconcile…");
    click(&mut h, "Choose File…");
    assert!(has(&h, "On statement only (1)"));
    assert!(has(&h, "In PFRust only, in this period (1)"));
    click(&mut h, "Pair with Mystery");
    h.get_by_label_contains("2026-02-27 FEE").click();
    h.run();
    assert!(!has(&h, "On statement only"));
    assert!(!has(&h, "In PFRust only, in this period"));
    assert!(has(&h, "Matched (1)"));
    assert!(has(&h, "Manual"));
    click(&mut h, "Unpair");
    assert!(has(&h, "On statement only (1)"));
}

#[test]
/// UI-REC-04, R-REC-17: editing the amount of a Reconciled row asks; Cancel keeps it, Change saves it;
/// a payee edit does not ask; deleting a Reconciled row asks.
fn ui_rec_04_reconciled_edit_warning() {
    let mut t = TestDb::new();
    let a = t.account("Checking", AccountType::Checking);
    let rec = t
        .tx(a, "2026-03-01", "-4.50", "Coffee")
        .status(TransactionStatus::Reconciled)
        .done();
    let db = t.build();
    let mut app = PfApp::new_for_test(engine_with(db));
    app.set_screen(Screen::Account(a));
    let amount = |app: &PfApp| {
        let shared = app.engine();
        let e = shared.lock().expect("engine lock");
        e.db().transaction(rec).expect("row").amount.to_string()
    };
    {
        let mut h = harness(&mut app);
        h.run();
        // Payee: no warning.
        h.get_by_label("Payee Coffee").click();
        h.run();
        h.get_by_label("Payee Coffee").type_text(" Shop");
        h.run();
        h.key_press(Key::Enter);
        h.run();
        assert!(!has(&h, "Change Reconciled Transaction?"));
        // Amount: Cancel.
        h.get_by_label("Amount Coffee Shop").click();
        h.run();
        h.get_by_label("Amount Coffee Shop").focus();
        h.run();
        h.key_press_modifiers(Modifiers::COMMAND, Key::A);
        h.run();
        h.get_by_label("Amount Coffee Shop").type_text("-42.5");
        h.run();
        h.key_press(Key::Enter);
        h.run();
        assert!(has(&h, "Change Reconciled Transaction?"));
        click(&mut h, "Cancel");
    }
    assert_eq!(amount(&app), "-4.50");
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Amount Coffee Shop").click();
        h.run();
        h.get_by_label("Amount Coffee Shop").focus();
        h.run();
        h.key_press_modifiers(Modifiers::COMMAND, Key::A);
        h.run();
        h.get_by_label("Amount Coffee Shop").type_text("-42.5");
        h.run();
        h.key_press(Key::Enter);
        h.run();
        click(&mut h, "Change");
    }
    assert_eq!(amount(&app), "-42.5");
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Payee Coffee Shop").click();
        h.run();
        h.get_by_label("Payee Coffee Shop").click_secondary();
        h.run();
        click(&mut h, "Delete");
        assert!(has(&h, "Delete Reconciled Transaction?"));
        click(&mut h, "Delete");
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    assert_eq!(e.db().live_transactions().count(), 0);
}

#[test]
/// R-REC-2: a CSV asks for the period and balances; a bad row is an error.
fn ui_rec_csv_needs_balances() {
    let mut t = TestDb::new();
    let a = t.account("Checking", AccountType::Checking);
    let file = temp_file("Date,Amount\n2026-02-02,-5\nbad,1\n", ".csv");
    let mut app = open_with(t.build(), a, &file);
    let mut h = harness(&mut app);
    h.run();
    click(&mut h, "Reconcile…");
    click(&mut h, "Choose File…");
    for (label, text) in [
        ("Period start", "2026-02-01"),
        ("Period end", "2026-02-28"),
        ("Opening balance", "100"),
        ("Closing balance", "95"),
    ] {
        h.get_by_role_and_label(Role::TextInput, label).click();
        h.run();
        h.get_by_role_and_label(Role::TextInput, label)
            .type_text(text);
        h.run();
    }
    click(&mut h, "Load");
    assert!(has(&h, "Line 2: invalid date or amount."));
}

#[test]
/// R-REC-7: Flip signs changes the reading line and the counts.
fn ui_rec_flip_signs() {
    let mut t = TestDb::new();
    let a = t.account("Mastercard", AccountType::Credit);
    t.tx(a, "2026-02-14", "-40", "Shell").done();
    t.tx(a, "2026-02-25", "300", "Payment").done();
    let file = statement_file("card-single.json");
    let mut app = open_with(t.build(), a, &file);
    let mut h = harness(&mut app);
    h.run();
    click(&mut h, "Reconcile…");
    click(&mut h, "Choose File…");
    assert!(has(&h, "Read as: owed balances, statement signs flipped"));
    assert!(has(&h, "(2 of 3 lines match)"));
    click(&mut h, "Flip signs");
    assert!(has(
        &h,
        "Read as: asset balances, statement signs as printed"
    ));
    assert!(has(&h, "(0 of 3 lines match)"));
}

#[test]
/// R-REC-13, R-REC-14: the first-time note and checkbox; ticking it marks older rows on Finish.
fn ui_rec_first_time_checkbox() {
    let mut t = TestDb::new();
    let a = t.account("Checking", AccountType::Checking);
    let old: TransactionId = t.tx(a, "2026-01-05", "100", "Starting balance").done();
    t.tx(a, "2026-02-03", "-5", "Coffee").done();
    let json = r#"{"period_start":"2026-02-01","period_end":"2026-02-28","opening_balance":"100","closing_balance":"95",
      "lines":[{"date":"2026-02-03","description":"COFFEE","amount":"-5"}]}"#;
    let file = temp_file(json, ".json");
    let mut app = open_with(t.build(), a, &file);
    {
        let mut h = harness(&mut app);
        h.run();
        click(&mut h, "Reconcile…");
        click(&mut h, "Choose File…");
        assert!(has(
            &h,
            "This is the first reconciliation for this account."
        ));
        click(
            &mut h,
            "Mark earlier transactions in this account as Reconciled",
        );
        click(&mut h, "Finish");
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    assert_eq!(
        e.db().transaction(old).expect("row").status,
        TransactionStatus::Reconciled
    );
}

#[test]
/// R-REC-14: "Reconcile…" is hidden while a scenario is active.
fn ui_rec_button_reality_only() {
    let mut t = TestDb::new();
    let a = t.account("Checking", AccountType::Checking);
    let s = t.scenario("Plan");
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Account(a));
    {
        let mut h = harness(&mut app);
        h.run();
        assert!(h.query_by_label("Reconcile…").is_some());
    }
    app.engine()
        .lock()
        .expect("engine lock")
        .set_active_scenario(Some(s));
    let mut h = harness(&mut app);
    h.run();
    assert!(h.query_by_label("Reconcile…").is_none());
}
