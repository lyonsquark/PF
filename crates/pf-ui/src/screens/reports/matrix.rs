//! The Monthly Matrix report (R-RPT-5): the category names on the left and the headings on top stay in
//! place while the amounts scroll under them. The names and the amounts are two tables side by side,
//! because a table cannot freeze a column; they share one vertical scroll offset.

use egui::scroll_area::ScrollBarVisibility;
use egui::{Align, Button, Layout, RichText, TextStyle, Ui, UiBuilder};
use egui_extras::{Column, TableBuilder};
use pf_core::format;
use pf_core::reports::{
    MONTH_SHORT, MatrixCell, MatrixMode, MonthlyMatrix, Tone, matrix_cell, matrix_drill,
    matrix_drill_through, matrix_drill_title, tone,
};
use pf_model::{Db, ScenarioId, TransactionId};
use rust_decimal::Decimal;

use super::{Drill, money, with_right};
use crate::widgets::col_fit::{fit, longest};
use crate::widgets::colors;
use crate::widgets::hscroll::{self, HScroll};

/// The narrowest the category column starts, in points.
const LABEL_WIDTH: f32 = 140.0;
/// Width of a month or total column, in points.
const CELL_WIDTH: f32 = 90.0;
/// Height of a one-line row, in points.
const ROW_HEIGHT: f32 = 22.0;
/// Height of the heading row, in points.
const HEADER_HEIGHT: f32 = 20.0;

/// Where the matrix is scrolled to. Kept in the screen's state.
#[derive(Default)]
pub(super) struct MatrixScroll {
    /// The amounts' sideways scroll.
    horizontal: HScroll,
    /// How far both tables are scrolled down, in points.
    vertical: f32,
}

/// What the matrix draws.
pub(super) struct MatrixView<'a> {
    /// The database, for the drill-downs.
    pub db: &'a Db,
    /// The numbers, already cut off at `cutoff` (R-RPT-5f).
    pub grid: &'a MonthlyMatrix,
    /// What each cell shows.
    pub mode: MatrixMode,
    /// The active scenario.
    pub scenario: Option<ScenarioId>,
    /// The last month shown, 1..=12 (R-RPT-5f).
    pub cutoff: u8,
}

/// The monthly matrix: one row per category with its months up to the cutoff, the total, and the NET
/// row. Each cell is a button that opens the transactions behind it (R-RPT-5); the NET row is plain.
/// The headings stay put when scrolling down, and the category names when scrolling sideways.
pub(super) fn matrix_body(
    ui: &mut Ui,
    view: &MatrixView<'_>,
    scroll: &mut MatrixScroll,
    drill: &mut Option<Drill>,
) {
    let MatrixView { grid, mode, .. } = *view;
    let label_w = fit(
        ui,
        "Category",
        &longest(grid.rows.iter().map(|r| r.label.as_str())),
    )
    .max(LABEL_WIDTH);
    // R-RPT-5d: only a cell with a budget, in Actual + Budget mode, has three lines.
    let small = ui.text_style_height(&TextStyle::Small);
    let spacing = ui.spacing().item_spacing;
    let tall = ui.spacing().interact_size.y + 2.0 * (small + spacing.y) + spacing.y;
    let height = |budget: &[Decimal; 12]| {
        if mode == MatrixMode::All && budget.iter().any(|b| !b.is_zero()) {
            tall
        } else {
            ROW_HEIGHT
        }
    };
    let heights: Vec<f32> = grid
        .rows
        .iter()
        .map(|r| height(&r.budget))
        .chain(std::iter::once(height(&grid.net_budget)))
        .collect();

    let space = ui.available_rect_before_wrap();
    let mut names = space;
    names.set_width(label_w);
    let mut amounts = space;
    amounts.set_left(names.right() + spacing.x);

    // A sideways swipe over the names moves the amounts too (`hscroll` watches only its own area).
    if ui.rect_contains_pointer(names) {
        scroll.horizontal.offset -= ui.input(|i| i.smooth_scroll_delta.x);
    }
    let names_offset = ui
        .scope_builder(UiBuilder::new().max_rect(names), |ui| {
            names_table(ui, grid, &heights, label_w, scroll.vertical)
        })
        .inner;
    let mut amounts_offset = names_offset;
    ui.scope_builder(UiBuilder::new().max_rect(amounts), |ui| {
        hscroll::show(ui, &mut scroll.horizontal, |ui| {
            amounts_offset = amounts_table(ui, view, &heights, names_offset, drill);
        });
    });
    // A scroll over the amounts reaches the names on the next frame.
    if (amounts_offset - names_offset).abs() > f32::EPSILON {
        ui.ctx().request_repaint();
    }
    scroll.vertical = amounts_offset;
}

/// The frozen column of category names, ending with NET. Returns how far it is scrolled down.
fn names_table(ui: &mut Ui, grid: &MonthlyMatrix, heights: &[f32], width: f32, offset: f32) -> f32 {
    TableBuilder::new(ui)
        .id_salt("reports_matrix_names")
        .auto_shrink([false, false])
        .max_scroll_height(f32::INFINITY)
        .vertical_scroll_offset(offset)
        .scroll_bar_visibility(ScrollBarVisibility::AlwaysHidden)
        .column(Column::exact(width).clip(true))
        .header(HEADER_HEIGHT, |mut header| {
            header.col(|ui| {
                ui.strong("Category");
            });
        })
        .body(|body| {
            body.heterogeneous_rows(heights.iter().copied(), |mut table_row| {
                let row = grid.rows.get(table_row.index());
                table_row.col(|ui| {
                    ui.with_layout(Layout::top_down(Align::Min), |ui| match row {
                        Some(row) => {
                            ui.add(egui::Label::new(&row.label).truncate());
                        }
                        None => {
                            ui.label(RichText::new("NET").strong());
                        }
                    });
                });
            });
        })
        .state
        .clamped_offset()
        .y
}

/// The months up to the cutoff and the total of every row. Returns how far it is scrolled down.
fn amounts_table(
    ui: &mut Ui,
    view: &MatrixView<'_>,
    heights: &[f32],
    offset: f32,
    drill: &mut Option<Drill>,
) -> f32 {
    let MatrixView {
        db,
        grid,
        mode,
        scenario,
        cutoff,
    } = *view;
    let shown = usize::from(cutoff).min(12);
    TableBuilder::new(ui)
        .id_salt("reports_matrix")
        .resizable(true)
        .auto_shrink([false, false])
        .max_scroll_height(f32::INFINITY)
        .vertical_scroll_offset(offset)
        .columns(Column::initial(CELL_WIDTH), shown + 1)
        .header(HEADER_HEIGHT, |mut header| {
            for h in MONTH_SHORT.into_iter().take(shown).chain(["Total"]) {
                header.col(|ui| {
                    with_right(ui, |ui| {
                        ui.strong(h);
                    });
                });
            }
        })
        .body(|body| {
            body.heterogeneous_rows(heights.iter().copied(), |mut table_row| {
                let Some(row) = grid.rows.get(table_row.index()) else {
                    for m in 0..shown {
                        let cell = matrix_cell(mode, grid.net_actual[m], grid.net_budget[m]);
                        table_row.col(|ui| {
                            cell_ui::<fn() -> Vec<TransactionId>>(ui, &cell, None, drill);
                        });
                    }
                    let total_actual: Decimal = grid.net_actual.iter().sum();
                    let total_budget: Decimal = grid.net_budget.iter().sum();
                    let cell = matrix_cell(mode, total_actual, total_budget);
                    table_row.col(|ui| {
                        cell_ui::<fn() -> Vec<TransactionId>>(ui, &cell, None, drill);
                    });
                    return;
                };
                for m in 0..shown {
                    let month = u8::try_from(m + 1).unwrap_or(1);
                    let cell = matrix_cell(mode, row.actual[m], row.budget[m]);
                    let click = Click {
                        title: matrix_drill_title(&row.label, grid.year, Some(month)),
                        ids: move || {
                            matrix_drill(db, grid.year, scenario, row.category_id, Some(month))
                        },
                    };
                    table_row.col(|ui| cell_ui(ui, &cell, Some(click), drill));
                }
                let cell = matrix_cell(mode, row.actual_total(), row.budget_total());
                let click = Click {
                    title: matrix_drill_title(&row.label, grid.year, None),
                    ids: move || {
                        matrix_drill_through(db, grid.year, scenario, row.category_id, cutoff)
                    },
                };
                table_row.col(|ui| cell_ui(ui, &cell, Some(click), drill));
            });
        })
        .state
        .clamped_offset()
        .y
}

/// What a matrix cell opens when clicked: its heading, and the transactions behind it (computed only
/// on the click).
struct Click<F: FnOnce() -> Vec<TransactionId>> {
    title: String,
    ids: F,
}

/// One matrix cell. A dash for nothing; one amount; or actual with its budget and difference. The
/// actual is the button that opens the drill-down; a cell without a `click` (the NET row) is plain.
fn cell_ui<F: FnOnce() -> Vec<TransactionId>>(
    ui: &mut Ui,
    cell: &MatrixCell,
    click: Option<Click<F>>,
    drill: &mut Option<Drill>,
) {
    let mut click = click;
    ui.with_layout(Layout::top_down(Align::Max), |ui| {
        let mut amount = |ui: &mut Ui, value: Decimal| {
            if click.is_some() {
                if clickable_amount(ui, value, false).clicked()
                    && let Some(c) = click.take()
                {
                    *drill = Some(Drill::Transactions {
                        title: c.title,
                        ids: (c.ids)(),
                    });
                }
            } else {
                money(ui, value, false);
            }
        };
        match cell {
            MatrixCell::Dash => {
                ui.label(RichText::new("—").weak());
            }
            MatrixCell::One(v) => amount(ui, *v),
            MatrixCell::Three {
                actual,
                budget,
                diff,
            } => {
                amount(ui, *actual);
                ui.label(
                    RichText::new(format::format_currency(*budget))
                        .small()
                        .weak(),
                );
                let diff_color = if *diff >= Decimal::ZERO {
                    colors::GREEN
                } else {
                    colors::ORANGE
                };
                ui.label(
                    RichText::new(format::format_currency(*diff))
                        .small()
                        .color(diff_color),
                );
            }
        }
    });
}

/// An amount as a button, coloured by its sign.
fn clickable_amount(ui: &mut Ui, amount: Decimal, bold: bool) -> egui::Response {
    let mut text = RichText::new(format::format_currency(amount));
    if bold {
        text = text.strong();
    }
    text = match tone(amount) {
        Tone::Positive => text.color(colors::GREEN),
        Tone::Negative => text.color(colors::RED),
        Tone::Zero => text,
    };
    ui.add(Button::new(text).frame(false))
}
