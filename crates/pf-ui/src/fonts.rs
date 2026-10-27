//! Font setup: the UI font is one of a fixed list of macOS `.ttf` files (SF Pro by default), and
//! the monospaced font is always SF Pro Mono. Missing files are logged and egui's defaults are used.

use std::sync::Arc;

use egui::epaint::text::VariationCoords;
use egui::{Color32, FontData, FontDefinitions, FontFamily};

/// Weight for the UI font's `wght` axis (SF Pro is variable). 400 looked thin next to native macOS text.
pub const UI_FONT_WEIGHT: f32 = 500.0;

/// The macOS monospaced font, used for numbers in tables. Read at runtime from the user's own Mac.
pub const MONO_FONT_PATH: &str = "/System/Library/Fonts/SFNSMono.ttf";

/// One choice in Settings: a stable key stored in the `uiFont` setting, the plain-text label
/// shown in the combo box, and the macOS font file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiFontChoice {
    /// Stable key stored in the `uiFont` setting.
    pub key: &'static str,
    /// Label shown in Settings (plain ASCII; the default font lacks some symbols).
    pub label: &'static str,
    /// Absolute macOS path to a `.ttf` file (TTC collections are not used).
    pub path: &'static str,
}

/// SF Pro, the default UI font (Settings and the `uiFont` setting when missing or unknown).
pub const SF_PRO: UiFontChoice = UiFontChoice {
    key: "sf_pro",
    label: "SF Pro",
    path: "/System/Library/Fonts/SFNS.ttf",
};

/// Every font offered in Settings. The first entry is the default. Each file was checked on macOS.
pub const CHOICES: [UiFontChoice; 6] = [
    SF_PRO,
    UiFontChoice {
        key: "arial",
        label: "Arial",
        path: "/System/Library/Fonts/Supplemental/Arial.ttf",
    },
    UiFontChoice {
        key: "verdana",
        label: "Verdana",
        path: "/System/Library/Fonts/Supplemental/Verdana.ttf",
    },
    UiFontChoice {
        key: "tahoma",
        label: "Tahoma",
        path: "/System/Library/Fonts/Supplemental/Tahoma.ttf",
    },
    UiFontChoice {
        key: "trebuchet",
        label: "Trebuchet MS",
        path: "/System/Library/Fonts/Supplemental/Trebuchet MS.ttf",
    },
    UiFontChoice {
        key: "geneva",
        label: "Geneva",
        path: "/System/Library/Fonts/Geneva.ttf",
    },
];

/// The choice for a stored `uiFont` key. A missing or unknown key gives the default (SF Pro).
pub fn choice_for_key(key: Option<&str>) -> &'static UiFontChoice {
    key.and_then(|k| CHOICES.iter().find(|c| c.key == k))
        .unwrap_or(&SF_PRO)
}

/// Builds the font definitions. `None` keeps egui's default for that family.
pub fn definitions(ui_font: Option<Vec<u8>>, mono_font: Option<Vec<u8>>) -> FontDefinitions {
    let mut defs = FontDefinitions::default();
    if let Some(bytes) = ui_font {
        let mut data = FontData::from_owned(bytes);
        data.tweak.coords = VariationCoords::new([(b"wght", UI_FONT_WEIGHT)]);
        defs.font_data.insert("ui_font".to_owned(), Arc::new(data));
        defs.families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "ui_font".to_owned());
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

/// Installs the chosen UI font (proportional) and SF Pro Mono (monospace) on the egui context.
/// `key` is the stored `uiFont` setting; missing or unknown keys give SF Pro. A missing font file
/// is logged and that family keeps egui's default.
pub fn install(ctx: &egui::Context, key: Option<&str>) {
    let choice = choice_for_key(key);
    let ui_font = read_optional(choice.path);
    let mono_font = read_optional(MONO_FONT_PATH);
    ctx.set_fonts(definitions(ui_font, mono_font));
}

/// Text colour override for light mode: macOS draws text darker than egui's default grey.
/// Dark mode keeps egui's own colours, so `None` is returned.
pub fn text_color_for(dark_mode: bool) -> Option<Color32> {
    if dark_mode {
        None
    } else {
        Some(Color32::BLACK)
    }
}

/// Applies the text colour for the current theme, writing only when it changes.
pub fn apply_text_contrast(ctx: &egui::Context) {
    let want = text_color_for(ctx.global_style().visuals.dark_mode);
    if ctx.global_style().visuals.override_text_color != want {
        ctx.global_style_mut(|style| style.visuals.override_text_color = want);
    }
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
    use std::path::Path;

    #[test]
    fn default_choice_is_sf_pro() {
        let choice = choice_for_key(None);
        assert_eq!(choice.key, "sf_pro");
        assert_eq!(choice.label, "SF Pro");
        assert_eq!(choice.path, "/System/Library/Fonts/SFNS.ttf");
        assert_eq!(CHOICES[0], SF_PRO);
    }

    #[test]
    fn key_maps_to_its_font_path() {
        let arial = choice_for_key(Some("arial"));
        assert_eq!(arial.label, "Arial");
        assert_eq!(arial.path, "/System/Library/Fonts/Supplemental/Arial.ttf");
        let trebuchet = choice_for_key(Some("trebuchet"));
        assert_eq!(
            trebuchet.path,
            "/System/Library/Fonts/Supplemental/Trebuchet MS.ttf"
        );
    }

    #[test]
    fn unknown_key_falls_back_to_default() {
        assert_eq!(choice_for_key(Some("comic_sans")), &SF_PRO);
        assert_eq!(choice_for_key(Some("")), &SF_PRO);
    }

    #[test]
    fn choices_are_unique_short_ttf_files_with_plain_labels() {
        assert!(CHOICES.len() <= 6);
        for (i, a) in CHOICES.iter().enumerate() {
            assert!(a.path.ends_with(".ttf"), "{} is not a .ttf", a.path);
            assert!(a.label.is_ascii() && !a.label.is_empty());
            for b in &CHOICES[i + 1..] {
                assert_ne!(a.key, b.key);
            }
        }
    }

    #[test]
    fn chosen_font_becomes_first_proportional() {
        let defs = definitions(Some(vec![0, 1, 2]), None);
        let first = defs.families[&FontFamily::Proportional].first();
        assert_eq!(first.map(String::as_str), Some("ui_font"));
        assert!(defs.font_data.contains_key("ui_font"));
    }

    #[test]
    fn missing_fonts_keep_egui_defaults() {
        let defs = definitions(None, None);
        assert_eq!(defs, FontDefinitions::default());
    }

    #[test]
    fn light_mode_text_is_black_and_dark_mode_keeps_defaults() {
        assert_eq!(text_color_for(false), Some(Color32::BLACK));
        assert_eq!(text_color_for(true), None);
    }

    #[test]
    fn mono_font_is_first_monospace() {
        let defs = definitions(None, Some(vec![0, 1, 2]));
        let first = defs.families[&FontFamily::Monospace].first();
        assert_eq!(first.map(String::as_str), Some("sf_mono"));
    }

    #[test]
    /// Every choice whose file exists on this Mac parses in egui (a bad font panics in layout).
    fn every_present_choice_loads_in_egui() {
        for choice in CHOICES {
            if !Path::new(choice.path).exists() {
                continue;
            }
            let ctx = egui::Context::default();
            install(&ctx, Some(choice.key));
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                ui.label("Aa 123");
            });
            out.textures_delta.clear();
        }
    }
}
