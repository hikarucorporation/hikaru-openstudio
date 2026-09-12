use egui::*;
use crate::views::mixer;
use hikaru_transport::TransportPosition;
use crate::audio_proxy::AudioProxy;

pub fn show(
    ui: &mut Ui,
    tracks: &mut [mixer::Track],
    _transport: &TransportPosition,
    _audio_proxy: &AudioProxy,
) {
    if tracks.is_empty() {
        return;
    }

    let track_width = 110.0;
    let clip_slot_height = 45.0;
    let scenes_count = 8; // Cantidad de slots verticales por pista

    // Separamos la pista 0 (MASTER) del resto de las pistas de audio
    let (master_track, audio_tracks) = tracks.split_first_mut().unwrap();

    ScrollArea::both()
        .id_source("session_matrix_scroll")
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // =========================================================
                // 1. COLUMNA MASTER (Izquierda)
                // =========================================================
                ui.vertical(|ui| {
                    Frame::group(ui.style()).show(ui, |ui| {
                        ui.set_width(track_width);
                        ui.label(RichText::new("MASTER").strong().color(Color32::LIGHT_BLUE));
                        
                        // Faders y tiras de canal verticales abajo
                        ui.add_space(8.0);
                        ui.add(Slider::new(&mut master_track.volume, 0.0..=1.0)
                            .vertical()
                            .text("Vol"));
                        
                        ui.horizontal(|ui| {
                            ui.toggle_value(&mut master_track.mute, "M");
                            ui.toggle_value(&mut master_track.solo, "S");
                        });
                    });
                });

                ui.separator();

                // =========================================================
                // 2. MATRIZ DE PISTAS Y CLIPS (Columnas Verticales)
                // =========================================================
                for (i, track) in audio_tracks.iter_mut().enumerate() {
                    ui.vertical(|ui| {
                        ui.set_width(track_width);

                        // Header superior de la pista
                        Frame::group(ui.style()).show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.label(RichText::new(format!("TRK {:02}", i + 1)).strong());
                                ui.label(RichText::new(format!("Track {}", i + 1)).size(10.0));
                            });
                        });

                        ui.add_space(4.0);

                        // Slots de Clips para esta pista (Session Grid)
                        for scene_idx in 0..scenes_count {
                            let (slot_rect, response) = ui.allocate_exact_size(
                                Vec2::new(track_width, clip_slot_height),
                                Sense::click(),
                            );

                            if ui.is_rect_visible(slot_rect) {
                                let painter = ui.painter();
                                let fill_color = if response.hovered() {
                                    Color32::from_rgb(35, 35, 35)
                                } else {
                                    Color32::from_rgb(25, 25, 25)
                                };

                                painter.rect_filled(slot_rect, 3.0, fill_color);
                                painter.rect_stroke(
                                    slot_rect,
                                    3.0,
                                    Stroke::new(1.0_f32, Color32::from_gray(45)),
                                );

                                // Texto indicativo
                                painter.text(
                                    slot_rect.center(),
                                    Align2::CENTER_CENTER,
                                    format!("[ Scene {}-{} ]", i + 1, scene_idx + 1),
                                    FontId::proportional(10.0),
                                    Color32::DARK_GRAY,
                                );
                            }

                            ui.add_space(2.0);
                        }

                        ui.separator();

                        // Controles de Mixer de Pista abajo (Paneo, Fader, M/S)
                        ui.vertical_centered(|ui| {
                            ui.add(Slider::new(&mut track.pan, -1.0..=1.0).show_value(false));
                            ui.add(Slider::new(&mut track.volume, 0.0..=1.0).vertical());
                            ui.horizontal(|ui| {
                                ui.toggle_value(&mut track.mute, "M");
                                ui.toggle_value(&mut track.solo, "S");
                            });
                        });
                    });

                    ui.add_space(4.0);
                }
            });
        });
}