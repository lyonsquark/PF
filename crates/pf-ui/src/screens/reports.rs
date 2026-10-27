//! Reports screen (T23, Swift `ReportsView`): Profit & Loss, Balance Sheet and the Monthly Matrix. The
//! numbers come from `pf_core::reports` (cached per engine revision); this file lays them out and wires
//! the clicks to the drill-down dialogs. The wide tables scroll sideways with the clamped `hscroll`.

use egui::{Align, Button, ComboBox, Frame, Layout, RichText, Role, Ui, WidgetInfo};
use egui_extras::{Column, DatePickerButton, TableBuilder};
use jiff::civil::Date;
use pf_core::format;
use pf_core::reports::{
    BalanceSheet, MONTH_SHORT, MatrixCell, MatrixMode, MonthlyMatrix, PlRow, QUICK_SELECT,
    ReportKind, Tone, balance_sheet, budget_drill_title, default_pnl_range, matrix_cell,
    matrix_drill, matrix_drill_title, matrix_year_options, monthly_matrix, pnl_drill_title,
    profit_and_loss, quick_select_range, tone,
};
use pf_model::{BudgetItemId, Db, ScenarioId, TransactionId};
use rust_decimal::Decimal;

use crate::app::UiCtx;
use crate::caches::Cached;
use crate::dialogs::Dialog;
use crate::dialogs::report_drilldown::open_transactions;
use crate::widgets::colors;
use crate::widgets::hscroll::{self, HScroll};

/// Width of the category column in the matrix, in points.
const MATRIX_LABEL_WIDTH: f32 = 140.0;
/// Width of a month or total column in the matrix, in points.
const MATRIX_CELL_WIDTH: f32 = 90.0;
/// Width of the balance sheet, in points.
const BS_WIDTH: f32 = 700.0;

/// The report state. It lives in `ScreenStates.reports`, so it survives switching screens.
#[derive(Default)]
pub struct State {
    /// The report shown.
    pub report: ReportKind,
    /// The date range of Profit & Loss (and the as-of date of the balance sheet, its end). `None` until
    /// the screen is first shown, then the R-RPT default.
    pub range: Option<(Date, Date)>,
    /// The matrix year. `None` means this year.
    pub matrix_year: Option<i16>,
    /// The matrix's display mode.
    pub matrix_mode: MatrixMode,
    pl_hscroll: HScroll,
    matrix_hscroll: HScroll,
    pnl: Cached<(Option<ScenarioId>, Date, Date), Vec<PlRow>>,
    bs: Cached<(Option<ScenarioId>, Date), BalanceSheet>,
    matrix: Cached<(Option<ScenarioId>, i16), MonthlyMatrix>,
}

/// A drill-down the user asked for, opened after the report is drawn.
enum Drill {
    Transactions {
        title: String,
        ids: Vec<TransactionId>,
    },
    BudgetItems {
        title: String,
        ids: Vec<BudgetItemId>,
        start: Date,
        end: Date,
    },
}

/// Draws the Reports screen into the central panel.
pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut Ui) {
    let today = cx.engine.today();
    let revision = cx.engine.revision();
    let scenario = cx.engine.state.active_scenario;
    let (mut start, mut end) = st.range.unwrap_or_else(|| default_pnl_range(today));
    let mut drill: Option<Drill> = None;

    ui.heading("Reports");
    ui.horizontal(|ui| {
        for kind in ReportKind::ALL {
            ui.selectable_value(&mut st.report, kind, kind.label());
        }
    });
    // One row of controls: the quick buttons right-align, and they must not take the table's height.
    ui.horizontal(|ui| match st.report {
        ReportKind::ProfitAndLoss => {
            date_row(ui, "Start", "report_start", &mut start);
            ui.label("-");
            date_row(ui, "End", "report_end", &mut end);
            quick_buttons(ui, today, &mut start, &mut end);
        }
        ReportKind::BalanceSheet => {
            date_row(ui, "As of", "report_asof", &mut end);
        }
        ReportKind::MonthlyMatrix => {
            matrix_controls(ui, today, st);
        }
    });
    st.range = Some((start, end));
    ui.separator();

    let State {
        report,
        matrix_year,
        matrix_mode,
        pl_hscroll,
        matrix_hscroll,
        pnl,
        bs,
        matrix,
        ..
    } = st;
    let db = cx.engine.db();
    match report {
        ReportKind::ProfitAndLoss => {
            let rows = pnl.get(revision, &(scenario, start, end), || {
                profit_and_loss(db, start, end, scenario)
            });
            profit_and_loss_body(ui, rows, start, end, pl_hscroll, &mut drill);
        }
        ReportKind::BalanceSheet => {
            let sheet = bs.get(revision, &(scenario, end), || {
                balance_sheet(db, end, scenario)
            });
            balance_sheet_body(ui, sheet);
        }
        ReportKind::MonthlyMatrix => {
            let year = matrix_year.unwrap_or(today.year());
            let grid = matrix.get(revision, &(scenario, year), || {
                monthly_matrix(db, year, scenario)
            });
            matrix_body(
                ui,
                db,
                grid,
                *matrix_mode,
                scenario,
                matrix_hscroll,
                &mut drill,
            );
        }
    }

    match drill {
        None => {}
        Some(Drill::Transactions { title, ids }) => open_transactions(cx, title, ids),
        Some(Drill::BudgetItems {
            title,
            ids,
            start,
            end,
        }) => cx.dialogs.open(Dialog::ReportBudgetItems {
            title,
            ids,
            start,
            end,
        }),
    }
}

/// A labelled date picker, with a stable id for each.
fn date_row(ui: &mut Ui, label: &str, salt: &str, date: &mut Date) {
    ui.label(label);
    ui.add(DatePickerButton::new(date).id_salt(salt).show_icon(false));
}

/// The quick ranges (1M, 60D, YTD, 1Y): each sets the start from its period, and the end to today.
fn quick_buttons(ui: &mut Ui, today: Date, start: &mut Date, end: &mut Date) {
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        for (label, period) in QUICK_SELECT.iter().rev() {
            if ui.button(*label).clicked() {
                let (s, e) = quick_select_range(*period, today, *start);
                *start = s;
                *end = e;
            }
        }
    });
}

/// The matrix's mode and year pickers.
fn matrix_controls(ui: &mut Ui, today: Date, st: &mut State) {
    let year = st.matrix_year.unwrap_or(today.year());
    ui.horizontal(|ui| {
        let mode = ComboBox::from_id_salt("matrix_mode")
            .selected_text(st.matrix_mode.label())
            .show_ui(ui, |ui| {
                for mode in MatrixMode::ALL {
                    ui.selectable_value(&mut st.matrix_mode, mode, mode.label());
                }
            });
        mode.response
            .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, "Matrix Mode"));
        ui.label("Year");
        let mut chosen = year;
        let year_box = ComboBox::from_id_salt("matrix_year")
            .selected_text(year.to_string())
            .show_ui(ui, |ui| {
                for y in matrix_year_options(today.year()) {
                    ui.selectable_value(&mut chosen, y, y.to_string());
                }
            });
        year_box
            .response
            .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, "Matrix Year"));
        st.matrix_year = Some(chosen);
    });
}

/// The money text and its colour (R-RPT-2): green above zero, red below, plain at zero.
fn money(ui: &mut Ui, amount: Decimal, bold: bool) {
    let mut text = RichText::new(format::format_currency(amount));
    if bold {
        text = text.strong();
    }
    match tone(amount) {
        Tone::Positive => ui.label(text.color(colors::GREEN)),
        Tone::Negative => ui.label(text.color(colors::RED)),
        Tone::Zero => ui.label(text),
    };
}

/// The Profit & Loss table: category, budgeted, actual, difference. The budgeted and actual figures
/// are buttons that open the budget items and the transactions behind them.
fn profit_and_loss_body(
    ui: &mut Ui,
    rows: &[PlRow],
    start: Date,
    end: Date,
    hscroll_state: &mut HScroll,
    drill: &mut Option<Drill>,
) {
    if rows.is_empty() {
        ui.label(RichText::new("No categories in this range").weak());
        return;
    }
    hscroll::show(ui, hscroll_state, |ui| {
        TableBuilder::new(ui)
            .striped(true)
            .auto_shrink([false, false])
            .max_scroll_height(f32::INFINITY)
            .column(Column::remainder().at_least(200.0))
            .column(Column::initial(130.0))
            .column(Column::initial(130.0))
            .column(Column::initial(130.0))
            .header(20.0, |mut header| {
                for h in ["Category", "Budgeted", "Actual", "Difference"] {
                    header.col(|ui| {
                        ui.strong(h);
                    });
                }
            })
            .body(|body| {
                body.rows(22.0, rows.len(), |mut row| {
                    let r = &rows[row.index()];
                    row.col(|ui| {
                        let text = if r.is_total {
                            RichText::new(&r.label).strong()
                        } else {
                            RichText::new(&r.label)
                        };
                        ui.label(text);
                    });
                    row.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let text = format::format_currency(r.budgeted);
                            if ui.add(Button::new(text).frame(false)).clicked() {
                                *drill = Some(Drill::BudgetItems {
                                    title: budget_drill_title(&r.label),
                                    ids: r.budget_item_ids.clone(),
                                    start,
                                    end,
                                });
                            }
                        });
                    });
                    row.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let text = format::format_currency(r.actual);
                            if ui.add(Button::new(text).frame(false)).clicked() {
                                *drill = Some(Drill::Transactions {
                                    title: pnl_drill_title(&r.label),
                                    ids: r.transaction_ids.clone(),
                                });
                            }
                        });
                    });
                    row.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            money(ui, r.difference(), r.is_total);
                        });
                    });
                });
            });
    });
}

/// The balance sheet (max width 700): the accounts with pending amounts, the obligations (when there
/// are any), and net worth with what is available after obligations (R-RPT-4).
fn balance_sheet_body(ui: &mut Ui, sheet: &BalanceSheet) {
    ui.set_max_width(BS_WIDTH);
    ui.label(RichText::new(format!("As of {}", format_date(sheet.as_of))).weak());
    Frame::group(ui.style()).show(ui, |ui| {
        egui::Grid::new("bs_accounts")
            .num_columns(3)
            .spacing([24.0, 6.0])
            .show(ui, |ui| {
                ui.strong("Account");
                ui.strong("Balance");
                ui.strong("Pending");
                ui.end_row();
                for a in &sheet.accounts {
                    ui.vertical(|ui| {
                        ui.label(&a.name);
                        if !a.institution.is_empty() {
                            ui.label(RichText::new(&a.institution).small().weak());
                        }
                    });
                    with_right(ui, |ui| money(ui, a.total, false));
                    with_right(ui, |ui| {
                        if a.pending.is_zero() {
                            ui.label("");
                        } else {
                            ui.label(
                                RichText::new(format::format_currency(a.pending))
                                    .color(colors::ORANGE),
                            );
                        }
                    });
                    ui.end_row();
                }
                ui.strong("Total");
                with_right(ui, |ui| money(ui, sheet.total, true));
                with_right(ui, |ui| money(ui, sheet.pending_total, true));
                ui.end_row();
            });
    });
    if !sheet.obligations.is_empty() {
        ui.add_space(8.0);
        Frame::group(ui.style()).show(ui, |ui| {
            egui::Grid::new("bs_obligations")
                .num_columns(2)
                .spacing([24.0, 6.0])
                .show(ui, |ui| {
                    ui.strong("Obligation");
                    ui.strong("Balance");
                    ui.end_row();
                    for o in &sheet.obligations {
                        ui.label(&o.name);
                        with_right(ui, |ui| {
                            let text = RichText::new(format::format_currency(o.balance));
                            if o.is_funding {
                                ui.label(text);
                            } else {
                                ui.label(text.color(colors::RED));
                            }
                        });
                        ui.end_row();
                    }
                    ui.strong("Total Obligations");
                    with_right(ui, |ui| money(ui, sheet.total_obligations, true));
                    ui.end_row();
                });
        });
    }
    ui.add_space(8.0);
    Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Net Worth").size(18.0).strong());
            with_right(ui, |ui| money(ui, sheet.net_worth, true));
        });
        ui.horizontal(|ui| {
            ui.label("Available (Net Worth − Obligations)");
            with_right(ui, |ui| money(ui, sheet.available, false));
        });
    });
}

/// Lays `add` out right-aligned in the current cell.
fn with_right(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.with_layout(Layout::right_to_left(Align::Center), add);
}

/// The monthly matrix: one row per category with its months, the total, and the NET row. Each cell is a
/// button that opens the transactions behind it (R-RPT-5); the NET row is plain.
fn matrix_body(
    ui: &mut Ui,
    db: &Db,
    grid: &MonthlyMatrix,
    mode: MatrixMode,
    scenario: Option<ScenarioId>,
    hscroll_state: &mut HScroll,
    drill: &mut Option<Drill>,
) {
    hscroll::show(ui, hscroll_state, |ui| {
        egui::ScrollArea::vertical()
            .auto_shrink([false, true])
            .show(ui, |ui| {
                egui::Grid::new("matrix")
                    .num_columns(14)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        ui.add_sized(
                            [MATRIX_LABEL_WIDTH, 18.0],
                            egui::Label::new(RichText::new("Category").strong()),
                        );
                        for m in MONTH_SHORT {
                            ui.add_sized(
                                [MATRIX_CELL_WIDTH, 18.0],
                                egui::Label::new(RichText::new(m).strong()),
                            );
                        }
                        ui.add_sized(
                            [MATRIX_CELL_WIDTH, 18.0],
                            egui::Label::new(RichText::new("Total").strong()),
                        );
                        ui.end_row();
                        for row in &grid.rows {
                            ui.label(&row.label);
                            for m in 0..12 {
                                let month = u8::try_from(m + 1).unwrap_or(1);
                                let cell = matrix_cell(mode, row.actual[m], row.budget[m]);
                                let click = Click {
                                    title: matrix_drill_title(&row.label, grid.year, Some(month)),
                                    ids: move || {
                                        matrix_drill(
                                            db,
                                            grid.year,
                                            scenario,
                                            row.category_id,
                                            Some(month),
                                        )
                                    },
                                };
                                cell_ui(ui, &cell, Some(click), drill);
                            }
                            let cell = matrix_cell(mode, row.actual_total(), row.budget_total());
                            let click = Click {
                                title: matrix_drill_title(&row.label, grid.year, None),
                                ids: move || {
                                    matrix_drill(db, grid.year, scenario, row.category_id, None)
                                },
                            };
                            cell_ui(ui, &cell, Some(click), drill);
                            ui.end_row();
                        }
                        ui.label(RichText::new("NET").strong());
                        for m in 0..12 {
                            let cell = matrix_cell(mode, grid.net_actual[m], grid.net_budget[m]);
                            cell_ui::<fn() -> Vec<TransactionId>>(ui, &cell, None, drill);
                        }
                        let total_actual: Decimal = grid.net_actual.iter().sum();
                        let total_budget: Decimal = grid.net_budget.iter().sum();
                        let cell = matrix_cell(mode, total_actual, total_budget);
                        cell_ui::<fn() -> Vec<TransactionId>>(ui, &cell, None, drill);
                        ui.end_row();
                    });
            });
    });
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
    ui.allocate_ui_with_layout(
        egui::vec2(MATRIX_CELL_WIDTH, 36.0),
        Layout::top_down(Align::Max),
        |ui| {
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
        },
    );
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

/// "Mar 1, 2026" (the Swift date format).
fn format_date(d: Date) -> String {
    d.strftime("%b %-d, %Y").to_string()
}
