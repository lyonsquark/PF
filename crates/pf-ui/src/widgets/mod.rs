//! Shared widgets and colours used across screens.

pub mod account_multiselect;
pub mod amount_field;
pub mod category;
pub mod color;
pub mod contexts;
pub mod hscroll;
pub mod status;

use egui::{Color32, Response, Ui};
use pf_core::{color as core_color, format};
use rust_decimal::Decimal;

/// System-like colours used across screens (match the Swift app's SwiftUI colours).
pub mod colors {
    use egui::Color32;

    /// Positive amounts and success.
    pub const GREEN: Color32 = Color32::from_rgb(52, 199, 89);
    /// Links and the default context colour.
    pub const BLUE: Color32 = Color32::from_rgb(0, 122, 255);
    /// Negative amounts and destructive actions.
    pub const RED: Color32 = Color32::from_rgb(255, 59, 48);
    /// The active scenario.
    pub const ORANGE: Color32 = Color32::from_rgb(255, 149, 0);
    /// Reconciled status.
    pub const BROWN: Color32 = Color32::from_rgb(140, 69, 18);
}

/// Money text "$1,234.56" / "-$1,234.56" via pf_core's currency formatter; red when negative if
/// `red_negative`.
pub fn money_label(ui: &mut Ui, amount: Decimal, red_negative: bool) -> Response {
    let text = format::format_currency(amount);
    if red_negative && amount < Decimal::ZERO {
        ui.colored_label(colors::RED, text)
    } else {
        ui.label(text)
    }
}

/// Parses "#RRGGBB" / "#RRGGBBAA" (R-CTX-4, pf_core) into a colour. Anything else, or `None`,
/// gives the default blue.
pub fn color_from_hex(hex: Option<&str>) -> Color32 {
    let [r, g, b, a] = hex
        .and_then(core_color::parse_hex)
        .unwrap_or(core_color::DEFAULT_BLUE);
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// A text field with an accessible label (kittest finds fields by label).
pub fn labeled_text(ui: &mut Ui, label: &str, text: &mut String) -> Response {
    let caption = ui.label(label);
    ui.add(egui::TextEdit::singleline(text))
        .labelled_by(caption.id)
}

/// A heading on its own row.
pub fn heading_row(ui: &mut Ui, title: &str) -> Response {
    ui.horizontal(|ui| ui.heading(title)).inner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Covers: R-CTX-4. Hex parsing: six digits, eight digits (alpha), and the blue default.
    fn color_from_hex_parses_and_defaults() {
        let srgba = |c: Color32| c.to_srgba_unmultiplied();
        assert_eq!(srgba(color_from_hex(Some("#FF0000"))), [255, 0, 0, 255]);
        assert_eq!(srgba(color_from_hex(Some("#FF000080"))), [255, 0, 0, 128]);
        assert_eq!(srgba(color_from_hex(Some("zz"))), [0, 122, 255, 255]);
        assert_eq!(srgba(color_from_hex(None)), [0, 122, 255, 255]);
    }
}
