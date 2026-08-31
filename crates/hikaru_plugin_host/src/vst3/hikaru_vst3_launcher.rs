// crates/hikaru_plugin_host/src/vst3/hikaru_vst3_launcher.rs

use crate::PluginInstance;
use hikaru_core::AudioBuffer;
use libloading::Library;
use std::path::Path;

pub struct Vst3Instance {
    _lib: Library,
    name: String,
    // Aquí irían los punteros a la interfaz IComponent e IAudioProcessor de VST3
}

impl Vst3Instance {
    pub fn load(path: &Path) -> Result<Self, String> {
        unsafe {
            let lib = Library::new(path).map_err(|e| e.to_string())?;
            // Lógica de Init VST3...
            Ok(Self {
                _lib: lib,
                name: path.file_name().unwrap().to_string_lossy().into(),
            })
        }
    }
}

impl PluginInstance for Vst3Instance {
    fn process(&mut self, _buffer: &mut AudioBuffer) {
        // Mapear AudioBuffer a los buffers de VST3 y llamar a processor.process()
        // ¡CERO ALLOCATIONS AQUÍ!
    }

    fn show_gui(&mut self, _handle: raw_window_handle::RawWindowHandle) {
        // Embedding de la ventana usando el handle de winit
    }

    fn hide_gui(&mut self) { /* ... */ }
    fn get_name(&self) -> &str { &self.name }
}