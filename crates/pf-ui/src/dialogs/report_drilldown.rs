//! Drill-down dialogs for the reports (T23, Swift `ReportSubViews`): the transactions behind a number,
//! and the budget items behind a budgeted figure. The rows come from `pf_core::reports`; this file lays
//! them out.

use egui::{Align, Id, Layout, Modal, RichText, Ui};
use egui_extras::{Column, TableBuilder};
use jiff::civil::Date;
use pf_core::format;
use pf_core::reports::{budget_drill_rows, drill_rows};
use pf_model::{BudgetItemId, TransactionId};
use rust_decimal::Decimal;

use super::{Dialog, Outcome};
use crate::app::UiCtx;
use crate::widgets::colors;
use crate::widgets::hscroll::{self, HScroll};

/// Row height in points.
const ROW_HEIGHT: f32 = 22.0;
/// Header height in points.
const HEADER_HEIGHT: f32 = 20.0;
/// The drill-down dialogs' width, in points, when the window is wide enough.
const DIALOG_WIDTH: f32 = 900.0;

/// Opens the transactions drill-down (used by the reports and, from T24b, by the charts).
pub fn open_transactions(cx: &mut UiCtx, title: String, ids: Vec<TransactionId>) {
    cx.dialogs.open(Dialog::ReportTransactions { title, ids });
}

/// The transactions behind a report number: date, account, payee, amount, memo, context, status and,
/// only while a scenario is active, the scenario. Newest first.
pub fn show_transactions(
    cx: &mut UiCtx,
    ui: &mut Ui,
    title: &str,
    ids: &[TransactionId],
) -> Outcome {
    let scenario_active = cx.engine.state.active_scenario.is_some();
    let rows = drill_rows(cx.engine.db(), ids, scenario_active);
    let mut close = false;
    let response = Modal::new(egui::Id::unique(("report_transactions",))).show(ui.ctx(), |ui| {
        ui.set_width(dialog_width(ui));
        ui.heading(title);
        if rows.is_empty() {
            ui.label(RichText::new("No transactions").weak());
        } else {
            with_hscroll(ui, "report_transactions_hscroll", |ui| {
                // The memo takes the rest of the width, so the table fits the dialog and the sideways
                // scroll is only for a scenario's extra column on a narrow window.
                let mut table = TableBuilder::new(ui)
                    .striped(true)
                    .auto_shrink([false, false])
                    .max_scroll_height(360.0)
                    .column(Column::initial(96.0))
                    .column(Column::initial(100.0))
                    .column(Column::initial(150.0))
                    .column(Column::initial(90.0))
                    .column(Column::remainder().at_least(120.0))
                    .column(Column::initial(120.0))
                    .column(Column::initial(80.0));
                if scenario_active {
                    table = table.column(Column::initial(100.0));
                }
                let headers: Vec<&str> = if scenario_active {
                    vec![
                        "Date", "Account", "Payee", "Amount", "Memo", "Context", "Status",
                        "Scenario",
                    ]
                } else {
                    vec![
                        "Date", "Account", "Payee", "Amount", "Memo", "Context", "Status",
                    ]
                };
                table
                    .header(HEADER_HEIGHT, |mut header| {
                        for h in headers {
                            header.col(|ui| {
                                ui.strong(h);
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(ROW_HEIGHT, rows.len(), |mut row| {
                            let r = &rows[row.index()];
                            row.col(|ui| {
                                ui.label(format_date(r.date));
                            });
                            row.col(|ui| {
                                ui.label(&r.account);
                            });
                            row.col(|ui| {
                                ui.label(&r.payee);
                            });
                            row.col(|ui| {
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(amount_text(r.amount));
                                });
                            });
                            row.col(|ui| {
                                ui.label(RichText::new(&r.memo).weak());
                            });
                            row.col(|ui| {
                                ui.label(&r.contexts);
                            });
                            row.col(|ui| {
                                ui.label(r.status);
                            });
                            if scenario_active {
                                row.col(|ui| {
                                    if r.scenario == "Reality" {
                                        ui.label(RichText::new("Reality").weak());
                                    } else {
                                        ui.label(RichText::new(&r.scenario).color(colors::BLUE));
                                    }
                                });
                            }
                        });
                    });
            });
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
            with_hscroll(ui, "report_budget_items_hscroll", |ui| {
                TableBuilder::new(ui)
                    .striped(true)
                    .auto_shrink([false, false])
                    .max_scroll_height(360.0)
                    .column(Column::initial(170.0))
                    .column(Column::initial(150.0))
                    .column(Column::initial(90.0))
                    .column(Column::initial(110.0))
                    .column(Column::initial(100.0))
                    .column(Column::initial(100.0))
                    .column(Column::initial(130.0))
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
            });
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

/// Draws a drill-down table with a sideways scroll. The dialog is rebuilt every frame, so the scroll
/// position is kept in egui's memory under `salt`.
fn with_hscroll(ui: &mut Ui, salt: &'static str, add: impl FnOnce(&mut Ui)) {
    let id = Id::unique(salt);
    let mut state: HScroll = ui
        .ctx()
        .data_mut(|d| *d.get_temp_mut_or_default::<HScroll>(id));
    hscroll::show(ui, &mut state, add);
    ui.ctx().data_mut(|d| d.insert_temp(id, state));
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
