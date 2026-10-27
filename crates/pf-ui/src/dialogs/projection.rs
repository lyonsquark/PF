//! The Generate Projection dialog (T17, Swift `BudgetProjectionView`): pick the budget items, a
//! window and a scenario (an existing one, or a new one by name), and generate the Future rows
//! (R-PRJ-6..8). Each item shows the dates it would produce in the window.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use egui::{ComboBox, Id, Modal, RichText, Role, Ui, WidgetInfo};
use egui_extras::DatePickerButton;
use jiff::Span;
use jiff::civil::Date;
use pf_core::format;
use pf_core::projector::generate_dates;
use pf_engine::commands::budget::{
    ProjectionRequest, ProjectionTarget, generate_projection, last_projection,
};
use pf_model::{BudgetItem, BudgetItemId, BudgetItemType, Db, ScenarioId};

use super::Outcome;
use crate::app::UiCtx;
use crate::widgets::colors;
use crate::widgets::labeled_text;

/// Dialog width in points.
const MIN_WIDTH: f32 = 520.0;
/// Seconds the success line stays before the dialog closes itself.
const AUTO_CLOSE_SECS: f64 = 1.5;
/// Height of the item list in points.
const LIST_HEIGHT: f32 = 300.0;

/// Date previews per item, for one window and one engine revision.
type Previews = BTreeMap<BudgetItemId, String>;

/// The form state of the projection dialog. It lives in `Dialog::Projection` while open.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectionForm {
    /// Generate into a new scenario named by `new_name`.
    pub new_scenario: bool,
    /// The name of the new scenario (trimmed on generate).
    pub new_name: String,
    /// The existing scenario to generate into.
    pub target: Option<ScenarioId>,
    /// First day of the window (inclusive).
    pub start: Date,
    /// Last day of the window (inclusive).
    pub end: Date,
    /// The budget items to project.
    pub selected: BTreeSet<BudgetItemId>,
    /// After a successful generate: the number of rows and the time it happened (for auto-close).
    pub generated: Option<(usize, f64)>,
    /// An error from the last generate, shown in red.
    pub error: Option<String>,
    /// Cached date previews, with the revision, window and items they were made for.
    previews: Option<(u64, Date, Date, Previews)>,
}

impl ProjectionForm {
    /// The form for `db` today: all live items selected, the last projection's scenario and window
    /// when saved (R-PRJ-8), otherwise today to three months on.
    pub fn new(db: &Db, today: Date) -> Self {
        let (scenario, start, end) = last_projection(db);
        let default_end = today.checked_add(Span::new().months(3)).unwrap_or(today);
        Self {
            new_scenario: scenario.is_none(),
            new_name: String::new(),
            target: scenario,
            start: start.unwrap_or(today),
            end: end.unwrap_or(default_end),
            selected: db.live_budget_items().map(|b| b.id).collect(),
            generated: None,
            error: None,
            previews: None,
        }
    }

    /// R-PRJ-7: Generate is disabled when no item is selected, when a new scenario has no name, or
    /// when an existing scenario is wanted but none is chosen.
    pub fn generate_disabled(&self) -> bool {
        self.selected.is_empty()
            || (self.new_scenario && self.new_name.trim().is_empty())
            || (!self.new_scenario && self.target.is_none())
    }
}

/// The button pressed this frame, applied after the modal is drawn.
enum Action {
    None,
    Cancel,
    Generate,
}

/// Draws the projection modal and runs the chosen action.
pub fn show(cx: &mut UiCtx, ui: &mut Ui, form: &mut ProjectionForm) -> Outcome {
    let lists = cx.caches.lookups(cx.engine).clone();
    let revision = cx.engine.revision();
    let now = ui.input(|i| i.time);
    let mut action = Action::None;

    let response = Modal::new(Id::unique(("generate_projection",))).show(ui.ctx(), |ui| {
        ui.set_min_width(MIN_WIDTH);
        ui.heading("Generate Projection");
        scenario_row(ui, form, &lists.scenarios);
        egui::Grid::new("projection_window")
            .num_columns(4)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label("From");
                ui.add(
                    DatePickerButton::new(&mut form.start)
                        .id_salt("projection_start")
                        .show_icon(false),
                );
                ui.label("Until");
                ui.add(
                    DatePickerButton::new(&mut form.end)
                        .id_salt("projection_end")
                        .show_icon(false),
                );
                ui.end_row();
            });
        ui.separator();
        ensure_previews(cx.engine.db(), form, revision);
        item_sections(ui, form, cx.engine.db());
        ui.separator();
        if let Some((count, _)) = form.generated {
            ui.label(RichText::new(generated_text(count)).color(colors::GREEN));
        }
        if let Some(err) = &form.error {
            ui.label(RichText::new(err).color(colors::RED));
        }
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() {
                action = Action::Cancel;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(!form.generate_disabled(), egui::Button::new("Generate"))
                    .clicked()
                {
                    action = Action::Generate;
                }
            });
        });
    });

    let mut close = response.should_close();
    match action {
        Action::None => {}
        Action::Cancel => close = true,
        Action::Generate => generate(cx, form, now),
    }
    // A finished generate shows its count for a moment, then closes.
    match form.generated {
        Some((_, at)) if now - at >= AUTO_CLOSE_SECS => close = true,
        Some(_) => ui.ctx().request_repaint_after(Duration::from_millis(200)),
        None => {}
    }
    if close {
        Outcome::Close
    } else {
        Outcome::KeepOpen
    }
}

/// Runs Generate: the engine adds the rows as one step (R-PRJ-6..8). The count is shown and toasted.
fn generate(cx: &mut UiCtx, form: &mut ProjectionForm, now: f64) {
    let target = if form.new_scenario {
        ProjectionTarget::New(form.new_name.trim().to_string())
    } else {
        match form.target {
            Some(id) => ProjectionTarget::Existing(id),
            None => return,
        }
    };
    let request = ProjectionRequest {
        target,
        start: form.start,
        end: form.end,
        items: form.selected.iter().copied().collect(),
    };
    match generate_projection(cx.engine, request) {
        Ok(result) => {
            form.error = None;
            form.generated = Some((result.count, now));
            cx.toasts.success(generated_text(result.count));
        }
        Err(e) => form.error = Some(e.to_string()),
    }
}

/// "Generated 4 transactions" (singular for one).
pub fn generated_text(count: usize) -> String {
    let word = if count == 1 {
        "transaction"
    } else {
        "transactions"
    };
    format!("Generated {count} {word}")
}

/// The scenario choice: a new scenario named in the field, or an existing one from the list.
fn scenario_row(ui: &mut Ui, form: &mut ProjectionForm, scenarios: &[(ScenarioId, String)]) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut form.new_scenario, "New Scenario");
        if form.new_scenario {
            labeled_text(ui, "Scenario Name", &mut form.new_name);
        } else {
            let shown = form
                .target
                .and_then(|id| scenarios.iter().find(|(s, _)| *s == id))
                .map_or("Select Scenario", |(_, name)| name.as_str())
                .to_string();
            let resp = ComboBox::from_id_salt("projection_scenario")
                .width(220.0)
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    for (id, name) in scenarios {
                        ui.selectable_value(&mut form.target, Some(*id), name);
                    }
                });
            resp.response
                .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, "Scenario"));
        }
    });
}

/// The three sections (Bills, Expenses, Income) of live items, each with All/None, in one scroll
/// area. Each row shows the amount and the dates the item would produce in the window.
fn item_sections(ui: &mut Ui, form: &mut ProjectionForm, db: &Db) {
    egui::ScrollArea::vertical()
        .max_height(LIST_HEIGHT)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for (tab, title) in [
                (BudgetItemType::Bill, "Bills"),
                (BudgetItemType::Expense, "Expenses"),
                (BudgetItemType::Income, "Income"),
            ] {
                let items: Vec<&BudgetItem> = db
                    .budget_items_by_name()
                    .into_iter()
                    .filter(|b| b.item_type == tab)
                    .collect();
                if items.is_empty() {
                    continue;
                }
                section(ui, form, title, &items);
            }
        });
}

/// One section: its heading, the All/None toggle, and a row per item.
fn section(ui: &mut Ui, form: &mut ProjectionForm, title: &str, items: &[&BudgetItem]) {
    let all = items.iter().all(|b| form.selected.contains(&b.id));
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let toggle = if all { "None" } else { "All" };
            // A stable id, so the button keeps its identity when its label changes to "All"/"None".
            let clicked = ui
                .push_id(("toggle_all", title), |ui| {
                    ui.small_button(toggle).clicked()
                })
                .inner;
            if clicked {
                for b in items {
                    if all {
                        form.selected.remove(&b.id);
                    } else {
                        form.selected.insert(b.id);
                    }
                }
            }
        });
    });
    for b in items {
        ui.horizontal(|ui| {
            let mut on = form.selected.contains(&b.id);
            if ui.checkbox(&mut on, b.name.as_str()).changed() {
                if on {
                    form.selected.insert(b.id);
                } else {
                    form.selected.remove(&b.id);
                }
            }
            let amount = format::format_currency(b.amount);
            if b.item_type == BudgetItemType::Income {
                ui.label(RichText::new(amount).color(colors::GREEN));
            } else {
                ui.label(RichText::new(amount).weak());
            }
            let preview = form
                .previews
                .as_ref()
                .and_then(|(_, _, _, map)| map.get(&b.id))
                .cloned()
                .unwrap_or_default();
            ui.label(RichText::new(preview).weak().small());
        });
    }
}

/// Refreshes the date previews when the data, the window or the revision changed (not every frame).
fn ensure_previews(db: &Db, form: &mut ProjectionForm, revision: u64) {
    let fresh = form
        .previews
        .as_ref()
        .is_some_and(|(r, s, e, _)| *r == revision && *s == form.start && *e == form.end);
    if fresh {
        return;
    }
    let map: Previews = db
        .live_budget_items()
        .map(|b| (b.id, preview(b, form.start, form.end)))
        .collect();
    form.previews = Some((revision, form.start, form.end, map));
}

/// The dates an item produces in the window, as "3/1, 4/1" ("no dates" when there are none).
pub fn preview(item: &BudgetItem, start: Date, end: Date) -> String {
    let dates = generate_dates(item, start, end);
    if dates.is_empty() {
        return "no dates".to_string();
    }
    dates
        .iter()
        .map(|d| d.strftime("%-m/%-d").to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::BudgetFrequency;
    use pf_model::testing::{TestDb, d};

    #[test]
    /// Covers: R-PRJ-7. Generate is disabled with no items, a blank new name, or no existing target.
    fn r_prj_7_generate_disabled_rules() {
        let mut t = TestDb::new();
        let scenario = t.scenario("Plan A");
        let item = t
            .budget_item(
                BudgetItemType::Bill,
                "Rent",
                "1500",
                "2026-01-01",
                BudgetFrequency::Monthly,
            )
            .done();
        let db = t.build();
        let today = d("2026-03-15");

        let mut f = ProjectionForm::new(&db, today);
        f.new_scenario = true;
        f.new_name = "Plan B".into();
        assert!(!f.generate_disabled(), "valid new scenario");

        f.new_name = "  ".into();
        assert!(f.generate_disabled(), "blank new name");

        f.new_scenario = false;
        f.target = None;
        assert!(f.generate_disabled(), "no existing target");

        f.target = Some(scenario);
        assert!(!f.generate_disabled(), "existing target");

        f.selected.clear();
        assert!(f.generate_disabled(), "nothing selected");
        f.selected.insert(item);
        assert!(!f.generate_disabled());
    }

    #[test]
    /// Covers: R-PRJ-8. The form starts from the saved scenario and window.
    fn r_prj_8_projection_form_prefills_last() {
        let mut t = TestDb::new();
        let plan = t.scenario("Plan A");
        t.setting("lastProjectionScenario", "Plan A");
        t.setting("lastProjectionStartDate", "2026-04-01");
        t.setting("lastProjectionEndDate", "2026-06-30");
        let db = t.build();
        let f = ProjectionForm::new(&db, d("2026-03-15"));
        assert_eq!(f.target, Some(plan));
        assert!(!f.new_scenario);
        assert_eq!(f.start, d("2026-04-01"));
        assert_eq!(f.end, d("2026-06-30"));
    }

    #[test]
    /// The footer text: plural for many, singular for one.
    fn generated_text_is_singular_for_one() {
        assert_eq!(generated_text(4), "Generated 4 transactions");
        assert_eq!(generated_text(1), "Generated 1 transaction");
    }
}
