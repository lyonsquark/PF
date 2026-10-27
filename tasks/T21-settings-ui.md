# T21: Settings screen (backup, export/import, Monarch CSV entry, danger zone, orphaned transactions)

**Phase:** 3 · **Deps:** T13, T10c · **Crate(s):** `pf-ui` · **Size:** medium (≈400 lines + 200 lines tests)

## Goal

Port the Settings screen. It covers: choosing the automatic backup folder and "Backup Now"; exporting and
importing the Markdown backup (import wipes existing data); starting a Monarch CSV import; listing and
deleting orphaned transactions; and deleting all data. Every file chooser goes through the `FileDialogs` trait,
so the screen is fully testable with the fake.

> **Shell contract (T13).** The screen is `screens/settings.rs` with `#[derive(Default)] pub struct State` and
> `pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut egui::Ui)` (state in `ScreenStates.settings`). File choosers use
> `cx.files` (`FileDialogs::{pick_file(title, exts), pick_folder(title), save_file(title, default_name, exts), open_path(path)}`;
> tests use `FakeDialogs::{push_answer, calls}` via `PfApp::new_for_test_with_files`). Confirmations are
> `Dialog::Confirm { title, message, confirm_label, action: ConfirmAction }`. This task adds `ConfirmAction::WipeAndImport(PathBuf)`
> and `ConfirmAction::DeleteAllData`, with their arms in `run_confirm`, and `Dialog::Orphaned`. Alerts use `cx.dialogs.alert(title, msg)`.
> Toasts use `cx.toasts.show_for(text, ToastKind::Success, 4.0)`. **T13 already owns backup** (`backup_now` in `app.rs`, used by ⌘S and
> File ▸ Backup Data). This task only makes it `pub(crate)` and calls it from the "Backup Now" button.
> Commands: `commands::backup::{export_markdown(e) -> String, import_markdown_replacing(e, text, &mut progress) -> ImportCounts,
> wipe_all_data(e), backup_folder(e), set_backup_folder(e, path)}` (T10c), and
> `commands::transactions::{orphaned_transactions(db) -> Vec<TransactionId>, delete_orphaned(e) -> usize}` (T10a).
> Fallback: if a name here differs from the code, use the actual name in `pf-ui`/`pf-engine`; do not add engine behaviour here.

## Read first

- Docs: `docs/markdown-format.md` (import/export), `docs/data-model.md` (settings keys `backupFolder`), `docs/testing.md` (L5)
- Swift: `../PFApp/Sources/PFApp/Views/SettingsView.swift` (whole file), `../PFApp/Sources/PFApp/Services/BackupManager.swift`,
  `../PFApp/Sources/PFApp/Views/OrphanedTransactionsView.swift`
- Rules: R-MD-5, R-MD-7, R-TXN-15
- Engine: see the shell contract above (T10a, T10c); `tasks/T13-ui-shell.md` → "Backup", "Files", "Dialogs"

## Deliverables

```rust
// crates/pf-ui/src/screens/settings.rs
#[derive(Default)]
pub struct State {
    importing: Option<(PathBuf, String)>,   // pending import: path + progress text ("Preparing import...")
    started: bool,                          // set after the overlay has been drawn for one frame
}
pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut egui::Ui);

// crates/pf-ui/src/dialogs/orphaned.rs
pub fn show(cx: &mut UiCtx, ui: &mut egui::Ui) -> Outcome;      // Dialog::Orphaned

// crates/pf-ui/src/dialogs/mod.rs (additive)
//   Dialog::Orphaned
//   ConfirmAction::WipeAndImport(PathBuf), ConfirmAction::DeleteAllData  (+ arms in run_confirm)
// crates/pf-ui/src/app.rs (additive): `fn backup_now` becomes `pub(crate) fn backup_now(cx: &mut UiCtx)`
```

Accepting `WipeAndImport(path)` sets `ScreenStates.settings.importing = Some((path, "Preparing import...".into()))`. `run_confirm`
cannot reach the screen state, so instead it opens nothing and stores the request in a field T13's `UiCtx` can carry. Simplest:
add `pub pending_import: Option<PathBuf>` to `Dialogs` (additive), set it in `run_confirm`, and have `settings::show` move it into
`st.importing`.

## Screen layout

```
┌ Settings ──────────────────────────────────────────────────────────┐
│ Automatic Backups                                                  │
│  [📁 Automatic Backup Folder                     Configured ]      │  (or "Not Set" in red)
│  [⟳ Backup Now (Cmd-S)]                                             │  (only when a folder is set; blue)
│                                                                    │
│ Manual Export & Import                                             │
│  [⬆ Export to Markdown]                                  (blue)    │
│  [⬇ Import from Markdown (Wipes current data)]           (orange)  │
│                                                                    │
│ Data Ingest                                                        │
│  [📄 Import from Monarch CSV]                            (green)   │
│                                                                    │
│ Danger Zone                                                        │
│  [⚠ Find Orphaned Transactions (3)]   (orange if >0, disabled if 0)│
│  [🗑 Delete All Data]                                     (red)     │
└────────────────────────────────────────────────────────────────────┘
```

Use plain buttons with these exact labels (the emoji icons are optional decoration; tests match the text after the icon,
so put each icon in a separate label or leave it out). Section headers use `ui.heading` or a strong label.

## Behaviour

| Control | Action |
|---|---|
| Automatic Backup Folder | `files.pick_folder("Set Backup Folder")`. On Some ⇒ set the setting `backupFolder` (absolute path). The right side shows "Configured" (weak) when set, else "Not Set" (red). |
| Backup Now (Cmd-S) | `perform_backup` (below). |
| Export to Markdown | Build the export via the engine, then `files.save_file("pf_{yyyyMMdd_HHmmss}.md", "md")`; write the file. On error: alert **"Data Management"** / `Export failed: {err}`. Cancel ⇒ nothing. |
| Import from Markdown (Wipes current data) | `files.open_file(&["md", "txt"])`. On Some ⇒ confirm dialog **"Wipe and Import?"** with message `This will completely wipe your existing data and replace it with the imported Markdown data. Are you sure?`, buttons Cancel / **Import**. On Import: read the file (error ⇒ alert `File Read Error: {err}`), show a blocking overlay (spinner + progress text, starting "Preparing import..."), call the engine's import-replacing command (it wipes, imports and clears undo, R-MD-5), then alert `Data successfully imported from Markdown.` On error ⇒ alert `Internal Import Error: {err}`. |
| Import from Monarch CSV | `files.open_file(&["csv", "txt"])`, read it (error ⇒ alert `Failed to load CSV: {err}`), then open the CSV wizard in **Monarch mode** with no target account: `dialogs::csv_wizard::open(app, content, is_monarch = true, target = None)` (T22). If T22 is not done yet, create a stub `open` that shows the toast "CSV import is not available yet"; T22 replaces the stub. |
| Find Orphaned Transactions (N) | N = live transactions with no account. Disabled when N = 0. Opens the Orphaned dialog. |
| Delete All Data | Confirm **"Delete All Data?"** with message `This action cannot be undone. All accounts, transactions, and scenarios will be permanently deleted.`, buttons Cancel / **Delete**. On Delete ⇒ the wipe command (hard delete everything, clear undo), then alert `All data has been deleted.` |

All alerts use the title "Data Management" except where another title is given above. Alerts have one OK button.

### `perform_backup` (R-MD-7)

1. If `backupFolder` is unset ⇒ call `pick_folder`. If the user cancels ⇒ stop; otherwise save the setting and continue.
2. If the folder no longer exists ⇒ alert **"Backup Error"** / `Backup folder access lost. Please choose the folder again.`,
   clear the setting, stop.
3. Call the engine backup command, which writes `PFBackup_yyyy-MM-dd_HH-mm-ss.md` in that folder.
4. On success ⇒ toast `Backup saved to: {full path}` for 4 seconds. On error ⇒ alert **"Backup Error"** /
   `Failed to create backup: {err}`.

### Import overlay

While importing, draw a centred `egui::Window`/`Area` over the screen holding `ui.spinner()` and the progress text, and
disable the rest of the screen (`add_enabled_ui(false, …)`). Implementation: on confirm, set
`state = Importing { path, progress: "Preparing import..." }` and call `ctx.request_repaint()`. On the **next** frame,
run the import (it takes < 3 s per the perf budget), then clear the state. This makes the overlay visible for at least one frame.

### Orphaned Transactions dialog

```
┌ Orphaned Transactions ─────────────────────────────── [Cancel] [Delete All] ┐
│ 3 transaction(s) with no account                                             │
│ Date        Payee        Amount     Category   Memo   Status    Scenario     │
│ 2026-02-01  Coffee      -$4.50(red) None              Cleared   Reality(grey)│
└──────────────────────────────────────────────────────────────────────────────┘
```

* If there are none: centred "No Orphaned Transactions" plus "All transactions have a valid account."
* Table (`TableBuilder`, virtualised), sorted date desc. Amount red if < 0, green otherwise. Category = full name or "None".
  Status = display label. Scenario = name in blue, or "Reality" in grey.
* **Delete All** (disabled when empty) ⇒ the delete-orphaned command (R-TXN-15: soft delete, one undo step), then close.
  **Cancel** closes.

## Steps

1. Extend `FileDialogs` and its fake if needed (in T13's module; additive only).
2. Write the tests.
3. Implement the screen, `perform_backup`, alerts/confirms, the import overlay, and the orphaned dialog.
4. Make sure T13's ⌘S handler calls `settings::perform_backup` (change a single call site if T13 stubbed it).
5. Run Done-when.

## Tests (required)

`crates/pf-ui/tests/ui_settings.rs`. Use `tempfile::tempdir()` for every path, and queue answers in the fake `FileDialogs`.

| Test name | Covers | What it checks |
|---|---|---|
| `ui_set_01_export_then_import_round_trip` | UI-SET-01, R-MD-5 | Export to a temp file via the fake save dialog; add a transaction; import that file (confirm Import) ⇒ the added transaction is gone, the original data is present, undo is unavailable, and the success alert is shown |
| `ui_set_backup_now_writes_file_and_toasts` | R-MD-7, BEH-BAK-01 | With a folder set, clicking "Backup Now (Cmd-S)" writes one `PFBackup_*.md`; the toast text starts with "Backup saved to:" |
| `ui_set_backup_prompts_for_folder_when_unset` | R-MD-7 | With no folder set, ⌘S calls `pick_folder`; a cancelled pick writes nothing |
| `ui_set_backup_missing_folder_alerts` | R-MD-7 | Setting points to a deleted dir ⇒ the "Backup Error" alert appears and the setting is cleared |
| `ui_set_folder_status_label` | — | "Not Set" before choosing, "Configured" after |
| `ui_set_import_cancel_keeps_data` | — | Choosing a file then pressing Cancel on "Wipe and Import?" changes nothing |
| `ui_set_delete_all_data` | — | Confirm Delete ⇒ there are no accounts or transactions; alert "All data has been deleted." |
| `ui_set_orphaned_button_disabled_when_zero` | R-TXN-15 | With no orphans the button is disabled; the label shows "(0)" |
| `ui_set_orphaned_delete_all_undoable` | R-TXN-15 | With 2 orphans: open, Delete All ⇒ 0 orphans; Cmd-Z ⇒ 2 again |
| `ui_set_monarch_opens_wizard` | — | After choosing a CSV via the fake, the CSV wizard dialog (or the stub toast) is opened in Monarch mode |

## Pitfalls

- Never call `rfd` directly; always use `app.files`. The tests depend on this.
- Release the engine lock before calling a file dialog (dialogs block).
- Use the local time zone for the file-name timestamps (`jiff::Zoned::now()`); format with `strftime("%Y%m%d_%H%M%S")`
  for export and `"%Y-%m-%d_%H-%M-%S"` for backups.
- Swift stored a macOS bookmark; we store a plain path. There is no "stale bookmark" case beyond "folder missing".

## Done when

```bash
cargo test -p pf-ui --test ui_settings
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules
```
Manual: set a folder, press ⌘S, check the file; export, then import a fixture.

## Out of scope

- The Markdown format itself (T05) and the engine import/backup commands (T10c).
- The CSV wizard (T22).
