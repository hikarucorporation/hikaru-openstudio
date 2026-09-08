// Copyright (C) Hikaru Corporation - 2026
// GNU Affero General Public License v3
// Hikaru OpenStudio - Código fuente del Header
// crates/hikaru_gui/src/views/header.rs

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use egui::{Button, Color32, Frame, Margin, RichText, Separator, Slider, Ui, Vec2};
use crate::app::AppMode;

use crate::audio_proxy::{AudioClipData, AudioProxy, GuiCommand};
use crate::views::playlist::{ClipType, PlaylistState};

use hikaru_transport::transport_state::{TransportPlaybackState, TransportPosition};

// --- HELPER: CONVIERTE BEATS/COMPASES A TIMECODE (HH:MM:SS.ms) ---
fn format_timecode(bars: f32, bpm: f64) -> String {
    let beats = (bars - 1.0).max(0.0) * 4.0;
    let total_seconds = (beats / bpm as f32) * 60.0;

    let hours = (total_seconds / 3600.0) as u32;
    let minutes = ((total_seconds % 3600.0) / 60.0) as u32;
    let seconds = (total_seconds % 60.0) as u32;
    let millis = (total_seconds.fract() * 1000.0) as u32;

    format!("{:02}:{:02}:{:02}.{:03}", hours, minutes, seconds, millis)
}

pub fn show(
    ui: &mut Ui, 
    transport: &mut TransportPosition,
    position_clock: &Arc<AtomicU64>,
    is_looping: &mut bool,
    is_recording: &mut bool,
    mode: &mut AppMode, 
    show_mixer: &mut bool, 
    show_dsp_rack: &mut bool,
    show_explorer: &mut bool,
    playlist_state: &PlaylistState,
    audio_proxy: &AudioProxy,
) {
    // Siempre leer la posición real del reloj del engine para que el
    // timecode y el playhead se actualicen tanto en OpenStudio como en OpenLive.
    transport.sample_count = position_clock.load(Ordering::Relaxed);

    // 1. CALCULAMOS LA BARRA ACTUAL CON EL RELEVO REAL DEL AUDIO
    let samples_per_beat = (transport.sample_rate.get() as f64 * 60.0) / transport.bpm;
    let samples_per_bar = samples_per_beat * transport.beats_per_bar as f64;
    
    let mut current_bar = 1.0 + (transport.sample_count as f64 / samples_per_bar) as f32;
    let old_bar = current_bar;

    Frame::none()
        .inner_margin(Margin::symmetric(10.0, 8.0)) 
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;

                let btn_size = Vec2::new(34.0, 32.0);

                // --- BOTONES DE TRANSPORTE ---
                // Reiniciar al inicio (Barra 1)
                if ui.add_sized(btn_size, Button::new(RichText::new("⏮").size(16.0))).clicked() {
                    transport.sample_count = 0;
                    audio_proxy.send(GuiCommand::Seek { sample_count: 0 });
                }

                // Retroceder 4 compases
                if ui.add_sized(btn_size, Button::new(RichText::new("⏪").size(14.0))).clicked() {
                    let samples_to_sub = (samples_per_bar * 4.0) as u64;
                    transport.sample_count = transport.sample_count.saturating_sub(samples_to_sub);
                    audio_proxy.send(GuiCommand::Seek { sample_count: transport.sample_count });
                }

                // PLAY
                let is_playing = transport.playback_state == TransportPlaybackState::Playing;
                let play_bg = if is_playing { Color32::from_rgb(40, 180, 80) } else { Color32::from_gray(45) };
                let play_txt = if is_playing { Color32::BLACK } else { Color32::WHITE };
                if ui.add_sized(btn_size, Button::new(RichText::new("▶").size(16.0).color(play_txt)).fill(play_bg)).clicked() {
                    transport.playback_state = TransportPlaybackState::Playing;

                    // FUGA #1: detener el preview del explorer ANTES del Play
                    // global para que su buffer no se mezcle con el Master
                    // Mixer durante la reproducción del timeline. El engine
                    // también hace `stop()` en `play()` + gate en `process()`;
                    // este `StopPreview` es la orden sincrónica GUI -> motor.
                    audio_proxy.send(GuiCommand::StopPreview);
                    // FUGA #2: sincronizar BPM de forma SINCRÓNICA antes de
                    // convertir ticks->secs y antes del Seek/Play. Sin esto el
                    // engine seguía con el tempo viejo mientras la GUI ya
                    // calculaba `clip_length` con el nuevo (ej. clip 135 BPM
                    // vs proyecto 120 BPM): los compases visuales del Clip
                    // Editor no coincidían con la parada real del engine.
                    audio_proxy.send(GuiCommand::SetBpm(transport.bpm as f32));

                    match *mode {
                        AppMode::OpenStudio => {
                            // En Studio sincronizamos los clips lineales del timeline.
                            // Longitud en frames (`clip_length_frames` del engine)
                            // derivada del tempo REAL del transporte vía
                            // `ticks_to_samples`: compases visuales == parada
                            // del engine, sin deriva f32 manual.
                            let sr = transport.sample_rate.get().max(1.0);
                            let clips_to_sync: Vec<AudioClipData> = playlist_state
                                .clips
                                .iter()
                                .filter_map(|(track_id, clip)| {
                                    if let ClipType::Audio { sample_path, sample_offset_ticks, .. } = &clip.clip_type {
                                        // `clip_length_frames` usa BPM activo + SR
                                        // real + PPQN único: idéntico al que el
                                        // engine usará tras el `SetBpm` de arriba.
                                        // Compases visuales == parada del engine.
                                        let clip_length_frames = transport.clip_length_frames(clip.duration_ticks);
                                        let duration_secs = clip_length_frames as f32 / sr;
                                        Some(AudioClipData {
                                            clip_id: clip.id,
                                            path: sample_path.clone(),
                                            start_secs: transport.ticks_to_samples(clip.start_tick) as f32 / sr,
                                            duration_secs,
                                            offset_secs: transport.ticks_to_samples(*sample_offset_ticks) as f32 / sr,
                                            track_index: *track_id,
                                        })
                                    } else {
                                        None
                                    }
                                })
                                .collect();

                            audio_proxy.send(GuiCommand::SyncPlaylistClips { clips: clips_to_sync });
                        }
                        AppMode::OpenLive => {
                            // En Live no enviamos clips de la playlist ni previews del explorer.
                            // Simplemente nos aseguramos de que el motor corra en modo OpenLive (false).
                            audio_proxy.send(GuiCommand::SetAppMode(false));
                        }
                    }

                    audio_proxy.send(GuiCommand::Seek { sample_count: transport.sample_count });
                    audio_proxy.send(GuiCommand::Play);
                }

                // PAUSE
                let is_paused = transport.playback_state == TransportPlaybackState::Paused;
                let pause_bg = if is_paused { Color32::from_rgb(230, 160, 20) } else { Color32::from_gray(45) };
                let pause_txt = if is_paused { Color32::BLACK } else { Color32::WHITE };
                if ui.add_sized(btn_size, Button::new(RichText::new("⏸").size(16.0).color(pause_txt)).fill(pause_bg)).clicked() {
                    if is_playing {
                        transport.playback_state = TransportPlaybackState::Paused;
                        audio_proxy.send(GuiCommand::Pause);
                    } else if is_paused {
                        transport.playback_state = TransportPlaybackState::Playing;
                        // Reanudar también silencia el preview y re-afirma el
                        // BPM real antes del Play (mismas fugas #1/#2).
                        audio_proxy.send(GuiCommand::StopPreview);
                        audio_proxy.send(GuiCommand::SetBpm(transport.bpm as f32));
                        audio_proxy.send(GuiCommand::Play);
                    }
                }

                // STOP
                if ui.add_sized(btn_size, Button::new(RichText::new("⏹").size(16.0))).clicked() {
                    transport.playback_state = TransportPlaybackState::Stopped;
                    transport.sample_count = 0;
                    audio_proxy.send(GuiCommand::Stop);
                }

                // RECORD
                let rec_bg = if *is_recording { Color32::from_rgb(220, 30, 30) } else { Color32::from_gray(45) };
                let rec_txt = if *is_recording { Color32::WHITE } else { Color32::from_rgb(200, 60, 60) };
                if ui.add_sized(btn_size, Button::new(RichText::new("⏺").size(16.0).color(rec_txt)).fill(rec_bg)).clicked() {
                    *is_recording = !*is_recording;
                }

                // BOTÓN LOOP (🔁)
                // Estado solo en GUI (`is_looping`); la guarda ppqn previa al
                // envío vive en playlist.rs/app.rs. No se escribe campo alguno
                // en el transporte/DSP desde aquí (anti-congelamiento).
                let loop_bg = if *is_looping { Color32::from_rgb(0, 180, 220) } else { Color32::from_gray(45) };
                let loop_txt = if *is_looping { Color32::BLACK } else { Color32::WHITE };
                if ui.add_sized(btn_size, Button::new(RichText::new("🔁").size(14.0).color(loop_txt)).fill(loop_bg)).clicked() {
                    *is_looping = !*is_looping;
                }

                // Avanzar 4 compases
                if ui.add_sized(btn_size, Button::new(RichText::new("⏩").size(14.0))).clicked() {
                    transport.sample_count += (samples_per_bar * 4.0) as u64;
                    audio_proxy.send(GuiCommand::Seek { sample_count: transport.sample_count });
                }

                // Avanzar 16 compases
                if ui.add_sized(btn_size, Button::new(RichText::new("⏭").size(16.0))).clicked() {
                    transport.sample_count += (samples_per_bar * 16.0) as u64;
                    audio_proxy.send(GuiCommand::Seek { sample_count: transport.sample_count });
                }

                ui.add_space(8.0);

                // DISPLAY DE TIEMPO / TIMECODE
                let time_code = format_timecode(current_bar, transport.bpm);
                Frame::none()
                    .fill(Color32::from_rgb(15, 18, 22))
                    .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(45, 55, 65)))
                    .inner_margin(Margin::symmetric(8.0, 5.0))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(time_code)
                                .font(egui::FontId::monospace(13.0))
                                .color(Color32::from_rgb(0, 220, 255))
                                .strong(),
                        );
                    });

                // SLIDER DE BÚSQUEDA (SCRUBBER)
                if ui.add(
                    Slider::new(&mut current_bar, 1.0..=128.0)
                        .show_value(false)
                        .trailing_fill(true)
                ).changed() {
                    let bar_delta = current_bar - old_bar;
                    let samples_delta = (bar_delta as f64 * samples_per_bar) as i64;
                    
                    if samples_delta < 0 {
                        transport.sample_count = transport.sample_count.saturating_sub(samples_delta.unsigned_abs());
                    } else {
                        transport.sample_count += samples_delta as u64;
                    }
                    audio_proxy.send(GuiCommand::Seek { sample_count: transport.sample_count });
                }

                ui.add_space(10.0);
                ui.add(Separator::default().vertical());

                // Selector de Modo con intercepción de cambio
                if ui.selectable_label(*mode == AppMode::OpenLive, "OPENLIVE").clicked() {
                    if *mode != AppMode::OpenLive {
                        *mode = AppMode::OpenLive;
                        transport.playback_state = TransportPlaybackState::Stopped;
                        transport.sample_count = 0;
                        audio_proxy.send(GuiCommand::Stop);
                        audio_proxy.send(GuiCommand::SetAppMode(false)); // false = OpenLive
                    }
                }

                if ui.selectable_label(*mode == AppMode::OpenStudio, "OPENSTUDIO").clicked() {
                    if *mode != AppMode::OpenStudio {
                        *mode = AppMode::OpenStudio;
                        transport.playback_state = TransportPlaybackState::Stopped;
                        transport.sample_count = 0;
                        audio_proxy.send(GuiCommand::Stop);
                        audio_proxy.send(GuiCommand::SetAppMode(true)); // true = OpenStudio
                    }
                }

                // BPM del transport
                ui.label(RichText::new("BPM").strong());
                ui.add(egui::DragValue::new(&mut transport.bpm).speed(1.0).clamp_range(40.0..=300.0));

                ui.add_space(10.0);
                ui.add(Separator::default().vertical());
                ui.add_space(10.0);

                // Ventanas
                ui.toggle_value(show_mixer, "MIXER (F9)");
                ui.toggle_value(show_dsp_rack, "DSP RACK (F10)");
                ui.toggle_value(show_explorer, "📁 EXPLORER (F11)");
            });
        });
}