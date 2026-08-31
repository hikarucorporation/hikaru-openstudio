// Copyright (C) Hikaru Corporation - 2026
// Miyu's Audio Engine
// GNU Affero General Public License v3
// crates/hikaru_audio_engine/src/lib.rs

use hikaru_core::{AudioBuffer, SampleRate};
use hikaru_transport::{TransportPosition, TransportPlaybackState};
use hikaru_sequencer::TrackMatrix;
use hikaru_dsp::synth::wavetable::WavetableOscillator;
use hikaru_dsp::effects::filter::StateVariableFilter;

pub struct AudioClipInstance {
    pub samples: Vec<f32>,
    pub start_frame: u64,
    pub channels: usize,
}

pub struct AudioEngine<'a> {
    pub transport: TransportPosition,
    pub matrix: TrackMatrix,
    pub main_filter: StateVariableFilter,
    pub oscillator: WavetableOscillator<'a>,
    pub sample_rate: f32,
    pub clips: Vec<AudioClipInstance>,
}

impl<'a> AudioEngine<'a> {
    pub fn new(sr: SampleRate, wavetable: &'a [f32]) -> Self {
        let mut filter = StateVariableFilter::new();
        filter.set_params(2000.0, 0.707, sr.get());

        Self {
            transport: TransportPosition::new(sr, 128.0),
            matrix: TrackMatrix::new(),
            main_filter: filter,
            oscillator: WavetableOscillator::new(wavetable, sr),
            sample_rate: sr.get(),
            clips: Vec::new(),
        }
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

    pub fn add_clip(&mut self, samples: Vec<f32>, start_secs: f32, channels: usize) {
        let start_frame = (start_secs * self.sample_rate) as u64;
        self.clips.push(AudioClipInstance {
            samples,
            start_frame,
            channels: channels.max(1),
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

    pub fn process(&mut self, out_buffer: &mut AudioBuffer<'_>) {
        let samples = out_buffer.get_samples_mut();
        let num_channels = 2; // Estéreo

        // 1. Limpieza de buffer de salida
        samples.fill(0.0);

        if self.transport.playback_state != TransportPlaybackState::Playing {
            return;
        }

        let buffer_frames = (samples.len() / num_channels) as u64;
        let frame_start = self.transport.sample_count;
        let frame_end = frame_start + buffer_frames;

        // 2. Mezcla polifónica de todos los clips
        for clip in &self.clips {
            let clip_frames = (clip.samples.len() / clip.channels) as u64;
            let clip_frame_end = clip.start_frame + clip_frames;

            // Evaluamos si este clip entra en el rango del buffer actual
            if frame_start < clip_frame_end && frame_end > clip.start_frame {
                let start_f = if clip.start_frame > frame_start {
                    clip.start_frame - frame_start
                } else {
                    0
                };

                let end_f = if clip_frame_end < frame_end {
                    clip_frame_end - frame_start
                } else {
                    buffer_frames
                };

                for f in start_f..end_f {
                    let global_frame = frame_start + f;
                    let clip_frame_offset = (global_frame - clip.start_frame) as usize;

                    let out_l_idx = (f as usize) * num_channels;
                    let out_r_idx = out_l_idx + 1;

                    let clip_sample_l = clip_frame_offset * clip.channels;
                    let clip_sample_r = if clip.channels > 1 {
                        clip_sample_l + 1
                    } else {
                        clip_sample_l
                    };

                    if let Some(&val_l) = clip.samples.get(clip_sample_l) {
                        samples[out_l_idx] += val_l;
                    }
                    if let Some(&val_r) = clip.samples.get(clip_sample_r) {
                        samples[out_r_idx] += val_r;
                    }
                }
            }
        }

        // 3. Avanzamos el contador del transport del Engine
        self.transport.sample_count += buffer_frames;
    }
}