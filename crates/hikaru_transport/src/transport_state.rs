// crates/hikaru_transport/src/transport_state.rs

use hikaru_core::SampleRate;

/// Resolución única del secuenciador: ticks por negra (quarter note).
/// ÚNICA fuente de verdad del PPQN. La GUI (playlist/app) NO debe
/// hardcodear 960: debe usar `DEFAULT_PPQN` / `transport.ppqn()`.
pub const DEFAULT_PPQN: u64 = 960;

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

    /// PPQN activo del transporte (constante única `DEFAULT_PPQN`).
    /// La GUI debe llamar a esto en lugar de hardcodear 960.
    pub fn ppqn(&self) -> u64 {
        DEFAULT_PPQN
    }

    /// Calcula la duración de un compás en samples (f64 para precisión).
    pub fn samples_per_bar(&self) -> f64 {
        self.samples_per_beat() * self.beats_per_bar as f64
    }

    /// Samples por negra con el BPM activo y el Sample Rate real.
    pub fn samples_per_beat(&self) -> f64 {
        let bpm = self.bpm.max(1.0);
        let sr = self.sample_rate.get() as f64;
        if sr <= 0.0 {
            return 0.0;
        }
        (sr * 60.0) / bpm
    }

    /// Segundos por tick con el BPM activo (guarda bpm <= 0).
    pub fn seconds_per_tick(&self) -> f64 {
        let bpm = self.bpm.max(1.0);
        (60.0 / bpm) / self.ppqn() as f64
    }

    /// Ticks -> samples con BPM activo + SR real + PPQN único.
    pub fn ticks_to_samples(&self, ticks: u64) -> u64 {
        let sr = self.sample_rate.get() as f64;
        if sr <= 0.0 {
            return 0;
        }
        (ticks as f64 * self.seconds_per_tick() * sr).round() as u64
    }

    /// Samples -> ticks con BPM activo + SR real + PPQN único.
    /// Inversa exacta de `ticks_to_samples` (salvo redondeo).
    pub fn samples_to_ticks(&self, samples: u64) -> u64 {
        let sr = self.sample_rate.get() as f64;
        if sr <= 0.0 {
            return 0;
        }
        let seconds = samples as f64 / sr;
        let spt = self.seconds_per_tick();
        if spt <= 0.0 {
            return 0;
        }
        (seconds / spt).round() as u64
    }
    
    // Getter para BPM si lo necesitás como f64
    pub fn get_bpm(&self) -> f64 {
        self.bpm
    }
}