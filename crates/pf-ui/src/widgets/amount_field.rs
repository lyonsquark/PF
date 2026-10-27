//! Amount editing (port of Swift `AmountEditField.swift`, T15b). Idle, the field shows the money
//! label. A click (when not locked) switches to a plain text field holding the raw number. Enter,
//! or losing focus, commits a changed value; Esc cancels.

use std::str::FromStr;

use egui::{
    Align2, Color32, Frame, Id, Key, Label, RichText, Role, Sense, TextEdit, Ui, WidgetInfo,
};
use pf_core::format;
use rust_decimal::{Decimal, RoundingStrategy};

use super::colors;

/// Text shown while editing: no grouping, no currency, at most 2 decimals, trailing zeros trimmed.
/// Half-way values round away from zero (0.125 ⇒ "0.13").
pub fn amount_edit_text(d: Decimal) -> String {
    d.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
        .normalize()
        .to_string()
}

/// Parses user input: "$" and "," are removed and surrounding space is trimmed. `None` when the
/// rest is not a decimal number.
pub fn parse_amount_input(s: &str) -> Option<Decimal> {
    let cleaned: String = s
        .chars()
        .filter(|c| *c != '$' && *c != ',')
        .collect::<String>();
    Decimal::from_str(cleaned.trim()).ok()
}

/// Colour of the money label: green when positive, faded when locked (Swift `foregroundColor`).
fn label_color(value: Decimal, locked: bool) -> Option<Color32> {
    if value > Decimal::ZERO {
        Some(if locked {
            colors::GREEN.gamma_multiply(0.6)
        } else {
            colors::GREEN
        })
    } else {
        None
    }
}

/// The amount field. `buf` holds the edit text while editing and is `None` when idle; the caller
/// stores it between frames. Returns `Some(new)` only when an edit commits a parsed value that
/// differs from `value`.
pub fn amount_field(
    ui: &mut Ui,
    a11y_label: &str,
    value: Decimal,
    locked: bool,
    buf: &mut Option<String>,
) -> Option<Decimal> {
    let id = ui.scope_id().with(("amount_edit", a11y_label));
    let Some(current) = buf.clone() else {
        return idle(ui, a11y_label, value, locked, id, buf);
    };
    let mut text = current;
    let resp = ui.add(
        TextEdit::singleline(&mut text)
            .id(id)
            .frame(Frame::NONE)
            .align(Align2::RIGHT_CENTER)
            .desired_width(f32::INFINITY),
    );
    resp.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, a11y_label));
    // Esc cancels: no commit, and the buffer is dropped. egui's TextEdit surrenders focus on Esc
    // itself, so the key is checked whenever the field had focus this frame or just lost it.
    if ui.input(|i| i.key_pressed(Key::Escape)) && (resp.has_focus() || resp.lost_focus()) {
        *buf = None;
        resp.surrender_focus();
        return None;
    }
    if resp.lost_focus() {
        // Enter and click-away both end here; the buffer is dropped either way.
        *buf = None;
        return parse_amount_input(&text).filter(|v| *v != value);
    }
    *buf = Some(text);
    None
}

/// Idle state: the money label. A click starts editing and focuses the text field next frame.
fn idle(
    ui: &mut Ui,
    a11y_label: &str,
    value: Decimal,
    locked: bool,
    id: Id,
    buf: &mut Option<String>,
) -> Option<Decimal> {
    let mut rich = RichText::new(format::format_currency(value));
    match label_color(value, locked) {
        Some(color) => rich = rich.color(color),
        None if locked => rich = rich.weak(),
        None => {}
    }
    let sense = if locked {
        Sense::hover()
    } else {
        Sense::click()
    };
    let resp = ui.add(Label::new(rich).sense(sense));
    resp.widget_info(|| WidgetInfo::labeled(Role::Button, !locked, a11y_label));
    if !locked && resp.clicked() {
        *buf = Some(amount_edit_text(value));
        ui.ctx().memory_mut(|m| m.request_focus(id));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    /// Covers: T15b amount text. Plain numbers, two decimals at most, half away from zero.
    fn amount_edit_text_formats_like_swift() {
        assert_eq!(amount_edit_text(dec!(1234.5)), "1234.5");
        assert_eq!(amount_edit_text(dec!(-12)), "-12");
        assert_eq!(amount_edit_text(dec!(0.125)), "0.13");
        assert_eq!(amount_edit_text(dec!(1000)), "1000");
        assert_eq!(amount_edit_text(dec!(-0.5)), "-0.5");
    }

    #[test]
    /// Covers: T15b amount input. "$" and "," are stripped; anything else must be a number.
    fn parse_amount_input_strips_symbols() {
        assert_eq!(parse_amount_input("$1,234.50"), Some(dec!(1234.50)));
        assert_eq!(parse_amount_input(" -12 "), Some(dec!(-12)));
        assert_eq!(parse_amount_input("abc"), None);
        assert_eq!(parse_amount_input(""), None);
    }
}
