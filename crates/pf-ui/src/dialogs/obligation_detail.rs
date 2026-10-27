//! The Edit Obligation dialog (T18, Swift `ObligationDetailView`): the name, the Reality balance and
//! progress towards the target, the status, the account and context, the target and balance, the due
//! date, notes, dates, and the transactions tagged with its context. Each change is one engine command.
//! Text fields commit on focus loss or Enter.

use std::cmp::Ordering;

use egui::{
    Align2, ComboBox, Id, Modal, ProgressBar, RichText, Role, TextEdit, TextStyle, Ui, WidgetInfo,
};
use egui_extras::DatePickerButton;
use jiff::civil::Date;
use pf_core::format;
use pf_engine::commands::obligations::{ObligationField, display_balance, edit_obligation};
use pf_model::{AccountId, ContextId, Db, ObligationId, ObligationStatus, Transaction};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;

use super::Outcome;
use crate::app::UiCtx;
use crate::screens::obligations::format_date;
use crate::widgets::amount_field::{amount_edit_text, parse_amount_input};
use crate::widgets::colors;

/// Dialog width in points.
const MIN_WIDTH: f32 = 520.0;
/// Width of the progress bar in points.
const BAR_WIDTH: f32 = 200.0;
/// Height of the related transactions list in points.
const RELATED_HEIGHT: f32 = 240.0;

/// The text the detail dialog edits. It lives in `Dialog::ObligationDetail` while open.
#[derive(Debug, Clone, PartialEq)]
pub struct ObligationDetailForm {
    /// The obligation shown.
    pub id: ObligationId,
    /// The name, as typed.
    pub name: String,
    /// The target amount, as typed (blank means none).
    pub target: String,
    /// The initial (funded) balance, as typed.
    pub initial: String,
    /// The notes, as typed.
    pub notes: String,
    /// An error from the last commit, shown in red.
    pub error: Option<String>,
}

/// The form for obligation `id`, filled from the data now. `None` if it is missing or deleted.
pub fn form_for(db: &Db, id: ObligationId) -> Option<ObligationDetailForm> {
    let ob = db.obligation(id).filter(|o| !o.is_deleted)?;
    Some(ObligationDetailForm {
        id,
        name: ob.name.clone(),
        target: ob.target_amount.map(amount_edit_text).unwrap_or_default(),
        initial: amount_edit_text(ob.current_balance),
        notes: ob.notes.clone().unwrap_or_default(),
        error: None,
    })
}

/// Parses an optional amount: blank is `Ok(None)`, a number `Ok(Some)`, anything else `Err`.
fn parse_optional(text: &str) -> Result<Option<Decimal>, ()> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    parse_amount_input(text).map(Some).ok_or(())
}

/// Draws the detail modal and applies the edits it asks for.
pub fn show(cx: &mut UiCtx, ui: &mut Ui, form: &mut ObligationDetailForm) -> Outcome {
    let today = cx.engine.today();
    let tz = cx.engine.tz().clone();
    let lists = cx.caches.lookups(cx.engine).clone();
    let Some(ob) = cx
        .engine
        .db()
        .obligation(form.id)
        .filter(|o| !o.is_deleted)
        .cloned()
    else {
        return Outcome::Close;
    };
    let balance = display_balance(cx.engine.db(), form.id);
    let related = related_transactions(cx.engine.db(), &ob);
    let mut commits: Vec<ObligationField> = Vec::new();
    let mut done = false;

    let response =
        Modal::new(Id::unique(("obligation_detail", form.id.raw()))).show(ui.ctx(), |ui| {
            ui.set_min_width(MIN_WIDTH);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Done").clicked() {
                        done = true;
                    }
                });
            });
            ui.vertical_centered(|ui| {
                let name = ui.add(
                    TextEdit::singleline(&mut form.name)
                        .font(TextStyle::Heading)
                        .frame(egui::Frame::NONE)
                        .align(Align2::CENTER_CENTER)
                        .desired_width(MIN_WIDTH),
                );
                name.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, "Name"));
                if name.lost_focus() {
                    let typed = form.name.trim();
                    if !typed.is_empty() && typed != ob.name {
                        commits.push(ObligationField::Name(typed.to_string()));
                    }
                }
                let mut big = RichText::new(format::format_currency(balance)).size(36.0);
                big = big.color(if balance >= Decimal::ZERO {
                    colors::GREEN
                } else {
                    colors::RED
                });
                ui.label(big);
                if let Some(target) = ob.target_amount.filter(|t| *t > Decimal::ZERO) {
                    progress(ui, balance, target);
                    ui.label(
                        RichText::new(format!("of {} goal", format::format_currency(target)))
                            .weak(),
                    );
                }
            });
            ui.add_space(6.0);
            status_row(ui, ob.status, &mut commits);
            ui.separator();
            ui.label(RichText::new("Details").strong());
            egui::Grid::new("obligation_detail_fields")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.label("Account");
                    let mut account = ob.account_id;
                    account_combo(ui, &mut account, &lists.accounts);
                    if account != ob.account_id {
                        commits.push(ObligationField::Account(account));
                    }
                    ui.end_row();

                    ui.label("Context");
                    let mut context = ob.context_id;
                    context_combo(ui, &mut context, &lists.contexts);
                    if context != ob.context_id {
                        commits.push(ObligationField::Context(context));
                    }
                    ui.end_row();

                    let target_resp = labeled_field(ui, "Target Amount", &mut form.target);
                    if target_resp.lost_focus() {
                        match parse_optional(&form.target) {
                            Ok(value) if value != ob.target_amount => {
                                commits.push(ObligationField::Target(value));
                            }
                            Ok(_) => {}
                            Err(()) => form.error = Some("Invalid target amount".into()),
                        }
                    }
                    ui.end_row();

                    let balance_resp = labeled_field(ui, "Initial Balance", &mut form.initial);
                    if balance_resp.lost_focus() {
                        match parse_amount_input(&form.initial) {
                            Some(value) if value != ob.current_balance => {
                                commits.push(ObligationField::Balance(value));
                            }
                            Some(_) => {}
                            None => form.error = Some("Invalid initial balance".into()),
                        }
                    }
                    ui.end_row();

                    ui.label("Due Date");
                    due_date(ui, ob.due_date, today, &mut commits);
                    ui.end_row();
                });
            ui.add_space(6.0);
            ui.label(RichText::new("Notes").strong());
            let notes = ui.add(
                TextEdit::multiline(&mut form.notes)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );
            notes.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, "Notes"));
            if notes.lost_focus() {
                let typed = form.notes.trim();
                let value = (!typed.is_empty()).then(|| typed.to_string());
                if value != ob.notes {
                    commits.push(ObligationField::Notes(value));
                }
            }
            dates_line(ui, ob.created_date, ob.last_updated_date, &tz);
            if ob.context_id.is_some() {
                ui.separator();
                related_list(ui, cx.engine.db(), &related);
            }
            if let Some(err) = &form.error {
                ui.label(RichText::new(err).color(colors::RED));
            }
        });

    let close = response.should_close() || done;
    for field in commits {
        if let Err(e) = edit_obligation(cx.engine, form.id, field) {
            form.error = Some(e.to_string());
        }
    }
    if close {
        Outcome::Close
    } else {
        Outcome::KeepOpen
    }
}

/// A labelled text field in the details grid. Returns the response so the caller can act on focus loss.
fn labeled_field(ui: &mut Ui, label: &str, text: &mut String) -> egui::Response {
    ui.label(label);
    let resp = ui.add(TextEdit::singleline(text).desired_width(160.0));
    resp.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, label));
    resp
}

/// The progress bar towards the target: the balance as a share of the target, capped at 1. Green when
/// the target is reached, blue otherwise.
fn progress(ui: &mut Ui, balance: Decimal, target: Decimal) {
    let ratio = (balance / target).to_f32().unwrap_or(0.0).clamp(0.0, 1.0);
    let color = if ratio >= 1.0 {
        colors::GREEN
    } else {
        colors::BLUE
    };
    ui.add(ProgressBar::new(ratio).desired_width(BAR_WIDTH).fill(color));
}

/// The status choice. A change is one engine command.
fn status_row(ui: &mut Ui, current: ObligationStatus, commits: &mut Vec<ObligationField>) {
    let mut selected = current;
    ui.horizontal(|ui| {
        for s in ObligationStatus::ALL {
            ui.selectable_value(&mut selected, s, s.label());
        }
    });
    if selected != current {
        commits.push(ObligationField::Status(selected));
    }
}

/// The due date: a date picker with an "x" when set, or "Set Due Date" (today) when not.
fn due_date(ui: &mut Ui, due: Option<Date>, today: Date, commits: &mut Vec<ObligationField>) {
    ui.horizontal(|ui| match due {
        Some(current) => {
            let mut date = current;
            ui.add(
                DatePickerButton::new(&mut date)
                    .id_salt("obligation_due")
                    .show_icon(false),
            );
            if date != current {
                commits.push(ObligationField::DueDate(Some(date)));
            }
            if ui
                .small_button("x")
                .on_hover_text("Remove due date")
                .clicked()
            {
                commits.push(ObligationField::DueDate(None));
            }
        }
        None => {
            if ui.button("Set Due Date").clicked() {
                commits.push(ObligationField::DueDate(Some(today)));
            }
        }
    });
}

/// The created and last-updated dates. The update time is shown in the engine's zone.
fn dates_line(
    ui: &mut Ui,
    created: Option<jiff::Timestamp>,
    updated: Option<jiff::Timestamp>,
    tz: &jiff::tz::TimeZone,
) {
    let mut parts = Vec::new();
    if let Some(c) = created {
        let date = c.to_zoned(tz.clone()).date();
        parts.push(format!("Created {}", format_date(date)));
    }
    if let Some(u) = updated {
        parts.push(format!(
            "Updated {}",
            u.to_zoned(tz.clone()).strftime("%b %-d, %Y %-I:%M %p")
        ));
    }
    if !parts.is_empty() {
        ui.label(RichText::new(parts.join("   ")).small().weak());
    }
}

/// The transactions tagged with the obligation's context (any scenario), newest first.
fn related_transactions<'a>(db: &'a Db, ob: &pf_model::Obligation) -> Vec<&'a Transaction> {
    let Some(ctx) = ob.context_id else {
        return Vec::new();
    };
    let mut txs: Vec<&Transaction> = db.transactions_with_context(ctx).collect();
    txs.sort_by(|a, b| match b.date.cmp(&a.date) {
        Ordering::Equal => b.id.cmp(&a.id),
        other => other,
    });
    txs
}

/// The related transactions: a count, then each row with its payee, date, account, scenario and memo.
fn related_list(ui: &mut Ui, db: &Db, txs: &[&Transaction]) {
    if txs.is_empty() {
        return;
    }
    ui.horizontal(|ui| {
        ui.label(RichText::new("Related Transactions").strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(txs.len().to_string()).weak());
        });
    });
    egui::ScrollArea::vertical()
        .max_height(RELATED_HEIGHT)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for tx in txs {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&tx.payee).strong());
                        let mut line = vec![format_date(tx.date)];
                        if let Some(a) = tx.account_id.and_then(|a| db.account(a)) {
                            line.push(a.name.clone());
                        }
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(line.join(" • ")).small());
                            if let Some(s) = tx.scenario_id.and_then(|s| db.scenario(s)) {
                                ui.label(
                                    RichText::new(format!("• {}", s.name))
                                        .small()
                                        .color(colors::ORANGE),
                                );
                            }
                        });
                        if let Some(memo) = tx.memo.as_deref().filter(|m| !m.is_empty()) {
                            ui.label(RichText::new(memo).small().weak());
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let amount = format::format_currency(tx.amount);
                        if tx.amount >= Decimal::ZERO {
                            ui.label(RichText::new(amount).color(colors::GREEN));
                        } else {
                            ui.label(amount);
                        }
                    });
                });
            }
        });
}

/// The account picker: None first, then live accounts by name.
fn account_combo(ui: &mut Ui, value: &mut Option<AccountId>, accounts: &[(AccountId, String)]) {
    let shown = value
        .and_then(|id| accounts.iter().find(|(a, _)| *a == id))
        .map_or("None", |(_, name)| name.as_str())
        .to_string();
    let resp = ComboBox::from_id_salt("obligation_detail_account")
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

/// The context picker, shown as "#name": None first, then live contexts by name.
fn context_combo(
    ui: &mut Ui,
    value: &mut Option<ContextId>,
    contexts: &[(ContextId, String, Option<String>)],
) {
    let shown = value
        .and_then(|id| contexts.iter().find(|(c, _, _)| *c == id))
        .map_or("None".to_string(), |(_, name, _)| format!("#{name}"));
    let resp = ComboBox::from_id_salt("obligation_detail_context")
        .width(220.0)
        .selected_text(shown)
        .show_ui(ui, |ui| {
            ui.selectable_value(value, None, "None");
            for (id, name, _) in contexts {
                ui.selectable_value(value, Some(*id), format!("#{name}"));
            }
        });
    resp.response
        .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, "Context"));
}
