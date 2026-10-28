//! The chart grids: only the major lines. egui_plot draws a line for each level of its grid, finest to
//! coarsest; `major_only` keeps the coarsest level. Its lines get the axis labels, so a few values show on
//! each axis and nothing else is drawn.

use egui_plot::{GridInput, GridMark, log_grid_spacer};

/// The grid spacer of every chart axis: egui_plot's log spacer with only its coarsest level kept.
pub(crate) fn major_only(input: GridInput) -> Vec<GridMark> {
    let marks = log_grid_spacer(10)(input);
    let Some(coarse) = marks.iter().map(|m| m.step_size).reduce(f64::max) else {
        return marks;
    };
    marks
        .into_iter()
        .filter(|m| m.step_size >= coarse)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Only marks of the coarsest level are kept: every kept mark has the same step.
    fn major_only_keeps_the_coarsest_level() {
        let marks = major_only(GridInput {
            bounds: (0.0, 1000.0),
            base_step_size: 8.0,
        });
        assert!(!marks.is_empty());
        let step = marks[0].step_size;
        assert!(marks.iter().all(|m| m.step_size == step));
    }
}
