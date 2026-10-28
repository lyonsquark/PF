//! The Orphaned Transactions dialog (T21, Settings → Danger Zone). Lists live transactions with no
//! account (R-TXN-15) and offers one undoable Delete All.

use egui::{Id, Modal, RichText, Ui};
use egui_extras::{Column, TableBuilder};
use pf_core::format::format_currency;
use pf_engine::commands::transactions;
use pf_model::Db;

use crate::app::UiCtx;
use crate::dialogs::Outcome;
use crate::widgets::col_fit::{fit, longest};
use crate::widgets::colors;

/// One table row, with every cell already converted to text.
struct Row {
    date: String,
    payee: String,
    amount: rust_decimal::Decimal,
    category: String,
    memo: String,
    status: &'static str,
    /// Scenario name, or `None` for Reality.
    scenario: Option<String>,
}

/// The orphan rows, newest first (the engine's order, R-TXN-15).
fn rows_of(db: &Db) -> Vec<Row> {
    transactions::orphaned_transactions(db)
        .into_iter()
        .filter_map(|id| db.transaction(id))
        .map(|t| Row {
            date: t.date.to_string(),
            payee: t.payee.clone(),
            amount: t.amount,
            category: t
                .category_id
                .map_or_else(|| "None".to_string(), |c| db.category_full_name(c)),
            memo: t.memo.clone().unwrap_or_default(),
            status: t.status.label(),
            scenario: t
                .scenario_id
                .and_then(|s| db.scenario(s))
                .map(|s| s.name.clone()),
        })
        .collect()
}

/// Draws the dialog. Cancel, Escape or a click outside closes it. Delete All soft-deletes every
/// orphan in one undo step, then closes it.
pub fn show(cx: &mut UiCtx, ui: &mut Ui) -> Outcome {
    let rows = rows_of(cx.engine.db());
    let mut cancel = false;
    let mut delete_all = false;
    let response = Modal::new(Id::unique("orphaned")).show(ui.ctx(), |ui| {
        ui.set_min_width(680.0);
        ui.horizontal(|ui| {
            ui.heading("Orphaned Transactions");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(!rows.is_empty(), egui::Button::new("Delete All"))
                    .clicked()
                {
                    delete_all = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
        ui.add_space(8.0);
        if rows.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(24.0);
                ui.strong("No Orphaned Transactions");
                ui.label("All transactions have a valid account.");
                ui.add_space(24.0);
            });
        } else {
            ui.weak(format!("{} transaction(s) with no account", rows.len()));
            table(ui, &rows);
        }
    });
    if delete_all {
        if let Err(e) = transactions::delete_orphaned(cx.engine) {
            cx.dialogs.alert("Data Management", e.to_string());
        }
        return Outcome::Close;
    }
    if cancel || response.should_close() {
        Outcome::Close
    } else {
        Outcome::KeepOpen
    }
}

/// The virtualised table: Date, Payee, Amount, Category, Memo, Status, Scenario.
fn table(ui: &mut Ui, rows: &[Row]) {
    // Each column starts as wide as its widest text (see `widgets::col_fit`); Scenario takes the rest.
    let date_w = fit(ui, "Date", &["Sep 30, 2026"]);
    let payee_w = fit(ui, "Payee", &longest(rows.iter().map(|r| r.payee.as_str())));
    let amount_w = fit(ui, "Amount", &["-$999,999.99"]);
    let category_w = fit(
        ui,
        "Category",
        &longest(rows.iter().map(|r| r.category.as_str())),
    );
    let memo_w = fit(ui, "Memo", &longest(rows.iter().map(|r| r.memo.as_str())));
    let status_w = fit(ui, "Status", &["Reconciled", "Pending"]);
    TableBuilder::new(ui)
        .striped(true)
        .max_scroll_height(360.0)
        .column(Column::initial(date_w))
        .column(Column::initial(payee_w))
        .column(Column::initial(amount_w))
        .column(Column::initial(category_w))
        .column(Column::initial(memo_w))
        .column(Column::initial(status_w))
        .column(Column::remainder())
        .header(20.0, |mut header| {
            for title in [
                "Date", "Payee", "Amount", "Category", "Memo", "Status", "Scenario",
            ] {
                header.col(|ui| {
                    ui.strong(title);
                });
            }
        })
        .body(|body| {
            body.rows(22.0, rows.len(), |mut row| {
                let r = &rows[row.index()];
                row.col(|ui| {
                    ui.label(&r.date);
                });
                row.col(|ui| {
                    ui.label(&r.payee);
                });
                row.col(|ui| {
                    let color = if r.amount < rust_decimal::Decimal::ZERO {
                        colors::RED
                    } else {
                        colors::GREEN
                    };
                    ui.colored_label(color, format_currency(r.amount));
                });
                row.col(|ui| {
                    ui.label(&r.category);
                });
                row.col(|ui| {
                    ui.label(&r.memo);
                });
                row.col(|ui| {
                    ui.label(r.status);
                });
                row.col(|ui| match &r.scenario {
                    Some(name) => {
                        ui.colored_label(colors::BLUE, name);
                    }
                    None => {
                        ui.label(RichText::new("Reality").color(ui.visuals().weak_text_color()));
                    }
                });
            });
        });
}
