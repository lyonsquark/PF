# T13: UI shell (app struct, sidebar, navigation, scenario menu, menu bar, undo/redo, toasts, dialogs, files, main)

**Phase:** 3 · **Deps:** T10a, T10b, T10c (backup only) · **Crate(s):** `pf-ui`, `apps/pf` · **Size:** large (≈700 lines + tests)

## Goal

Replace the T00 placeholder with the real application frame. After this task, `cargo run -p pf`
opens a window with a working sidebar, navigation between (still mostly empty) screens, a scenario
menu, an Edit menu with undo/redo, toasts, modal alerts and confirmations, a file-dialog abstraction
that tests can fake, and the HTTP API started in the background. **Every later UI task (T14–T24)
builds on the names defined here, so implement them exactly as written.**

> If a command or engine function name differs from what is written here, use the actual one from
> `pf-engine` / `pf-api`; do not add new engine behaviour here.

## Read first

- `PLAN.md` → "UI (pf-ui)", "Engine (pf-engine)"; `CLAUDE.md`; `docs/egui-cheatsheet.md` (all of it)
- `docs/testing.md` → L5 (UI behaviour tests), UI-NAV-01, UI-SCN-01
- Rules: R-SCN-2, R-UND-5, R-ACC-1, R-ACC-4, R-MD-7 (`docs/business-rules.md`)
- Swift: `../PFApp/Sources/PFApp/Views/ContentView.swift` (`AppScreen`, `ContentView`, `SidebarView`),
  `Views/ScenariosMenu.swift`, `PFApp.swift` (menu command "Backup Data" ⌘S), `Services/BackupManager.swift`
- The engine's public API in `crates/pf-engine/src/lib.rs` (read it; do not guess names)

## Deliverables

### Module layout (create exactly these files)

```
crates/pf-ui/src/
├── lib.rs             pub mod declarations + `pub use app::PfApp;`
├── app.rs             PfApp, UiCtx, ui_root, shortcuts, menu bar, toolbar, screen dispatch
├── screen.rs          Screen enum + ScreenStates
├── sidebar.rs         sidebar panel
├── scenario_menu.rs   scenario menu widget
├── toasts.rs          Toasts (pure state + draw fn)
├── caches.rs          Cached<K, V>, ViewCaches, Lookups
├── files.rs           FileDialogs trait, RfdDialogs, FakeDialogs
├── dialogs/mod.rs     Dialog enum, Dialogs, ConfirmAction, show_active()
├── dialogs/alert.rs   Alert + Confirm modals
├── screens/mod.rs     one `pub mod` per screen
├── screens/dashboard.rs   placeholder `show` (T14 fills in)
├── screens/account.rs     placeholder (T15a)
├── screens/budget.rs      placeholder (T17)
├── screens/obligations.rs placeholder (T18)
├── screens/scenarios.rs   placeholder (T19)
├── screens/categories.rs  placeholder (T20)
├── screens/contexts.rs    placeholder (T20)
├── screens/reports.rs     placeholder (T23)
├── screens/charts.rs      placeholder (T24a)
├── screens/settings.rs    placeholder (T21)
└── widgets/mod.rs     shared widgets (start with `money_label`, `heading_row`)
```

A placeholder screen is:

```rust
//! Budget screen. Implemented in T17.
use crate::app::UiCtx;

/// Per-screen UI state (filled in by T17).
#[derive(Default)]
pub struct State {}

/// Draws the screen into the central panel.
pub fn show(_cx: &mut UiCtx, _st: &mut State, ui: &mut egui::Ui) {
    ui.heading("Budget");
}
```

The account screen's signature takes the id: `pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut egui::Ui, id: AccountId)`.

### `screen.rs`

```rust
use pf_model::AccountId;

/// Which screen the central panel shows. Mirrors Swift `AppScreen`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Dashboard,
    Account(AccountId),
    Budget,
    Obligations,
    Scenarios,
    Contexts,
    Categories,
    Reports,
    Charts,
    Settings,
}

impl Screen {
    /// Title shown in the toolbar ("Dashboard", "Budget", …; account screens show the account name, resolved by the caller).
    pub fn title(&self) -> &'static str;
    /// R-SCN UI rule: the scenario menu is hidden on these screens (Swift ContentView toolbar).
    pub fn shows_scenario_menu(&self) -> bool; // false for Budget, Categories, Contexts, Settings, Obligations, Charts
}

/// Per-screen UI state. Each screen module owns its `State`; T13 creates them all with Default.
#[derive(Default)]
pub struct ScreenStates {
    pub dashboard: crate::screens::dashboard::State,
    pub account: crate::screens::account::State,
    pub budget: crate::screens::budget::State,
    pub obligations: crate::screens::obligations::State,
    pub scenarios: crate::screens::scenarios::State,
    pub contexts: crate::screens::contexts::State,
    pub categories: crate::screens::categories::State,
    pub reports: crate::screens::reports::State,
    pub charts: crate::screens::charts::State,
    pub settings: crate::screens::settings::State,
}
```

### `app.rs`

```rust
use std::sync::{Arc, Mutex};
use pf_engine::{Engine, SharedEngine};

/// Everything a screen or dialog may touch during one frame. Built fresh each frame by `ui_root`.
pub struct UiCtx<'a> {
    pub engine: &'a mut Engine,          // the locked engine; all writes via pf_engine::commands::*
    pub screen: &'a mut Screen,          // set this to navigate
    pub dialogs: &'a mut Dialogs,        // open modals with dialogs.open(...)
    pub toasts: &'a mut Toasts,
    pub caches: &'a mut ViewCaches,      // shared lookups (accounts, categories, …)
    pub files: &'a dyn FileDialogs,
}

/// The egui application.
pub struct PfApp {
    engine: SharedEngine,
    screen: Screen,
    states: ScreenStates,
    sidebar: crate::sidebar::State,     // grouping choice (UI-only, not persisted)
    dialogs: Dialogs,
    toasts: Toasts,
    caches: ViewCaches,
    files: Box<dyn FileDialogs>,
}

impl PfApp {
    /// Production constructor (apps/pf).
    pub fn new(engine: SharedEngine, files: Box<dyn FileDialogs>) -> Self;
    /// Headless constructor for kittest: FakeDialogs, no API server.
    pub fn new_for_test(engine: Engine) -> Self;
    /// Like `new_for_test` but with a FakeDialogs the test keeps a clone of.
    pub fn new_for_test_with_files(engine: Engine, files: FakeDialogs) -> Self;
    /// Shared engine handle (tests lock it to assert on state).
    pub fn engine(&self) -> SharedEngine;
    pub fn screen(&self) -> Screen;
    pub fn set_screen(&mut self, s: Screen);
    /// Show a toast (used by apps/pf for "API server not started").
    pub fn toast(&mut self, msg: impl Into<String>);
    /// Draws the whole app. Called from `eframe::App::ui` and from kittest harnesses.
    pub fn ui_root(&mut self, ui: &mut egui::Ui);
}

impl eframe::App for PfApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) { self.ui_root(ui) }
}
```

`ui_root` order per frame:

1. Lock: `let engine_arc = self.engine.clone(); let mut guard = engine_arc.lock().unwrap_or_else(|p| p.into_inner());`
   (a poisoned lock is recovered, not a panic). Build `UiCtx` from `&mut *guard` and the other fields.
   rfd dialogs run on the UI thread while the lock is held; the API thread simply waits (acceptable for v1).
2. **Navigation guard**: if `screen == Screen::Account(id)` and the account is missing or deleted ⇒ `Screen::Dashboard`.
3. **Shortcuts** (`fn handle_shortcuts`), skipped entirely while a dialog is open:
   - ⇧⌘Z redo, checked **before** ⌘Z undo. Undo/redo are skipped while a text field has focus
     (`ui.ctx().memory(|m| m.focused().is_some())`), because the text field handles its own undo.
   - ⌘S backup (always, even with focus): see "Backup" below.
   - Later tasks add ⌘N (T16) and ⌘F (T15c) in this function.
4. `Panel::top("menu")` → menu bar (below).
5. `Panel::top("toolbar")`: left = screen title (`ui.heading`; account screens show the account name);
   right (`Layout::right_to_left(Align::Center)`) = scenario menu if `screen.shows_scenario_menu()`.
6. `Panel::left("sidebar").resizable(true).default_size(220.0)` → `sidebar::show(cx, ui)`.
7. `CentralPanel::default().show(ui, …)` → dispatch on `screen` to `screens::<x>::show(cx, &mut states.<x>, ui)`.
8. `dialogs::show_active(cx, ui)`: at most one modal at a time.
9. `toasts::draw(&mut toasts, ui)`.

After undo/redo, show the toast `Undo {name}` / `Redo {name}` (2 s).

### Menu bar

```
File                 Edit                         View
 Backup Data   ⌘S     Undo {name}   ⌘Z             Dashboard
                      Redo {name}   ⇧⌘Z            Budget · Obligations · Scenarios
                                                   Charts · Reports · Categories · Contexts · Settings
```

- Edit items: `"Undo {name}"` / `"Redo {name}"` from `engine.undo_name()` / `redo_name()`; when empty the
  label is just `"Undo"` / `"Redo"` and the button is disabled (`ui.add_enabled(false, Button::new("Undo"))`). (R-UND-5)
- Show shortcut text with `egui::Button::new(label).shortcut_text("⌘Z")` (exists in egui 0.37).
- Every item calls `ui.close()` after acting.

### Backup (⌘S and File → Backup Data) (R-MD-7)

```rust
fn backup_now(cx: &mut UiCtx) {
    // 1. folder = commands::backup::backup_folder(engine); if None: cx.files.pick_folder("Set Backup Folder");
    //    None ⇒ return silently; Some(p) ⇒ commands::backup::set_backup_folder(engine, &p) (not undoable)
    // 2. let now = jiff::Zoned::now();   // the UI may read the clock; pf-core may not
    //    match commands::backup::perform_backup(engine, &folder, &now) {
    //        Ok(path) => toast "Backup saved to: {path}" for 4 s,
    //        Err(e)   => Dialog::Alert { title: "Backup Error", message: e.to_string() } }
}
```
If the folder no longer exists: alert "Backup folder access lost. Please choose the folder again." and clear the setting.

### Sidebar (`sidebar.rs`). Port of Swift `SidebarView`

`pub fn show(cx: &mut UiCtx, st: &mut State, ui: &mut egui::Ui)`

```
┌──────────────────────────────┐
│ ▦ Dashboard                  │   selectable_label, text "Dashboard"
│ ACCOUNTS        [A-Z ▾]      │   small strong header + ComboBox grouping (A-Z | Institution | Type)
│   Checking                   │   selectable_label per account (selected when Screen::Account(id))
│   🌐 Brokerage               │   external accounts prefixed "🌐 " (if the glyph renders as a box, use "(ext) " suffix)
│ PLANNING                     │
│   Budget                     │
│   Obligations                │
│   Scenarios                  │
│   Charts                     │
│   Reports                    │
│ SETTINGS                     │
│   Categories                 │
│   Contexts                   │
│   Settings                   │
└──────────────────────────────┘
```

- Wrap everything in `egui::ScrollArea::vertical()`.
- Grouping (R-ACC-4): `#[derive(Default)] pub struct State { pub grouping: Grouping }` with
  `pub enum Grouping { #[default] Az, Institution, Type }` (labels "A-Z", "Institution", "Type"), held in the `PfApp.sidebar` field and
  passed as `sidebar::show(cx, &mut self.sidebar, ui)`. Not persisted (Swift keeps it in `@State`).
  - **A-Z**: flat list, sorted by name, case-insensitive.
  - **Institution**: `egui::CollapsingHeader::new(institution).default_open(true)` per institution, keys sorted;
    an empty institution goes under `"Other"`.
  - **Type**: one collapsing header per `AccountType::ALL` (in that order) with a capitalised label ("Checking", "Savings", …); empty types are skipped.
- Account row context menu (`resp.context_menu`): **"Delete"** → `Dialog::Confirm { title: "Delete Account?",
  message: "Are you sure? This will delete the account and all its transactions.", confirm_label: "Delete",
  action: ConfirmAction::DeleteAccount(id) }`. (T16 adds "Edit Account" above it and a "+" button in the header.)
- Labels are exact; kittest finds them with `get_by_label("Budget")`.

### Scenario menu (`scenario_menu.rs`). Port of `ScenariosMenu.swift`

```rust
pub fn show(cx: &mut UiCtx, ui: &mut egui::Ui);
```
- Button text: active scenario's name or `"Reality"`, followed by `" ⑂"`; orange
  (`Color32::from_rgb(255,149,0)`) when a scenario is active, normal text otherwise. Use `ui.menu_button(RichText, …)`.
- Items: `"✓ Reality"` or `"Reality"`; separator; each live scenario sorted by name, with a `"✓ "` prefix when active.
- Clicking "Reality" calls `cx.engine.set_active_scenario(None)`; clicking a scenario calls
  `cx.engine.toggle_scenario(Some(id))` (R-SCN-2; T09 API). The UI state change is **not** an undo step.

### Dialogs (`dialogs/mod.rs`)

```rust
/// One modal at a time. Later tasks ADD variants (and their match arms in `show_active`).
pub enum Dialog {
    Alert { title: String, message: String },
    Confirm { title: String, message: String, confirm_label: String, action: ConfirmAction },
    // T16: AddTransaction(Box<…>), AddAccount(…), EditAccount(…)
    // T17: BudgetItemEditor(…), Projection(…)
    // T18–T22 add their own.
}

/// What a confirmation does when accepted. Later tasks ADD variants.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfirmAction {
    DeleteAccount(AccountId),
}

#[derive(Default)]
pub struct Dialogs { active: Option<Dialog> }
impl Dialogs {
    pub fn open(&mut self, d: Dialog);       // replaces any open dialog
    pub fn close(&mut self);
    pub fn is_open(&self) -> bool;
    pub fn alert(&mut self, title: impl Into<String>, message: impl Into<String>);
}

/// Draws the active dialog (if any) as an egui::Modal and runs its outcome.
pub fn show_active(cx: &mut UiCtx, ui: &mut egui::Ui);
```

Implementation pattern (avoid borrowing `cx.dialogs` while passing `cx`): `let Some(mut d) = cx.dialogs.active.take() else { return };`
show it; if it is still open, put it back (`cx.dialogs.active = Some(d)`), unless the dialog code opened a different one.
Each dialog module exposes `pub fn show(cx: &mut UiCtx, ui: &mut egui::Ui, form: &mut Form) -> Outcome`, with
`pub enum Outcome { KeepOpen, Close }` defined in `dialogs/mod.rs`.

- Alert: `Modal::new(Id::unique(("alert", title)))`, heading = title, label = message, button **"OK"**.
- Confirm: heading, message, buttons **"Cancel"** and `confirm_label` (red text). On confirm run
  `run_confirm(cx, action)`. For `DeleteAccount(id)`: `commands::accounts::delete_account(engine, id)`
  (soft, undoable, R-ACC-1); if the current screen is that account, go to Dashboard; on error show an Alert.
- `Esc` / click outside (`resp.should_close()`) = Cancel.

### Toasts (`toasts.rs`)

```rust
#[derive(Default)]
pub struct Toasts { current: Option<Toast> }
pub struct Toast { pub text: String, pub kind: ToastKind, pub secs: f64, shown_at: Option<f64> }
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToastKind { Info, Success }

impl Toasts {
    pub fn info(&mut self, text: impl Into<String>);                  // 2.5 s
    pub fn success(&mut self, text: impl Into<String>);               // green, 2.5 s
    pub fn show_for(&mut self, text: impl Into<String>, kind: ToastKind, secs: f64);
    pub fn current_text(&self) -> Option<&str>;                       // for tests
    /// Pure expiry logic: sets shown_at on first call, clears when now - shown_at >= secs.
    pub fn tick(&mut self, now: f64);
}
/// Draw at the bottom centre with egui::Area (Order::Foreground); call ctx.request_repaint_after(...) until it expires.
pub fn draw(toasts: &mut Toasts, ui: &mut egui::Ui);
```
Time comes from `ui.input(|i| i.time)`. A new toast replaces the current one.

### Caches (`caches.rs`)

```rust
/// Memoises a derived value on (engine revision, key). Recomputes only when either changes.
pub struct Cached<K, V> { stamp: Option<(u64, K)>, value: Option<V> }
impl<K: PartialEq + Clone, V> Default for Cached<K, V> { … }
impl<K: PartialEq + Clone, V> Cached<K, V> {
    pub fn get(&mut self, revision: u64, key: &K, compute: impl FnOnce() -> V) -> &V;
    pub fn invalidate(&mut self);
}

/// Lists used by pickers across many screens.
pub struct Lookups {
    pub accounts: Vec<(AccountId, String)>,             // live, sorted by name (case-insensitive)
    pub categories: Vec<(CategoryId, String)>,          // = commands::categories::categories_for_picker(db) (T10b): live, sorted by full name ci (R-CAT-4); prefix "{icon} " when the icon is an emoji (non-ASCII)
    pub contexts: Vec<(ContextId, String, Option<String>)>, // live, not archived, sorted by name; (id, name, color_hex)
    pub scenarios: Vec<(ScenarioId, String)>,           // live, sorted by name
}

#[derive(Default)]
pub struct ViewCaches { lookups: Cached<(), Lookups> }
impl ViewCaches {
    pub fn lookups(&mut self, engine: &Engine) -> &Lookups;   // keyed by engine.revision()
}
```
Screen-specific derived data (dashboard balances, account rows, report tables) is cached in that
screen's own `State` with a `Cached<K, V>` field. **Never recompute big lists every frame.**

### Files (`files.rs`)

```rust
use std::path::{Path, PathBuf};
/// Native dialogs + "open in default app", behind a trait so tests can fake them.
pub trait FileDialogs {
    fn pick_file(&self, title: &str, extensions: &[&str]) -> Option<PathBuf>;
    fn pick_folder(&self, title: &str) -> Option<PathBuf>;
    fn save_file(&self, title: &str, default_name: &str, extensions: &[&str]) -> Option<PathBuf>;
    /// Opens a file with the OS default app (`open <path>` on macOS).
    fn open_path(&self, path: &Path) -> Result<(), String>;
}
/// rfd-backed implementation.
pub struct RfdDialogs;
/// Test double: answers are queued; calls are recorded. Clone shares state (Rc<RefCell<…>>).
#[derive(Clone, Default)]
pub struct FakeDialogs { inner: std::rc::Rc<std::cell::RefCell<FakeState>> }
#[derive(Default)]
pub struct FakeState {
    pub picks: std::collections::VecDeque<Option<PathBuf>>,   // answers for pick_file / pick_folder / save_file, in order
    pub opened: Vec<PathBuf>,
    pub calls: Vec<String>,                                    // e.g. "pick_folder:Set Backup Folder"
}
impl FakeDialogs {
    pub fn push_answer(&self, p: Option<PathBuf>);
    pub fn opened(&self) -> Vec<PathBuf>;
    pub fn calls(&self) -> Vec<String>;
}
```
`RfdDialogs::open_path` runs `std::process::Command::new("open").arg(path).status()`.
An empty queue in `FakeDialogs` returns `None`.

### Shared widgets (`widgets/mod.rs`), start with

```rust
/// Money text "$1,234.56" / "-$1,234.56" via pf_core's currency formatter; red when negative if `red_negative`.
pub fn money_label(ui: &mut egui::Ui, amount: Decimal, red_negative: bool) -> egui::Response;
/// Parse "#RRGGBB"/"#RRGGBBAA" (R-CTX-4, pf_core) into Color32, default blue rgb(0,122,255).
pub fn color_from_hex(hex: Option<&str>) -> egui::Color32;
/// A text field with an accessible label (kittest needs this; see the cheat sheet).
pub fn labeled_text(ui: &mut egui::Ui, label: &str, text: &mut String) -> egui::Response;
/// System-like colours used across screens (match the Swift app's SwiftUI colours).
pub mod colors {
    use egui::Color32;
    pub const GREEN: Color32 = Color32::from_rgb(52, 199, 89);
    pub const BLUE: Color32 = Color32::from_rgb(0, 122, 255);
    pub const RED: Color32 = Color32::from_rgb(255, 59, 48);
    pub const ORANGE: Color32 = Color32::from_rgb(255, 149, 0);
    pub const BROWN: Color32 = Color32::from_rgb(140, 69, 18);   // reconciled status (0.55, 0.27, 0.07)
}
```
Later tasks add `amount_field`, `category_picker`, `context_chips`, `status_picker`, `date_field` here.

### `apps/pf/src/main.rs`

```rust
fn main() -> anyhow::Result<()> {
    env_logger::init();
    // $PF_DB overrides the location (handy for development); else Engine::default_db_path() (T09)
    let path = std::env::var_os("PF_DB").map(std::path::PathBuf::from).unwrap_or_else(pf_engine::Engine::default_db_path);
    if let Some(dir) = path.parent() { std::fs::create_dir_all(dir)?; }
    let engine = pf_engine::Engine::open(&path)?;
    let port = engine.db().settings.get("apiPort").and_then(|v| v.parse().ok()).unwrap_or(8989u16);
    let shared: pf_engine::SharedEngine = std::sync::Arc::new(std::sync::Mutex::new(engine));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1200.0, 800.0]).with_min_inner_size([900.0, 600.0]).with_title("PF"),
        ..Default::default()
    };
    eframe::run_native("PF", options, Box::new(move |cc| {
        let ctx = cc.egui_ctx.clone();
        let repaint: std::sync::Arc<dyn Fn() + Send + Sync> = std::sync::Arc::new(move || ctx.request_repaint());
        let mut app = pf_ui::PfApp::new(shared.clone(), Box::new(pf_ui::files::RfdDialogs));
        match pf_api::start(shared.clone(), port, repaint) {     // R-API-1 (T12 signature: -> std::io::Result<ApiHandle>)
            Ok(handle) => app.keep_alive(Box::new(handle)),     // server lives as long as the app
            Err(e) => { log::warn!("API server not started: {e}"); app.toast(format!("API server not started (port {port} in use?)")); }
        }
        Ok(Box::new(app))
    })).map_err(|e| anyhow::anyhow!(e.to_string()))
}
```
Add `keep_alive: Vec<Box<dyn std::any::Any>>` to `PfApp` and `pub fn keep_alive(&mut self, h: Box<dyn std::any::Any>)`, which pushes onto it.

## Steps

1. **Read the engine API** (`pf-engine/src/lib.rs`, `commands/`) and check the actual names for: `Engine::toggle_scenario` /
   `set_active_scenario` / `undo` / `redo` / `undo_name` (T09), `commands::accounts::delete_account` (T10a),
   `commands::categories::categories_for_picker` (T10b), `commands::backup::{backup_folder, set_backup_folder, perform_backup}` (T10c).
   The backup part needs T10c. Tasks are taken in number order, so T10c is normally done already; if it is not, mark T13
   `blocked` ("waiting for T10c") rather than stubbing or ignoring tests.
2. Write `toasts.rs`, `caches.rs`, `files.rs` with their **unit tests first** (no egui needed).
3. Write `screen.rs`, the placeholder screens, `dialogs/`, `widgets/mod.rs`.
4. Write `sidebar.rs`, `scenario_menu.rs`.
5. Write `app.rs` (`ui_root`, menu, toolbar, shortcuts, backup).
6. Write `apps/pf/src/main.rs` (uses `Engine::default_db_path()` from T09 plus the `$PF_DB` override).
7. Write the kittest tests in `crates/pf-ui/tests/ui_shell.rs` with a shared helper module `tests/common/mod.rs`:
   ```rust
   pub fn engine_with_fixture() -> Engine   // uses pf_engine's World/test helper or imports fixtures/SampleData2.md into an in-memory engine
   pub fn harness(app: &mut PfApp) -> egui_kittest::Harness<'_>  // Harness::new_ui(|ui| app.ui_root(ui)) with size 1200×800 (see Harness builder)
   ```
8. `cargo run -p pf` manually: click every sidebar entry, switch scenarios, press ⌘Z after deleting an account from the sidebar.

## Tests (required)

Unit tests (in `src/`):

| Test name | Covers | What it checks |
|---|---|---|
| `toasts_expire_after_duration` | – | `tick(0.0)` shows, `tick(2.4)` still shown, `tick(2.6)` cleared |
| `toasts_new_replaces_old` | – | second `info` replaces first |
| `cached_recomputes_only_on_revision_or_key_change` | – | compute closure call count: same (rev,key) ⇒ 1 call; new rev ⇒ 2; new key ⇒ 3 |
| `fake_dialogs_returns_queued_answers_in_order` | – | push 2 answers; picks return them, then `None`; `calls()` recorded |
| `screen_scenario_menu_visibility` | R-SCN-2 (UI) | `shows_scenario_menu` false exactly for Budget, Categories, Contexts, Settings, Obligations, Charts |
| `lookups_sorted_and_live_only` | R-CAT-4 | deleted rows absent; categories sorted by full name case-insensitive |
| `color_from_hex_parses_and_defaults` | R-CTX-4 | `#FF0000` red, `#FF000080` alpha, `"zz"` ⇒ default blue |

kittest (`crates/pf-ui/tests/ui_shell.rs`):

| Test name | Covers | What it checks |
|---|---|---|
| `ui_nav_01_every_sidebar_entry_opens_its_screen` | UI-NAV-01 | click each of Dashboard, Budget, Obligations, Scenarios, Charts, Reports, Categories, Contexts, Settings ⇒ `app.screen()` matches |
| `ui_nav_01_account_entry_opens_account_screen` | UI-NAV-01 | click an account name ⇒ `Screen::Account(id)` |
| `ui_scn_01_scenario_menu_switches_and_toggles` | UI-SCN-01 | open menu "Reality", click a scenario ⇒ engine active scenario set; open menu (now labelled with the scenario name), click the same scenario ⇒ back to Reality |
| `ui_scn_01_scenario_menu_hidden_on_budget` | UI-SCN-01 | on Budget, `query_by_label("Reality")` is None; on Dashboard it is Some |
| `ui_shell_edit_menu_undo_label_and_action` | R-UND-5 | engine empty history ⇒ "Undo" disabled; after a command (create account via engine) ⇒ menu shows "Undo {name}"; clicking it removes the account |
| `ui_shell_delete_account_from_sidebar_confirms_and_undoes` | R-ACC-1 | context-menu Delete → Confirm "Delete Account?" → "Delete" ⇒ account gone from sidebar; Edit → Undo ⇒ back. (If kittest cannot open a context menu, call `dialogs.open(Confirm…)` via a test hook and assert the rest.) |
| `ui_shell_cmd_z_undoes_and_shift_cmd_z_redoes` | R-UND-5 | create an account via the engine; `h.key_press_modifiers(Modifiers::COMMAND, Key::Z)` ⇒ gone; `key_press_modifiers(COMMAND \| SHIFT, Key::Z)` ⇒ back (verified kittest API) |
| `ui_shell_cmd_z_ignored_while_dialog_open` | – | open an Alert; ⌘Z ⇒ nothing undone |
| `ui_shell_deleted_selected_account_returns_to_dashboard` | – | select an account, delete it through the engine, run a frame ⇒ `Screen::Dashboard` |
| `ui_shell_sidebar_grouping_institution_uses_other` | R-ACC-4 | account with empty institution appears under "Other" header when grouping = Institution |
| `ui_shell_backup_prompts_for_folder_then_writes` | R-MD-7 | FakeDialogs answer = temp dir; click File → Backup Data ⇒ a `PFBackup_*.md` file exists in it; toast text starts with "Backup saved to:" |
| `ui_shell_backup_cancelled_folder_does_nothing` | R-MD-7 | queued answer `None` ⇒ no setting stored, no file, no alert |

**Accessible labels required:** every sidebar entry (visible text), the grouping ComboBox (label "Grouping" via `labelled_by` or
`ComboBox::from_label`), scenario menu button (its visible text), menu titles "File"/"Edit"/"View" and items, dialog buttons.

## Pitfalls

- egui 0.37: `Panel::left/top` (not SidePanel/TopBottomPanel) shown with `.show(ui, …)` **before** `CentralPanel`. `Id::unique`, not `Id::new`.
- Do not call `engine.lock()` twice in one frame (deadlock). Everything uses the one `UiCtx`.
- Borrow-checker trick for dialogs: `take()` the active dialog out of `Dialogs`, show it, put it back.
- Redo shortcut must be checked before undo (⇧⌘Z also matches ⌘Z if checked first with `consume_shortcut`… it does not
  in egui since modifiers must match exactly, but check redo first anyway: it is cheap and safe).
- Do not run undo/redo while a dialog is open: the form would point at rows that no longer exist.
- Do not persist UI-only state (grouping, selected screen) through engine commands; that would create undo steps.
- `rfd` dialogs block the UI thread; that is fine (macOS modal), but never call them from the API thread.
- Kittest: `drop(harness)` before using `app` again; `harness.run()` after every click.
- A focused `TextEdit` consumes ⌘Z itself (its own text undo). This was verified: in a test, pressing ⌘Z while a text field is
  focused reverts the typed text. So the shell must not also run engine undo then, and tests of engine undo must not leave a field focused.
- `TextEdit::frame` takes an `egui::Frame` (`Frame::NONE`), while `Button::frame` takes a `bool`.

## Done when

```bash
cargo test -p pf-ui
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask rules
cargo run -p pf        # manual: navigate everything, scenario menu, undo via Edit menu and ⌘Z, ⌘S backup
```

## Out of scope

- Screen contents (T14–T24). Placeholders only show a heading.
- Add/Edit Account dialogs and the sidebar "+" button (T16). Add Transaction and ⌘N (T16). Search and ⌘F (T15c).
- Native macOS menu bar, app icon/bundle (T25).
