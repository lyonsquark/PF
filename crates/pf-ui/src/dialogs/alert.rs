//! Alert (OK) and Confirm (Cancel / coloured confirm) modals.

use egui::{Id, Modal, RichText, Ui};

use crate::dialogs::Outcome;
use crate::widgets::colors;

/// The user's reply to a confirmation this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reply {
    /// No button pressed yet.
    Pending,
    /// Cancel, Escape, or a click outside.
    Cancel,
    /// The confirm button.
    Accept,
}

/// Draws an alert with an OK button. Escape or a click outside also closes it.
pub fn show_alert(ui: &mut Ui, title: &str, message: &str) -> Outcome {
    let mut ok = false;
    let response = Modal::new(Id::unique(("alert", title))).show(ui.ctx(), |ui| {
        ui.heading(title);
        ui.label(message);
        ui.add_space(8.0);
        if ui.button("OK").clicked() {
            ok = true;
        }
    });
    if ok || response.should_close() {
        Outcome::Close
    } else {
        Outcome::KeepOpen
    }
}

/// Draws a confirmation with Cancel and a red confirm button.
pub fn show_confirm(ui: &mut Ui, title: &str, message: &str, confirm_label: &str) -> Reply {
    let mut reply = Reply::Pending;
    let response = Modal::new(Id::unique(("confirm", title))).show(ui.ctx(), |ui| {
        ui.heading(title);
        ui.label(message);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() {
                reply = Reply::Cancel;
            }
            let confirm = RichText::new(confirm_label).color(colors::RED);
            if ui.button(confirm).clicked() {
                reply = Reply::Accept;
            }
        });
    });
    if reply == Reply::Pending && response.should_close() {
        Reply::Cancel
    } else {
        reply
    }
}
