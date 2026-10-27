//! Font setup: uses the macOS system UI font (SF Pro) when it is present, else egui's defaults.

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily};

/// The macOS system UI font. Read at runtime from the user's own Mac; never bundled.
pub const UI_FONT_PATH: &str = "/System/Library/Fonts/SFNS.ttf";
/// The macOS system monospaced font, used for numbers in tables.
pub const MONO_FONT_PATH: &str = "/System/Library/Fonts/SFNSMono.ttf";

/// Builds the font definitions. `None` keeps egui's default for that family.
pub fn definitions(ui_font: Option<Vec<u8>>, mono_font: Option<Vec<u8>>) -> FontDefinitions {
    let mut defs = FontDefinitions::default();
    if let Some(bytes) = ui_font {
        defs.font_data
            .insert("sf".to_owned(), Arc::new(FontData::from_owned(bytes)));
        defs.families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "sf".to_owned());
    }
    if let Some(bytes) = mono_font {
        defs.font_data
            .insert("sf_mono".to_owned(), Arc::new(FontData::from_owned(bytes)));
        defs.families
            .entry(FontFamily::Monospace)
            .or_default()
            .insert(0, "sf_mono".to_owned());
    }
    defs
}

/// Reads the system fonts and installs them on the egui context. Missing files are logged and skipped.
pub fn install(ctx: &egui::Context) {
    let ui_font = read_optional(UI_FONT_PATH);
    let mono_font = read_optional(MONO_FONT_PATH);
    ctx.set_fonts(definitions(ui_font, mono_font));
}

fn read_optional(path: &str) -> Option<Vec<u8>> {
    match std::fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(e) => {
            log::warn!("system font {path} not loaded: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fonts_keep_egui_defaults() {
        let defs = definitions(None, None);
        assert_eq!(defs, FontDefinitions::default());
    }

    #[test]
    fn system_ui_font_is_first_proportional() {
        let defs = definitions(Some(vec![0, 1, 2]), None);
        let first = defs.families[&FontFamily::Proportional].first();
        assert_eq!(first.map(String::as_str), Some("sf"));
        assert!(defs.font_data.contains_key("sf"));
    }

    #[test]
    fn mono_font_is_first_monospace() {
        let defs = definitions(None, Some(vec![0, 1, 2]));
        let first = defs.families[&FontFamily::Monospace].first();
        assert_eq!(first.map(String::as_str), Some("sf_mono"));
    }
}
