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
            clip.duration_frames = (duration_secs * self.sample_rate) as u64;
            clip.sample_offset = (offset_secs * self.sample_rate) as usize * clip.channels;
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
        let duration_frames = (duration_secs * self.sample_rate) as u64;
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

    pub fn seek(&mut self, position_secs: f32) {
        self.transport.sample_count = (position_secs * self.sample_rate) as u64;
    }

    pub fn trigger_clip(&mut self, track_index: usize, scene_index: usize) {
        for clip in self.clips.iter_mut().filter(|c| c.track_index == track_index) {
            clip.is_playing = clip.scene_index == scene_index;
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
        for clip in &self.clips {
            let clip_frame_end = clip.start_frame + clip.duration_frames;

            if self.mode == EngineMode::OpenStudio {
                if frame_start < clip_frame_end && frame_end > clip.start_frame {
                    let start_f = if clip.start_frame > frame_start {
                        (clip.start_frame - frame_start) as usize
                    } else {
                        0
                    };
                    let end_f = if clip_frame_end < frame_end {
                        (clip_frame_end - frame_start) as usize
                    } else {
                        buffer_frames as usize
                    };

                    for f in start_f..end_f {
                        let global_frame = frame_start + f as u64;
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
                }
            } else {
                if !clip.is_playing {
                    continue;
                }

                let clip_len_frames = (clip.samples.len() / clip.channels) as u64;
                if clip_len_frames > 0 {
                    for f in 0..buffer_frames as usize {
                        let global_frame = frame_start + f as u64;
                        let relative_frame = (global_frame % clip_len_frames) as usize;

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
        }

        self.transport.sample_count += buffer_frames;
        self.position_clock.store(self.transport.sample_count, Ordering::Relaxed);
    }
}