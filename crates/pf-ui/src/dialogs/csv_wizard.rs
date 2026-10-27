//! Entry point of the CSV import wizard. T21 calls `open` for Monarch CSV. This is a stub until
//! T22 builds the wizard, and T22 replaces the body.

use pf_model::AccountId;

use crate::app::UiCtx;

/// Opens the CSV wizard for `content`. `is_monarch` selects Monarch mode; `target` is the account
/// to import into, if any. Stub: shows a toast until the wizard exists (T22).
pub fn open(cx: &mut UiCtx, content: &str, is_monarch: bool, target: Option<AccountId>) {
    log::debug!(
        "csv wizard stub: {} bytes, monarch={is_monarch}, target={target:?}",
        content.len()
    );
    cx.toasts.info("CSV import is not available yet");
}
