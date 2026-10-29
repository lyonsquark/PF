//! The Load step: choose a JSON or CSV statement file (R-REC-1, R-REC-2).

use egui::{RichText, Role, TextEdit, Ui, WidgetInfo};
use pf_core::csv_import::{parse_csv_amount, parse_csv_date};
use pf_core::reconcile::{parse_statement_csv, parse_statement_json};
use pf_model::AccountId;

use super::Action;
use crate::app::UiCtx;
use crate::widgets::colors;

/// The Load step's fields.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LoadState {
    /// File name shown after choosing, for a CSV waiting for its balances.
    pub csv_name: Option<String>,
    /// The CSV text, kept until Load is pressed.
    pub csv_text: Option<String>,
    /// Period start, `YYYY-MM-DD` (CSV only).
    pub start: String,
    /// Period end, `YYYY-MM-DD` (CSV only).
    pub end: String,
    /// Opening balance as printed (CSV only).
    pub opening: String,
    /// Closing balance as printed (CSV only).
    pub closing: String,
    /// The last error, shown in red.
    pub error: Option<String>,
}

/// A labelled single-line field with an accessible name.
fn field(ui: &mut Ui, label: &str, text: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        let r = ui.add(TextEdit::singleline(text).desired_width(140.0));
        r.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, label));
    });
}

/// Draws the Load step.
pub(super) fn show(
    cx: &mut UiCtx,
    ui: &mut Ui,
    account: AccountId,
    load: &mut LoadState,
    action: &mut Action,
) {
    let name = cx
        .engine
        .db()
        .account(account)
        .map(|a| a.name.clone())
        .unwrap_or_default();
    ui.label(format!("Account: {name}"));
    ui.label("Choose a statement file: JSON, or a CSV of the statement's lines.");
    if ui.button("Choose File…").clicked() {
        choose(cx, load, action);
    }
    if let Some(file) = &load.csv_name {
        ui.separator();
        ui.label(format!(
            "{file}: enter the period and the balances printed on the statement."
        ));
        field(ui, "Period start", &mut load.start);
        field(ui, "Period end", &mut load.end);
        field(ui, "Opening balance", &mut load.opening);
        field(ui, "Closing balance", &mut load.closing);
        if ui.button("Load").clicked() {
            load_csv(cx, load, action);
        }
    }
    if let Some(err) = &load.error {
        ui.label(RichText::new(err).color(colors::RED));
    }
    ui.add_space(6.0);
    if ui.button("Cancel").clicked() {
        *action = Action::Cancel;
    }
}

/// Picks and reads a file. JSON goes straight to the Review step; CSV waits for its balances.
fn choose(cx: &mut UiCtx, load: &mut LoadState, action: &mut Action) {
    let Some(path) = cx.files.pick_file("Choose Statement", &["json", "csv"]) else {
        return;
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            load.error = Some(format!("Failed to read the file: {e}"));
            return;
        }
    };
    load.error = None;
    let is_csv = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("csv"));
    if is_csv {
        load.csv_name = path.file_name().map(|n| n.to_string_lossy().into_owned());
        load.csv_text = Some(text);
        return;
    }
    load.csv_name = None;
    load.csv_text = None;
    match parse_statement_json(&text) {
        Ok(st) => *action = Action::Loaded(Box::new(st)),
        Err(e) => load.error = Some(e.to_string()),
    }
}

/// Parses the CSV with the typed period and balances.
fn load_csv(cx: &UiCtx, load: &mut LoadState, action: &mut Action) {
    let tz = cx.engine.tz().clone();
    let (Some(start), Some(end)) = (
        parse_csv_date(&load.start, &tz),
        parse_csv_date(&load.end, &tz),
    ) else {
        load.error = Some("Enter the period start and end as YYYY-MM-DD.".into());
        return;
    };
    let (Some(opening), Some(closing)) = (
        parse_csv_amount(&load.opening),
        parse_csv_amount(&load.closing),
    ) else {
        load.error = Some("Enter the opening and closing balances as numbers.".into());
        return;
    };
    let text = load.csv_text.clone().unwrap_or_default();
    match parse_statement_csv(&text, start, end, opening, closing, &tz) {
        Ok(st) => *action = Action::Loaded(Box::new(st)),
        Err(e) => load.error = Some(e.to_string()),
    }
}
