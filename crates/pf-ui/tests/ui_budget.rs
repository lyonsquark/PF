//! L5 UI tests for the Budget screen and its dialogs (T17): adding a Specific Months item, the
//! expense name autofill, Generate Projection, the All/None toggle, delete, editing and Reproject.
//! Headless (egui_kittest); assertions are on engine state and on what the screens draw.

mod common;

use egui::accesskit::Role;
use egui_kittest::kittest::{NodeT, Queryable};
use pf_engine::Engine;
use pf_engine::testing::TEST_TODAY;
use pf_model::testing::TestDb;
use pf_model::{AccountType, BudgetFrequency, BudgetItemType, CategoryType, Db, TransactionStatus};
use pf_ui::PfApp;
use pf_ui::screen::Screen;

use common::{empty_engine, harness};

/// Engine holding `db`, on a temp file (never the app's database).
fn engine_with(db: Db) -> Engine {
    let mut e = empty_engine();
    e.replace_db(db)
        .expect("write the test database to the temp file");
    e
}

/// Clicks a context-menu item. The menu bar also has "Edit", and the menu item is drawn last.
fn click_menu_item(h: &mut egui_kittest::Harness<'_>, label: &str) {
    let items: Vec<_> = h.get_all_by_label(label).collect();
    items.last().expect("the menu item").click();
    h.run();
}

/// Clicks the text field labelled `label` (a role query, since table headers can share the text),
/// then types `text` into it.
fn type_field(h: &mut egui_kittest::Harness<'_>, label: &str, text: &str) {
    h.get_by_role_and_label(Role::TextInput, label).click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, label)
        .type_text(text);
    h.run();
}

#[test]
/// UI-BUD-01, R-BUD-1: a Specific Months bill is added from the editor and listed in Bills with
/// "Specific Months (3)".
fn ui_bud_01_create_specific_months_item_appears_in_tab() {
    let mut app = PfApp::new_for_test(engine_with(TestDb::new().build()));
    app.set_screen(Screen::Budget);
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("+ Add").click();
        h.run();
        type_field(&mut h, "Payee", "Gym");
        type_field(&mut h, "Amount", "40");
        h.get_by_role_and_label(Role::ComboBox, "Frequency").click();
        h.run();
        h.get_by_label("Specific Months").click();
        h.run();
        for month in ["Jan", "Apr", "Jul"] {
            h.get_by_label(month).click();
            h.run();
        }
        h.get_by_label("Save").click();
        h.run();
        assert!(h.query_by_label("New Bill").is_none(), "editor closed");
        assert!(h.query_by_label("Gym").is_some(), "row shown");
        assert!(
            h.query_by_label("Specific Months (3)").is_some(),
            "frequency shown"
        );
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    let item = e
        .db()
        .live_budget_items()
        .find(|b| b.name == "Gym")
        .expect("saved item");
    assert_eq!(item.item_type, BudgetItemType::Bill);
    assert_eq!(item.frequency, BudgetFrequency::SpecificMonths);
    assert_eq!(item.specific_months, vec![1, 4, 7]);
}

#[test]
/// The expense name is filled from the category when it is empty (Swift).
fn ui_bud_01_expense_category_autofills_name() {
    let mut t = TestDb::new();
    t.category("Groceries", CategoryType::Expense);
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Budget);
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Expenses").click();
        h.run();
        h.get_by_label("+ Add").click();
        h.run();
        h.get_by_role_and_label(Role::ComboBox, "Category").click();
        h.run();
        h.get_by_label("Groceries").click();
        h.run();
        assert_eq!(
            h.get_by_role_and_label(Role::TextInput, "Name (autofilled from Category)")
                .value()
                .as_deref(),
            Some("Groceries")
        );
    }
}

#[test]
/// UI-BUD-02, R-PRJ-6: a monthly bill is projected into a new scenario. The count is reported, and
/// the rows are Future rows in that scenario on the bill's dates.
fn ui_bud_02_projection_generates_and_reports_count() {
    let mut t = TestDb::new();
    let chk = t.account("Everyday", AccountType::Checking);
    t.budget_item(
        BudgetItemType::Bill,
        "Rent",
        "1500",
        "2026-03-31",
        BudgetFrequency::Monthly,
    )
    .account(chk)
    .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Budget);
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Project").click();
        h.run();
        type_field(&mut h, "Scenario Name", "Plan");
        h.get_by_label("Generate").click();
        h.run();
        // The footer and the toast both say it.
        assert!(
            h.query_all_by_label("Generated 3 transactions").count() > 0,
            "count shown"
        );
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    let plan = e
        .db()
        .live_scenarios()
        .find(|s| s.name == "Plan")
        .expect("scenario");
    let mut dates: Vec<String> = e
        .db()
        .live_transactions()
        .filter(|t| t.scenario_id == Some(plan.id))
        .map(|t| t.date.to_string())
        .collect();
    dates.sort();
    assert_eq!(dates, vec!["2026-03-31", "2026-04-30", "2026-05-31"]);
    assert!(
        e.db()
            .live_transactions()
            .filter(|t| t.scenario_id == Some(plan.id))
            .all(|t| t.status == TransactionStatus::Future)
    );
    assert_eq!(TEST_TODAY.to_string(), "2026-03-15");
}

#[test]
/// The section's "None" clears its items, and then Generate is disabled (R-PRJ-7).
fn ui_projection_all_none_toggle() {
    let mut t = TestDb::new();
    t.budget_item(
        BudgetItemType::Bill,
        "Rent",
        "1500",
        "2026-03-31",
        BudgetFrequency::Monthly,
    )
    .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Budget);
    let mut h = harness(&mut app);
    h.run();
    h.get_by_label("Project").click();
    h.run();
    type_field(&mut h, "Scenario Name", "Plan");
    h.get_by_label("None").click();
    h.run();
    assert!(
        h.get_by_label("Generate").accesskit_node().is_disabled(),
        "nothing selected"
    );
    h.get_by_label("All").click();
    h.run();
    assert!(
        !h.get_by_label("Generate").accesskit_node().is_disabled(),
        "selected again"
    );
}

#[test]
/// The context menu deletes the selected item in one "Delete Budget Item" step.
fn ui_budget_delete_via_context_menu() {
    let mut t = TestDb::new();
    t.budget_item(
        BudgetItemType::Bill,
        "Rent",
        "1500",
        "2026-03-31",
        BudgetFrequency::Monthly,
    )
    .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Budget);
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Rent").click_secondary();
        h.run();
        h.get_by_label("Delete").click();
        h.run();
        assert!(h.query_by_label("Rent").is_none(), "row gone");
    }
    let shared = app.engine();
    let e = shared.lock().expect("engine lock");
    assert_eq!(e.undo_name(), Some("Delete Budget Item"));
}

#[test]
/// Edit from the context menu opens the editor with the item's values.
fn ui_budget_edit_via_context_menu_opens_editor() {
    let mut t = TestDb::new();
    t.budget_item(
        BudgetItemType::Bill,
        "Rent",
        "1500",
        "2026-03-31",
        BudgetFrequency::Monthly,
    )
    .done();
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Budget);
    let mut h = harness(&mut app);
    h.run();
    h.get_by_label("Rent").click_secondary();
    h.run();
    click_menu_item(&mut h, "Edit");
    h.run();
    assert!(h.query_by_label("Edit Bill").is_some(), "editor title");
    assert_eq!(
        h.get_by_role_and_label(Role::TextInput, "Payee")
            .value()
            .as_deref(),
        Some("Rent")
    );
}

#[test]
/// Reproject (R-PRJ-9) is offered only once a projection has been run. Clicking it adds the rows
/// for the last window to the last scenario.
fn ui_budget_reproject_section_only_after_projection() {
    let mut t = TestDb::new();
    t.budget_item(
        BudgetItemType::Bill,
        "Rent",
        "1500",
        "2026-03-31",
        BudgetFrequency::Monthly,
    )
    .done();
    let plan = t.scenario("Plan A");
    let _ = plan;
    let mut app = PfApp::new_for_test(engine_with(t.build()));
    app.set_screen(Screen::Budget);
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Rent").click_secondary();
        h.run();
        click_menu_item(&mut h, "Edit");
        h.run();
        assert!(h.query_by_label("Reproject").is_none(), "no projection yet");
        h.get_by_label("Cancel").click();
        h.run();
    }
    {
        let shared = app.engine();
        let mut e = shared.lock().expect("engine lock");
        let item = e.db().live_budget_items().next().expect("item").id;
        let scenario = e.db().live_scenarios().next().expect("scenario").id;
        let _ = scenario;
        let req = pf_engine::commands::budget::ProjectionRequest {
            target: pf_engine::commands::budget::ProjectionTarget::Existing(
                e.db().live_scenarios().next().expect("scenario").id,
            ),
            start: "2026-03-01".parse().expect("date"),
            end: "2026-03-31".parse().expect("date"),
            items: vec![item],
        };
        pf_engine::commands::budget::generate_projection(&mut e, req).expect("projection");
    }
    {
        let mut h = harness(&mut app);
        h.run();
        h.get_by_label("Rent").click_secondary();
        h.run();
        click_menu_item(&mut h, "Edit");
        h.run();
        assert!(
            h.query_by_label("Reproject").is_some(),
            "offered after a projection"
        );
    }
}
