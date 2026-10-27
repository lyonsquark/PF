//! The egui user interface: app shell, screens, dialogs, and shared widgets (T13).
//! Writes go through `pf_engine::commands`; this crate never touches SQLite or `Db` fields mutably.

pub mod app;
pub mod caches;
pub mod dialogs;
pub mod files;
pub mod scenario_menu;
pub mod screen;
pub mod screens;
pub mod sidebar;
pub mod toasts;
pub mod widgets;

pub use app::PfApp;
