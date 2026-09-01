/*
 * Hikaru OpenStudio - Playlist / Arrangement View
 * License: AGPL-3.0-later
 */

use egui::{
    Ui, RichText, Color32, ScrollArea, Frame, Stroke, Vec2, Rect, Pos2, 
    Sense, Align2, PointerButton, CursorIcon
};
use std::f32::consts::PI;
use std::path::PathBuf;
use crate::views::mixer::Track;
use crate::audio_proxy::{AudioProxy, GuiCommand};

#[derive(Clone, Debug)]
pub struct CurvePoint {
    pub rel_tick: u64,
    pub value: f32,
    pub tension: f32,
}

#[derive(Clone, Debug)]
pub enum ClipType {
    Pattern { pattern_id: usize },
    Audio { 
        sample_path: String,
        peaks: Vec<f32>,
        sample_offset_ticks: u64, // Para slip-editing / offset interno del audio
    },
    Automation { 
        points: Vec<CurvePoint>,
        target_param: String, 
    },
}

#[derive(Clone, Debug)]
pub struct PlaylistClip {
    pub id: usize,
    pub name: String,
    pub start_tick: u64,
    pub duration_ticks: u64,
    pub clip_type: ClipType,
    pub color: Color32,
}

pub struct PlaylistState {
    pub clips: Vec<(usize, PlaylistClip)>,
    pub playhead_tick: u64,
    pub ppqn: u64,
    pub next_clip_id: usize,
    pub header_width: f32,
    pub grid_numerator: u32,
    pub grid_denominator: u32,
    pub zoom_x: f32,
    pub selected_clips: Vec<usize>,
}

impl Default for PlaylistState {
    fn default() -> Self {
        Self {
            clips: Vec::new(),
            playhead_tick: 0,
            ppqn: 960,
            next_clip_id: 1,
            header_width: 180.0,
            grid_numerator: 1,
            grid_denominator: 4,
            zoom_x: 0.04,
            selected_clips: Vec::new(),
        }
    }
}

// Helper: Conversión estilo Generic DAW (px <-> time)
#[inline]
fn px_to_ticks(px: f32, zoom_x: f32) -> u64 {
    (px.max(0.0) / zoom_x) as u64
}

#[inline]
fn ticks_to_px(ticks: u64, zoom_x: f32) -> f32 {
    ticks as f32 * zoom_x
}

fn knob_ui(ui: &mut Ui, value: &mut f32, radius: f32) -> egui::Response {
    let desired_size = Vec2::splat(radius * 2.0);
    let (rect, response) = ui.allocate_at_least(desired_size, Sense::click_and_drag());

    if response.dragged() {
        let delta = response.drag_delta();
        *value += (delta.x - delta.y) * 0.8; 
        *value = value.clamp(-100.0, 100.0);
    }

    if response.double_clicked() {
        *value = 0.0;
    }

    if ui.is_rect_visible(rect) {
        let center = rect.center();
        let painter = ui.painter();

        painter.circle_filled(center, radius, Color32::from_rgb(35, 35, 42));
        painter.circle_stroke(center, radius, Stroke::new(1.5_f32, Color32::from_gray(80)));

        let norm_val = (*value + 100.0) / 200.0;
        let angle = PI * 1.25 - norm_val * (PI * 1.5);

        let indicator_len = radius - 2.5;
        let line_end = Pos2::new(
            center.x + angle.cos() * indicator_len,
            center.y - angle.sin() * indicator_len,
        );

        painter.line_segment([center, line_end], Stroke::new(2.0_f32, Color32::from_rgb(0, 255, 255)));
    }

    response
}

fn custom_h_slider(ui: &mut Ui, value: &mut f32, width: f32) -> egui::Response {
    let height = 14.0;
    let desired_size = Vec2::new(width, height);
    let (rect, response) = ui.allocate_at_least(desired_size, Sense::click_and_drag());

    if response.double_clicked() {
        *value = 0.75;
    } else if response.dragged() {
        let delta_x = response.drag_delta().x;
        *value = (*value + delta_x / rect.width()).clamp(0.0, 1.0);
    } else if response.clicked() {
        if let Some(mouse_pos) = response.interact_pointer_pos() {
            *value = ((mouse_pos.x - rect.min.x) / rect.width()).clamp(0.0, 1.0);
        }
    }

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();

        let track_rect = Rect::from_center_size(rect.center(), Vec2::new(rect.width(), 4.0));
        painter.rect_filled(track_rect, 2.0, Color32::from_rgb(20, 20, 25));

        let fill_w = rect.width() * (*value).clamp(0.0, 1.0);
        let fill_rect = Rect::from_min_size(track_rect.min, Vec2::new(fill_w, 4.0));
        painter.rect_filled(fill_rect, 2.0, Color32::from_rgb(0, 200, 220));

        let handle_x = rect.min.x + fill_w;
        let handle_rect = Rect::from_center_size(Pos2::new(handle_x, rect.center().y), Vec2::new(8.0, 12.0));
        painter.rect_filled(handle_rect, 1.0, Color32::from_gray(180));
        painter.rect_stroke(handle_rect, 1.0, Stroke::new(1.0_f32, Color32::BLACK));
    }

    response
}

fn load_sample_info(path: &PathBuf, ppqn: u64, bpm: f64) -> (u64, Vec<f32>) {
    let ticks_per_bar = ppqn * 4;
    let default_ticks = ticks_per_bar * 4;
    let mut peaks = Vec::new();

    if let Ok(mut reader) = hound::WavReader::open(path) {
        let spec = reader.spec();
        if spec.sample_rate > 0 {
            let duration_sec = reader.duration() as f32 / spec.sample_rate as f32;
            
            // Calculamos cuántos compases representa la duración del archivo según el BPM del proyecto
            let seconds_per_bar = (60.0 / bpm as f32) * 4.0;
            let bars = duration_sec / seconds_per_bar; 
            let calculated_ticks = ((bars * ticks_per_bar as f32) as u64).max(ppqn);

            let target_peaks = 256;
            let total_samples = reader.duration() as usize;
            let step = (total_samples / target_peaks).max(1);

            let samples: Vec<f32> = match spec.sample_format {
                hound::SampleFormat::Int => {
                    let max_val = (1 << (spec.bits_per_sample - 1)) as f32;
                    reader.samples::<i32>()
                        .filter_map(|s| s.ok())
                        .map(|s| (s as f32 / max_val).abs())
                        .collect()
                }
                hound::SampleFormat::Float => {
                    reader.samples::<f32>()
                        .filter_map(|s| s.ok())
                        .map(|s| s.abs())
                        .collect()
                }
            };

            for chunk in samples.chunks(step) {
                let max_peak = chunk.iter().cloned().fold(0.0_f32, f32::max);
                peaks.push(max_peak.clamp(0.0, 1.0));
            }

            return (calculated_ticks, peaks);
        }
    }
    
    (default_ticks, peaks)
}

pub fn show(
    ui: &mut Ui, 
    state: &mut PlaylistState, 
    tracks: &mut Vec<Track>, 
    current_bar: &mut f32,
    dragged_sample: &mut Option<PathBuf>,
    audio_proxy: &AudioProxy,
    bpm: f64,
    sample_rate: u32,
) {
    let ticks_per_bar = state.ppqn * 4;
    state.playhead_tick = ((*current_bar - 1.0).max(0.0) * ticks_per_bar as f32) as u64;

    let samples_per_beat = (sample_rate as f64 * 60.0) / bpm;
    let samples_per_bar = samples_per_beat * 4.0;

    let zoom_x = state.zoom_x;
    let track_height = 54.0_f32;
    let row_spacing = 0.0_f32;
    let total_row_step = track_height + row_spacing;
    let header_width = state.header_width;

    ui.vertical(|ui| {
        let non_master_count = tracks.iter().filter(|t| !t.is_master).count();

        ui.horizontal(|ui| {
            ui.label(RichText::new("PLAYLIST / ARRANGEMENT").strong().color(Color32::from_rgb(0, 255, 255)));
            ui.separator();
            
            ui.label(RichText::new("Grid:").size(11.0).color(Color32::from_gray(180)));
            
            ui.add(egui::DragValue::new(&mut state.grid_numerator)
                .clamp_range(1..=32)
                .speed(0.1));

            ui.label(RichText::new("/").size(11.0).color(Color32::from_gray(180)));

            egui::ComboBox::from_id_source("grid_denom_combo")
                .selected_text(format!("{}", state.grid_denominator))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut state.grid_denominator, 2, "2");
                    ui.selectable_value(&mut state.grid_denominator, 4, "4");
                    ui.selectable_value(&mut state.grid_denominator, 8, "8");
                    ui.selectable_value(&mut state.grid_denominator, 16, "16");
                });
        });

        ScrollArea::vertical()
            .id_source("playlist_main_scroll")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    
                    // 1. COLUMNA DE HEADERS
                    ui.vertical(|ui| {
                        ui.set_width(header_width);

                        ui.allocate_ui_with_layout(
                            Vec2::new(header_width, 24.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.label(RichText::new("TRACKS").size(10.0).strong().color(Color32::from_gray(140)));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.button(RichText::new("[ - ]").strong().color(Color32::from_gray(160))).clicked() {
                                        if non_master_count > 1 {
                                            if let Some(pos) = tracks.iter().rposition(|t| !t.is_master) {
                                                tracks.remove(pos);
                                            }
                                        }
                                    }
                                    if ui.button(RichText::new("[ + ]").strong().color(Color32::from_rgb(255, 140, 0))).clicked() {
                                        let next_id = tracks.iter().map(|t| t.id).max().unwrap_or(0) + 1;
                                        let track_num = non_master_count + 1;
                                        let new_track = Track::new(next_id, format!("TRACK {:02}", track_num), false);
                                        tracks.push(new_track);
                                    }
                                });
                            }
                        );

                        for track in tracks.iter_mut().filter(|t| !t.is_master) {
                            let (rect, _) = ui.allocate_exact_size(
                                Vec2::new(header_width, total_row_step), 
                                Sense::hover()
                            );

                            ui.allocate_ui_at_rect(rect, |ui| {
                                Frame::none()
                                    .fill(Color32::from_rgb(28, 28, 32))
                                    .stroke(Stroke::new(1.0_f32, Color32::from_gray(45)))
                                    .inner_margin(0.0)
                                    .outer_margin(0.0)
                                    .show(ui, |ui| {
                                        ui.set_min_size(Vec2::new(rect.width() - 8.0, track_height));
                                        ui.vertical(|ui| {
                                            ui.horizontal(|ui| {
                                                let text_width = (header_width - 70.0).max(40.0);
                                                ui.add(
                                                    egui::TextEdit::singleline(&mut track.name)
                                                        .text_color(Color32::WHITE)
                                                        .font(egui::FontId::proportional(11.0))
                                                        .frame(false)
                                                        .desired_width(text_width)
                                                );

                                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                    ui.toggle_value(&mut track.solo, "S");
                                                    ui.toggle_value(&mut track.mute, "M");
                                                });
                                            });

                                            ui.add_space(2.0);

                                            ui.horizontal(|ui| {
                                                knob_ui(ui, &mut track.pan, 8.0);

                                                let pan_text = if track.pan < -1.0 {
                                                    format!("L{:.0}", track.pan.abs())
                                                } else if track.pan > 1.0 {
                                                    format!("R{:.0}", track.pan)
                                                } else {
                                                    "C".to_string()
                                                };
                                                ui.label(RichText::new(pan_text).size(9.0).color(Color32::from_rgb(0, 255, 255)));

                                                ui.add_space(2.0);

                                                let slider_width = (header_width - 90.0).max(30.0);
                                                custom_h_slider(ui, &mut track.volume, slider_width);

                                                let db_val = if track.volume <= 0.0 {
                                                    -60.0
                                                } else if track.volume <= 0.75 {
                                                    -60.0 + (track.volume / 0.75) * 60.0
                                                } else {
                                                    ((track.volume - 0.75) / 0.25) * 6.0
                                                };
                                                ui.label(RichText::new(format!("{:.1}dB", db_val)).size(8.5).weak());
                                            });
                                        });
                                    });
                            });
                        }
                    });

                    // RESIZER
                    let total_height = 24.0 + (non_master_count as f32 * total_row_step);
                    let (resizer_rect, resizer_response) = ui.allocate_at_least(
                        Vec2::new(6.0, total_height), 
                        Sense::click_and_drag()
                    );

                    if resizer_response.hovered() || resizer_response.dragged() {
                        ui.output_mut(|o| o.cursor_icon = CursorIcon::ResizeHorizontal);
                    }

                    if resizer_response.dragged() {
                        let delta = resizer_response.drag_delta().x;
                        state.header_width = (state.header_width + delta).clamp(120.0, 1331.0);
                    }

                    if ui.is_rect_visible(resizer_rect) {
                        let line_x = resizer_rect.center().x;
                        let line_color = if resizer_response.dragged() || resizer_response.hovered() {
                            Color32::from_rgb(0, 255, 255)
                        } else {
                            Color32::from_gray(50)
                        };
                        ui.painter().line_segment(
                            [Pos2::new(line_x, resizer_rect.min.y), Pos2::new(line_x, resizer_rect.max.y)],
                            Stroke::new(2.0, line_color)
                        );
                    }

                    // 2. TIMELINE CANVAS
                    ScrollArea::horizontal()
                        .id_source("timeline_horizontal_scroll")
                        .show(ui, |ui| {
                            let available_w = ui.available_width();
                            let min_canvas_w = ticks_to_px(state.ppqn * 4 * 128, zoom_x); 
                            let canvas_width = available_w.max(min_canvas_w);
                            let total_tracks_height = non_master_count as f32 * total_row_step;
                            let canvas_height: f32 = 24.0 + total_tracks_height;

                            let (response, painter) = ui.allocate_painter(
                                Vec2::new(canvas_width, canvas_height), 
                                Sense::click_and_drag()
                            );

                            // Zoom con Ctrl + Scroll
                            if response.hovered() {
                                let ctrl_pressed = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);
                                if ctrl_pressed {
                                    let mut zoom_delta = 0.0_f32;
                                    ui.input(|i| {
                                        for event in &i.events {
                                            if let egui::Event::MouseWheel { delta, .. } = event {
                                                zoom_delta += delta.y;
                                            }
                                        }
                                    });

                                    if zoom_delta != 0.0 {
                                        let zoom_factor = if zoom_delta > 0.0 { 1.15 } else { 0.85 };
                                        state.zoom_x = (state.zoom_x * zoom_factor).clamp(0.005, 0.5);
                                    }
                                }
                            }

                            let rect = response.rect;
                            painter.rect_filled(rect, 0.0, Color32::from_rgb(16, 16, 20));

                            // RULER & GRID DINÁMICO
                            let ruler_rect = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 24.0));
                            painter.rect_filled(ruler_rect, 0.0, Color32::from_rgb(24, 24, 30));
                            painter.line_segment(
                                [ruler_rect.left_bottom(), ruler_rect.right_bottom()], 
                                Stroke::new(1.0_f32, Color32::from_gray(60))
                            );

                            let snap_step_ticks = (state.ppqn * 4 * state.grid_numerator as u64) / state.grid_denominator as u64;
                            let step_width = ticks_to_px(snap_step_ticks, zoom_x);
                            let total_grid_steps = (canvas_width / step_width.max(1.0)).ceil() as usize;
                            let steps_per_bar = (state.grid_denominator / state.grid_numerator).max(1) as usize;

                            for step in 0..total_grid_steps {
                                let x = rect.min.x + (step as f32 * step_width);
                                let is_main_bar = step % steps_per_bar == 0;

                                let line_color = if is_main_bar {
                                    Color32::from_gray(50)
                                } else {
                                    Color32::from_gray(28)
                                };

                                painter.line_segment(
                                    [Pos2::new(x, rect.min.y + 24.0), Pos2::new(x, rect.max.y)],
                                    Stroke::new(1.0_f32, line_color)
                                );

                                if is_main_bar {
                                    let bar_num = (step / steps_per_bar) + 1;
                                    painter.text(
                                        Pos2::new(x + 4.0, rect.min.y + 4.0),
                                        Align2::LEFT_TOP,
                                        format!("{}", bar_num),
                                        egui::FontId::monospace(10.0),
                                        Color32::from_gray(180)
                                    );
                                }
                            }

                            // RENDER DE PISTAS Y CLIPS
                            let mut current_y = rect.min.y + 24.0;
                            let valid_tracks: Vec<usize> = tracks.iter().filter(|t| !t.is_master).map(|t| t.id).collect();
                            let mut clip_to_delete: Option<usize> = None;

                            for track_id in &valid_tracks {
                                let track_rect = Rect::from_min_size(
                                    Pos2::new(rect.min.x, current_y), 
                                    Vec2::new(rect.width(), track_height)
                                );

                                painter.line_segment(
                                    [track_rect.left_bottom(), track_rect.right_bottom()], 
                                    Stroke::new(1.0_f32, Color32::from_gray(28))
                                );

                                for (clip_track_id, clip) in state.clips.iter_mut() {
                                    if clip_track_id == track_id {
                                        let clip_x = rect.min.x + ticks_to_px(clip.start_tick, zoom_x);
                                        let clip_w = ticks_to_px(clip.duration_ticks, zoom_x).max(8.0);

                                        let clip_rect = Rect::from_min_size(
                                            Pos2::new(clip_x, current_y + 1.0), 
                                            Vec2::new(clip_w, track_height - 2.0)
                                        );

                                        // Zonas de Trim (Edges izq y der) - Choreo visual del Generic DAW
                                        let trim_handle_w = 6.0_f32;
                                        let left_trim_rect = Rect::from_min_size(clip_rect.min, Vec2::new(trim_handle_w, clip_rect.height()));
                                        let right_trim_rect = Rect::from_min_size(
                                            Pos2::new(clip_rect.max.x - trim_handle_w, clip_rect.min.y), 
                                            Vec2::new(trim_handle_w, clip_rect.height())
                                        );

                                        let clip_id_egui = ui.make_persistent_id(format!("clip_{}_{}", clip_track_id, clip.id));
                                        let clip_response = ui.interact(clip_rect, clip_id_egui, Sense::click_and_drag());

                                        // Click Derecho para eliminar (estilo Generic DAW)
                                        if clip_response.secondary_clicked() {
                                            clip_to_delete = Some(clip.id);
                                        }

                                        // LÓGICA DE DRAG / TRIM / SLIP
                                        if let Some(mouse_pos) = clip_response.interact_pointer_pos() {
                                            if left_trim_rect.contains(mouse_pos) || right_trim_rect.contains(mouse_pos) {
                                                ui.output_mut(|o| o.cursor_icon = CursorIcon::ResizeHorizontal);
                                            } else if clip_response.dragged() {
                                                ui.output_mut(|o| o.cursor_icon = CursorIcon::Grabbing);
                                            } else if clip_response.hovered() {
                                                ui.output_mut(|o| o.cursor_icon = CursorIcon::Grab);
                                            }

                                            if clip_response.dragged() {
                                                if clip_response.drag_started_by(PointerButton::Primary) {
                                                    let grab_offset = (mouse_pos.x - clip_rect.min.x).max(0.0);
                                                    ui.data_mut(|d| d.insert_temp(clip_id_egui, grab_offset));
                                                }

                                                let grab_offset_x: f32 = ui.data(|d| d.get_temp(clip_id_egui)).unwrap_or(0.0);

                                                // Si arrastra el borde derecho (Trim End)
                                                if right_trim_rect.contains(mouse_pos) || ui.data(|d| d.get_temp(ui.make_persistent_id("trim_end")).unwrap_or(false)) {
                                                    ui.data_mut(|d| d.insert_temp(ui.make_persistent_id("trim_end"), true));
                                                    let new_end_px = (mouse_pos.x - rect.min.x).max(clip_x + step_width);
                                                    let new_end_tick = px_to_ticks(new_end_px, zoom_x);
                                                    
                                                    let snapped_end = if snap_step_ticks > 0 {
                                                        ((new_end_tick as f64 / snap_step_ticks as f64).round() as u64) * snap_step_ticks
                                                    } else {
                                                        new_end_tick
                                                    };

                                                    if snapped_end > clip.start_tick {
                                                        clip.duration_ticks = snapped_end - clip.start_tick;
                                                    }
                                                } 
                                                // Mover clip entero
                                                else {
                                                    let mouse_x_relative = (mouse_pos.x - rect.min.x - grab_offset_x).max(0.0);
                                                    let raw_tick = px_to_ticks(mouse_x_relative, zoom_x);

                                                    clip.start_tick = if snap_step_ticks > 0 {
                                                        ((raw_tick as f64 / snap_step_ticks as f64).round() as u64) * snap_step_ticks
                                                    } else {
                                                        raw_tick
                                                    };

                                                    let rel_y = mouse_pos.y - (rect.min.y + 24.0);
                                                    if rel_y >= 0.0 {
                                                        let target_idx = (rel_y / total_row_step).floor() as usize;
                                                        if target_idx < valid_tracks.len() {
                                                            *clip_track_id = valid_tracks[target_idx];
                                                        }
                                                    }
                                                }
                                            } else {
                                                ui.data_mut(|d| d.insert_temp(ui.make_persistent_id("trim_end"), false));
                                            }
                                        }

                                        // RENDER ESTILO GENERIC DAW (Solid fill + Stroke)
                                        let is_hovered = clip_response.hovered();
                                        let is_dragged = clip_response.dragged();

                                        let (border_color, border_width) = if is_dragged {
                                            (Color32::from_rgb(0, 255, 255), 2.0_f32)
                                        } else if is_hovered {
                                            (Color32::WHITE, 1.5_f32)
                                        } else {
                                            (Color32::from_white_alpha(100), 1.0_f32)
                                        };

                                        painter.rect_filled(clip_rect, 2.0, clip.color);
                                        painter.rect_stroke(clip_rect, 2.0, Stroke::new(border_width, border_color));

                                        // WAVEFORM RENDERER (Robado de generic_daw_gui/src/widget/clip.rs)
                                        if let ClipType::Audio { peaks, .. } = &clip.clip_type {
                                            if !peaks.is_empty() {
                                                let inner_rect = clip_rect.shrink2(Vec2::new(2.0, 4.0));
                                                let center_y = inner_rect.center().y;
                                                let wave_color = Color32::from_rgba_unmultiplied(255, 255, 255, 180);

                                                let step_x = 2.0_f32;
                                                let max_bars = (inner_rect.width() / step_x) as usize;

                                                for i in 0..max_bars {
                                                    let x = inner_rect.min.x + (i as f32 * step_x);
                                                    if x >= inner_rect.max.x { break; }

                                                    let peak_idx = ((i as f32 / max_bars as f32) * peaks.len() as f32) as usize;
                                                    let peak_val = peaks.get(peak_idx).cloned().unwrap_or(0.0);

                                                    let bar_height = (inner_rect.height() * 0.8) * peak_val;
                                                    if bar_height > 0.5 {
                                                        painter.line_segment(
                                                            [
                                                                Pos2::new(x, center_y - bar_height * 0.5),
                                                                Pos2::new(x, center_y + bar_height * 0.5),
                                                            ],
                                                            Stroke::new(1.0, wave_color),
                                                        );
                                                    }
                                                }
                                            }
                                        }

                                        // Header / Clip Title
                                        painter.text(
                                            clip_rect.min + Vec2::new(4.0, 2.0),
                                            Align2::LEFT_TOP,
                                            &clip.name,
                                            egui::FontId::proportional(10.0),
                                            Color32::WHITE
                                        );
                                    }
                                }

                                current_y += total_row_step;
                            }

                            // Borrar clip si se hizo click derecho
                            if let Some(id_del) = clip_to_delete {
                                state.clips.retain(|(_, c)| c.id != id_del);
                            }

                            // PLAYHEAD (Sincronización por Ticks)
                            let playhead_x = rect.min.x + ticks_to_px(state.playhead_tick, zoom_x);
                            painter.line_segment(
                                [Pos2::new(playhead_x, rect.min.y), Pos2::new(playhead_x, rect.max.y)],
                                Stroke::new(2.0_f32, Color32::from_rgb(0, 255, 255))
                            );

                            // Mover Playhead al hacer clic en el Ruler / Canvas
                            if response.dragged() || response.clicked() {
                                if let Some(pointer_pos) = response.interact_pointer_pos() {
                                    let rel_x = (pointer_pos.x - rect.min.x).max(0.0);
                                    let clicked_ticks = px_to_ticks(rel_x, zoom_x);
                                    *current_bar = (clicked_ticks as f32 / ticks_per_bar as f32) + 1.0;
                                    
                                    let target_samples = ((*current_bar - 1.0) as f64 * samples_per_bar) as u64;
                                    audio_proxy.send(GuiCommand::Seek { sample_count: target_samples });
                                }
                            }
                            
                            // DROP DE ARCHIVOS AUDIO
                            if let Some(path) = dragged_sample.clone() {
                                if ui.input(|i| i.pointer.any_released() || i.pointer.primary_released()) {
                                    if let Some(drop_pos) = ui.input(|i| i.pointer.hover_pos()) {
                                        if rect.contains(drop_pos) {
                                            let rel_x = (drop_pos.x - rect.min.x).max(0.0);
                                            let raw_drop_tick = px_to_ticks(rel_x, zoom_x);
                                            
                                            let drop_tick = if snap_step_ticks > 0 {
                                                ((raw_drop_tick as f64 / snap_step_ticks as f64).round() as u64) * snap_step_ticks
                                            } else {
                                                raw_drop_tick
                                            };

                                            let rel_y = drop_pos.y - (rect.min.y + 24.0);
                                            if rel_y >= 0.0 {
                                                let track_idx = (rel_y / total_row_step).floor() as usize;

                                                let valid_tracks_list: Vec<&Track> = tracks.iter().filter(|t| !t.is_master).collect();
                                                if track_idx < valid_tracks_list.len() {
                                                    let target_track_id = valid_tracks_list[track_idx].id;
                                                    let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                                                    let sample_path_str = path.to_string_lossy().to_string();
                                                    
                                                    // PASS BPM HERE:
                                                    let (duration_ticks, peaks) = load_sample_info(&path, state.ppqn, bpm);

                                                    let clip = PlaylistClip {
                                                        id: state.next_clip_id,
                                                        name: file_name,
                                                        start_tick: drop_tick,
                                                        duration_ticks,
                                                        clip_type: ClipType::Audio { 
                                                            sample_path: sample_path_str.clone(),
                                                            peaks,
                                                            sample_offset_ticks: 0,
                                                        },
                                                        color: Color32::from_rgb(32, 95, 145),
                                                    };

                                                    state.next_clip_id += 1;
                                                    state.clips.push((target_track_id, clip));

                                                    let seconds_per_tick = 60.0 / (bpm as f32 * state.ppqn as f32);
                                                    let position_secs = drop_tick as f32 * seconds_per_tick;

                                                    audio_proxy.send(GuiCommand::LoadClip {
                                                        path: sample_path_str,
                                                        position_secs,
                                                        track_index: target_track_id,
                                                    });
                                                }
                                            }
                                        }
                                    }
                                    *dragged_sample = None;
                                }
                            }
                        });
                });
            });
    });
}