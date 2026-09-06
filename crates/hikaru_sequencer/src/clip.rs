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
    /// Longitud real del audio fuente en samples (frames mono).
    /// DEBE fijarse desde la duración real del archivo decodificado
    /// (samples / canales); NUNCA hardcodearse a 1 compás
    /// (ej. `1 * ppqn * 4` ticks). 0 = desconocida (sin loop por longitud).
    pub total_frames: u64,
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
            total_frames: 0,
        }
    }

    /// Fija la longitud real del audio fuente (frames = samples / canales).
    /// Llamar al decodificar/cargar el sample en el engine. Clampea además
    /// una región de loop previa que exceda la longitud real.
    pub fn set_source_length_frames(&mut self, frames: u64) {
        self.total_frames = frames;
        if self.has_valid_clip_loop() && self.loop_end > frames {
            self.loop_end = frames;
            if self.loop_end <= self.loop_start {
                self.loop_start = 0;
                self.loop_end = 0;
            }
        }
    }

    /// Longitud efectiva de reproducción del clip en samples:
    /// región de loop individual si es válida, si no la duración real del
    /// audio. Nunca un valor hardcodeado en compases: si se desconoce la
    /// duración real (`total_frames == 0`) devuelve 0.
    pub fn loop_length_frames(&self) -> u64 {
        if self.has_valid_clip_loop() {
            self.loop_end - self.loop_start
        } else {
            self.total_frames
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_length_is_dynamic_not_one_bar() {
        let mut clip = Clip::new(1, 7);
        // Sin duración real conocida: 0, nunca 1 compás hardcodeado.
        assert_eq!(clip.loop_length_frames(), 0);
        // Longitud real del audio decodificado (ej. 3.2s @ 48kHz mono).
        clip.set_source_length_frames(153_600);
        assert_eq!(clip.loop_length_frames(), 153_600);
        // Con loop individual válido: la región manda.
        clip.set_loop_points(10_000, 100_000);
        assert_eq!(clip.loop_length_frames(), 90_000);
    }

    #[test]
    fn loop_clamped_to_real_length() {
        let mut clip = Clip::new(2, 7);
        clip.set_source_length_frames(50_000);
        clip.set_loop_points(10_000, 90_000);
        // El loop excede lo real → se clampa al fijar la duración.
        clip.set_source_length_frames(50_000);
        assert!(clip.loop_end <= 50_000);
    }
}