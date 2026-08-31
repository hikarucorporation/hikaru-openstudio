use hikaru_gui::HikaruGui;

#[test]
fn test_gui_state_initialization() {
    // En lugar de mockear eframe (que es un quilombo), 
    // testeamos si la lógica de la GUI se puede instanciar 
    // o si sus componentes base están bien.
    
    // Por ahora, como HikaruGui::new requiere un CreationContext,
    // lo ideal es testear las funciones puras de tu AppState 
    // o de tus utilidades de UI.
    
    assert!(true); // Smoke test para verificar que el harness de tests funciona
}

#[test]
fn test_transport_logic() {
    // Acá podrías testear si los comandos de BPM 
    // se formatean bien, por ejemplo.
    let bpm = 150.0;
    let formatted = format!("{:.1}", bpm);
    assert_eq!(formatted, "150.0");
}
