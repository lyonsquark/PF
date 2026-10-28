//! The account filter of the Charts screen (T24a, Swift `AccountMultiSelectView`): a button that shows
//! the selection, and a menu with a Select All / Deselect All toggle, Select None (to build a selection
//! from scratch) and one checkbox per account.

use std::collections::BTreeSet;

use egui::Ui;
use pf_core::charts::{select_all_button_label, selection_label};
use pf_model::{AccountId, Db};

/// The account filter. `accounts` is the list of live accounts (id, name), by name.
pub fn account_multiselect(
    ui: &mut Ui,
    db: &Db,
    accounts: &[(AccountId, String)],
    selected: &mut BTreeSet<AccountId>,
) {
    ui.menu_button(selection_label(db, selected), |ui| {
        ui.label(egui::RichText::new("Filter Accounts").strong());
        let all: BTreeSet<AccountId> = accounts.iter().map(|(id, _)| *id).collect();
        if ui.button(select_all_button_label(db, selected)).clicked() {
            if *selected == all {
                selected.clear();
            } else {
                *selected = all;
            }
        }
        if ui.button("Select None").clicked() {
            selected.clear();
        }
        ui.separator();
        for (id, name) in accounts {
            let mut on = selected.contains(id);
            if ui.checkbox(&mut on, name.as_str()).changed() {
                if on {
                    selected.insert(*id);
                } else {
                    selected.remove(id);
                }
            }
        }
    });
}
