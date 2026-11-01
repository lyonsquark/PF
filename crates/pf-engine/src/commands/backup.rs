//! Backup commands (T10c): Markdown export and import (Danger Zone, Settings), the Cmd-S backup
//! file and its folder setting. Rules R-MD-5 and R-MD-7 and BEH-BAK-01; the Swift sources are
//! `Views/SettingsView.swift` (`importMarkdown`) and `Services/BackupManager.swift`.

use std::path::{Path, PathBuf};

use jiff::Timestamp;
use jiff::Zoned;
use pf_core::markdown::{export_markdown as export_md, import_markdown};
use pf_model::Db;

use crate::engine::Engine;
use crate::error::{EngineError, Result};

/// Setting: the folder that Cmd-S writes backups into (not undoable).
const KEY_BACKUP_FOLDER: &str = "backupFolder";

/// Row counts of a Markdown import, for the UI message.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImportCounts {
    /// Accounts imported.
    pub accounts: usize,
    /// Transactions imported.
    pub transactions: usize,
    /// Categories imported.
    pub categories: usize,
    /// Contexts imported.
    pub contexts: usize,
    /// Scenarios imported.
    pub scenarios: usize,
    /// Obligations imported.
    pub obligations: usize,
    /// Budget items imported.
    pub budget_items: usize,
    /// Import mappings imported.
    pub import_mappings: usize,
}

/// Markdown export of the current data, in the engine's time zone (R-MD-1).
pub fn export_markdown(e: &Engine) -> String {
    export_md(e.db(), e.tz())
}

/// Counts of every table in `db`.
fn counts_of(db: &Db) -> ImportCounts {
    ImportCounts {
        accounts: db.accounts.len(),
        transactions: db.transactions.len(),
        categories: db.categories.len(),
        contexts: db.contexts.len(),
        scenarios: db.scenarios.len(),
        obligations: db.obligations.len(),
        budget_items: db.budget_items.len(),
        import_mappings: db.import_mappings.len(),
    }
}

/// The fallback timestamp for obligation rows that lack one: noon of the injected today, in the
/// engine's time zone. Keeps the engine free of clock reads.
fn import_fallback_now(e: &Engine) -> Result<Timestamp> {
    e.today()
        .at(12, 0, 0, 0)
        .to_zoned(e.tz().clone())
        .map(|z| z.timestamp())
        .map_err(|err| EngineError::Validation(err.to_string()))
}

/// R-MD-5. Builds a fresh Db from `text` (R-MD-2, the engine's time zone, progress strings passed
/// through), then replaces all data with it. The old data is wiped only after the new Db exists,
/// and history is cleared and the active scenario reset. Not undoable.
pub fn import_markdown_replacing(
    e: &mut Engine,
    text: &str,
    progress: &mut dyn FnMut(&str),
) -> Result<ImportCounts> {
    let now = import_fallback_now(e)?;
    let imported = import_markdown(text, e.tz(), now, progress);
    let counts = counts_of(&imported.db);
    e.replace_db(imported.db)?;
    Ok(counts)
}

/// Danger Zone "Delete All Data": replaces everything with an empty Db. Not undoable (clears
/// the history, as R-MD-5 does).
pub fn wipe_all_data(e: &mut Engine) -> Result<()> {
    e.replace_db(Db::new())
}

/// The stem of a backup file name, from local wall-clock time (R-MD-7).
fn backup_stem(now: &Zoned) -> String {
    now.strftime("PFBackup_%Y-%m-%d_%H-%M-%S").to_string()
}

/// R-MD-7 file name: `PFBackup_yyyy-MM-dd_HH-mm-ss.md`, e.g. `PFBackup_2026-03-15_09-05-07.md`.
pub fn backup_filename(now: &Zoned) -> String {
    format!("{}.md", backup_stem(now))
}

/// The first name in `folder` that does not exist yet: `{stem}.md`, then `{stem}-2.md`, and so on.
/// Backups never overwrite an existing file.
fn free_backup_name(folder: &Path, stem: &str) -> String {
    let mut name = format!("{stem}.md");
    let mut n = 2;
    while folder.join(&name).exists() {
        name = format!("{stem}-{n}.md");
        n += 1;
    }
    name
}

/// R-MD-8: true when the newest `PFBackup_*.md` in `folder` already holds exactly what a backup would
/// write now. A folder without backups, or a newest file that cannot be read, is not current.
pub fn backup_is_current(e: &Engine, folder: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return false;
    };
    // Names sort by time. The stem is the key because `X-2.md` sorts before `X.md` but is newer.
    let newest = entries
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter_map(|name| {
            let stem = name.strip_suffix(".md")?.to_owned();
            stem.starts_with("PFBackup_").then_some((stem, name))
        })
        .max();
    newest.is_some_and(|(_, name)| {
        std::fs::read(folder.join(name)).is_ok_and(|bytes| bytes == export_markdown(e).as_bytes())
    })
}

/// Cmd-S backup. Writes `export_markdown(e)` into `folder` under a free `PFBackup_*.md` name
/// stamped with the engine's instant in its time zone, through a `.tmp` file that is renamed into
/// place (atomic). Returns the full path, for the toast `Backup saved to: {path}`. An I/O failure
/// is `Store("Failed to create backup: {error}")`.
pub fn perform_backup(e: &Engine, folder: &Path) -> Result<PathBuf> {
    let content = export_markdown(e);
    let now = e.now().to_zoned(e.tz().clone());
    let name = free_backup_name(folder, &backup_stem(&now));
    let target = folder.join(&name);
    let tmp = folder.join(format!("{name}.tmp"));
    let written =
        std::fs::write(&tmp, content.as_bytes()).and_then(|()| std::fs::rename(&tmp, &target));
    if let Err(err) = written {
        // Best effort: the temp file may not exist, which is fine.
        let _ = std::fs::remove_file(&tmp);
        return Err(EngineError::Store(format!(
            "Failed to create backup: {err}"
        )));
    }
    Ok(target)
}

/// The backup folder setting, if one was chosen. `None` means the UI must prompt first.
pub fn backup_folder(e: &Engine) -> Option<PathBuf> {
    e.setting(KEY_BACKUP_FOLDER).map(PathBuf::from)
}

/// Stores the backup folder setting. Not undoable.
pub fn set_backup_folder(e: &mut Engine, folder: &Path) -> Result<()> {
    let value = folder
        .to_str()
        .ok_or_else(|| EngineError::Validation("Backup folder path must be valid UTF-8".into()))?;
    e.set_setting_direct(KEY_BACKUP_FOLDER, value)
}
