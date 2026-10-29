//! Category picker for a transaction row (Swift Category column, T15b).

use std::fmt::Debug;
use std::hash::Hash;

use egui::{ComboBox, Event, Key, Modifiers, Role, Ui, WidgetInfo};
use pf_model::CategoryId;

/// Width of the category picker, in points.
pub const PICKER_WIDTH: f32 = 150.0;

/// Seconds without a key press after which the typed letters start again from nothing.
const TYPE_AHEAD_RESET_SECS: f64 = 1.0;

/// Type-ahead memory for one picker while its list is open: the letters typed so far, when the
/// last one came, and the category the typing has moved the selection bar to.
#[derive(Clone, Default)]
struct TypeAhead {
    text: String,
    at: f64,
    highlight: Option<CategoryId>,
}

/// The first category whose name starts with `typed` (ignoring case); if none does, the first
/// whose name contains it. A blank `typed` matches nothing.
pub fn type_ahead_match(cats: &[(CategoryId, String)], typed: &str) -> Option<CategoryId> {
    let typed = typed.trim().to_lowercase();
    if typed.is_empty() {
        return None;
    }
    let starts = cats
        .iter()
        .find(|(_, name)| name.to_lowercase().starts_with(&typed));
    let found = starts.or_else(|| {
        cats.iter()
            .find(|(_, name)| name.to_lowercase().contains(&typed))
    });
    found.map(|(id, _)| *id)
}

/// A category picker: "None" first, then `cats` in the order given (already sorted, R-CAT-4).
/// Returns `Some(new)` when the choice changes. `a11y` is the accessible label
/// (`Category {payee}`).
///
/// While the list is open, typing letters moves the selection bar to the first matching category
/// and the list scrolls to it; typing more letters refines the match (a pause of a second starts
/// over). Enter picks the highlighted category.
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
    let memo_id = ui.make_persistent_id(("category_type_ahead", &id_salt));
    ui.add_enabled_ui(enabled, |ui| {
        let resp = ComboBox::from_id_salt(id_salt)
            .width(PICKER_WIDTH)
            .selected_text(shown)
            .show_ui(ui, |ui| {
                let mut memo: TypeAhead =
                    ui.ctx().data(|d| d.get_temp(memo_id)).unwrap_or_default();
                let now = ui.input(|i| i.time);
                if now - memo.at > TYPE_AHEAD_RESET_SECS {
                    memo.text.clear();
                    memo.highlight = None;
                }
                let typed: String = ui.input(|i| {
                    i.events
                        .iter()
                        .filter_map(|e| match e {
                            Event::Text(t) => Some(t.as_str()),
                            _ => None,
                        })
                        .collect()
                });
                let mut moved = false;
                if !typed.is_empty() {
                    memo.text.push_str(&typed);
                    memo.at = now;
                    memo.highlight = type_ahead_match(cats, &memo.text);
                    moved = true;
                }
                let enter = ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
                if let (true, Some(id)) = (enter, memo.highlight) {
                    if current != Some(id) {
                        picked = Some(Some(id));
                    }
                    memo = TypeAhead::default();
                    ui.close();
                }
                ui.ctx().data_mut(|d| d.insert_temp(memo_id, memo.clone()));

                if ui.selectable_label(current.is_none(), "None").clicked() && current.is_some() {
                    picked = Some(None);
                }
                for (id, name) in cats {
                    let selected = current == Some(*id);
                    let highlighted = memo.highlight == Some(*id);
                    let row = ui.selectable_label(selected || highlighted, name);
                    if moved && highlighted {
                        row.scroll_to_me(Some(egui::Align::Center));
                    }
                    if row.clicked() && !selected {
                        picked = Some(Some(*id));
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

    fn cats() -> Vec<(CategoryId, String)> {
        vec![
            (CategoryId(1), "Groceries".to_string()),
            (CategoryId(2), "Gas".to_string()),
            (CategoryId(3), "Dining out".to_string()),
            (CategoryId(4), "Coffee shops".to_string()),
        ]
    }

    #[test]
    /// Typing moves to the first category that starts with the letters, ignoring case.
    fn type_ahead_prefers_the_first_prefix_match() {
        let c = cats();
        assert_eq!(type_ahead_match(&c, "g"), Some(CategoryId(1)));
        assert_eq!(type_ahead_match(&c, "GA"), Some(CategoryId(2)));
        assert_eq!(type_ahead_match(&c, "gro"), Some(CategoryId(1)));
    }

    #[test]
    /// With no prefix match, the first name containing the letters is used; nothing matches blank.
    fn type_ahead_falls_back_to_contains() {
        let c = cats();
        assert_eq!(type_ahead_match(&c, "out"), Some(CategoryId(3)));
        assert_eq!(type_ahead_match(&c, "zzz"), None);
        assert_eq!(type_ahead_match(&c, "  "), None);
    }
}
