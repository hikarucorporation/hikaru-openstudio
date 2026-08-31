/* 
 * HIKARU OPENSTUDIO - ARRANGER VIEW
 * Licencia: GNU AGPLv3
 */

use egui::Ui;
use crate::AppState;
use crate::audio_proxy::AudioProxy;

pub fn show(ui: &mut Ui, _state: &mut AppState, _proxy: &AudioProxy) {
    ui.vertical_centered(|ui| {
        ui.add_space(50.0);
        ui.heading("🎹 Playlist / Arranger Timeline");
        ui.label("Área de secuenciación principal.");
        ui.add_space(20.0);
        
        let rect = ui.available_rect_before_wrap();
        let painter = ui.painter();
        
        // Dibujamos un fondo simple para el playlist
        painter.rect_filled(rect, 5.0, egui::Color32::from_black_alpha(100));
        painter.text(rect.center(), egui::Align2::CENTER_CENTER, "Arranger Canvas", egui::FontId::proportional(20.0), egui::Color32::GRAY);
    });
}
