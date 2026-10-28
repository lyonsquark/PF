//! Column widths sized to their text (`widgets::col_fit`): a short column is narrow, a very long one is
//! capped, and the widths are measured in a real frame.

use std::cell::Cell;

use pf_ui::widgets::col_fit::fit;

/// The fitted width of `header` and `samples`, measured in one headless frame.
fn measure(header: &str, samples: &[&str]) -> f32 {
    let width = Cell::new(0.0);
    let mut h = egui_kittest::Harness::new_ui(|ui| {
        width.set(fit(ui, header, samples));
    });
    h.run();
    width.get()
}

#[test]
/// A short column is much narrower than the old fixed widths (110 to 180 points), and never below the
/// minimum.
fn col_fit_short_column_is_narrow() {
    let date = measure("Date", &["Sep 30, 2026"]);
    assert!(date > 48.0 && date < 110.0, "date column is {date} points");
    let empty = measure("", &[]);
    assert!(empty >= 48.0, "minimum width is kept");
}

#[test]
/// A very long text does not make the column wider than the cap.
fn col_fit_long_text_is_capped() {
    let long = "This memo is much too long to fit in any sensible table column at all, really";
    let width = measure("Memo", &[long, long]);
    assert!(width <= 320.0, "capped at 320 points, got {width}");
}
