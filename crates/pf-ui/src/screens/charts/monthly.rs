//! Profit and loss by month (T24a, R-CHT-4) and the income and expense trend (T24a, R-CHT-8). Both are
//! per-month sums from `pf_core::charts::profit_loss_by_month` and `income_expense_trend`.

use egui::{ComboBox, RichText, Ui};
use egui_plot::{Bar, BarChart, Legend, Line, Plot, PlotPoints};
use pf_core::charts::ChartFilter;
use pf_core::charts::NOT_ENOUGH_DATA;
use pf_core::charts::monthly::{
    MonthSummary, PL_MONTH_OPTIONS, TREND_PERIODS, income_expense_trend, profit_loss_by_month,
};

use super::State;
use super::style::{EXPENSE, INCOME, money_axis, to_f64};
use crate::app::UiCtx;

/// Profit and loss by month: grouped income and expense bars, and the net as a line.
pub fn profit_loss_card(ui: &mut Ui, cx: &UiCtx, st: &mut State, f: &ChartFilter) {
    let db = cx.engine.db();
    let revision = cx.engine.revision();
    let months = if st.pl_months == 0 { 6 } else { st.pl_months };
    let key = ((f.accounts.clone(), f.scenario, f.today), months);
    let rows = st
        .profit_loss
        .get(revision, &key, || profit_loss_by_month(db, f, months))
        .clone();
    ui.horizontal(|ui| {
        ui.label(RichText::new("Monthly Profit & Loss").weak());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let shown = PL_MONTH_OPTIONS
                .iter()
                .find(|(_, m)| *m == months)
                .map_or("6M", |(label, _)| label);
            let resp = ComboBox::from_id_salt("pl_months")
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    for (label, m) in PL_MONTH_OPTIONS {
                        ui.selectable_value(&mut st.pl_months, m, label);
                    }
                });
            resp.response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::accesskit::Role::ComboBox,
                    true,
                    "Profit and Loss Months",
                )
            });
        });
    });
    if rows.is_empty() {
        ui.label(RichText::new(NOT_ENOUGH_DATA).weak());
        return;
    }
    let income: Vec<Bar> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| Bar::new(i as f64 - 0.18, to_f64(r.income)).width(0.32))
        .collect();
    let expenses: Vec<Bar> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| Bar::new(i as f64 + 0.18, to_f64(r.expenses)).width(0.32))
        .collect();
    let net: Vec<[f64; 2]> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| [i as f64, to_f64(r.net())])
        .collect();
    let labels = rows.clone();
    Plot::new("chart_profit_loss")
        .height(super::PLOT_HEIGHT)
        .allow_zoom(false)
        .allow_drag(false)
        .allow_scroll(false)
        .include_y(0.0)
        .x_axis_formatter(move |mark, _| label_at(&labels, mark.value))
        .y_axis_formatter(|mark, _| money_axis(mark.value))
        .show(ui, |plot| {
            plot.bar_chart(BarChart::new("Income", income).color(INCOME));
            plot.bar_chart(BarChart::new("Expenses", expenses).color(EXPENSE));
            plot.line(
                Line::new("Net Income", PlotPoints::from(net))
                    .color(super::style::MAIN)
                    .width(2.0),
            );
        });
    legend_row(ui);
}

/// The income and expense trend: one line each, filled to zero.
pub fn trend_card(ui: &mut Ui, cx: &UiCtx, st: &mut State, f: &ChartFilter) {
    let db = cx.engine.db();
    let revision = cx.engine.revision();
    let period = st.trend_period;
    let key = ((f.accounts.clone(), f.scenario, f.today), period);
    let rows = st
        .trend
        .get(revision, &key, || income_expense_trend(db, f, period))
        .clone();
    ui.horizontal(|ui| {
        let shown = TREND_PERIODS
            .iter()
            .find(|(_, p)| *p == period)
            .map_or("1Y", |(label, _)| label);
        ui.label(RichText::new(shown).weak());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            for (label, p) in TREND_PERIODS.iter().rev() {
                ui.selectable_value(&mut st.trend_period, *p, *label);
            }
        });
    });
    if rows.is_empty() {
        ui.label(RichText::new(NOT_ENOUGH_DATA).weak());
        return;
    }
    let income: Vec<[f64; 2]> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| [i as f64, to_f64(r.income)])
        .collect();
    let expenses: Vec<[f64; 2]> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| [i as f64, to_f64(r.expenses)])
        .collect();
    let labels = rows.clone();
    Plot::new("chart_trend")
        .height(super::PLOT_HEIGHT)
        .legend(Legend::default())
        .allow_zoom(false)
        .allow_drag(false)
        .allow_scroll(false)
        .include_y(0.0)
        .x_axis_formatter(move |mark, _| label_at(&labels, mark.value))
        .y_axis_formatter(|mark, _| money_axis(mark.value))
        .show(ui, |plot| {
            plot.line(
                Line::new("Income", PlotPoints::from(income))
                    .color(INCOME)
                    .width(2.5)
                    .fill(0.1),
            );
            plot.line(
                Line::new("Expenses", PlotPoints::from(expenses))
                    .color(EXPENSE)
                    .width(2.5)
                    .fill(0.1),
            );
        });
}

/// The month label at the x position (rounded), or nothing past the end.
fn label_at(rows: &[MonthSummary], x: f64) -> String {
    usize::try_from(x.round() as i64)
        .ok()
        .and_then(|i| rows.get(i))
        .map_or_else(String::new, |r| r.label.clone())
}

/// The legend below the profit and loss plot: Income, Expenses, Net Income.
fn legend_row(ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.colored_label(INCOME, "Income");
        ui.colored_label(EXPENSE, "Expenses");
        ui.colored_label(super::style::MAIN, "Net Income");
    });
}
