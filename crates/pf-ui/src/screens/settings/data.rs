//! The actions behind the Settings buttons: Markdown export and import (R-MD-5), the Monarch CSV
//! entry, the orphan count, and Delete All Data. Each file chooser goes through `cx.files`.

use std::path::Path;

use pf_engine::commands::{backup, transactions};

use crate::app::UiCtx;
use crate::dialogs::{ConfirmAction, Dialog, csv_wizard};

/// Alert title used by every Settings alert except where the spec gives another.
const ALERT_TITLE: &str = "Data Management";

/// Builds the Markdown export and saves it under `pf_yyyyMMdd_HHmmss.md`, stamped with the
/// engine's injected instant in its zone. A cancelled save does nothing (T21).
pub fn export_markdown(cx: &mut UiCtx) {
    let content = backup::export_markdown(cx.engine);
    let now = cx.engine.now().to_zoned(cx.engine.tz().clone());
    let name = now.strftime("pf_%Y%m%d_%H%M%S.md").to_string();
    let Some(path) = cx.files.save_file("Export to Markdown", &name, &["md"]) else {
        return;
    };
    if let Err(e) = std::fs::write(&path, content.as_bytes()) {
        cx.dialogs.alert(ALERT_TITLE, format!("Export failed: {e}"));
    }
}

/// Asks for a Markdown file and, on a choice, opens the "Wipe and Import?" confirmation (R-MD-5).
pub fn choose_import_file(cx: &mut UiCtx) {
    let Some(path) = cx.files.pick_file("Import from Markdown", &["md", "txt"]) else {
        return;
    };
    cx.dialogs.open(Dialog::Confirm {
        title: "Wipe and Import?".into(),
        message: "This will completely wipe your existing data and replace it with the imported Markdown data. Are you sure?".into(),
        confirm_label: "Import".into(),
        action: ConfirmAction::WipeAndImport(path),
    });
}

/// Reads `path` and replaces all data with it (R-MD-5). Wipes, imports and clears undo in one
/// engine call. Shows the success alert, or the read or import error.
pub fn run_import(cx: &mut UiCtx, path: &Path) {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) => {
            cx.dialogs
                .alert(ALERT_TITLE, format!("File Read Error: {e}"));
            return;
        }
    };
    let mut progress = |_status: &str| {};
    match backup::import_markdown_replacing(cx.engine, &text, &mut progress) {
        Ok(_) => cx
            .dialogs
            .alert(ALERT_TITLE, "Data successfully imported from Markdown."),
        Err(e) => cx
            .dialogs
            .alert(ALERT_TITLE, format!("Internal Import Error: {e}")),
    }
}

/// Asks for a Monarch CSV and opens the CSV wizard in Monarch mode with no target account (T22).
pub fn import_monarch_csv(cx: &mut UiCtx) {
    let Some(path) = cx
        .files
        .pick_file("Import from Monarch CSV", &["csv", "txt"])
    else {
        return;
    };
    match std::fs::read_to_string(&path) {
        Ok(content) => csv_wizard::open(cx, &content, true, None),
        Err(e) => cx
            .dialogs
            .alert(ALERT_TITLE, format!("Failed to load CSV: {e}")),
    }
}

/// R-TXN-15: the number of live transactions with no account, shown on the Find button.
pub fn orphan_count(cx: &UiCtx) -> usize {
    transactions::orphaned_transactions(cx.engine.db()).len()
}

/// Opens the "Delete All Data?" confirmation (Danger Zone).
pub fn confirm_delete_all(cx: &mut UiCtx) {
    cx.dialogs.open(Dialog::Confirm {
        title: "Delete All Data?".into(),
        message: "This action cannot be undone. All accounts, transactions, and scenarios will be permanently deleted.".into(),
        confirm_label: "Delete".into(),
        action: ConfirmAction::DeleteAllData,
    });
}
