//! Modal dialogs. At most one is open at a time (Swift alerts and confirmations).

pub mod alert;

use pf_engine::commands;
use pf_model::AccountId;

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
}

/// What a confirmation does when accepted. Later tasks add variants.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfirmAction {
    /// Soft-delete an account (R-ACC-1).
    DeleteAccount(AccountId),
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
    let Some(dialog) = cx.dialogs.active.take() else {
        return;
    };
    let outcome = match &dialog {
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
                Outcome::Close
            }
        },
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
