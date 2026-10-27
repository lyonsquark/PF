//! Modal dialogs. At most one is open at a time (Swift alerts and confirmations).

pub mod account;
pub mod add_transaction;
pub mod alert;
pub mod budget_item;
pub mod category_edit;
pub mod context_edit;
pub mod csv_wizard;
pub mod obligation_add;
pub mod obligation_detail;
pub mod orphaned;
pub mod projection;
pub mod report_drilldown;
pub mod scenario_edit;

use std::path::PathBuf;

use jiff::civil::Date;
use pf_engine::commands;
use pf_model::{AccountId, BudgetItemId, TransactionId};

use crate::app::UiCtx;
use crate::screen::Screen;

/// One modal. Later tasks add variants and their arms in `show_active`.
#[derive(Debug, Clone, PartialEq)]
pub enum Dialog {
    /// A message with an OK button.
    Alert {
        /// Heading of the modal.
        title: String,
        /// Body text.
        message: String,
    },
    /// A question with Cancel and a coloured confirm button.
    Confirm {
        /// Heading of the modal.
        title: String,
        /// Body text.
        message: String,
        /// Label of the confirm button, e.g. "Delete".
        confirm_label: String,
        /// What happens when the user confirms.
        action: ConfirmAction,
    },
    /// Orphaned transactions (live rows with no account) with Delete All (T21, R-TXN-15).
    Orphaned,
    /// New Transaction (T16).
    AddTransaction(Box<add_transaction::TxForm>),
    /// New Account (T16).
    AddAccount(account::AccountForm),
    /// Edit Account, with its starting balance (T16).
    EditAccount(Box<account::EditAccountForm>),
    /// Add or edit a budget item (T17).
    BudgetItemEditor(Box<budget_item::BudgetItemForm>),
    /// Generate Projection (T17).
    Projection(Box<projection::ProjectionForm>),
    /// New Obligation (T18).
    AddObligation(Box<obligation_add::AddObligationForm>),
    /// Edit Obligation (T18).
    ObligationDetail(Box<obligation_detail::ObligationDetailForm>),
    /// New or Edit Scenario (T19).
    ScenarioEdit(Box<scenario_edit::ScenarioForm>),
    /// New or Edit Category (T20).
    CategoryEdit(Box<category_edit::CategoryForm>),
    /// New or Edit Context (T20).
    ContextEdit(Box<context_edit::ContextForm>),
    /// The CSV import wizard (T22).
    CsvWizard(Box<csv_wizard::CsvWizard>),
    /// Transactions behind a report number (T23).
    ReportTransactions {
        /// Heading of the modal.
        title: String,
        /// The transactions.
        ids: Vec<TransactionId>,
    },
    /// Budget items behind a budgeted figure (T23).
    ReportBudgetItems {
        /// Heading of the modal.
        title: String,
        /// The budget items.
        ids: Vec<BudgetItemId>,
        /// First day of the window.
        start: Date,
        /// Last day of the window.
        end: Date,
    },
}

/// What a confirmation does when accepted. Later tasks add variants.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfirmAction {
    /// Soft-delete an account (R-ACC-1).
    DeleteAccount(AccountId),
    /// Wipe all data, then import this Markdown file (R-MD-5). The Settings screen runs the import.
    WipeAndImport(PathBuf),
    /// Hard-delete everything (Settings, Danger Zone). Not undoable.
    DeleteAllData,
}

/// Whether a dialog stays open after a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Keep drawing it next frame.
    KeepOpen,
    /// Remove it.
    Close,
}

/// The open dialog, if any.
#[derive(Debug, Default)]
pub struct Dialogs {
    active: Option<Dialog>,
    /// A confirmed Markdown import waiting for the Settings screen to run it (T21).
    pub pending_import: Option<PathBuf>,
}

impl Dialogs {
    /// Opens `d`, replacing any open dialog.
    pub fn open(&mut self, d: Dialog) {
        self.active = Some(d);
    }

    /// Closes the open dialog, if any.
    pub fn close(&mut self) {
        self.active = None;
    }

    /// Whether a dialog is open.
    pub fn is_open(&self) -> bool {
        self.active.is_some()
    }

    /// Opens an alert.
    pub fn alert(&mut self, title: impl Into<String>, message: impl Into<String>) {
        self.open(Dialog::Alert {
            title: title.into(),
            message: message.into(),
        });
    }
}

/// Draws the active dialog (if any) as a modal and runs its outcome.
pub fn show_active(cx: &mut UiCtx, ui: &mut egui::Ui) {
    let Some(mut dialog) = cx.dialogs.active.take() else {
        return;
    };
    let outcome = match &mut dialog {
        Dialog::Alert { title, message } => alert::show_alert(ui, title, message),
        Dialog::Confirm {
            title,
            message,
            confirm_label,
            action,
        } => match alert::show_confirm(ui, title, message, confirm_label) {
            alert::Reply::Pending => Outcome::KeepOpen,
            alert::Reply::Cancel => Outcome::Close,
            alert::Reply::Accept => {
                run_confirm(cx, action);
                // The action may leave work for the next frame (a pending import).
                ui.ctx().request_repaint();
                Outcome::Close
            }
        },
        Dialog::Orphaned => orphaned::show(cx, ui),
        Dialog::AddTransaction(form) => add_transaction::show(cx, ui, form),
        Dialog::AddAccount(form) => account::show_add(cx, ui, form),
        Dialog::EditAccount(form) => account::show_edit(cx, ui, form),
        Dialog::BudgetItemEditor(form) => budget_item::show(cx, ui, form),
        Dialog::Projection(form) => projection::show(cx, ui, form),
        Dialog::AddObligation(form) => obligation_add::show(cx, ui, form),
        Dialog::ObligationDetail(form) => obligation_detail::show(cx, ui, form),
        Dialog::ScenarioEdit(form) => scenario_edit::show(cx, ui, form),
        Dialog::CategoryEdit(form) => category_edit::show(cx, ui, form),
        Dialog::ContextEdit(form) => context_edit::show(cx, ui, form),
        Dialog::CsvWizard(w) => csv_wizard::show(cx, ui, w),
        Dialog::ReportTransactions { title, ids } => {
            report_drilldown::show_transactions(cx, ui, title.as_str(), ids.as_slice())
        }
        Dialog::ReportBudgetItems {
            title,
            ids,
            start,
            end,
        } => report_drilldown::show_budget_items(
            cx,
            ui,
            title.as_str(),
            ids.as_slice(),
            *start,
            *end,
        ),
    };
    // Put it back unless the action already opened a different dialog.
    if outcome == Outcome::KeepOpen && cx.dialogs.active.is_none() {
        cx.dialogs.active = Some(dialog);
    }
}

/// Runs an accepted confirmation.
fn run_confirm(cx: &mut UiCtx, action: &ConfirmAction) {
    match action {
        ConfirmAction::DeleteAccount(id) => {
            match commands::accounts::delete_account(cx.engine, *id) {
                Ok(()) => {
                    if *cx.screen == Screen::Account(*id) {
                        *cx.screen = Screen::Dashboard;
                    }
                }
                Err(e) => cx.dialogs.alert("Delete Account", e.to_string()),
            }
        }
        ConfirmAction::WipeAndImport(path) => {
            // The Settings screen draws the progress overlay and runs the import next frame.
            cx.dialogs.pending_import = Some(path.clone());
        }
        ConfirmAction::DeleteAllData => match commands::backup::wipe_all_data(cx.engine) {
            Ok(()) => cx
                .dialogs
                .alert("Data Management", "All data has been deleted."),
            Err(e) => cx.dialogs.alert("Data Management", e.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Covers: dialog slot (T13 spec). Opening replaces; close empties; alert helper opens an alert.
    fn dialogs_hold_one_at_a_time() {
        let mut d = Dialogs::default();
        assert!(!d.is_open());
        d.alert("A", "first");
        d.alert("B", "second");
        assert!(d.is_open());
        assert_eq!(
            d.active,
            Some(Dialog::Alert {
                title: "B".into(),
                message: "second".into()
            })
        );
        d.close();
        assert!(!d.is_open());
    }
}
