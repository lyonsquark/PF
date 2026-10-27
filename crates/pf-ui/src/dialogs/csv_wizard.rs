//! The CSV import wizard (T22, Swift `CSVImportFlowView`): map the columns, map the unrecognized
//! accounts, categories and payees, then import through one engine command and show the summary.
//! Parsing, detection, planning and importing are `pf_core` and `pf_engine`; this file only draws them.
//! The translation step and its sub-dialogs live in `csv_wizard/translate.rs`.

mod translate;

use egui::{ComboBox, Id, Key, Modal, RichText, Role, Ui, WidgetInfo};
use pf_core::csv_import::{
    ColumnMapping, ImportField, ImportResult, ParsedCsv, TranslationChoices, UnmappedValues,
    collect_unmapped, detect_columns, parse_csv, saved_column_assignments, saved_negate,
    validate_required,
};
use pf_engine::commands::import::{CsvImportRequest, run_csv_import};
use pf_model::AccountId;

use super::Outcome;
use crate::app::UiCtx;
use crate::widgets::colors;

pub use translate::SubDialog;

/// Dialog width in points.
const MIN_WIDTH: f32 = 560.0;
/// Height of the column list in points.
const COLUMN_LIST_HEIGHT: f32 = 300.0;
/// Width of a header in the column list, in points.
const HEADER_WIDTH: f32 = 180.0;

/// The step the wizard is on.
#[derive(Debug, Clone, PartialEq)]
pub enum WizardStep {
    /// Assign each column a field.
    ColumnMapping,
    /// Map the unrecognized values.
    Translation,
    /// The result of the import.
    Summary(ImportResult),
}

/// The wizard's state. It lives in `Dialog::CsvWizard` while open.
#[derive(Debug, Clone, PartialEq)]
pub struct CsvWizard {
    /// Monarch mode: accounts are resolved per row.
    pub is_monarch: bool,
    /// The account to import into (bank mode).
    pub target: Option<AccountId>,
    /// The parsed file, read once when the wizard opens.
    pub parsed: ParsedCsv,
    /// The column mapping (flags such as negate are read from here).
    pub mapping: ColumnMapping,
    /// One entry per header: its index, name and field.
    pub assignments: Vec<(usize, String, ImportField)>,
    /// The current step.
    pub step: WizardStep,
    /// The values that need a decision (step 2).
    pub unmapped: UnmappedValues,
    /// The decisions for those values.
    pub choices: TranslationChoices,
    /// A sub-dialog open over the wizard (New Account, New Parent Category).
    pub sub_dialog: Option<SubDialog>,
    /// An error shown in red in the current step.
    pub error: Option<String>,
}

/// Opens the wizard for `csv_text`. `is_monarch` selects Monarch mode; `target` is the account to
/// import into (bank mode). The saved column assignments and negate flag are applied (R-CSV-15).
pub fn open(cx: &mut UiCtx, csv_text: &str, is_monarch: bool, target: Option<AccountId>) {
    let parsed = parse_csv(csv_text);
    let mut mapping = detect_columns(&parsed.headers);
    let db = cx.engine.db();
    // Saved assignments replace the detected ones, but only when there are some: applying an empty
    // list would clear every column.
    let saved = saved_column_assignments(db, &parsed.headers, target, is_monarch);
    if !saved.is_empty() {
        mapping.apply_assignments(&saved);
    }
    if !is_monarch
        && let Some(account) = target
        && let Some(negate) = saved_negate(db, account)
    {
        mapping.negate_amounts = negate;
    }
    let assignments = mapping.assignments(&parsed.headers);
    cx.dialogs
        .open(super::Dialog::CsvWizard(Box::new(CsvWizard {
            is_monarch,
            target,
            parsed,
            mapping,
            assignments,
            step: WizardStep::ColumnMapping,
            unmapped: UnmappedValues::default(),
            choices: TranslationChoices::default(),
            sub_dialog: None,
            error: None,
        })));
}

/// Asks for a bank CSV and opens the wizard for `account` (Account detail, Reality only). A file that
/// cannot be read is an alert.
pub fn pick_bank_csv(cx: &mut UiCtx, account: AccountId) {
    let Some(path) = cx.files.pick_file("Import CSV", &["csv", "txt"]) else {
        return;
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => open(cx, &text, false, Some(account)),
        Err(e) => cx
            .dialogs
            .alert("Import CSV", format!("Failed to load CSV: {e}")),
    }
}

/// The button pressed this frame, applied after the modal is drawn.
pub(super) enum Action {
    None,
    Cancel,
    Continue,
    Import,
    Done,
}

/// Draws the wizard modal for the current step and runs the chosen action.
pub fn show(cx: &mut UiCtx, ui: &mut Ui, w: &mut CsvWizard) -> Outcome {
    let mut action = Action::None;
    let title = match &w.step {
        WizardStep::ColumnMapping => "Map CSV Columns",
        WizardStep::Translation => "Map Unrecognized Values",
        WizardStep::Summary(_) => "Import Complete",
    };
    let response = Modal::new(Id::unique(("csv_wizard",))).show(ui.ctx(), |ui| {
        ui.set_min_width(MIN_WIDTH);
        ui.heading(title);
        match w.step.clone() {
            WizardStep::ColumnMapping => {
                column_step(ui, w, &mut action);
            }
            WizardStep::Translation => {
                translate::show(cx, ui, w, &mut action);
            }
            WizardStep::Summary(result) => {
                summary_step(ui, &result, &mut action);
            }
        }
    });
    translate::show_sub_dialog(cx, ui, w);

    let mut close = response.should_close();
    match action {
        Action::None => {}
        Action::Cancel => close = true,
        Action::Continue => continue_to_translation(cx, w),
        Action::Import => finish_import(cx, w),
        Action::Done => close = true,
    }
    if close {
        Outcome::Close
    } else {
        Outcome::KeepOpen
    }
}

/// Step 1: a row per header with a field picker and the first sample value; the negate flag for a
/// bank; Continue checks the required columns (R-CSV-3).
fn column_step(ui: &mut Ui, w: &mut CsvWizard, action: &mut Action) {
    ui.label("Verify how each CSV column maps to a transaction field.");
    ui.separator();
    let headers = w.parsed.headers.clone();
    egui::ScrollArea::vertical()
        .max_height(COLUMN_LIST_HEIGHT)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            egui::Grid::new("csv_columns")
                .num_columns(3)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("CSV Column").strong());
                    ui.label(RichText::new("Maps To").strong());
                    ui.label(RichText::new("Sample Data").strong());
                    ui.end_row();
                    for (i, header) in headers.iter().enumerate() {
                        ui.add_sized([HEADER_WIDTH, 20.0], egui::Label::new(header).truncate());
                        field_picker(ui, w, i);
                        let sample = w
                            .parsed
                            .rows
                            .first()
                            .and_then(|r| r.get(i))
                            .cloned()
                            .unwrap_or_default();
                        ui.label(RichText::new(sample).weak());
                        ui.end_row();
                    }
                });
        });
    ui.separator();
    if !w.is_monarch {
        ui.checkbox(
            &mut w.mapping.negate_amounts,
            "Negate amounts (flip +/− signs)",
        );
    }
    if let Some(err) = &w.error {
        ui.label(
            RichText::new("Missing Required Columns")
                .strong()
                .color(colors::RED),
        );
        ui.label(RichText::new(err).color(colors::RED));
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui.button("Cancel").clicked() {
            *action = Action::Cancel;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Continue").clicked() {
                *action = Action::Continue;
            }
        });
    });
}

/// The field picker for column `i`. The choice is stored back into `assignments`.
fn field_picker(ui: &mut Ui, w: &mut CsvWizard, i: usize) {
    let current = w
        .assignments
        .iter()
        .find(|(index, _, _)| *index == i)
        .map_or(ImportField::Skip, |(_, _, f)| *f);
    let mut picked = current;
    let resp = ComboBox::from_id_salt(("csvcol", i))
        .width(180.0)
        .selected_text(current.as_str())
        .show_ui(ui, |ui| {
            for field in ImportField::PICKER {
                ui.selectable_value(&mut picked, field, field.as_str());
            }
        });
    resp.response.widget_info(|| {
        WidgetInfo::labeled(
            Role::ComboBox,
            true,
            format!("Column {}", w.parsed.headers[i]),
        )
    });
    if picked != current {
        let header = w.parsed.headers[i].clone();
        match w.assignments.iter_mut().find(|(index, _, _)| *index == i) {
            Some(entry) => entry.2 = picked,
            None => w.assignments.push((i, header, picked)),
        }
    }
}

/// Continue on step 1: the required columns must be mapped, then the unrecognized values are collected
/// for step 2 (R-CSV-3, R-CSV-5).
fn continue_to_translation(cx: &mut UiCtx, w: &mut CsvWizard) {
    if let Err(msg) = validate_required(&w.assignments) {
        w.error = Some(msg);
        return;
    }
    w.error = None;
    let pairs: Vec<(usize, ImportField)> = w.assignments.iter().map(|(i, _, f)| (*i, *f)).collect();
    w.mapping.apply_assignments(&pairs);
    w.unmapped = collect_unmapped(
        cx.engine.db(),
        &w.parsed.rows,
        &w.mapping,
        w.target,
        w.is_monarch,
    );
    w.choices = TranslationChoices::defaults(&w.unmapped);
    w.step = WizardStep::Translation;
}

/// Step 3: the summary lines, and Done (Enter as well).
fn summary_step(ui: &mut Ui, result: &ImportResult, action: &mut Action) {
    ui.add_space(8.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new("✔").size(36.0).color(colors::GREEN));
        for line in result.summary_lines() {
            ui.label(line);
        }
    });
    ui.add_space(8.0);
    ui.vertical_centered(|ui| {
        if ui.button("Done").clicked() {
            *action = Action::Done;
        }
    });
    if ui.input(|i| i.key_pressed(Key::Enter)) {
        *action = Action::Done;
    }
}

/// The request for `run_csv_import`, with the mapping plan built from the current choices. The
/// sub-dialogs have already created any new accounts, so `new_accounts` is empty.
fn request(w: &CsvWizard, db: &pf_model::Db) -> CsvImportRequest {
    let plan = pf_core::csv_import::plan_mappings(
        db,
        &w.parsed.headers,
        &w.mapping,
        &w.choices,
        w.target,
        w.is_monarch,
    );
    CsvImportRequest {
        is_monarch: w.is_monarch,
        target_account: w.target,
        rows: w.parsed.rows.clone(),
        mapping: w.mapping.clone(),
        new_accounts: Vec::new(),
        mapping_plan: plan,
    }
}

/// Runs the import and moves to the summary; an engine error is shown in the step.
fn finish_import(cx: &mut UiCtx, w: &mut CsvWizard) {
    let req = request(w, cx.engine.db());
    match run_csv_import(cx.engine, req) {
        Ok(result) => {
            w.error = None;
            w.step = WizardStep::Summary(result);
        }
        Err(e) => w.error = Some(e.to_string()),
    }
}
