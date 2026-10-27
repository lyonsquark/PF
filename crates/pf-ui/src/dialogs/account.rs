//! The account dialogs (T16): New Account, and Edit Account with its starting balance (R-ACC-2,
//! R-ACC-3). Edit works on a copy: Cancel discards it, Save writes the details and then the starting
//! balance (two undo steps).

use egui::{ComboBox, Id, Modal, RichText, Ui};
use jiff::civil::Date;
use pf_engine::commands::accounts::{
    AccountInput, StartingBalanceInfo, calculate_starting_balance, create_account,
    set_starting_balance, starting_balance_info, update_account,
};
use pf_model::{AccountId, AccountType, Db};

use super::Outcome;
use crate::app::UiCtx;
use crate::widgets::amount_field::{amount_edit_text, parse_amount_input};
use crate::widgets::colors;
use crate::widgets::labeled_text;

/// Dialog width in points.
const MIN_WIDTH: f32 = 380.0;

/// The details of an account as typed: name, institution, type, currency and external flag.
#[derive(Debug, Clone, PartialEq)]
pub struct AccountForm {
    /// Display name; trimmed on save.
    pub name: String,
    /// Institution name.
    pub institution: String,
    /// Checking, savings, and so on.
    pub account_type: AccountType,
    /// ISO currency code; empty means USD (engine rule).
    pub currency: String,
    /// External accounts are left out of totals.
    pub is_external: bool,
    /// An engine error from the last Save attempt, shown in red.
    pub error: Option<String>,
}

impl Default for AccountForm {
    fn default() -> Self {
        Self {
            name: String::new(),
            institution: String::new(),
            account_type: AccountType::default(),
            currency: "USD".to_string(),
            is_external: false,
            error: None,
        }
    }
}

impl AccountForm {
    /// The engine input for this form.
    fn input(&self) -> AccountInput {
        AccountInput {
            name: self.name.clone(),
            account_type: self.account_type,
            institution: self.institution.clone(),
            currency: self.currency.clone(),
            is_external: self.is_external,
        }
    }
}

/// The Edit Account form: the details, the starting balance as read when the dialog opened, and
/// the two amount texts (R-ACC-2, R-ACC-3).
#[derive(Debug, Clone, PartialEq)]
pub struct EditAccountForm {
    /// The account being edited.
    pub id: AccountId,
    /// The details section.
    pub details: AccountForm,
    /// Starting balance data, read when the dialog opened.
    pub info: StartingBalanceInfo,
    /// Starting balance as typed.
    pub starting_text: String,
    /// The actual current balance, for "Calculate".
    pub current_text: String,
}

/// The Edit Account form for `id`, filled from the data now. `None` if the account is not live.
pub fn edit_form(db: &Db, id: AccountId, today: Date) -> Option<EditAccountForm> {
    let account = db.account(id).filter(|a| !a.is_deleted)?;
    let info = starting_balance_info(db, id, today);
    Some(EditAccountForm {
        id,
        details: AccountForm {
            name: account.name.clone(),
            institution: account.institution.clone(),
            account_type: account.account_type,
            currency: account.currency.clone(),
            is_external: account.is_external,
            error: None,
        },
        starting_text: amount_edit_text(info.amount),
        current_text: amount_edit_text(info.amount + info.other_sum),
        info,
    })
}

/// The button pressed this frame, applied after the modal is drawn.
enum Action {
    None,
    Save,
    Cancel,
    Calculate,
}

/// The New Account modal. Save is disabled while the name is blank.
pub fn show_add(cx: &mut UiCtx, ui: &mut Ui, form: &mut AccountForm) -> Outcome {
    let mut action = Action::None;
    let response = Modal::new(Id::unique(("new_account",))).show(ui.ctx(), |ui| {
        ui.set_min_width(MIN_WIDTH);
        ui.heading("New Account");
        details(ui, form);
        error_line(ui, form.error.as_deref());
        ui.add_space(8.0);
        buttons(ui, &mut action, form.name.trim().is_empty());
    });
    let mut close = response.should_close();
    match action {
        Action::Cancel => close = true,
        Action::Save => match create_account(cx.engine, form.input()) {
            Ok(_) => close = true,
            Err(e) => form.error = Some(e.to_string()),
        },
        Action::None | Action::Calculate => {}
    }
    outcome(close)
}

/// The Edit Account modal: details, then the starting balance section (R-ACC-2). Calculate fills the
/// starting balance from the actual current balance (R-ACC-3).
pub fn show_edit(cx: &mut UiCtx, ui: &mut Ui, form: &mut EditAccountForm) -> Outcome {
    let mut action = Action::None;
    let response = Modal::new(Id::unique(("edit_account", form.id.raw()))).show(ui.ctx(), |ui| {
        ui.set_min_width(MIN_WIDTH + 200.0);
        ui.heading("Edit Account");
        details(ui, &mut form.details);
        ui.add_space(8.0);
        ui.label(RichText::new("Starting Balance").strong());
        starting_section(ui, form);
        ui.separator();
        ui.horizontal(|ui| {
            labeled_text(ui, "Current Balance", &mut form.current_text);
            if ui.button("Calculate").clicked() {
                action = Action::Calculate;
            }
        });
        ui.label(
            RichText::new(
                "Enter your actual current balance and click Calculate to determine the starting balance.",
            )
            .small()
            .weak(),
        );
        error_line(ui, form.details.error.as_deref());
        ui.add_space(8.0);
        buttons(ui, &mut action, form.details.name.trim().is_empty());
    });
    let mut close = response.should_close();
    match action {
        Action::None => {}
        Action::Cancel => close = true,
        Action::Calculate => match parse_amount_input(&form.current_text) {
            Some(current) => {
                let start = calculate_starting_balance(current, form.info.other_sum);
                form.starting_text = amount_edit_text(start);
                form.details.error = None;
            }
            None => form.details.error = Some("Enter the current balance as a number".into()),
        },
        Action::Save => {
            if let Err(e) = save_edit(cx, form) {
                form.details.error = Some(e);
            } else {
                close = true;
            }
        }
    }
    outcome(close)
}

/// Saves the details, then the starting balance. The first error stops the save and is returned.
fn save_edit(cx: &mut UiCtx, form: &EditAccountForm) -> Result<(), String> {
    let start = parse_amount_input(&form.starting_text)
        .ok_or_else(|| "Enter the starting balance as a number".to_string())?;
    update_account(cx.engine, form.id, form.details.input()).map_err(|e| e.to_string())?;
    set_starting_balance(cx.engine, form.id, start).map_err(|e| e.to_string())?;
    Ok(())
}

/// The starting balance section: the earliest transaction, the balance date, the amount, and a note
/// when a starting-balance row already exists.
fn starting_section(ui: &mut Ui, form: &mut EditAccountForm) {
    egui::Grid::new("starting_balance")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("Earliest transaction:");
            match form.info.earliest_other {
                Some(d) => ui.label(format_date(d)),
                None => ui.label(RichText::new("No transactions yet").weak()),
            };
            ui.end_row();
            ui.label("Starting balance date:");
            ui.label(format_date(form.info.balance_date));
            ui.end_row();
            labeled_text(ui, "Starting Balance", &mut form.starting_text);
            ui.end_row();
        });
    if form.info.existing.is_some() {
        ui.label(
            RichText::new("A \"Starting Balance\" transaction exists and will be updated on save.")
                .small()
                .weak(),
        );
    }
}

/// "Mar 1, 2026" (R-ACC-2 display format).
fn format_date(d: Date) -> String {
    d.strftime("%b %-d, %Y").to_string()
}

/// The fields shared by both dialogs.
fn details(ui: &mut Ui, form: &mut AccountForm) {
    egui::Grid::new("account_details")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            labeled_text(ui, "Account Name", &mut form.name);
            ui.end_row();
            labeled_text(ui, "Institution", &mut form.institution);
            ui.end_row();
            ui.label("Type");
            let resp = ComboBox::from_id_salt("account_type")
                .selected_text(form.account_type.label())
                .show_ui(ui, |ui| {
                    for t in AccountType::ALL {
                        ui.selectable_value(&mut form.account_type, t, t.label());
                    }
                });
            resp.response
                .widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, "Type"));
            ui.end_row();
            labeled_text(ui, "Currency", &mut form.currency);
            ui.end_row();
        });
    ui.checkbox(&mut form.is_external, "External Account");
}

/// A red error line, when there is one.
fn error_line(ui: &mut Ui, error: Option<&str>) {
    if let Some(err) = error {
        ui.label(RichText::new(err).color(colors::RED));
    }
}

/// Cancel and Save, Save disabled when `save_blocked` (a blank name).
fn buttons(ui: &mut Ui, action: &mut Action, save_blocked: bool) {
    ui.horizontal(|ui| {
        if ui.button("Cancel").clicked() {
            *action = Action::Cancel;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add_enabled(!save_blocked, egui::Button::new("Save"))
                .clicked()
            {
                *action = Action::Save;
            }
        });
    });
}

/// Maps "close this frame" to the dialog outcome.
fn outcome(close: bool) -> Outcome {
    if close {
        Outcome::Close
    } else {
        Outcome::KeepOpen
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    #[test]
    /// The default new-account form is USD and not external; its input trims nothing (the engine
    /// trims the name on save).
    fn account_form_defaults_to_usd() {
        let f = AccountForm::default();
        assert_eq!(f.currency, "USD");
        assert!(!f.is_external);
        assert_eq!(f.input().name, "");
    }

    #[test]
    /// Covers: R-ACC-3. The starting balance is the actual current balance minus the other rows.
    fn calculate_uses_other_rows() {
        let current = Decimal::new(1000, 0);
        let other = Decimal::new(200, 0);
        assert_eq!(
            calculate_starting_balance(current, other),
            Decimal::new(800, 0)
        );
    }

    #[test]
    /// The date text matches the Swift display format.
    fn dates_display_like_swift() {
        let d: Date = "2026-03-01".parse().expect("date literal");
        assert_eq!(format_date(d), "Mar 1, 2026");
    }
}
