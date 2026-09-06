// Copyright (C) Hikaru Corporation - 2026
// Miyu's Audio Engine
// GNU Affero General Public License v3
// crates/hikaru_audio_engine/src/lib.rs

pub mod preview_player;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use hikaru_core::{AudioBuffer, SampleRate};
use hikaru_dsp::effects::filter::StateVariableFilter;
use hikaru_dsp::synth::wavetable::WavetableOscillator;
use hikaru_sequencer::TrackMatrix;
use hikaru_transport::{TransportPlaybackState, TransportPosition};

use crate::preview_player::PreviewPlayer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineMode {
    OpenLive,
    OpenStudio,
}

pub struct AudioClipInstance {
    pub id: usize,
    pub track_index: usize,
    pub scene_index: usize,
    pub samples: Vec<f32>,
    pub start_frame: u64,
    pub duration_frames: u64,
    pub sample_offset: usize,
    pub channels: usize,
    pub is_playing: bool,
    /// Loop individual del clip (OpenLive), en frames mono.
    /// Independiente del loop global del transporte. Si no es válido
    /// (`!has_valid_clip_loop`), el clip loopea su duración real completa,
    /// nunca un largo hardcodeado en compases.
    pub clip_loop_enabled: bool,
    pub clip_loop_start: u64,
    pub clip_loop_end: u64,
}

impl AudioClipInstance {
    /// Longitud real del audio en frames mono (dinámica, del archivo).
    pub fn natural_frames(&self) -> u64 {
        (self.samples.len() / self.channels.max(1)) as u64
    }

    /// Región de loop individual válida y clampeada a la longitud real.
    pub fn has_valid_clip_loop(&self) -> bool {
        self.clip_loop_enabled
            && self.clip_loop_end > self.clip_loop_start
            && self.clip_loop_start < self.natural_frames()
    }

    /// Longitud efectiva de reproducción: loop individual si es válido,
    /// si no la duración real del audio. Nunca 1 compás hardcodeado.
    pub fn playback_length_frames(&self) -> u64 {
        if self.has_valid_clip_loop() {
            let end = self.clip_loop_end.min(self.natural_frames());
            end.saturating_sub(self.clip_loop_start)
        } else {
            self.natural_frames()
        }
    }
}

pub struct AudioEngine<'a> {
    pub transport: TransportPosition,
    pub mode: EngineMode,
    pub matrix: TrackMatrix,
    pub main_filter: StateVariableFilter,
    pub oscillator: WavetableOscillator<'a>,
    pub sample_rate: f32,
    pub clips: Vec<AudioClipInstance>,
    pub preview_player: PreviewPlayer,
    // Referencia compartida del reloj de muestras con la GUI
    pub position_clock: Arc<AtomicU64>,
}

impl<'a> AudioEngine<'a> {
    pub fn new(sr: SampleRate, wavetable: &'a [f32], position_clock: Arc<AtomicU64>) -> Self {
        let mut filter = StateVariableFilter::new();
        filter.set_params(2000.0, 0.707, sr.get());

        Self {
            transport: TransportPosition::new(sr, 128.0),
            mode: EngineMode::OpenStudio,
            matrix: TrackMatrix::new(),
            main_filter: filter,
            oscillator: WavetableOscillator::new(wavetable, sr),
            sample_rate: sr.get(),
            clips: Vec::new(),
            preview_player: PreviewPlayer::new(),
            position_clock,
        }
    }

    pub fn set_mode(&mut self, mode: EngineMode) {
        self.mode = mode;
    }

    pub fn set_sample_rate(&mut self, new_sr: f32) {
        self.sample_rate = new_sr;
        self.transport.sample_rate = hikaru_core::SampleRate::new(new_sr);
        self.main_filter.set_params(2000.0, 0.707, new_sr);
    }

    /// Limpia los clips anteriores y sincroniza la nueva lista que llega desde la GUI
    pub fn sync_clips(&mut self, new_clips: Vec<AudioClipInstance>) {
        self.clips = new_clips;
    }

    pub fn update_clip_bounds(
        &mut self,
        clip_id: usize,
        track_index: usize,
        scene_index: usize,
        start_secs: f32,
        duration_secs: f32,
        offset_secs: f32,
    ) {
        let is_studio = self.mode == EngineMode::OpenStudio;
        if let Some(clip) = self
            .clips
            .iter_mut()
            .find(|c| {
                if is_studio {
                    c.id == clip_id
                } else {
                    c.track_index == track_index && c.scene_index == scene_index
                }
            })
        {
            clip.start_frame = (start_secs * self.sample_rate) as u64;
            // Si la GUI manda duración 0/desconocida (ej. matriz OpenLive al
            // cargar), derivarla de la duración REAL del audio decodificado,
            // nunca de un largo hardcodeado en compases.
            let natural = clip.natural_frames();
            clip.duration_frames = if duration_secs > 0.0 {
                (duration_secs * self.sample_rate) as u64
            } else {
                natural
            };
            clip.sample_offset = (offset_secs * self.sample_rate) as usize * clip.channels;
        }
    }

    /// Puntos de loop individual del clip (OpenLive, en segundos GUI).
    /// Se convierten a frames con el SR real del engine y se clampan a la
    /// longitud real del audio. `enabled == false` o `end <= start`
    /// desactiva el loop individual (el clip loopea completo).
    pub fn set_clip_loop(
        &mut self,
        track_index: usize,
        scene_index: usize,
        start_secs: f32,
        end_secs: f32,
        enabled: bool,
    ) {
        let sr = self.sample_rate.max(1.0);
        if let Some(clip) = self
            .clips
            .iter_mut()
            .find(|c| c.track_index == track_index && c.scene_index == scene_index)
        {
            let natural = clip.natural_frames();
            let start = (start_secs.max(0.0) * sr) as u64;
            let mut end = (end_secs.max(0.0) * sr) as u64;
            if natural > 0 {
                end = end.min(natural);
            }
            if enabled && end > start && start < natural {
                clip.clip_loop_enabled = true;
                clip.clip_loop_start = start;
                clip.clip_loop_end = end;
            } else {
                clip.clip_loop_enabled = false;
                clip.clip_loop_start = 0;
                clip.clip_loop_end = 0;
            }
        }
    }

    pub fn add_clip(
        &mut self,
        id: usize,
        track_index: usize,
        scene_index: usize,
        samples: Vec<f32>,
        start_secs: f32,
        duration_secs: f32,
        offset_secs: f32,
        channels: usize,
    ) {
        let ch = channels.max(1);
        let start_frame = (start_secs * self.sample_rate) as u64;
        // Duración dinámica: si la GUI manda 0 (matriz OpenLive al cargar),
        // usar la longitud REAL del audio decodificado, nunca 1 compás fijo.
        let natural_frames = (samples.len() / ch) as u64;
        let duration_frames = if duration_secs > 0.0 {
            (duration_secs * self.sample_rate) as u64
        } else {
            natural_frames
        };
        let sample_offset = (offset_secs * self.sample_rate) as usize * ch;

        let instance = AudioClipInstance {
            id,
            track_index,
            scene_index,
            samples,
            start_frame,
            duration_frames,
            sample_offset,
            channels: ch,
            is_playing: self.mode == EngineMode::OpenStudio,
            clip_loop_enabled: false,
            clip_loop_start: 0,
            clip_loop_end: 0,
        };

        let is_studio = self.mode == EngineMode::OpenStudio;
        if let Some(existing) = self
            .clips
            .iter_mut()
            .find(|c| {
                if is_studio {
                    c.id == id
                } else {
                    c.track_index == track_index && c.scene_index == scene_index
                }
            })
        {
            *existing = instance;
        } else {
            self.clips.push(instance);
        }
    }

    pub fn remove_clip(&mut self, clip_id: usize, track_index: usize, scene_index: usize) {
        let is_studio = self.mode == EngineMode::OpenStudio;
        self.clips.retain(|c| {
            if is_studio {
                c.id != clip_id
            } else {
                !(c.track_index == track_index && c.scene_index == scene_index)
            }
        });
    }

    pub fn play(&mut self) {
        self.transport.playback_state = TransportPlaybackState::Playing;
    }

    pub fn pause(&mut self) {
        self.transport.playback_state = TransportPlaybackState::Paused;
    }

    pub fn stop(&mut self) {
        self.transport.playback_state = TransportPlaybackState::Stopped;
        self.transport.sample_count = 0;
    }

    /// Configura el loop global del transporte (dueño único del wrap).
    /// `start`/`end` en SAMPLES, convertidos en la GUI con los mismos ticks
    /// de la barra (`ticks_to_samples`), así engine y GUI coinciden exacto.
    /// El wrap se aplica en `process()`; la GUI no reescribe `sample_count`.
    pub fn set_global_loop(&mut self, start_samples: u64, end_samples: u64, enabled: bool) {
        self.transport.set_loop_region_samples(start_samples, end_samples);
        self.transport.set_loop_enabled(enabled);
    }

    pub fn seek(&mut self, position_secs: f32) {
        self.transport.sample_count = (position_secs * self.sample_rate) as u64;
    }

    /// Dispara (o frena, toggle) el clip de un pad.
    /// Si el clip ya estaba sonando se frena; si no, suena en exclusiva en
    /// su pista y su fase arranca en el transporte actual (`start_frame`),
    /// para que el re-disparo reinicie el sample en lugar de enganchar una
    /// fase arbitraria del reloj global.
    pub fn trigger_clip(&mut self, track_index: usize, scene_index: usize) {
        let now = self.transport.sample_count;
        let target_was_playing = self
            .clips
            .iter()
            .find(|c| c.track_index == track_index && c.scene_index == scene_index)
            .map(|c| c.is_playing)
            .unwrap_or(false);
        for clip in self.clips.iter_mut().filter(|c| c.track_index == track_index) {
            if clip.scene_index == scene_index {
                clip.is_playing = !target_was_playing;
                if clip.is_playing {
                    clip.start_frame = now;
                }
            } else {
                clip.is_playing = false;
            }
        }
    }

    /// Dispara una escena completa: en cada pista suena el clip de esa
    /// escena (si existe) y se frenan los demás. Las fases arrancan en el
    /// transporte actual para un ataque en conjunto.
    pub fn trigger_scene(&mut self, scene_index: usize) {
        let now = self.transport.sample_count;
        // Pistas que tienen clip en la escena.
        let mut tracks_with_clip = std::collections::HashSet::new();
        for clip in self.clips.iter() {
            if clip.scene_index == scene_index {
                tracks_with_clip.insert(clip.track_index);
            }
        }
        for clip in self.clips.iter_mut() {
            if clip.scene_index == scene_index {
                clip.is_playing = true;
                clip.start_frame = now;
            } else if tracks_with_clip.contains(&clip.track_index) {
                clip.is_playing = false;
            }
        }
    }

    pub fn stop_track(&mut self, track_index: usize) {
        for clip in self.clips.iter_mut().filter(|c| c.track_index == track_index) {
            clip.is_playing = false;
        }
    }

    pub fn process(&mut self, out_buffer: &mut AudioBuffer<'_>) {
        let samples = out_buffer.get_samples_mut();
        let num_channels = 2; // Estéreo

        samples.fill(0.0);

        // Procesar preview incluso si el transport está detenido
        self.preview_player.process(samples);

        if self.transport.playback_state != TransportPlaybackState::Playing {
            self.position_clock.store(self.transport.sample_count, Ordering::Relaxed);
            return;
        }

        let buffer_frames = (samples.len() / num_channels) as u64;
        let frame_start = self.transport.sample_count;
        let frame_end = frame_start + buffer_frames;

        // MEZCLA MULTIPISTA
        // El wrap del loop global se aplica ACÁ, en el callback de audio,
        // frame a frame vía `transport.wrap_sample_count` (dueño único).
        // Así engine y GUI (que deriva el playhead del `position_clock`)
        // ven exactamente el mismo reloj, sin Seek espurios desde la GUI.
        for clip in &self.clips {
            let clip_frame_end = clip.start_frame + clip.duration_frames;

            if self.mode == EngineMode::OpenStudio {
                for f in 0..buffer_frames as usize {
                    let global_frame = self.transport.wrap_sample_count(frame_start + f as u64);
                    if global_frame >= clip.start_frame && global_frame < clip_frame_end {
                        let relative_frame = (global_frame - clip.start_frame) as usize;
                        let sample_index_frame = (clip.sample_offset / clip.channels) + relative_frame;
                        let clip_sample_l = sample_index_frame * clip.channels;
                        let clip_sample_r = if clip.channels > 1 { clip_sample_l + 1 } else { clip_sample_l };

                        let out_l_idx = f * num_channels;
                        let out_r_idx = out_l_idx + 1;

                        if clip_sample_l < clip.samples.len() {
                            samples[out_l_idx] += clip.samples[clip_sample_l];
                        }
                        if clip.channels > 1 && clip_sample_r < clip.samples.len() {
                            samples[out_r_idx] += clip.samples[clip_sample_r];
                        } else if clip.channels == 1 && clip_sample_l < clip.samples.len() {
                            samples[out_r_idx] += clip.samples[clip_sample_l];
                        }
                    }
                }
            } else {
                if !clip.is_playing {
                    continue;
                }

                // Longitud efectiva dinámica: loop individual si es válido,
                // si no la duración REAL del audio. Nunca 1 compás fijo.
                let natural = clip.natural_frames();
                let loop_valid = clip.has_valid_clip_loop();
                let loop_start = clip.clip_loop_start.min(natural);
                let loop_end = clip.clip_loop_end.min(natural.max(1)).max(loop_start + 1);
                let loop_len = loop_end.saturating_sub(loop_start).max(1);
                if natural == 0 {
                    continue;
                }
                for f in 0..buffer_frames as usize {
                    // Reloj global con wrap del loop del transporte.
                    let pos = self.transport.wrap_sample_count(frame_start + f as u64);
                    // La fase arranca en el disparo (`start_frame`), no en el
                    // cero absoluto del transporte.
                    if pos < clip.start_frame {
                        continue;
                    }
                    let elapsed = pos - clip.start_frame;
                    let relative_frame = if loop_valid {
                        if elapsed < loop_end {
                            elapsed
                        } else {
                            loop_start + ((elapsed - loop_start) % loop_len)
                        }
                    } else {
                        elapsed % natural
                    } as usize;

                    let clip_sample_l = relative_frame * clip.channels;
                    let clip_sample_r = if clip.channels > 1 { clip_sample_l + 1 } else { clip_sample_l };

                    let out_l_idx = f * num_channels;
                    let out_r_idx = out_l_idx + 1;

                    if clip_sample_l < clip.samples.len() {
                        samples[out_l_idx] += clip.samples[clip_sample_l];
                    }
                    if clip.channels > 1 && clip_sample_r < clip.samples.len() {
                        samples[out_r_idx] += clip.samples[clip_sample_r];
                    } else if clip.channels == 1 && clip_sample_l < clip.samples.len() {
                        samples[out_r_idx] += clip.samples[clip_sample_l];
                    }
                }
            }
        }

        // Avance con wrap: el cursor (GUI vía `position_clock`) coincide
        // exactamente con lo que el motor mezcló en este buffer.
        self.transport.sample_count = self.transport.wrap_sample_count(frame_end);
        self.position_clock.store(self.transport.sample_count, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;

    static TABLE: [f32; 2048] = [0.0; 2048];

    fn test_engine() -> AudioEngine<'static> {
        let clock = Arc::new(AtomicU64::new(0));
        AudioEngine::new(SampleRate::new(44100.0), &TABLE, clock)
    }

    fn run_frames(engine: &mut AudioEngine, frames: u64) {
        // Buffer estéreo: 2 samples por frame.
        let mut raw = vec![0.0f32; (frames * 2) as usize];
        let mut buf = AudioBuffer::new(&mut raw);
        engine.process(&mut buf);
    }

    #[test]
    fn openlive_clip_duration_is_real_audio_length() {
        let mut engine = test_engine();
        engine.set_mode(EngineMode::OpenLive);
        // 1s estéreo @44100: 44100 frames. duration_secs = 0 (como manda la
        // matriz al cargar) debe derivarse del audio real, no de 1 compás.
        let samples = vec![0.5f32; 44100 * 2];
        engine.add_clip(1, 0, 0, samples, 0.0, 0.0, 0.0, 2);
        assert_eq!(engine.clips[0].duration_frames, 44100);
        assert_eq!(engine.clips[0].playback_length_frames(), 44100);
    }

    #[test]
    fn trigger_clip_toggles_and_restarts_phase() {
        let mut engine = test_engine();
        engine.set_mode(EngineMode::OpenLive);
        engine.transport.sample_count = 8000;
        engine.add_clip(1, 0, 0, vec![0.5f32; 100 * 2], 0.0, 0.0, 0.0, 2);
        engine.trigger_clip(0, 0);
        assert!(engine.clips[0].is_playing);
        assert_eq!(engine.clips[0].start_frame, 8000);
        // Segundo toque al pad que suena: frena (toggle).
        engine.trigger_clip(0, 0);
        assert!(!engine.clips[0].is_playing);
    }

    #[test]
    fn trigger_scene_plays_whole_row() {
        let mut engine = test_engine();
        engine.set_mode(EngineMode::OpenLive);
        engine.add_clip(1, 0, 0, vec![0.5f32; 100 * 2], 0.0, 0.0, 0.0, 2);
        engine.add_clip(2, 1, 0, vec![0.5f32; 100 * 2], 0.0, 0.0, 0.0, 2);
        engine.add_clip(3, 0, 1, vec![0.5f32; 100 * 2], 0.0, 0.0, 0.0, 2);
        engine.trigger_scene(1);
        assert!(!engine.clips[0].is_playing);
        assert!(!engine.clips[1].is_playing);
        assert!(engine.clips[2].is_playing);
    }

    #[test]
    fn global_loop_wraps_in_engine_not_gui() {
        let mut engine = test_engine();
        engine.play();
        engine.set_global_loop(0, 44100, true);
        // 100 buffers de 512 frames = 51200 > 44100 → wrap a 7100.
        for _ in 0..100 {
            run_frames(&mut engine, 512);
        }
        assert_eq!(engine.transport.sample_count, 51200 - 44100);
        assert_eq!(
            engine.position_clock.load(Ordering::Relaxed),
            engine.transport.sample_count
        );
    }

    #[test]
    fn global_loop_disabled_never_wraps() {
        let mut engine = test_engine();
        engine.play();
        for _ in 0..100 {
            run_frames(&mut engine, 512);
        }
        // Sin loop (OpenLive sin región): el cursor avanza libre, jamás
        // vuelve a 0 en el compás 2.
        assert_eq!(engine.transport.sample_count, 51200);
    }

    #[test]
    fn clip_loop_region_is_honored() {
        let mut engine = test_engine();
        engine.set_mode(EngineMode::OpenLive);
        engine.add_clip(1, 0, 0, vec![0.5f32; 44100 * 2], 0.0, 0.0, 0.0, 2);
        engine.set_clip_loop(0, 0, 0.0, 0.5, true);
        let clip = &engine.clips[0];
        assert!(clip.has_valid_clip_loop());
        assert_eq!(clip.clip_loop_start, 0);
        assert_eq!(clip.clip_loop_end, 22050);
        assert_eq!(clip.playback_length_frames(), 22050);
        // Desactivar: vuelve a loopear el audio completo.
        engine.set_clip_loop(0, 0, 0.0, 0.0, false);
        assert!(!engine.clips[0].has_valid_clip_loop());
        assert_eq!(engine.clips[0].playback_length_frames(), 44100);
    }
}