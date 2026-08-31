/* 
 * HIKARU OPENSTUDIO - TRANSPORT BAR
 * Licencia: GNU AGPLv3
 */

use egui::{Ui, RichText, Button, Color32, Vec2};
use crate::{AppState, AppMode};

pub fn show(ui: &mut Ui, app_state: &mut AppState, _proxy: &crate::audio_proxy::AudioProxy) {
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let btn_size = Vec2::new(30.0, 26.0);

        if ui.add_sized(btn_size, Button::new("⏮")).clicked() {
            app_state.playhead_position = 0.0;
        }

        let play_bg = if app_state.is_playing { Color32::from_rgb(40, 180, 80) } else { Color32::from_gray(40) };
        if ui.add_sized(btn_size, Button::new(RichText::new("▶").color(Color32::WHITE)).fill(play_bg)).clicked() {
            app_state.is_playing = true;
        }

        if ui.add_sized(btn_size, Button::new("⏹")).clicked() {
            app_state.is_playing = false;
        }

        let rec_bg = if app_state.is_recording { Color32::from_rgb(220, 30, 30) } else { Color32::from_gray(40) };
        if ui.add_sized(btn_size, Button::new(RichText::new("⏺").color(Color32::RED)).fill(rec_bg)).clicked() {
            app_state.is_recording = !app_state.is_recording;
        }

        ui.separator();

        ui.selectable_value(&mut app_state.active_mode, AppMode::OpenLive, "OPENLIVE");
        ui.selectable_value(&mut app_state.active_mode, AppMode::OpenStudio, "OPENSTUDIO");
    });
}