use egui::Ui;
use crate::AppState;
pub fn show(ui: &mut Ui, state: &AppState) {
    ui.horizontal(|ui| {
        ui.label(format!("Sample Rate: {} Hz", state.sample_rate));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label("Hikaru OpenStudio | AGPLv3");
        });
    });
}
