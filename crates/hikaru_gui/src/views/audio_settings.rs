/*
 * Hikaru OpenStudio - Audio Setup Window
 * License: AGPL-3.0-only
 */

use egui::{Window, ComboBox, Grid, RichText, Color32};

#[derive(Clone, Debug, PartialEq)]
pub enum AudioBackend {
    PipeWire,
    Jack,
    Alsa,
    PulseAudio,
}

pub struct AudioSettingsState {
    pub is_open: bool,
    pub selected_backend: AudioBackend,
    pub selected_device: String,
    pub sample_rate: u32,
    pub buffer_size: u32,
    pub available_devices: Vec<String>,
}

impl Default for AudioSettingsState {
    fn default() -> Self {
        Self {
            is_open: false,
            selected_backend: AudioBackend::PipeWire,
            selected_device: "Default Output Device".to_string(),
            sample_rate: 44100,
            buffer_size: 512,
            available_devices: vec![
                "Default Output Device".to_string(),
                "ALSA: PulseAudio / PipeWire Sound Server".to_string(),
                "JACK Audio Connection Kit".to_string(),
            ],
        }
    }
}

pub fn show(ctx: &egui::Context, state: &mut AudioSettingsState) {
    if !state.is_open {
        return;
    }

    let mut open = true;

    Window::new("Audio Setup (JACK / ALSA / PipeWire)")
        .open(&mut open)
        .resizable(false)
        .collapsible(false)
        .default_width(420.0)
        .show(ctx, |ui| {
            ui.add_space(4.0);
            ui.label(RichText::new("CONFIGURACIÓN DE AUDIO & HARDWARE").strong().color(Color32::from_rgb(0, 255, 255)));
            ui.separator();
            ui.add_space(6.0);

            // 1. BACKEND SELECTION
            ui.horizontal(|ui| {
                ui.label("Driver / Subsistema:");
                ComboBox::from_id_source("audio_backend_combo")
                    .selected_text(format!("{:?}", state.selected_backend))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut state.selected_backend, AudioBackend::PipeWire, "PipeWire (Recomendado)");
                        ui.selectable_value(&mut state.selected_backend, AudioBackend::Jack, "JACK (Baja Latencia)");
                        ui.selectable_value(&mut state.selected_backend, AudioBackend::Alsa, "ALSA (Nativo Linux)");
                        ui.selectable_value(&mut state.selected_backend, AudioBackend::PulseAudio, "PulseAudio");
                    });
            });

            ui.add_space(4.0);

            // 2. DEVICE SELECTION
            ui.horizontal(|ui| {
                ui.label("Dispositivo de Salida:");
                ComboBox::from_id_source("audio_device_combo")
                    .selected_text(&state.selected_device)
                    .show_ui(ui, |ui| {
                        for dev in &state.available_devices {
                            ui.selectable_value(&mut state.selected_device, dev.clone(), dev);
                        }
                    });
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);

            // 3. SAMPLE RATE & BUFFER SIZE
            Grid::new("audio_params_grid")
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.label("Sample Rate:");
                    ComboBox::from_id_source("sample_rate_combo")
                        .selected_text(format!("{} Hz", state.sample_rate))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut state.sample_rate, 44100, "44100 Hz (CD)");
                            ui.selectable_value(&mut state.sample_rate, 48000, "48000 Hz (Pro/Estándar)");
                            ui.selectable_value(&mut state.sample_rate, 88200, "88200 Hz");
                            ui.selectable_value(&mut state.sample_rate, 96000, "96000 Hz (Hi-Res)");
                        });
                    ui.end_row();

                    ui.label("Buffer Size (Latencia):");
                    ComboBox::from_id_source("buffer_size_combo")
                        .selected_text(format!("{} samples", state.buffer_size))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut state.buffer_size, 128, "128 samples (~2.9 ms)");
                            ui.selectable_value(&mut state.buffer_size, 256, "256 samples (~5.8 ms)");
                            ui.selectable_value(&mut state.buffer_size, 512, "512 samples (~11.6 ms)");
                            ui.selectable_value(&mut state.buffer_size, 1024, "1024 samples (~23.2 ms)");
                        });
                    ui.end_row();
                });

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(6.0);

            // 4. ACTION BUTTONS
            ui.horizontal(|ui| {
                if ui.button("Reiniciar Driver Audio").clicked() {
                    // Re-inicialización de cpal
                }
                
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Cerrar").clicked() {
                        state.is_open = false;
                    }
                });
            });
        });

    if !open {
        state.is_open = false;
    }
}