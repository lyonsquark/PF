//! Account balances over time (T24a, R-CHT-1): the balance on the left, a period picker on the right,
//! and one blue line filled down to zero.

use egui::{ComboBox, RichText, Ui};
use egui_plot::{Line, Plot, PlotPoints};
use pf_core::charts::ChartFilter;
use pf_core::charts::NOT_ENOUGH_DATA;
use pf_core::charts::balance::{BALANCE_PERIODS, balance_over_time};
use pf_core::format;

use super::State;
use super::grid;
use super::style::{MAIN, day_axis, day_number, money_axis_small, to_f64};
use crate::app::UiCtx;

/// Draws the balance card's content.
pub fn card(ui: &mut Ui, cx: &UiCtx, st: &mut State, f: &ChartFilter) {
    let db = cx.engine.db();
    let revision = cx.engine.revision();
    let period = st.networth_period;
    let key = ((f.accounts.clone(), f.scenario, f.today), period);
    let points = st
        .networth
        .get(revision, &key, || balance_over_time(db, f, period))
        .clone();
    ui.horizontal(|ui| {
        let last = points.last().map(|p| p.balance);
        match last {
            Some(v) => {
                ui.label(
                    RichText::new(format::format_currency(v))
                        .size(22.0)
                        .strong(),
                );
            }
            None => {
                ui.label(RichText::new(NOT_ENOUGH_DATA).weak());
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let resp = ComboBox::from_id_salt("networth_period")
                .selected_text(period_label(period))
                .show_ui(ui, |ui| {
                    for (label, p) in BALANCE_PERIODS {
                        ui.selectable_value(&mut st.networth_period, p, label);
                    }
                });
            resp.response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::accesskit::Role::ComboBox, true, "Balance Period")
            });
        });
    });
    if points.is_empty() {
        return;
    }
    let long = points
        .first()
        .zip(points.last())
        .is_some_and(|(a, b)| day_number(b.date) - day_number(a.date) > 120.0);
    let values: Vec<[f64; 2]> = points
        .iter()
        .map(|p| [day_number(p.date), to_f64(p.balance)])
        .collect();
    let grid_base = ui.visuals().text_color();
    let grid_minor = st.grid_minor;
    Plot::new("chart_networth")
        .show_grid(false)
        .height(super::PLOT_HEIGHT)
        .allow_zoom(true)
        .allow_drag(true)
        .allow_scroll(false)
        .x_axis_formatter(move |mark, _| day_axis(mark.value, long))
        .y_axis_formatter(|mark, _| money_axis_small(mark.value))
        .show(ui, |plot| {
            grid::draw(plot, grid_base, grid_minor);
            plot.line(
                Line::new("Net Worth", PlotPoints::from(values))
                    .color(MAIN)
                    .width(2.5)
                    .fill(0.0),
            );
        });
}

/// The label of a period in the picker.
fn period_label(period: pf_model::TimePeriod) -> &'static str {
    BALANCE_PERIODS
        .iter()
        .find(|(_, p)| *p == period)
        .map_or("1Y", |(label, _)| label)
}
