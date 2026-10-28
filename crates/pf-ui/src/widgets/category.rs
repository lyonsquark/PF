//! Category picker for a transaction row (Swift Category column, T15b).

use std::fmt::Debug;
use std::hash::Hash;

use egui::{ComboBox, Role, Ui, WidgetInfo};
use pf_model::CategoryId;

/// A category picker: "None" first, then `cats` in the order given (already sorted, R-CAT-4).
/// Returns `Some(new)` when the choice changes. `a11y` is the accessible label
/// (`Category {payee}`).
/// Width of the category picker, in points.
pub const PICKER_WIDTH: f32 = 150.0;

pub fn category_picker(
    ui: &mut Ui,
    id_salt: impl Hash + Debug,
    current: Option<CategoryId>,
    cats: &[(CategoryId, String)],
    enabled: bool,
    a11y: &str,
) -> Option<Option<CategoryId>> {
    let shown = current
        .and_then(|id| cats.iter().find(|(c, _)| *c == id))
        .map_or("None", |(_, name)| name.as_str())
        .to_string();
    let mut picked = None;
    ui.add_enabled_ui(enabled, |ui| {
        let resp = ComboBox::from_id_salt(id_salt)
            .width(PICKER_WIDTH)
            .selected_text(shown)
            .show_ui(ui, |ui| {
                if ui.selectable_label(current.is_none(), "None").clicked() && current.is_some() {
                    picked = Some(None);
                }
                for (id, name) in cats {
                    let selected = current == Some(*id);
                    if ui.selectable_label(selected, name).clicked() && !selected {
                        picked = Some(Some(*id));
                    }
                }
            });
        resp.response
            .widget_info(|| WidgetInfo::labeled(Role::ComboBox, enabled, a11y));
    });
    picked
}
