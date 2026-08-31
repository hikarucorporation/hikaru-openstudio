/* 
 * HIKARU OPENSTUDIO - HEADER BAR (Minimalist)
 * Licencia: GNU AGPLv3
 */

use egui::{Ui, Color32, RichText, Button};
use crate::{AppState, AppMode};
use crate::audio_proxy::{AudioProxy, GuiCommand};

pub fn show(ui: &mut Ui, state: &mut AppState, proxy: &AudioProxy) {
    let accent = Color32::from_rgb(0, 255, 255); // Cyan manual

    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.heading(RichText::new("🚀 HIKARU").strong().color(Color32::WHITE));
        ui.add_space(20.0);

        ui.group(|ui| {
            let studio_active = state.active_mode == AppMode::OpenStudio;
            let studio_btn = Button::new("OPENSTUDIO")
                .fill(if studio_active { Color32::from_rgb(60, 60, 80) } else { Color32::TRANSPARENT })
                .stroke(if studio_active { (1.0, accent) } else { (0.0, Color32::TRANSPARENT) });

            if ui.add(studio_btn).clicked() { state.active_mode = AppMode::OpenStudio; }

            let live_active = state.active_mode == AppMode::OpenLive;
            let live_btn = Button::new("OPENLIVE")
                .fill(if live_active { Color32::from_rgb(60, 60, 80) } else { Color32::TRANSPARENT })
                .stroke(if live_active { (1.0, accent) } else { (0.0, Color32::TRANSPARENT) });

            if ui.add(live_btn).clicked() { state.active_mode = AppMode::OpenLive; }
        });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(8.0);
            ui.label("BPM");
            if ui.add(egui::DragValue::new(&mut state.bpm).clamp_range(20.0..=300.0)).changed() {
                proxy.send(GuiCommand::SetBpm(state.bpm));
            }
            ui.separator();
            if ui.button(if state.is_playing { "⏸" } else { "▶" }).clicked() {
                state.is_playing = !state.is_playing;
            }
        });
    });
}
