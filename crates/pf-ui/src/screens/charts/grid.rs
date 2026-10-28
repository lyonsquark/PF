//! The chart grids. Charts with numbers on an axis get the lines egui_plot's grid puts at least
//! `MAJOR_SPACING` points apart, so a few values show on each axis. The waterfall gets one line per bar.

use egui_plot::{GridInput, GridMark, log_grid_spacer};

/// egui_plot's grid spacing in points; its spacer's base step is this many points of the axis.
const SPACING_MIN: f64 = 8.0;
/// The least distance between two major lines, in points: enough room for a value's label.
const MAJOR_SPACING: f64 = 60.0;

/// The grid spacer of the value axes: egui_plot's levels, keeping only lines `MAJOR_SPACING` apart.
pub(crate) fn major_only(input: GridInput) -> Vec<GridMark> {
    let Some(points_per_value) = points_per_value(&input) else {
        return Vec::new();
    };
    log_grid_spacer(10)(input)
        .into_iter()
        .filter(|m| points_per_value * m.step_size >= MAJOR_SPACING)
        .collect()
}

/// The grid spacer of a category axis: one line at each of the `count` bars (at whole numbers, as the bars
/// are placed).
pub(crate) fn per_bar(count: usize) -> impl Fn(GridInput) -> Vec<GridMark> {
    move |input| {
        let (lo, hi) = input.bounds;
        (0..count)
            .filter(|&i| (i as f64) >= lo && (i as f64) <= hi)
            .map(|i| GridMark {
                value: i as f64,
                step_size: 1.0,
            })
            .collect()
    }
}

/// Points per unit of value on the axis, from egui_plot's base step (the value per point times 8).
fn points_per_value(input: &GridInput) -> Option<f64> {
    (input.base_step_size > 0.0).then(|| SPACING_MIN / input.base_step_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Every line of the value axes is at least `MAJOR_SPACING` points from the next one.
    fn major_lines_are_spaced_for_labels() {
        let input = GridInput {
            bounds: (0.0, 1000.0),
            base_step_size: SPACING_MIN, // one point per unit
        };
        let marks = major_only(input);
        assert!(marks.len() >= 3, "a few lines: {}", marks.len());
        assert!(marks.iter().all(|m| m.step_size >= MAJOR_SPACING));
    }

    #[test]
    /// The bar grid has one line per bar inside the view.
    fn per_bar_gives_one_line_per_bar() {
        let marks = per_bar(4)(GridInput {
            bounds: (-0.5, 2.5),
            base_step_size: SPACING_MIN,
        });
        let values: Vec<f64> = marks.iter().map(|m| m.value).collect();
        assert_eq!(values, vec![0.0, 1.0, 2.0]);
    }
}
