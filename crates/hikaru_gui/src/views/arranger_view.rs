/* 
 * HIKARU OPENSTUDIO - ARRANGER VIEW
 * Licencia: GNU AGPLv3
 */

use egui::{Color32, Vec2, Ui, Slider, RichText};
use crate::audio_proxy::{AudioProxy, GuiCommand};
use crate::views::mixer;
use crate::views::matrix::SessionMatrixState;
use hikaru_transport::TransportPosition;

pub fn show(
    ui: &mut Ui,
    tracks: &mut [mixer::Track],
    matrix_state: &SessionMatrixState,
    _transport: &TransportPosition,
    audio_proxy: &AudioProxy,
) {
    let scene_header_width = 110.0;
    let track_column_width = 130.0;
    let track_header_height = 85.0; // Espacio vertical para Nombre + Pan + Volume
    let scene_row_height = 50.0;

    ui.horizontal(|ui| {
        ui.heading("🎛️ OpenLive - Arranger View");
        ui.label("(Presiona [Tab] para conmutar a Session Matrix)");
    });
    ui.separator();

    let mut live_tracks: Vec<(usize, &mut mixer::Track)> = tracks
        .iter_mut()
        .enumerate()
        .filter(|(_, t)| !t.is_master)
        .collect();

    let num_scenes = matrix_state.scenes.len().max(8);

    egui::ScrollArea::both().show(ui, |ui| {
        // 1. DIBUJAR CABECERAS SUPERIORES (AUDIO TRACKS + PAN + VOLUMEN)
        ui.horizontal(|ui| {
            // Esquina superior izquierda (Cruce entre Scene Headers y Track Headers)
            ui.allocate_ui(Vec2::new(scene_header_width, track_header_height), |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label(RichText::new("SCENES / TRACKS").size(10.0).color(Color32::GRAY));
                });
            });

            // Columnas de Pistas
            for (track_idx, track) in live_tracks.iter_mut() {
                let track_idx = *track_idx;
                ui.allocate_ui(Vec2::new(track_column_width, track_header_height), |ui| {
                    egui::Frame::group(ui.style())
                        .fill(Color32::from_rgb(32, 35, 42))
                        .inner_margin(4.0)
                        .show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.label(RichText::new(&track.name).strong().color(Color32::WHITE));
                                ui.add_space(2.0);

                                // Knob / Slider de Paneo (-1.0 a 1.0)
                                let prev_pan = track.pan;
                                ui.add(
                                    Slider::new(&mut track.pan, -1.0..=1.0)
                                        .text("Pan")
                                        .show_value(false)
                                );
                                if (track.pan - prev_pan).abs() > 0.001 {
                                    audio_proxy.send(GuiCommand::SetTrackPan {
                                        track_idx,
                                        pan: track.pan,
                                    });
                                }

                                // Fader / Slider de Volumen (0.0 a 1.2 / +1.5dB)
                                let prev_vol = track.volume;
                                ui.add(
                                    Slider::new(&mut track.volume, 0.0..=1.2)
                                        .text("Vol")
                                        .show_value(false)
                                );
                                if (track.volume - prev_vol).abs() > 0.001 {
                                    audio_proxy.send(GuiCommand::SetTrackVolume {
                                        track_idx,
                                        volume_db: track.volume,
                                    });
                                }
                            });
                        });
                });
            }
        });

        ui.add_space(4.0);

        // 2. DIBUJAR FILAS (SCENES A LA IZQUIERDA + GRILLA DE CLIPS EN EL CENTRO)
        for scene_idx in 0..num_scenes {
            ui.horizontal(|ui| {
                // Cabecera de la Escena (Fila lateral izquierda)
                let scene_name = matrix_state
                    .scenes
                    .get(scene_idx)
                    .map(|s| s.name.as_str())
                    .unwrap_or("");

                ui.allocate_ui(Vec2::new(scene_header_width, scene_row_height), |ui| {
                    egui::Frame::group(ui.style())
                        .fill(Color32::from_rgb(28, 30, 36))
                        .show(ui, |ui| {
                            ui.centered_and_justified(|ui| {
                                let label_text = if scene_name.is_empty() {
                                    format!("Scene {}", scene_idx + 1)
                                } else {
                                    scene_name.to_string()
                                };
                                ui.label(RichText::new(label_text).strong().color(Color32::LIGHT_GRAY));
                            });
                        });
                });

                // Celdas / Slots para cada Track en esta Scene
                for (track_idx, _track) in live_tracks.iter() {
                    let track_idx = *track_idx;

                    ui.allocate_ui(Vec2::new(track_column_width, scene_row_height), |ui| {
                        let (rect, _response) = ui.allocate_exact_size(
                            Vec2::new(track_column_width - 4.0, scene_row_height - 4.0),
                            egui::Sense::click(),
                        );

                        let painter = ui.painter();

                        // Renderizado del slot/clip correspondiente
                        let mut bg_color = Color32::from_rgb(22, 24, 28);
                        let mut clip_title = String::new();

                        if let Some(track_slots) = matrix_state.grid.get(track_idx) {
                            if let Some(slot) = track_slots.get(scene_idx) {
                                if let Some(ref clip) = slot.clip {
                                    bg_color = Color32::from_rgb(45, 75, 110);
                                    clip_title = clip
                                        .path
                                        .file_name()
                                        .map(|n| n.to_string_lossy().to_string())
                                        .unwrap_or_else(|| "Clip".to_string());
                                }
                            }
                        }

                        painter.rect_filled(rect, 3.0, bg_color);
                        painter.rect_stroke(rect, 3.0, (1.0, Color32::from_white_alpha(20)));

                        if !clip_title.is_empty() {
                            painter.text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                clip_title,
                                egui::FontId::proportional(11.0),
                                Color32::WHITE,
                            );
                        }
                    });
                }
            });
        }
    });
}