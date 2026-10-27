# egui 0.37 / eframe 0.37 / egui_extras 0.37 / egui_plot 0.38 cheat sheet

Everything below was **compiled and run** on 2026-10-26 (rustc 1.99) in a probe project.
egui changes its API often, and your memory of it is probably from an older version.
**Trust this file and the crate source over memory.** Source lives at
`~/.cargo/registry/src/index.crates.io-*/egui-0.37.0/src/` (after the first `cargo build`).

## Differences from older egui that will bite you

| Old (≤0.3x) | egui 0.37 |
|---|---|
| `impl eframe::App { fn update(&mut self, ctx: &egui::Context, frame) }` | **`fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame)`** is the required method. Optional `fn logic(&mut self, ctx, frame)` runs before it. Get the context with `ui.ctx()`. |
| `egui::SidePanel::left("id").show(ctx, …)` | **`egui::Panel::left("id").show(ui, …)`** (also `Panel::right/top/bottom`). Panels are shown **inside the root `Ui`**. |
| `egui::TopBottomPanel::top("id")` | `egui::Panel::top("id")` |
| `CentralPanel::default().show(ctx, …)` | `CentralPanel::default().show(ui, …)`. `show_inside` is deprecated (it was renamed to `show`). |
| `egui::Id::new("x")` | Deprecated: use **`egui::Id::unique("x")`** for a global id, or `id_salt("x")` on widgets. |
| `egui::menu::bar(ui, …)` | **`egui::MenuBar::new().ui(ui, |ui| { ui.menu_button("File", |ui| { … ui.close(); }); })`** |
| `ui.close_menu()` | `ui.close()` |
| `egui_extras::DatePickerButton::new(&mut chrono::NaiveDate)` | **`DatePickerButton::new(&mut jiff::civil::Date)`** (needs the `datepicker` feature). |
| `egui_plot::Line::new(points)` | **`Line::new("name", PlotPoints::from(vec![[x, y], …]))`**; `BarChart::new("name", bars)` |

## App skeleton (verified)

```rust
fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1200.0, 800.0]).with_title("PF"),
        ..Default::default()
    };
    eframe::run_native("PF", options, Box::new(|_cc| Ok(Box::new(MyApp::default()))))
}

impl eframe::App for MyApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| { if ui.button("Backup").clicked() { ui.close(); } });
            });
        });
        egui::Panel::left("sidebar").resizable(true).show(ui, |ui| {
            if ui.selectable_label(self.screen == Screen::Dashboard, "Dashboard").clicked() { self.screen = Screen::Dashboard; }
        });
        egui::CentralPanel::default().show(ui, |ui| { /* screen */ });
    }
}
```

Panel order matters: show all `Panel`s **before** `CentralPanel`.

## Keyboard shortcuts

```rust
let undo = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Z);
let redo = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::Z);
// check redo FIRST (it is a superset of undo's modifiers)
if ui.ctx().input_mut(|i| i.consume_shortcut(&redo)) { … }
if ui.ctx().input_mut(|i| i.consume_shortcut(&undo)) { … }
```
`Modifiers::COMMAND` = ⌘ on macOS, Ctrl elsewhere. Skip global shortcuts while a text
field has focus: `ui.ctx().memory(|m| m.focused().is_some())` (that is the text widget's own undo).

## Widgets

```rust
ui.text_edit_singleline(&mut self.text);
let resp = ui.add(egui::TextEdit::singleline(&mut buf).desired_width(200.0).hint_text("Payee"));
if resp.lost_focus() { /* commit, R-TXN-12 (also fires on Enter) */ }

egui::ComboBox::from_id_salt(("cat", row_id)).selected_text(label).show_ui(ui, |ui| {
    ui.selectable_value(&mut selected, None, "None");
    for c in cats { ui.selectable_value(&mut selected, Some(c.id), c.full_name.as_str()); }
});

ui.add(egui_extras::DatePickerButton::new(&mut date).id_salt("start"));   // date: jiff::civil::Date
ui.checkbox(&mut flag, "External Account");
ui.add_enabled(!locked, egui::Button::new("Save"));
ui.add_enabled_ui(!locked, |ui| { /* everything inside disabled when locked */ });
ui.colored_label(egui::Color32::from_rgb(0x8c, 0x45, 0x12), "Reconciled");
resp.context_menu(|ui| { if ui.button("Duplicate").clicked() { …; ui.close(); } });
resp.on_hover_text("Attach file");
```

## Modal dialog (verified)

```rust
if self.show_modal {
    let r = egui::Modal::new(egui::Id::unique("add_tx")).show(ui.ctx(), |ui| {
        ui.heading("Add Transaction");
        if ui.button("Cancel").clicked() { self.show_modal = false; }
    });
    if r.should_close() { self.show_modal = false; }   // Esc or click outside
}
```

## Virtualised table (verified with 100,000 rows)

```rust
use egui_extras::{TableBuilder, Column};
TableBuilder::new(ui)
    .striped(true)
    .resizable(true)
    .sense(egui::Sense::click())                       // row click / selection
    .column(Column::initial(110.0))                    // Date
    .column(Column::remainder().at_least(150.0))       // Payee
    .column(Column::initial(100.0))                    // Amount
    .header(20.0, |mut h| { h.col(|ui| { ui.strong("Date"); }); h.col(|ui| { ui.strong("Payee"); }); h.col(|ui| { ui.strong("Amount"); }); })
    .body(|body| {
        body.rows(22.0, rows.len(), |mut row| {
            let r = &rows[row.index()];
            row.set_selected(selection.contains(&r.id));
            row.col(|ui| { ui.label(r.date.to_string()); });
            row.col(|ui| { ui.label(&r.payee); });
            row.col(|ui| { ui.label(fmt_money(r.amount)); });
            if row.response().clicked() { /* toggle selection; check ui.input(|i| i.modifiers.command) */ }
        });
    });
```
Only the visible rows' closures run, so per-row cost matters little but the `rows` Vec must be
precomputed (cache it keyed on the engine revision).

## Plots (egui_plot 0.38, verified)

```rust
use egui_plot::{Plot, Line, PlotPoints, BarChart, Bar, Legend};
Plot::new("networth").height(220.0).legend(Legend::default()).show(ui, |p| {
    p.line(Line::new("Checking", PlotPoints::from(vec![[0.0, 1.0], [1.0, 2.0]])));
    p.bar_chart(BarChart::new("P&L", vec![Bar::new(0.0, 3.0), Bar::new(1.0, -2.0).base_offset(1.0)]));
});
```
X axis as dates: use day numbers (`date.since(epoch)` in days) and `.x_axis_formatter(|mark, _| …)`.
Waterfall = bars with `base_offset`. Check the method names in `egui_plot-0.38.0/src/plot.rs` before use.

## Custom painting (donut, Sankey)

```rust
let (rect, resp) = ui.allocate_exact_size(egui::vec2(300.0, 300.0), egui::Sense::click());
let painter = ui.painter_at(rect);
painter.circle_filled(rect.center(), 40.0, egui::Color32::LIGHT_BLUE);
painter.add(egui::Shape::convex_polygon(points, fill, egui::Stroke::NONE));   // wedge segments
painter.text(pos, egui::Align2::CENTER_CENTER, "Food", egui::FontId::proportional(12.0), color);
if let Some(pos) = resp.hover_pos() { /* hit-test segments */ }
```
Donut wedges: build each wedge as a polygon of points along the outer then inner arc (it is
not convex, so use `Shape::Path` or split into convex quads, one per small angle step).

## UI tests (egui_kittest 0.37, verified)

```rust
use egui_kittest::{Harness, kittest::Queryable};   // Queryable brings get_by_label etc. into scope
let mut h = Harness::new_ui(|ui| app.ui_root(ui));
h.run();
h.get_by_label("Budget").click();
h.run();
assert!(h.query_by_label("Bills").is_some());
```
The closure borrows `app` mutably; drop the harness (`drop(h)`) before asserting on `app`.

**Text fields are not found by `hint_text`.** Give every text field an accessible label (verified):
```rust
// in the UI code:
let label = ui.label("Payee");
ui.add(egui::TextEdit::singleline(&mut text)).labelled_by(label.id);
// in the test:
h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Payee").focus();
h.run();
h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Payee").type_text("Coffee");
h.run();
```
Inside table rows where there is no visible label, use a hidden-label pattern: give the
widget `.on_hover_text("Payee row 3")`, or test through the engine instead of the widget.
**Unlabelled widgets in table rows** (verified by the T13 author): attach accessibility info directly:
```rust
let resp = ui.add(egui::TextEdit::singleline(&mut buf).frame(egui::Frame::NONE));   // borderless inline field
resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, ui.is_enabled(), format!("Payee row {id}")));
```
then `h.get_by_label("Payee row 7").click(); h.run(); …type_text("x")`. Modifier keys: `key_press_modifiers`.
A focused TextEdit consumes ⌘Z itself, so global undo must skip while a text field has focus.

For more (`key_press`, `query_all_by_label`), read `kittest-0.5.*/src/query.rs` in the registry.

## Repaint from another thread (the API server)

```rust
let ctx: egui::Context = cc.egui_ctx.clone();          // in the eframe creator closure
std::thread::spawn(move || { /* … */ ctx.request_repaint(); });
```

## Verified Cargo features

```toml
eframe = { version = "0.37", default-features = false, features = ["default_fonts", "wgpu", "accesskit"] }
egui_extras = { version = "0.37", default-features = false, features = ["datepicker"] }
egui_plot = "0.38"
egui_kittest = "0.37"   # dev-dependency
```
