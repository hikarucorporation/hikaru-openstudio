/*
 * Hikaru OpenStudio - Internal File Explorer & Sample Preview
 * License: AGPL-3.0-or-later
 */

use egui::{Ui, ScrollArea, Color32, RichText, Pos2, Vec2, Stroke};
use std::fs;
use std::path::PathBuf;
use crate::audio_proxy::{AudioProxy, GuiCommand};

pub struct FileExplorerState {
    pub current_path: PathBuf,
    pub path_input: String,
    pub selected_file: Option<PathBuf>,
    pub history_back: Vec<PathBuf>,
    pub history_forward: Vec<PathBuf>,
    pub preview_volume: f32,
    pub preview_position: f32,
    pub is_playing_preview: bool,
    
    // Controles de Sync y Tempo
    pub sample_bpm: f32,
    pub is_synced: bool,

    // Cache de Waveform
    pub cached_path: Option<PathBuf>,
    pub cached_waveform: Vec<f32>,
    pub sample_duration_secs: f32,
}

impl Default for FileExplorerState {
    fn default() -> Self {
        let root = PathBuf::from("/");
        Self {
            current_path: root.clone(),
            path_input: root.to_string_lossy().to_string(),
            selected_file: None,
            history_back: Vec::new(),
            history_forward: Vec::new(),
            preview_volume: 0.8,
            preview_position: 0.0,
            is_playing_preview: false,
            sample_bpm: 140.0,
            is_synced: false,
            cached_path: None,
            cached_waveform: Vec::new(),
            sample_duration_secs: 1.0,
        }
    }
}

impl FileExplorerState {
    pub fn navigate_to(&mut self, new_path: PathBuf) {
        if new_path.exists() && new_path.is_dir() {
            self.history_back.push(self.current_path.clone());
            self.history_forward.clear();
            self.current_path = new_path.clone();
            self.path_input = new_path.to_string_lossy().to_string();
        }
    }

    pub fn go_back(&mut self) {
        if let Some(prev) = self.history_back.pop() {
            self.history_forward.push(self.current_path.clone());
            self.current_path = prev.clone();
            self.path_input = prev.to_string_lossy().to_string();
        }
    }

    pub fn go_forward(&mut self) {
        if let Some(next) = self.history_forward.pop() {
            self.history_back.push(self.current_path.clone());
            self.current_path = next.clone();
            self.path_input = next.to_string_lossy().to_string();
        }
    }

    pub fn current_speed(&self, project_bpm: f32) -> f32 {
            if self.is_synced && self.sample_bpm > 0.0 {
                (project_bpm / self.sample_bpm).clamp(0.25, 4.0)
            } else {
                1.0
            }
        
    }

    pub fn load_waveform_peaks(&mut self, path: &PathBuf, target_bins: usize) {
        if self.cached_path.as_ref() == Some(path) && self.cached_waveform.len() == target_bins {
            return;
        }

        let mut peaks = vec![0.0_f32; target_bins];

        if let Ok(reader) = hound::WavReader::open(path) {
            let spec = reader.spec();
            let total_samples = reader.len() as usize;
            
            if spec.sample_rate > 0 {
                self.sample_duration_secs = total_samples as f32 / (spec.sample_rate as f32 * spec.channels as f32);
            }

            if total_samples > 0 {
                let samples_per_bin = (total_samples / target_bins).max(1);
                
                let raw_samples: Vec<f32> = match spec.sample_format {
                    hound::SampleFormat::Int => {
                        let max_val = (1 << (spec.bits_per_sample - 1)) as f32;
                        reader.into_samples::<i32>()
                            .filter_map(|s| s.ok())
                            .map(|s| (s as f32 / max_val).abs())
                            .collect()
                    }
                    hound::SampleFormat::Float => {
                        reader.into_samples::<f32>()
                            .filter_map(|s| s.ok())
                            .map(|s| s.abs())
                            .collect()
                    }
                };

                for bin in 0..target_bins {
                    let start = bin * samples_per_bin;
                    let end = (start + samples_per_bin).min(raw_samples.len());
                    if start < raw_samples.len() {
                        let mut max_peak = 0.0_f32;
                        for &sample in &raw_samples[start..end] {
                            if sample > max_peak {
                                max_peak = sample;
                            }
                        }
                        peaks[bin] = max_peak;
                    }
                }
            }
        }

        self.cached_waveform = peaks;
        self.cached_path = Some(path.clone());
    }
}

pub fn show(
    ui: &mut Ui, 
    state: &mut FileExplorerState, 
    dragged_sample: &mut Option<PathBuf>,
    audio_proxy: &AudioProxy,
    project_bpm: f32,
) {
    if dragged_sample.is_some() && ui.input(|i| !i.pointer.any_down()) {
        *dragged_sample = None;
    }

    // Animación de avance del Playhead según duración real y factor de Sync
    if state.is_playing_preview {
        let dt = ui.input(|i| i.stable_dt);
        let speed_factor = if state.is_synced && state.sample_bpm > 0.0 {
            project_bpm / state.sample_bpm
        } else {
            1.0
        };

        let duration = state.sample_duration_secs.max(0.1);
        state.preview_position += (dt / duration) * speed_factor;

        if state.preview_position >= 1.0 {
            state.is_playing_preview = false;
            state.preview_position = 0.0;
        }
        ui.ctx().request_repaint();
    }

    ui.vertical(|ui| {
        // --- BARRA DE NAVEGACIÓN ---
        ui.horizontal(|ui| {
            if ui.add_enabled(!state.history_back.is_empty(), egui::Button::new("⮜")).clicked() {
                state.go_back();
            }
            if ui.add_enabled(!state.history_forward.is_empty(), egui::Button::new("⮞")).clicked() {
                state.go_forward();
            }
            if ui.button("⬆").on_hover_text("Carpeta Padre").clicked() {
                if let Some(parent) = state.current_path.parent() {
                    let parent_buf = parent.to_path_buf();
                    state.navigate_to(parent_buf);
                }
            }

            let response = ui.add_sized(
                [ui.available_width(), 20.0],
                egui::TextEdit::singleline(&mut state.path_input).hint_text("Ruta de entrada..."),
            );

            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                let new_path = PathBuf::from(&state.path_input);
                state.navigate_to(new_path);
            }
        });

        ui.add_space(4.0);
        ui.separator();

        // --- ÁREA DE EXPLORACIÓN DE ARCHIVOS ---
        let available_height = ui.available_height() - 120.0;
        ScrollArea::vertical()
            .max_height(available_height)
            .show(ui, |ui| {
                if let Ok(entries) = fs::read_dir(&state.current_path) {
                    let mut dirs = Vec::new();
                    let mut files = Vec::new();

                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() {
                            dirs.push(path);
                        } else if is_audio_file(&path) {
                            files.push(path);
                        }
                    }

                    dirs.sort();
                    files.sort();

                    for dir in dirs {
                        let dir_name = dir.file_name().unwrap_or_default().to_string_lossy();
                        if ui.selectable_label(false, format!("📁 {}", dir_name)).clicked() {
                            state.navigate_to(dir);
                        }
                    }

                    for file in files {
                        let file_name = file.file_name().unwrap_or_default().to_string_lossy();
                        let is_selected = state.selected_file.as_ref() == Some(&file);

                        let label_text = RichText::new(format!("🎵 {}", file_name))
                            .color(if is_selected { Color32::from_rgb(0, 220, 255) } else { Color32::WHITE });

                        let response = ui.selectable_label(is_selected, label_text);

                        if response.clicked() {
                            state.selected_file = Some(file.clone());
                            state.is_playing_preview = true;
                            state.preview_position = 0.0;

                            // Tratar de auto-detectar BPM desde el nombre del archivo (ej. "Loop_140BPM.wav")
                            if let Some(detected_bpm) = parse_bpm_from_filename(&file_name) {
                                state.sample_bpm = detected_bpm;
                            }

                            audio_proxy.send(GuiCommand::PreviewSample {
                                path: file.to_string_lossy().to_string(),
                                volume: state.preview_volume,
                                speed: state.current_speed(project_bpm),
                            });
                        }

                        if response.drag_started() {
                            *dragged_sample = Some(file.clone());
                            if state.is_playing_preview {
                                state.is_playing_preview = false;
                                audio_proxy.send(GuiCommand::StopPreview);
                            }
                        }
                    }
                } else {
                    ui.label(RichText::new("⚠️ Permiso denegado o directorio no válido.").color(Color32::RED).small());
                }
            });

        ui.separator();

        // --- PANEL DE PREVIEW CON CONTROLES DE BPM/SYNC ---
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                let play_icon = if state.is_playing_preview { "⏸" } else { "▶" };
                if ui.button(play_icon).clicked() {
                    state.is_playing_preview = !state.is_playing_preview;
                    if let Some(ref file) = state.selected_file {
                        if state.is_playing_preview {
                            audio_proxy.send(GuiCommand::PreviewSample {
                                path: file.to_string_lossy().to_string(),
                                volume: state.preview_volume,
                                speed: state.current_speed(project_bpm),
                            });
                        } else {
                            audio_proxy.send(GuiCommand::StopPreview);
                        }
                    }
                }

                ui.label(RichText::new("🔊").small());
                if ui.add_sized([50.0, 16.0], egui::Slider::new(&mut state.preview_volume, 0.0..=1.0).show_value(false)).changed() {
                    if state.is_playing_preview {
                        audio_proxy.send(GuiCommand::SetPreviewVolume(state.preview_volume));
                    }
                }

                ui.separator();

                // Botón Toggle Sync
                let sync_color = if state.is_synced { Color32::from_rgb(0, 220, 255) } else { Color32::GRAY };
                if ui.add(egui::Button::new(RichText::new("Sync").small().color(sync_color))).clicked() {
                    state.is_synced = !state.is_synced;
                }

                // Caja numérica de BPM para el sample
                ui.add_sized(
                    [45.0, 18.0],
                    egui::DragValue::new(&mut state.sample_bpm)
                        .speed(1.0)
                        .clamp_range(40.0..=300.0)
                        .suffix(" BPM")
                );
            });

            ui.add_space(2.0);

            // Area Interactiva del Waveform + Playhead
            let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 40.0), egui::Sense::click_and_drag());
            
            ui.painter().rect_filled(rect, 4.0, Color32::from_rgb(20, 22, 26));
            ui.painter().rect_stroke(rect, 4.0, Stroke::new(1.0_f32, Color32::from_rgb(45, 50, 60)));

            if let Some(selected_path) = state.selected_file.clone() {
                let file_name = selected_path.file_name().unwrap_or_default().to_string_lossy().to_string();
                let points_count = (rect.width() / 4.0).max(20.0) as usize;
                state.load_waveform_peaks(&selected_path, points_count);

                let stroke = Stroke::new(1.5_f32, Color32::from_rgb(0, 230, 200));
                let center_y = rect.center().y;
                let step = rect.width() / points_count as f32;

                if !state.cached_waveform.is_empty() {
                    for (i, &amplitude) in state.cached_waveform.iter().enumerate() {
                        let x = rect.min.x + (i as f32 * step);
                        let h = (amplitude * 16.0).max(1.0);
                        ui.painter().line_segment(
                            [Pos2::new(x, center_y - h), Pos2::new(x, center_y + h)],
                            stroke,
                        );
                    }
                }

                // Playhead interactivo
                let playhead_x = rect.min.x + (state.preview_position * rect.width());
                ui.painter().line_segment(
                    [Pos2::new(playhead_x, rect.min.y), Pos2::new(playhead_x, rect.max.y)],
                    Stroke::new(2.0_f32, Color32::from_rgb(255, 80, 80)),
                );

                ui.painter().text(
                    Pos2::new(rect.min.x + 6.0, rect.min.y + 4.0),
                    egui::Align2::LEFT_TOP,
                    &file_name,
                    egui::FontId::proportional(11.0),
                    Color32::WHITE,
                );

                if response.clicked() {
                    if let Some(pointer_pos) = response.interact_pointer_pos() {
                        let norm_x = ((pointer_pos.x - rect.min.x) / rect.width()).clamp(0.0, 1.0);
                        state.preview_position = norm_x;
                        state.is_playing_preview = true;

                        audio_proxy.send(GuiCommand::PreviewSample {
                            path: selected_path.to_string_lossy().to_string(),
                            volume: state.preview_volume,
                            speed: state.current_speed(project_bpm),
                        });
                    }
                }

                if response.drag_started() {
                    *dragged_sample = Some(selected_path.clone());
                    if state.is_playing_preview {
                        state.is_playing_preview = false;
                        audio_proxy.send(GuiCommand::StopPreview);
                    }
                }
            } else {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Ningún sample seleccionado",
                    egui::FontId::proportional(11.0),
                    Color32::GRAY,
                );
            }
        });
    });
}

fn is_audio_file(path: &PathBuf) -> bool {
    if let Some(ext) = path.extension() {
        let ext_str = ext.to_string_lossy().to_lowercase();
        matches!(ext_str.as_str(), "wav" | "flac" | "ogg" | "mp3" | "aiff" | "synth")
    } else {
        false
    }
}

// Función auxiliar para auto-detectar el BPM si está en el nombre (ej. "Loop 140 BPM.wav")
fn parse_bpm_from_filename(name: &str) -> Option<f32> {
    let lower = name.to_lowercase();
    if let Some(bpm_idx) = lower.find("bpm") {
        let sub = &lower[..bpm_idx];
        let parts: Vec<&str> = sub.split(|c: char| !c.is_numeric()).collect();
        if let Some(last_num) = parts.into_iter().filter(|s| !s.is_empty()).last() {
            return last_num.parse::<f32>().ok();
        }
    }
    None
}