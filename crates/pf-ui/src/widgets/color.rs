//! Scenario and context colours (T19): hex text to and from colours, a colour button for a form, and a
//! colour dot for a list. Hex is "#RRGGBB" (R-CTX-4), lower case when written (Swift `toHex()`).

use egui::{Color32, Response, Role, Sense, Ui, WidgetInfo};

/// The colour used when a colour is missing or invalid (Swift purple, `#800080`).
pub const FALLBACK_RGB: [u8; 3] = [0x80, 0x00, 0x80];

/// Parses "#RRGGBB" or "#RRGGBBAA" (R-CTX-4, `pf_core`). `None` if the text is not a colour.
pub fn parse_hex(hex: &str) -> Option<Color32> {
    pf_core::color::parse_hex(hex).map(|[r, g, b, a]| Color32::from_rgba_unmultiplied(r, g, b, a))
}

/// Lower-case "#rrggbb" for an RGB colour (matches Swift `toHex()`, which uses "%06x").
pub fn to_hex(rgb: [u8; 3]) -> String {
    pf_core::color::to_hex(rgb)
}

/// The colour for a stored hex value, or the purple fallback when it is missing or invalid.
pub fn rgb_or_fallback(hex: Option<&str>) -> [u8; 3] {
    hex.and_then(pf_core::color::parse_hex)
        .map_or(FALLBACK_RGB, |[r, g, b, _]| [r, g, b])
}

/// A labelled colour button. Returns true when the colour changed.
pub fn color_field(ui: &mut Ui, label: &str, rgb: &mut [u8; 3]) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        let resp: Response = ui.color_edit_button_srgb(rgb);
        resp.widget_info(|| WidgetInfo::labeled(Role::ColorWell, true, label));
        resp.changed()
    })
    .inner
}

/// A filled 12-point circle, for a colour dot in a list.
pub fn color_dot(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 6.0, color);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Covers: R-CTX-4. Hex is written in lower case.
    fn color_to_hex_lowercase() {
        assert_eq!(to_hex([255, 0, 170]), "#ff00aa");
        assert_eq!(to_hex(FALLBACK_RGB), "#800080");
    }

    #[test]
    /// Covers: R-CTX-4. Six or eight digits parse (with or without "#"); anything else is None.
    fn color_parse_hex_variants() {
        assert_eq!(parse_hex("#ff0000"), Some(Color32::from_rgb(255, 0, 0)));
        assert_eq!(parse_hex("ff0000"), Some(Color32::from_rgb(255, 0, 0)));
        assert!(parse_hex("#ff000080").is_some());
        assert_eq!(parse_hex("#123"), None);
        assert_eq!(parse_hex("zzzzzz"), None);
        assert_eq!(parse_hex(""), None);
    }

    #[test]
    /// A missing or invalid stored colour falls back to purple.
    fn rgb_or_fallback_uses_purple_when_invalid() {
        assert_eq!(rgb_or_fallback(None), FALLBACK_RGB);
        assert_eq!(rgb_or_fallback(Some("nope")), FALLBACK_RGB);
        assert_eq!(rgb_or_fallback(Some("#00ff00")), [0, 255, 0]);
    }
}
