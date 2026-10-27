# T19: Scenarios screen

**Phase:** 3 · **Deps:** T13 · **Crate(s):** `pf-ui` · **Size:** small (≈200 lines + 120 lines tests)

## Goal

Port the Scenarios screen: a list of scenarios with a colour dot. Clicking a row toggles it as the active
scenario. There are New and Edit dialogs, and Delete (which also deactivates the scenario and hides its
transactions). It also adds the shared colour-picker widget that T20 reuses.

> **Shell contract (T13).** The screen is `screens/scenarios.rs` with `#[derive(Default)] pub struct State` and
> `pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut egui::Ui)` (state in `ScreenStates.scenarios`). The dialog is a new
> `Dialog::ScenarioEdit(Box<ScenarioForm>)` variant with an arm in `dialogs::show_active`, and its module exposes
> `show(cx, ui, form) -> Outcome`. The toolbar scenario menu is T13's `scenario_menu.rs`. This screen uses the same engine calls,
> so both stay in sync: `cx.engine.toggle_scenario(Some(id))` and `cx.engine.state.active_scenario` (T09 `AppState`).
> Commands (T10b, `pf_engine::commands::scenarios`): `create_scenario(e, ScenarioInput { name, color_hex, notes })`,
> `update_scenario(e, id, ScenarioInput)`, `delete_scenario(e, id)`. Lists: `cx.caches.lookups(cx.engine).scenarios`.
> Fallback: if a name here differs from the code, use the actual name in `pf-ui`/`pf-engine`; do not add engine behaviour here.

## Read first

- Docs: `docs/egui-cheatsheet.md`, `docs/testing.md` (L5)
- Swift: `../PFApp/Sources/PFApp/Views/ScenariosView.swift` (whole file, including `EditScenarioSheet`),
  `../PFApp/Sources/PFApp/Views/ScenariosMenu.swift`, `ContextsView.swift` (`Color(hex:)` and `toHex()` at the end)
- Rules: R-SCN-2, R-SCN-3, R-SCN-4, R-CTX-4

## Deliverables

```rust
// crates/pf-ui/src/widgets/color.rs   (shared; T20 uses it). `pub mod color;` in widgets/mod.rs.
// T13 already provides `widgets::color_from_hex(Option<&str>) -> Color32` (default blue); use it where blue is the default.
/// Parse "#RRGGBB" / "#RRGGBBAA" (R-CTX-4) using the pf-core helper from T04; None if invalid (for non-blue defaults).
pub fn parse_hex(hex: &str) -> Option<egui::Color32>;
/// Lower-case "#rrggbb" (matches Swift `toHex()`, which uses "%06x").
pub fn to_hex(c: [u8; 3]) -> String;
/// A labelled colour button: `ui.label(label)` + `ui.color_edit_button_srgb(rgb)`. Returns true when changed.
pub fn color_field(ui: &mut egui::Ui, label: &str, rgb: &mut [u8; 3]) -> bool;
/// A filled 12×12 circle.
pub fn color_dot(ui: &mut egui::Ui, color: egui::Color32);

// crates/pf-ui/src/screens/scenarios.rs
#[derive(Default)] pub struct State {}
pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut egui::Ui);

// crates/pf-ui/src/dialogs/scenario_edit.rs
pub struct ScenarioForm { pub editing: Option<ScenarioId>, pub name: String, pub notes: String, pub rgb: [u8; 3] }
pub fn show(cx: &mut UiCtx, ui: &mut egui::Ui, form: &mut ScenarioForm) -> Outcome;
```

`ui.color_edit_button_srgb(&mut [u8; 3])` exists in egui 0.37 (verified in `egui-0.37.0/src/ui.rs`).

## Screen layout

```
┌ Scenarios ──────────────────────────────────── [ + Add Scenario ] ┐
│ Scenarios                                                          │
│  ● Plan A            (orange text when active)               ✓    │
│    Cutting back dining out   (small, grey)                         │
│  ● Vacation Prep                                                   │
└────────────────────────────────────────────────────────────────────┘
```

* Section header "Scenarios". Live scenarios only. **Order:** Swift uses query order (unsorted); we sort by name.
* Row: a colour dot (scenario colour; purple `#800080` if None or invalid); the name (strong; **orange** when this is
  the active scenario); notes (small, weak) if non-empty; a "✓" in orange at the right when active.
* **Click** a row ⇒ `cx.engine.toggle_scenario(Some(id))` (R-SCN-2): active ⇒ Reality, otherwise ⇒ this one. Not an undo step.
* **Right-click** ⇒ context menu: "Edit Scenario", a separator, "Delete".
* Delete ⇒ `delete_scenario(cx.engine, id)` (R-SCN-3: deactivates if active, soft-deletes the scenario and its
  transactions, one undo step "Delete Scenario"). No confirmation dialog (matches Swift). Errors ⇒ `cx.dialogs.alert(...)`.

## New / Edit dialog

```
┌ New Scenario ─────────────────────┐      ┌ Edit Scenario ───────────┐
│ Name   [___________________]      │      │ (same fields, prefilled) │
│ Color  [■]                        │      │                          │
│ Notes  [___________________]      │      │        [Cancel] [Save]   │
│                 [Cancel] [Create] │      └──────────────────────────┘
└───────────────────────────────────┘
```

* New: defaults name "", notes "", colour purple (`#800080` ⇒ `[0x80, 0x00, 0x80]`). **Create** is disabled while
  the trimmed name is empty (R-SCN-4). Create calls `create_scenario(cx.engine, ScenarioInput { name, color_hex: Some(to_hex(rgb)), notes: Some(notes) })`;
  the engine trims the name and turns empty notes into None (R-SCN-4).
* Edit: prefilled from the scenario (an invalid or missing colour shows purple). **Save** is disabled while the trimmed
  name is empty. Save calls `update_scenario(cx.engine, id, ScenarioInput { … })`. One undo step.
* Cancel / Esc closes without changes.

## Steps

1. Write the tests below.
2. Implement `widgets/color.rs` (unit-test `to_hex` and `parse_hex` round trips here).
3. Implement the screen, then the dialog.
4. Run Done-when.

## Tests (required)

`crates/pf-ui/tests/ui_scenarios.rs` (using T13's `tests/common::{engine_with_fixture, harness}` and `PfApp::new_for_test`)
plus unit tests in `widgets/color.rs`.

| Test name | Covers | What it checks |
|---|---|---|
| `ui_scn_click_row_toggles_active` | R-SCN-2 | Click "Plan A" ⇒ the engine's active scenario is Plan A; click again ⇒ Reality |
| `ui_scn_active_row_shows_checkmark` | — | When active, the "✓" label is present |
| `ui_scn_create_via_dialog` | R-SCN-4 | "+ Add Scenario", type a name, Create ⇒ the scenario exists with colour `#800080` |
| `ui_scn_create_disabled_when_name_empty` | — | Create does nothing with an empty name |
| `ui_scn_edit_renames` | — | Context menu → Edit Scenario → change the name → Save ⇒ renamed; one undo step |
| `ui_scn_delete_active_deactivates_and_hides_tx` | R-SCN-3 | Delete the active scenario ⇒ active = Reality, the scenario's transactions are hidden; Cmd-Z restores both |
| `color_to_hex_lowercase` | R-CTX-4 | `to_hex([255,0,170]) == "#ff00aa"` |
| `color_parse_hex_variants` | R-CTX-4 | `#RRGGBB`, `RRGGBB`, `#RRGGBBAA` parse; `#123`, `zzzzzz`, `""` ⇒ None |

## Pitfalls

- Toggling must use the **engine's** scenario state (`cx.engine.state.active_scenario`) so the dashboard, menu and charts all see it.
  Do not keep a separate copy in the screen.
- `color_edit_button_srgb` opens a popup. In kittest, test the colour through the engine rather than by clicking the popup.
- Swift compares the active scenario by name in `toggleScenario` (a bug); R-SCN-2 says compare by id. `toggle_scenario` already does.

## Done when

```bash
cargo test -p pf-ui --test ui_scenarios
cargo test -p pf-ui --lib color_
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules
```

## Out of scope

- The toolbar scenario menu (T13). Generating scenario transactions (T17).
