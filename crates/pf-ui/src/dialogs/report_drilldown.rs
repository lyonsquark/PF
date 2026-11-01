//! Drill-down dialogs for the reports (T23, Swift `ReportSubViews`): the transactions behind a number,
//! and the budget items behind a budgeted figure. The rows come from `pf_core::reports`; this file lays
//! them out.

use egui::{Align, Layout, Modal, RichText, Sense, Ui};
use egui_extras::{Column, TableBuilder};
use jiff::civil::Date;
use pf_core::format;
use pf_core::reports::{budget_drill_rows, drill_rows};
use pf_engine::commands::transactions::{TxField, edit_transaction};
use pf_model::{BudgetItemId, CategoryId, TransactionId};
use rust_decimal::Decimal;

use super::{Dialog, Outcome, Reveal};
use crate::app::UiCtx;
use crate::widgets::category::{PICKER_WIDTH, category_picker};
use crate::widgets::colors;

/// Row height in points.
const ROW_HEIGHT: f32 = 22.0;
/// Header height in points.
const HEADER_HEIGHT: f32 = 20.0;
/// The drill-down dialogs' width, in points, when the window is wide enough.
const DIALOG_WIDTH: f32 = 1040.0;
/// Fixed column widths of the drill-down tables, in points. Together with the gaps between columns they
/// fit inside `DIALOG_WIDTH`: a table wider than its dialog would push the dialog and its text off screen.
const TX_DATE_WIDTH: f32 = 90.0;
const TX_ACCOUNT_WIDTH: f32 = 100.0;
const TX_PAYEE_WIDTH: f32 = 130.0;
const TX_AMOUNT_WIDTH: f32 = 90.0;
const TX_CONTEXT_WIDTH: f32 = 100.0;
/// The category column: the picker (fixed width) plus the cell padding.
const TX_CATEGORY_WIDTH: f32 = PICKER_WIDTH + 16.0;
const TX_STATUS_WIDTH: f32 = 80.0;
const TX_SCENARIO_WIDTH: f32 = 90.0;
const BUDGET_NAME_WIDTH: f32 = 130.0;
const BUDGET_CATEGORY_WIDTH: f32 = 120.0;
const BUDGET_TYPE_WIDTH: f32 = 64.0;
const BUDGET_FREQUENCY_WIDTH: f32 = 120.0;
const BUDGET_AMOUNT_WIDTH: f32 = 90.0;
const BUDGET_START_WIDTH: f32 = 90.0;
const BUDGET_TOTAL_WIDTH: f32 = 176.0;
/// The narrowest the memo column may be; it takes the rest of the transaction window.
const TX_MEMO_MIN_WIDTH: f32 = 80.0;

/// Opens the transactions drill-down (used by the reports and, from T24b, by the charts).
pub fn open_transactions(cx: &mut UiCtx, title: String, ids: Vec<TransactionId>) {
    cx.dialogs.open(Dialog::ReportTransactions { title, ids });
}

/// Which transactions window is drawn. A report number's window has a Context column; a context's
/// window (opened from the Contexts screen) leaves it out, since every row has that context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowKind {
    /// Behind a report number.
    Report,
    /// A context's transactions.
    Context,
}

/// The transactions behind a report number or a context: date, account, payee, category (editable),
/// amount, memo, context (reports only), status and, only while a scenario is active, the scenario.
/// Newest first, with the total of the rows underneath. Changing a transaction's category takes it out
/// of this list (`ids`), so the total follows; the screen behind updates.
pub fn show_transactions(
    cx: &mut UiCtx,
    ui: &mut Ui,
    title: &str,
    ids: &mut Vec<TransactionId>,
    kind: WindowKind,
) -> Outcome {
    let is_report = kind == WindowKind::Report;
    let scenario_active = cx.engine.state.active_scenario.is_some();
    let rows = drill_rows(cx.engine.db(), ids, scenario_active);
    // The current category of each row, and the list the picker offers.
    let categories = cx.caches.lookups(&*cx.engine).categories.clone();
    let current: Vec<Option<CategoryId>> = rows
        .iter()
        .map(|r| cx.engine.db().transaction(r.id).and_then(|t| t.category_id))
        .collect();
    // A category picked this frame, applied after the window is drawn.
    let mut picked: Option<(TransactionId, Option<CategoryId>)> = None;
    // A row clicked this frame: its transaction is shown in its account after the window closes.
    let mut clicked: Option<TransactionId> = None;
    let mut close = false;
    let response = Modal::new(egui::Id::unique(("report_transactions",))).show(ui.ctx(), |ui| {
        ui.set_width(dialog_width(ui));
        // Where the Amount column ends on screen, read from its heading as the table is drawn.
        let mut amount_right: Option<f32> = None;
        ui.heading(title);
        if rows.is_empty() {
            ui.label(RichText::new("No transactions").weak());
        } else {
            {
                // The memo takes the rest of the width, so the table fits the dialog and the sideways
                // scroll is only for a scenario's extra column on a narrow window. Each column starts as
                // wide as its widest text (see `widgets::col_fit`).
                let date_w = TX_DATE_WIDTH;
                let account_w = TX_ACCOUNT_WIDTH;
                let payee_w = TX_PAYEE_WIDTH;
                let category_w = TX_CATEGORY_WIDTH;
                let amount_w = TX_AMOUNT_WIDTH;
                let context_w = TX_CONTEXT_WIDTH;
                let status_w = TX_STATUS_WIDTH;
                let scenario_w = TX_SCENARIO_WIDTH;
                let mut table = TableBuilder::new(ui)
                    .id_salt("drill_transactions")
                    .resizable(true)
                    .striped(true)
                    .auto_shrink([false, true])
                    .max_scroll_height(360.0)
                    .column(Column::initial(date_w))
                    .column(Column::initial(account_w))
                    .column(Column::initial(payee_w))
                    .column(Column::initial(category_w))
                    .column(Column::initial(amount_w))
                    .column(Column::remainder().at_least(TX_MEMO_MIN_WIDTH));
                if is_report {
                    table = table.column(Column::initial(context_w));
                }
                table = table.column(Column::initial(status_w));
                if scenario_active {
                    table = table.column(Column::initial(scenario_w));
                }
                let mut headers: Vec<&str> = if scenario_active {
                    vec![
                        "Date", "Account", "Payee", "Category", "Amount", "Memo", "Context",
                        "Status", "Scenario",
                    ]
                } else {
                    vec![
                        "Date", "Account", "Payee", "Category", "Amount", "Memo", "Context",
                        "Status",
                    ]
                };
                if !is_report {
                    headers.retain(|h| *h != "Context");
                }
                table
                    .header(HEADER_HEIGHT, |mut header| {
                        for h in headers {
                            header.col(|ui| {
                                if h == "Amount" {
                                    amount_right = Some(ui.max_rect().right());
                                }
                                ui.strong(h);
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(ROW_HEIGHT, rows.len(), |mut row| {
                            let index = row.index();
                            let r = &rows[index];
                            let mut row_clicked = false;
                            row.col(|ui| {
                                row_clicked |= clickable(ui, format_date(r.date));
                            });
                            row.col(|ui| {
                                row_clicked |= clickable(ui, &r.account);
                            });
                            row.col(|ui| {
                                row_clicked |= clickable(ui, &r.payee);
                            });
                            row.col(|ui| {
                                if let Some(choice) = category_picker(
                                    ui,
                                    ("drill_category", r.id.raw()),
                                    current[index],
                                    &categories,
                                    true,
                                    &format!("Category {}", r.payee),
                                ) {
                                    picked = Some((r.id, choice));
                                }
                            });
                            row.col(|ui| {
                                row_clicked |= ui
                                    .with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        clickable(ui, amount_text(r.amount))
                                    })
                                    .inner;
                            });
                            row.col(|ui| {
                                row_clicked |= clickable(ui, RichText::new(&r.memo).weak());
                            });
                            if is_report {
                                row.col(|ui| {
                                    row_clicked |= clickable(ui, &r.contexts);
                                });
                            }
                            row.col(|ui| {
                                row_clicked |= clickable(ui, r.status);
                            });
                            if scenario_active {
                                row.col(|ui| {
                                    row_clicked |= if r.scenario == "Reality" {
                                        clickable(ui, RichText::new("Reality").weak())
                                    } else {
                                        clickable(
                                            ui,
                                            RichText::new(&r.scenario).color(colors::BLUE),
                                        )
                                    };
                                });
                            }
                            // A click on the row (not on its picker) shows the transaction in its account.
                            if row_clicked {
                                clicked = Some(r.id);
                            }
                        });
                    });
            }
        }
        // The total ends where the Amount column ends, so it reads as that column's sum.
        if let Some(right) = amount_right {
            let total: Decimal = rows.iter().map(|r| r.amount).sum();
            ui.separator();
            let size = egui::vec2(
                (right - ui.cursor().left()).max(0.0),
                ui.spacing().interact_size.y,
            );
            ui.allocate_ui_with_layout(size, Layout::right_to_left(Align::Center), |ui| {
                ui.label(amount_text(total).strong());
                ui.label(RichText::new("Total").strong());
            });
        }
        ui.add_space(8.0);
        if ui.button("Close").clicked() {
            close = true;
        }
    });
    // Apply a picked category as one undo step. In a report window the transaction then leaves, since
    // its category may decide whether it belongs to the report; a context's window keeps it, because
    // the context does not depend on the category.
    if let Some((id, category)) = picked {
        match edit_transaction(cx.engine, id, TxField::Category(category)) {
            Ok(_) if is_report => ids.retain(|x| *x != id),
            Ok(_) => {}
            Err(e) => cx.toasts.info(e.to_string()),
        }
    }
    // A clicked row closes this window and asks the app to show it in its account (the account
    // screen selects and scrolls to it).
    if let Some(id) = clicked {
        match cx.engine.db().transaction(id).and_then(|t| t.account_id) {
            Some(account) => {
                cx.dialogs.reveal = Some(Reveal {
                    account,
                    transaction: id,
                });
                close = true;
            }
            None => cx
                .toasts
                .info("This transaction has no account, so it cannot be shown."),
        }
    }
    if close || response.should_close() {
        Outcome::Close
    } else {
        Outcome::KeepOpen
    }
}

/// A label that reports a click on it, so a click anywhere on a drill-down row shows its transaction.
fn clickable(ui: &mut Ui, text: impl Into<egui::WidgetText>) -> bool {
    ui.add(egui::Label::new(text).sense(Sense::click()))
        .clicked()
}

/// The budget items behind a budgeted figure, with each one's total for the window.
pub fn show_budget_items(
    cx: &mut UiCtx,
    ui: &mut Ui,
    title: &str,
    ids: &[BudgetItemId],
    start: Date,
    end: Date,
) -> Outcome {
    let rows = budget_drill_rows(cx.engine.db(), ids, start, end);
    let mut close = false;
    let response = Modal::new(egui::Id::unique(("report_budget_items",))).show(ui.ctx(), |ui| {
        ui.set_width(dialog_width(ui));
        ui.heading(title);
        if rows.is_empty() {
            ui.label(RichText::new("No budget items").weak());
        } else {
            let name_w = BUDGET_NAME_WIDTH;
            let category_w = BUDGET_CATEGORY_WIDTH;
            let type_w = BUDGET_TYPE_WIDTH;
            let frequency_w = BUDGET_FREQUENCY_WIDTH;
            let amount_w = BUDGET_AMOUNT_WIDTH;
            let start_w = BUDGET_START_WIDTH;
            let total_w = BUDGET_TOTAL_WIDTH;
            {
                TableBuilder::new(ui)
                    .id_salt("drill_budget_items")
                    .resizable(true)
                    .striped(true)
                    .auto_shrink([false, true])
                    .max_scroll_height(360.0)
                    .column(Column::initial(name_w))
                    .column(Column::initial(category_w))
                    .column(Column::initial(type_w))
                    .column(Column::initial(frequency_w))
                    .column(Column::initial(amount_w))
                    .column(Column::initial(start_w))
                    .column(Column::initial(total_w))
                    .header(HEADER_HEIGHT, |mut header| {
                        for h in [
                            "Name",
                            "Category",
                            "Type",
                            "Frequency",
                            "Amount",
                            "Start Date",
                            "Calculated Total (In Window)",
                        ] {
                            header.col(|ui| {
                                ui.strong(h);
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(ROW_HEIGHT, rows.len(), |mut row| {
                            let r = &rows[row.index()];
                            row.col(|ui| {
                                ui.label(&r.name);
                            });
                            row.col(|ui| {
                                ui.label(&r.category);
                            });
                            row.col(|ui| {
                                ui.label(r.item_type);
                            });
                            row.col(|ui| {
                                ui.label(r.frequency);
                            });
                            row.col(|ui| {
                                ui.label(format::format_currency(r.amount));
                            });
                            row.col(|ui| {
                                ui.label(format_date(r.start_date));
                            });
                            row.col(|ui| {
                                let text =
                                    RichText::new(format::format_currency(r.calculated_total))
                                        .strong();
                                ui.label(tone_text(text, r.calculated_total));
                            });
                        });
                    });
            }
        }
        ui.add_space(8.0);
        if ui.button("Close").clicked() {
            close = true;
        }
    });
    if close || response.should_close() {
        Outcome::Close
    } else {
        Outcome::KeepOpen
    }
}

/// The width of a drill-down dialog: `DIALOG_WIDTH`, or narrower when the window is, so the dialog
/// never extends past the window's edges.
fn dialog_width(ui: &Ui) -> f32 {
    (ui.ctx().content_rect().width() - 48.0).min(DIALOG_WIDTH)
}

/// An amount coloured by its sign: green above zero, red below (R-RPT-2).
pub fn amount_text(amount: Decimal) -> RichText {
    tone_text(RichText::new(format::format_currency(amount)), amount)
}

/// Colours `text` by the sign of `amount`.
fn tone_text(text: RichText, amount: Decimal) -> RichText {
    if amount > Decimal::ZERO {
        text.color(colors::GREEN)
    } else if amount < Decimal::ZERO {
        text.color(colors::RED)
    } else {
        text
    }
}

/// "Mar 1, 2026" (the Swift date format).
fn format_date(d: Date) -> String {
    d.strftime("%b %-d, %Y").to_string()
}
