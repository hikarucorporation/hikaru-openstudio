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

    // --- Dimensiones compactas para los pads ---
    let track_width = 100.0;
    let clip_slot_height = 25.0;
    let scenes_count = 8;

    let (master_track, audio_tracks) = tracks.split_first_mut().unwrap();

    ScrollArea::both()
        .id_source("session_matrix_scroll")
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // 1. MATRIZ DE PISTAS Y CLIPS (Audio Tracks primero)
                for (i, track) in audio_tracks.iter_mut().enumerate() {
                    ui.vertical(|ui| {
                        ui.set_width(track_width);

                        // Header compacto
                        Frame::group(ui.style()).show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.label(RichText::new(format!("TRK {:02}", i + 1)).strong());
                                ui.label(RichText::new(format!("Audio {}", i + 1)).size(9.0));
                            });
                        });

                        ui.add_space(2.0);

                        // Grid de slots
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

                                painter.rect_filled(slot_rect, 2.0, fill_color);
                                painter.rect_stroke(
                                    slot_rect,
                                    2.0,
                                    Stroke::new(1.0_f32, Color32::from_gray(45)),
                                );

                                painter.text(
                                    slot_rect.center(),
                                    Align2::CENTER_CENTER,
                                    format!("[ Scene {} | Audio {} ]", scene_idx + 1, i + 1),
                                    FontId::proportional(9.0),
                                    Color32::DARK_GRAY,
                                );
                            }

                            ui.add_space(1.0);
                        }

                        ui.add_space(62.0);
                        ui.separator();

                        // Controles inferiores
                        ui.vertical_centered(|ui| {
                            custom_pan_slider(ui, &mut track.pan);
                            ui.add_space(4.0);
                            custom_volume_fader(ui, &mut track.volume);
                            ui.add_space(4.0);

                            ui.horizontal(|ui| {
                                ui.toggle_value(&mut track.mute, "M");
                                ui.toggle_value(&mut track.solo, "S");
                            });
                        });
                    });

                    ui.add_space(2.0);
                }

                ui.separator();

                // 2. COLUMNA MASTER (Scene Master Launchers)
                ui.vertical(|ui| {
                    ui.set_width(track_width);
                    
                    Frame::group(ui.style()).show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(RichText::new("MASTER").strong().color(Color32::LIGHT_BLUE));
                            ui.label(RichText::new("Main Out").size(9.0));
                        });
                    });

                    ui.add_space(2.0);

                    // Botones Master Scene Launchers (Misma estructura exacta que las audio tracks)
                    for scene_idx in 0..scenes_count {
                        let (slot_rect, response) = ui.allocate_exact_size(
                            Vec2::new(track_width, clip_slot_height),
                            Sense::click(),
                        );

                        if ui.is_rect_visible(slot_rect) {
                            let painter = ui.painter();
                            let fill_color = if response.hovered() {
                                Color32::from_rgb(50, 50, 60)
                            } else {
                                Color32::from_rgb(30, 30, 40)
                            };

                            painter.rect_filled(slot_rect, 2.0, fill_color);
                            painter.rect_stroke(
                                slot_rect,
                                2.0,
                                Stroke::new(1.0_f32, Color32::from_rgb(0, 150, 190)),
                            );

                            painter.text(
                                slot_rect.center(),
                                Align2::CENTER_CENTER,
                                format!("▶ Scene {}", scene_idx + 1),
                                FontId::proportional(10.0),
                                Color32::WHITE,
                            );
                        }

                        ui.add_space(1.0);
                    }

                    ui.add_space(62.0);
                    ui.separator();

                    // Controles inferiores Master
                    ui.vertical_centered(|ui| {
                        custom_pan_slider(ui, &mut master_track.pan);
                        ui.add_space(4.0);
                        custom_volume_fader(ui, &mut master_track.volume);
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.toggle_value(&mut master_track.mute, "M");
                            ui.toggle_value(&mut master_track.solo, "S");
                        });
                    });
                });
            });
        });
}

// --- Helper: Custom Pan Slider ---
fn custom_pan_slider(ui: &mut Ui, pan_val: &mut f32) -> Response {
    let desired_size = vec2(90.0, 20.0);
    let (rect, response) = ui.allocate_at_least(desired_size, Sense::drag());

    let thumb_width = 12.0;
    let half_thumb = thumb_width / 2.0;

    let min_x = rect.min.x + half_thumb;
    let max_x = rect.max.x - half_thumb;

    if response.dragged() {
        if let Some(pointer_pos) = response.interact_pointer_pos() {
            let normalized = ((pointer_pos.x - min_x) / (max_x - min_x)).clamp(0.0, 1.0);
            *pan_val = (normalized * 200.0) - 100.0;
        }
    }

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let center_x = rect.center().x;
        let track_y = rect.center().y;

        // 1. Riel base (Gris oscuro)
        let rail_rect = Rect::from_min_max(
            pos2(min_x - 2.0, track_y - 3.0),
            pos2(max_x + 2.0, track_y + 3.0),
        );
        painter.rect_filled(rail_rect, 1.0, Color32::from_rgb(45, 45, 45));

        // 2. Riel lleno (Cyan)
        let current_norm = (*pan_val + 100.0) / 200.0;
        let thumb_x = min_x + (current_norm * (max_x - min_x));

        if (thumb_x - center_x).abs() > 0.5 {
            let fill_min_x = center_x.min(thumb_x);
            let fill_max_x = center_x.max(thumb_x);
            let filled_rect = Rect::from_min_max(
                pos2(fill_min_x, track_y - 3.0),
                pos2(fill_max_x, track_y + 3.0),
            );
            painter.rect_filled(filled_rect, 0.0, Color32::from_rgb(0, 150, 190));
        }

        // Marca central del riel
        painter.line_segment(
            [pos2(center_x, track_y - 5.0), pos2(center_x, track_y + 5.0)],
            Stroke::new(1.0, Color32::GRAY),
        );

        // 3. Capuchón celeste y cuadrado (Thumb)
        let thumb_rect = Rect::from_center_size(
            pos2(thumb_x, track_y),
            vec2(thumb_width, 18.0),
        );
        let thumb_color = if response.hovered() || response.dragged() {
            Color32::from_rgb(100, 200, 255)
        } else {
            Color32::from_rgb(0, 162, 232)
        };

        painter.rect_filled(thumb_rect, 0.0, thumb_color);
        painter.rect_stroke(thumb_rect, 0.0, Stroke::new(1.0, Color32::BLACK));
    }

    response
}

// --- Helper: Custom Volume Vertical Fader ---
fn custom_volume_fader(ui: &mut Ui, volume_val: &mut f32) -> Response {
    let desired_size = vec2(30.0, 180.0); // 140px de largo total para el recorrido
    let (rect, response) = ui.allocate_at_least(desired_size, Sense::drag());

    let thumb_width = 50.0;   // Ancho/Largo del capuchón
    let thumb_height = 20.0;  // Alto/Grosor del capuchón
    let half_thumb = thumb_height / 2.0;

    let min_y = rect.min.y + half_thumb;
    let max_y = rect.max.y - half_thumb;

    if response.dragged() {
        if let Some(pointer_pos) = response.interact_pointer_pos() {
            let normalized = (1.0 - ((pointer_pos.y - min_y) / (max_y - min_y))).clamp(0.0, 1.0);
            *volume_val = normalized;
        }
    }

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let track_x = rect.center().x;

        // Grosor del riel (4.0 a cada lado = 8px de ancho total)
        let rail_width = 16.0; 

        // 1. Riel base (Gris oscuro)
        let rail_rect = Rect::from_min_max(
            pos2(track_x - rail_width, min_y - 2.0),
            pos2(track_x + rail_width, max_y + 2.0),
        );
        painter.rect_filled(rail_rect, 1.0, Color32::from_rgb(45, 45, 45));

        // 2. Riel lleno desde abajo hacia la posición actual (Cyan)
        let current_norm = volume_val.clamp(0.0, 1.0);
        let thumb_y = max_y - (current_norm * (max_y - min_y));

        if (max_y - thumb_y).abs() > 0.5 {
            let filled_rect = Rect::from_min_max(
                pos2(track_x - rail_width, thumb_y),
                pos2(track_x + rail_width, max_y + 2.0),
            );
            painter.rect_filled(filled_rect, 0.0, Color32::from_rgb(0, 150, 190));
        }

        // 3. Capuchón celeste y rectangular (Thumb)
        let thumb_rect = Rect::from_center_size(
            pos2(track_x, thumb_y),
            vec2(thumb_width, thumb_height),
        );
        let thumb_color = if response.hovered() || response.dragged() {
            Color32::from_rgb(100, 200, 255)
        } else {
            Color32::from_rgb(0, 162, 232)
        };

        painter.rect_filled(thumb_rect, 0.0, thumb_color);
        painter.rect_stroke(thumb_rect, 0.0, Stroke::new(1.0, Color32::BLACK));
    }

    response
}