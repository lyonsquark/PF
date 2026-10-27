//! The New and Edit Category dialogs (T20, Swift `CategoryEditorView`): name, icon, notes, the payee
//! auto-match rules, type, and the parent. Save validates the rules through the engine, which reports
//! the ones it removed. "Apply" runs the typed rules over uncategorized transactions (R-CAT-3).

use egui::{ComboBox, Id, Modal, RichText, Role, Ui, WidgetInfo};
use pf_engine::commands::categories::{CategoryInput, apply_rules_to_uncategorized, save_category};
use pf_model::{CategoryId, CategoryType, Db};

use super::Outcome;
use crate::app::UiCtx;
use crate::widgets::colors;
use crate::widgets::labeled_text;

/// Dialog width in points.
const MIN_WIDTH: f32 = 460.0;

/// The form state of the dialog. It lives in `Dialog::CategoryEdit` while open.
#[derive(Debug, Clone, PartialEq)]
pub struct CategoryForm {
    /// The category being edited; `None` when creating.
    pub editing: Option<CategoryId>,
    /// The name, as typed (trimmed on save).
    pub name: String,
    /// The icon: an emoji, or blank.
    pub icon: String,
    /// Notes, as typed.
    pub notes: String,
    /// The auto-match rules, comma separated.
    pub patterns: String,
    /// Income, expense or transfer.
    pub category_type: CategoryType,
    /// The parent category, if this is a subcategory.
    pub parent: Option<CategoryId>,
    /// An error from the last save, shown in red.
    pub error: Option<String>,
    /// The result of the last "Apply" (shown inline, so the form stays open with its text).
    pub notice: Option<String>,
}

impl Default for CategoryForm {
    fn default() -> Self {
        Self {
            editing: None,
            name: String::new(),
            icon: String::new(),
            notes: String::new(),
            patterns: String::new(),
            category_type: CategoryType::Expense,
            parent: None,
            error: None,
            notice: None,
        }
    }
}

impl CategoryForm {
    /// The form for editing category `id`, with its rules joined by ", ". `None` if it is missing.
    pub fn edit(db: &Db, id: CategoryId) -> Option<Self> {
        let c = db.category(id).filter(|c| !c.is_deleted)?;
        Some(Self {
            editing: Some(id),
            name: c.name.clone(),
            icon: c.icon.clone().unwrap_or_default(),
            notes: c.notes.clone().unwrap_or_default(),
            patterns: c.match_patterns.join(", "),
            category_type: c.category_type,
            parent: c.parent_id,
            ..Self::default()
        })
    }

    /// The engine input for this form.
    fn input(&self) -> CategoryInput {
        CategoryInput {
            name: self.name.clone(),
            category_type: self.category_type,
            icon: self.icon.clone(),
            notes: self.notes.clone(),
            patterns_csv: self.patterns.clone(),
            parent: self.parent,
        }
    }
}

/// The button pressed this frame, applied after the modal is drawn.
enum Action {
    None,
    Cancel,
    Save,
    Apply(CategoryId),
}

/// Draws the category modal and runs the chosen action.
pub fn show(cx: &mut UiCtx, ui: &mut Ui, form: &mut CategoryForm) -> Outcome {
    let db = cx.engine.db();
    let has_children = form
        .editing
        .is_some_and(|id| !db.subcategories(id).is_empty());
    let parents = parent_choices(db, form.category_type, form.editing);
    let mut action = Action::None;
    let title = if form.editing.is_some() {
        "Edit Category"
    } else {
        "New Category"
    };

    let response = Modal::new(Id::unique(("category_edit", form.editing.map(|i| i.raw())))).show(
        ui.ctx(),
        |ui| {
            ui.set_min_width(MIN_WIDTH);
            ui.heading(title);
            egui::Grid::new("category_fields")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    labeled_text(ui, "Name", &mut form.name);
                    ui.end_row();
                    labeled_text(ui, "Icon (Emoji)", &mut form.icon);
                    ui.end_row();
                    ui.label("Notes");
                    ui.add(
                        egui::TextEdit::multiline(&mut form.notes)
                            .desired_rows(3)
                            .desired_width(280.0),
                    )
                    .widget_info(|| WidgetInfo::labeled(Role::TextInput, true, "Notes"));
                    ui.end_row();
                });
            ui.add_space(4.0);
            ui.label(
                RichText::new("Payee Auto-Match Rules (comma separated)")
                    .small()
                    .weak(),
            );
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut form.patterns)
                        .desired_rows(2)
                        .desired_width(340.0),
                )
                .widget_info(|| {
                    WidgetInfo::labeled(Role::TextInput, true, "Payee Auto-Match Rules")
                });
                let apply_hover = if form.editing.is_some() {
                    "Apply rules to uncategorized transactions"
                } else {
                    "Save category first to apply rules"
                };
                let can_apply = form.editing.is_some() && !form.patterns.trim().is_empty();
                let apply = ui.add_enabled(can_apply, egui::Button::new("Apply"));
                if apply.on_hover_text(apply_hover).clicked()
                    && let Some(id) = form.editing
                {
                    action = Action::Apply(id);
                }
            });
            if let Some(notice) = &form.notice {
                ui.label(RichText::new(notice).color(colors::GREEN));
            }
            ui.add_space(4.0);
            ui.label(RichText::new("Type").small().weak());
            ui.add_enabled_ui(!has_children, |ui| {
                ui.horizontal(|ui| {
                    let before = form.category_type;
                    for t in [
                        CategoryType::Income,
                        CategoryType::Expense,
                        CategoryType::Transfer,
                    ] {
                        ui.selectable_value(&mut form.category_type, t, t.label());
                    }
                    if form.category_type != before {
                        form.parent = None;
                    }
                });
            });
            ui.label(RichText::new("Hierarchy").small().weak());
            let shown = form
                .parent
                .and_then(|p| parents.iter().find(|(id, _)| *id == p))
                .map_or("None", |(_, name)| name.as_str())
                .to_string();
            let resp = ComboBox::from_id_salt("category_parent")
                .width(220.0)
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut form.parent, None, "None");
                    for (id, name) in &parents {
                        ui.selectable_value(&mut form.parent, Some(*id), name);
                    }
                });
            resp.response
                .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, "Parent Category"));
            if let Some(err) = &form.error {
                ui.label(RichText::new(err).color(colors::RED));
            }
            ui.add_space(8.0);
            let blocked = form.name.trim().is_empty();
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    action = Action::Cancel;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(!blocked, egui::Button::new("Save"))
                        .clicked()
                    {
                        action = Action::Save;
                    }
                });
            });
        },
    );

    let mut close = response.should_close();
    match action {
        Action::None => {}
        Action::Cancel => close = true,
        Action::Apply(id) => match apply_rules_to_uncategorized(cx.engine, id, &form.patterns) {
            Ok(n) => {
                form.error = None;
                form.notice = Some(format!("Applied to {n} uncategorized transaction(s)."));
            }
            Err(e) => form.error = Some(e.to_string()),
        },
        Action::Save => match save_category(cx.engine, form.editing, form.input()) {
            Ok(result) => {
                close = true;
                if !result.rejected_patterns.is_empty() {
                    let list: String = result
                        .rejected_patterns
                        .iter()
                        .map(|p| format!("\n• {p}"))
                        .collect();
                    cx.dialogs.alert(
                        "Invalid Regex Pattern",
                        format!(
                            "The following patterns are not valid regular expressions and were removed:{list}"
                        ),
                    );
                }
            }
            Err(e) => form.error = Some(e.to_string()),
        },
    }
    if close {
        Outcome::Close
    } else {
        Outcome::KeepOpen
    }
}

/// The parent choices (R-CAT-2): live top-level categories of the same type, excluding the one being
/// edited, by name.
fn parent_choices(
    db: &Db,
    t: CategoryType,
    editing: Option<CategoryId>,
) -> Vec<(CategoryId, String)> {
    db.categories_by_name()
        .into_iter()
        .filter(|c| c.category_type == t && c.parent_id.is_none() && Some(c.id) != editing)
        .map(|c| (c.id, c.name.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::testing::TestDb;

    #[test]
    /// The editor joins the rules with ", " and carries the parent and type.
    fn edit_form_reads_the_category() {
        let mut t = TestDb::new();
        let food = t.category("Food", CategoryType::Expense);
        t.patterns(food, &["KROGER", "ALDI"]);
        let db = t.build();
        let f = CategoryForm::edit(&db, food).expect("form");
        assert_eq!(f.patterns, "KROGER, ALDI");
        assert_eq!(f.category_type, CategoryType::Expense);
        assert_eq!(f.parent, None);
    }

    #[test]
    /// The parent list holds only top-level categories of the same type, and not the one being edited.
    fn parent_choices_same_type_top_level_only() {
        let mut t = TestDb::new();
        let food = t.category("Food", CategoryType::Expense);
        let _child = t.child_category(food, "Cafe");
        let salary = t.category("Salary", CategoryType::Income);
        let db = t.build();
        let names: Vec<String> = parent_choices(&db, CategoryType::Expense, None)
            .into_iter()
            .map(|(_, n)| n)
            .collect();
        assert_eq!(names, vec!["Food"]);
        assert!(parent_choices(&db, CategoryType::Income, Some(salary)).is_empty());
    }
}
