//! Chart styling shared by the charts (T24a): the grey series colour, the money axis labels of the
//! Swift charts, and the colour of a stored hex value. Plot coordinates are days since 1970 (`f64`).

use egui::Color32;
use jiff::civil::{Date, date};

use crate::widgets::colors;

/// The grey of the "previous" series and the budget line (`#8e8e93`).
pub const GREY: Color32 = Color32::from_rgb(0x8e, 0x8e, 0x93);

/// The day that plot x = 0 stands for.
pub const EPOCH: Date = date(1970, 1, 1);

/// Days since 1970-01-01, for a plot x coordinate.
pub fn day_number(d: Date) -> f64 {
    d.since(EPOCH).map_or(0.0, |s| f64::from(s.get_days()))
}

/// The date a plot x coordinate stands for (rounded to the day).
pub fn day_from_number(x: f64) -> Option<Date> {
    let days = i64::try_from(x.round() as i128).ok()?;
    EPOCH.checked_add(jiff::Span::new().days(days)).ok()
}

/// A money amount as a plot coordinate. Only the plot sees the float (money stays exact elsewhere).
pub fn to_f64(d: rust_decimal::Decimal) -> f64 {
    use rust_decimal::prelude::ToPrimitive;
    d.to_f64().unwrap_or(0.0)
}

/// The x-axis label for a day: "Mar 15" (or "Mar 2026" for a long range, the caller's choice).
pub fn day_axis(x: f64, long: bool) -> String {
    day_from_number(x).map_or_else(String::new, |d| {
        let fmt = if long { "%b %Y" } else { "%b %-d" };
        d.strftime(fmt).to_string()
    })
}

/// The money axis label of the Swift charts: `$12k` (`'$' + (v/1000).toFixed(0) + 'k'`).
pub fn money_axis(v: f64) -> String {
    format!("${:.0}k", v / 1000.0)
}

/// The net-worth axis: small values in dollars, larger ones in thousands.
pub fn money_axis_small(v: f64) -> String {
    if v.abs() < 1000.0 {
        format!("${v:.0}")
    } else {
        money_axis(v)
    }
}

/// The colour of a stored "#rrggbb" value (a category colour), blue when it is not a colour.
pub fn hex_color(hex: &str) -> Color32 {
    crate::widgets::color_from_hex(Some(hex))
}

/// The series colours used across the charts.
pub const INCOME: Color32 = colors::GREEN;
/// Expense colour.
pub const EXPENSE: Color32 = colors::RED;
/// Blue of the main series.
pub const MAIN: Color32 = colors::BLUE;

/// The hover text of a line chart: the value of the nearest data point, its x and y formatted by `x` and
/// `y`. Nothing is shown when the pointer is away from the data.
pub fn hover_label(
    pos: &egui_plot::HoverPosition<'_>,
    x: impl Fn(f64) -> String,
    y: impl Fn(f64) -> String,
) -> Option<String> {
    match pos {
        egui_plot::HoverPosition::NearDataPoint { position, .. } => {
            Some(format!("{}\n{}", x(position.x), y(position.y)))
        }
        egui_plot::HoverPosition::Elsewhere { .. } => None,
    }
}

/// A money amount to the cent, as the hover text shows it: "$1,234.56".
pub fn money_exact(v: f64) -> String {
    use rust_decimal::prelude::FromPrimitive;
    let amount = rust_decimal::Decimal::from_f64(v)
        .unwrap_or_default()
        .round_dp(2);
    pf_core::format::format_currency(amount)
}

/// The full date of a day number, as the hover text shows it: "Mar 15, 2026".
pub fn day_hover(x: f64) -> String {
    day_from_number(x).map_or_else(String::new, |d| d.strftime("%b %-d, %Y").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// The money axis matches the Swift label: thousands with "k", negatives keep their sign.
    fn money_axis_format() {
        assert_eq!(money_axis(12345.0), "$12k");
        assert_eq!(money_axis(-2000.0), "$-2k");
        assert_eq!(money_axis_small(950.0), "$950");
        assert_eq!(money_axis_small(45_000.0), "$45k");
    }

    #[test]
    /// A category's colour, as the charts draw it, is the colour of its stored hex (R-CHT-10).
    fn hex_color_matches_category_hex() {
        let hex = pf_core::charts::category_hex("Groceries");
        let [r, g, b, _] = pf_core::color::parse_hex(&hex).expect("a valid category hex");
        assert_eq!(hex_color(&hex), Color32::from_rgb(r, g, b));
        assert_eq!(hex_color("#ff0000"), Color32::from_rgb(255, 0, 0));
        let d = date(2026, 3, 15);
        assert_eq!(day_from_number(day_number(d)), Some(d));
        assert_eq!(day_number(EPOCH), 0.0);
    }

    #[test]
    /// The hover text shows money to the cent, with thousands separators and a sign.
    fn money_exact_shows_cents() {
        assert_eq!(money_exact(1234.5), "$1,234.50");
        assert_eq!(money_exact(-40.0), "-$40.00");
    }

    #[test]
    /// Away from the data there is no hover text.
    fn hover_label_is_empty_away_from_data() {
        let pos = egui_plot::HoverPosition::Elsewhere {
            position: egui_plot::PlotPoint::new(1.0, 2.0),
        };
        assert_eq!(
            hover_label(&pos, |x| x.to_string(), |y| y.to_string()),
            None
        );
    }
}
