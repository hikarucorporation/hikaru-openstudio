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

/// Modo de renderizado del carril de Playlist.
/// - `Arranger`: vista principal (Timeline global), manda comandos reales
///   al `hikaru_audio_engine` (Seek, LoadClip, UpdateClipBounds) y permite
///   agregar/quitar pistas.
/// - `ClipEditor`: instancia embebida dentro de `matrix.rs` (Session Matrix
///   Clip Editor). Usa la MISMA lógica visual y de edición (grilla, waveform,
///   trim, slice, drag&drop) pero:
///     * No dispara `Seek` global (el "playhead" es local al contenedor).
///     * No manda `LoadClip`/`UpdateClipBounds` al motor todavía: la
///       reproducción real de sub-clips dentro de un `MatrixClip` es
///       responsabilidad de `hikaru_sequencer` (FASE 4, ver ORDEN-DESARROLLO.md)
///       y aún no está implementada en el motor. Queda marcado con TODO.
///     * Oculta los botones [+]/[-] de pistas (el contenedor es de 1 sola pista).
#[derive(Clone, Copy)]
pub enum ViewMode<'a> {
    Arranger,
    ClipEditor { title: &'a str },
}

impl<'a> ViewMode<'a> {
    #[inline]
    fn drives_engine(&self) -> bool {
        matches!(self, ViewMode::Arranger)
    }

    #[inline]
    fn allows_track_add_remove(&self) -> bool {
        matches!(self, ViewMode::Arranger)
    }

    fn header_label(&self) -> String {
        match self {
            ViewMode::Arranger => "PLAYLIST / ARRANGEMENT".to_string(),
            ViewMode::ClipEditor { title } => title.to_string(),
        }
    }
}

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
        sample_offset_ticks: u64, // Slip editing / Trim inicio en ticks
        total_sample_ticks: u64,  // Duración total original del audio en ticks
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

#[derive(Clone, Debug)]
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
    pub clipboard: Vec<(usize, PlaylistClip)>,
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
            clipboard: Vec::new(),
        }
    }
}

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
    let mut peaks = Vec::new();

    match hound::WavReader::open(path) {
        Ok(mut reader) => {
            let spec = reader.spec();
            let total_frames = reader.duration() as u64; // frames (per-channel)
            let channels = spec.channels.max(1) as u64;

            // Log de diagnóstico — sacalo una vez confirmemos el root cause
            eprintln!(
                "[PLAYLIST DEBUG] {:?} | fmt={:?} bits={} ch={} sr={} frames={}",
                path.file_name().unwrap_or_default(),
                spec.sample_format, spec.bits_per_sample, spec.channels,
                spec.sample_rate, total_frames
            );

            if spec.sample_rate > 0 && total_frames > 0 && bpm > 0.0 {
                let duration_sec = total_frames as f64 / spec.sample_rate as f64;
                let seconds_per_tick = (60.0 / bpm) / ppqn.max(1) as f64;
                let calculated_ticks = (duration_sec / seconds_per_tick).round() as u64;

                eprintln!(
                    "[PLAYLIST DEBUG] duration_sec={:.4} -> calculated_ticks={}",
                    duration_sec, calculated_ticks
                );

                let target_peaks = 512;
                // step en FRAMES, no en samples interleaved
                let step_frames = ((total_frames as usize) / target_peaks).max(1);
                let step_samples = step_frames * channels as usize;

                let samples: Vec<f32> = match spec.sample_format {
                    hound::SampleFormat::Int => {
                        let max_val = (1i64 << (spec.bits_per_sample - 1)) as f32;
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

                for chunk in samples.chunks(step_samples) {
                    let max_peak = chunk.iter().cloned().fold(0.0_f32, f32::max);
                    peaks.push(max_peak.clamp(0.0, 1.0));
                }

                return (calculated_ticks.max(1), peaks);
            } else {
                eprintln!(
                    "[PLAYLIST ERROR] Datos inválidos: sr={} frames={} bpm={}",
                    spec.sample_rate, total_frames, bpm
                );
            }
        }
        Err(err) => {
            eprintln!("[PLAYLIST ERROR] Hound no pudo abrir {:?}: {}", path, err);
        }
    }

    (0, peaks)
}

/// Construye un `PlaylistClip` de tipo `Audio` a partir de un sample en disco.
/// Centraliza la lectura de `hound` + generación de peaks para que tanto el
/// Arranger como el Clip Editor de la Session Matrix (`matrix.rs`) creen
/// clips 100% compatibles entre sí (mismo `ClipType::Audio`).
pub fn build_audio_clip(
    id: usize,
    name: String,
    path: &PathBuf,
    start_tick: u64,
    ppqn: u64,
    bpm: f64,
    color: Color32,
) -> PlaylistClip {
    let (duration_ticks, peaks) = load_sample_info(path, ppqn, bpm);
    let duration_ticks = duration_ticks.max(1);

    PlaylistClip {
        id,
        name,
        start_tick,
        duration_ticks,
        clip_type: ClipType::Audio {
            sample_path: path.to_string_lossy().to_string(),
            peaks,
            sample_offset_ticks: 0,
            total_sample_ticks: duration_ticks,
        },
        color,
    }
}

/// Vista principal del Arranger (Timeline global). Manda comandos reales
/// al `hikaru_audio_engine`.
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
    show_impl(
        ui, state, tracks, current_bar, dragged_sample, audio_proxy, bpm, sample_rate,
        ViewMode::Arranger,
    );
}

/// Vista embebida usada por `matrix.rs` dentro del Clip Editor de la Session
/// Matrix. Reutiliza exactamente el mismo carril (header + grilla + waveform
/// + trim + slice + drag&drop) que el Arranger, pero acotado a la(s)
/// pista(s) locales del `MatrixClip` seleccionado y sin tocar el transporte
/// global ni el motor de audio real (ver `ViewMode::ClipEditor`).
pub fn show_embedded(
    ui: &mut Ui,
    state: &mut PlaylistState,
    tracks: &mut Vec<Track>,
    local_bar: &mut f32,
    dragged_sample: &mut Option<PathBuf>,
    audio_proxy: &AudioProxy,
    bpm: f64,
    sample_rate: u32,
    title: &str,
) {
    show_impl(
        ui, state, tracks, local_bar, dragged_sample, audio_proxy, bpm, sample_rate,
        ViewMode::ClipEditor { title },
    );
}

fn show_impl(
    ui: &mut Ui,
    state: &mut PlaylistState,
    tracks: &mut Vec<Track>,
    current_bar: &mut f32,
    dragged_sample: &mut Option<PathBuf>,
    audio_proxy: &AudioProxy,
    bpm: f64,
    sample_rate: u32,
    mode: ViewMode,
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

    let ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);
    let shift = ui.input(|i| i.modifiers.shift);

    // Helper genérico para convertir ticks a segundos (CORREGIDO)
    let ticks_to_secs = |ticks: u64| -> f32 {
        (ticks as f32 / state.ppqn as f32) * (60.0 / bpm as f32)
    };

    if ctrl && ui.input(|i| i.key_pressed(egui::Key::C)) {
        state.clipboard = state.clips.iter()
            .filter(|(_, c)| state.selected_clips.contains(&c.id))
            .cloned()
            .collect();
    }

    if ctrl && ui.input(|i| i.key_pressed(egui::Key::X)) {
        state.clipboard = state.clips.iter()
            .filter(|(_, c)| state.selected_clips.contains(&c.id))
            .cloned()
            .collect();
        state.clips.retain(|(_, c)| !state.selected_clips.contains(&c.id));
        state.selected_clips.clear();
    }

    if ctrl && ui.input(|i| i.key_pressed(egui::Key::V)) {
        if !state.clipboard.is_empty() {
            let min_start = state.clipboard.iter().map(|(_, c)| c.start_tick).min().unwrap_or(0);
            let mut new_selection = Vec::new();

            for (track_id, clip) in state.clipboard.clone() {
                let offset = clip.start_tick.saturating_sub(min_start);
                let mut new_clip = clip.clone();
                new_clip.id = state.next_clip_id;
                new_clip.start_tick = state.playhead_tick + offset;

                if mode.drives_engine() {
                    if let ClipType::Audio { ref sample_path, sample_offset_ticks, .. } = new_clip.clip_type {
                        let position_secs = ticks_to_secs(new_clip.start_tick);

                        // Enviamos LoadClip completo de una
                        audio_proxy.send(GuiCommand::LoadClip {
                            clip_id: new_clip.id,
                            path: sample_path.clone(),
                            position_secs,
                            duration_secs: ticks_to_secs(new_clip.duration_ticks),
                            offset_secs: ticks_to_secs(sample_offset_ticks),
                            track_index: track_id,
                        });
                    }
                }

                new_selection.push(new_clip.id);
                state.clips.push((track_id, new_clip));
                state.next_clip_id += 1;
            }
            state.selected_clips = new_selection;
        }
    }

    if ctrl && ui.input(|i| i.key_pressed(egui::Key::D)) {
        let selected_items: Vec<(usize, PlaylistClip)> = state.clips.iter()
            .filter(|(_, c)| state.selected_clips.contains(&c.id))
            .cloned()
            .collect();

        if !selected_items.is_empty() {
            let min_start = selected_items.iter().map(|(_, c)| c.start_tick).min().unwrap_or(0);
            let max_end = selected_items.iter().map(|(_, c)| c.start_tick + c.duration_ticks).max().unwrap_or(0);
            let duration_block = max_end - min_start;
            let mut new_selection = Vec::new();

            for (track_id, clip) in selected_items {
                let mut new_clip = clip.clone();
                new_clip.id = state.next_clip_id;
                new_clip.start_tick = clip.start_tick + duration_block;

                if mode.drives_engine() {
                    if let ClipType::Audio { ref sample_path, sample_offset_ticks, .. } = new_clip.clip_type {
                        let position_secs = ticks_to_secs(new_clip.start_tick);

                        audio_proxy.send(GuiCommand::LoadClip {
                            clip_id: new_clip.id,
                            path: sample_path.clone(),
                            position_secs,
                            duration_secs: ticks_to_secs(new_clip.duration_ticks),
                            offset_secs: ticks_to_secs(sample_offset_ticks),
                            track_index: track_id,
                        });
                    }
                }

                new_selection.push(new_clip.id);
                state.clips.push((track_id, new_clip));
                state.next_clip_id += 1;
            }
            state.selected_clips = new_selection;
        }
    }

    if ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) {
        state.clips.retain(|(_, c)| !state.selected_clips.contains(&c.id));
        state.selected_clips.clear();
    }

    ui.vertical(|ui| {
        let non_master_count = tracks.iter().filter(|t| !t.is_master).count();

        ui.horizontal(|ui| {
            ui.label(RichText::new(mode.header_label()).strong().color(Color32::from_rgb(0, 255, 255)));
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
                    ui.vertical(|ui| {
                        ui.set_width(header_width);

                        ui.allocate_ui_with_layout(
                            Vec2::new(header_width, 24.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.label(RichText::new("TRACKS").size(10.0).strong().color(Color32::from_gray(140)));
                                if mode.allows_track_add_remove() {
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
                            Stroke::new(2.0_f32, line_color)
                        );
                    }

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

                            if response.clicked_by(PointerButton::Primary) && !shift {
                                state.selected_clips.clear();
                            }

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

                            // Herramienta de corte (Slice, tecla 'S'). Compartida entre el Arranger
                            // y el Clip Editor embebido de la Session Matrix: parte un clip de Audio
                            // en dos en la posición del cursor, respetando sample_offset_ticks.
                            let slice_key_pressed = ui.input(|i| i.key_pressed(egui::Key::S));
                            let slice_hover_pos = response.interact_pointer_pos()
                                .or_else(|| ui.input(|i| i.pointer.hover_pos()));
                            let mut pending_slices: Vec<(usize, PlaylistClip)> = Vec::new();

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

                                // Detección de clips y Trimming limpio
                                for (clip_track_id, clip) in state.clips.iter_mut() {
                                    if clip_track_id == track_id {
                                        let clip_x = rect.min.x + ticks_to_px(clip.start_tick, zoom_x);
                                        let clip_w = ticks_to_px(clip.duration_ticks, zoom_x).max(12.0);

                                        let clip_rect = Rect::from_min_size(
                                            Pos2::new(clip_x, current_y + 1.0), 
                                            Vec2::new(clip_w, track_height - 2.0)
                                        );

                                        // Zonas del borde (Trim Handles)
                                        let handle_w = 6.0_f32;
                                        let left_handle_rect = Rect::from_min_size(clip_rect.min, Vec2::new(handle_w, clip_rect.height()));
                                        let right_handle_rect = Rect::from_min_size(
                                            Pos2::new(clip_rect.max.x - handle_w, clip_rect.min.y), 
                                            Vec2::new(handle_w, clip_rect.height())
                                        );

                                        let id_left = ui.make_persistent_id(format!("trim_L_{}_{}", clip_track_id, clip.id));
                                        let id_right = ui.make_persistent_id(format!("trim_R_{}_{}", clip_track_id, clip.id));
                                        let id_body = ui.make_persistent_id(format!("clip_body_{}_{}", clip_track_id, clip.id));

                                        let body_rect = Rect::from_min_max(
                                            Pos2::new(clip_rect.min.x + handle_w, clip_rect.min.y),
                                            Pos2::new(clip_rect.max.x - handle_w, clip_rect.max.y),
                                        );

                                        let resp_left = ui.interact(left_handle_rect, id_left, Sense::drag());
                                        let resp_right = ui.interact(right_handle_rect, id_right, Sense::drag());
                                        let resp_body = ui.interact(body_rect, id_body, Sense::click_and_drag());

                                        if resp_left.hovered() || resp_right.hovered() || resp_left.dragged() || resp_right.dragged() {
                                            ui.output_mut(|o| o.cursor_icon = CursorIcon::ResizeHorizontal);
                                        }

                                        let is_selected = state.selected_clips.contains(&clip.id);

                                        // SLICE ('S'): corta el clip en dos en la posición del cursor.
                                        if slice_key_pressed {
                                            if let Some(pos) = slice_hover_pos {
                                                if clip_rect.contains(pos) {
                                                    let rel_x = (pos.x - rect.min.x).max(0.0);
                                                    let raw_slice_tick = px_to_ticks(rel_x, zoom_x);
                                                    let slice_tick = if snap_step_ticks > 0 {
                                                        ((raw_slice_tick as f64 / snap_step_ticks as f64).round() as u64) * snap_step_ticks
                                                    } else {
                                                        raw_slice_tick
                                                    };

                                                    let clip_end_tick = clip.start_tick + clip.duration_ticks;

                                                    if slice_tick > clip.start_tick && slice_tick < clip_end_tick {
                                                        if let ClipType::Audio { ref sample_path, ref peaks, sample_offset_ticks, total_sample_ticks } = clip.clip_type {
                                                            let left_duration = slice_tick - clip.start_tick;
                                                            let right_offset = sample_offset_ticks + left_duration;
                                                            let right_duration = clip_end_tick - slice_tick;

                                                            let right_clip = PlaylistClip {
                                                                id: state.next_clip_id,
                                                                name: format!("{} (Slice)", clip.name),
                                                                start_tick: slice_tick,
                                                                duration_ticks: right_duration,
                                                                clip_type: ClipType::Audio {
                                                                    sample_path: sample_path.clone(),
                                                                    peaks: peaks.clone(),
                                                                    sample_offset_ticks: right_offset,
                                                                    total_sample_ticks,
                                                                },
                                                                color: clip.color,
                                                            };

                                                            pending_slices.push((*clip_track_id, right_clip));
                                                            state.next_clip_id += 1;
                                                            clip.duration_ticks = left_duration;
                                                        }
                                                    }
                                                }
                                            }
                                        }

                                        // 1. TRIM IZQUIERDO (Start Trim + Offset)
                                        if resp_left.dragged() {
                                            if let Some(mouse_pos) = resp_left.interact_pointer_pos() {
                                                let mouse_x_rel = (mouse_pos.x - rect.min.x).max(0.0);
                                                let raw_target_tick = px_to_ticks(mouse_x_rel, zoom_x);
                                                
                                                let snapped_target_tick = if snap_step_ticks > 0 {
                                                    ((raw_target_tick as f64 / snap_step_ticks as f64).round() as u64) * snap_step_ticks
                                                } else {
                                                    raw_target_tick
                                                };

                                                if let ClipType::Audio { ref mut sample_offset_ticks, total_sample_ticks, .. } = clip.clip_type {
                                                    let current_end_tick = clip.start_tick + clip.duration_ticks;
                                                    
                                                    if snapped_target_tick < current_end_tick {
                                                        let original_start = clip.start_tick.saturating_sub(*sample_offset_ticks);
                                                        
                                                        if snapped_target_tick >= original_start {
                                                            let new_offset = snapped_target_tick - original_start;
                                                            if new_offset < total_sample_ticks {
                                                                *sample_offset_ticks = new_offset;
                                                                clip.start_tick = snapped_target_tick;
                                                                clip.duration_ticks = current_end_tick - snapped_target_tick;
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        } else if resp_left.drag_stopped() {
                                            if mode.drives_engine() {
                                                if let ClipType::Audio { sample_offset_ticks, .. } = clip.clip_type {
                                                    audio_proxy.send(GuiCommand::UpdateClipBounds {
                                                        clip_id: clip.id,
                                                        position_secs: ticks_to_secs(clip.start_tick),
                                                        duration_secs: ticks_to_secs(clip.duration_ticks),
                                                        offset_secs: ticks_to_secs(sample_offset_ticks),
                                                    });
                                                }
                                            }
                                        }
                                        // 2. TRIM DERECHO (End Trim)
                                        else if resp_right.dragged() {
                                            if let Some(mouse_pos) = resp_right.interact_pointer_pos() {
                                                let mouse_x_rel = (mouse_pos.x - rect.min.x).max(0.0);
                                                let raw_target_tick = px_to_ticks(mouse_x_rel, zoom_x);
                                                
                                                let snapped_target_tick = if snap_step_ticks > 0 {
                                                    ((raw_target_tick as f64 / snap_step_ticks as f64).round() as u64) * snap_step_ticks
                                                } else {
                                                    raw_target_tick
                                                };

                                                if snapped_target_tick > clip.start_tick {
                                                    let requested_duration = snapped_target_tick - clip.start_tick;

                                                    if let ClipType::Audio { sample_offset_ticks, total_sample_ticks, .. } = clip.clip_type {
                                                        let max_available_duration = total_sample_ticks.saturating_sub(sample_offset_ticks);
                                                        let new_duration = requested_duration.min(max_available_duration).max(1);
                                                        clip.duration_ticks = new_duration;
                                                    }
                                                }
                                            }
                                        } else if resp_right.drag_stopped() {
                                            if mode.drives_engine() {
                                                if let ClipType::Audio { sample_offset_ticks, .. } = clip.clip_type {
                                                    audio_proxy.send(GuiCommand::UpdateClipBounds {
                                                        clip_id: clip.id,
                                                        position_secs: ticks_to_secs(clip.start_tick),
                                                        duration_secs: ticks_to_secs(clip.duration_ticks),
                                                        offset_secs: ticks_to_secs(sample_offset_ticks),
                                                    });
                                                }
                                            }
                                        }
                                        // 3. MOVER CLIP ENTERO
                                        else if resp_body.dragged() {
                                            if resp_body.drag_started_by(PointerButton::Primary) {
                                                if let Some(mouse_pos) = resp_body.interact_pointer_pos() {
                                                    let grab_offset = (mouse_pos.x - clip_rect.min.x).max(0.0);
                                                    ui.data_mut(|d| d.insert_temp(id_body, grab_offset));
                                                }
                                            }

                                            if let Some(mouse_pos) = resp_body.interact_pointer_pos() {
                                                let grab_offset_x: f32 = ui.data(|d| d.get_temp(id_body)).unwrap_or(0.0);
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
                                        } else if resp_body.drag_stopped() {
                                            if mode.drives_engine() {
                                                if let ClipType::Audio { sample_offset_ticks, .. } = clip.clip_type {
                                                    audio_proxy.send(GuiCommand::UpdateClipBounds {
                                                        clip_id: clip.id,
                                                        position_secs: ticks_to_secs(clip.start_tick),
                                                        duration_secs: ticks_to_secs(clip.duration_ticks),
                                                        offset_secs: ticks_to_secs(sample_offset_ticks),
                                                    });
                                                }
                                            }
                                        }

                                        // Selección & Delete
                                        if resp_body.clicked_by(PointerButton::Primary) {
                                            if shift {
                                                if is_selected {
                                                    state.selected_clips.retain(|&id| id != clip.id);
                                                } else {
                                                    state.selected_clips.push(clip.id);
                                                }
                                            } else {
                                                state.selected_clips = vec![clip.id];
                                            }
                                        }

                                        if resp_body.secondary_clicked() {
                                            clip_to_delete = Some(clip.id);
                                        }

                                        // Dibujado del Clip
                                        let (border_color, border_width) = if is_selected {
                                            (Color32::from_rgb(255, 200, 0), 2.0_f32)
                                        } else if resp_body.dragged() || resp_left.dragged() || resp_right.dragged() {
                                            (Color32::from_rgb(0, 255, 255), 2.0_f32)
                                        } else {
                                            (Color32::from_white_alpha(100), 1.0_f32)
                                        };

                                        painter.rect_filled(clip_rect, 2.0, clip.color);
                                        painter.rect_stroke(clip_rect, 2.0, Stroke::new(border_width, border_color));

                                        if resp_left.hovered() || resp_left.dragged() {
                                            painter.rect_filled(left_handle_rect, 0.0, Color32::from_white_alpha(80));
                                        }
                                        if resp_right.hovered() || resp_right.dragged() {
                                            painter.rect_filled(right_handle_rect, 0.0, Color32::from_white_alpha(80));
                                        }

                                        // Waveform
                                        if let ClipType::Audio { peaks, sample_offset_ticks, total_sample_ticks, .. } = &clip.clip_type {
                                            if !peaks.is_empty() && *total_sample_ticks > 0 {
                                                let inner_rect = clip_rect.shrink2(Vec2::new(2.0, 4.0));
                                                let center_y = inner_rect.center().y;
                                                let wave_color = Color32::from_rgba_unmultiplied(255, 255, 255, 180);

                                                let step_x = 2.0_f32;
                                                let total_render_steps = (inner_rect.width() / step_x) as usize;

                                                let start_ratio = *sample_offset_ticks as f32 / *total_sample_ticks as f32;
                                                let duration_ratio = clip.duration_ticks as f32 / *total_sample_ticks as f32;

                                                for i in 0..total_render_steps {
                                                    let x = inner_rect.min.x + (i as f32 * step_x);
                                                    if x >= inner_rect.max.x { break; }

                                                    let local_norm = i as f32 / total_render_steps as f32;
                                                    let sample_norm = start_ratio + (local_norm * duration_ratio);

                                                    let peak_idx = (sample_norm * peaks.len() as f32) as usize;
                                                    if let Some(&peak_val) = peaks.get(peak_idx) {
                                                        let bar_height = (inner_rect.height() * 0.8) * peak_val;
                                                        if bar_height > 0.5 {
                                                            painter.line_segment(
                                                                [
                                                                    Pos2::new(x, center_y - bar_height * 0.5),
                                                                    Pos2::new(x, center_y + bar_height * 0.5),
                                                                ],
                                                                Stroke::new(1.0_f32, wave_color),
                                                            );
                                                        }
                                                    }
                                                }
                                            }
                                        }

                                        painter.text(
                                            clip_rect.min + Vec2::new(8.0, 2.0),
                                            Align2::LEFT_TOP,
                                            &clip.name,
                                            egui::FontId::proportional(10.0),
                                            Color32::WHITE
                                        );
                                    }
                                }

                                current_y += total_row_step;
                            }

                            if let Some(id_del) = clip_to_delete {
                                state.clips.retain(|(_, c)| c.id != id_del);
                            }

                            if !pending_slices.is_empty() {
                                state.clips.extend(pending_slices);
                            }

                            let playhead_x = rect.min.x + ticks_to_px(state.playhead_tick, zoom_x);
                            painter.line_segment(
                                [Pos2::new(playhead_x, rect.min.y), Pos2::new(playhead_x, rect.max.y)],
                                Stroke::new(2.0_f32, Color32::from_rgb(0, 255, 255))
                            );

                            if response.dragged() || response.clicked() {
                                if let Some(pointer_pos) = response.interact_pointer_pos() {
                                    let rel_x = (pointer_pos.x - rect.min.x).max(0.0);
                                    let clicked_ticks = px_to_ticks(rel_x, zoom_x);
                                    *current_bar = (clicked_ticks as f32 / ticks_per_bar as f32) + 1.0;

                                    if mode.drives_engine() {
                                        let target_samples = ((*current_bar - 1.0) as f64 * samples_per_bar) as u64;
                                        audio_proxy.send(GuiCommand::Seek { sample_count: target_samples });
                                    }
                                    // En ViewMode::ClipEditor el "playhead" es local al MatrixClip
                                    // (posición dentro del contenedor); no debe mover el transporte
                                    // global. La audición in-place queda pendiente de hikaru_sequencer.
                                }
                            }
                            
                            // DROP DE SAMPLES DESDE EL EXPLORADOR
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
                                                    
                                                    let (duration_ticks, peaks) = load_sample_info(&path, state.ppqn, bpm);
                                                    eprintln!("[PLAYLIST DEBUG] state.ppqn={} bpm={} -> duration_ticks={}", state.ppqn, bpm, duration_ticks);

                                                    let clip = PlaylistClip {
                                                        id: state.next_clip_id,
                                                        name: file_name,
                                                        start_tick: drop_tick,
                                                        duration_ticks,
                                                        clip_type: ClipType::Audio { 
                                                            sample_path: sample_path_str.clone(),
                                                            peaks,
                                                            sample_offset_ticks: 0,
                                                            total_sample_ticks: duration_ticks,
                                                        },
                                                        color: Color32::from_rgb(32, 95, 145),
                                                    };

                                                    let created_clip_id = state.next_clip_id;
                                                    state.next_clip_id += 1;
                                                    state.clips.push((target_track_id, clip));

                                                    if mode.drives_engine() {
                                                        let position_secs = ticks_to_secs(drop_tick);
                                                        let duration_secs = ticks_to_secs(duration_ticks);

                                                        // Se manda UN solo comando directo al audio engine
                                                        audio_proxy.send(GuiCommand::LoadClip {
                                                            clip_id: created_clip_id,
                                                            path: sample_path_str,
                                                            position_secs,
                                                            duration_secs,
                                                            offset_secs: 0.0,
                                                            track_index: target_track_id,
                                                        });
                                                    }
                                                    // TODO(FASE 4 - hikaru_sequencer): cuando exista el
                                                    // scheduler de Clip Launcher, este sub-clip local debe
                                                    // registrarse ahí para reproducirse al disparar el pad
                                                    // de la Session Matrix.
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