//! PFRust desktop entry point. Wiring of engine and API server happens in T13.

fn main() -> eframe::Result {
    env_logger::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_title("PF"),
        ..Default::default()
    };
    eframe::run_native("PF", options, Box::new(|_cc| Ok(Box::new(pf_ui::PfApp))))
}
