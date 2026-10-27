//! The search bar under the action row (T15c, Swift `TransactionListView` search): text in one
//! scope (Payee, Category or Context), an optional date range, the result count and a close button.
//! The filter itself is pf-core's (R-FLT-3, R-FLT-4); this file draws it and opens and closes it.

use egui::{Frame, RichText, Role, TextEdit, Ui, WidgetInfo};
use egui_extras::DatePickerButton;
use jiff::civil::Date;
use pf_core::filter::{SearchScope, SearchState};

/// Opens the bar, or closes it when it is open (R-FLT-6 on close). The first open creates the state
/// with the R-FLT-4 defaults (a year back to today). Opening asks for focus on the text field.
pub fn toggle(search: &mut Option<SearchState>, today: Date, focus: &mut bool) {
    match search {
        Some(s) if s.open => s.close(),
        Some(s) => {
            s.open = true;
            *focus = true;
        }
        None => {
            // `new` starts closed; the first open turns it on.
            let mut opened = SearchState::new(today);
            opened.open = true;
            *search = Some(opened);
            *focus = true;
        }
    }
}

/// Draws the open bar. `count` is the number of result rows, shown when the search filters.
/// The "×" button closes the bar (R-FLT-6).
pub fn bar(ui: &mut Ui, search: &mut SearchState, count: usize, focus: &mut bool) {
    let mut close = false;
    Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .inner_margin(6.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let hint = format!("Search {}", search.scope.label());
                let resp = ui.add(
                    TextEdit::singleline(&mut search.text)
                        .hint_text(hint)
                        .desired_width(250.0),
                );
                resp.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, "Search"));
                if std::mem::take(focus) {
                    resp.request_focus();
                }
                for scope in SearchScope::ALL {
                    ui.selectable_value(&mut search.scope, scope, scope.label());
                }
                ui.checkbox(&mut search.use_date_range, "Date Range");
                if search.use_date_range {
                    ui.add(
                        DatePickerButton::new(&mut search.start)
                            .id_salt("search_start")
                            .show_icon(false),
                    );
                    ui.label(RichText::new("to").weak());
                    ui.add(
                        DatePickerButton::new(&mut search.end)
                            .id_salt("search_end")
                            .show_icon(false),
                    );
                }
                if search.is_active() {
                    // Swift wording kept, "1 results" included (parity).
                    ui.label(RichText::new(format!("{count} results")).small().weak());
                }
                if ui.button("×").on_hover_text("Close search").clicked() {
                    close = true;
                }
            });
        });
    if close {
        search.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::testing::d;

    #[test]
    /// Covers: R-FLT-6. Closing clears the text and turns the date range off; the scope is kept.
    fn r_flt_6_close_clears_text_and_range() {
        let mut s = SearchState::new(d("2026-03-15"));
        s.open = true;
        s.text = "coffee".into();
        s.use_date_range = true;
        s.scope = SearchScope::Category;
        s.close();
        assert!(!s.open);
        assert!(s.text.is_empty());
        assert!(!s.use_date_range);
        assert_eq!(s.scope, SearchScope::Category);
    }

    #[test]
    /// The bar opens with the R-FLT-4 defaults, asks for focus, and closes again on a second toggle.
    fn toggle_opens_then_closes_and_asks_for_focus() {
        let today = d("2026-03-15");
        let mut search = None;
        let mut focus = false;
        toggle(&mut search, today, &mut focus);
        assert!(focus);
        let s = search.as_ref().expect("state created");
        assert!(s.open);
        assert_eq!(s.end, today);
        focus = false;
        toggle(&mut search, today, &mut focus);
        assert!(!search.as_ref().expect("state kept").open);
        assert!(!focus, "closing does not ask for focus");
    }
}
