//! The account transaction table (T15a read-only, T15b editable): nine Swift columns drawn with
//! `TableBuilder`. Only the visible rows are drawn. Clicks are collected while drawing and applied
//! to the selection after the table closure, because the table closures borrow `ui`. Edits are
//! recorded as `Pending` commits in `Edits` and applied by the screen after drawing.

use std::collections::BTreeSet;

use egui::{
    Align, Frame, Key, Layout, Modifiers, RichText, Role, ScrollArea, Sense, TextEdit, Ui,
    WidgetInfo,
};
use egui_extras::{Column, DatePickerButton, TableBuilder};
use pf_core::format;
use pf_engine::commands::transactions::TxField;
use pf_model::{ContextId, TransactionId};

use super::edit::{CellEdit, EditCol, Edits, Pending};
use super::menu;
use super::rows::TxRow;
use crate::widgets::amount_field::amount_field;
use crate::widgets::category::PICKER_WIDTH as CATEGORY_PICKER_WIDTH;
use crate::widgets::category::category_picker;
use crate::widgets::col_fit::{fit, longest};
use crate::widgets::colors;
use crate::widgets::contexts::{context_chips, context_toggle_menu};
use crate::widgets::hscroll::{self, HScroll};
use crate::widgets::status::{PICKER_WIDTH, status_picker};

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

/// Draws `rows` (already built and cached), applies row clicks to `selection`, and records the
/// edits the user made in `edits.pending`.
pub fn show(
    ui: &mut Ui,
    rows: &[TxRow],
    selection: &mut BTreeSet<TransactionId>,
    anchor: &mut Option<TransactionId>,
    hscroll: &mut HScroll,
    edits: &mut Edits<'_>,
) {
    if rows.is_empty() {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| ui.label(RichText::new("No transactions").weak()));
        return;
    }
    // A tag menu's row is remembered only while some popup is open (see `contexts_cell`).
    let modifiers = ui.input(|i| i.modifiers);
    let mut clicked: Option<usize> = None;
    // Sideways scroll so every column is reachable in a narrow window (clamped, no rubber band).
    // The table keeps its own vertical scroll and row virtualisation (it gets the viewport height).
    // Each column starts as wide as its widest text (the longest candidates, plus fixed-format
    // templates for amounts and dates).
    let date_w = fit(ui, "Date", &["Sep 30, 2026"]);
    let amount_w = fit(ui, "Amount", &["-$999,999.99"]);
    let category_w = fit(
        ui,
        "Category",
        &longest(rows.iter().map(|r| r.category_label.as_str())),
    )
    .max(CATEGORY_PICKER_WIDTH + 16.0);
    let memo_w = fit(ui, "Memo", &longest(rows.iter().map(|r| r.memo.as_str())));
    let att_w = fit(ui, "Att.", &["Attach"]);
    let context_w = fit(
        ui,
        "Context",
        &longest(
            rows.iter()
                .flat_map(|r| r.contexts.iter().map(|c| c.1.as_str())),
        ),
    );
    let running_w = fit(ui, "Running Balance", &["-$999,999.99"]);
    // The status picker is a fixed-width combo box, so the column is at least that wide.
    let status_w = fit(ui, "Status", &["Reconciled", "Pending"]).max(PICKER_WIDTH + 16.0);
    hscroll::show(ui, hscroll, |ui| {
        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .sense(Sense::click())
            .auto_shrink([false, false])
            .max_scroll_height(f32::INFINITY)
            .column(Column::initial(date_w))
            .column(Column::remainder().at_least(150.0))
            .column(Column::initial(amount_w))
            .column(Column::initial(category_w))
            .column(Column::initial(memo_w))
            .column(Column::initial(att_w))
            .column(Column::initial(context_w))
            .column(Column::initial(running_w))
            .column(Column::initial(status_w))
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
                    cells(&mut row, tx, edits, &mut row_clicked);
                    // A right-click anywhere on the row, even on an editable cell, selects the row if it was not
                    // selected, and opens its menu (T15c).
                    let response = row.response();
                    if response.container_secondary_clicked() && !selection.contains(&tx.id) {
                        selection.clear();
                        selection.insert(tx.id);
                    }
                    response.container_context_menu(|ui| {
                        let picked: Vec<&TxRow> =
                            rows.iter().filter(|r| selection.contains(&r.id)).collect();
                        if let Some(action) = menu::items(ui, &picked) {
                            edits.pending.push(Pending::Menu(action));
                        }
                    });
                    if row_clicked {
                        clicked = Some(index);
                    }
                });
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

/// Draws the nine cells of one row. Editable cells draw their own widgets (disabled when the row is
/// locked, R-TXN-1); the other cells register a click target for row selection.
fn cells(
    row: &mut egui_extras::TableRow<'_, '_>,
    tx: &TxRow,
    edits: &mut Edits<'_>,
    clicked: &mut bool,
) {
    let locked = tx.locked;
    row.col(|ui| date_cell(ui, edits, tx));
    row.col(|ui| text_cell(ui, edits, tx, EditCol::Payee, "Payee", &tx.payee, clicked));
    row.col(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            amount_cell(ui, edits, tx, clicked);
        });
    });
    row.col(|ui| {
        if let Some(c) = category_picker(
            ui,
            ("category", tx.id.raw()),
            tx.category_id,
            edits.categories,
            !locked,
            &format!("Category {}", tx.payee),
        ) {
            edits
                .pending
                .push(Pending::Field(tx.id, TxField::Category(c)));
        }
    });
    row.col(|ui| text_cell(ui, edits, tx, EditCol::Memo, "Memo", &tx.memo, clicked));
    row.col(|ui| attachment_cell(ui, edits, tx));
    row.col(|ui| contexts_cell(ui, edits, tx));
    row.col(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            // R-BAL-8: the search balance is blue while a search filters the rows.
            let shown = RichText::new(format::format_currency(tx.running));
            let shown = if edits.searching {
                shown.color(colors::BLUE)
            } else {
                shown.weak()
            };
            ui.label(shown);
        });
        click_target(ui, clicked);
    });
    row.col(|ui| {
        if let Some(s) = status_picker(
            ui,
            ("status", tx.id.raw()),
            tx.status,
            !locked,
            &format!("Status {}", tx.payee),
        ) {
            edits.pending.push(Pending::Status(tx.id, s));
        }
    });
}

/// The date picker (R-TXN-9 is checked by the engine). A changed date is a commit.
fn date_cell(ui: &mut Ui, edits: &mut Edits<'_>, tx: &TxRow) {
    let mut date = tx.date;
    let salt = format!("date-{}", tx.id.raw());
    let resp = ui.add_enabled(
        !tx.locked,
        DatePickerButton::new(&mut date)
            .id_salt(&salt)
            .show_icon(false),
    );
    resp.widget_info(|| {
        WidgetInfo::labeled(Role::Button, !tx.locked, format!("Date {}", tx.payee))
    });
    if date != tx.date {
        edits
            .pending
            .push(Pending::Field(tx.id, TxField::Date(date)));
    }
}

/// The amount: a click-to-edit label (`amount_field`). The buffer lives in `edits.amount` for the
/// row being edited.
fn amount_cell(ui: &mut Ui, edits: &mut Edits<'_>, tx: &TxRow, clicked: &mut bool) {
    let mine = edits.amount.as_ref().is_some_and(|(id, _)| *id == tx.id);
    let mut buf = edits
        .amount
        .as_ref()
        .filter(|(id, _)| *id == tx.id)
        .map(|(_, s)| s.clone());
    let label = format!("Amount {}", tx.payee);
    if let Some(v) = amount_field(ui, &label, tx.amount, tx.locked, &mut buf) {
        edits
            .pending
            .push(Pending::Field(tx.id, TxField::Amount(v)));
    }
    // A click that starts an amount edit also selects the row (as a click on the payee does).
    if !mine && buf.is_some() {
        *clicked = true;
    }
    if buf.is_some() || mine {
        *edits.amount = buf.map(|s| (tx.id, s));
    }
}

/// A text cell (Payee, Memo). Its buffer lives in `edits.cell` only while the cell has focus.
/// Enter or focus loss commits a changed value; Esc cancels (R-TXN-12). Locked rows are disabled.
/// Gaining focus by a click also selects the row, as in the read-only table (T15a).
fn text_cell(
    ui: &mut Ui,
    edits: &mut Edits<'_>,
    tx: &TxRow,
    col: EditCol,
    label: &str,
    stored: &str,
    clicked: &mut bool,
) {
    let editing = edits
        .cell
        .as_ref()
        .filter(|c| c.id == tx.id && c.col == col)
        .map(|c| c.text.clone());
    let mut text = editing.unwrap_or_else(|| stored.to_string());
    let enabled = !tx.locked;
    let mut edit = TextEdit::singleline(&mut text)
        .frame(Frame::NONE)
        .desired_width(f32::INFINITY);
    if !enabled {
        edit = edit.text_color(ui.visuals().weak_text_color());
    }
    let resp = ui.add_enabled(enabled, edit);
    resp.widget_info(|| {
        WidgetInfo::labeled(Role::TextInput, enabled, format!("{label} {}", tx.payee))
    });
    if resp.gained_focus() {
        *clicked = true;
    }
    // Esc cancels. egui's TextEdit surrenders focus on Esc itself, so the key is checked whenever
    // the field had focus this frame or just lost it.
    let escape =
        ui.input(|i| i.key_pressed(Key::Escape)) && (resp.has_focus() || resp.lost_focus());
    if escape {
        *edits.cell = None;
        resp.surrender_focus();
    } else if resp.has_focus() {
        *edits.cell = Some(CellEdit {
            id: tx.id,
            col,
            text,
        });
    } else if resp.lost_focus()
        && let Some(c) = edits.cell.take().filter(|c| c.id == tx.id && c.col == col)
        && c.text != stored
    {
        let field = match col {
            EditCol::Payee => TxField::Payee(c.text),
            EditCol::Memo => TxField::Memo(if c.text.is_empty() {
                None
            } else {
                Some(c.text)
            }),
        };
        edits.pending.push(Pending::Field(tx.id, field));
    }
}

/// True while the pointer is over this row's band of the table. The hover buttons (Attach, Open,
/// remove, the tag "+") show only then, so the columns stay quiet.
fn row_hovered(ui: &Ui) -> bool {
    let band = ui.max_rect();
    ui.ctx()
        .pointer_hover_pos()
        .is_some_and(|p| ui.clip_rect().contains(p) && p.y >= band.top() && p.y <= band.bottom())
}

/// The attachment. Without the hover buttons it shows a weak "Yes" when a file is attached. With
/// them: "Open" (the hover text is the path), "×" to remove when the row is unlocked, and "Attach"
/// when there is no file (T15c, R-TXN-14).
fn attachment_cell(ui: &mut Ui, edits: &mut Edits<'_>, tx: &TxRow) {
    if !row_hovered(ui) {
        if tx.attachment.is_some() {
            ui.label(RichText::new("Yes").weak());
        }
        return;
    }
    ui.horizontal(|ui| match &tx.attachment {
        Some(path) => {
            if ui.button("Open").on_hover_text(path.as_str()).clicked() {
                edits.pending.push(Pending::OpenAttachment(path.clone()));
            }
            if !tx.locked
                && ui
                    .small_button("×")
                    .on_hover_text("Remove attachment")
                    .clicked()
            {
                edits
                    .pending
                    .push(Pending::Field(tx.id, TxField::Attachment(None)));
            }
        }
        None => {
            if !tx.locked
                && ui
                    .small_button("Attach")
                    .on_hover_text("Attach file")
                    .clicked()
            {
                edits.pending.push(Pending::Attach(tx.id));
            }
        }
    });
}

/// The context chips in a scroll area, then the "+" menu (unlocked rows only). The "+" shows on hover,
/// and stays while its own menu is open.
fn contexts_cell(ui: &mut Ui, edits: &mut Edits<'_>, tx: &TxRow) {
    let show_menu = !tx.locked && (row_hovered(ui) || *edits.tag_open == Some(tx.id));
    ui.horizontal(|ui| {
        let width = if show_menu {
            (ui.available_width() - 28.0).max(0.0)
        } else {
            ui.available_width()
        };
        ScrollArea::horizontal()
            .id_salt(("contexts", tx.id.raw()))
            .max_width(width)
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| context_chips(ui, &tx.contexts, tx.locked));
        if show_menu {
            let ids: Vec<ContextId> = tx.contexts.iter().map(|(id, _, _)| *id).collect();
            let menu = context_toggle_menu(ui, &ids, edits.contexts);
            // Keep this row's "+" shown while its menu is open; the pointer leaves the row to reach it.
            if menu.open {
                *edits.tag_open = Some(tx.id);
            } else if *edits.tag_open == Some(tx.id) {
                *edits.tag_open = None;
            }
            if let Some(list) = menu.changed {
                edits
                    .pending
                    .push(Pending::Field(tx.id, TxField::Contexts(list)));
            }
        }
    });
}

/// Registers a click target over the whole cell, after its contents. Used by the non-editable
/// cells, whose labels are hover-only, so without this the cell never gets the click (egui
/// hit-testing) and the row would not select. Sets `clicked` when the cell is clicked.
fn click_target(ui: &mut Ui, clicked: &mut bool) {
    let rect = ui.max_rect();
    let id = ui.scope_id().with("row_click");
    if ui.interact(rect, id, Sense::click()).clicked() {
        *clicked = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::TransactionStatus;
    use pf_model::testing::d;
    use rust_decimal::Decimal;

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
