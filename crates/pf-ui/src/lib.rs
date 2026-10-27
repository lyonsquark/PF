//! The egui user interface.

#[cfg(test)]
mod tests {
    #[test]
    fn scaffold_compiles() {}
}

/// Placeholder app; replaced in T13.
#[derive(Default)]
pub struct PfApp;

impl eframe::App for PfApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("PFRust");
            ui.label("Scaffold OK");
        });
    }
}
