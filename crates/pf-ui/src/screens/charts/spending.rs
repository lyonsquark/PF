//! Cumulative spending, month against month (T24a, R-CHT-2), and budget pacing against actual spending
//! (T24a, R-CHT-3).

use egui::{RichText, Ui};
use egui_extras::DatePickerButton;
use egui_plot::{Legend, Line, Plot, PlotPoints};
use jiff::civil::Date;
use pf_core::charts::ChartFilter;
use pf_core::charts::spending::{budget_pacing, spending_comparison, step_month};
use pf_core::charts::{NO_BUDGETS, NOT_ENOUGH_DATA};
use pf_core::format;

use super::State;
use super::grid;
use super::style::{GREY, MAIN, to_f64};
use crate::app::UiCtx;

/// The comparison card: the two months' cumulative spending, with each month stepped by the arrows.
pub fn comparison_card(ui: &mut Ui, cx: &UiCtx, st: &mut State, f: &ChartFilter) {
    let db = cx.engine.db();
    let revision = cx.engine.revision();
    let today = f.today;
    let a = st.compare_a.unwrap_or(today);
    let b = st.compare_b.unwrap_or_else(|| step_month(a, -1));
    let key = ((f.accounts.clone(), f.scenario, f.today), a, b);
    let cmp = st
        .comparison
        .get(revision, &key, || spending_comparison(db, f, a, b))
        .clone();
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!(
                "Total: {} vs {}",
                format::format_currency(cmp.total_b),
                format::format_currency(cmp.total_a)
            ))
            .weak(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Right to left: B first on the right, then "vs", then A.
            if ui.button(">").clicked() {
                st.compare_b = Some(step_month(b, 1));
            }
            ui.label(month_text(b));
            if ui.button("<").clicked() {
                st.compare_b = Some(step_month(b, -1));
            }
            ui.label("vs");
            if ui.button(">").clicked() {
                st.compare_a = Some(step_month(a, 1));
            }
            ui.label(month_text(a));
            if ui.button("<").clicked() {
                st.compare_a = Some(step_month(a, -1));
            }
        });
    });
    if cmp.a.points.is_empty() && cmp.b.points.is_empty() {
        ui.label(RichText::new(NOT_ENOUGH_DATA).weak());
        return;
    }
    let a_points: Vec<[f64; 2]> = cmp
        .a
        .points
        .iter()
        .map(|(d, v)| [f64::from(*d), to_f64(*v)])
        .collect();
    let b_points: Vec<[f64; 2]> = cmp
        .b
        .points
        .iter()
        .map(|(d, v)| [f64::from(*d), to_f64(*v)])
        .collect();
    let grid_base = ui.visuals().text_color();
    let grid_minor = st.grid_minor;
    Plot::new("chart_spending")
        .show_grid(false)
        .height(super::PLOT_HEIGHT)
        .legend(Legend::default())
        .allow_zoom(false)
        .allow_drag(false)
        .allow_scroll(false)
        .include_y(0.0)
        .x_axis_formatter(|mark, _| format!("Day {}", mark.value.round()))
        .show(ui, |plot| {
            grid::draw(plot, grid_base, grid_minor);
            plot.line(
                Line::new(cmp.b.label.clone(), PlotPoints::from(b_points))
                    .color(GREY)
                    .width(2.0)
                    .style(egui_plot::LineStyle::Dashed { length: 6.0 }),
            );
            plot.line(
                Line::new(cmp.a.label.clone(), PlotPoints::from(a_points))
                    .color(MAIN)
                    .width(3.0),
            );
        });
}

/// The budget pacing card: what has been spent against the budget so far this month.
pub fn pacing_card(ui: &mut Ui, cx: &UiCtx, st: &mut State, f: &ChartFilter) {
    let db = cx.engine.db();
    let revision = cx.engine.revision();
    let base = st.pacing_date.unwrap_or(f.today);
    let key = ((f.accounts.clone(), f.scenario, f.today), base);
    let p = st
        .pacing
        .get(revision, &key, || budget_pacing(db, f, base))
        .clone();
    ui.horizontal(|ui| {
        let mut spent = RichText::new(format!("{} Spent", format::format_currency(p.spent)))
            .size(22.0)
            .strong();
        if p.over_budget {
            spent = spent.color(crate::widgets::colors::RED);
        }
        ui.label(spent);
        ui.label(
            RichText::new(format!(
                "Budget: {}",
                format::format_currency(p.total_budget)
            ))
            .weak(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut date = base;
            ui.add(
                DatePickerButton::new(&mut date)
                    .id_salt("pacing_date")
                    .show_icon(false),
            );
            if date != base {
                st.pacing_date = Some(date);
            }
        });
    });
    if p.total_budget <= rust_decimal::Decimal::ZERO {
        ui.label(RichText::new(NO_BUDGETS).weak());
        return;
    }
    let budget: Vec<[f64; 2]> = p
        .budget_line
        .iter()
        .map(|(d, v)| [f64::from(*d), to_f64(*v)])
        .collect();
    let actual: Vec<[f64; 2]> = p
        .actual_line
        .iter()
        .map(|(d, v)| [f64::from(*d), to_f64(*v)])
        .collect();
    let grid_base = ui.visuals().text_color();
    let grid_minor = st.grid_minor;
    Plot::new("chart_pacing")
        .show_grid(false)
        .height(super::PLOT_HEIGHT)
        .legend(Legend::default())
        .allow_zoom(false)
        .allow_drag(false)
        .allow_scroll(false)
        .include_y(0.0)
        .x_axis_formatter(|mark, _| format!("{}", mark.value.round()))
        .show(ui, |plot| {
            grid::draw(plot, grid_base, grid_minor);
            plot.line(
                Line::new("Budget Pacing", PlotPoints::from(budget))
                    .color(GREY)
                    .width(2.0)
                    .style(egui_plot::LineStyle::Dashed { length: 6.0 }),
            );
            plot.line(
                Line::new("Actual Spending", PlotPoints::from(actual))
                    .color(MAIN)
                    .width(3.0)
                    .fill(0.0),
            );
        });
}

/// "Mar 2026", the label of a comparison month.
fn month_text(d: Date) -> String {
    d.strftime("%b %Y").to_string()
}
