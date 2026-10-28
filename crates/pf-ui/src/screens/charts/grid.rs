//! The chart grids: egui_plot's own grid, with the minor lines at the Charts screen's "Minor grid lines"
//! strength.
//!
//! egui_plot draws a grid line in the text colour times its strength to the `FADE` power. The strength
//! comes from the line's spacing on screen, which is `step_size` times the axis scale. The coarsest level
//! (the major lines) keeps its real spacing. The finer levels are given the spacing that yields the
//! slider's alpha: the slider times the major alpha, from none (0) to the same (1). Items are not drawn,
//! so the plot's scale is unchanged.

use std::ops::RangeInclusive;

use egui_plot::{GridInput, GridMark, log_grid_spacer};

/// egui_plot's grid spacing range in points: a line this far apart is at full strength.
const SPACING_MIN: f64 = 8.0;
const SPACING_MAX: f64 = 300.0;
/// The grid fade the charts use (egui_plot's default): a line's alpha is its strength to this power.
pub(crate) const FADE: f32 = 0.5;

/// Wraps an axis label formatter so that minor marks get no label. egui_plot places a label where it draws
/// a line and fades it by the same spacing, so the minor marks' labels would show up at full slider. The
/// minor marks have a negative `step_size`: egui_plot takes its absolute value for spacing, so they are
/// placed and faded as `spacer` says, and only the formatter sees the sign.
pub(crate) fn hide_minor(
    format: impl Fn(GridMark, &RangeInclusive<f64>) -> String,
) -> impl Fn(GridMark, &RangeInclusive<f64>) -> String {
    move |mark, range| {
        if mark.step_size < 0.0 {
            String::new()
        } else {
            format(mark, range)
        }
    }
}

/// The grid spacer for one axis of a chart, with its minor lines at `minor` (0 to 1) of the major lines'
/// alpha. Set on both axes with `x_grid_spacer` and `y_grid_spacer`.
pub(crate) fn spacer(minor: f32) -> impl Fn(GridInput) -> Vec<GridMark> {
    move |input| {
        let base_step = input.base_step_size;
        let mut marks = log_grid_spacer(10)(input);
        let Some(coarse) = marks.iter().map(|m| m.step_size).reduce(f64::max) else {
            return marks;
        };
        if minor <= 0.0 {
            marks.retain(|m| m.step_size >= coarse);
            return marks;
        }
        if base_step <= 0.0 {
            return marks;
        }
        // egui_plot passes the value per point times SPACING_MIN, so points per value follow from it.
        let points_per_value = SPACING_MIN / base_step;
        let major_strength = strength(points_per_value, coarse);
        // alpha = strength^FADE, so the minor alpha is `minor` times the major alpha when its strength is
        // minor^(1/FADE) times the major strength.
        let minor_strength = f64::from(minor).powf(f64::from(1.0 / FADE)) * major_strength;
        let minor_spacing = SPACING_MIN + minor_strength * (SPACING_MAX - SPACING_MIN);
        let minor_step = minor_spacing / points_per_value;
        for mark in &mut marks {
            if mark.step_size < coarse {
                mark.step_size = -minor_step;
            }
        }
        marks
    }
}

/// egui_plot's strength of a line `step` value apart.
fn strength(points_per_value: f64, step: f64) -> f64 {
    ((points_per_value * step - SPACING_MIN) / (SPACING_MAX - SPACING_MIN)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Marks for an axis of 1000 units at `points_per_value` points per unit.
    fn marks_at(minor: f32, points_per_value: f64) -> Vec<GridMark> {
        let input = GridInput {
            bounds: (0.0, 1000.0),
            base_step_size: SPACING_MIN / points_per_value,
        };
        spacer(minor)(input)
    }

    /// The alpha egui_plot would draw a mark with: its strength to the fade power.
    fn alpha(points_per_value: f64, mark: &GridMark) -> f64 {
        strength(points_per_value, mark.step_size.abs()).powf(f64::from(FADE))
    }

    #[test]
    /// At 0 the minor lines are gone: every mark is a major (coarsest level) line.
    fn minor_zero_keeps_only_major_lines() {
        let marks = marks_at(0.0, 0.5);
        let coarse = marks.iter().map(|m| m.step_size).fold(0.0, f64::max);
        assert!(!marks.is_empty());
        assert!(marks.iter().all(|m| m.step_size == coarse));
    }

    #[test]
    /// At 1 the minor lines have the same alpha as the major lines.
    fn minor_one_matches_major_alpha() {
        let ppv = 0.5;
        let marks = marks_at(1.0, ppv);
        let coarse = marks.iter().map(|m| m.step_size).fold(0.0, f64::max);
        let major = alpha(
            ppv,
            marks
                .iter()
                .find(|m| m.step_size == coarse)
                .expect("a major mark"),
        );
        for m in marks.iter().filter(|m| m.step_size < coarse) {
            let a = alpha(ppv, m);
            assert!((a - major).abs() < 1e-6, "minor alpha {a} vs major {major}");
        }
    }

    #[test]
    /// At 0.5 the minor lines have half the major lines' alpha.
    fn minor_half_is_half_the_major_alpha() {
        let ppv = 0.5;
        let marks = marks_at(0.5, ppv);
        let coarse = marks.iter().map(|m| m.step_size).fold(0.0, f64::max);
        let major = alpha(
            ppv,
            marks
                .iter()
                .find(|m| m.step_size == coarse)
                .expect("a major mark"),
        );
        let minors: Vec<&GridMark> = marks.iter().filter(|m| m.step_size < coarse).collect();
        assert!(!minors.is_empty(), "there are minor lines to test");
        for m in minors {
            let a = alpha(ppv, m);
            assert!(
                (a - 0.5 * major).abs() < 1e-6,
                "minor alpha {a} vs half of {major}"
            );
        }
    }
}

#[cfg(test)]
mod label_tests {
    use super::*;

    #[test]
    /// A minor mark (negative step) gets no label; a major mark keeps its text.
    fn minor_marks_have_no_label() {
        let label = hide_minor(|mark, _| format!("{}", mark.value));
        let range = 0.0..=10.0;
        assert_eq!(
            label(
                GridMark {
                    value: 5.0,
                    step_size: -2.0
                },
                &range
            ),
            ""
        );
        assert_eq!(
            label(
                GridMark {
                    value: 5.0,
                    step_size: 10.0
                },
                &range
            ),
            "5"
        );
    }
}
