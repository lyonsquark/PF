//! Settings screen: UI font, automatic backups (R-MD-7), Markdown export and import (R-MD-5),
//! Monarch CSV entry, and the Danger Zone (orphaned transactions R-TXN-15, Delete All Data).
//! Layout follows Swift `SettingsView`; file choosers go through `cx.files`.

mod data;

use std::path::PathBuf;

use egui::{Button, RichText, Ui};
use pf_engine::commands::{backup, settings};

use crate::app::{self, UiCtx};
use crate::fonts;
use crate::widgets::colors;

/// Per-screen UI state for the Markdown import (R-MD-5).
#[derive(Default)]
pub struct State {
    /// A confirmed import waiting to run: the file and the overlay's progress text.
    importing: Option<(PathBuf, String)>,
    /// Set once the overlay has been drawn for one frame; the import runs on the next frame.
    started: bool,
}

/// Draws the screen into the central panel.
pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut Ui) {
    // A confirmed "Wipe and Import?" arrives here from the dialog layer (T21).
    if let Some(path) = cx.dialogs.pending_import.take() {
        st.importing = Some((path, "Preparing import...".into()));
        st.started = false;
    }
    // The overlay was drawn last frame, so the import can run now (R-MD-5).
    let ready = st.started;
    if let Some((path, _)) = st.importing.take_if(|_| ready) {
        st.started = false;
        data::run_import(cx, &path);
    }
    let busy = st.importing.is_some();

    ui.heading("Settings");
    ui.add_space(8.0);
    ui.add_enabled_ui(!busy, |ui| {
        font_row(cx, ui);
        ui.add_space(12.0);
        backup_section(cx, ui);
        ui.add_space(12.0);
        export_import_section(cx, ui);
        ui.add_space(12.0);
        ingest_section(cx, ui);
        ui.add_space(12.0);
        danger_section(cx, ui);
    });

    if let Some((_, progress)) = &st.importing {
        overlay(ui, progress);
        st.started = true;
        ui.ctx().request_repaint();
    }
}

/// A section heading.
fn section(ui: &mut Ui, title: &str) {
    ui.strong(title);
    ui.add_space(4.0);
}

/// The "Font" row: a combo box over `fonts::CHOICES`. A new choice is saved to `uiFont` through
/// the engine and applied to the egui context at once.
fn font_row(cx: &mut UiCtx, ui: &mut Ui) {
    let current = fonts::choice_for_key(settings::ui_font(cx.engine)).key;
    let mut selected = current;
    ui.horizontal(|ui| {
        let label = ui.label("Font");
        let shown = fonts::choice_for_key(Some(current)).label;
        egui::ComboBox::from_id_salt("ui_font")
            .selected_text(shown)
            .show_ui(ui, |ui| {
                for choice in &fonts::CHOICES {
                    ui.selectable_value(&mut selected, choice.key, choice.label);
                }
            })
            .response
            .labelled_by(label.id);
    });
    if selected != current {
        match settings::set_ui_font(cx.engine, selected) {
            Ok(()) => fonts::install(ui.ctx(), Some(selected)),
            Err(e) => cx.toasts.info(format!("Could not save the font: {e}")),
        }
    }
}

/// The folder that Cmd-S writes into. An empty setting means "not set" (see `app::backup_now`).
fn backup_folder_is_set(cx: &UiCtx) -> bool {
    backup::backup_folder(cx.engine).is_some_and(|p| !p.as_os_str().is_empty())
}

/// Automatic Backups: the folder row and, once a folder is set, Backup Now (R-MD-7).
fn backup_section(cx: &mut UiCtx, ui: &mut Ui) {
    section(ui, "Automatic Backups");
    let set = backup_folder_is_set(cx);
    ui.horizontal(|ui| {
        if ui.button("Automatic Backup Folder").clicked() {
            choose_backup_folder(cx);
        }
        if set {
            ui.weak("Configured");
        } else {
            ui.colored_label(colors::RED, "Not Set");
        }
    });
    if set {
        let now = RichText::new("Backup Now (Cmd-S)").color(colors::BLUE);
        if ui.add(Button::new(now)).clicked() {
            app::backup_now(cx);
        }
    }
    // R-MD-8: on by default. With no folder set, exiting asks for one, as Cmd-S does.
    let mut on_exit = settings::backup_on_exit(cx.engine);
    if ui
        .checkbox(&mut on_exit, "Back up automatically when the app exits")
        .changed()
        && let Err(e) = settings::set_backup_on_exit(cx.engine, on_exit)
    {
        cx.dialogs.alert("Backup Error", e.to_string());
    }
}

/// Asks for a folder and stores it as `backupFolder`. A cancelled pick changes nothing.
fn choose_backup_folder(cx: &mut UiCtx) {
    let Some(folder) = cx.files.pick_folder("Set Backup Folder") else {
        return;
    };
    if let Err(e) = backup::set_backup_folder(cx.engine, &folder) {
        cx.dialogs.alert("Backup Error", e.to_string());
    }
}

/// Manual Export & Import: Export to Markdown, and Import from Markdown (wipes data).
fn export_import_section(cx: &mut UiCtx, ui: &mut Ui) {
    section(ui, "Manual Export & Import");
    let export = RichText::new("Export to Markdown").color(colors::BLUE);
    if ui.add(Button::new(export)).clicked() {
        data::export_markdown(cx);
    }
    let import = RichText::new("Import from Markdown (Wipes current data)").color(colors::ORANGE);
    if ui.add(Button::new(import)).clicked() {
        data::choose_import_file(cx);
    }
}

/// Data Ingest: the Monarch CSV entry point (the wizard itself is T22).
fn ingest_section(cx: &mut UiCtx, ui: &mut Ui) {
    section(ui, "Data Ingest");
    let monarch = RichText::new("Import from Monarch CSV").color(colors::GREEN);
    if ui.add(Button::new(monarch)).clicked() {
        data::import_monarch_csv(cx);
    }
}

/// Danger Zone: Find Orphaned Transactions (N), and Delete All Data.
fn danger_section(cx: &mut UiCtx, ui: &mut Ui) {
    section(ui, "Danger Zone");
    let count = data::orphan_count(cx);
    let label = format!("Find Orphaned Transactions ({count})");
    let colour = if count > 0 {
        colors::ORANGE
    } else {
        ui.visuals().weak_text_color()
    };
    if ui
        .add_enabled(count > 0, Button::new(RichText::new(label).color(colour)))
        .clicked()
    {
        cx.dialogs.open(crate::dialogs::Dialog::Orphaned);
    }
    let delete = RichText::new("Delete All Data").color(colors::RED);
    if ui.add(Button::new(delete)).clicked() {
        data::confirm_delete_all(cx);
    }
}

/// The blocking import overlay: a spinner and the progress text, centred over the screen.
fn overlay(ui: &mut Ui, progress: &str) {
    egui::Modal::new(egui::Id::unique("settings_import_overlay")).show(ui.ctx(), |ui| {
        ui.vertical_centered(|ui| {
            ui.spinner();
            ui.add_space(8.0);
            ui.heading(progress);
        });
    });
}
