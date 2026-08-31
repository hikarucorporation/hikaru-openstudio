// crates/hikaru_plugin_host/src/lib.rs

pub mod vst3;
pub mod clap;

use hikaru_core::AudioBuffer;
use raw_window_handle::RawWindowHandle;

// Re-exportamos para que el motor de audio los use fácil
pub use vst3::Vst3Instance;
pub use clap::ClapInstance;

/// Trait unificado para cualquier instancia de plugin (VST3 o CLAP)
pub trait PluginInstance {
    fn process(&mut self, buffer: &mut AudioBuffer);
    fn show_gui(&mut self, window_handle: RawWindowHandle);
    fn hide_gui(&mut self);
    fn get_name(&self) -> &str;
}

/// El Host encargado de gestionar el escaneo y carga de plugins
pub struct PluginHost;

impl PluginHost {
    /// Simula el escaneo de plugins en las rutas estándar de Linux
    pub fn scan_plugins() {
        println!("Buscando plugins en:");
        println!(" - ~/.vst3");
        println!(" - /usr/lib/vst3");
        println!(" - ~/.clap");
        println!("¡Escaneo completado!");
    }
}