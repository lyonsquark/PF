//! Inline edit state and commit helpers for the account table (T15b). The table only records what
//! the user asked for (`Pending`); the screen applies those after drawing, one engine command each,
//! so every edit is one undo step (R-UND-1).

use std::path::Path;

use pf_engine::commands::budget::add_to_budget;
use pf_engine::commands::transactions::{
    TxField, change_status, delete_transactions, duplicate_transactions, edit_transaction,
    is_safe_attachment,
};
use pf_model::{CategoryId, TransactionId, TransactionStatus};

use super::menu::MenuAction;
use crate::app::UiCtx;
use crate::dialogs::Dialog;
use crate::widgets::contexts::ContextEntry;

/// Title of the attach file dialog (T15c).
const ATTACH_TITLE: &str = "Select a file to attach to this transaction";
/// Body of the alert shown for a file type that may not be opened (R-TXN-14).
const UNSAFE_MESSAGE: &str = "This attachment cannot be opened because its file type is not allowed. Only PDFs, images, and text files may be opened.";

/// The text columns that edit in a text field. The amount has its own buffer (`Edits::amount`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditCol {
    /// Payee text.
    Payee,
    /// Memo text.
    Memo,
}

/// The one text cell being edited. The buffer must outlive a frame, so it lives in the screen state.
#[derive(Debug, Clone, PartialEq)]
pub struct CellEdit {
    /// The row being edited.
    pub id: TransactionId,
    /// The column being edited.
    pub col: EditCol,
    /// The text typed so far.
    pub text: String,
}

/// A commit requested while drawing the table, applied after the table closure.
#[derive(Debug, Clone, PartialEq)]
pub enum Pending {
    /// An inline field edit (`edit_transaction`).
    Field(TransactionId, TxField),
    /// A status change (`change_status`), which may promote a scenario row.
    Status(TransactionId, TransactionStatus),
    /// Ask for a file and attach it to the row (T15c).
    Attach(TransactionId),
    /// Open an attachment with the OS, if its type is allowed (R-TXN-14).
    OpenAttachment(String),
    /// A context-menu action (T15c).
    Menu(MenuAction),
}

/// What the table draws edits into: the buffers it reads and writes, the lookup lists for the
/// pickers, and the commits it asks for.
pub struct Edits<'a> {
    /// The text cell being edited, if any.
    pub cell: &'a mut Option<CellEdit>,
    /// The amount cell being edited (row id and its buffer), if any.
    pub amount: &'a mut Option<(TransactionId, String)>,
    /// Category picker list, sorted (R-CAT-4).
    pub categories: &'a [(CategoryId, String)],
    /// Non-archived contexts for the "⊕" menu.
    pub contexts: &'a [ContextEntry],
    /// Whether a search filters the rows: the running column then shows the search balance (R-BAL-8).
    pub searching: bool,
    /// The row whose tag "+" menu is open; its button stays shown while the menu is.
    pub tag_open: &'a mut Option<TransactionId>,
    /// Commits requested this frame, in draw order.
    pub pending: Vec<Pending>,
}

/// Applies one field edit. An engine error is a toast; an unchanged value is no step (engine).
pub fn commit(cx: &mut UiCtx, id: TransactionId, field: TxField) {
    if let Err(e) = edit_transaction(cx.engine, id, field) {
        cx.toasts.info(e.to_string());
    }
}

/// Applies one status change. Engine errors (R-TXN-2..4) are toasts with a "Warning: " prefix (plain text; the ⚠️ glyph is missing from the default fonts).
pub fn commit_status(cx: &mut UiCtx, id: TransactionId, status: TransactionStatus) {
    if let Err(e) = change_status(cx.engine, id, status) {
        cx.toasts.info(format!("Warning: {e}"));
    }
}

/// Asks for a file and attaches it to the row (R-TXN-14). Cancelling the dialog changes nothing.
pub fn attach(cx: &mut UiCtx, id: TransactionId) {
    if let Some(path) = cx.files.pick_file(ATTACH_TITLE, &[]) {
        commit(
            cx,
            id,
            TxField::Attachment(Some(path.display().to_string())),
        );
    }
}

/// Opens an attachment (R-TXN-14). A type that may not be opened gets an alert instead, and the
/// file is never handed to the OS. An error from the OS is a toast.
pub fn open_attachment(cx: &mut UiCtx, path: &str) {
    if !is_safe_attachment(path) {
        cx.dialogs.open(Dialog::Alert {
            title: "Unsafe Attachment".to_string(),
            message: UNSAFE_MESSAGE.to_string(),
        });
        return;
    }
    if let Err(e) = cx.files.open_path(Path::new(path)) {
        cx.toasts.info(e);
    }
}

/// "Add to Budget" (R-TXN-11). The engine's text becomes the success toast.
pub fn add_to_budget_toast(cx: &mut UiCtx, id: TransactionId) {
    match add_to_budget(cx.engine, id) {
        Ok(text) => cx.toasts.success(text),
        Err(e) => cx.toasts.info(e.to_string()),
    }
}

/// Duplicates the rows (R-TXN-6). Returns the new ids so the caller can select them.
pub fn duplicate(cx: &mut UiCtx, ids: &[TransactionId]) -> Option<Vec<TransactionId>> {
    match duplicate_transactions(cx.engine, ids) {
        Ok(new) => Some(new),
        Err(e) => {
            cx.toasts.info(e.to_string());
            None
        }
    }
}

/// Deletes the rows (R-TXN-7). An error is a toast.
pub fn delete(cx: &mut UiCtx, ids: &[TransactionId]) {
    if let Err(e) = delete_transactions(cx.engine, ids) {
        cx.toasts.info(e.to_string());
    }
}
