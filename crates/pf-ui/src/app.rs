//! The application shell: state, the per-frame `ui_root`, shortcuts, menu bar, toolbar and backup
//! (T13). Every write goes through an engine command; the engine is locked once per frame.

use std::any::Any;
use std::path::Path;

use egui::{Align, Button, Key, KeyboardShortcut, Layout, Modifiers, Ui};
use pf_engine::commands::backup;
use pf_engine::{Engine, SharedEngine};
use pf_model::AccountId;

use crate::caches::ViewCaches;
use crate::dialogs::{self, Dialogs};
use crate::files::{FakeDialogs, FileDialogs};
use crate::screen::{Screen, ScreenStates};
use crate::sidebar;
use crate::toasts::{self, ToastKind, Toasts};
use crate::{scenario_menu, screens};

/// Seconds the Undo/Redo toast stays up.
const UNDO_TOAST_SECS: f64 = 2.0;
/// Seconds the backup toast stays up (R-MD-7).
const BACKUP_TOAST_SECS: f64 = 4.0;

/// Cmd-Z.
const UNDO: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Z);
/// Shift-Cmd-Z. Checked before Cmd-Z (R-UND-5).
const REDO: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::Z);
/// Cmd-S (R-MD-7).
const BACKUP: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
/// Cmd-N: New Transaction on the account screen (T16).
const NEW_TX: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::N);
/// Cmd-F: the account screen's search bar (T15c).
const SEARCH: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::F);
/// Cmd-\: show or hide the sidebar.
const TOGGLE_SIDEBAR: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Backslash);

/// Everything a screen or dialog may touch during one frame. Built fresh each frame by `ui_root`.
pub struct UiCtx<'a> {
    /// The locked engine; all writes go through `pf_engine::commands::*`.
    pub engine: &'a mut Engine,
    /// Set this to navigate.
    pub screen: &'a mut Screen,
    /// Open modals with `dialogs.open(...)`.
    pub dialogs: &'a mut Dialogs,
    /// Bottom toast.
    pub toasts: &'a mut Toasts,
    /// Shared lookups (accounts, categories, ...).
    pub caches: &'a mut ViewCaches,
    /// Native dialogs, or the fake in tests.
    pub files: &'a dyn FileDialogs,
}

/// The egui application.
pub struct PfApp {
    engine: SharedEngine,
    screen: Screen,
    states: ScreenStates,
    sidebar: sidebar::State,
    /// Whether the sidebar is shown (View > Hide Sidebar, Cmd-\\).
    sidebar_open: bool,
    dialogs: Dialogs,
    toasts: Toasts,
    caches: ViewCaches,
    files: Box<dyn FileDialogs>,
    /// Resources that live as long as the app, e.g. the API server handle.
    keep_alive: Vec<Box<dyn Any>>,
}

impl PfApp {
    /// Production constructor (apps/pf).
    pub fn new(engine: SharedEngine, files: Box<dyn FileDialogs>) -> Self {
        Self {
            engine,
            screen: Screen::Dashboard,
            states: ScreenStates::default(),
            sidebar: sidebar::State::default(),
            sidebar_open: true,
            dialogs: Dialogs::default(),
            toasts: Toasts::default(),
            caches: ViewCaches::default(),
            files,
            keep_alive: Vec::new(),
        }
    }

    /// Opens a dialog directly, for kittest (the editors of rows are normally opened by a double-click,
    /// which kittest cannot send).
    pub fn open_dialog_for_test(&mut self, d: crate::dialogs::Dialog) {
        self.dialogs.open(d);
    }

    /// Headless constructor for kittest: fake dialogs, no API server.
    pub fn new_for_test(engine: Engine) -> Self {
        Self::new(engine.shared(), Box::new(FakeDialogs::default()))
    }

    /// Like `new_for_test`, but with a fake the test keeps a clone of.
    pub fn new_for_test_with_files(engine: Engine, files: FakeDialogs) -> Self {
        Self::new(engine.shared(), Box::new(files))
    }

    /// Shared engine handle (tests lock it to assert on state).
    pub fn engine(&self) -> SharedEngine {
        self.engine.clone()
    }

    /// The screen currently shown.
    pub fn screen(&self) -> Screen {
        self.screen
    }

    /// Navigates to `s`.
    pub fn set_screen(&mut self, s: Screen) {
        self.screen = s;
    }

    /// Shows an info toast (used by apps/pf for "API server not started").
    pub fn toast(&mut self, msg: impl Into<String>) {
        self.toasts.info(msg);
    }

    /// The selected transaction ids on the account screen, in id order (kittest accessor).
    pub fn account_selection(&self) -> Vec<pf_model::TransactionId> {
        self.states.account.selection.iter().copied().collect()
    }

    /// Keeps `h` alive for as long as the app runs (the API server handle).
    pub fn keep_alive(&mut self, h: Box<dyn Any>) {
        self.keep_alive.push(h);
    }

    /// Draws the whole app. Called from `eframe::App::ui` and from kittest harnesses.
    pub fn ui_root(&mut self, ui: &mut Ui) {
        crate::fonts::apply_text_contrast(ui.ctx());
        // The engine is locked once per frame; a poisoned lock is recovered, not a panic.
        let engine_arc = self.engine.clone();
        let mut guard = engine_arc
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut cx = UiCtx {
            engine: &mut guard,
            screen: &mut self.screen,
            dialogs: &mut self.dialogs,
            toasts: &mut self.toasts,
            caches: &mut self.caches,
            files: self.files.as_ref(),
        };

        guard_navigation(&mut cx);
        handle_shortcuts(&mut cx, ui);
        // Cmd-F opens or closes the account search bar, but not while a dialog is open (T15c).
        if !cx.dialogs.is_open()
            && let Screen::Account(_) = *cx.screen
            && ui.ctx().input_mut(|i| i.consume_shortcut(&SEARCH))
        {
            screens::account::toggle_search(&mut self.states.account, cx.engine.today());
        }
        // Cmd-N opens New Transaction for the account on screen (T16).
        if !cx.dialogs.is_open()
            && let Screen::Account(id) = *cx.screen
            && ui.ctx().input_mut(|i| i.consume_shortcut(&NEW_TX))
        {
            let today = cx.engine.today();
            let scenario = cx.engine.state.active_scenario;
            cx.dialogs.open(dialogs::Dialog::AddTransaction(Box::new(
                crate::dialogs::add_transaction::TxForm::new(Some(id), scenario, today),
            )));
        }

        if !cx.dialogs.is_open() && ui.ctx().input_mut(|i| i.consume_shortcut(&TOGGLE_SIDEBAR)) {
            self.sidebar_open = !self.sidebar_open;
        }
        egui::Panel::top("menu").show(ui, |ui| menu_bar(&mut cx, ui, &mut self.sidebar_open));
        egui::Panel::top("toolbar").show(ui, |ui| toolbar(&mut cx, ui));
        egui::Panel::left("sidebar")
            .resizable(true)
            .default_size(180.0)
            .min_size(150.0)
            .show_collapsible(ui, &mut self.sidebar_open, |ui| {
                sidebar::show(&mut cx, &mut self.sidebar, ui)
            });
        egui::CentralPanel::default().show(ui, |ui| {
            let states = &mut self.states;
            match *cx.screen {
                Screen::Dashboard => screens::dashboard::show(&mut cx, &mut states.dashboard, ui),
                Screen::Account(id) => screens::account::show(&mut cx, &mut states.account, ui, id),
                Screen::Budget => screens::budget::show(&mut cx, &mut states.budget, ui),
                Screen::Obligations => {
                    screens::obligations::show(&mut cx, &mut states.obligations, ui)
                }
                Screen::Scenarios => screens::scenarios::show(&mut cx, &mut states.scenarios, ui),
                Screen::Contexts => screens::contexts::show(&mut cx, &mut states.contexts, ui),
                Screen::Categories => {
                    screens::categories::show(&mut cx, &mut states.categories, ui)
                }
                Screen::Reports => screens::reports::show(&mut cx, &mut states.reports, ui),
                Screen::Charts => screens::charts::show(&mut cx, &mut states.charts, ui),
                Screen::Settings => screens::settings::show(&mut cx, &mut states.settings, ui),
            }
        });
        dialogs::show_active(&mut cx, ui);
        // A transaction chosen in a drill-down window is shown in its account.
        if let Some(r) = cx.dialogs.reveal.take() {
            screens::account::reveal(&mut cx, &mut self.states.account, r.account, r.transaction);
        }
        toasts::draw(cx.toasts, ui);
    }
}

impl eframe::App for PfApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.ui_root(ui);
    }
}

/// Returns to the dashboard when the selected account is missing or deleted (R-ACC-1).
fn guard_navigation(cx: &mut UiCtx) {
    if let Screen::Account(id) = *cx.screen {
        let gone = cx.engine.db().account(id).is_none_or(|a| a.is_deleted);
        if gone {
            *cx.screen = Screen::Dashboard;
        }
    }
}

/// Keyboard shortcuts. Skipped entirely while a dialog is open. Undo and redo are skipped while a
/// text field has focus, because the text field handles its own undo (R-UND-5).
fn handle_shortcuts(cx: &mut UiCtx, ui: &mut Ui) {
    if cx.dialogs.is_open() {
        return;
    }
    let text_focused = ui.ctx().memory(|m| m.focused().is_some());
    if !text_focused {
        // Redo first: ⇧⌘Z must not be taken as ⌘Z.
        if ui.ctx().input_mut(|i| i.consume_shortcut(&REDO)) {
            redo(cx);
        } else if ui.ctx().input_mut(|i| i.consume_shortcut(&UNDO)) {
            undo(cx);
        }
    }
    if ui.ctx().input_mut(|i| i.consume_shortcut(&BACKUP)) {
        backup_now(cx);
    }
}

/// The top menu bar: File, Edit, View.
fn menu_bar(cx: &mut UiCtx, ui: &mut Ui, sidebar_open: &mut bool) {
    egui::MenuBar::new().ui(ui, |ui| {
        ui.menu_button("File", |ui| {
            if shortcut_item(ui, "Backup Data", "Cmd-S", true) {
                backup_now(cx);
                ui.close();
            }
        });
        ui.menu_button("Edit", |ui| {
            let undo_name = cx.engine.undo_name().map(str::to_owned);
            let undo_label = match &undo_name {
                Some(name) => format!("Undo {name}"),
                None => "Undo".to_string(),
            };
            if shortcut_item(ui, &undo_label, "Cmd-Z", undo_name.is_some()) {
                undo(cx);
                ui.close();
            }
            let redo_name = cx.engine.redo_name().map(str::to_owned);
            let redo_label = match &redo_name {
                Some(name) => format!("Redo {name}"),
                None => "Redo".to_string(),
            };
            if shortcut_item(ui, &redo_label, "Shift-Cmd-Z", redo_name.is_some()) {
                redo(cx);
                ui.close();
            }
        });
        ui.menu_button("View", |ui| {
            let label = if *sidebar_open {
                "Hide Sidebar"
            } else {
                "Show Sidebar"
            };
            if shortcut_item(ui, label, "Cmd-\\", true) {
                *sidebar_open = !*sidebar_open;
                ui.close();
            }
            ui.separator();
            view_item(cx, ui, "Dashboard", Screen::Dashboard);
            ui.separator();
            view_item(cx, ui, "Budget", Screen::Budget);
            view_item(cx, ui, "Obligations", Screen::Obligations);
            view_item(cx, ui, "Scenarios", Screen::Scenarios);
            ui.separator();
            view_item(cx, ui, "Charts", Screen::Charts);
            view_item(cx, ui, "Reports", Screen::Reports);
            view_item(cx, ui, "Categories", Screen::Categories);
            view_item(cx, ui, "Contexts", Screen::Contexts);
            view_item(cx, ui, "Settings", Screen::Settings);
        });
    });
}

/// A menu item with its shortcut drawn on the right. The accessible label is the plain item name
/// (the shortcut text would otherwise be read as part of it). Returns true when clicked.
fn shortcut_item(ui: &mut Ui, label: &str, shortcut: &str, enabled: bool) -> bool {
    let response = ui.add_enabled(enabled, Button::new(label).shortcut_text(shortcut));
    let plain = label.to_string();
    response
        .widget_info(move || egui::WidgetInfo::labeled(egui::Role::Button, enabled, plain.clone()));
    response.clicked()
}

/// One View menu item: navigates and closes the menu.
fn view_item(cx: &mut UiCtx, ui: &mut Ui, label: &str, target: Screen) {
    if ui.button(label).clicked() {
        *cx.screen = target;
        ui.close();
    }
}

/// The toolbar: the screen title on the left, the scenario menu on the right where allowed.
fn toolbar(cx: &mut UiCtx, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.heading(screen_title(cx));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if cx.screen.shows_scenario_menu() {
                scenario_menu::show(cx, ui);
            }
        });
    });
}

/// The toolbar title: the account name for account screens, else the screen's own title.
fn screen_title(cx: &UiCtx) -> String {
    match *cx.screen {
        Screen::Account(id) => account_name(cx, id),
        other => other.title().to_string(),
    }
}

/// The name of a live account, or "Account" when it is missing.
fn account_name(cx: &UiCtx, id: AccountId) -> String {
    cx.engine
        .db()
        .account(id)
        .map_or_else(|| "Account".to_string(), |a| a.name.clone())
}

/// Undoes the newest step and says so in a toast (R-UND-5).
fn undo(cx: &mut UiCtx) {
    match cx.engine.undo() {
        Ok(Some(name)) => {
            cx.toasts
                .show_for(format!("Undo {name}"), ToastKind::Info, UNDO_TOAST_SECS)
        }
        Ok(None) => {}
        Err(e) => cx.dialogs.alert("Undo Failed", e.to_string()),
    }
}

/// Redoes the newest undone step and says so in a toast (R-UND-5).
fn redo(cx: &mut UiCtx) {
    match cx.engine.redo() {
        Ok(Some(name)) => {
            cx.toasts
                .show_for(format!("Redo {name}"), ToastKind::Info, UNDO_TOAST_SECS)
        }
        Ok(None) => {}
        Err(e) => cx.dialogs.alert("Redo Failed", e.to_string()),
    }
}

/// Cmd-S / File → Backup Data (R-MD-7). Asks for a folder when none is set; a cancelled pick does
/// nothing. A folder that has gone missing is forgotten and reported.
pub(crate) fn backup_now(cx: &mut UiCtx) {
    let folder = match backup::backup_folder(cx.engine) {
        Some(path) if !path.as_os_str().is_empty() => path,
        _ => {
            let Some(picked) = cx.files.pick_folder("Set Backup Folder") else {
                return;
            };
            if let Err(e) = backup::set_backup_folder(cx.engine, &picked) {
                cx.dialogs.alert("Backup Error", e.to_string());
                return;
            }
            picked
        }
    };
    if !folder.is_dir() {
        // An empty path is the "no folder" setting, so the next backup asks again.
        if let Err(e) = backup::set_backup_folder(cx.engine, Path::new("")) {
            log::warn!("could not clear the backup folder: {e}");
        }
        cx.dialogs.alert(
            "Backup Error",
            "Backup folder access lost. Please choose the folder again.",
        );
        return;
    }
    match backup::perform_backup(cx.engine, &folder) {
        Ok(path) => cx.toasts.show_for(
            format!("Backup saved to: {}", path.display()),
            ToastKind::Success,
            BACKUP_TOAST_SECS,
        ),
        Err(e) => cx.dialogs.alert("Backup Error", e.to_string()),
    }
}
