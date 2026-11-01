# Rust PF: Port Plan (PFApp Swift → Rust)

Rust PF is a ground-up Rust rewrite of the PFApp personal finance app
(`../PFApp`, Swift 6 / SwiftUI / SwiftData, about 10.5k lines). Goals:

1. **Fast**: instant startup, no UI stalls with 100k+ transactions (see the perf budgets in `docs/testing.md`).
2. **Feature parity** with the Swift app, verified by tests (`docs/business-rules.md`).
3. **Safe migration**: the Swift app's data is never touched; data moves via the Markdown backup.
4. **Buildable by small models**: work is split into small, self-contained tasks in `tasks/`.

## How to use this plan (read this first if you are an agent)

1. Read `CLAUDE.md` (rules for agents) in this folder.
2. Open `tasks/STATUS.md`, pick the **lowest-numbered task whose dependencies are all `done`**
   and whose status is `todo`. Set it to `in-progress` with your session note.
3. Read the task file completely, then read every doc and Swift file it lists under "Read first".
4. Follow the task's steps. Write tests first. Run the task's "Done when" commands.
5. Set the status to `done` and add a 1–3 line note (what you did, anything surprising).
   If blocked, set `blocked` with the reason and stop. Do not improvise across task boundaries.

## Decisions (fixed: do not re-litigate)

| Topic | Decision | Why |
|---|---|---|
| GUI | **egui / eframe 0.37** (wgpu backend), `egui_extras` 0.37 (TableBuilder, DatePickerButton), `egui_plot` 0.38 | Same stack as DeckCraft (`storytold/deckcraft` uses eframe 0.37 + wgpu) and NoteCraft. Immediate mode suits data-heavy tables; virtualised rows handle 100k transactions. |
| Dates | **`jiff` 0.2** (`jiff::civil::Date` for calendar days, `jiff::Timestamp` for instants) | `egui_extras::DatePickerButton` 0.37 takes `&mut jiff::civil::Date`; one date library everywhere. **Do not add chrono.** |
| Money | **`rust_decimal` 1.x** (`Decimal`), stored as TEXT | Exact, like Swift `Decimal`. |
| Storage | **SQLite via `rusqlite` 0.41 (`bundled`)**, one file `~/Library/Application Support/PF/PF.sqlite` | Small, fast, inspectable. Never opens the Swift `PF.store`. |
| Data access | **Whole DB in memory** (`Db` of `BTreeMap`s), loaded at startup; SQLite is write-through persistence | Personal finance data is small (100k rows ≈ tens of MB). In-memory reads are what make the UI fast; this is the main speed win over SwiftData `@Query`. |
| Mutations & undo | Every mutation goes through `Engine::apply(name, closure)`, which records before/after row images ⇒ generic undo/redo, atomic persistence | Mirrors DeckCraft's "every action is a command with undo". No per-command inverse code to get wrong. |
| Migration | Import the Swift app's **Markdown backup** (`docs/markdown-format.md`) | Already exists, already tested in Swift, human-readable. No reverse-engineering of Core Data. |
| HTTP API | `tiny_http` 0.12 on `127.0.0.1:8989`, same routes and bodies as Swift | External scripts keep working. |
| CLI | `pf-cli` binary on the same engine | Headless testing and scripting; DeckCraft also ships a CLI. |
| Charts | `egui_plot` for line/bar charts; custom `egui::Painter` for donut, waterfall and Sankey | No WebView, no JavaScript (the Swift app embeds ECharts in a WKWebView). |
| File dialogs | `rfd` 0.17 behind a `FileDialogs` trait (fake implementation in tests) | Testable. |
| Platform | macOS first (Apple Silicon). Keep code portable; no `unsafe` (`unsafe_code = "forbid"`). | Same as DeckCraft. |
| Rust | edition 2024, toolchain 1.99 (installed via Homebrew). | |

Versions were checked on 2026-10-26, and a probe project with this exact dependency set built and passed
its egui_kittest test. See `docs/egui-cheatsheet.md` for the verified API.

## Architecture

```
PF/
├── Cargo.toml                 workspace (versions pinned in [workspace.dependencies])
├── CLAUDE.md                  rules for agents
├── PLAN.md                    this file
├── docs/                      data-model, business-rules, markdown-format, testing, egui-cheatsheet
├── tasks/                     one file per task + STATUS.md
├── fixtures/                  copied sample data, goldens, parity captures
├── scripts/                   build_app.sh, capture_swift_parity.sh
├── xtask/                     `cargo xtask rules|ci|bundle`
├── crates/
│   ├── pf-model/   entities, enums, Db + queries, TestDb builder        (deps: rust_decimal, jiff, serde)
│   ├── pf-core/    pure logic: balances, filters, projector, categorize,
│   │               obligations, markdown, csv import, reports, chart data, formatting
│   │                                                                     (deps: pf-model, regex)
│   ├── pf-store/   SQLite load/save/purge                                (deps: pf-model, rusqlite)
│   ├── pf-engine/  Engine (apply/undo/redo), AppState, commands, World test helper
│   │                                                                     (deps: pf-core, pf-store)
│   ├── pf-api/     HTTP server                                           (deps: pf-engine, tiny_http, serde_json)
│   └── pf-ui/      egui app: shell, screens, dialogs, charts             (deps: pf-engine, eframe, egui_extras, egui_plot, rfd)
└── apps/
    ├── pf/         desktop binary "PF" (opens engine, starts API, runs UI)
    └── pf-cli/     command-line tool
```

**Dependency rule:** arrows only point down this list. `pf-core` never does IO, never reads
the clock or time zone (they are parameters). `pf-ui` never touches SQLite or `Db` fields
mutably, and every write is an engine command.

### Engine (pf-engine): the core contract

```rust
pub enum Record {                       // one row of any table
    Account(Account), Transaction(Transaction), Category(Category), Context(Context),
    Scenario(Scenario), Obligation(Obligation), BudgetItem(BudgetItem),
    ImportMapping(ImportMapping), Setting { key: String, value: String },
}
pub enum RowKey { Account(AccountId), Transaction(TransactionId), /* … */ Setting(String) }
pub struct Change { pub key: RowKey, pub before: Option<Record>, pub after: Option<Record> }
pub struct Step   { pub name: String, pub changes: Vec<Change> }

pub struct Engine {
    db: Db, store: Store, undo: Vec<Step>, redo: Vec<Step>,
    pub state: AppState,               // active scenario, time period: UI state, not undoable
    revision: u64,                     // bumps on every apply/undo/redo; UI caches key on it
    today: Box<dyn Fn() -> Date + Send>, // injectable clock (tests fix it)
}

impl Engine {
    pub fn open(path: &Path) -> Result<Engine>;          // load + purge (R-PRG-1)
    pub fn open_in_memory() -> Result<Engine>;
    pub fn db(&self) -> &Db;
    pub fn revision(&self) -> u64;
    pub fn today(&self) -> Date;
    /// Run `f` against a write handle. On Ok: diff → persist in ONE SQL transaction →
    /// push Step (if any change) → clear redo → bump revision. On Err: restore before-images.
    pub fn apply<T>(&mut self, name: &str, f: impl FnOnce(&mut Writer) -> Result<T>) -> Result<T>;
    pub fn undo(&mut self) -> Result<Option<String>>;    // returns the step name
    pub fn redo(&mut self) -> Result<Option<String>>;
    pub fn undo_name(&self) -> Option<&str>;
    pub fn redo_name(&self) -> Option<&str>;
    pub fn clear_history(&mut self);
}
// Writer: insert_*(…) -> Id, update_*(id, |row| …), set_deleted(key, bool), delete_hard(key),
// set_setting(k, v). Writer records the before-image the first time a row is touched.

pub type SharedEngine = std::sync::Arc<std::sync::Mutex<Engine>>;
```

* **Commands** (`pf-engine::commands::{accounts, transactions, scenarios, obligations,
  categories, contexts, budget, import, backup}`) are plain functions,
  `pub fn delete_account(e: &mut Engine, id: AccountId) -> Result<()>`. They enforce the
  business rules (locks, validation) and call `e.apply(...)` exactly once (one undo step).
* **Errors**: `pub enum EngineError { Locked, NotFound(String), Validation(String), Store(String) }`,
  implementing `Display` with the user-facing message from the business rules.
* **Threading**: the desktop app wraps the engine in `SharedEngine`. The UI locks it once
  per frame; the API thread locks it per request and then calls a repaint callback.
  Never hold the lock across a file dialog or a sleep.

### UI (pf-ui)

```rust
pub struct PfApp {
    engine: SharedEngine,
    screen: Screen,                   // Dashboard | Account(AccountId) | Budget | Obligations | Scenarios
                                      // | Contexts | Categories | Reports | Charts | Settings
    dialogs: Dialogs,                 // which modal is open + its form state
    toasts: Toasts,                   // bottom toast with expiry
    caches: ViewCaches,               // derived data keyed by (engine.revision(), params)
    files: Box<dyn FileDialogs>,      // rfd in prod, fake in tests
}
impl eframe::App for PfApp { fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) { self.ui_root(ui) } }
impl PfApp { pub fn new_for_test(engine: Engine) -> Self; pub fn ui_root(&mut self, ui: &mut egui::Ui); }
```

* One module per screen in `crates/pf-ui/src/screens/`, one per dialog in `src/dialogs/`,
  shared widgets (amount field, category picker, context chips, colour swatch) in `src/widgets/`.
* **Never compute derived data from scratch every frame** for big lists: put it in `ViewCaches`
  keyed by `engine.revision()` plus the parameters (account, period, search, scenario).
* Tables use `egui_extras::TableBuilder` with `body.rows(height, n, …)` (virtualised).
* Menus: an in-window `egui::MenuBar` (File, Edit, View). egui has no native macOS menu bar;
  that is an accepted limitation. Shortcuts: ⌘Z undo, ⇧⌘Z redo, ⌘S backup, ⌘N add transaction (on account screens), ⌘F search.
* SF Symbols are not available. Category icons that are emoji are shown; SF Symbol names are
  kept in data but rendered as nothing (or a generic glyph).

## Phases and tasks

Each task is sized for one focused session of a small model (about 150–600 lines of code plus tests).
"Deps" must be `done` before starting. Task files: `tasks/Txx-*.md`.

### Phase 0: Foundation
| Task | Title | Deps |
|---|---|---|
| T00 | Workspace scaffold (exact files given, builds an empty window) | – |
| T00b | `xtask`: `rules` traceability check, `ci` | T00 |

### Phase 1: Model and pure logic (no UI; heavily unit-tested)
| Task | Title | Deps |
|---|---|---|
| T01 | pf-model: enums, entities, `Db`, queries, `TestDb` builder | T00 |
| T02 | pf-core: budget projector (R-PRJ-1…5) + property tests | T01 |
| T03 | pf-core: scenario visibility, balances, running balance, filters, obligations (R-SCN, R-BAL, R-FLT, R-OBL) | T01 |
| T04 | pf-core: categorize, pattern validation, colours, currency formatting, context-name generator (R-CAT, R-CTX-4, R-OBL-4) | T01 |
| T05 | pf-core: Markdown import/export + fixtures + goldens (R-MD) | T01 |
| T06 | pf-core: CSV parse, detect, mapping lookup, unmapped collection, import planning (R-CSV) | T01, T04 |
| T07 | pf-store: SQLite schema, load, apply changes, purge | T01 |
| T08 | pf-core: reports (P&L, balance sheet, monthly matrix) (R-RPT) | T02, T03, T05 |
| T08b | pf-core: chart data for all 9 charts (R-CHT) | T02, T03, T05, T08 |

### Phase 2: Engine, CLI, API
| Task | Title | Deps |
|---|---|---|
| T09 | pf-engine: `Engine`, `Writer`, undo/redo, persistence, `World` test helper (R-UND) | T03, T07 |
| T10a | Commands: accounts, transactions, starting balance, status rules (BEH-ACC, BEH-TXN) | T09, T04 |
| T10b | Commands: scenarios, obligations, contexts, categories (BEH-SCN, BEH-OBL, BEH-CTX, BEH-CAT) | T09, T04 |
| T10c | Commands: budget items, projection, reproject, CSV import, Markdown import/backup (BEH-BUD, BEH-CSV, BEH-MD-03, BEH-BAK, BEH-UND, BEH-PRG) | T09, T02, T05, T06 |
| T11 | apps/pf-cli | T10c |
| T12 | pf-api: HTTP server, byte-compatible with Swift (R-API) | T10c |
| T12b | Perf suite (100k transactions) and fixes until the budgets pass | T10c |

### Phase 3: UI
| Task | Title | Deps |
|---|---|---|
| T13 | UI shell: app struct, sidebar, navigation, scenario menu, menu bar, undo/redo, toasts, `FileDialogs`, apps/pf main | T10a, T10b, T10c |
| T14 | Dashboard | T13 |
| T15a | Account detail: header balances, time period, virtualised read-only transaction table + running balance | T13 |
| T15b | Transaction table: inline editing, lock rules, status menu, auto-categorize | T15a |
| T15c | Transaction table: search bar, context menu (duplicate, delete, add to budget), attachments, multi-select | T15b |
| T16 | Dialogs: Add Transaction, Add/Edit Account (+ starting balance) | T13 |
| T17 | Budget screen, budget item editor, projection dialog | T13, T10c |
| T18 | Obligations screen + detail | T13 |
| T19 | Scenarios screen | T13 |
| T20 | Categories screen, Contexts screen | T13 |
| T21 | Settings: backup folder, export/import Markdown, Monarch CSV entry, wipe, orphaned transactions | T13, T10c |
| T22 | CSV import wizard: column mapping and translation mapping | T15a, T10c |
| T23 | Reports screen (P&L + drill-down, balance sheet, monthly matrix) | T13, T08 |
| T24a | Charts dashboard + line/bar charts (egui_plot) | T13, T08b |
| T24b | Donut, waterfall and Sankey charts (custom painter) | T24a |

### Phase 4: Ship
| Task | Title | Deps |
|---|---|---|
| T25 | `scripts/build_app.sh`: PF.app bundle, icon, install | T13 |
| T26 | Swift parity capture script + parity tests un-ignored | T12 |
| T27 | Final parity checklist walkthrough (`docs/parity-checklist.md`), README for the user | all |

Parallelism: after T01, tasks T02–T07 are independent. After T13, the screen tasks T14–T24 are
independent except where listed.

## Migration procedure (for the user, documented in T27's README)

1. In the Swift app: Settings → Export (or `curl -s localhost:8989/api/export/markdown > ~/pf-export.md`).
2. Quit the Swift app (both apps use port 8989).
3. `pf-cli import-md ~/pf-export.md`, or in Rust PF: Settings → Import.
4. Compare: `pf-cli balances --format text` vs the Swift app's `curl localhost:8989/api/accounts/balances?format=text`.

## Out of scope (v1)

* iOS. The Swift package targets iOS 17 but the app is macOS-centric. egui can do iOS later.
* Native macOS menu bar, SF Symbols, Finder "Open With".
* Reading the SwiftData `PF.store` directly.
* Multi-window.
