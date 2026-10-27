//! L5 UI tests for the Categories screen and editor (T20): the tree by type, a child under its
//! parent, the invalid-pattern alert, Apply, the parent picker's type rule, and delete with undo.
//! Headless (egui_kittest); assertions are on engine state and on what the screen draws.

mod common;

use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::kittest::{NodeT, Queryable};
use pf_engine::Engine;
use pf_model::testing::TestDb;
use pf_model::{AccountType, CategoryType, Db, TransactionStatus};
use pf_ui::PfApp;
use pf_ui::dialogs::Dialog;
use pf_ui::dialogs::category_edit::CategoryForm;
use pf_ui::screen::Screen;

use common::{empty_engine, harness};

/// Engine holding `db`, on a temp file (never the app's database).
fn engine_with(db: Db) -> Engine {
    let mut e = empty_engine();
    e.replace_db(db)
        .expect("write the test database to the temp file");
    e
}

/// Clicks the text field labelled `label` (by role), then types `text` into it.
fn type_field(h: &mut egui_kittest::Harness<'_>, label: &str, text: &str) {
    h.get_by_role_and_label(Role::TextInput, label).click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, label)
        .type_text(text);
    h.run();
}

/// Picks `option` from the combo box labelled `label` (the option drawn last).
fn pick(h: &mut egui_kittest::Harness<'_>, label: &str, option: &str) {
    h.get_by_role_and_label(Role::ComboBox, label).click();
    h.run();
    let options: Vec<_> = h.get_all_by_label(option).collect();
    options.last().expect("the option").click();
    h.run();
}

#[test]
/// UI-CAT-01, R-CAT-8: a child created under "Dining Out" appears after it, indented.
fn ui_cat_01_child_shown_under_parent() {
    let mut t = TestDb::new();
    t.category("Dining Out", CategoryType::Expense);
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Categories);
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("+ Add Category").click();
        h.run();
        type_field(&mut h, "Name", "Coffee");
        pick(&mut h, "Parent Category", "Dining Out");
        h.get_by_label("Save").click();
        h.run();
        let parent_x = h.get_by_label("Dining Out").rect().min.x;
        let child_x = h.get_by_label("Coffee").rect().min.x;
        assert!(child_x > parent_x, "child is indented under its parent");
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    assert!(
        e.db()
            .live_categories()
            .any(|c| c.name == "Coffee" && c.parent_id.is_some())
    );
}

#[test]
/// R-CAT-8: Income is listed before Expense; Transfer is left out when it has no categories.
fn ui_cat_sections_by_type() {
    let mut t = TestDb::new();
    t.category("Groceries", CategoryType::Expense);
    t.category("Salary", CategoryType::Income);
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Categories);
    let mut h = harness(&mut app);
    h.run();
    let income_y = h.get_by_label("Income").rect().min.y;
    let expense_y = h.get_by_label("Expense").rect().min.y;
    assert!(income_y < expense_y, "Income first");
    assert!(
        h.query_by_label("Transfer").is_none(),
        "no transfer section"
    );
}

#[test]
/// R-CAT-2: an invalid pattern is removed on save, and the alert lists it.
fn ui_cat_invalid_regex_alert() {
    let mut t = TestDb::new();
    t.category("Food", CategoryType::Expense);
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Categories);
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("+ Add Category").click();
        h.run();
        type_field(&mut h, "Name", "Shops");
        type_field(&mut h, "Payee Auto-Match Rules", "ok, [bad");
        h.get_by_label("Save").click();
        h.run();
        assert!(
            h.query_by_label("Invalid Regex Pattern").is_some(),
            "alert shown"
        );
        assert!(
            h.query_by_label_contains("• [bad").is_some(),
            "lists the bad pattern"
        );
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    let c = e
        .db()
        .live_categories()
        .find(|c| c.name == "Shops")
        .expect("category");
    assert_eq!(c.match_patterns, vec!["ok".to_string()]);
}

#[test]
/// R-CAT-3: Apply runs the typed rules over uncategorized transactions and reports the count.
fn ui_cat_apply_rules_alert() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    let transport = t.category("Transport", CategoryType::Expense);
    t.tx(chk, "2026-03-01", "-18", "Uber Trip")
        .status(TransactionStatus::Cleared)
        .done();
    let db = t.build();
    let form = CategoryForm::edit(&db, transport).expect("form");
    let mut app = PfApp::new_for_test(engine_with(db));
    app.set_screen(Screen::Categories);
    app.open_dialog_for_test(Dialog::CategoryEdit(Box::new(CategoryForm {
        patterns: "uber".into(),
        ..form
    })));
    let mut h = harness(&mut app);
    h.run();
    h.get_by_label("Apply").click();
    h.run();
    assert!(
        h.query_by_label("Applied to 1 uncategorized transaction(s).")
            .is_some(),
        "count shown"
    );
}

#[test]
/// In a new category, Apply is disabled: there is no saved category to apply.
fn ui_cat_apply_disabled_for_new() {
    let mut app = PfApp::new_for_test(engine_with(TestDb::new().build()));
    app.set_screen(Screen::Categories);
    let mut h = harness(&mut app);
    h.run();
    h.get_by_label("+ Add Category").click();
    h.run();
    assert!(h.get_by_label("Apply").accesskit_node().is_disabled());
}

#[test]
/// The parent list holds only categories of the chosen type.
fn ui_cat_parent_picker_same_type_only() {
    let mut t = TestDb::new();
    t.category("Dining Out", CategoryType::Expense);
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Categories);
    let mut h = harness(&mut app);
    h.run();
    h.get_by_label("+ Add Category").click();
    h.run();
    pick_type(&mut h, "Income");
    h.get_by_role_and_label(Role::ComboBox, "Parent Category")
        .click();
    h.run();
    assert_eq!(
        // One match is the screen's own row; the open parent list must add no other.
        h.get_all_by_label("Dining Out").count(),
        1,
        "no expense parents for income"
    );
}

/// Clicks the type choice `label` in the category editor.
fn pick_type(h: &mut egui_kittest::Harness<'_>, label: &str) {
    let options: Vec<_> = h.get_all_by_label(label).collect();
    options.last().expect("the type").click();
    h.run();
}

#[test]
/// R-CAT-7: deleting a parent removes it and its child; their transactions lose the category. Cmd-Z
/// brings all of them back.
fn ui_cat_delete_cascades_undo() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    let food = t.category("Food", CategoryType::Expense);
    let cafe = t.child_category(food, "Cafe");
    let tx = t
        .tx(chk, "2026-03-01", "-4", "Morning")
        .category(food)
        .status(TransactionStatus::Cleared)
        .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Categories);
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Food").click_secondary();
        h.run();
        h.get_by_label("Delete").click();
        h.run();
    }
    {
        let shared = app.engine();
        let e = shared.lock().expect("engine lock");
        assert!(e.db().category(food).is_some_and(|c| c.is_deleted));
        assert!(e.db().category(cafe).is_some_and(|c| c.is_deleted));
        assert_eq!(e.db().transaction(tx).expect("tx").category_id, None);
    }
    {
        let mut h = harness(&mut app);
        h.run();
        h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
        h.run();
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    assert!(
        e.db().category(food).is_some_and(|c| !c.is_deleted),
        "parent back"
    );
    assert!(
        e.db().category(cafe).is_some_and(|c| !c.is_deleted),
        "child back"
    );
    assert_eq!(e.db().transaction(tx).expect("tx").category_id, Some(food));
}

#[test]
/// R-CAT-7: the right-click menu offers Edit, which opens the category editor (the row has no edit button).
fn ui_cat_context_menu_edit_opens_editor() {
    let mut t = TestDb::new();
    t.category("Food", CategoryType::Expense);
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Categories);
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Food").click_secondary();
        h.run();
        // The app's own top-bar "Edit" menu is drawn first; the row's menu item comes last.
        h.get_all_by_role_and_label(Role::Button, "Edit")
            .last()
            .expect("Edit in the row menu")
            .click();
        h.run();
        assert!(
            h.query_by_label("Edit Category").is_some()
                || h.query_by_value("Edit Category").is_some()
        );
    }
}
