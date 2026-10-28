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

#[test]
/// R-CHT-6: a chart's drill-down window stays inside a narrow window: its heading and its last column
/// are on screen, not pushed past the edge.
fn ui_chart_drill_window_fits_narrow_window() {
    use egui_kittest::kittest::Queryable;
    use pf_model::testing::TestDb;
    use pf_model::{AccountType, CategoryType, TransactionStatus};
    let mut t = TestDb::new();
    let chk = t.account("Checking", AccountType::Checking);
    let food = t.category("Food", CategoryType::Expense);
    for i in 0..3 {
        t.tx(chk, "2026-03-05", "-100", &format!("Market {i}"))
            .category(food)
            .status(TransactionStatus::Cleared)
            .done();
    }
    let scn = t.scenario("Plan");
    let mut e = pf_engine::Engine::open_in_memory().expect("engine");
    e.replace_db(t.build()).expect("db");
    e.set_active_scenario(Some(scn));
    let ids: Vec<pf_model::TransactionId> = e.db().live_transactions().map(|x| x.id).collect();
    let mut app = pf_ui::PfApp::new_for_test(e);
    app.set_screen(pf_ui::screen::Screen::Charts);
    app.open_dialog_for_test(pf_ui::dialogs::Dialog::ReportTransactions {
        title: "Waterfall Test".into(),
        ids,
    });
    let window = egui::Vec2::new(1000.0, 640.0);
    let mut h = egui_kittest::Harness::builder()
        .with_size(window)
        .build_ui(|ui| app.ui_root(ui));
    h.run();
    let heading = h
        .query_all_by_value("Waterfall Test")
        .next()
        .expect("drill heading")
        .rect();
    let last = h
        .query_all_by_value("Scenario")
        .next()
        .expect("Scenario column header")
        .rect();
    assert!(
        heading.left() >= 0.0,
        "heading starts inside the window: {heading:?}"
    );
    assert!(
        last.right() <= window.x,
        "last column ends inside the window: {last:?}"
    );
}

#[test]
/// The Charts screen has a "Minor grid lines" slider, so the minor grid can be lightened or hidden.
fn ui_cht_minor_grid_slider_is_present() {
    use egui_kittest::kittest::Queryable;
    let mut app = pf_ui::PfApp::new_for_test(pf_engine::Engine::open_in_memory().expect("engine"));
    app.set_screen(pf_ui::screen::Screen::Charts);
    let mut h = crate::common::harness(&mut app);
    h.run();
    assert!(
        h.query_all_by_label("Minor grid lines").next().is_some(),
        "the minor grid slider"
    );
}
