// crates/hikaru_transport/src/transport_state.rs

use hikaru_core::SampleRate;

/// Posibles estados de reproducción.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransportPlaybackState {
    Stopped,
    Playing,
    Paused,
}

/// Estado global del transporte de audio.
#[derive(Debug, Clone, Copy)]
pub struct TransportPosition {
    pub sample_rate: SampleRate,
    pub bpm: f64,
    pub beats_per_bar: u32,
    pub beat_division: u32,
    pub sample_count: u64, // Nombre unificado
    pub playback_state: TransportPlaybackState,
}

impl TransportPosition {
    /// Crea un nuevo estado de transporte.
    pub fn new(sample_rate: SampleRate, bpm: f64) -> Self {
        Self {
            sample_rate,
            bpm,
            beats_per_bar: 4,
            beat_division: 4,
            sample_count: 0,
            playback_state: TransportPlaybackState::Stopped,
        }
    }

    /// Actualiza la posición sumando el número de samples procesados.
    pub fn advance(&mut self, samples: u64) {
        if self.playback_state == TransportPlaybackState::Playing {
            self.sample_count += samples;
        }
    }

    /// Reinicia la posición a cero.
    pub fn reset(&mut self) {
        self.sample_count = 0;
    }

    /// Cambia el BPM de forma segura.
    pub fn set_bpm(&mut self, new_bpm: f64) {
        if new_bpm > 0.0 {
            self.bpm = new_bpm;
        }
    }

    /// Calcula la duración de un compás en samples (f64 para precisión).
    pub fn samples_per_bar(&self) -> f64 {
        let samples_per_beat = (60.0 / self.bpm) * self.sample_rate.get() as f64;
        samples_per_beat * self.beats_per_bar as f64
    }
    
    // Getter para BPM si lo necesitás como f64
    pub fn get_bpm(&self) -> f64 {
        self.bpm
    }
}