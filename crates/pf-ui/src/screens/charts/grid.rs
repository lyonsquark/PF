//! Chart grid lines, drawn by the chart instead of by egui_plot, so the minor lines can be lightened
//! with the Charts screen's "Minor grid lines" slider. egui_plot fades a line by its spacing with a fixed
//! rule; here the major lines (the coarsest level) use the base colour and the minor lines use it at the
//! slider's strength.

use egui::Color32;
use egui_plot::{GridInput, HLine, PlotUi, VLine, log_grid_spacer};

/// Grid spacing range in points, as egui_plot uses it: a line this far apart is at full strength.
const SPACING_MIN: f64 = 8.0;
const SPACING_MAX: f64 = 300.0;

/// Draws the grid of both axes on `plot_ui`. Major lines are `base`; minor lines are `base` at `minor`
/// (0 draws none, 1 is as dark as the major lines). Call it first, so the data is drawn over it.
pub(crate) fn draw(plot_ui: &mut PlotUi, base: Color32, minor: f32) {
    let transform = *plot_ui.transform();
    let bounds = plot_ui.plot_bounds();
    let ranges = [bounds.range_x(), bounds.range_y()];
    let spacer = log_grid_spacer(10);
    let value_per_point = transform.dvalue_dpos();
    let points_per_value = transform.dpos_dvalue();
    for axis in 0..2 {
        let input = GridInput {
            bounds: (*ranges[axis].start(), *ranges[axis].end()),
            base_step_size: value_per_point[axis].abs() * SPACING_MIN,
        };
        // (value, strength) for every line of this axis, the strongest kept where levels coincide.
        let mut lines: Vec<(f64, f64)> = Vec::new();
        for mark in spacer(input) {
            let spacing = points_per_value[axis].abs() * mark.step_size;
            if spacing <= SPACING_MIN {
                continue;
            }
            let strength = ((spacing - SPACING_MIN) / (SPACING_MAX - SPACING_MIN)).clamp(0.0, 1.0);
            lines.push((mark.value, strength));
        }
        lines.sort_by(|a, b| a.0.total_cmp(&b.0));
        lines.dedup_by(|later, kept| {
            let same = (later.0 - kept.0).abs() < 1e-9 * kept.0.abs().max(1.0);
            if same {
                kept.1 = kept.1.max(later.1);
            }
            same
        });
        for (value, strength) in lines {
            let major = strength >= 1.0;
            if !major && minor <= 0.0 {
                continue;
            }
            let color = if major {
                base
            } else {
                base.gamma_multiply(minor)
            };
            if axis == 0 {
                plot_ui.vline(
                    VLine::new("", value)
                        .color(color)
                        .width(1.0)
                        .allow_hover(false),
                );
            } else {
                plot_ui.hline(
                    HLine::new("", value)
                        .color(color)
                        .width(1.0)
                        .allow_hover(false),
                );
            }
        }
    }
}
