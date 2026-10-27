//! The New Transaction dialog (T16, Swift `AddTransactionView`): type, accounts, payee and category,
//! amount, date, memo, status and contexts. Save closes it, Save & Another keeps it open for the next
//! row. The form logic is pure and unit-tested; `show` only draws it and applies the chosen action.

use egui::{ComboBox, Id, Modal, RichText, Ui};
use egui_extras::DatePickerButton;
use jiff::civil::Date;
use pf_core::categorize::auto_categorize_substring;
use pf_engine::commands::transactions::{
    NewTransaction, TxKind, add_payee_pattern, add_transaction, can_add_payee_pattern,
};
use pf_model::{AccountId, CategoryId, ContextId, ScenarioId, TransactionStatus};

use super::Outcome;
use crate::app::UiCtx;
use crate::widgets::amount_field::parse_amount_input;
use crate::widgets::category::category_picker;
use crate::widgets::colors;
use crate::widgets::labeled_text;

/// Dialog width in points.
const MIN_WIDTH: f32 = 420.0;

/// The three kinds of transaction the dialog can add.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxType {
    /// Money in.
    Deposit,
    /// Money out (the default).
    Withdrawal,
    /// Two rows, out of one account and into another.
    Transfer,
}

impl TxType {
    /// Picker order and labels.
    pub const ALL: [TxType; 3] = [TxType::Deposit, TxType::Withdrawal, TxType::Transfer];

    /// The label shown in the picker.
    pub fn label(self) -> &'static str {
        match self {
            TxType::Deposit => "Deposit",
            TxType::Withdrawal => "Withdrawal",
            TxType::Transfer => "Transfer",
        }
    }
}

/// The form state of the New Transaction dialog. It lives in `Dialog::AddTransaction` while open.
#[derive(Debug, Clone, PartialEq)]
pub struct TxForm {
    /// Deposit, withdrawal or transfer.
    pub tx_type: TxType,
    /// "Account", or "From Account" for a transfer.
    pub account: Option<AccountId>,
    /// "To Account" (transfers only).
    pub to_account: Option<AccountId>,
    /// Payee text (not used by transfers).
    pub payee: String,
    /// Category, if any.
    pub category: Option<CategoryId>,
    /// Amount as typed; parsed on Save (never stored as a half-typed number).
    pub amount_text: String,
    /// Calendar date of the movement.
    pub date: Date,
    /// Optional memo text.
    pub memo: String,
    /// Cleared or Pending in Reality; Future in a scenario.
    pub status: TransactionStatus,
    /// Context tags, in the order they were chosen.
    pub contexts: Vec<ContextId>,
    /// The scenario the rows go into (`None` is Reality). Set when the dialog opens.
    pub scenario: Option<ScenarioId>,
    /// An engine error from the last Save attempt, shown in red.
    pub error: Option<String>,
}

impl TxForm {
    /// A blank withdrawal on `account` for today. In a scenario the status is Future.
    pub fn new(account: Option<AccountId>, scenario: Option<ScenarioId>, today: Date) -> Self {
        Self {
            tx_type: TxType::Withdrawal,
            account,
            to_account: None,
            payee: String::new(),
            category: None,
            amount_text: String::new(),
            date: today,
            memo: String::new(),
            status: default_status(scenario),
            contexts: Vec::new(),
            scenario,
            error: None,
        }
    }

    /// The parsed amount, if the text is a number.
    fn amount(&self) -> Option<rust_decimal::Decimal> {
        parse_amount_input(&self.amount_text)
    }

    /// Swift `isSaveDisabled`, plus the future-date rule for Reality (R-TXN-9): no amount or a zero
    /// amount, no account, a transfer without a different destination, a deposit or withdrawal
    /// without a payee, or a Reality date after `today`.
    pub fn save_disabled(&self, today: Date) -> bool {
        match self.amount() {
            Some(a) if !a.is_zero() => {}
            _ => return true,
        }
        if self.account.is_none() {
            return true;
        }
        match self.tx_type {
            TxType::Transfer => {
                if self.to_account.is_none() || self.to_account == self.account {
                    return true;
                }
            }
            TxType::Deposit | TxType::Withdrawal => {
                if self.payee.trim().is_empty() {
                    return true;
                }
            }
        }
        self.scenario.is_none() && self.date > today
    }

    /// The message shown for a Reality date after today (R-TXN-9), if that is the problem.
    pub fn future_date_error(&self, today: Date) -> Option<&'static str> {
        (self.scenario.is_none() && self.date > today).then_some("Date cannot be in the future")
    }

    /// The engine input for Save. `None` when the amount does not parse or a required account is
    /// missing. The engine applies the sign for the kind (deposits positive, withdrawals negative).
    pub fn to_new_transaction(&self) -> Option<NewTransaction> {
        let amount = self.amount()?;
        let account = self.account?;
        let kind = match self.tx_type {
            TxType::Deposit => TxKind::Deposit,
            TxType::Withdrawal => TxKind::Withdrawal,
            TxType::Transfer => TxKind::Transfer {
                to: self.to_account?,
            },
        };
        Some(NewTransaction {
            kind,
            account,
            date: self.date,
            amount,
            payee: self.payee.clone(),
            memo: self.memo.clone(),
            status: self.status,
            category: self.category,
            contexts: self.contexts.clone(),
        })
    }

    /// R-TXN-10, "Save & Another": keeps the account, date, type and status; clears the amount,
    /// payee, memo, category, contexts and transfer destination.
    pub fn reset_for_another(&mut self) {
        self.to_account = None;
        self.payee.clear();
        self.category = None;
        self.amount_text.clear();
        self.memo.clear();
        self.contexts.clear();
        self.error = None;
    }
}

/// Cleared in Reality; Future in a scenario (the scenario picker is not offered there).
fn default_status(scenario: Option<ScenarioId>) -> TransactionStatus {
    if scenario.is_some() {
        TransactionStatus::Future
    } else {
        TransactionStatus::Cleared
    }
}

/// The button the user pressed this frame, applied after the modal is drawn.
enum Action {
    None,
    Save,
    SaveAnother,
    Cancel,
    /// The "Rec" button: add the payee to the category's auto-match rules (R-CAT-5).
    AddPattern(CategoryId),
}

/// Draws the New Transaction modal and runs the chosen action.
pub fn show(cx: &mut UiCtx, ui: &mut Ui, form: &mut TxForm) -> Outcome {
    let today = cx.engine.today();
    // A copy of the lists, so the modal can be drawn while the engine is borrowed for the actions.
    let lists = cx.caches.lookups(cx.engine).clone();
    let mut action = Action::None;
    let response = Modal::new(Id::unique(("new_transaction",))).show(ui.ctx(), |ui| {
        ui.set_min_width(MIN_WIDTH);
        ui.heading("New Transaction");
        type_row(ui, form);
        ui.add_space(4.0);
        let db = cx.engine.db();
        egui::Grid::new("new_transaction_fields")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                account_rows(ui, form, &lists.accounts);
                if form.tx_type != TxType::Transfer {
                    let changed = labeled_text(ui, "Payee", &mut form.payee).changed();
                    ui.end_row();
                    if changed && let Some(c) = auto_categorize_substring(db, &form.payee) {
                        form.category = Some(c);
                    }
                    ui.label("Category");
                    ui.horizontal(|ui| {
                        if let Some(c) = category_picker(
                            ui,
                            "new_transaction_category",
                            form.category,
                            &lists.categories,
                            true,
                            "Category",
                        ) {
                            form.category = c;
                        }
                        let rec = form
                            .category
                            .is_some_and(|c| can_add_payee_pattern(db, c, &form.payee));
                        let button = ui.add_enabled(rec, egui::Button::new("Rec"));
                        if button
                            .on_hover_text("Add payee to this category's auto-match rules")
                            .clicked()
                            && let Some(c) = form.category
                        {
                            action = Action::AddPattern(c);
                        }
                    });
                    ui.end_row();
                }
                labeled_text(ui, "Amount", &mut form.amount_text);
                ui.end_row();
                ui.label("Date");
                ui.add(
                    DatePickerButton::new(&mut form.date)
                        .id_salt("new_transaction_date")
                        .show_icon(false),
                );
                ui.end_row();
                labeled_text(ui, "Memo", &mut form.memo);
                ui.end_row();
                ui.label("Status");
                status_row(ui, form);
                ui.end_row();
            });
        if !lists.contexts.is_empty() {
            ui.add_space(4.0);
            ui.label("Contexts (Tags)");
            ui.horizontal_wrapped(|ui| {
                for (id, name, _) in &lists.contexts {
                    let selected = form.contexts.contains(id);
                    if ui.selectable_label(selected, format!("#{name}")).clicked() {
                        if selected {
                            form.contexts.retain(|c| c != id);
                        } else {
                            form.contexts.push(*id);
                        }
                    }
                }
            });
        }
        if let Some(name) = form
            .scenario
            .and_then(|s| cx.engine.db().scenario(s))
            .map(|s| s.name.clone())
        {
            ui.label(RichText::new(format!("Adding to Scenario: {name}")).color(colors::ORANGE));
        }
        if let Some(msg) = form.future_date_error(today) {
            ui.label(RichText::new(msg).color(colors::RED));
        }
        if let Some(err) = &form.error {
            ui.label(RichText::new(err).color(colors::RED));
        }
        ui.add_space(8.0);
        let disabled = form.save_disabled(today);
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() {
                action = Action::Cancel;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(!disabled, egui::Button::new("Save"))
                    .clicked()
                {
                    action = Action::Save;
                }
                if ui
                    .add_enabled(!disabled, egui::Button::new("Save & Another"))
                    .clicked()
                {
                    action = Action::SaveAnother;
                }
            });
        });
    });

    let mut close = response.should_close();
    match action {
        Action::None => {}
        Action::Cancel => close = true,
        Action::AddPattern(c) => match add_payee_pattern(cx.engine, c, &form.payee) {
            Ok(text) => cx.toasts.success(text),
            Err(e) => form.error = Some(e.to_string()),
        },
        Action::Save | Action::SaveAnother => {
            if let Some(new) = form.to_new_transaction() {
                match add_transaction(cx.engine, new) {
                    Ok(_) => {
                        if matches!(action, Action::SaveAnother) {
                            form.reset_for_another();
                            cx.toasts.success("Transaction saved");
                        } else {
                            close = true;
                        }
                    }
                    Err(e) => form.error = Some(e.to_string()),
                }
            }
        }
    }
    if close {
        Outcome::Close
    } else {
        Outcome::KeepOpen
    }
}

/// The type picker: Deposit, Withdrawal, Transfer.
fn type_row(ui: &mut Ui, form: &mut TxForm) {
    ui.horizontal(|ui| {
        for t in TxType::ALL {
            ui.selectable_value(&mut form.tx_type, t, t.label());
        }
    });
}

/// The account row(s): one "Account", or "From Account" and "To Account" for a transfer.
fn account_rows(ui: &mut Ui, form: &mut TxForm, accounts: &[(AccountId, String)]) {
    let transfer = form.tx_type == TxType::Transfer;
    let from_label = if transfer { "From Account" } else { "Account" };
    ui.label(from_label);
    account_combo(
        ui,
        "new_transaction_account",
        from_label,
        &mut form.account,
        accounts,
    );
    ui.end_row();
    if transfer {
        ui.label("To Account");
        account_combo(
            ui,
            "new_transaction_to",
            "To Account",
            &mut form.to_account,
            accounts,
        );
        ui.end_row();
    }
}

/// An account picker. The transfer destination cannot be the source.
fn account_combo(
    ui: &mut Ui,
    salt: &str,
    a11y: &str,
    value: &mut Option<AccountId>,
    accounts: &[(AccountId, String)],
) {
    let shown = value
        .and_then(|id| accounts.iter().find(|(a, _)| *a == id))
        .map_or("Select Account", |(_, name)| name.as_str())
        .to_string();
    let resp = ComboBox::from_id_salt(salt)
        .width(220.0)
        .selected_text(shown)
        .show_ui(ui, |ui| {
            for (id, name) in accounts {
                ui.selectable_value(value, Some(*id), name);
            }
        });
    resp.response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, a11y));
}

/// The status row: Cleared or Pending in Reality; a note in a scenario, where the status is Future.
fn status_row(ui: &mut Ui, form: &mut TxForm) {
    if form.scenario.is_some() {
        ui.label(RichText::new("Future").weak());
    } else {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut form.status, TransactionStatus::Cleared, "Cleared");
            ui.selectable_value(&mut form.status, TransactionStatus::Pending, "Pending");
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_engine::commands::transactions::TxKind as Kind;
    use pf_model::testing::d;
    use rust_decimal_macros::dec;

    const TODAY: &str = "2026-03-15";

    /// A valid withdrawal on account 1 with payee "Lunch" and amount "12.50".
    fn valid() -> TxForm {
        let mut f = TxForm::new(Some(AccountId(1)), None, d(TODAY));
        f.payee = "Lunch".into();
        f.amount_text = "12.50".into();
        f
    }

    #[test]
    /// Covers: R-TXN-8. Save is disabled for a missing or zero amount, no account, a transfer
    /// without a different destination, and a deposit or withdrawal without a payee.
    fn txform_save_disabled_rules() {
        let today = d(TODAY);
        assert!(!valid().save_disabled(today));

        let mut f = valid();
        f.amount_text = String::new();
        assert!(f.save_disabled(today), "no amount");
        f.amount_text = "0".into();
        assert!(f.save_disabled(today), "zero amount");

        let mut f = valid();
        f.account = None;
        assert!(f.save_disabled(today), "no account");

        let mut f = valid();
        f.payee = "  ".into();
        assert!(f.save_disabled(today), "withdrawal without payee");

        let mut f = valid();
        f.tx_type = TxType::Transfer;
        f.payee.clear();
        f.to_account = None;
        assert!(f.save_disabled(today), "transfer without destination");
        f.to_account = Some(AccountId(1));
        assert!(f.save_disabled(today), "transfer to the same account");
        f.to_account = Some(AccountId(2));
        assert!(!f.save_disabled(today), "valid transfer needs no payee");
    }

    #[test]
    /// Covers: R-TXN-9. A Reality date after today disables Save with its message; in a scenario a
    /// future date is allowed.
    fn txform_reality_future_date_disabled() {
        let today = d(TODAY);
        let mut f = valid();
        f.date = d("2026-03-16");
        assert!(f.save_disabled(today));
        assert_eq!(
            f.future_date_error(today),
            Some("Date cannot be in the future")
        );

        let mut scenario = TxForm::new(Some(AccountId(1)), Some(ScenarioId(9)), today);
        scenario.payee = "Lunch".into();
        scenario.amount_text = "12.50".into();
        scenario.date = d("2026-03-16");
        assert!(!scenario.save_disabled(today));
        assert_eq!(scenario.future_date_error(today), None);
    }

    #[test]
    /// Covers: R-TXN-8. Each form type maps to its engine kind; a transfer carries its destination.
    fn txform_to_new_transaction_kinds() {
        let mut f = valid();
        let n = f.to_new_transaction().expect("valid form");
        assert_eq!(n.kind, Kind::Withdrawal);
        assert_eq!(n.amount, dec!(12.50));
        assert_eq!(n.payee, "Lunch");

        f.tx_type = TxType::Deposit;
        f.amount_text = "-3".into();
        let n = f.to_new_transaction().expect("valid form");
        assert_eq!(n.kind, Kind::Deposit);
        assert_eq!(n.amount, dec!(-3), "sign is left to the engine");

        f.tx_type = TxType::Transfer;
        f.to_account = Some(AccountId(2));
        let n = f.to_new_transaction().expect("valid form");
        assert_eq!(n.kind, Kind::Transfer { to: AccountId(2) });
    }

    #[test]
    /// Covers: R-TXN-10. "Save & Another" keeps account, date, type and status, and clears the rest.
    fn r_txn_10_reset_for_another_keeps_account_date_type() {
        let mut f = valid();
        f.tx_type = TxType::Transfer;
        f.to_account = Some(AccountId(2));
        f.date = d("2026-03-01");
        f.memo = "note".into();
        f.category = Some(CategoryId(5));
        f.contexts = vec![ContextId(7)];
        f.status = TransactionStatus::Pending;
        f.error = Some("old".into());

        f.reset_for_another();

        assert_eq!(f.account, Some(AccountId(1)));
        assert_eq!(f.date, d("2026-03-01"));
        assert_eq!(f.tx_type, TxType::Transfer);
        assert_eq!(f.status, TransactionStatus::Pending);
        assert!(f.payee.is_empty() && f.amount_text.is_empty() && f.memo.is_empty());
        assert_eq!(f.category, None);
        assert!(f.contexts.is_empty());
        assert_eq!(f.to_account, None);
        assert_eq!(f.error, None);
    }

    #[test]
    /// A new form in a scenario is Future; in Reality it is Cleared.
    fn txform_new_in_scenario_defaults_future() {
        let today = d(TODAY);
        assert_eq!(
            TxForm::new(None, Some(ScenarioId(1)), today).status,
            TransactionStatus::Future
        );
        assert_eq!(
            TxForm::new(None, None, today).status,
            TransactionStatus::Cleared
        );
    }
}
