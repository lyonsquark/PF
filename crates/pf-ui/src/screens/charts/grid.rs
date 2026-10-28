//! The chart grids: drawn by the chart, behind its data, with the minor lines at the Charts screen's
//! "Minor grid lines" strength.
//!
//! egui_plot's own grid cannot have its minor lines faded without also changing its axis labels, since a
//! label and its line share one spacing. So the plot draws no grid (`show_grid(false)`, background off)
//! and the lines are painted here, into a slot reserved before the plot. Lines follow the plot's transform,
//! and no plot items are added, so the scale and the labels are exactly egui_plot's.
//!
//! Each line is drawn the way egui_plot draws its grid: the text colour times its strength to the `FADE`
//! power, the strength coming from its spacing on screen. The major lines (the coarsest level) use that
//! colour; the minor lines use the major colour times the slider.

use egui::epaint::RectShape;
use egui::{Color32, Rect, Shape, Stroke, StrokeKind, Ui, layers::ShapeIdx};
use egui_plot::{GridInput, PlotTransform, log_grid_spacer};

/// egui_plot's grid spacing range in points: a line this far apart is at full strength.
const SPACING_MIN: f64 = 8.0;
const SPACING_MAX: f64 = 300.0;
/// The grid fade (egui_plot's default): a line's alpha is its strength to this power.
const FADE: f64 = 0.5;

/// Reserves the place for a chart's grid in the painter, before the plot is drawn, so the grid is painted
/// under the plot's data. Pass the slot to `paint` once the plot is drawn.
pub(crate) fn reserve(ui: &Ui) -> ShapeIdx {
    ui.painter().add(Shape::Noop)
}

/// Fills the reserved slot with the plot's background (the same fill egui_plot paints) and the grid of the
/// plot whose transform is `transform`. `rect` is the plot widget's rectangle. `base` is the text colour;
/// `minor` is the minor lines' strength (0 none, 1 as dark as the major lines).
pub(crate) fn paint(
    ui: &Ui,
    slot: ShapeIdx,
    rect: Rect,
    transform: &PlotTransform,
    base: Color32,
    minor: f32,
) {
    let mut shapes = vec![Shape::from(RectShape::new(
        rect,
        2,
        ui.visuals().extreme_bg_color,
        ui.visuals().widgets.noninteractive.bg_stroke,
        StrokeKind::Inside,
    ))];
    shapes.extend(grid_shapes(transform, base, minor));
    ui.painter().set(slot, Shape::Vec(shapes));
}

/// The grid lines of both axes, in screen coordinates.
fn grid_shapes(transform: &PlotTransform, base: Color32, minor: f32) -> Vec<Shape> {
    let frame: Rect = *transform.frame();
    let bounds = transform.bounds();
    let ranges = [bounds.range_x(), bounds.range_y()];
    let value_per_point = transform.dvalue_dpos();
    let points_per_value = transform.dpos_dvalue();
    let mut shapes = Vec::new();
    for axis in 0..2 {
        let input = GridInput {
            bounds: (*ranges[axis].start(), *ranges[axis].end()),
            base_step_size: value_per_point[axis].abs() * SPACING_MIN,
        };
        let marks = log_grid_spacer(10)(input);
        let Some(coarse) = marks.iter().map(|m| m.step_size).reduce(f64::max) else {
            continue;
        };
        let major_color =
            base.gamma_multiply(strength(points_per_value[axis], coarse).powf(FADE) as f32);
        // Lines at one value are drawn once, in the major colour if any of their levels is major.
        let mut values: Vec<(f64, bool)> = marks
            .iter()
            .filter(|m| points_per_value[axis].abs() * m.step_size > SPACING_MIN)
            .map(|m| (m.value, m.step_size >= coarse))
            .collect();
        values.sort_by(|a, b| a.0.total_cmp(&b.0));
        values.dedup_by(|later, kept| {
            let same = (later.0 - kept.0).abs() < 1e-9 * kept.0.abs().max(1.0);
            kept.1 |= same && later.1;
            same
        });
        for (value, major) in values {
            let color = if major {
                major_color
            } else if minor > 0.0 {
                major_color.gamma_multiply(minor)
            } else {
                continue;
            };
            let stroke = Stroke::new(1.0, color);
            if axis == 0 {
                let x = transform.position_from_point_x(value);
                if x >= frame.min.x && x <= frame.max.x {
                    shapes.push(Shape::line_segment(
                        [egui::pos2(x, frame.min.y), egui::pos2(x, frame.max.y)],
                        stroke,
                    ));
                }
            } else {
                let y = transform.position_from_point_y(value);
                if y >= frame.min.y && y <= frame.max.y {
                    shapes.push(Shape::line_segment(
                        [egui::pos2(frame.min.x, y), egui::pos2(frame.max.x, y)],
                        stroke,
                    ));
                }
            }
        }
    }
    shapes
}

/// egui_plot's strength of a line `step` value apart on an axis with `points_per_value`.
fn strength(points_per_value: f64, step: f64) -> f64 {
    ((points_per_value * step - SPACING_MIN) / (SPACING_MAX - SPACING_MIN)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Strength is 0 at or below the minimum spacing and 1 at or above the maximum.
    fn strength_is_clamped_to_the_spacing_range() {
        assert_eq!(strength(1.0, 8.0), 0.0);
        assert_eq!(strength(1.0, 300.0), 1.0);
        assert!((strength(1.0, 154.0) - 0.5).abs() < 1e-9);
    }
}
