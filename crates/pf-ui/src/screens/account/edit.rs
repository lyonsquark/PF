//! Inline edit state and commit helpers for the account table (T15b). The table only records what
//! the user asked for (`Pending`); the screen applies those after drawing, one engine command each,
//! so every edit is one undo step (R-UND-1).

use pf_engine::commands::transactions::{TxField, change_status, edit_transaction};
use pf_model::{CategoryId, TransactionId, TransactionStatus};

use crate::app::UiCtx;
use crate::widgets::contexts::ContextEntry;

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
