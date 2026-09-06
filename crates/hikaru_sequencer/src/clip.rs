// crates/hikaru_sequencer/src/clip.rs

use std::sync::atomic::{AtomicU8, Ordering};

/// Estados posibles de un clip en la matriz.
/// Representados como u8 para manipulación atómica rápida.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClipState {
    Stopped = 0,
    Queued = 1,   // Esperando al próximo pulso de cuantización
    Playing = 2,
    Stopping = 3, // Seguirá sonando hasta el final del ciclo de cuantización
}

impl From<u8> for ClipState {
    fn from(value: u8) -> Self {
        match value {
            1 => ClipState::Queued,
            2 => ClipState::Playing,
            3 => ClipState::Stopping,
            _ => ClipState::Stopped,
        }
    }
}

/// Modos de disparo del clip (Trigger Modes)
#[derive(Debug, Clone, Copy)]
pub enum TriggerMode {
    Trigger, // Se dispara y sigue hasta el final o hasta que se frene
    Toggle,  // Un clic arranca, otro clic frena (respetando cuantización)
    Repeat,  // Mientras se mantenga apretado (Gate)
    Legato,  // Cambia entre clips manteniendo la posición de transporte
}

pub struct Clip {
    pub id: u32,
    pub audio_buffer_id: u32, // Referencia al buffer precargado en hikaru_core
    pub state: AtomicU8,
    pub trigger_mode: TriggerMode,
    pub start_sample: u64,    // Cuándo empezó a sonar realmente
    pub loop_enabled: bool,
    /// Punto de loop inicial del clip individual (en samples).
    /// Independiente del transporte global.
    pub loop_start: u64,
    /// Punto de loop final del clip individual (en samples).
    /// Si `loop_end <= loop_start`, el clip loopea completo.
    pub loop_end: u64,
}

impl Clip {
    pub fn new(id: u32, audio_buffer_id: u32) -> Self {
        Self {
            id,
            audio_buffer_id,
            state: AtomicU8::new(ClipState::Stopped as u8),
            trigger_mode: TriggerMode::Trigger,
            start_sample: 0,
            loop_enabled: true,
            loop_start: 0,
            loop_end: 0,
        }
    }

    /// Establece los puntos de loop del clip individual garantizando
    /// una ventana válida mínima (si `end <= start`, loopea completo).
    pub fn set_loop_points(&mut self, start: u64, end: u64) {
        if end <= start {
            self.loop_start = 0;
            self.loop_end = 0;
        } else {
            self.loop_start = start;
            self.loop_end = end;
        }
    }

    /// Indica si el clip tiene una región de loop individual válida.
    pub fn has_valid_clip_loop(&self) -> bool {
        self.loop_enabled && self.loop_end > self.loop_start
    }

    pub fn get_state(&self) -> ClipState {
        ClipState::from(self.state.load(Ordering::Relaxed))
    }

    pub fn set_state(&self, new_state: ClipState) {
        self.state.store(new_state as u8, Ordering::Release);
    }
}