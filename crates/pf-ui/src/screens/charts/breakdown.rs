//! Categorical breakdown (T24b, card 5, R-CHT-5): a donut of the spending by group, with the legend beside
//! it. Painted with `egui::Painter` as convex quads, one per small angle step. Hover shows the slice;
//! clicks do nothing (R-CHT-11).

use std::f32::consts::TAU;

use egui::{Color32, Pos2, RichText, Sense, Shape, Stroke, Ui, Vec2};
use pf_core::charts::ChartFilter;
use pf_core::charts::NOT_ENOUGH_DATA;
use pf_core::charts::Slice;
use pf_core::charts::categories::{BREAKDOWN_PERIODS, breakdown_drill, category_breakdown};
use pf_core::format;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;

use super::style::{hex_color, to_f64};
use super::{Drill, State};
use crate::app::UiCtx;
use crate::widgets::color::color_dot;

/// Height of the donut card's drawing area, in points.
const AREA_HEIGHT: f32 = 250.0;
/// The most degrees one quad covers, so a wedge is smooth.
const STEP_DEGREES: f32 = 3.0;

/// The inner and outer radius of the donut, as fractions of its half-size (ECharts 44 % / 72 %).
const INNER: f32 = 0.44 / 0.72;

/// Builds one donut wedge from `a0` to `a1` (radians, clockwise from twelve o'clock) as convex quads.
pub(crate) fn donut_wedge(
    center: Pos2,
    r_in: f32,
    r_out: f32,
    a0: f32,
    a1: f32,
    fill: Color32,
) -> Vec<Shape> {
    let step = STEP_DEGREES.to_radians();
    let steps = ((a1 - a0) / step).ceil().max(1.0);
    let d = (a1 - a0) / steps;
    let mut out = Vec::new();
    let mut a = a0;
    for _ in 0..usize::try_from(steps as i64).unwrap_or(1) {
        let b = a + d;
        let points = vec![
            polar(center, r_out, a),
            polar(center, r_out, b),
            polar(center, r_in, b),
            polar(center, r_in, a),
        ];
        out.push(Shape::convex_polygon(points, fill, Stroke::NONE));
        a = b;
    }
    out
}

/// The index of the wedge under `p`, if any. `angles` gives each wedge's start and end (radians, clockwise
/// from twelve o'clock). The hole and the area outside the outer radius hit nothing.
pub(crate) fn hit_wedge(
    center: Pos2,
    r_in: f32,
    r_out: f32,
    angles: &[(f32, f32)],
    p: Pos2,
) -> Option<usize> {
    let v = p - center;
    let dist = v.length();
    if dist < r_in || dist > r_out {
        return None;
    }
    // Clockwise from twelve o'clock: x is sin, and screen y points down, so the angle is atan2(x, -y).
    let angle = v.x.atan2(-v.y).rem_euclid(TAU);
    angles.iter().position(|(a0, a1)| {
        // How far clockwise the pointer is from this wedge's start, against the wedge's span.
        let from_start = (angle - a0).rem_euclid(TAU);
        from_start < (a1 - a0)
    })
}

/// A point at `r` from `center`, at angle `a` (clockwise from twelve o'clock).
fn polar(center: Pos2, r: f32, a: f32) -> Pos2 {
    center + Vec2::new(r * a.sin(), -r * a.cos())
}

/// The breakdown card: the total, the period picker, the donut and its legend.
pub fn card(ui: &mut Ui, cx: &UiCtx, st: &mut State, f: &ChartFilter, drill: &mut Drill) {
    let db = cx.engine.db();
    let revision = cx.engine.revision();
    let period = st.breakdown_period;
    let key = ((f.accounts.clone(), f.scenario, f.today), period);
    let slices = st
        .breakdown
        .get(revision, &key, || category_breakdown(db, f, period))
        .clone();
    let total: Decimal = slices.iter().map(|s| s.amount.abs()).sum();
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("{} Total", format::format_currency(total)))
                .size(22.0)
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let shown = BREAKDOWN_PERIODS
                .iter()
                .find(|(_, p)| *p == period)
                .map_or("1M", |(l, _)| l);
            let resp = egui::ComboBox::from_id_salt("breakdown_period")
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    for (label, p) in BREAKDOWN_PERIODS {
                        ui.selectable_value(&mut st.breakdown_period, p, label);
                    }
                });
            resp.response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::accesskit::Role::ComboBox, true, "Breakdown Period")
            });
        });
    });
    if slices.is_empty() || total <= Decimal::ZERO {
        ui.label(RichText::new(NOT_ENOUGH_DATA).weak());
        return;
    }
    ui.horizontal(|ui| {
        let width = ui.available_width();
        let (rect, resp) =
            ui.allocate_exact_size(Vec2::new(width * 0.6, AREA_HEIGHT), Sense::click());
        let center = rect.center();
        let half = rect.width().min(rect.height()) / 2.0;
        let (r_in, r_out) = (half * INNER, half);
        let angles = wedge_angles(&slices, total);
        let hovered = resp
            .hover_pos()
            .and_then(|p| hit_wedge(center, r_in, r_out, &angles, p));
        // A click on a wedge opens the transactions behind it (R-CHT-5).
        if resp.clicked()
            && let Some(i) = hovered
        {
            *drill = Some(breakdown_drill(db, f, period, &slices[i].name));
        }
        let painter = ui.painter_at(rect);
        for (i, ((a0, a1), slice)) in angles.iter().zip(&slices).enumerate() {
            let fill = hex_color(&slice.color);
            let (ri, ro) = if hovered == Some(i) {
                (r_in, r_out + 4.0)
            } else {
                (r_in, r_out)
            };
            for shape in donut_wedge(center, ri, ro, *a0, *a1, fill) {
                painter.add(shape);
            }
        }
        if let Some(i) = hovered {
            let slice = &slices[i];
            let pct = pct_of(slice.amount, total);
            resp.on_hover_ui_at_pointer(|ui| {
                ui.label(RichText::new(&slice.name).strong());
                ui.label(format::format_currency(slice.amount));
                ui.label(format!("{pct:.1}% of total"));
            });
        }
        ui.add_space(12.0);
        ui.vertical(|ui| {
            for slice in &slices {
                legend_row(ui, slice, total);
            }
        });
    });
}

/// Each slice's start and end angle (radians), clockwise from twelve o'clock, by its share of the total.
fn wedge_angles(slices: &[Slice], total: Decimal) -> Vec<(f32, f32)> {
    let mut a = 0.0_f32;
    slices
        .iter()
        .map(|s| {
            let share = (to_f64(s.amount.abs()) / to_f64(total)) as f32;
            let start = a;
            a += share * TAU;
            (start, a.min(start + TAU))
        })
        .collect()
}

/// A slice's share of the total, in percent.
fn pct_of(amount: Decimal, total: Decimal) -> f64 {
    if total.is_zero() {
        return 0.0;
    }
    (amount.abs() / total * Decimal::from(100))
        .to_f64()
        .unwrap_or(0.0)
}

/// One legend row: the colour dot, the name, and the value.
fn legend_row(ui: &mut Ui, slice: &Slice, total: Decimal) {
    ui.horizontal(|ui| {
        color_dot(ui, hex_color(&slice.color));
        let name: String = slice.name.chars().take(18).collect();
        ui.label(name);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(format!("{:.0}%", pct_of(slice.amount, total))).weak());
            ui.label(format::format_currency(slice.amount.abs()));
        });
    });
}

#[cfg(test)]
mod tests {
    use std::f32::consts::PI;

    use super::*;

    const C: Pos2 = Pos2::new(100.0, 100.0);

    #[test]
    /// Two slices of 25 % and 75 %: 1 o'clock hits the first, 6 o'clock the second, the hole hits nothing.
    fn donut_hit_test_quadrants() {
        let angles = vec![(0.0, PI / 2.0), (PI / 2.0, 2.0 * PI)];
        // 1 o'clock: 30 degrees clockwise from twelve.
        let one = polar(C, 40.0, 30f32.to_radians());
        assert_eq!(hit_wedge(C, 20.0, 50.0, &angles, one), Some(0));
        // 6 o'clock: 180 degrees.
        let six = polar(C, 40.0, PI);
        assert_eq!(hit_wedge(C, 20.0, 50.0, &angles, six), Some(1));
        assert_eq!(hit_wedge(C, 20.0, 50.0, &angles, C), None, "the hole");
    }

    #[test]
    /// A 90 degree wedge is made of at least 30 convex quads, all between the two radii.
    fn donut_wedge_is_convex_quads() {
        let shapes = donut_wedge(C, 20.0, 50.0, 0.0, PI / 2.0, Color32::RED);
        assert!(shapes.len() >= 30, "{} quads", shapes.len());
        for shape in &shapes {
            let Shape::Path(path) = shape else {
                panic!("a convex polygon is a path shape");
            };
            for p in &path.points {
                let r = (*p - C).length();
                assert!((19.9..=50.1).contains(&r), "radius {r}");
            }
        }
    }
}
