//! Account detail screen. Implemented in T15a.
use pf_model::AccountId;

use crate::app::UiCtx;

/// Per-screen UI state (filled in by T15a).
#[derive(Default)]
pub struct State {}

/// Draws the account detail for `id` into the central panel.
pub fn show(_cx: &mut UiCtx, _st: &mut State, ui: &mut egui::Ui, id: AccountId) {
    ui.heading("Account");
    ui.label(format!("Account {} (detail arrives in T15a)", id.raw()));
}
