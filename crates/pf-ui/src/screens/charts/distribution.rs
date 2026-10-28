//! Expense distribution (T24a, R-CHT-7): a horizontal bar per category, largest at the top, each in its
//! category colour, with the total above.

use egui::{ComboBox, RichText, Ui};
use egui_plot::{Bar, BarChart, Plot};
use pf_core::charts::ChartFilter;
use pf_core::charts::NOT_ENOUGH_DATA;
use pf_core::charts::categories::{
    DISTRIBUTION_PERIODS, distribution_height, expense_distribution,
};
use pf_core::format;
use rust_decimal::Decimal;

use super::State;
use super::grid;
use super::style::{hex_color, to_f64};
use crate::app::UiCtx;

/// The distribution card: the total, the period picker, and the bars.
pub fn card(ui: &mut Ui, cx: &UiCtx, st: &mut State, f: &ChartFilter) {
    let db = cx.engine.db();
    let revision = cx.engine.revision();
    let period = st.distribution_period;
    let key = ((f.accounts.clone(), f.scenario, f.today), period);
    let slices = st
        .distribution
        .get(revision, &key, || expense_distribution(db, f, period))
        .clone();
    let total: Decimal = slices.iter().map(|s| s.amount).sum();
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("{} Total", format::format_currency(total)))
                .size(22.0)
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let shown = DISTRIBUTION_PERIODS
                .iter()
                .find(|(_, p)| *p == period)
                .map_or("1Y", |(label, _)| label);
            let resp = ComboBox::from_id_salt("distribution_period")
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    for (label, p) in DISTRIBUTION_PERIODS {
                        ui.selectable_value(&mut st.distribution_period, p, label);
                    }
                });
            resp.response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::accesskit::Role::ComboBox,
                    true,
                    "Distribution Period",
                )
            });
        });
    });
    if slices.is_empty() {
        ui.label(RichText::new(NOT_ENOUGH_DATA).weak());
        return;
    }
    let names: Vec<String> = slices.iter().map(|s| s.name.clone()).collect();
    let bars: Vec<Bar> = slices
        .iter()
        .enumerate()
        .map(|(i, s)| {
            Bar::new(i as f64, to_f64(s.amount))
                .name(s.name.clone())
                .fill(hex_color(&s.color))
        })
        .collect();
    let height = distribution_height(slices.len());
    Plot::new((
        "chart_distribution",
        super::plot_key(f, st.distribution_period),
    ))
    .x_grid_spacer(grid::major_x)
    .y_grid_spacer(grid::per_bar(slices.len()))
    .height(height)
    .allow_zoom(false)
    .allow_drag(false)
    .allow_scroll(false)
    .include_x(0.0)
    .x_axis_formatter(|mark, _| format!("{:.0}", mark.value))
    .y_axis_formatter(move |mark, _| {
        usize::try_from(mark.value.round() as i64)
            .ok()
            .and_then(|i| names.get(i).cloned())
            .unwrap_or_default()
    })
    .show(ui, |plot| {
        plot.bar_chart(BarChart::new("Expenses", bars).horizontal());
    });
}
