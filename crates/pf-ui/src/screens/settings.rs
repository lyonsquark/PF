//! Settings screen (preferences). Currently: the UI font.
use egui::Ui;
use pf_engine::commands::settings;

use crate::app::UiCtx;
use crate::fonts;

/// Per-screen UI state (none yet).
#[derive(Default)]
pub struct State {}

/// Draws the screen into the central panel.
pub fn show(cx: &mut UiCtx, _st: &mut State, ui: &mut Ui) {
    ui.heading("Settings");
    ui.add_space(8.0);
    font_row(cx, ui);
}

/// The "Font" row: a combo box over `fonts::CHOICES`. A new choice is saved to `uiFont` through
/// the engine and applied to the egui context at once.
fn font_row(cx: &mut UiCtx, ui: &mut Ui) {
    let current = fonts::choice_for_key(settings::ui_font(cx.engine)).key;
    let mut selected = current;
    ui.horizontal(|ui| {
        let label = ui.label("Font");
        let shown = fonts::choice_for_key(Some(current)).label;
        egui::ComboBox::from_id_salt("ui_font")
            .selected_text(shown)
            .show_ui(ui, |ui| {
                for choice in &fonts::CHOICES {
                    ui.selectable_value(&mut selected, choice.key, choice.label);
                }
            })
            .response
            .labelled_by(label.id);
    });
    if selected != current {
        match settings::set_ui_font(cx.engine, selected) {
            Ok(()) => fonts::install(ui.ctx(), Some(selected)),
            Err(e) => cx.toasts.info(format!("Could not save the font: {e}")),
        }
    }
}
