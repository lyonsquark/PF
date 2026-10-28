//! PFRust desktop entry point (T13). Opens the engine, starts the local API on the same engine
//! (R-API-1), and runs the egui window.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use pf_engine::{Engine, SharedEngine};

/// The API port used when the database has no `apiPort` setting (Swift's fixed port).
const DEFAULT_API_PORT: u16 = 8989;

/// The app icon (1024 px PNG), decoded at start-up.
const APP_ICON_PNG: &[u8] = include_bytes!("../macos/AppIcon.png");

fn main() -> anyhow::Result<()> {
    env_logger::init();
    // $PF_DB overrides the location (handy for development); otherwise the app's default file.
    let path = std::env::var_os("PF_DB")
        .map(PathBuf::from)
        .unwrap_or_else(Engine::default_db_path);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let engine = Engine::open(&path)?;
    let ui_font = pf_engine::commands::settings::ui_font(&engine).map(str::to_owned);
    let port: u16 = engine
        .setting("apiPort")
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_API_PORT);
    let shared: SharedEngine = Arc::new(Mutex::new(engine));

    // The app icon: eframe sets the running app's Dock icon from this (macOS), and the window icon elsewhere.
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1200.0, 800.0])
        .with_min_inner_size([900.0, 600.0])
        .with_title("PF");
    match eframe::icon_data::from_png_bytes(APP_ICON_PNG) {
        Ok(icon) => viewport = viewport.with_icon(icon),
        Err(e) => log::warn!("app icon not loaded: {e}"),
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "PF",
        options,
        Box::new(move |cc| {
            pf_ui::fonts::install(&cc.egui_ctx, ui_font.as_deref());
            let ctx = cc.egui_ctx.clone();
            let repaint: Arc<dyn Fn() + Send + Sync> = Arc::new(move || ctx.request_repaint());
            let mut app =
                pf_ui::PfApp::new(Arc::clone(&shared), Box::new(pf_ui::files::RfdDialogs));
            match pf_api::start(Arc::clone(&shared), port, repaint) {
                Ok(handle) => app.keep_alive(Box::new(handle)),
                Err(e) => {
                    log::warn!("API server not started on port {port}: {e}");
                    app.toast(format!("API server not started (port {port} in use?)"));
                }
            }
            let app: Box<dyn eframe::App> = Box::new(app);
            Ok(app)
        }),
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))
}
