# ESTRUCTURA.md

# ARQUITECTURA Y ESPECIFICACIÓN TÉCNICA: HIKARU OPENSTUDIO

## 1. MANDATOS DIRECTOS DE DESARROLLO
- **Lenguaje:** 100% Rust idiomático. Prohibido usar bindings a C/C++ salvo en librerías de bajo nivel estrictamente necesarias.
- **Estrategia de Desarrollo:** **UI First / Mockups Funcionales**. La interfaz debe ser visualmente atractiva, fluida y profesional ANTES de conectar la lógica pesada del backend. Si la UI no te dan ganas de manosearla, no sirve.
- **Diseño Visual (GUI):** Estética nivel DAW comercial (Bitwig, Ableton, FL Studio) con tema oscuro, alto contraste y renderizado acelerado por hardware via GPU (`egui` + `eframe`).
- **Layout Rígido por Paneles:** Para evitar colapsos y desposicionamiento de widgets, la interfaz debe estructurarse mediante:
  - `egui::TopBottomPanel::top()`: Header bar (Transporte, Modos, BPM).
  - `egui::SidePanel::right()`: Mixer multipista y DSP Rack.
  - `egui::CentralPanel::default()`: Vista principal (Matriz de Clips en OPENLIVE / Timeline en OPENSTUDIO).
  - `egui::TopBottomPanel::bottom()`: Footer de monitoreo (CPU, RAM, Estado).

## 2. ARQUITECTURA DEL WORKSPACE (CRATES)

hikaru_workspace/
├── crates/
│   ├── hikaru_gui/          # [PRIORIDAD ACTUAL] Interfaz visual interactiva, mockups, paneles egui.
│   ├── hikaru_core/         # Tipos base, utilidades de audio, SampleRate, AudioBuffer f32.
│   ├── hikaru_transport/    # Reloj sample-accurate, BPM, cuantización y estado de reproducción.
│   ├── hikaru_audio_engine/ # Motor real-time lock-free, ruteo de canales y bus Master.
│   ├── hikaru_dsp/          # Sintetizador Wavetable/Espectral nativo, filtros y efectos DSP.
│   ├── hikaru_sequencer/    # Secuenciador, piano roll, arreglador y matriz de clips.
│   └── hikaru_plugin_host/  # Host nativo para plugins VST3 y CLAP.

## 3. FLUJOS DE COMUNICACIÓN (MOCKUP -> BACKEND)

  ┌──────────────────┐     Lock-Free RingBuffer (Events)     ┌──────────────────────┐
  │    hikaru_gui    │ ────────────────────────────────────> │ hikaru_audio_engine  │
  │ (egui / Layout)  │ <──────────────────────────────────── │    (Audio Thread)    │
  └──────────────────┘       Atomic State Updates / Metering └──────────────────────┘

## 4. MODO LIVE PERFORMANCE (HIKARU OPENLIVE)
Integrado dentro de Hikaru OpenStudio (activable via atajo o pestaña en la UI).
- **Matriz de Clips (Session View):** Rejilla de clips de Audio/MIDI organizados por canales (columnas) y escenas (filas).
- **Interacción:** Mapeo MIDI externo, botones de disparo con retroalimentación visual inmediata en `egui`.