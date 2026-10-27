//! The New Obligation dialog (T18, Swift Add Obligation): name, target, initial funded amount, account,
//! and a context (a new one named after the obligation, or an existing one). Save creates the
//! obligation and its context as one undo step.

use egui::{ComboBox, Id, Modal, RichText, Role, Ui, WidgetInfo};
use pf_engine::commands::obligations::{
    ContextChoice, NewObligation, create_obligation, suggest_context_name,
};
use pf_model::{AccountId, ContextId};
use rust_decimal::Decimal;

use super::Outcome;
use crate::app::UiCtx;
use crate::widgets::amount_field::parse_amount_input;
use crate::widgets::colors;
use crate::widgets::labeled_text;

/// Dialog width in points.
const MIN_WIDTH: f32 = 420.0;

/// The form state of the dialog. It lives in `Dialog::AddObligation` while open.
#[derive(Debug, Clone, PartialEq)]
pub struct AddObligationForm {
    /// The obligation's name.
    pub name: String,
    /// The target amount as typed; blank means none.
    pub target: String,
    /// The initial funded amount as typed; blank or zero is fine.
    pub initial: String,
    /// The linked account (required).
    pub account: Option<AccountId>,
    /// Create a new context named `context_name` (otherwise use `context`).
    pub create_new_context: bool,
    /// The name of the new context; regenerated from the name while `create_new_context` is on.
    pub context_name: String,
    /// The existing context used when not creating one.
    pub context: Option<ContextId>,
    /// An error from the last Save attempt, shown in red.
    pub error: Option<String>,
}

impl Default for AddObligationForm {
    fn default() -> Self {
        Self {
            name: String::new(),
            target: String::new(),
            initial: "0".to_string(),
            account: None,
            create_new_context: true,
            context_name: String::new(),
            context: None,
            error: None,
        }
    }
}

/// Parses an optional amount: blank is `Ok(None)`, a number is `Ok(Some)`, anything else `Err`.
fn parse_optional(text: &str) -> Result<Option<Decimal>, ()> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    parse_amount_input(text).map(Some).ok_or(())
}

/// The button pressed this frame, applied after the modal is drawn.
enum Action {
    None,
    Cancel,
    Save,
}

/// Draws the New Obligation modal and runs the chosen action.
pub fn show(cx: &mut UiCtx, ui: &mut Ui, form: &mut AddObligationForm) -> Outcome {
    let lists = cx.caches.lookups(cx.engine).clone();
    let target = parse_optional(&form.target);
    let initial = parse_optional(&form.initial);
    let invalid = target.is_err() || initial.is_err();
    let mut action = Action::None;

    let response = Modal::new(Id::unique(("new_obligation",))).show(ui.ctx(), |ui| {
        ui.set_min_width(MIN_WIDTH);
        ui.heading("New Obligation");
        egui::Grid::new("new_obligation_fields")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                if labeled_text(ui, "Name", &mut form.name).changed() && form.create_new_context {
                    form.context_name = suggest_context_name(cx.engine.db(), &form.name);
                }
                ui.end_row();
                labeled_text(ui, "Target Amount", &mut form.target);
                ui.end_row();
                labeled_text(ui, "Initial Funded Amount", &mut form.initial);
                ui.end_row();
                ui.label("Account");
                account_combo(ui, &mut form.account, &lists.accounts);
                ui.end_row();
            });
        ui.add_space(6.0);
        ui.label(RichText::new("Context").strong());
        ui.checkbox(&mut form.create_new_context, "Create New Context");
        if form.create_new_context {
            labeled_text(ui, "Context Name", &mut form.context_name);
        } else {
            context_combo(ui, &mut form.context, &lists.contexts);
        }
        if invalid {
            ui.label(RichText::new("Invalid amount").color(colors::RED));
        }
        if let Some(err) = &form.error {
            ui.label(RichText::new(err).color(colors::RED));
        }
        ui.add_space(8.0);
        let blocked = form.name.trim().is_empty() || form.account.is_none() || invalid;
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() {
                action = Action::Cancel;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(!blocked, egui::Button::new("Save"))
                    .clicked()
                {
                    action = Action::Save;
                }
            });
        });
    });

    let mut close = response.should_close();
    match action {
        Action::None => {}
        Action::Cancel => close = true,
        Action::Save => match save(form, target, initial) {
            Ok(input) => match create_obligation(cx.engine, input) {
                Ok(_) => close = true,
                Err(e) => form.error = Some(e.to_string()),
            },
            Err(e) => form.error = Some(e),
        },
    }
    if close {
        Outcome::Close
    } else {
        Outcome::KeepOpen
    }
}

/// The engine input for Save. The context follows the form: a new one named by the field (blank
/// means none, which the engine handles), or the chosen existing one, or none.
fn save(
    form: &AddObligationForm,
    target: Result<Option<Decimal>, ()>,
    initial: Result<Option<Decimal>, ()>,
) -> Result<NewObligation, String> {
    let target = target.map_err(|()| "Invalid amount".to_string())?;
    let current_balance = initial
        .map_err(|()| "Invalid amount".to_string())?
        .unwrap_or(Decimal::ZERO);
    let account = form
        .account
        .ok_or_else(|| "Choose an account".to_string())?;
    let context = if form.create_new_context {
        ContextChoice::New(form.context_name.clone())
    } else {
        match form.context {
            Some(id) => ContextChoice::Existing(id),
            None => ContextChoice::None,
        }
    };
    Ok(NewObligation {
        name: form.name.clone(),
        target_amount: target,
        current_balance,
        account,
        context,
    })
}

/// The account picker: None first, then live accounts by name.
fn account_combo(ui: &mut Ui, value: &mut Option<AccountId>, accounts: &[(AccountId, String)]) {
    let shown = value
        .and_then(|id| accounts.iter().find(|(a, _)| *a == id))
        .map_or("None", |(_, name)| name.as_str())
        .to_string();
    let resp = ComboBox::from_id_salt("obligation_account")
        .width(220.0)
        .selected_text(shown)
        .show_ui(ui, |ui| {
            ui.selectable_value(value, None, "None");
            for (id, name) in accounts {
                ui.selectable_value(value, Some(*id), name);
            }
        });
    resp.response
        .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, "Account"));
}

/// The existing-context picker: None first, then live contexts by name.
fn context_combo(
    ui: &mut Ui,
    value: &mut Option<ContextId>,
    contexts: &[(ContextId, String, Option<String>)],
) {
    let shown = value
        .and_then(|id| contexts.iter().find(|(c, _, _)| *c == id))
        .map_or("None", |(_, name, _)| name.as_str())
        .to_string();
    let resp = ComboBox::from_id_salt("obligation_context")
        .width(220.0)
        .selected_text(shown)
        .show_ui(ui, |ui| {
            ui.selectable_value(value, None, "None");
            for (id, name, _) in contexts {
                ui.selectable_value(value, Some(*id), name);
            }
        });
    resp.response
        .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, "Select Context"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Blank is "no amount"; a number parses; text is an error.
    fn parse_optional_rules() {
        assert_eq!(parse_optional("  "), Ok(None));
        assert_eq!(parse_optional("1,000"), Ok(Some(Decimal::new(1000, 0))));
        assert_eq!(parse_optional("abc"), Err(()));
    }

    #[test]
    /// The initial funded amount defaults to zero, and the context is created by default.
    fn defaults_are_zero_and_create_context() {
        let f = AddObligationForm::default();
        assert_eq!(f.initial, "0");
        assert!(f.create_new_context);
    }
}
