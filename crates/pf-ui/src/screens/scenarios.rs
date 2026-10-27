//! Scenarios screen (T19, Swift `ScenariosView`): the live scenarios by name, each with its colour dot.
//! A click makes a scenario active (or back to Reality, R-SCN-2) through the engine's state, so the
//! toolbar menu, the dashboard and the charts agree. Right-click offers Edit and Delete (R-SCN-3).

use egui::{Align, Layout, RichText, Sense, Ui};
use pf_engine::commands::scenarios::delete_scenario;
use pf_model::ScenarioId;

use crate::app::UiCtx;
use crate::dialogs::Dialog;
use crate::dialogs::scenario_edit::ScenarioForm;
use crate::widgets::color::{color_dot, parse_hex};
use crate::widgets::colors;

/// Colour of the dot when a scenario has none (matches the dialog's default).
const FALLBACK: egui::Color32 = egui::Color32::from_rgb(0x80, 0x00, 0x80);

/// Per-screen UI state. Nothing is kept here: the active scenario lives in the engine.
#[derive(Default)]
pub struct State {}

/// What the user asked for this frame, applied after the list is drawn.
enum Action {
    Add,
    Toggle(ScenarioId),
    Edit(ScenarioId),
    Delete(ScenarioId),
}

/// Draws the Scenarios screen into the central panel.
pub fn show(cx: &mut UiCtx, _st: &mut State, ui: &mut Ui) {
    let mut action: Option<Action> = None;
    ui.horizontal(|ui| {
        ui.heading("Scenarios");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button("+ Add Scenario").clicked() {
                action = Some(Action::Add);
            }
        });
    });
    ui.separator();
    ui.label(RichText::new("Scenarios").strong());

    let active = cx.engine.state.active_scenario;
    let rows: Vec<(ScenarioId, String, Option<String>, Option<String>)> = cx
        .engine
        .db()
        .scenarios_by_name()
        .into_iter()
        .map(|s| (s.id, s.name.clone(), s.color_hex.clone(), s.notes.clone()))
        .collect();
    if rows.is_empty() {
        ui.add_space(12.0);
        ui.label(RichText::new("No scenarios yet").weak());
    }
    for (id, name, color, notes) in &rows {
        let is_active = active == Some(*id);
        let resp = ui
            .horizontal(|ui| {
                let dot = color.as_deref().and_then(parse_hex).unwrap_or(FALLBACK);
                color_dot(ui, dot);
                ui.vertical(|ui| {
                    let mut label = RichText::new(name).strong();
                    if is_active {
                        label = label.color(colors::ORANGE);
                    }
                    ui.label(label);
                    if let Some(notes) = notes {
                        ui.label(RichText::new(notes).small().weak());
                    }
                });
                if is_active {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new("Active").small().color(colors::ORANGE));
                    });
                }
            })
            .response;
        let click = ui.interact(
            resp.rect,
            ui.scope_id().with(("scenario_row", id.raw())),
            Sense::click(),
        );
        if click.clicked() {
            action = Some(Action::Toggle(*id));
        }
        click.context_menu(|ui| {
            if ui.button("Edit Scenario").clicked() {
                action = Some(Action::Edit(*id));
                ui.close();
            }
            ui.separator();
            if ui.button("Delete").clicked() {
                action = Some(Action::Delete(*id));
                ui.close();
            }
        });
        ui.add_space(4.0);
    }

    match action {
        None => {}
        Some(Action::Add) => {
            cx.dialogs.open(Dialog::ScenarioEdit(Box::default()));
        }
        Some(Action::Toggle(id)) => cx.engine.toggle_scenario(Some(id)),
        Some(Action::Edit(id)) => {
            if let Some(form) = ScenarioForm::edit(cx.engine.db(), id) {
                cx.dialogs.open(Dialog::ScenarioEdit(Box::new(form)));
            }
        }
        Some(Action::Delete(id)) => {
            if let Err(e) = delete_scenario(cx.engine, id) {
                cx.dialogs.alert("Delete Scenario", e.to_string());
            }
        }
    }
}
