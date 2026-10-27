//! The New and Edit Context dialogs (T20, Swift `ContextEditorView`): name, colour and notes.
//! Create and Save are disabled while the trimmed name is empty.

use egui::{Id, Modal, RichText, Ui};
use pf_engine::commands::contexts::{ContextInput, create_context, update_context};
use pf_model::{ContextId, Db};

use super::Outcome;
use crate::app::UiCtx;
use crate::widgets::color::{color_field, to_hex};
use crate::widgets::colors;
use crate::widgets::labeled_text;

/// Dialog width in points.
const MIN_WIDTH: f32 = 360.0;
/// The default context colour (R-CTX-4, blue).
pub const DEFAULT_RGB: [u8; 3] = [0, 0, 255];

/// The form state of the dialog. It lives in `Dialog::ContextEdit` while open.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextForm {
    /// The context being edited; `None` when creating.
    pub editing: Option<ContextId>,
    /// The name, as typed (trimmed on save).
    pub name: String,
    /// Notes, as typed.
    pub notes: String,
    /// The colour.
    pub rgb: [u8; 3],
    /// An error from the last save, shown in red.
    pub error: Option<String>,
}

impl Default for ContextForm {
    fn default() -> Self {
        Self {
            editing: None,
            name: String::new(),
            notes: String::new(),
            rgb: DEFAULT_RGB,
            error: None,
        }
    }
}

impl ContextForm {
    /// The form for editing context `id`. A missing or invalid colour shows blue. `None` if missing.
    pub fn edit(db: &Db, id: ContextId) -> Option<Self> {
        let c = db.context(id).filter(|c| !c.is_deleted)?;
        let rgb = c
            .color_hex
            .as_deref()
            .and_then(pf_core::color::parse_hex)
            .map_or(DEFAULT_RGB, |[r, g, b, _]| [r, g, b]);
        Some(Self {
            editing: Some(id),
            name: c.name.clone(),
            notes: c.notes.clone().unwrap_or_default(),
            rgb,
            error: None,
        })
    }

    /// The engine input for this form.
    fn input(&self) -> ContextInput {
        ContextInput {
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

/// Draws the context modal and runs the chosen action.
pub fn show(cx: &mut UiCtx, ui: &mut Ui, form: &mut ContextForm) -> Outcome {
    let mut action = Action::None;
    let title = if form.editing.is_some() {
        "Edit Context"
    } else {
        "New Context"
    };
    let response = Modal::new(Id::unique(("context_edit", form.editing.map(|i| i.raw())))).show(
        ui.ctx(),
        |ui| {
            ui.set_min_width(MIN_WIDTH);
            ui.heading(title);
            labeled_text(ui, "Context Name", &mut form.name);
            color_field(ui, "Context Color", &mut form.rgb);
            ui.label("Notes");
            let notes = ui.add(
                egui::TextEdit::multiline(&mut form.notes)
                    .desired_rows(3)
                    .desired_width(280.0),
            );
            notes.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, "Notes"));
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
                        .add_enabled(
                            !blocked,
                            egui::Button::new(if form.editing.is_some() {
                                "Save"
                            } else {
                                "Create"
                            }),
                        )
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
                Some(id) => update_context(cx.engine, id, form.input()).map(|()| id),
                None => create_context(cx.engine, form.input()),
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

    #[test]
    /// A new context is blue and empty.
    fn context_form_defaults_to_blue() {
        let f = ContextForm::default();
        assert_eq!(f.rgb, [0, 0, 255]);
        assert_eq!(f.editing, None);
    }
}
