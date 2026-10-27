//! The read-only transaction table (T15a): nine Swift columns drawn with `TableBuilder`. Only the
//! visible rows are drawn. Clicks are collected while drawing and applied to the selection after
//! the table closure, because the table closures borrow `ui`.

use std::collections::BTreeSet;

use egui::{
    Align, Color32, Frame, Label, Layout, Margin, Modifiers, RichText, ScrollArea, Sense, Ui,
};
use egui_extras::{Column, TableBuilder};
use pf_core::format;
use pf_model::{TransactionId, TransactionStatus};
use rust_decimal::Decimal;

use super::rows::TxRow;
use crate::widgets::{color_from_hex, colors};

/// Row height in points; it fits a ComboBox (T15b).
const ROW_HEIGHT: f32 = 24.0;
/// Header row height in points.
const HEADER_HEIGHT: f32 = 20.0;
/// The header labels, in column order. The paperclip (T15a spec) is not in egui's fonts, so the
/// attachment column uses plain text (user decision: no glyphs the default fonts lack).
const HEADERS: [&str; 9] = [
    "Date",
    "Payee",
    "Amount",
    "Category",
    "Memo",
    "Att.",
    "Context",
    "Running Balance",
    "Status",
];

/// Draws `rows` (already built and cached) and applies row clicks to `selection`.
pub fn show(
    ui: &mut Ui,
    rows: &[TxRow],
    selection: &mut BTreeSet<TransactionId>,
    anchor: &mut Option<TransactionId>,
) {
    if rows.is_empty() {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| ui.label(RichText::new("No transactions").weak()));
        return;
    }
    let modifiers = ui.input(|i| i.modifiers);
    let mut clicked: Option<usize> = None;
    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .sense(Sense::click())
        .auto_shrink([false, false])
        .max_scroll_height(f32::INFINITY)
        .column(Column::initial(110.0))
        .column(Column::remainder().at_least(150.0))
        .column(Column::initial(100.0))
        .column(Column::initial(160.0))
        .column(Column::initial(140.0))
        .column(Column::initial(36.0))
        .column(Column::initial(170.0))
        .column(Column::initial(110.0))
        .column(Column::initial(100.0))
        .header(HEADER_HEIGHT, |mut header| {
            for title in HEADERS {
                header.col(|ui| {
                    ui.strong(title);
                });
            }
        })
        .body(|body| {
            body.rows(ROW_HEIGHT, rows.len(), |mut row| {
                let index = row.index();
                let tx = &rows[index];
                row.set_selected(selection.contains(&tx.id));
                let mut row_clicked = false;
                cells(&mut row, tx, &mut row_clicked);
                if row_clicked {
                    clicked = Some(index);
                }
            });
        });
    if let Some(index) = clicked {
        apply_click(rows, selection, anchor, index, modifiers);
    }
}

/// Selection rules (T15a): click selects only that row; ⌘-click toggles it; ⇧-click selects the
/// range from the anchor to this row in the current order (replacing the selection). The anchor
/// follows plain and ⌘ clicks.
fn apply_click(
    rows: &[TxRow],
    selection: &mut BTreeSet<TransactionId>,
    anchor: &mut Option<TransactionId>,
    index: usize,
    modifiers: Modifiers,
) {
    let id = rows[index].id;
    if modifiers.shift {
        let from = anchor.and_then(|a| rows.iter().position(|r| r.id == a));
        if let Some(from) = from {
            let (lo, hi) = (from.min(index), from.max(index));
            selection.clear();
            selection.extend(rows[lo..=hi].iter().map(|r| r.id));
            return;
        }
    }
    if modifiers.command {
        if !selection.remove(&id) {
            selection.insert(id);
        }
    } else {
        selection.clear();
        selection.insert(id);
    }
    *anchor = Some(id);
}

/// Draws the nine cells of one row. A locked row renders its text weak (R-TXN-1). `clicked` is set
/// when any cell is clicked (see `click_target`).
fn cells(row: &mut egui_extras::TableRow<'_, '_>, tx: &TxRow, clicked: &mut bool) {
    let locked = tx.locked;
    row.col(|ui| {
        text(ui, &tx.date.to_string(), locked);
        click_target(ui, clicked);
    });
    row.col(|ui| {
        text(ui, &tx.payee, locked);
        click_target(ui, clicked);
    });
    row.col(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let value = format::format_currency(tx.amount);
            if tx.amount >= Decimal::ZERO {
                ui.label(RichText::new(value).color(fade(colors::GREEN, locked)));
            } else {
                text(ui, &value, locked);
            }
        });
        click_target(ui, clicked);
    });
    row.col(|ui| {
        if tx.category_id.is_none() {
            ui.label(RichText::new(&tx.category_label).weak());
        } else {
            text(ui, &tx.category_label, locked);
        }
        click_target(ui, clicked);
    });
    row.col(|ui| {
        text(ui, &tx.memo, locked);
        click_target(ui, clicked);
    });
    row.col(|ui| {
        if tx.attachment.is_some() {
            ui.label("Yes");
        }
        click_target(ui, clicked);
    });
    row.col(|ui| {
        contexts(ui, tx);
        click_target(ui, clicked);
    });
    row.col(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(format::format_currency(tx.running)).weak());
        });
        click_target(ui, clicked);
    });
    row.col(|ui| {
        status(ui, tx.status, locked);
        click_target(ui, clicked);
    });
}

/// Registers a click target over the whole cell, after its contents. The labels inside are
/// hover-only, so without this the cell never gets the click (egui hit-testing) and the row would
/// not select. Sets `clicked` when the cell is clicked.
fn click_target(ui: &mut Ui, clicked: &mut bool) {
    let rect = ui.max_rect();
    let id = ui.scope_id().with("row_click");
    if ui.interact(rect, id, Sense::click()).clicked() {
        *clicked = true;
    }
}

/// Plain text, weak when the row is locked.
fn text(ui: &mut Ui, s: &str, locked: bool) {
    let rich = RichText::new(s);
    if locked {
        ui.add(Label::new(rich.weak()));
    } else {
        ui.add(Label::new(rich));
    }
}

/// The colour for a locked row: 60 % alpha, as Swift's `Color.green.opacity(0.6)` (AmountEditField).
fn fade(color: Color32, locked: bool) -> Color32 {
    if locked {
        color.gamma_multiply(0.6)
    } else {
        color
    }
}

/// The status label and its colour (Swift `statusColor`).
fn status(ui: &mut Ui, status: TransactionStatus, locked: bool) {
    let (label, color) = match status {
        TransactionStatus::Cleared => ("Cleared", None),
        TransactionStatus::Pending => ("Pending", Some(colors::GREEN)),
        TransactionStatus::Reconciled => ("Reconciled", Some(colors::BROWN)),
        TransactionStatus::Future => ("Future", Some(colors::RED)),
    };
    match color {
        Some(c) => {
            ui.label(RichText::new(label).color(fade(c, locked)));
        }
        None => text(ui, label, locked),
    }
}

/// The context chips in a horizontal scroll area with the bars hidden. "No Context" when none.
fn contexts(ui: &mut Ui, tx: &TxRow) {
    if tx.contexts.is_empty() {
        ui.label(RichText::new("No Context").weak());
        return;
    }
    ScrollArea::horizontal()
        .id_salt(("contexts", tx.id.raw()))
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for (_, name, hex) in &tx.contexts {
                    let color = fade(color_from_hex(hex.as_deref()), tx.locked);
                    chip(ui, name, color);
                }
            });
        });
}

/// One context chip: "#name" on a tinted rounded frame (fill at 15 % of the colour).
fn chip(ui: &mut Ui, name: &str, color: Color32) {
    Frame::new()
        .fill(color.gamma_multiply(0.15))
        .corner_radius(6.0)
        .inner_margin(Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(format!("#{name}")).small().color(color));
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::testing::d;

    fn row(id: i64) -> TxRow {
        TxRow {
            id: TransactionId(id),
            date: d("2026-03-01"),
            payee: format!("p{id}"),
            amount: Decimal::ZERO,
            category_id: None,
            category_label: "None".into(),
            memo: String::new(),
            attachment: None,
            contexts: Vec::new(),
            running: Decimal::ZERO,
            status: TransactionStatus::Cleared,
            is_scenario: false,
            locked: false,
        }
    }

    fn ids(s: &BTreeSet<TransactionId>) -> Vec<i64> {
        s.iter().map(|t| t.raw()).collect()
    }

    #[test]
    /// Covers: T15a selection. Plain click selects one; ⌘-click toggles; ⇧-click ranges from the
    /// anchor (replacing the selection) and keeps the anchor.
    fn apply_click_plain_command_and_shift() {
        let rows: Vec<TxRow> = (1..=5).map(row).collect();
        let mut sel = BTreeSet::new();
        let mut anchor = None;
        let plain = Modifiers::NONE;
        let cmd = Modifiers::COMMAND;
        let shift = Modifiers::SHIFT;

        apply_click(&rows, &mut sel, &mut anchor, 0, plain);
        assert_eq!(ids(&sel), vec![1]);
        apply_click(&rows, &mut sel, &mut anchor, 1, cmd);
        assert_eq!(ids(&sel), vec![1, 2]);
        apply_click(&rows, &mut sel, &mut anchor, 1, cmd);
        assert_eq!(ids(&sel), vec![1], "⌘-click toggles off");
        apply_click(&rows, &mut sel, &mut anchor, 1, cmd);
        apply_click(&rows, &mut sel, &mut anchor, 4, shift);
        assert_eq!(ids(&sel), vec![2, 3, 4, 5]);
        assert_eq!(anchor, Some(TransactionId(2)));
        apply_click(&rows, &mut sel, &mut anchor, 2, plain);
        assert_eq!(ids(&sel), vec![3]);
    }
}
