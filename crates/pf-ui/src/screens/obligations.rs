//! Obligations screen. Implemented in T18.
use crate::app::UiCtx;

/// Per-screen UI state (filled in by T18).
#[derive(Default)]
pub struct State {}

/// Draws the screen into the central panel.
pub fn show(_cx: &mut UiCtx, _st: &mut State, ui: &mut egui::Ui) {
    ui.heading("Obligations");
}
