// crates/hikaru_plugin_host/src/clap/hikaru_clap_launcher.rs

use crate::PluginInstance;
use hikaru_core::AudioBuffer;
use std::path::Path;

pub struct ClapInstance {
    name: String,
    // Instancia de clack_host::instance::PluginInstance
}

impl ClapInstance {
    pub fn load(path: &Path) -> Result<Self, String> {
        // Lógica de carga CLAP:
        // 1. Cargar .so
        // 2. Obtener la factory
        // 3. Crear instancia
        Ok(Self {
            name: path.file_name().unwrap().to_string_lossy().into(),
        })
    }
}

impl PluginInstance for ClapInstance {
    fn process(&mut self, _buffer: &mut AudioBuffer) {
        // Procesamiento CLAP con clack-host
    }

    fn show_gui(&mut self, _handle: raw_window_handle::RawWindowHandle) {
        // CLAP tiene una extensión de GUI muy limpia
    }

    fn hide_gui(&mut self) { /* ... */ }
    fn get_name(&self) -> &str { &self.name }
}