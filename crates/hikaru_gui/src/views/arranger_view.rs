use egui::*;
use crate::views::mixer;
use hikaru_transport::TransportPosition;
use crate::audio_proxy::AudioProxy;
// Importamos el estado y handlers de la Session Matrix
use crate::views::matrix::{self, SessionMatrixState, SlotState};

pub fn show(
    ui: &mut Ui,
    tracks: &mut [mixer::Track],
    matrix_state: &mut SessionMatrixState, // <-- Recibimos el estado de la matriz
    _transport: &TransportPosition,
    audio_proxy: &AudioProxy,               // <-- Usado para emitir los GuiCommands
) {
    if tracks.is_empty() {
        return;
    }

    let track_width = 100.0;
    let clip_slot_height = 25.0;
    let scenes_count = matrix_state.scenes.len(); // Usamos la cantidad dinámica de escenas

    let (master_track, audio_tracks) = tracks.split_first_mut().unwrap();

    ScrollArea::both()
        .id_source("session_matrix_scroll")
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // 1. COLUMNA ESCENAS / MASTER (Panel Izquierdo)
                ui.vertical(|ui| {
                    ui.set_width(track_width);
                    
                    Frame::group(ui.style()).show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(RichText::new("SCENES").strong().color(Color32::LIGHT_BLUE));
                            ui.label(RichText::new("Master Launch").size(9.0));
                        });
                    });

                    ui.add_space(2.0);

                    // Renderizado y disparo dinámico de Escenas
                    for scene_idx in 0..scenes_count {
                        let (slot_rect, response) = ui.allocate_exact_size(
                            Vec2::new(track_width, clip_slot_height),
                            Sense::click(),
                        );

                        // ACCIÓN: Al hacer click en el disparador de escena de la izquierda
                        if response.clicked() {
                            matrix::trigger_scene(matrix_state, audio_proxy, scene_idx);
                        }

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

                            let scene_name = matrix_state.scenes.get(scene_idx)
                                .map(|s| s.name.as_str())
                                .unwrap_or("Scene");

                            painter.text(
                                slot_rect.center(),
                                Align2::CENTER_CENTER,
                                format!("▶ {}", scene_name),
                                FontId::proportional(10.0),
                                Color32::WHITE,
                            );
                        }

                        ui.add_space(1.0);
                    }

                    ui.add_space(62.0);
                    ui.separator();

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

                ui.separator();

                // 2. MATRIZ DE PISTAS DE AUDIO SINCRONIZADA CON MATRIX_STATE
                for (i, track) in audio_tracks.iter_mut().enumerate() {
                    ui.vertical(|ui| {
                        ui.set_width(track_width);

                        Frame::group(ui.style()).show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.label(RichText::new(format!("TRK {:02}", i + 1)).strong());
                                ui.label(RichText::new(format!("Audio {}", i + 1)).size(9.0));
                            });
                        });

                        ui.add_space(2.0);

                        for scene_idx in 0..scenes_count {
                            let (slot_rect, response) = ui.allocate_exact_size(
                                Vec2::new(track_width, clip_slot_height),
                                Sense::click(),
                            );

                            // ACCIÓN: Al hacer click en un Pad/Slot individual
                            if response.clicked() && i < matrix_state.grid.len() {
                                matrix_state.selected_slot = Some((i, scene_idx));
                                matrix::trigger_pad(matrix_state, audio_proxy, i, scene_idx);
                            }

                            if ui.is_rect_visible(slot_rect) {
                                let painter = ui.painter();
                                
                                // Lectura del estado real del Slot (Playing, Stopped, Empty, etc.)
                                let slot_info = matrix_state.grid.get(i).and_then(|row| row.get(scene_idx));
                                let (fill_color, border_color, text_label) = match slot_info {
                                    Some(slot) => match &slot.state {
                                        SlotState::Playing => (
                                            Color32::from_rgb(35, 135, 60),
                                            Color32::GREEN,
                                            slot.clip.as_ref().map(|c| format!("▶ {}", c.name)).unwrap_or_else(|| "▶ Clip".into()),
                                        ),
                                        SlotState::Stopped => (
                                            Color32::from_rgb(45, 55, 75),
                                            Color32::from_rgb(90, 130, 190),
                                            slot.clip.as_ref().map(|c| c.name.clone()).unwrap_or_else(|| "[ Slot ]".into()),
                                        ),
                                        _ => (
                                            if response.hovered() { Color32::from_rgb(35, 35, 35) } else { Color32::from_rgb(25, 25, 25) },
                                            Color32::from_gray(45),
                                            format!("[ Scene {} ]", scene_idx + 1),
                                        ),
                                    },
                                    None => (Color32::from_rgb(25, 25, 25), Color32::from_gray(45), "".into()),
                                };

                                painter.rect_filled(slot_rect, 2.0, fill_color);
                                painter.rect_stroke(slot_rect, 2.0, Stroke::new(1.0_f32, border_color));

                                painter.text(
                                    slot_rect.center(),
                                    Align2::CENTER_CENTER,
                                    text_label,
                                    FontId::proportional(9.0),
                                    Color32::WHITE,
                                );
                            }

                            ui.add_space(1.0);
                        }

                        ui.add_space(62.0);
                        ui.separator();

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