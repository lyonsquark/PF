//! Context tags on a transaction row: the chips and the "+" toggle menu (Swift Context column, T15b).

use egui::containers::menu::MenuButton;
use egui::{Color32, Frame, Margin, RichText, Ui};
use pf_model::ContextId;

use super::{color_from_hex, colors};

/// A context as stored on a row: (id, name, colour hex).
pub type ContextEntry = (ContextId, String, Option<String>);

/// The chips "#name" (fill at 15 % of the colour), or a weak "No Context". `locked` fades them
/// (R-TXN-1).
pub fn context_chips(ui: &mut Ui, chips: &[ContextEntry], locked: bool) {
    if chips.is_empty() {
        ui.label(RichText::new("No Context").weak());
        return;
    }
    ui.horizontal(|ui| {
        for (_, name, hex) in chips {
            let color = color_from_hex(hex.as_deref());
            let color = if locked {
                color.gamma_multiply(0.6)
            } else {
                color
            };
            chip(ui, name, color);
        }
    });
}

/// One chip: "#name" on a tinted rounded frame.
fn chip(ui: &mut Ui, name: &str, color: Color32) {
    Frame::new()
        .fill(color.gamma_multiply(0.15))
        .corner_radius(6.0)
        .inner_margin(Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(format!("#{name}")).small().color(color));
        });
}

/// The "+" menu (hover text "Add or remove context tags") listing `all` (non-archived contexts),
/// with "• " on the selected ones. Clicking toggles: removes when present, else appends at the end.
/// Returns `Some(new list)` when a click changed it.
pub fn context_toggle_menu(
    ui: &mut Ui,
    current: &[ContextId],
    all: &[ContextEntry],
) -> Option<Vec<ContextId>> {
    let mut changed = None;
    MenuButton::new(RichText::new("+").color(colors::BLUE))
        .ui(ui, |ui| {
            for (id, name, _) in all {
                let selected = current.contains(id);
                let label = if selected {
                    format!("• {name}")
                } else {
                    name.clone()
                };
                if ui.button(label).clicked() {
                    let mut next = current.to_vec();
                    if selected {
                        next.retain(|c| c != id);
                    } else {
                        next.push(*id);
                    }
                    changed = Some(next);
                }
            }
        })
        .0
        .on_hover_text("Add or remove context tags");
    changed
}
