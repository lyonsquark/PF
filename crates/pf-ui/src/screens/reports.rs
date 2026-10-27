//! Reports screen. Implemented in T23.
use crate::app::UiCtx;

/// Per-screen UI state (filled in by T23).
#[derive(Default)]
pub struct State {}

/// Draws the screen into the central panel.
pub fn show(_cx: &mut UiCtx, _st: &mut State, ui: &mut egui::Ui) {
    ui.heading("Reports");
}
