/*
 * Hikaru OpenStudio - Global Menu Bar
 * License: AGPL-3.0-only
 */

use egui::Ui;
use crate::app::{HikaruApp, AppMode};

pub fn show(ui: &mut Ui, app: &mut HikaruApp) {
    egui::menu::bar(ui, |ui| {
        ui.style_mut().visuals.button_frame = false;

        // --- FILE ---
        ui.menu_button("FILE", |ui| {
            if ui.button("📄 New Project").clicked() {
                ui.close_menu();
            }
            if ui.button("📂 Open Project...").clicked() {
                ui.close_menu();
            }
            if ui.button("💾 Save").clicked() {
                ui.close_menu();
            }
            if ui.button("💾 Save As...").clicked() {
                ui.close_menu();
            }
            ui.separator();
            if ui.button("🎵 Export Audio (WAV/FLAC)...").clicked() {
                ui.close_menu();
            }
            ui.separator();
            if ui.button("❌ Exit").clicked() {
                std::process::exit(0);
            }
        });

        // --- EDIT ---
        ui.menu_button("EDIT", |ui| {
            if ui.button("↩ Undo").clicked() {
                ui.close_menu();
            }
            if ui.button("↪ Redo").clicked() {
                ui.close_menu();
            }
            ui.separator();
            if ui.button("✂ Cut").clicked() {
                ui.close_menu();
            }
            if ui.button("📋 Copy").clicked() {
                ui.close_menu();
            }
            if ui.button("📋 Paste").clicked() {
                ui.close_menu();
            }
        });

        // --- VIEW ---
        ui.menu_button("VIEW", |ui| {
            if ui.button("🎛 Mixer (F9)").clicked() {
                app.show_mixer = !app.show_mixer;
                ui.close_menu();
            }
            if ui.button("🎚 DSP Rack (F10)").clicked() {
                app.show_dsp_rack = !app.show_dsp_rack;
                ui.close_menu();
            }
            if ui.button("🎹 Piano Roll (F4)").clicked() {
                ui.close_menu();
            }
            ui.separator();
            if ui.button("🔄 Reset Layout").clicked() {
                ui.close_menu();
            }
        });

        // --- SETTINGS ---
        ui.menu_button("SETTINGS", |ui| {
            if ui.button("🔊 Audio Setup (JACK/ALSA/PipeWire)...").clicked() {
                ui.close_menu();
            }
            if ui.button("🎹 MIDI Devices...").clicked() {
                ui.close_menu();
            }
            if ui.button("🔌 VST3 / CLAP Plugin Paths...").clicked() {
                ui.close_menu();
            }
            ui.separator();
            if ui.button("🎨 Interface & Themes").clicked() {
                ui.close_menu();
            }
        });

        // --- HELP ---
        ui.menu_button("HELP", |ui| {
            if ui.button("📖 Manual / Docs").clicked() {
                ui.close_menu();
            }
            if ui.button("⌨ Keyboard Shortcuts").clicked() {
                ui.close_menu();
            }
            ui.separator();

            let about_label = match app.mode {
                AppMode::OpenLive => "ℹ About Hikaru OpenLive",
                AppMode::OpenStudio => "ℹ About Hikaru OpenStudio",
            };

            if ui.button(about_label).clicked() {
                app.show_about = true;
                ui.close_menu();
            }
        });
    });
}