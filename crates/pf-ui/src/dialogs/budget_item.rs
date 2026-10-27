//! The budget item editor (T17, Swift `BudgetItemEditorView`): add or edit a bill, an expense or an
//! income. The form logic (name label, amount and end-date rules) is pure and unit-tested; `show`
//! draws it and runs the chosen action. Reproject (R-PRJ-9) shows only after a projection.

use egui::{ComboBox, Id, Modal, RichText, Role, TextEdit, Ui, WidgetInfo};
use egui_extras::DatePickerButton;
use jiff::Span;
use jiff::civil::Date;
use pf_engine::commands::budget::{
    BudgetItemInput, default_budget_input, last_projection, reproject_item, save_budget_item,
};
use pf_model::{AccountId, BudgetFrequency, BudgetItemId, BudgetItemType, Db};

use super::Outcome;
use crate::app::UiCtx;
use crate::widgets::amount_field::{amount_edit_text, parse_amount_input};
use crate::widgets::category::category_picker;
use crate::widgets::colors;
use crate::widgets::labeled_text;

/// Dialog width in points.
const MIN_WIDTH: f32 = 420.0;
/// Short month names for the Specific Months checkboxes, January first.
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// The form state of the editor. It lives in `Dialog::BudgetItemEditor` while open.
#[derive(Debug, Clone, PartialEq)]
pub struct BudgetItemForm {
    /// The item being edited; `None` when adding.
    pub id: Option<BudgetItemId>,
    /// The values the engine saves. The amount and end date are filled in from the fields on save.
    pub input: BudgetItemInput,
    /// The amount as typed; parsed on save.
    pub amount_text: String,
    /// Whether the item has an end date (R-BUD-2).
    pub has_end_date: bool,
    /// The end date, used when `has_end_date` is set.
    pub end_date: Date,
    /// An error from the last save or reproject, shown in red.
    pub error: Option<String>,
}

impl BudgetItemForm {
    /// A new item of `tab`, with the engine's defaults (start of this year, end a year out).
    pub fn new_for(tab: BudgetItemType, today: Date) -> Self {
        let input = default_budget_input(tab, today);
        Self {
            id: None,
            has_end_date: input.end_date.is_some(),
            end_date: input.end_date.unwrap_or(today),
            input,
            amount_text: String::new(),
            error: None,
        }
    }

    /// The form for editing item `id`. `None` if the item is missing or deleted.
    pub fn edit(db: &Db, id: BudgetItemId, today: Date) -> Option<Self> {
        let item = db.budget_item(id).filter(|b| !b.is_deleted)?;
        let input = BudgetItemInput {
            item_type: item.item_type,
            name: item.name.clone(),
            amount: item.amount,
            start_date: item.start_date,
            end_date: item.end_date,
            frequency: item.frequency,
            specific_months: item.specific_months.clone(),
            notes: item.notes.clone().unwrap_or_default(),
            category: item.category_id,
            account: item.account_id,
        };
        let default_end = today.checked_add(Span::new().years(1)).unwrap_or(today);
        Some(Self {
            id: Some(id),
            has_end_date: item.end_date.is_some(),
            end_date: item.end_date.unwrap_or(default_end),
            amount_text: amount_edit_text(item.amount),
            input,
            error: None,
        })
    }

    /// The label of the name field: Swift `nameLabel` ("Payee", "Name (autofilled from Category)",
    /// "Source").
    pub fn name_label(&self) -> &'static str {
        match self.input.item_type {
            BudgetItemType::Bill => "Payee",
            BudgetItemType::Expense => "Name (autofilled from Category)",
            BudgetItemType::Income => "Source",
        }
    }

    /// The engine input, with the amount parsed from the text and the end date set only when ticked
    /// (R-BUD-2). `Err` with the message to show when the amount is not a number.
    pub fn to_input(&self) -> Result<BudgetItemInput, String> {
        let amount = parse_amount_input(&self.amount_text)
            .ok_or_else(|| "Enter the amount as a number".to_string())?;
        let mut input = self.input.clone();
        input.amount = amount;
        input.end_date = self.has_end_date.then_some(self.end_date);
        input.specific_months.sort_unstable();
        input.specific_months.dedup();
        Ok(input)
    }
}

/// The word used in the title: "Bill", "Expense" or "Income".
fn type_word(t: BudgetItemType) -> &'static str {
    t.label()
}

/// The button pressed this frame, applied after the modal is drawn.
enum Action {
    None,
    Cancel,
    Save,
    Reproject(BudgetItemId),
}

/// Draws the editor modal and runs the chosen action.
pub fn show(cx: &mut UiCtx, ui: &mut Ui, form: &mut BudgetItemForm) -> Outcome {
    let lists = cx.caches.lookups(cx.engine).clone();
    let reproject = reproject_target(cx.engine.db(), form.id);
    let mut action = Action::None;
    let title = format!(
        "{} {}",
        if form.id.is_some() { "Edit" } else { "Add" },
        type_word(form.input.item_type)
    );
    let response = Modal::new(Id::unique(("budget_item_editor",))).show(ui.ctx(), |ui| {
        ui.set_min_width(MIN_WIDTH);
        ui.heading(title);
        type_row(ui, form);
        ui.add_space(4.0);
        let name_label = form.name_label();
        egui::Grid::new("budget_item_fields")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label("Category");
                if let Some(c) = category_picker(
                    ui,
                    "budget_item_category",
                    form.input.category,
                    &lists.categories,
                    true,
                    "Category",
                ) {
                    form.input.category = c;
                    // The name is filled from the category only while it is empty (Swift).
                    if form.input.name.trim().is_empty()
                        && let Some(c) = c
                        && let Some(cat) = cx.engine.db().category(c)
                    {
                        form.input.name = cat.name.clone();
                    }
                }
                ui.end_row();
                labeled_text(ui, name_label, &mut form.input.name);
                ui.end_row();
                labeled_text(ui, "Amount", &mut form.amount_text);
                ui.end_row();
                ui.label("Account");
                account_combo(ui, &mut form.input.account, &lists.accounts);
                ui.end_row();
                ui.label("Start Date");
                ui.add(
                    DatePickerButton::new(&mut form.input.start_date)
                        .id_salt("budget_start")
                        .show_icon(false),
                );
                ui.end_row();
                ui.checkbox(&mut form.has_end_date, "End Date");
                if form.has_end_date {
                    ui.add(
                        DatePickerButton::new(&mut form.end_date)
                            .id_salt("budget_end")
                            .show_icon(false),
                    );
                } else {
                    ui.label("");
                }
                ui.end_row();
            });
        ui.add_space(6.0);
        ui.label(RichText::new("Recurrence").strong());
        frequency_row(ui, form);
        ui.add_space(6.0);
        ui.label("Notes (Optional)");
        let notes = ui.add(
            TextEdit::multiline(&mut form.input.notes)
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        );
        notes.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, "Notes"));
        if let Some(rp) = &reproject {
            ui.add_space(6.0);
            ui.label(RichText::new("Reproject to Scenario").strong());
            ui.horizontal(|ui| {
                ui.label(format!(
                    "Scenario: {}  {} to {}",
                    rp.scenario_name,
                    format_date(rp.start),
                    format_date(rp.end)
                ));
                if ui.button("Reproject").clicked()
                    && let Some(id) = form.id
                {
                    action = Action::Reproject(id);
                }
            });
        }
        if let Some(err) = &form.error {
            ui.label(RichText::new(err).color(colors::RED));
        }
        ui.add_space(8.0);
        let save_blocked = form.input.name.trim().is_empty();
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() {
                action = Action::Cancel;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(!save_blocked, egui::Button::new("Save"))
                    .clicked()
                {
                    action = Action::Save;
                }
            });
        });
    });

    let mut close = response.should_close();
    match action {
        Action::None => {}
        Action::Cancel => close = true,
        Action::Save => match form.to_input() {
            Err(e) => form.error = Some(e),
            Ok(input) => match save_budget_item(cx.engine, form.id, input) {
                Ok(_) => close = true,
                Err(e) => form.error = Some(e.to_string()),
            },
        },
        Action::Reproject(id) => match form.to_input() {
            Err(e) => form.error = Some(e),
            Ok(input) => match reproject_item(cx.engine, id, input) {
                Ok(text) => {
                    form.error = None;
                    cx.toasts.success(text);
                }
                Err(e) => form.error = Some(e.to_string()),
            },
        },
    }
    if close {
        Outcome::Close
    } else {
        Outcome::KeepOpen
    }
}

/// The projection a Reproject button would use (R-PRJ-9): the scenario name, and the window of the
/// last projection. `None` until a projection has been run, or for an item that is not saved yet.
struct ReprojectTarget {
    scenario_name: String,
    start: Date,
    end: Date,
}

/// The reproject target for `id`, if the last projection's settings are all present.
fn reproject_target(db: &Db, id: Option<BudgetItemId>) -> Option<ReprojectTarget> {
    id?;
    let (scenario, start, end) = last_projection(db);
    let scenario = scenario.and_then(|s| db.scenario(s))?;
    Some(ReprojectTarget {
        scenario_name: scenario.name.clone(),
        start: start?,
        end: end?,
    })
}

/// "Mar 1, 2026" (the Swift date format).
fn format_date(d: Date) -> String {
    d.strftime("%b %-d, %Y").to_string()
}

/// The type picker: Bill, Expense, Income.
fn type_row(ui: &mut Ui, form: &mut BudgetItemForm) {
    ui.horizontal(|ui| {
        for t in BudgetItemType::ALL {
            ui.selectable_value(&mut form.input.item_type, t, t.label());
        }
    });
}

/// The account picker ("Select Account" when none).
fn account_combo(ui: &mut Ui, value: &mut Option<AccountId>, accounts: &[(AccountId, String)]) {
    let shown = value
        .and_then(|id| accounts.iter().find(|(a, _)| *a == id))
        .map_or("Select Account", |(_, name)| name.as_str())
        .to_string();
    let resp = ComboBox::from_id_salt("budget_item_account")
        .width(220.0)
        .selected_text(shown)
        .show_ui(ui, |ui| {
            for (id, name) in accounts {
                ui.selectable_value(value, Some(*id), name);
            }
        });
    resp.response
        .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, "Account"));
}

/// The frequency picker, and, for Specific Months, twelve month checkboxes in a grid.
fn frequency_row(ui: &mut Ui, form: &mut BudgetItemForm) {
    egui::Grid::new("budget_item_frequency")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            ui.label("Frequency");
            let resp = ComboBox::from_id_salt("budget_item_frequency")
                .selected_text(form.input.frequency.label())
                .show_ui(ui, |ui| {
                    for f in BudgetFrequency::ALL {
                        ui.selectable_value(&mut form.input.frequency, f, f.label());
                    }
                });
            resp.response
                .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, "Frequency"));
            ui.end_row();
        });
    if form.input.frequency == BudgetFrequency::SpecificMonths {
        egui::Grid::new("budget_item_months")
            .num_columns(4)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                for (i, name) in MONTHS.iter().enumerate() {
                    let month = u32::try_from(i + 1).unwrap_or(1);
                    let mut on = form.input.specific_months.contains(&month);
                    if ui.checkbox(&mut on, *name).changed() {
                        if on {
                            form.input.specific_months.push(month);
                        } else {
                            form.input.specific_months.retain(|m| *m != month);
                        }
                    }
                    if i % 4 == 3 {
                        ui.end_row();
                    }
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::testing::{TestDb, d};
    use pf_model::{BudgetFrequency, BudgetItemType};
    use rust_decimal_macros::dec;

    #[test]
    /// Covers: T17 name label. The label follows the item type (Swift `nameLabel`).
    fn budget_form_name_label_per_type() {
        let today = d("2026-03-15");
        let mut f = BudgetItemForm::new_for(BudgetItemType::Bill, today);
        assert_eq!(f.name_label(), "Payee");
        f.input.item_type = BudgetItemType::Expense;
        assert_eq!(f.name_label(), "Name (autofilled from Category)");
        f.input.item_type = BudgetItemType::Income;
        assert_eq!(f.name_label(), "Source");
    }

    #[test]
    /// Covers: T17 amount. Text that is not a number is an error; a comma-grouped number parses.
    fn budget_form_to_input_rejects_bad_amount() {
        let today = d("2026-03-15");
        let mut f = BudgetItemForm::new_for(BudgetItemType::Bill, today);
        f.amount_text = "abc".into();
        assert!(f.to_input().is_err());
        f.amount_text = "1,500".into();
        assert_eq!(f.to_input().expect("amount").amount, dec!(1500));
    }

    #[test]
    /// Covers: R-BUD-2. The end date is sent only when the box is ticked.
    fn budget_form_end_date_only_when_ticked() {
        let today = d("2026-03-15");
        let mut f = BudgetItemForm::new_for(BudgetItemType::Bill, today);
        f.amount_text = "10".into();
        f.has_end_date = false;
        assert_eq!(f.to_input().expect("input").end_date, None);
        f.has_end_date = true;
        f.end_date = d("2027-03-15");
        assert_eq!(f.to_input().expect("input").end_date, Some(d("2027-03-15")));
    }

    #[test]
    /// Covers: T17 edit. Editing an item reads its values, and a missing item gives no form.
    fn budget_form_edit_reads_the_item() {
        let mut t = TestDb::new();
        let id = t
            .budget_item(
                BudgetItemType::Bill,
                "Rent",
                "1500",
                "2026-01-01",
                BudgetFrequency::Monthly,
            )
            .done();
        let db = t.build();
        let f = BudgetItemForm::edit(&db, id, d("2026-03-15")).expect("form");
        assert_eq!(f.input.name, "Rent");
        assert_eq!(f.amount_text, "1500");
        assert!(BudgetItemForm::edit(&db, BudgetItemId(999), d("2026-03-15")).is_none());
    }
}
