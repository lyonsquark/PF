//! L5 UI tests for the custom charts (T24b): all nine cards render with the fixture and with an empty
//! database. The geometry and the drill-down inputs are unit-tested in the chart modules (donut hit test,
//! waterfall bar offsets, Sankey layout); the clicks themselves need plot coordinates, which kittest
//! cannot aim, so they are checked by hand. Headless (egui_kittest).

mod common;

use egui_kittest::kittest::Queryable;
use pf_core::charts::{CHART_TITLES, NOT_ENOUGH_DATA};
use pf_engine::Engine;
use pf_model::Db;
use pf_model::testing::TestDb;
use pf_ui::PfApp;
use pf_ui::screen::Screen;

use common::{empty_engine, engine_with_fixture, harness};

/// Engine holding `db`, on a temp file (never the app's database).
fn engine_with(db: Db) -> Engine {
    let mut e = empty_engine();
    e.replace_db(db)
        .expect("write the test database to the temp file");
    e
}

/// True when the screen shows `text` (as a name or a value). More than one match is fine.
fn shows(h: &egui_kittest::Harness<'_>, text: &str) -> bool {
    h.query_all_by_label(text).count() > 0 || h.query_all_by_value(text).count() > 0
}

#[test]
/// UI-CHT-01: all nine cards render with the fixture, over several frames.
fn ui_cht_01_all_nine_render() {
    let mut app = PfApp::new_for_test(engine_with_fixture());
    app.set_screen(Screen::Charts);
    let mut h = harness(&mut app);
    for _ in 0..3 {
        h.run();
    }
    for title in CHART_TITLES {
        assert!(shows(&h, title), "card {title:?}");
    }
}

#[test]
/// With an empty database the custom cards show their empty state, and every card still renders.
fn ui_cht_01_all_nine_render_empty_db() {
    let mut app = PfApp::new_for_test(engine_with(TestDb::new().build()));
    app.set_screen(Screen::Charts);
    let mut h = harness(&mut app);
    for _ in 0..3 {
        h.run();
    }
    for title in CHART_TITLES {
        assert!(shows(&h, title), "card {title:?}");
    }
    assert!(shows(&h, NOT_ENOUGH_DATA), "the empty cards say so");
}
