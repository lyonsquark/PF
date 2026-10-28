//! The sidebar: screen list and account list with its grouping (Swift `SidebarView`, R-ACC-4).

use std::collections::BTreeMap;

use egui::{CollapsingHeader, ComboBox, Response, RichText, Role, ScrollArea, Ui, WidgetInfo};
use pf_model::AccountType;

use crate::app::UiCtx;
use crate::caches::AccountRow;
use crate::dialogs::{ConfirmAction, Dialog};
use crate::screen::Screen;

/// How the account list is grouped (R-ACC-4). UI only, not persisted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Grouping {
    /// Flat list sorted by name.
    #[default]
    Az,
    /// One collapsing header per institution.
    Institution,
    /// One collapsing header per account type.
    Type,
}

impl Grouping {
    /// Every grouping, in menu order.
    pub const ALL: [Grouping; 3] = [Grouping::Az, Grouping::Institution, Grouping::Type];

    /// Label in the grouping menu.
    pub fn label(self) -> &'static str {
        match self {
            Grouping::Az => "A-Z",
            Grouping::Institution => "Institution",
            Grouping::Type => "Type",
        }
    }
}

/// Sidebar state: the grouping choice.
#[derive(Debug, Default)]
pub struct State {
    /// The chosen grouping.
    pub grouping: Grouping,
}

/// Draws the sidebar: Dashboard, the accounts, then the Planning and Settings sections.
pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut Ui) {
    ScrollArea::vertical().show(ui, |ui| {
        nav_entry(cx, ui, Screen::Dashboard, "Dashboard");
        ui.add_space(6.0);
        accounts_section(cx, st, ui);
        ui.add_space(6.0);
        section_header(ui, "PLANNING");
        nav_entry(cx, ui, Screen::Budget, "Budget");
        nav_entry(cx, ui, Screen::Obligations, "Obligations");
        nav_entry(cx, ui, Screen::Scenarios, "Scenarios");
        nav_entry(cx, ui, Screen::Charts, "Charts");
        nav_entry(cx, ui, Screen::Reports, "Reports");
        ui.add_space(6.0);
        section_header(ui, "SETTINGS");
        nav_entry(cx, ui, Screen::Categories, "Categories");
        nav_entry(cx, ui, Screen::Contexts, "Contexts");
        nav_entry(cx, ui, Screen::Settings, "Settings");
    });
}

/// A small grey section title.
fn section_header(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(title).small().strong());
}

/// One selectable row that navigates to `target`.
fn nav_entry(cx: &mut UiCtx, ui: &mut Ui, target: Screen, label: &str) {
    let selected = *cx.screen == target;
    if ui.selectable_label(selected, label).clicked() {
        *cx.screen = target;
    }
}

/// The "ACCOUNTS" header with its grouping menu, then the account rows.
fn accounts_section(cx: &mut UiCtx, st: &mut State, ui: &mut Ui) {
    accounts_header(cx, st, ui);
    let rows = cx.caches.lookups(&*cx.engine).account_rows.clone();
    match st.grouping {
        Grouping::Az => {
            for row in &rows {
                account_row(cx, ui, row);
            }
        }
        Grouping::Institution => {
            let mut groups: BTreeMap<String, Vec<&AccountRow>> = BTreeMap::new();
            for row in &rows {
                let key = if row.institution.is_empty() {
                    "Other".to_string()
                } else {
                    row.institution.clone()
                };
                groups.entry(key).or_default().push(row);
            }
            for (institution, members) in groups {
                CollapsingHeader::new(institution)
                    .default_open(true)
                    .show(ui, |ui| {
                        for row in members {
                            account_row(cx, ui, row);
                        }
                    });
            }
        }
        Grouping::Type => {
            for account_type in AccountType::ALL {
                let members: Vec<&AccountRow> = rows
                    .iter()
                    .filter(|r| r.account_type == account_type)
                    .collect();
                if members.is_empty() {
                    continue;
                }
                CollapsingHeader::new(account_type.label())
                    .default_open(true)
                    .show(ui, |ui| {
                        for row in members {
                            account_row(cx, ui, row);
                        }
                    });
            }
        }
    }
}

/// The "ACCOUNTS" title and "+" to add an account (T16), with the grouping menu on the line below.
fn accounts_header(cx: &mut UiCtx, st: &mut State, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("ACCOUNTS").small().strong());
        let add = ui.small_button("+").on_hover_text("Add Account");
        add.widget_info(|| WidgetInfo::labeled(Role::Button, true, "Add Account"));
        if add.clicked() {
            cx.dialogs.open(Dialog::AddAccount(
                crate::dialogs::account::AccountForm::default(),
            ));
        }
    });
    ui.horizontal(|ui| {
        let combo = ComboBox::from_id_salt("sidebar_grouping")
            .selected_text(st.grouping.label())
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                for g in Grouping::ALL {
                    ui.selectable_value(&mut st.grouping, g, g.label());
                }
            });
        combo
            .response
            .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, "Grouping"));
    });
}

/// One account row: selects the account screen; right-click offers Delete (R-ACC-1).
fn account_row(cx: &mut UiCtx, ui: &mut Ui, row: &AccountRow) -> Response {
    let text = if row.is_external {
        // The globe glyph is not in egui's bundled fonts, so external accounts get a text suffix.
        format!("{} (ext)", row.name)
    } else {
        row.name.clone()
    };
    let target = Screen::Account(row.id);
    let selected = *cx.screen == target;
    let response = ui.selectable_label(selected, text);
    if response.clicked() {
        *cx.screen = target;
    }
    response.context_menu(|ui| {
        if ui.button("Edit Account").clicked() {
            let today = cx.engine.today();
            if let Some(form) = crate::dialogs::account::edit_form(cx.engine.db(), row.id, today) {
                cx.dialogs.open(Dialog::EditAccount(Box::new(form)));
            }
            ui.close();
        }
        ui.separator();
        if ui.button("Delete").clicked() {
            cx.dialogs.open(Dialog::Confirm {
                title: "Delete Account?".to_string(),
                message: "Are you sure? This will delete the account and all its transactions."
                    .to_string(),
                confirm_label: "Delete".to_string(),
                action: ConfirmAction::DeleteAccount(row.id),
            });
            ui.close();
        }
    });
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Covers: R-ACC-4. Grouping menu order and labels.
    fn grouping_labels_and_default() {
        assert_eq!(Grouping::default(), Grouping::Az);
        let labels: Vec<&str> = Grouping::ALL.iter().map(|g| g.label()).collect();
        assert_eq!(labels, vec!["A-Z", "Institution", "Type"]);
    }
}
