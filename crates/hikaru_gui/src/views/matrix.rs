// Copyright (C) Hikaru Corporation - 2026
// GNU Affero General Public License v3
// Bitwig-style Clip Launcher (OpenLive Dynamic Matrix)
// crates/hikaru_gui/src/views/matrix.rs

use egui::{
    Align2, Button, Color32, CursorIcon, Grid, Frame, ScrollArea, Sense, Stroke, Ui, Vec2
};
use std::path::PathBuf;
use crate::audio_proxy::{AudioProxy, GuiCommand};
use crate::views::mixer::Track;
use crate::views::playlist::{self, PlaylistState};

#[derive(Clone, Debug, PartialEq)]
pub enum SlotState {
    Empty,
    Stopped,
    QueuedToPlay,
    Playing,
    QueuedToStop,
}

#[derive(Clone, Debug)]
pub struct MatrixClip {
    pub id: usize,
    pub name: String,
    pub path: PathBuf,
    pub duration_secs: f64,
    pub local_state: PlaylistState,
    pub local_track: Track,
    pub local_bar: f32,
    /// Punto de loop inicial del clip individual (en ticks).
    /// Lo determina el loopeo visual de la regla (Shift+Drag) sobre el
    /// clip seleccionado. Independiente del transporte global.
    pub loop_start: u64,
    /// Punto de loop final del clip individual (en ticks).
    pub loop_end: u64,
    /// Si el loop individual del clip está habilitado.
    pub loop_enabled: bool,
}

impl MatrixClip {
    /// Longitud del loop individual en ticks (0 si es inválido).
    pub fn loop_length_ticks(&self) -> u64 {
        self.loop_end.saturating_sub(self.loop_start)
    }

    /// Indica si el clip tiene región de loop individual válida.
    pub fn has_valid_clip_loop(&self) -> bool {
        self.loop_enabled && self.loop_end > self.loop_start
    }
}

#[derive(Clone, Debug)]
pub struct MatrixSlot {
    pub state: SlotState,
    pub clip: Option<MatrixClip>,
}

impl Default for MatrixSlot {
    fn default() -> Self {
        Self {
            state: SlotState::Empty,
            clip: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TrackMeta {
    pub name: String,
    pub muted: bool,
    pub soloed: bool,
}

#[derive(Clone, Debug)]
pub struct SceneMeta {
    pub name: String,
}

pub struct SessionMatrixState {
    pub grid: Vec<Vec<MatrixSlot>>,
    pub tracks: Vec<TrackMeta>,
    pub scenes: Vec<SceneMeta>,
    pub next_clip_id: usize,
    pub selected_slot: Option<(usize, usize)>,
    pub editor_height: f32,
    pub editor_zoom_x: f32,
}

impl Default for SessionMatrixState {
    fn default() -> Self {
        let initial_tracks = 8;
        let initial_scenes = 8;

        let tracks = (0..initial_tracks)
            .map(|i| TrackMeta {
                name: format!("Audio {}", i + 1),
                muted: false,
                soloed: false,
            })
            .collect();

        let scenes = (0..initial_scenes)
            .map(|i| SceneMeta {
                name: format!("Scene {}", i + 1),
            })
            .collect();

        let grid = vec![vec![MatrixSlot::default(); initial_scenes]; initial_tracks];

        Self {
            grid,
            tracks,
            scenes,
            next_clip_id: 1,
            selected_slot: None,
            editor_height: 220.0,
            editor_zoom_x: 0.04,
        }
    }
}

impl SessionMatrixState {
    pub fn add_track(&mut self) {
        let track_num = self.tracks.len() + 1;
        self.tracks.push(TrackMeta {
            name: format!("Audio {}", track_num),
            muted: false,
            soloed: false,
        });

        let scene_count = self.scenes.len();
        self.grid.push(vec![MatrixSlot::default(); scene_count]);
    }

    pub fn remove_track(&mut self) {
        if self.tracks.len() > 1 {
            self.tracks.pop();
            self.grid.pop();

            if let Some((t, _)) = self.selected_slot {
                if t >= self.tracks.len() {
                    self.selected_slot = None;
                }
            }
        }
    }

    pub fn add_scene(&mut self) {
        let scene_num = self.scenes.len() + 1;
        self.scenes.push(SceneMeta {
            name: format!("Scene {}", scene_num),
        });

        for track_row in &mut self.grid {
            track_row.push(MatrixSlot::default());
        }
    }

    pub fn remove_scene(&mut self) {
        if self.scenes.len() > 1 {
            self.scenes.pop();
            for track_row in &mut self.grid {
                track_row.pop();
            }

            if let Some((_, s)) = self.selected_slot {
                if s >= self.scenes.len() {
                    self.selected_slot = None;
                }
            }
        }
    }
}

pub fn show(
    ui: &mut Ui,
    state: &mut SessionMatrixState,
    dragged_sample: &mut Option<PathBuf>,
    audio_proxy: &AudioProxy,
    _current_tick: u64,
    _ppqn: u64,
    bpm: f64,
    sample_rate: u32,
    transport_sample_count: u64,
) {
    ui.horizontal(|ui| {
        ui.heading("SESSION MATRIX"); // LOCO, no cambiés esto por nada en el mundo[cite: 11]
        ui.add_space(20.0);

        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Pistas (Tracks): {}", state.tracks.len()));
                if ui.button("➕").on_hover_text("Añadir Pista").clicked() {
                    state.add_track();
                }
                if ui.button("➖").on_hover_text("Quitar Pista").clicked() {
                    state.remove_track();
                }
            });
        });

        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Escenas (Scenes): {}", state.scenes.len()));
                if ui.button("➕").on_hover_text("Añadir Escena").clicked() {
                    state.add_scene();
                }
                if ui.button("➖").on_hover_text("Quitar Escena").clicked() {
                    state.remove_scene();
                }
            });
        });
    });

    // Si hay algún clip sonando en la matriz, mantener la UI actualizando
    let any_clip_playing = state.grid.iter().flatten()
        .any(|slot| slot.state == SlotState::Playing);
    if any_clip_playing {
        ui.ctx().request_repaint();
    }

    ui.add_space(6.0);

    ui.vertical(|ui| {
        let remaining_height = ui.available_height() - state.editor_height - 10.0;

        ui.allocate_ui(Vec2::new(ui.available_width(), remaining_height.max(100.0)), |ui| {
            ScrollArea::both().show(ui, |ui| {
                Grid::new("bitwig_clip_launcher_grid")
                    .spacing(Vec2::new(4.0, 4.0))
                    .show(ui, |ui| {
                        ui.label("");

                        for scene_idx in 0..state.scenes.len() {
                            let scene_name = &state.scenes[scene_idx].name;
                            let scene_btn = Button::new(format!("▶ {}", scene_name))
                                .fill(Color32::from_rgb(45, 45, 55))
                                .min_size(Vec2::new(110.0, 28.0));

                            if ui.add(scene_btn).clicked() {
                                trigger_scene(state, audio_proxy, scene_idx);
                            }
                        }

                        if ui.button("➕").on_hover_text("Añadir nueva Escena").clicked() {
                            state.add_scene();
                        }

                        ui.end_row();

                        for track_idx in 0..state.tracks.len() {
                            ui.group(|ui| {
                                ui.set_min_size(Vec2::new(130.0, 42.0));
                                ui.horizontal(|ui| {
                                    ui.label(&state.tracks[track_idx].name);
                                    let mute_btn = if state.tracks[track_idx].muted { "M!" } else { "M" };
                                    if ui.small_button(mute_btn).clicked() {
                                        state.tracks[track_idx].muted = !state.tracks[track_idx].muted;
                                    }

                                    let solo_btn = if state.tracks[track_idx].soloed { "S!" } else { "S" };
                                    if ui.small_button(solo_btn).clicked() {
                                        state.tracks[track_idx].soloed = !state.tracks[track_idx].soloed;
                                    }
                                });
                            });

                            for scene_idx in 0..state.scenes.len() {
                                render_pad(ui, state, dragged_sample, audio_proxy, track_idx, scene_idx, bpm);
                            }

                            ui.end_row();
                        }

                        if ui.button("➕ Añadir Pista").clicked() {
                            state.add_track();
                        }
                        ui.end_row();
                    });
            });
        });

        let (resizer_rect, resizer_response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), 6.0),
            Sense::drag(),
        );

        if resizer_response.hovered() || resizer_response.dragged() {
            ui.output_mut(|o| o.cursor_icon = CursorIcon::ResizeVertical);
        }

        if resizer_response.dragged() {
            let delta_y = resizer_response.drag_delta().y;
            state.editor_height = (state.editor_height - delta_y).clamp(80.0, 500.0);
        }

        ui.painter().rect_filled(resizer_rect, 0.0, Color32::from_gray(35));
        if resizer_response.hovered() || resizer_response.dragged() {
            ui.painter().rect_filled(resizer_rect, 0.0, Color32::from_rgb(0, 255, 255));
        }

        ui.allocate_ui(Vec2::new(ui.available_width(), state.editor_height), |ui| {
            render_clip_editor_track_view(ui, state, dragged_sample, audio_proxy, bpm, sample_rate, transport_sample_count);
        });
    });
}

fn render_pad(
    ui: &mut Ui,
    state: &mut SessionMatrixState,
    dragged_sample: &mut Option<PathBuf>,
    audio_proxy: &AudioProxy,
    track_idx: usize,
    scene_idx: usize,
    bpm: f64,
) {
    let slot = &state.grid[track_idx][scene_idx];
    let is_selected = state.selected_slot == Some((track_idx, scene_idx));

    let (bg_color, mut border_color, text) = match &slot.state {
        SlotState::Empty => (Color32::from_gray(25), Color32::from_gray(40), "".to_string()),
        SlotState::Stopped => (
            Color32::from_rgb(45, 55, 75),
            Color32::from_rgb(90, 130, 190),
            slot.clip.as_ref().map(|c| c.name.clone()).unwrap_or_default(),
        ),
        SlotState::QueuedToPlay => (
            Color32::from_rgb(120, 100, 30),
            Color32::YELLOW,
            format!("⌛ {}", slot.clip.as_ref().map(|c| &c.name).unwrap_or(&"".into())),
        ),
        SlotState::Playing => (
            Color32::from_rgb(35, 135, 60),
            Color32::GREEN,
            format!("▶ {}", slot.clip.as_ref().map(|c| &c.name).unwrap_or(&"".into())),
        ),
        SlotState::QueuedToStop => (
            Color32::from_rgb(130, 45, 45),
            Color32::RED,
            "⏹ Stop".to_string(),
        ),
    };

    if is_selected {
        border_color = Color32::WHITE;
    }

    let size = Vec2::new(110.0, 42.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    if ui.is_rect_visible(rect) {
        ui.painter().rect_filled(rect, 3.0, bg_color);
        let stroke_width = if is_selected { 2.0_f32 } else { 1.0_f32 };
        ui.painter().rect_stroke(rect, 3.0, Stroke::new(stroke_width, border_color));

        if !text.is_empty() {
            let display_text = if text.len() > 14 {
                format!("{}...", &text[..11])
            } else {
                text
            };

            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                display_text,
                egui::FontId::proportional(11.0),
                Color32::WHITE,
            );
        }
    }

    if response.clicked() {
        state.selected_slot = Some((track_idx, scene_idx));
        trigger_pad(state, audio_proxy, track_idx, scene_idx);
    }

    if ui.rect_contains_pointer(rect) {
        ui.output_mut(|o| o.cursor_icon = CursorIcon::PointingHand);

        if ui.input(|i| i.pointer.any_released()) {
            if let Some(sample_path) = dragged_sample.take() {
                if crate::views::explorer::is_audio_file(&sample_path) {
                    state.selected_slot = Some((track_idx, scene_idx));
                    load_clip_into_slot(state, audio_proxy, track_idx, scene_idx, sample_path, bpm);
                }
            }
        }

        let dropped_files = ui.input(|i| i.raw.dropped_files.clone());
        if let Some(file) = dropped_files.first() {
            if let Some(path) = &file.path {
                if crate::views::explorer::is_audio_file(path) {
                    state.selected_slot = Some((track_idx, scene_idx));
                    load_clip_into_slot(state, audio_proxy, track_idx, scene_idx, path.clone(), bpm);
                }
            }
        }
    }
}

fn render_clip_editor_track_view(
    ui: &mut Ui,
    state: &mut SessionMatrixState,
    dragged_sample: &mut Option<PathBuf>,
    audio_proxy: &AudioProxy,
    bpm: f64,
    sample_rate: u32,
    transport_sample_count: u64,
) {
    // Calculamos la posición del playhead desde el reloj del transporte (position_clock compartido)
    // en lugar de incrementar local_bar manualmente frame-a-frame.
    if let Some((track_idx, scene_idx)) = state.selected_slot {
        if let Some(slot) = state.grid.get_mut(track_idx).and_then(|r| r.get_mut(scene_idx)) {
            if slot.state == SlotState::Playing {
                let sr = sample_rate as f64;
                if sr > 0.0 && bpm > 0.0 {
                    let seconds = transport_sample_count as f64 / sr;
                    let beats = seconds * (bpm / 60.0);
                    let bars = beats / 4.0;
                    // local_bar es 1-based (1.0 = inicio del bar 1)
                    slot.clip.as_mut().unwrap().local_bar = bars as f32 + 1.0;
                    // Mantener el clip editor redibujando continuamente
                    ui.ctx().request_repaint();
                }
            }
        }
    }

    Frame::none()
        .fill(Color32::from_rgb(20, 20, 24))
        .stroke(Stroke::new(1.0_f32, Color32::from_gray(45)))
        .show(ui, |ui| {
            let Some((track_idx, scene_idx)) = state.selected_slot else {
                ui.centered_and_justified(|ui| {
                    ui.label("Seleccioná un clip de la matriz para desplegar su Track Editor.");
                });
                return;
            };

            if track_idx >= state.grid.len() || scene_idx >= state.grid[track_idx].len() {
                return;
            }

            if state.grid[track_idx][scene_idx].clip.is_none() {
                ui.horizontal(|ui| {
                    ui.heading("CLIP TRACK EDITOR");
                    ui.label(format!(
                        "- {} | {}",
                        state.tracks[track_idx].name, state.scenes[scene_idx].name
                    ));
                });
                ui.separator();
                ui.centered_and_justified(|ui| {
                    ui.label("Slot vacío. Arrastrá un sample para crear un Clip.");
                });
                return;
            }

            let title = format!(
                "CLIP TRACK EDITOR - {} | {}",
                state.tracks[track_idx].name, state.scenes[scene_idx].name
            );

            let clip = state.grid[track_idx][scene_idx].clip.as_mut().unwrap();
            let mut local_tracks = vec![clip.local_track.clone()];

            // Mismo handler de dibujado y selección de rango con Shift+Drag
            // que el Arranger: la regla del Clip Track Editor define el loop
            // individual del clip seleccionado.
            playlist::show_embedded(
                ui,
                &mut clip.local_state,
                &mut local_tracks,
                &mut clip.local_bar,
                dragged_sample,
                audio_proxy,
                bpm,
                sample_rate,
                &title,
                transport_sample_count,
                4,
            );

            if let Some(updated_track) = local_tracks.into_iter().next() {
                clip.local_track = updated_track;
            }

            // El loopeo visual de la regla determina los puntos de loop del
            // clip actual (`clip.loop_start` / `clip.loop_end`), permitiendo
            // que el clip individual loopee de forma independiente al
            // transporte global.
            // Los límites se envían al engine ÚNICAMENTE al terminar el
            // arrastre (`drag_stopped`, vía
            // `loop_drag_completed_this_frame`), nunca en cada frame de
            // renderizado GUI (evita SetClipLoop continuo).
            {
                // Solo propagar al engine cuando terminó el drag.
                if clip.local_state.loop_drag_completed_this_frame {
                    let ppqn = clip.local_state.ppqn.max(1) as f64;
                    let (new_start, new_end, new_enabled) = if clip.local_state.is_loop_region_valid() {
                        (
                            clip.local_state.loop_start_ticks,
                            clip.local_state.loop_end_ticks,
                            true,
                        )
                    } else {
                        (clip.loop_start, clip.loop_end, false)
                    };

                if new_start != clip.loop_start
                    || new_end != clip.loop_end
                    || new_enabled != clip.loop_enabled
                {
                    clip.loop_start = new_start;
                    clip.loop_end = new_end;
                    clip.loop_enabled = new_enabled;

                    let seconds_per_tick = if bpm > 0.0 {
                        (60.0 / bpm) / ppqn
                    } else {
                        0.0
                    };
                    audio_proxy.send(GuiCommand::SetClipLoop {
                        track_idx,
                        scene_idx,
                        loop_start_secs: (new_start as f64 * seconds_per_tick) as f32,
                        loop_end_secs: (new_end as f64 * seconds_per_tick) as f32,
                        enabled: new_enabled,
                    });
                }
                }
            }
        });
}

fn load_clip_into_slot(
    state: &mut SessionMatrixState,
    audio_proxy: &AudioProxy,
    track_idx: usize,
    scene_idx: usize,
    path: PathBuf,
    bpm: f64,
) {
    let name = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let new_id = state.next_clip_id;
    state.next_clip_id += 1;

    let path_str = path.to_string_lossy().to_string();

    let mut local_state = PlaylistState::default();
    local_state.zoom_x = state.editor_zoom_x;

    let sub_clip_id = local_state.next_clip_id;
    local_state.next_clip_id += 1;
    let initial_sub_clip = playlist::build_audio_clip(
        sub_clip_id,
        name.clone(),
        &path,
        0,
        local_state.ppqn,
        bpm,
        Color32::from_rgb(32, 95, 145),
    );
    local_state.clips.push((0, initial_sub_clip));

    state.grid[track_idx][scene_idx] = MatrixSlot {
        state: SlotState::Stopped,
        clip: Some(MatrixClip {
            id: new_id,
            name: name.clone(),
            path,
            duration_secs: 0.0,
            local_state,
            local_track: Track::new(0, name, false),
            local_bar: 1.0,
            loop_start: 0,
            loop_end: 0,
            loop_enabled: false,
        }),
    };

    audio_proxy.send(GuiCommand::LoadClip {
        clip_id: new_id,
        path: path_str,
        position_secs: 0.0,
        duration_secs: 0.0,
        offset_secs: 0.0,
        track_index: track_idx,
        scene_index: scene_idx,
    });
}

fn trigger_pad(state: &mut SessionMatrixState, audio_proxy: &AudioProxy, track_idx: usize, scene_idx: usize) {
    if state.grid[track_idx][scene_idx].clip.is_none() {
        return;
    }

    let current_state = state.grid[track_idx][scene_idx].state.clone();

    match current_state {
        SlotState::Stopped => {
            // Apagamos visualmente cualquier otro clip que estuviera sonando EN LA MISMA PISTA
            for s in 0..state.scenes.len() {
                if s != scene_idx && state.grid[track_idx][s].state == SlotState::Playing {
                    state.grid[track_idx][s].state = SlotState::Stopped;
                }
            }
            state.grid[track_idx][scene_idx].state = SlotState::Playing;

            // ÚNICAMENTE disparamos el evento de este pad
            audio_proxy.send(GuiCommand::TriggerClip {
                track_idx,
                scene_idx,
            });
        }
        SlotState::Playing => {
            state.grid[track_idx][scene_idx].state = SlotState::Stopped;

            // Al volver a tocarlo, enviamos la orden para detener ese pad puntual
            audio_proxy.send(GuiCommand::TriggerClip {
                track_idx,
                scene_idx,
            });
        }
        _ => {}
    }
}

fn trigger_scene(state: &mut SessionMatrixState, audio_proxy: &AudioProxy, scene_idx: usize) {
    for track_idx in 0..state.tracks.len() {
        if state.grid[track_idx][scene_idx].clip.is_some() {
            // Detenemos los otros clips de las pistas correspondientes y marcamos como playing
            for s in 0..state.scenes.len() {
                if s != scene_idx && state.grid[track_idx][s].state == SlotState::Playing {
                    state.grid[track_idx][s].state = SlotState::Stopped;
                }
            }
            state.grid[track_idx][scene_idx].state = SlotState::Playing;

            audio_proxy.send(GuiCommand::TriggerClip {
                track_idx,
                scene_idx,
            });
        }
    }

    // Enviamos el disparo masivo de la escena sin forzar Seek/Play global
    audio_proxy.send(GuiCommand::TriggerScene { scene_idx });
}