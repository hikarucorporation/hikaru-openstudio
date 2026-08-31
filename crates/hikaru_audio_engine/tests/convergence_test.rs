// crates/hikaru_audio_engine/tests/convergence_test.rs

use hikaru_audio_engine::AudioEngine; // <--- ¡Importación correcta para integración!
use hikaru_core::{SampleRate, AudioBuffer};
use hikaru_transport::TransportPlaybackState;

#[test]
fn test_engine_convergence() {
    let sr = SampleRate::new(44100.0);
    let mut raw_samples = [0.0; 512];
    let table = [0.0; 2048]; // Tabla de ondas de prueba
    
    // 1. Inicializar Motor
    let mut engine = AudioEngine::new(sr, &table);
    engine.oscillator.set_frequency(440.0);
    
    // 2. Configurar Transporte en Play
    engine.transport.playback_state = TransportPlaybackState::Playing;
    
    // 3. Crear un AudioBuffer real
    let mut buffer = AudioBuffer::new(&mut raw_samples);
    
    // 4. Procesar!
    engine.process(&mut buffer);
    
    // 5. Verificar que el transporte avanzó 512 muestras
    assert_eq!(engine.transport.sample_count, 512);
    
    println!("¡Convergencia total confirmada! Hikaru OpenStudio está vivo! 😍✨");
}
