//! The New and Edit Context dialogs (T20, Swift `ContextEditorView`): name, colour and notes.
//! Create and Save are disabled while the trimmed name is empty.

use egui::{Id, Modal, RichText, Ui};
use pf_core::format::format_currency;
use pf_engine::commands::contexts::{ContextInput, create_context, update_context};
use pf_model::{ContextId, Db};
use rust_decimal::Decimal;

use super::Outcome;
use crate::app::UiCtx;
use crate::widgets::color::{color_field, to_hex};
use crate::widgets::colors;
use crate::widgets::labeled_text;

/// Dialog width in points: narrow for a new context, wider to list the transactions of an edited one.
const MIN_WIDTH: f32 = 360.0;
const EDIT_WIDTH: f32 = 780.0;
/// Height of the transaction list before it scrolls, in points.
const LIST_HEIGHT: f32 = 240.0;
/// The default context colour (R-CTX-4, blue).
pub const DEFAULT_RGB: [u8; 3] = [0, 0, 255];

/// The form state of the dialog. It lives in `Dialog::ContextEdit` while open.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextForm {
    /// The context being edited; `None` when creating.
    pub editing: Option<ContextId>,
    /// The name, as typed (trimmed on save).
    pub name: String,
    /// Notes, as typed.
    pub notes: String,
    /// The colour.
    pub rgb: [u8; 3],
    /// An error from the last save, shown in red.
    pub error: Option<String>,
}

impl Default for ContextForm {
    fn default() -> Self {
        Self {
            editing: None,
            name: String::new(),
            notes: String::new(),
            rgb: DEFAULT_RGB,
            error: None,
        }
    }
}

impl ContextForm {
    /// The form for editing context `id`. A missing or invalid colour shows blue. `None` if missing.
    pub fn edit(db: &Db, id: ContextId) -> Option<Self> {
        let c = db.context(id).filter(|c| !c.is_deleted)?;
        let rgb = c
            .color_hex
            .as_deref()
            .and_then(pf_core::color::parse_hex)
            .map_or(DEFAULT_RGB, |[r, g, b, _]| [r, g, b]);
        Some(Self {
            editing: Some(id),
            name: c.name.clone(),
            notes: c.notes.clone().unwrap_or_default(),
            rgb,
            error: None,
        })
    }

    /// The engine input for this form.
    fn input(&self) -> ContextInput {
        ContextInput {
            name: self.name.clone(),
            color_hex: Some(to_hex(self.rgb)),
            notes: Some(self.notes.clone()),
        }
    }
}

/// One transaction tagged with a context, as the editor lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextTx {
    /// The transaction's date.
    pub date: jiff::civil::Date,
    /// The account's name, or "(no account)" for an orphaned transaction.
    pub account: String,
    /// The payee.
    pub payee: String,
    /// The memo, empty when there is none.
    pub memo: String,
    /// The signed amount.
    pub amount: Decimal,
    /// The scenario's name, or `None` for a reality transaction.
    pub scenario: Option<String>,
}

/// The live transactions tagged with `ctx`, newest first, and their total. These are the same
/// transactions the Contexts screen counts.
pub fn context_transactions(db: &Db, ctx: ContextId) -> (Vec<ContextTx>, Decimal) {
    let mut rows: Vec<ContextTx> = db
        .live_transactions()
        .filter(|tx| tx.context_ids.contains(&ctx))
        .map(|tx| ContextTx {
            date: tx.date,
            account: tx
                .account_id
                .and_then(|a| db.account(a))
                .map_or_else(|| "(no account)".to_string(), |a| a.name.clone()),
            payee: tx.payee.clone(),
            memo: tx.memo.clone().unwrap_or_default(),
            amount: tx.amount,
            scenario: tx
                .scenario_id
                .and_then(|s| db.scenario(s))
                .map(|s| s.name.clone()),
        })
        .collect();
    rows.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.payee.cmp(&b.payee)));
    let total = rows.iter().map(|r| r.amount).sum();
    (rows, total)
}

/// The button pressed this frame, applied after the modal is drawn.
enum Action {
    None,
    Cancel,
    Save,
}

/// Draws the context modal and runs the chosen action.
pub fn show(cx: &mut UiCtx, ui: &mut Ui, form: &mut ContextForm) -> Outcome {
    let mut action = Action::None;
    let listing = form
        .editing
        .map(|id| context_transactions(cx.engine.db(), id));
    let title = if form.editing.is_some() {
        "Edit Context"
    } else {
        "New Context"
    };
    let response = Modal::new(Id::unique(("context_edit", form.editing.map(|i| i.raw())))).show(
        ui.ctx(),
        |ui| {
            ui.set_min_width(if listing.is_some() {
                EDIT_WIDTH
            } else {
                MIN_WIDTH
            });
            ui.heading(title);
            labeled_text(ui, "Context Name", &mut form.name);
            color_field(ui, "Context Color", &mut form.rgb);
            ui.label("Notes");
            let notes = ui.add(
                egui::TextEdit::multiline(&mut form.notes)
                    .desired_rows(3)
                    .desired_width(280.0),
            );
            notes.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, "Notes"));
            if let Some(err) = &form.error {
                ui.label(RichText::new(err).color(colors::RED));
            }
            if let Some((rows, total)) = &listing {
                transaction_list(ui, rows, *total);
            }
            ui.add_space(8.0);
            let blocked = form.name.trim().is_empty();
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    action = Action::Cancel;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !blocked,
                            egui::Button::new(if form.editing.is_some() {
                                "Save"
                            } else {
                                "Create"
                            }),
                        )
                        .clicked()
                    {
                        action = Action::Save;
                    }
                });
            });
        },
    );

    let mut close = response.should_close();
    match action {
        Action::None => {}
        Action::Cancel => close = true,
        Action::Save => {
            let result = match form.editing {
                Some(id) => update_context(cx.engine, id, form.input()).map(|()| id),
                None => create_context(cx.engine, form.input()),
            };
            match result {
                Ok(_) => close = true,
                Err(e) => form.error = Some(e.to_string()),
            }
        }
    }
    if close {
        Outcome::Close
    } else {
        Outcome::KeepOpen
    }
}

/// The transactions tagged with the context, in a scrolling table with their total underneath.
fn transaction_list(ui: &mut Ui, rows: &[ContextTx], total: Decimal) {
    ui.add_space(8.0);
    ui.separator();
    ui.label(RichText::new(format!("Transactions ({})", rows.len())).strong());
    if rows.is_empty() {
        ui.label(RichText::new("No transactions use this context.").weak());
        return;
    }
    egui::ScrollArea::vertical()
        .max_height(LIST_HEIGHT)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            egui::Grid::new("context_transactions")
                .num_columns(6)
                .striped(true)
                .spacing([14.0, 4.0])
                .show(ui, |ui| {
                    for head in ["Date", "Account", "Payee", "Memo", "Amount", "Scenario"] {
                        ui.label(RichText::new(head).strong());
                    }
                    ui.end_row();
                    for r in rows {
                        ui.label(r.date.strftime("%b %-d, %Y").to_string());
                        ui.label(&r.account);
                        ui.label(&r.payee);
                        ui.label(RichText::new(&r.memo).weak());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(format_currency(r.amount));
                        });
                        ui.label(r.scenario.as_deref().unwrap_or("Reality"));
                        ui.end_row();
                    }
                });
        });
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(RichText::new("Total").strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(format_currency(total)).strong());
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::testing::{TestDb, m};
    use pf_model::{AccountType, TransactionStatus};

    #[test]
    /// The editor lists the tagged transactions across accounts, newest first, with their accounts and
    /// the total; untagged transactions are left out.
    fn context_transactions_lists_all_accounts_and_totals() {
        let mut t = TestDb::new();
        let chk = t.account("Checking", AccountType::Checking);
        let card = t.account("Card", AccountType::Credit);
        let trip = t.context("Trip");
        t.tx(chk, "2026-03-01", "-40", "Hotel")
            .contexts(&[trip])
            .status(TransactionStatus::Cleared)
            .done();
        t.tx(card, "2026-03-05", "-12.50", "Taxi")
            .contexts(&[trip])
            .status(TransactionStatus::Cleared)
            .done();
        t.tx(chk, "2026-03-02", "-9", "Coffee")
            .status(TransactionStatus::Cleared)
            .done();
        let db = t.build();
        let (rows, total) = context_transactions(&db, trip);
        let payees: Vec<&str> = rows.iter().map(|r| r.payee.as_str()).collect();
        assert_eq!(
            payees,
            vec!["Taxi", "Hotel"],
            "newest first, untagged left out"
        );
        assert_eq!(rows[0].account, "Card");
        assert_eq!(rows[1].account, "Checking");
        assert_eq!(total, m("-52.50"));
        assert_eq!(
            rows[0].memo, "",
            "a transaction without a memo has an empty one"
        );
    }

    #[test]
    /// A new context is blue and empty.
    fn context_form_defaults_to_blue() {
        let f = ContextForm::default();
        assert_eq!(f.rgb, [0, 0, 255]);
        assert_eq!(f.editing, None);
    }
}
