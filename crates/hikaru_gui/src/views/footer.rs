use egui::{Ui, RichText, Layout, Align};

pub fn show(ui: &mut Ui, cpu_usage: f32) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("HIKARU OPENSTUDIO | AGPLv3").small());
        ui.separator();
        ui.label(RichText::new(format!("CPU: {:.1}%", cpu_usage * 100.0)).small());
        
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new("ENGINE: IDLE").small());
        });
    });
}
