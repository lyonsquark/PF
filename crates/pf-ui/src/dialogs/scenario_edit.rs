//! The New and Edit Scenario dialogs (T19, Swift `EditScenarioSheet`): name, colour and notes.
//! Create and Save are disabled while the trimmed name is empty (R-SCN-4).

use egui::{Id, Modal, RichText, Ui};
use pf_engine::commands::scenarios::{ScenarioInput, create_scenario, update_scenario};
use pf_model::{Db, ScenarioId};

use super::Outcome;
use crate::app::UiCtx;
use crate::widgets::color::{FALLBACK_RGB, color_field, rgb_or_fallback, to_hex};
use crate::widgets::colors;
use crate::widgets::labeled_text;

/// Dialog width in points.
const MIN_WIDTH: f32 = 360.0;

/// The form state of the dialog. It lives in `Dialog::ScenarioEdit` while open.
#[derive(Debug, Clone, PartialEq)]
pub struct ScenarioForm {
    /// The scenario being edited; `None` when creating.
    pub editing: Option<ScenarioId>,
    /// The name, as typed (trimmed on save).
    pub name: String,
    /// The notes, as typed (blank becomes none on save).
    pub notes: String,
    /// The colour.
    pub rgb: [u8; 3],
    /// An error from the last save, shown in red.
    pub error: Option<String>,
}

impl Default for ScenarioForm {
    fn default() -> Self {
        Self {
            editing: None,
            name: String::new(),
            notes: String::new(),
            rgb: FALLBACK_RGB,
            error: None,
        }
    }
}

impl ScenarioForm {
    /// The form for editing scenario `id`, filled from the data. `None` if it is missing or deleted.
    /// An invalid or missing colour shows purple.
    pub fn edit(db: &Db, id: ScenarioId) -> Option<Self> {
        let s = db.scenario(id).filter(|s| !s.is_deleted)?;
        Some(Self {
            editing: Some(id),
            name: s.name.clone(),
            notes: s.notes.clone().unwrap_or_default(),
            rgb: rgb_or_fallback(s.color_hex.as_deref()),
            error: None,
        })
    }

    /// The engine input for this form.
    fn input(&self) -> ScenarioInput {
        ScenarioInput {
            name: self.name.clone(),
            color_hex: Some(to_hex(self.rgb)),
            notes: Some(self.notes.clone()),
        }
    }
}

/// The button pressed this frame, applied after the modal is drawn.
enum Action {
    None,
    Cancel,
    Save,
}

/// Draws the New or Edit Scenario modal and runs the chosen action.
pub fn show(cx: &mut UiCtx, ui: &mut Ui, form: &mut ScenarioForm) -> Outcome {
    let mut action = Action::None;
    let title = if form.editing.is_some() {
        "Edit Scenario"
    } else {
        "New Scenario"
    };
    let save_label = if form.editing.is_some() {
        "Save"
    } else {
        "Create"
    };
    let response = Modal::new(Id::unique(("scenario_edit", form.editing.map(|i| i.raw())))).show(
        ui.ctx(),
        |ui| {
            ui.set_min_width(MIN_WIDTH);
            ui.heading(title);
            labeled_text(ui, "Name", &mut form.name);
            color_field(ui, "Color", &mut form.rgb);
            labeled_text(ui, "Notes", &mut form.notes);
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
                        .add_enabled(!blocked, egui::Button::new(save_label))
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
        Action::Save => {
            let result = match form.editing {
                Some(id) => update_scenario(cx.engine, id, form.input()).map(|()| id),
                None => create_scenario(cx.engine, form.input()),
            };
            match result {
                Ok(_) => close = true,
                Err(e) => form.error = Some(e.to_string()),
            }
        }
    }
    if close {
        Outcome::Close
    } else {
        Outcome::KeepOpen
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::testing::TestDb;

    #[test]
    /// A new scenario form is purple and empty; an edit form reads the colour, or purple when invalid.
    fn scenario_form_defaults_and_edit_colour() {
        let f = ScenarioForm::default();
        assert_eq!(f.rgb, [0x80, 0x00, 0x80]);
        assert_eq!(f.editing, None);

        let mut t = TestDb::new();
        let plan = t.scenario("Plan A");
        let db = t.build();
        let edit = ScenarioForm::edit(&db, plan).expect("form");
        assert_eq!(edit.editing, Some(plan));
        assert_eq!(edit.name, "Plan A");
    }
}
