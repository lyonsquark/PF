//! The row context menu (T15c, Swift `.contextMenu(forSelectionType:)`): Add to Budget, Duplicate
//! and Delete. The menu only reports the chosen action; the screen runs it after the table is drawn.

use egui::Ui;
use pf_model::TransactionId;

use super::rows::TxRow;

/// One menu choice, with the rows it applies to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuAction {
    /// "Add to Budget": the first selected row, in row order (R-TXN-11).
    AddToBudget(TransactionId),
    /// "Duplicate": every selected row (R-TXN-6).
    Duplicate(Vec<TransactionId>),
    /// "Delete": the unlocked selected rows only (R-TXN-7).
    Delete(Vec<TransactionId>),
}

/// The menu items for the selected rows, given in row order. "Delete" is shown only when at least
/// one of them is unlocked (R-TXN-1). Returns the chosen action, if any.
pub fn items(ui: &mut Ui, selected: &[&TxRow]) -> Option<MenuAction> {
    let mut chosen = None;
    let ids: Vec<TransactionId> = selected.iter().map(|r| r.id).collect();
    if let Some(first) = ids.first()
        && ui.button("Add to Budget").clicked()
    {
        chosen = Some(MenuAction::AddToBudget(*first));
        ui.close();
    }
    if !ids.is_empty() && ui.button("Duplicate").clicked() {
        chosen = Some(MenuAction::Duplicate(ids.clone()));
        ui.close();
    }
    let unlocked: Vec<TransactionId> = selected
        .iter()
        .filter(|r| !r.locked)
        .map(|r| r.id)
        .collect();
    if !unlocked.is_empty() && ui.button("Delete").clicked() {
        chosen = Some(MenuAction::Delete(unlocked));
        ui.close();
    }
    chosen
}
