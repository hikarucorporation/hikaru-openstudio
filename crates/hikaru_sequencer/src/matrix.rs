// crates/hikaru_sequencer/src/matrix.rs

use crate::clip::Clip;

pub const MAX_TRACKS: usize = 16;
pub const MAX_SCENES: usize = 32;

pub struct TrackMatrix {
    // Usamos una matriz fija de slots para evitar alocaciones en runtime.
    // Option::None significa slot vacío.
    pub slots: [[Option<Clip>; MAX_SCENES]; MAX_TRACKS],
}

impl TrackMatrix {
    pub fn new() -> Self {
        // Inicialización de matriz vacía
        const EMPTY_SCENE: [Option<Clip>; MAX_SCENES] = [const { None }; MAX_SCENES];
        Self {
            slots: [EMPTY_SCENE; MAX_TRACKS],
        }
    }

    /// Dispara todos los clips de una escena (fila) respetando la cuantización
    pub fn trigger_scene(&self, scene_index: usize) {
        if scene_index >= MAX_SCENES { return; }
        
        for track in 0..MAX_TRACKS {
            if let Some(clip) = &self.slots[track][scene_index] {
                // Acá le mandamos el comando de Queued. 
                // El motor de audio se encargará de pasarlo a Playing usando el Quantizer.
                clip.set_state(crate::clip::ClipState::Queued);
            }
        }
    }
}