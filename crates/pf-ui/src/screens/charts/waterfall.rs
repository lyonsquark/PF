//! Monthly cashflow waterfall (T24b, card 6, R-CHT-6): income, expenses and savings as bars that step up
//! and down. Bars are `egui_plot` bars with a base offset. A click inside a bar opens the transactions
//! behind it (`waterfall_drill` in pf-core picks them).

use egui::{RichText, Ui};
use egui_extras::DatePickerButton;
use egui_plot::{Bar, BarChart, Plot, PlotPoint, Text};
use pf_core::charts::waterfall::{Waterfall, WaterfallBar, waterfall, waterfall_drill};
use pf_core::charts::{ChartFilter, NOT_ENOUGH_DATA};
use pf_core::format;
use rust_decimal::Decimal;

use super::State;
use super::grid;
use super::style::{hex_color, money_axis, to_f64};
use crate::app::UiCtx;
use crate::widgets::colors;

/// Height of the plot in points.
const PLOT_HEIGHT: f32 = 250.0;
/// Width of a bar, in plot units (a bar is 0.6 of its slot).
const BAR_WIDTH: f64 = 0.6;

/// The bars as egui_plot bars: the base is the lower end, the height the size of the step, the fill the
/// colour T08b picked (R-CHT-6).
pub(crate) fn waterfall_bars(bars: &[WaterfallBar]) -> Vec<Bar> {
    bars.iter()
        .enumerate()
        .map(|(i, b)| {
            let low = to_f64(b.y_start.min(b.y_end));
            let height = to_f64((b.y_end - b.y_start).abs());
            Bar::new(i as f64, height)
                .base_offset(low)
                .width(BAR_WIDTH)
                .fill(hex_color(&b.color))
                .name(b.label.clone())
        })
        .collect()
}

/// The signed thousands label of a step: "+$5.0k", "-$1.5k".
pub(crate) fn step_label(b: &WaterfallBar) -> String {
    let delta = b.y_end - b.y_start;
    let sign = if delta < Decimal::ZERO { "-" } else { "+" };
    let k = to_f64(delta.abs()) / 1000.0;
    format!("{sign}${k:.1}k")
}

/// The waterfall card: the month, the totals, a date picker, and the bars.
pub fn card(ui: &mut Ui, cx: &UiCtx, st: &mut State, f: &ChartFilter, drill: &mut super::Drill) {
    let db = cx.engine.db();
    let revision = cx.engine.revision();
    let base = st.waterfall_date.unwrap_or(f.today);
    let key = ((f.accounts.clone(), f.scenario, f.today), base);
    let w: Waterfall = st
        .waterfall
        .get(revision, &key, || waterfall(db, f, base))
        .clone();
    ui.horizontal(|ui| {
        ui.label(RichText::new(&w.month_label).size(22.0).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut date = base;
            ui.add(
                DatePickerButton::new(&mut date)
                    .id_salt("waterfall_date")
                    .show_icon(false),
            );
            if date != base {
                st.waterfall_date = Some(date);
            }
        });
    });
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("In {}", format::format_currency(w.total_in)))
                .color(colors::GREEN),
        );
        ui.label(
            RichText::new(format!("Out {}", format::format_currency(w.total_out)))
                .color(colors::RED),
        );
    });
    if w.bars.is_empty() {
        ui.label(RichText::new(NOT_ENOUGH_DATA).weak());
        return;
    }
    let names: Vec<String> = w.bars.iter().map(|b| b.label.clone()).collect();
    let grid_minor = st.grid_minor;
    let plot = Plot::new("chart_waterfall")
        .x_grid_spacer(grid::spacer(grid_minor))
        .y_grid_spacer(grid::spacer(grid_minor))
        .height(PLOT_HEIGHT)
        .allow_zoom(false)
        .allow_drag(false)
        .allow_scroll(false)
        .x_axis_formatter(move |mark, _| {
            usize::try_from(mark.value.round() as i64)
                .ok()
                .and_then(|i| names.get(i).cloned())
                .unwrap_or_default()
        })
        .y_axis_formatter(|mark, _| money_axis(mark.value))
        .show(ui, |plot_ui| {
            plot_ui.bar_chart(BarChart::new("Waterfall", waterfall_bars(&w.bars)));
            for (i, b) in w.bars.iter().enumerate() {
                let top = to_f64(b.y_start.max(b.y_end));
                plot_ui.text(Text::new("", PlotPoint::new(i as f64, top), step_label(b)));
            }
            plot_ui.pointer_coordinate()
        });
    // A click inside a bar opens the transactions behind it (R-CHT-6).
    if plot.response.clicked()
        && let Some(pc) = plot.inner
        && let Some(b) = usize::try_from(pc.x.round() as i64)
            .ok()
            .and_then(|i| w.bars.get(i))
    {
        let low = to_f64(b.y_start.min(b.y_end));
        let high = to_f64(b.y_start.max(b.y_end));
        if (low..=high).contains(&pc.y) {
            *drill = Some(waterfall_drill(db, f, base, &b.label));
        }
    }
    if plot.response.hovered()
        && let Some(pc) = plot.inner
    {
        let bar = usize::try_from(pc.x.round() as i64)
            .ok()
            .and_then(|i| w.bars.get(i))
            .cloned();
        if let Some(b) = bar {
            plot.response.on_hover_ui_at_pointer(|ui| {
                ui.label(RichText::new(&b.label).strong());
                ui.label(step_label(&b));
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_core::charts::waterfall::BarKind;

    fn bar(label: &str, start: i64, end: i64, color: &str) -> WaterfallBar {
        WaterfallBar {
            label: label.to_string(),
            y_start: Decimal::from(start),
            y_end: Decimal::from(end),
            kind: BarKind::Income,
            color: color.to_string(),
        }
    }

    #[test]
    /// Covers: R-CHT-6. A bar sits on its lower end, its height is the step, and its fill is the data's colour.
    fn waterfall_bars_offsets() {
        let bars = vec![
            bar("Income", 0, 5000, "#34c759"),
            bar("Housing", 5000, 3500, "#ff3b30"),
            bar("Savings", 0, 3500, "#30d158"),
        ];
        let out = waterfall_bars(&bars);
        assert_eq!(out.len(), 3);
        // The bars are read back through their public fields, which carry the offset and height.
        assert_eq!(out[0].base_offset, Some(0.0));
        assert_eq!(out[0].value, 5000.0);
        assert_eq!(out[1].base_offset, Some(3500.0));
        assert_eq!(out[1].value, 1500.0);
        assert_eq!(out[2].base_offset, Some(0.0));
        assert_eq!(out[2].value, 3500.0);
    }

    #[test]
    /// Covers: R-CHT-6. Each bar is filled with the colour T08b gave it.
    fn waterfall_bars_use_data_colors() {
        let bars = vec![bar("Income", 0, 5000, "#34c759")];
        let out = waterfall_bars(&bars);
        assert_eq!(out[0].fill, hex_color("#34c759"));
    }

    #[test]
    /// The step labels: "+$5.0k" up, "-$1.5k" down.
    fn step_labels_are_signed() {
        assert_eq!(step_label(&bar("Income", 0, 5000, "#000000")), "+$5.0k");
        assert_eq!(step_label(&bar("Housing", 5000, 3500, "#000000")), "-$1.5k");
    }
}
