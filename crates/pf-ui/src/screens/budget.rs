//! Budget screen (T17, Swift `BudgetView`): the Bills, Expenses and Incomes tabs, a table of the
//! items on the tab, a context menu (Edit, Delete), and the Project and Add buttons. Writes go
//! through `commands::budget`; the dialogs live in `dialogs::{budget_item, projection}`.

use std::collections::BTreeSet;

use egui::{Align, Layout, RichText, Sense, Ui};
use egui_extras::{Column, TableBuilder};
use jiff::civil::Date;
use pf_core::format;
use pf_engine::commands::budget::delete_budget_items;
use pf_model::{BudgetFrequency, BudgetItemId, BudgetItemType, Db};
use rust_decimal::Decimal;

use crate::app::UiCtx;
use crate::caches::Cached;
use crate::dialogs::Dialog;
use crate::dialogs::budget_item::BudgetItemForm;
use crate::dialogs::projection::ProjectionForm;
use crate::widgets::col_fit::{fit, longest};
use crate::widgets::colors;

/// Row height in points.
const ROW_HEIGHT: f32 = 24.0;
/// Header row height in points.
const HEADER_HEIGHT: f32 = 20.0;

/// Per-screen UI state.
pub struct State {
    /// The tab shown (Bill by default).
    pub tab: BudgetItemType,
    /// Selected items (click, ⌘-click).
    pub selection: BTreeSet<BudgetItemId>,
    /// The table's sideways scroll (clamped at both ends).
    pub hscroll: crate::widgets::hscroll::HScroll,
    /// The rows of the current tab, cached per engine revision and tab.
    rows: Cached<BudgetItemType, Vec<BudgetRow>>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            tab: BudgetItemType::Bill,
            selection: BTreeSet::new(),
            hscroll: Default::default(),
            rows: Cached::default(),
        }
    }
}

/// One row of the budget table, fully resolved for drawing.
#[derive(Debug, Clone, PartialEq)]
pub struct BudgetRow {
    /// The item.
    pub id: BudgetItemId,
    /// Payee, expense name or source.
    pub name: String,
    /// The live category's full name, or `None` (drawn as "Uncategorized").
    pub category_label: Option<String>,
    /// Amount per occurrence.
    pub amount: Decimal,
    /// Income items draw their amount green.
    pub is_income: bool,
    /// "Monthly", or "Specific Months (n)".
    pub frequency_label: String,
    /// First occurrence.
    pub start: Date,
    /// The live account's name, if any.
    pub account: Option<String>,
}

/// The tab title: the type's label with an "s" (Swift `rawValue + "s"`): "Bills", "Expenses", "Incomes".
pub fn tab_title(tab: BudgetItemType) -> String {
    format!("{}s", tab.label())
}

/// The header of the first column for a tab (Swift): "Payee", "Expense Name" or "Source".
fn name_header(tab: BudgetItemType) -> &'static str {
    match tab {
        BudgetItemType::Bill => "Payee",
        BudgetItemType::Expense => "Expense Name",
        BudgetItemType::Income => "Source",
    }
}

/// R-BUD-1: the live items of `tab`, sorted by name (ties by id).
pub fn build_rows(db: &Db, tab: BudgetItemType) -> Vec<BudgetRow> {
    db.budget_items_by_name()
        .into_iter()
        .filter(|b| b.item_type == tab)
        .map(|b| BudgetRow {
            id: b.id,
            name: b.name.clone(),
            category_label: b
                .category_id
                .filter(|&c| db.category(c).is_some_and(|c| !c.is_deleted))
                .map(|c| db.category_full_name(c)),
            amount: b.amount,
            is_income: b.item_type == BudgetItemType::Income,
            frequency_label: frequency_label(b.frequency, b.specific_months.len()),
            start: b.start_date,
            account: b
                .account_id
                .and_then(|a| db.account(a))
                .filter(|a| !a.is_deleted)
                .map(|a| a.name.clone()),
        })
        .collect()
}

/// "Specific Months (n)" for a specific-months item; otherwise the frequency's label.
pub fn frequency_label(frequency: BudgetFrequency, specific_months: usize) -> String {
    match frequency {
        BudgetFrequency::SpecificMonths => format!("Specific Months ({specific_months})"),
        other => other.label().to_string(),
    }
}

/// What the user asked for this frame, applied after the table is drawn.
enum Action {
    Add,
    Project,
    Edit(BudgetItemId),
    Delete(Vec<BudgetItemId>),
}

/// Draws the Budget screen into the central panel.
pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut Ui) {
    let revision = cx.engine.revision();
    let mut actions: Vec<Action> = Vec::new();

    ui.horizontal(|ui| {
        ui.heading("Budget");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .button("+ Add")
                .on_hover_text("Add a budget item")
                .clicked()
            {
                actions.push(Action::Add);
            }
            if ui
                .button("Project")
                .on_hover_text("Generate a projection into a scenario")
                .clicked()
            {
                actions.push(Action::Project);
            }
        });
    });
    ui.horizontal(|ui| {
        for tab in BudgetItemType::ALL {
            let before = st.tab;
            ui.selectable_value(&mut st.tab, tab, tab_title(tab));
            if st.tab != before {
                st.selection.clear();
            }
        }
    });
    ui.separator();

    let tab = st.tab;
    let rows = st
        .rows
        .get(revision, &tab, || build_rows(cx.engine.db(), tab));
    if rows.is_empty() {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| ui.label(RichText::new("No budget items").weak()));
    } else {
        table(
            ui,
            rows,
            tab,
            &mut st.selection,
            &mut actions,
            &mut st.hscroll,
        );
    }

    for action in actions {
        match action {
            Action::Add => {
                let today = cx.engine.today();
                cx.dialogs
                    .open(Dialog::BudgetItemEditor(Box::new(BudgetItemForm::new_for(
                        tab, today,
                    ))));
            }
            Action::Project => {
                let today = cx.engine.today();
                let form = ProjectionForm::new(cx.engine.db(), today);
                cx.dialogs.open(Dialog::Projection(Box::new(form)));
            }
            Action::Edit(id) => {
                let today = cx.engine.today();
                if let Some(form) = BudgetItemForm::edit(cx.engine.db(), id, today) {
                    cx.dialogs.open(Dialog::BudgetItemEditor(Box::new(form)));
                }
            }
            Action::Delete(ids) => {
                if let Err(e) = delete_budget_items(cx.engine, &ids) {
                    cx.toasts.info(e.to_string());
                }
                st.selection.clear();
            }
        }
    }
}

/// The table: name, category, amount, frequency, start date and account. A click selects, ⌘-click
/// toggles, double-click opens the editor, and the context menu offers Edit (one row) and Delete.
fn table(
    ui: &mut Ui,
    rows: &[BudgetRow],
    tab: BudgetItemType,
    selection: &mut BTreeSet<BudgetItemId>,
    actions: &mut Vec<Action>,
    hscroll: &mut crate::widgets::hscroll::HScroll,
) {
    let modifiers = ui.input(|i| i.modifiers);
    let mut clicked: Option<BudgetItemId> = None;
    let mut double: Option<BudgetItemId> = None;
    let name_title = name_header(tab);
    // Each column starts as wide as its widest text (see `widgets::col_fit`).
    let category_w = fit(
        ui,
        "Category",
        &longest(rows.iter().filter_map(|r| r.category_label.as_deref())),
    );
    let amount_w = fit(ui, "Amount", &["-$999,999.99"]);
    let frequency_w = fit(ui, "Frequency", &["Specific Months (12)"]);
    let start_w = fit(ui, "Start Date", &["Sep 30, 2026"]);
    let account_w = fit(
        ui,
        "Account",
        &longest(rows.iter().filter_map(|r| r.account.as_deref())),
    );
    crate::widgets::hscroll::show(ui, hscroll, |ui| {
        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .sense(Sense::click())
            .auto_shrink([false, false])
            .max_scroll_height(f32::INFINITY)
            .column(Column::remainder().at_least(160.0))
            .column(Column::initial(category_w))
            .column(Column::initial(amount_w))
            .column(Column::initial(frequency_w))
            .column(Column::initial(start_w))
            .column(Column::initial(account_w))
            .header(HEADER_HEIGHT, |mut header| {
                for title in [
                    name_title,
                    "Category",
                    "Amount",
                    "Frequency",
                    "Start Date",
                    "Account",
                ] {
                    header.col(|ui| {
                        ui.strong(title);
                    });
                }
            })
            .body(|body| {
                body.rows(ROW_HEIGHT, rows.len(), |mut row| {
                    let r = &rows[row.index()];
                    row.set_selected(selection.contains(&r.id));
                    row.col(|ui| {
                        ui.label(&r.name);
                    });
                    row.col(|ui| match &r.category_label {
                        Some(name) => {
                            ui.label(name);
                        }
                        None => {
                            ui.label(RichText::new("Uncategorized").weak());
                        }
                    });
                    row.col(|ui| {
                        // Top-aligned, like the other cells; right-aligned as money.
                        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                            // Income is green (Swift).
                            let text = RichText::new(format::format_currency(r.amount));
                            if r.is_income {
                                ui.label(text.color(colors::GREEN));
                            } else {
                                ui.label(text);
                            }
                        });
                    });
                    row.col(|ui| {
                        ui.label(&r.frequency_label);
                    });
                    row.col(|ui| {
                        ui.label(format_date(r.start));
                    });
                    row.col(|ui| match &r.account {
                        Some(name) => {
                            ui.label(name);
                        }
                        None => {
                            ui.label(RichText::new("—").weak());
                        }
                    });
                    let response = row.response();
                    if response.double_clicked() {
                        double = Some(r.id);
                    } else if response.clicked() {
                        clicked = Some(r.id);
                    }
                    if response.container_secondary_clicked() && !selection.contains(&r.id) {
                        selection.clear();
                        selection.insert(r.id);
                    }
                    response.container_context_menu(|ui| {
                        let one = selection.len() == 1;
                        if ui
                            .add_enabled(one, egui::Button::new("Edit"))
                            .on_disabled_hover_text("Select one item to edit")
                            .clicked()
                            && let Some(id) = selection.iter().next().copied()
                        {
                            actions.push(Action::Edit(id));
                            ui.close();
                        }
                        if ui.button("Delete").clicked() {
                            actions.push(Action::Delete(selection.iter().copied().collect()));
                            ui.close();
                        }
                    });
                });
            });
    });
    if let Some(id) = double {
        selection.clear();
        selection.insert(id);
        actions.push(Action::Edit(id));
    } else if let Some(id) = clicked {
        if modifiers.command {
            if !selection.remove(&id) {
                selection.insert(id);
            }
        } else {
            selection.clear();
            selection.insert(id);
        }
    }
}

/// "Mar 1, 2026" (the Swift date format).
fn format_date(d: Date) -> String {
    d.strftime("%b %-d, %Y").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::RowKey;
    use pf_model::testing::TestDb;

    /// Names of the rows, in order.
    fn names(rows: &[BudgetRow]) -> Vec<&str> {
        rows.iter().map(|r| r.name.as_str()).collect()
    }

    #[test]
    /// Covers: R-BUD-1. Each tab shows only its type; rows are sorted by name; deleted items are absent.
    fn r_bud_1_rows_by_tab_sorted_live_only() {
        let mut t = TestDb::new();
        t.budget_item(
            BudgetItemType::Bill,
            "Zeta",
            "10",
            "2026-01-01",
            BudgetFrequency::Monthly,
        )
        .done();
        t.budget_item(
            BudgetItemType::Bill,
            "Alpha",
            "20",
            "2026-01-01",
            BudgetFrequency::Monthly,
        )
        .done();
        t.budget_item(
            BudgetItemType::Expense,
            "Food",
            "30",
            "2026-01-01",
            BudgetFrequency::Weekly,
        )
        .done();
        let gone = t
            .budget_item(
                BudgetItemType::Bill,
                "Gone",
                "40",
                "2026-01-01",
                BudgetFrequency::Yearly,
            )
            .done();
        t.delete(RowKey::BudgetItem(gone));
        let db = t.build();

        assert_eq!(
            names(&build_rows(&db, BudgetItemType::Bill)),
            vec!["Alpha", "Zeta"]
        );
        assert_eq!(
            names(&build_rows(&db, BudgetItemType::Expense)),
            vec!["Food"]
        );
        assert!(build_rows(&db, BudgetItemType::Income).is_empty());
    }

    #[test]
    /// "Specific Months (n)" counts the months; other frequencies show their label.
    fn budget_frequency_label_specific_months_count() {
        assert_eq!(
            frequency_label(BudgetFrequency::SpecificMonths, 3),
            "Specific Months (3)"
        );
        assert_eq!(frequency_label(BudgetFrequency::Monthly, 0), "Monthly");
    }
}
