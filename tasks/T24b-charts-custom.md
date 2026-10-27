# T24b: Donut, waterfall and Sankey charts (custom painting + drill-down)

**Phase:** 3 · **Deps:** T24a · **Crate(s):** `pf-ui` · **Size:** large (≈500 lines + 200 lines tests)

## Goal

Fill the three placeholder cards from T24a: **Categorical Breakdown** (a donut drawn with `egui::Painter`),
**Monthly Cashflow Waterfall** (egui_plot bars using `base_offset`), and **Money Flow** (a Sankey diagram drawn with
`egui::Painter`). Add hover tooltips and the click drill-downs that match the Swift `ChartEvent` handling.

> **Shell contract (T13 + T24a).** Cards live in `screens/charts/{breakdown,waterfall,sankey}.rs`, each
> `pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut egui::Ui)` with T24a's charts `State` (`breakdown_period`, `waterfall_date`,
> `sankey_date`, plus a `Cached<K, V>` per card), `charts::filter(cx, st) -> ChartFilter`, `chart_card`, and `style::{GREY, money_axis,
> hex_color}`. Drill-downs open T23's dialog with `dialogs::report_drilldown::open_transactions(cx, title, ids)`.
> Data (T08b, `pf_core::charts`): `BREAKDOWN_PERIODS` + `category_breakdown(db, &f, period) -> Vec<Slice { name, amount, color }>`;
> `waterfall(db, &f, base_date) -> Waterfall { month_label, total_in, total_out, bars: Vec<WaterfallBar { label, y_start, y_end, kind:
> BarKind::{Income, Expense, Other, Savings}, color }> }`; `waterfall_drill(db, &f, base_date, label) -> (title, Vec<TransactionId>)`;
> `sankey(db, &f, base_date) -> Sankey { month_label, total_in, total_out, nodes: Vec<SankeyNode { name, color }>, links:
> Vec<SankeyLink { source, target, value }> }` (links index into nodes); `sankey_drill(db, &f, base_date, node) -> (title, ids)`;
> `NOT_ENOUGH_DATA`. Fallback: if a name here differs from the code, use the actual name in `pf-core`/`pf-ui`. Never compute chart
> data or drill-down selections in the UI.

## Read first

- Swift: `../PFApp/Sources/PFApp/Views/Charts/CategoryBreakdownChartView.swift`, `CashflowWaterfallChartView.swift`
  (`handleBarTap`, `topCategoryNames`), `SankeyChartView.swift` (`handleNodeTap`, `chartJSON`), `WebChart.swift` (`ChartEvent`, `categoryHex`)
- HTML (look and feel): `../PFApp/Sources/PFApp/WebCharts/{donut,waterfall,sankey}.html`
- `docs/egui-cheatsheet.md` → "Custom painting", "Plots"
- Rules: R-CHT-5, R-CHT-6, R-CHT-9, R-CHT-10, R-CHT-11 (only waterfall + Sankey respond to clicks)
- Task: `tasks/T08b-chart-data.md` (signatures above)

## ChartEvent handling in Swift (what to reproduce)

| Chart | Swift event | Swift action | Rust action |
|---|---|---|---|
| Donut (breakdown) | `drillDown(category)` posted on slice click | **ignored** (the view passes no `onEvent`) | Hover tooltip only, no click action |
| Waterfall | `drillDown(label)` on bar click | `handleBarTap` ⇒ transactions sheet titled `"{label} — {Month YYYY}"` | Same, via `open_transactions` |
| Sankey | `drillDown(name)` on **node** click (not links) | `handleNodeTap` ⇒ sheet titled `"{name trimmed} — {Month YYYY}"` | Same, via `open_transactions` |

The drill-down selection rules (Income / Savings / Other / category for the waterfall; Savings / name match for the Sankey) are
implemented and tested in T08b's `waterfall_drill` / `sankey_drill`. The UI only passes the clicked label or node name.

## Deliverables

```rust
// crates/pf-ui/src/screens/charts/breakdown.rs   (card 5)
pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut egui::Ui);
pub(crate) fn donut_wedge(center: Pos2, r_in: f32, r_out: f32, a0: f32, a1: f32, fill: Color32) -> Vec<egui::Shape>;
pub(crate) fn hit_wedge(center: Pos2, r_in: f32, r_out: f32, angles: &[(f32, f32)], p: Pos2) -> Option<usize>;

// crates/pf-ui/src/screens/charts/waterfall.rs   (card 6)
pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut egui::Ui);
/// base = min(y_start, y_end), height = |y_end − y_start|, fill = hex_color(&bar.color) (T08b already picks the colours).
pub(crate) fn waterfall_bars(bars: &[WaterfallBar]) -> Vec<egui_plot::Bar>;

// crates/pf-ui/src/screens/charts/sankey.rs      (card 9)
pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut egui::Ui);
pub(crate) struct SankeyLayout { pub nodes: Vec<NodeRect>, pub links: Vec<LinkRibbon> }
pub(crate) struct NodeRect { pub name: String, pub rect: egui::Rect, pub color: Color32, pub column: usize, pub value: f64 }
pub(crate) struct LinkRibbon { pub source: usize, pub target: usize, pub y0_top: f32, pub y1_top: f32, pub thickness: f32, pub value: f64 }
/// Pure layout over T08b's `Sankey` (links already index into nodes); unit-testable without a Ui.
pub(crate) fn layout_sankey(data: &Sankey, area: egui::Rect) -> SankeyLayout;
```

## Card 5: Categorical Breakdown (donut)

```
$3,210.00 Total (headline)                                   [1M ▾]  (1M / YTD / 1Y / All; default 1M)
        ╭──────╮            ● Groceries      $820
      ╱  ╭──╮   ╲           ● Housing      $1,500
     │   │  │    │          ● Dining Out     $410
      ╲  ╰──╯   ╱           ● Other          $480
        ╰──────╯
```
* Allocate the card width × 250 px. Left ~60%: the donut, centred; radius `R = min(w, h)/2`; inner `0.44·R`, outer `0.72·R`
  (ECharts `radius: ['44%','72%']`). The first slice starts at 12 o'clock and goes clockwise.
* Each wedge is built from small angular steps (≤ 3° each). Every step is a **convex quad**
  `[outer(a), outer(a+d), inner(a+d), inner(a)]` added with `Shape::convex_polygon`. Leave a 1 px gap via a background-coloured stroke.
* Data = `category_breakdown(db, &f, st.breakdown_period)` (period ComboBox over `BREAKDOWN_PERIODS`). Colour = `hex_color(&slice.color)` (R-CHT-10; T08b gives "Other"/"Uncategorized" `#8e8e93`).
* Hover: hit-test with polar coordinates (distance between r_in and r_out, angle range). The hovered wedge draws at
  `r_out + 4`, and a tooltip (`response.clone().on_hover_ui_at_pointer(|ui| …)`, verified in egui 0.37) shows the name in bold,
  the currency value, and `{pct}% of total` (one decimal). In the centre, show `{name}` and `{pct}%` while hovering.
* Right ~40%: the legend, one row per slice: a colour dot, the name (≤ 110 px, truncated) and the value (weak, right-aligned).
  Scroll if needed.
* No click action (R-CHT-11). Empty data ⇒ `NOT_ENOUGH_DATA`.

## Card 6: Monthly Cashflow Waterfall

```
March 2026 (headline)                                                    [2026-03-15 📅]
⬇ $5,000.00 In (green)   ⬆ $3,800.00 Out (red)
 +$5.0k   -$1.5k   -$0.8k   …   +$1.2k     (value labels on top of each bar)
 ████
 ████  ▀▀▀▀
 ████       ▀▀▀  …
 Income  Housing  Groceries … Other  Savings     (x labels, rotated look not required)
```
* Data = `waterfall(db, &f, date)`. Header: `month_label` (headline), then `⬇ {total_in} In` (green) and `⬆ {total_out} Out` (red) in caption size;
  a `DatePickerButton` on the right (`id_salt("waterfall_date")`) choosing the month.
* Plot: one `BarChart` built by `waterfall_bars`: bar i at x = i, width 0.6, `base_offset(min(y_start, y_end))`,
  height `|y_end − y_start|`, fill `hex_color(&bar.color)` (T08b assigns Income green, Savings blue/red, and the expense palette cycle).
* X formatter: the label for integer marks. Y formatter: `money_axis`. Value labels above each bar (by `bar.kind`): Income `+$5.0k`,
  Savings `+$1.2k` or `-$0.3k`, expenses `-$1.5k` (one decimal of thousands, as in the HTML).
* Hover tooltip: the label in bold, plus `+$amount` (income), signed `y_end` (savings), or `-$amount` (expense).
* **Click** inside the plot: take `pointer_coordinate()`, `i = x.round()`; if `0 ≤ i < n` and the pointer y is within that
  bar's span, call `let (title, ids) = waterfall_drill(db, &f, date, &bars[i].label)` and `open_transactions(cx, title, ids)`.
* Disable zoom/drag/scroll. `bars` empty ⇒ `NOT_ENOUGH_DATA`.

## Card 9: Money Flow (Sankey)

```
March 2026                                                              [2026-03-15 📅]
⬇ $5,000.00 In   ⬆ $3,800.00 Out
 Salary ███════════════╗ ██ Housing ═══════ ██ Rent
                        ╚═ ██ Food    ═════ ██ Groceries
 Bonus  ██══════════════ ██ Savings
```
* Data = `sankey(db, &f, date)`. Header identical to the waterfall's (own date picker, `id_salt("sankey_date")`). Card height 340.
* **Layout** (`layout_sankey`, pure):
  1. **Columns** by depth: column 0 = nodes with no incoming link; column(n) = 1 + max(column of its sources). In practice:
     income sources (0) → expense parents + "Savings" (1) → subcategories (2).
  2. **Node value** = max(sum of incoming, sum of outgoing).
  3. Padding: left 60, right 110, top 16, bottom 16 (as in the HTML). Node width 18, vertical gap 14.
  4. Per column, the vertical scale `k = (height − gap·(n−1)) / Σ values`; use the **smallest k over all columns** for every column,
     so equal values have equal heights.
  5. Nodes keep the input order inside their column (T08b already sorts them by value desc).
  6. Column x positions spread evenly between the left and right padding.
  7. Links: thickness = value·k. Stack outgoing links on the source's right edge, and incoming links on the target's left edge,
     in link order.
* **Drawing**: each link is a ribbon from (source right x, y0) to (target left x, y1), filled with the source colour at alpha 0.25
  (0.9 when it is adjacent to the hovered node or is itself hovered). Approximate the cubic curve (control points at the
  horizontal midpoint, ECharts `curveness: 0.5`) with ~24 segments, emitting one **convex quad per segment** (top edge i..i+1,
  bottom edge i+1..i). Nodes are filled rects in the node colour. Labels (12 pt) go right of the node for columns 0 and 1,
  and right of the node for the last column too (there is a 110 px right padding for them).
* **Hover**: node ⇒ tooltip with the name in bold, and highlight its adjacent links. Link ⇒ tooltip `Source → Target` plus currency
  (no decimals). Hit-test links by sampling the ribbon's centre line (distance ≤ thickness/2).
* **Click** on a node ⇒ `let (title, ids) = sankey_drill(db, &f, date, &node.name)` ⇒ `open_transactions(cx, title, ids)`.
  Clicks on links do nothing.
* `nodes` empty ⇒ `NOT_ENOUGH_DATA` (T08b returns empty nodes when either side is empty, as Swift returns `null`).

## Steps

1. Unit tests first for the pure parts: `donut_wedge`/`hit_wedge`, `waterfall_bars`, `layout_sankey`.
2. Implement each card and replace the T24a placeholders.
3. kittest tests for the drill-downs.
4. Run Done-when, and look at each chart manually with `fixtures/SampleData3.md`.

## Tests (required)

Unit tests in each file, plus `crates/pf-ui/tests/ui_charts_custom.rs`.

| Test name | Covers | What it checks |
|---|---|---|
| `ui_cht_01_all_nine_render` | UI-CHT-01 | All 9 charts render with the fixture **and** with an empty Db (no panic, 3 frames each); empty ⇒ three "Not enough data" labels for these cards |
| `donut_hit_test_quadrants` | — | With slices [25%, 75%], a point at 1 o'clock hits slice 0 and a point at 6 o'clock hits slice 1; the centre hole hits nothing |
| `donut_wedge_is_convex_quads` | — | A 90° wedge yields ≥ 30 convex shapes, all points between r_in and r_out |
| `waterfall_bars_offsets` | R-CHT-6 | Points Income(0→5000), Housing(5000→3500), Savings(0→3500) ⇒ bars base 0/h 5000, base 3500/h 1500, base 0/h 3500 |
| `waterfall_bars_use_data_colors` | R-CHT-6 | Each bar's fill equals `hex_color(&bar.color)` from T08b |
| `ui_cht_waterfall_click_opens_drilldown` | R-CHT-6 | Simulate a click on the "Income" bar (call the click handler with a plot coordinate, or use kittest hover/click at the bar's screen position) ⇒ the dialog "Income — {Month YYYY} Transactions" lists only positive rows |
| `sankey_layout_columns` | R-CHT-9 | Salary→Housing, Salary→Savings, Housing→Rent ⇒ columns 0/1/1/2 |
| `sankey_layout_heights_proportional` | R-CHT-9 | A node with value 2× another is 2× as tall (±1 px), and every column fits inside the area |
| `sankey_links_stack_without_overlap` | — | The outgoing ribbons of one node tile its height exactly |
| `ui_cht_sankey_node_click_opens_drilldown` | R-CHT-9 | Clicking the "Savings" node opens "Savings — {Month YYYY} Transactions" listing all filtered rows |
| `ui_cht_donut_click_does_nothing` | R-CHT-11 | Clicking a donut wedge opens no dialog |
| `ui_cht_only_waterfall_and_sankey_clickable` | R-CHT-11 | Across all 9 cards, only waterfall bar and Sankey node clicks open `Dialog::ReportTransactions`; clicks on Sankey links do nothing |

## Pitfalls

- `Shape::convex_polygon` requires **convex** point lists. Never pass a whole wedge or a whole ribbon as one polygon.
- egui's y axis points down. In `layout_sankey`, keep everything in screen coordinates (top-left origin).
- Floating-point drift: compute k once and derive every rect from it; snap rects to whole pixels for crisp edges.
- Tooltips: use `Response::on_hover_ui_at_pointer(|ui| …)` (verified in `egui-0.37.0/src/response.rs`). There is **no**
  `show_tooltip_at_pointer` in 0.37. Allocate the chart area with `Sense::click()` (hover is included) to get the `Response`.
- Use `ui.painter_at(rect)` so drawing is clipped to the card.

## Done when

```bash
cargo test -p pf-ui --lib charts::
cargo test -p pf-ui --test ui_charts_custom
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules
```

## Out of scope

- Chart data and drill-down selection logic (T08b). The drill-down dialog itself (T23).
