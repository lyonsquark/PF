//! Preference commands: UI preferences stored in the `settings` table. Not undoable (like
//! `backupFolder` and `selectedTimePeriod`), so they never enter the undo stack.

use crate::engine::Engine;
use crate::error::Result;

/// Setting: the key of the UI font chosen in Settings (missing means the default, SF Pro).
pub const KEY_UI_FONT: &str = "uiFont";

/// The stored `uiFont` key, if one was chosen. The UI maps missing or unknown keys to the default.
pub fn ui_font(e: &Engine) -> Option<&str> {
    e.setting(KEY_UI_FONT)
}

/// Stores the `uiFont` key. Not undoable.
pub fn set_ui_font(e: &mut Engine, key: &str) -> Result<()> {
    e.set_setting_direct(KEY_UI_FONT, key)
}

/// Setting: `false` turns off the backup made when the app exits (missing means on, R-MD-8).
pub const KEY_BACKUP_ON_EXIT: &str = "backupOnExit";

/// R-MD-8: whether the app backs up when it exits. On unless turned off.
pub fn backup_on_exit(e: &Engine) -> bool {
    e.setting(KEY_BACKUP_ON_EXIT) != Some("false")
}

/// Stores whether the app backs up when it exits. Not undoable.
pub fn set_backup_on_exit(e: &mut Engine, on: bool) -> Result<()> {
    e.set_setting_direct(KEY_BACKUP_ON_EXIT, if on { "true" } else { "false" })
}
