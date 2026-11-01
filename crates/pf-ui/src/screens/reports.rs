//! Reports screen (T23, Swift `ReportsView`): Profit & Loss, Balance Sheet and the Monthly Matrix. The
//! numbers come from `pf_core::reports` (cached per engine revision); this file lays them out and wires
//! the clicks to the drill-down dialogs. The wide tables scroll sideways with the clamped `hscroll`.

use egui::{Align, Button, ComboBox, Frame, Layout, RichText, Role, Ui, WidgetInfo};
use egui_extras::{Column, DatePickerButton, TableBuilder};
use jiff::civil::Date;
use pf_core::format;
use pf_core::reports::{
    BalanceSheet, MONTH_SHORT, MatrixMode, MonthlyMatrix, PlRow, QUICK_SELECT, ReportKind, Tone,
    balance_sheet, budget_drill_title, default_matrix_cutoff, default_pnl_range,
    matrix_year_options, monthly_matrix, pnl_drill_title, profit_and_loss, quick_select_range,
    tone,
};
use pf_model::{BudgetItemId, ScenarioId, TransactionId};
use rust_decimal::Decimal;

use self::matrix::{MatrixScroll, MatrixView, matrix_body};
use crate::app::UiCtx;
use crate::caches::Cached;
use crate::dialogs::Dialog;
use crate::dialogs::report_drilldown::open_transactions;
use crate::widgets::col_fit::fit;
use crate::widgets::colors;
use crate::widgets::hscroll::{self, HScroll};

mod matrix;

/// Width of the name column (account, category, obligation), in points.
const NAME_WIDTH: f32 = 200.0;
/// Width of the balance sheet, in points.
const BS_WIDTH: f32 = 700.0;
/// Height of an account row (two lines: name and institution), in points.
const BS_ACCOUNT_ROW_HEIGHT: f32 = 40.0;
/// Height of a single-line balance sheet row, in points.
const BS_ROW_HEIGHT: f32 = 22.0;

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
    /// The cutoff month picked, and for which year. Another year goes back to its default.
    pub matrix_cutoff: Option<(i16, u8)>,
    pl_hscroll: HScroll,
    matrix_scroll: MatrixScroll,
    pnl: Cached<(Option<ScenarioId>, Date, Date), Vec<PlRow>>,
    bs: Cached<(Option<ScenarioId>, Date), BalanceSheet>,
    matrix: Cached<(Option<ScenarioId>, i16, u8), MonthlyMatrix>,
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

    let year = st.matrix_year.unwrap_or(today.year());
    let cutoff = matrix_cutoff(st, year, today);
    let State {
        report,
        matrix_mode,
        pl_hscroll,
        matrix_scroll,
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
            let grid = matrix.get(revision, &(scenario, year, cutoff), || {
                monthly_matrix(db, year, scenario).through(cutoff)
            });
            let view = MatrixView {
                db,
                grid,
                mode: *matrix_mode,
                scenario,
                cutoff,
            };
            matrix_body(ui, &view, matrix_scroll, &mut drill);
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
        ui.label("Budget total cutoff");
        let cutoff = matrix_cutoff(st, chosen, today);
        let cutoff_box = ComboBox::from_id_salt("matrix_cutoff")
            // Tall enough to list all twelve months without scrolling.
            .height(400.0)
            .selected_text(MONTH_SHORT[usize::from(cutoff).clamp(1, 12) - 1])
            .show_ui(ui, |ui| {
                for (month, name) in (1u8..).zip(MONTH_SHORT) {
                    if ui.selectable_label(month == cutoff, name).clicked() {
                        st.matrix_cutoff = Some((chosen, month));
                    }
                }
            });
        cutoff_box
            .response
            .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, "Budget total cutoff"));
    });
}

/// R-RPT-5f: the last month the matrix shows for `year`: the one picked for that year, or its default.
fn matrix_cutoff(st: &State, year: i16, today: Date) -> u8 {
    match st.matrix_cutoff {
        Some((picked_year, month)) if picked_year == year => month,
        _ => default_matrix_cutoff(year, today),
    }
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
    // Each money column starts as wide as its widest figure (see `widgets::col_fit`).
    let money_w = fit(ui, "Difference", &["-$999,999.99"]);
    hscroll::show(ui, hscroll_state, |ui| {
        TableBuilder::new(ui)
            .id_salt("reports_pnl")
            .resizable(true)
            .striped(true)
            .auto_shrink([false, false])
            .max_scroll_height(f32::INFINITY)
            .column(Column::initial(NAME_WIDTH))
            .column(Column::initial(money_w))
            .column(Column::initial(money_w))
            .column(Column::initial(money_w))
            .header(20.0, |mut header| {
                header.col(|ui| {
                    ui.strong("Category");
                });
                for h in ["Budgeted", "Actual", "Difference"] {
                    header.col(|ui| {
                        with_right(ui, |ui| {
                            ui.strong(h);
                        });
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
    let money_w = fit(ui, "Pending", &["-$999,999.99"]).max(fit(ui, "Balance", &["-$999,999.99"]));
    Frame::group(ui.style()).show(ui, |ui| {
        TableBuilder::new(ui)
            .id_salt("reports_bs_accounts")
            .resizable(true)
            .auto_shrink([false, true])
            .column(Column::initial(NAME_WIDTH))
            .column(Column::initial(money_w))
            .column(Column::initial(money_w))
            .header(20.0, |mut header| {
                header.col(|ui| {
                    ui.strong("Account");
                });
                header.col(|ui| {
                    with_right(ui, |ui| {
                        ui.strong("Balance");
                    })
                });
                header.col(|ui| {
                    with_right(ui, |ui| {
                        ui.strong("Pending");
                    })
                });
            })
            .body(|body| {
                body.rows(
                    BS_ACCOUNT_ROW_HEIGHT,
                    sheet.accounts.len() + 1,
                    |mut row| {
                        let Some(a) = sheet.accounts.get(row.index()) else {
                            row.col(|ui| {
                                ui.strong("Total");
                            });
                            row.col(|ui| with_right(ui, |ui| money(ui, sheet.total, true)));
                            row.col(|ui| with_right(ui, |ui| money(ui, sheet.pending_total, true)));
                            return;
                        };
                        row.col(|ui| {
                            ui.vertical(|ui| {
                                ui.label(&a.name);
                                if !a.institution.is_empty() {
                                    ui.label(RichText::new(&a.institution).small().weak());
                                }
                            });
                        });
                        row.col(|ui| with_right(ui, |ui| money(ui, a.total, false)));
                        row.col(|ui| {
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
                        });
                    },
                );
            });
    });
    if !sheet.obligations.is_empty() {
        ui.add_space(8.0);
        Frame::group(ui.style()).show(ui, |ui| {
            TableBuilder::new(ui)
                .id_salt("reports_bs_obligations")
                .resizable(true)
                .auto_shrink([false, true])
                .column(Column::initial(NAME_WIDTH))
                .column(Column::initial(money_w))
                .header(20.0, |mut header| {
                    header.col(|ui| {
                        ui.strong("Obligation");
                    });
                    header.col(|ui| {
                        with_right(ui, |ui| {
                            ui.strong("Balance");
                        })
                    });
                })
                .body(|body| {
                    body.rows(BS_ROW_HEIGHT, sheet.obligations.len() + 1, |mut row| {
                        let Some(o) = sheet.obligations.get(row.index()) else {
                            row.col(|ui| {
                                ui.strong("Total Obligations");
                            });
                            row.col(|ui| {
                                with_right(ui, |ui| money(ui, sheet.total_obligations, true))
                            });
                            return;
                        };
                        row.col(|ui| {
                            ui.label(&o.name);
                        });
                        row.col(|ui| {
                            with_right(ui, |ui| {
                                let text = RichText::new(format::format_currency(o.balance));
                                if o.is_funding {
                                    ui.label(text);
                                } else {
                                    ui.label(text.color(colors::RED));
                                }
                            });
                        });
                    });
                });
        });
    }
    ui.add_space(8.0);
    Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Net Worth").size(18.0).strong());
            with_right(ui, |ui| money(ui, sheet.net_worth, true));
        });
        // The Dashboard's Net Worth uses cleared balances only; this one includes pending and future.
        ui.label(
            RichText::new("Includes pending and future transactions.")
                .small()
                .weak(),
        );
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

/// "Mar 1, 2026" (the Swift date format).
fn format_date(d: Date) -> String {
    d.strftime("%b %-d, %Y").to_string()
}
