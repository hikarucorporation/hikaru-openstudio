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

/// Un `MatrixClip` es ahora un contenedor de Playlist en miniatura: su
/// contenido (`local_state.clips`) son `playlist::PlaylistClip` reales
/// (mismo `ClipType::Audio`, mismos peaks, mismo sistema de ticks/PPQN) que
/// los del Arranger principal. Esto elimina la necesidad de `SubAudioClip`
/// y de cualquier lógica de render/slice/drag&drop duplicada: todo eso vive
/// una sola vez en `playlist.rs` y se reutiliza acá vía `playlist::show_embedded`.
#[derive(Clone, Debug)]
pub struct MatrixClip {
    pub id: usize,
    pub name: String,
    pub path: PathBuf,
    pub duration_secs: f64,

    /// Estado de Playlist embebido (ticks locales en PPQN, zoom, selección,
    /// clipboard, etc.) — exactamente el mismo tipo que usa `playlist.rs`.
    pub local_state: PlaylistState,

    /// Pseudo-pista que representa el carril único del Clip Editor. Se
    /// renderiza con el mismo header (nombre, mute, solo, pan, volumen) que
    /// cualquier pista de la Playlist principal. Su `id` es local al
    /// contenedor (no se cruza con los ids reales del Mixer).
    pub local_track: Track,

    /// Posición del cursor dentro del contenedor, en compases (mismo
    /// formato que `current_bar` en `playlist.rs`). Independiente del
    /// transporte global: mover este cursor NUNCA dispara `GuiCommand::Seek`.
    pub local_bar: f32,
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
    /// Matriz dinámica de Slots [track_idx][scene_idx]
    pub grid: Vec<Vec<MatrixSlot>>,
    pub tracks: Vec<TrackMeta>,
    pub scenes: Vec<SceneMeta>,
    pub next_clip_id: usize,
    pub selected_slot: Option<(usize, usize)>, // (track_idx, scene_idx)
    pub editor_height: f32,                     // Control dinámico de altura para el Editor
    /// Zoom horizontal por defecto para el `PlaylistState` local de cada
    /// `MatrixClip` recién creado (cada clip después puede tener el suyo).
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
) {
    // --- BARRA SUPERIOR DE CONTROL DE MATRIZ ---
    ui.horizontal(|ui| {
        ui.heading("CLIP LAUNCHER (OPENLIVE)");
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

    ui.add_space(6.0);

    // Layout principal vertical
    ui.vertical(|ui| {
        // 1. AREA SUPERIOR: MATRIX GRID
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

        // 2. BARRA RESIZER MANUAL (Sin el bug imán de TopBottomPanel)
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

        // 3. AREA INFERIOR: CLIP EDITOR (instancia real de la Playlist)
        ui.allocate_ui(Vec2::new(ui.available_width(), state.editor_height), |ui| {
            render_clip_editor_track_view(ui, state, dragged_sample, audio_proxy, bpm, sample_rate);
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
                if is_audio_file(&sample_path) {
                    state.selected_slot = Some((track_idx, scene_idx));
                    load_clip_into_slot(state, audio_proxy, track_idx, scene_idx, sample_path, bpm);
                }
            }
        }

        let dropped_files = ui.input(|i| i.raw.dropped_files.clone());
        if let Some(file) = dropped_files.first() {
            if let Some(path) = &file.path {
                if is_audio_file(path) {
                    state.selected_slot = Some((track_idx, scene_idx));
                    load_clip_into_slot(state, audio_proxy, track_idx, scene_idx, path.clone(), bpm);
                }
            }
        }
    }
}

/// Clip Editor: ya NO dibuja rectángulos simplificados a mano. En su lugar,
/// arma un `Vec<Track>` de una sola pseudo-pista (`clip.local_track`) y le
/// pasa el `PlaylistState` embebido del `MatrixClip` seleccionado a
/// `playlist::show_embedded`. Esto reutiliza tal cual: header/panel de
/// track, rejilla de ticks/compases, render de waveforms, selección, trim,
/// slice ('S') y drag&drop de samples desde el File Explorer — sin
/// duplicar ni un renglón de esa lógica acá.
fn render_clip_editor_track_view(
    ui: &mut Ui,
    state: &mut SessionMatrixState,
    dragged_sample: &mut Option<PathBuf>,
    audio_proxy: &AudioProxy,
    bpm: f64,
    sample_rate: u32,
) {
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

            // `playlist::show_embedded` espera un `&mut Vec<Track>`: le damos
            // una vista de una sola pista, que es exactamente el header del
            // carril del Clip Editor.
            let mut local_tracks = vec![clip.local_track.clone()];

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
            );

            // `show_embedded` puede haber tocado mute/solo/pan/volumen/nombre
            // del header (misma UI que en la Playlist principal); lo
            // persistimos de vuelta en el `MatrixClip`.
            if let Some(updated_track) = local_tracks.into_iter().next() {
                clip.local_track = updated_track;
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

    // Reutilizamos el mismo cargador de audio (hound + peaks) que usa la
    // Playlist principal, así el sub-clip inicial es un `ClipType::Audio`
    // 100% compatible con `playlist.rs`.
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
    // El id de pseudo-pista local es siempre 0: hay una sola pista dentro
    // del contenedor y no se cruza con los ids reales del Mixer.
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
        }),
    };

    // TODO(FASE 4 - hikaru_sequencer): este LoadClip sigue apuntando al
    // track real del Mixer (track_idx) porque el scheduler de Clip Launcher
    // todavía no existe. Cuando exista, la reproducción de los sub-clips
    // internos del MatrixClip debe pasar por el sequencer, no disparar el
    // motor directamente acá.
    audio_proxy.send(GuiCommand::LoadClip {
        clip_id: new_id,
        path: path_str,
        position_secs: 0.0,
        duration_secs: 0.0,
        offset_secs: 0.0,
        track_index: track_idx,
    });
}

fn trigger_pad(state: &mut SessionMatrixState, audio_proxy: &AudioProxy, track_idx: usize, scene_idx: usize) {
    if state.grid[track_idx][scene_idx].clip.is_none() {
        return;
    }

    let current_state = state.grid[track_idx][scene_idx].state.clone();

    match current_state {
        SlotState::Stopped => {
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
        SlotState::Playing => {
            state.grid[track_idx][scene_idx].state = SlotState::Stopped;

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
            trigger_pad(state, audio_proxy, track_idx, scene_idx);
        }
    }

    audio_proxy.send(GuiCommand::TriggerScene { scene_idx });
}

fn is_audio_file(path: &PathBuf) -> bool {
    path.extension()
        .map(|ext| {
            let ext_str = ext.to_string_lossy().to_lowercase();
            ext_str == "wav" || ext_str == "flac" || ext_str == "ogg" || ext_str == "mp3"
        })
        .unwrap_or(false)
}
