//! The scenario menu in the toolbar (Swift `ScenariosMenu.swift`, R-SCN-2, UI-SCN-01).

use egui::{RichText, Ui};

use crate::app::UiCtx;
use crate::widgets::colors;

/// Draws the scenario menu button and its items. Picking a scenario is not an undo step.
pub fn show(cx: &mut UiCtx, ui: &mut Ui) {
    let active = cx.engine.state.active_scenario;
    let scenarios = cx.caches.lookups(&*cx.engine).scenarios.clone();
    let active_name = active
        .and_then(|id| cx.engine.db().scenario(id))
        .map(|s| s.name.clone());
    let shown = active_name.unwrap_or_else(|| "Reality".to_string());
    let text_color = if active.is_some() {
        colors::ORANGE
    } else {
        ui.visuals().text_color()
    };
    // The visible text is the label, so kittest and screen readers see the name itself.
    let button_text = RichText::new(shown.as_str()).color(text_color);
    ui.menu_button(button_text, |ui| {
        let reality = if active.is_none() {
            "• Reality"
        } else {
            "Reality"
        };
        if ui.button(reality).clicked() {
            cx.engine.set_active_scenario(None);
            ui.close();
        }
        ui.separator();
        for (id, name) in &scenarios {
            let label = if active == Some(*id) {
                format!("• {name}")
            } else {
                name.clone()
            };
            if ui.button(label).clicked() {
                cx.engine.toggle_scenario(Some(*id));
                ui.close();
            }
        }
    });
}
