/* 
 * HIKARU OPENSTUDIO - OPENLIVE MATRIX VIEW
 * Licencia: GNU AGPLv3
 */

use egui::Ui;
use crate::AppState;
use crate::audio_proxy::AudioProxy;

pub fn show(ui: &mut Ui, _state: &mut AppState, _proxy: &AudioProxy) {
    ui.vertical_centered(|ui| {
        ui.add_space(50.0);
        ui.heading("🎛️ OPENLIVE MATRIX");
        ui.label("Modo de Performance en Vivo activado.");
        ui.add_space(20.0);
        ui.label("(Aquí se renderizará el Grid de Clips)");
    });
}
