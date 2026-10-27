//! Money flow (T24b, card 9, R-CHT-9): a Sankey diagram of the month. Nodes are the income sources, the
//! expense groups and the subcategories, and the links are the flows between them. The layout is pure
//! (`layout_sankey`, unit-tested); the card paints the ribbons as convex quads and handles the clicks.
//! A click on a node opens the transactions behind it (`sankey_drill` in pf-core picks them).

use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Shape, Stroke, Ui, Vec2};
use egui_extras::DatePickerButton;
use pf_core::charts::sankey::{Sankey, sankey, sankey_drill};
use pf_core::charts::{ChartFilter, NOT_ENOUGH_DATA};
use pf_core::format;

use super::State;
use super::style::{hex_color, to_f64};
use crate::app::UiCtx;
use crate::widgets::colors;

/// Height of the card's drawing area, in points.
const AREA_HEIGHT: f32 = 340.0;
/// Padding inside the area: left, right (room for the labels), top, bottom, in points.
const PAD_LEFT: f32 = 60.0;
const PAD_RIGHT: f32 = 110.0;
const PAD_TOP: f32 = 16.0;
const PAD_BOTTOM: f32 = 16.0;
/// Width of a node, and the vertical gap between nodes of one column, in points.
const NODE_WIDTH: f32 = 18.0;
const NODE_GAP: f32 = 14.0;
/// Segments per ribbon curve.
const SEGMENTS: usize = 24;

/// One node, placed.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NodeRect {
    /// The node's name (its key for the drill-down).
    pub name: String,
    /// Where it is drawn.
    pub rect: Rect,
    /// Its colour.
    pub color: Color32,
    /// Its column (depth from the sources).
    pub column: usize,
    /// Its value: the larger of the flow in and the flow out.
    pub value: f64,
}

/// One flow, placed: it leaves `source` at `y0_top` and enters `target` at `y1_top`, `thickness` tall.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LinkRibbon {
    pub source: usize,
    pub target: usize,
    pub y0_top: f32,
    pub y1_top: f32,
    pub thickness: f32,
    pub value: f64,
}

/// The placed diagram.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SankeyLayout {
    pub nodes: Vec<NodeRect>,
    pub links: Vec<LinkRibbon>,
}

/// Places the nodes in columns (sources on the left) and stacks the links on their ends. Every column is
/// scaled by the same factor, so equal values have equal heights (R-CHT-9).
pub(crate) fn layout_sankey(data: &Sankey, area: Rect) -> SankeyLayout {
    let n = data.nodes.len();
    if n == 0 {
        return SankeyLayout {
            nodes: Vec::new(),
            links: Vec::new(),
        };
    }
    // Column: one more than the deepest source. Repeating n times settles any chain.
    let mut column = vec![0usize; n];
    for _ in 0..n {
        for l in &data.links {
            if l.source < n && l.target < n {
                column[l.target] = column[l.target].max(column[l.source] + 1);
            }
        }
    }
    let columns = column.iter().copied().max().unwrap_or(0) + 1;
    // Flow in and out of each node; a node's value is the larger of the two.
    let mut inflow = vec![0.0f64; n];
    let mut outflow = vec![0.0f64; n];
    for l in &data.links {
        if l.source < n && l.target < n {
            let v = to_f64(l.value);
            outflow[l.source] += v;
            inflow[l.target] += v;
        }
    }
    let value: Vec<f64> = (0..n).map(|i| inflow[i].max(outflow[i])).collect();
    let inner = Rect::from_min_max(
        area.min + Vec2::new(PAD_LEFT, PAD_TOP),
        area.max - Vec2::new(PAD_RIGHT, PAD_BOTTOM),
    );
    // One scale for all columns: the tightest column decides it.
    let mut k = f64::INFINITY;
    for c in 0..columns {
        let members: Vec<usize> = (0..n).filter(|i| column[*i] == c).collect();
        let sum: f64 = members.iter().map(|i| value[*i]).sum();
        if sum > 0.0 {
            let gaps = f64::from(NODE_GAP) * members.len().saturating_sub(1) as f64;
            k = k.min((f64::from(inner.height()) - gaps) / sum);
        }
    }
    if !k.is_finite() {
        k = 0.0;
    }
    let mut nodes = Vec::with_capacity(n);
    for c in 0..columns {
        let x = if columns > 1 {
            inner.left() + (inner.width() - NODE_WIDTH) * c as f32 / (columns - 1) as f32
        } else {
            inner.left()
        };
        let mut y = inner.top();
        for i in (0..n).filter(|i| column[*i] == c) {
            let h = (value[i] * k) as f32;
            nodes.push((
                i,
                NodeRect {
                    name: data.nodes[i].name.clone(),
                    rect: Rect::from_min_size(Pos2::new(x, y), Vec2::new(NODE_WIDTH, h)),
                    color: hex_color(&data.nodes[i].color),
                    column: c,
                    value: value[i],
                },
            ));
            y += h + NODE_GAP;
        }
    }
    nodes.sort_by_key(|(i, _)| *i);
    let nodes: Vec<NodeRect> = nodes.into_iter().map(|(_, n)| n).collect();
    // Links stack on the source's right edge and the target's left edge, in link order.
    let mut out_off = vec![0.0f32; n];
    let mut in_off = vec![0.0f32; n];
    let mut links = Vec::with_capacity(data.links.len());
    for l in &data.links {
        if l.source >= n || l.target >= n {
            continue;
        }
        let thickness = (to_f64(l.value) * k) as f32;
        let y0 = nodes[l.source].rect.top() + out_off[l.source];
        let y1 = nodes[l.target].rect.top() + in_off[l.target];
        out_off[l.source] += thickness;
        in_off[l.target] += thickness;
        links.push(LinkRibbon {
            source: l.source,
            target: l.target,
            y0_top: y0,
            y1_top: y1,
            thickness,
            value: to_f64(l.value),
        });
    }
    SankeyLayout { nodes, links }
}

/// The quads of one ribbon: `SEGMENTS` convex pieces along an S-curve from (x0, y0) to (x1, y1), each
/// `thickness` tall. The curve is smoothstep in y, so the ends are horizontal.
pub(crate) fn ribbon_quads(x0: f32, y0: f32, x1: f32, y1: f32, thickness: f32) -> Vec<[Pos2; 4]> {
    let at = |t: f32| {
        let s = t * t * (3.0 - 2.0 * t);
        (x0 + (x1 - x0) * t, y0 + (y1 - y0) * s)
    };
    (0..SEGMENTS)
        .map(|i| {
            let t0 = i as f32 / SEGMENTS as f32;
            let t1 = (i + 1) as f32 / SEGMENTS as f32;
            let (xa, ya) = at(t0);
            let (xb, yb) = at(t1);
            [
                Pos2::new(xa, ya),
                Pos2::new(xb, yb),
                Pos2::new(xb, yb + thickness),
                Pos2::new(xa, ya + thickness),
            ]
        })
        .collect()
}

/// The money-flow card: the month, the totals, a date picker, the diagram, and node clicks.
pub fn card(ui: &mut Ui, cx: &UiCtx, st: &mut State, f: &ChartFilter, drill: &mut super::Drill) {
    let db = cx.engine.db();
    let revision = cx.engine.revision();
    let base = st.sankey_date.unwrap_or(f.today);
    let key = ((f.accounts.clone(), f.scenario, f.today), base);
    let data = st
        .sankey
        .get(revision, &key, || sankey(db, f, base))
        .clone();
    ui.horizontal(|ui| {
        ui.label(RichText::new(&data.month_label).size(22.0).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut date = base;
            ui.add(
                DatePickerButton::new(&mut date)
                    .id_salt("sankey_date")
                    .show_icon(false),
            );
            if date != base {
                st.sankey_date = Some(date);
            }
        });
    });
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("In {}", format::format_currency(data.total_in)))
                .color(colors::GREEN),
        );
        ui.label(
            RichText::new(format!("Out {}", format::format_currency(data.total_out)))
                .color(colors::RED),
        );
    });
    if data.nodes.is_empty() {
        ui.label(RichText::new(NOT_ENOUGH_DATA).weak());
        return;
    }
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, AREA_HEIGHT), Sense::click());
    let layout = layout_sankey(&data, rect);
    let pointer = resp.hover_pos();
    let hovered_node = pointer.and_then(|p| {
        layout
            .nodes
            .iter()
            .position(|n| n.rect.expand(2.0).contains(p))
    });
    let painter = ui.painter_at(rect);
    for link in &layout.links {
        let (x0, x1) = (
            layout.nodes[link.source].rect.right(),
            layout.nodes[link.target].rect.left(),
        );
        let mut color = layout.nodes[link.source].color.gamma_multiply(0.25);
        if hovered_node.is_some_and(|i| i == link.source || i == link.target) {
            color = layout.nodes[link.source].color.gamma_multiply(0.9);
        }
        for quad in ribbon_quads(x0, link.y0_top, x1, link.y1_top, link.thickness) {
            painter.add(Shape::convex_polygon(quad.to_vec(), color, Stroke::NONE));
        }
    }
    for node in &layout.nodes {
        painter.rect_filled(node.rect, 2.0, node.color);
        let label_pos = Pos2::new(node.rect.right() + 6.0, node.rect.center().y);
        painter.text(
            label_pos,
            Align2::LEFT_CENTER,
            &node.name,
            FontId::proportional(12.0),
            ui.visuals().text_color(),
        );
    }
    if resp.clicked()
        && let Some(i) = pointer.and_then(|p| {
            layout
                .nodes
                .iter()
                .position(|n| n.rect.expand(2.0).contains(p))
        })
    {
        let name = layout.nodes[i].name.clone();
        *drill = Some(sankey_drill(db, f, base, &name));
    }
    if let Some(i) = hovered_node {
        let node = &layout.nodes[i];
        let amount = format::format_currency(
            rust_decimal::Decimal::try_from(node.value).unwrap_or_default(),
        );
        resp.on_hover_ui_at_pointer(|ui| {
            ui.label(RichText::new(&node.name).strong());
            ui.label(amount);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_core::charts::sankey::{SankeyLink, SankeyNode};
    use rust_decimal::Decimal;

    /// Salary (0) to Housing (100) and Savings (200); Housing to Rent (100).
    fn sample() -> Sankey {
        let node = |name: &str| SankeyNode {
            name: name.to_string(),
            color: "#007aff".to_string(),
        };
        Sankey {
            month_label: "March 2026".to_string(),
            total_in: Decimal::from(300),
            total_out: Decimal::from(200),
            nodes: vec![
                node("Salary"),
                node("Housing"),
                node("Savings"),
                node("Rent"),
            ],
            links: vec![
                SankeyLink {
                    source: 0,
                    target: 1,
                    value: Decimal::from(100),
                },
                SankeyLink {
                    source: 0,
                    target: 2,
                    value: Decimal::from(200),
                },
                SankeyLink {
                    source: 1,
                    target: 3,
                    value: Decimal::from(100),
                },
            ],
        }
    }

    fn area() -> Rect {
        Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(800.0, 340.0))
    }

    #[test]
    /// Covers: R-CHT-9. Sources are in column 0; a node one link away is in column 1; the last in column 2.
    fn sankey_layout_columns() {
        let layout = layout_sankey(&sample(), area());
        let cols: Vec<usize> = layout.nodes.iter().map(|n| n.column).collect();
        assert_eq!(cols, vec![0, 1, 1, 2]);
    }

    #[test]
    /// Covers: R-CHT-9. A node worth twice another is twice as tall, and everything fits the area.
    fn sankey_layout_heights_proportional() {
        let layout = layout_sankey(&sample(), area());
        let housing = layout.nodes[1].rect.height();
        let savings = layout.nodes[2].rect.height();
        assert!(
            (savings - 2.0 * housing).abs() <= 1.0,
            "savings {savings}, housing {housing}"
        );
        for n in &layout.nodes {
            assert!(
                n.rect.top() >= 0.0 && n.rect.bottom() <= 340.0,
                "{:?}",
                n.rect
            );
        }
    }

    #[test]
    /// The ribbons out of one node tile its height exactly: no gaps, no overlap.
    fn sankey_links_stack_without_overlap() {
        let layout = layout_sankey(&sample(), area());
        let salary = &layout.nodes[0];
        let out: f32 = layout
            .links
            .iter()
            .filter(|l| l.source == 0)
            .map(|l| l.thickness)
            .sum();
        assert!(
            (out - salary.rect.height()).abs() < 0.01,
            "out {out}, height {}",
            salary.rect.height()
        );
        let mut tops: Vec<f32> = layout
            .links
            .iter()
            .filter(|l| l.source == 0)
            .map(|l| l.y0_top)
            .collect();
        tops.sort_by(f32::total_cmp);
        assert!(
            (tops[0] - salary.rect.top()).abs() < 0.01,
            "first ribbon starts at the top"
        );
    }

    #[test]
    /// Each ribbon is made of convex quads that run from the source to the target.
    fn ribbon_is_made_of_segments() {
        let quads = ribbon_quads(0.0, 10.0, 100.0, 50.0, 8.0);
        assert_eq!(quads.len(), SEGMENTS);
        assert!(
            (quads[0][0].x - 0.0).abs() < 1e-3 && (quads[SEGMENTS - 1][1].x - 100.0).abs() < 1e-3
        );
    }
}
