//! Shared helpers for the kittest UI tests. Every engine here is on a temp SQLite file (never the
//! app's database) and every harness runs headless (no window, no API server, no file dialogs).

use egui::Vec2;
use egui_kittest::Harness;
use pf_engine::Engine;
use pf_engine::testing::World;
use pf_ui::PfApp;

// Not every test binary uses the fixture helper.
#[allow(dead_code)]
/// Engine loaded with `fixtures/world.md` (accounts, categories, contexts, scenario "Plan A").
pub fn engine_with_fixture() -> Engine {
    World::with_fixture("world").into_engine()
}

// Not every test binary uses the empty engine.
#[allow(dead_code)]
/// Empty engine on a temp file.
pub fn empty_engine() -> Engine {
    World::new().into_engine()
}

/// A 1200×800 headless harness drawing the whole app.
pub fn harness(app: &mut PfApp) -> Harness<'_> {
    Harness::builder()
        .with_size(Vec2::new(1200.0, 800.0))
        .build_ui(|ui| app.ui_root(ui))
}
