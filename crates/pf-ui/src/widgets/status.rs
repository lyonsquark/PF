//! Status colours and the status picker (Swift `statusColor` and the Status column, T15b).

use std::fmt::Debug;
use std::hash::Hash;

use egui::{Color32, ComboBox, RichText, Role, Ui, WidgetInfo};
use pf_model::TransactionStatus;

use super::colors;

/// Picker order (Swift picker: Cleared, Pending, Reconciled, Future).
const ORDER: [TransactionStatus; 4] = [
    TransactionStatus::Cleared,
    TransactionStatus::Pending,
    TransactionStatus::Reconciled,
    TransactionStatus::Future,
];

/// Text colour for a status. Cleared has none (normal text).
/// Width of the status picker, in points.
pub const PICKER_WIDTH: f32 = 110.0;

pub fn status_color(s: TransactionStatus) -> Option<Color32> {
    match s {
        TransactionStatus::Cleared => None,
        TransactionStatus::Pending => Some(colors::GREEN),
        TransactionStatus::Reconciled => Some(colors::BROWN),
        TransactionStatus::Future => Some(colors::RED),
    }
}

/// The status as coloured text.
fn status_text(s: TransactionStatus) -> RichText {
    let text = RichText::new(s.label());
    match status_color(s) {
        Some(c) => text.color(c),
        None => text,
    }
}

/// A status picker. Returns `Some(new)` when a different status is picked. `a11y` is the
/// accessible label (`Status {payee}`) so tests can find the row's picker.
pub fn status_picker(
    ui: &mut Ui,
    id_salt: impl Hash + Debug,
    current: TransactionStatus,
    enabled: bool,
    a11y: &str,
) -> Option<TransactionStatus> {
    let mut picked = None;
    ui.add_enabled_ui(enabled, |ui| {
        let resp = ComboBox::from_id_salt(id_salt)
            .width(PICKER_WIDTH)
            .selected_text(status_text(current))
            .show_ui(ui, |ui| {
                for s in ORDER {
                    if ui.selectable_label(s == current, status_text(s)).clicked() && s != current {
                        picked = Some(s);
                    }
                }
            });
        resp.response
            .widget_info(|| WidgetInfo::labeled(Role::ComboBox, enabled, a11y));
    });
    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Covers: T15b status colours. Cleared is plain; Pending green; Reconciled brown; Future red.
    fn status_color_matches_swift() {
        assert_eq!(status_color(TransactionStatus::Cleared), None);
        assert_eq!(
            status_color(TransactionStatus::Pending),
            Some(colors::GREEN)
        );
        assert_eq!(
            status_color(TransactionStatus::Reconciled),
            Some(colors::BROWN)
        );
        assert_eq!(status_color(TransactionStatus::Future), Some(colors::RED));
    }
}
