//! Initial table column widths sized to their text: the widest of the heading and the cells, plus
//! the cell padding. A column starts as wide as its content needs and no wider; the user can still
//! drag its edge. Measuring every cell of a large table each frame would be too slow, so the
//! callers pass the longest texts (see `longest`) and a template for fixed-format values.

use egui::{Color32, FontId, TextStyle, Ui};

/// Padding added to a text's width: the cell's left and right margins, in points.
const CELL_PADDING: f32 = 16.0;
/// The narrowest a fitted column may be, in points.
const MIN_WIDTH: f32 = 48.0;
/// The widest a fitted column may be, in points. Longer text is cut off, so one long memo cannot
/// push the other columns out of the window.
const MAX_WIDTH: f32 = 320.0;
/// At most this many candidate texts are measured for one column.
const MAX_MEASURED: usize = 40;

/// The width of `text` drawn in the body font, in points.
pub fn text_width(ui: &Ui, text: &str) -> f32 {
    let font: FontId = TextStyle::Body.resolve(ui.style());
    ui.painter()
        .layout_no_wrap(text.to_owned(), font, Color32::WHITE)
        .size()
        .x
}

/// A column width for `header` and `samples`: the widest of them, plus padding.
pub fn fit(ui: &Ui, header: &str, samples: &[&str]) -> f32 {
    let widest = samples
        .iter()
        .copied()
        .chain(std::iter::once(header))
        .map(|t| text_width(ui, t))
        .fold(0.0_f32, f32::max);
    (widest + CELL_PADDING).clamp(MIN_WIDTH, MAX_WIDTH)
}

/// The texts that are (near) the longest of `texts`, at most `MAX_MEASURED` of them. Length is
/// counted in characters; within two characters of the longest is treated as the widest candidates.
pub fn longest<'a>(texts: impl Iterator<Item = &'a str>) -> Vec<&'a str> {
    let all: Vec<&str> = texts.collect();
    let Some(max) = all.iter().map(|t| t.chars().count()).max() else {
        return Vec::new();
    };
    all.into_iter()
        .filter(|t| t.chars().count() + 2 >= max)
        .take(MAX_MEASURED)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// The longest texts are kept (near the maximum length), short ones are dropped, and the list is
    /// capped.
    fn longest_keeps_the_near_longest() {
        let got = longest(["a", "abcdef", "abcde", "x"].into_iter());
        assert_eq!(got, vec!["abcdef", "abcde"]);
        assert!(longest(std::iter::empty()).is_empty());
        let many = vec!["abcd"; 100];
        assert_eq!(longest(many.into_iter()).len(), MAX_MEASURED);
    }
}
