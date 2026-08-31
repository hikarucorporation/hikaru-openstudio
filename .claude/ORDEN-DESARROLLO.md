# ORDEN-DESARROLLO.md

# MANUAL Y ESPECIFICACIÓN DE ORDEN DE DESARROLLO — HIKARU OPENSTUDIO / OPENLIVE

---

## 1. REGLAS INVARIABLES DE DESARROLLO
1. **LENGUAJE ÚNICO:** 100% Rust idiomático.
2. **FILOSOFÍA UI FIRST:** Primero se maqueta la interfaz completa con componentes interactivos y datos mock/falsos en `egui`. Una vez validada la estética y el feeling, se programa el backend.
3. **ESTRUCTURA RÍGIDA DE EGUI:** Prohibido calcular píxeles a mano para contenedores principales. Usar `TopBottomPanel`, `SidePanel` y `CentralPanel`.
4. **AISLAMIENTO DE TIEMPO REAL:** El motor de audio (`hikaru_audio_engine`) NUNCA debe alocar memoria dinámicamente (`Vec::push`, `Box::new`), ni usar locks bloqueantes (`Mutex`, `RwLock`) en el hilo de audio.

---

## 2. ORDEN ESTRICTO DE IMPLEMENTACIÓN (NUEVO PLAN)

### FASE 1: `hikaru_gui` (Maquetado Visual y Layout en `egui`)
- **Prioridad**: 1 (Inmediata)
- **Objetivo**: Crear un mockup 100% interactivo, con estética oscura moderna, respuesta visual fluida y diseño responsivo.
- **Entregables**:
  1. Header Bar (Botones Play/Stop, Selector OPENSTUDIO / OPENLIVE, Display BPM/Tempo).
  2. Vista OPENLIVE (Matriz de pads/clips responsiva con animaciones de estado faux-playing).
  3. Vista OPENSTUDIO (Mockup de Timeline / Pistas de arreglo).
  4. Mixer & DSP Rack en `SidePanel` (Faders interaccionables, perillas/knobs de EQ/Filtros, vumetros simulados).
  5. Footer Bar (Indicadores estéticos de CPU, RAM y Audio Status).

### FASE 2: `hikaru_core` & `hikaru_transport`
- **Prioridad**: 2
- **Objetivo**: Definir los tipos de datos reales (`AudioBuffer`, `SampleRate`) y la matemática del reloj/BPM para reemplazar las variables falsas de la GUI.

### FASE 3: `hikaru_audio_engine` & Canal IPC
- **Prioridad**: 3
- **Objetivo**: Conectar la GUI maquetada con el backend de audio mediante RingBuffers atómicos (`GuiCommand` / `AppState`).

### FASE 4: `hikaru_sequencer` & `hikaru_dsp`
- **Prioridad**: 4
- **Objetivo**: Lógica de disparo cuantizado de clips, sintetizador Wavetable y procesamiento de audio real.

### FASE 5: `hikaru_plugin_host`
- **Prioridad**: 5
- **Objetivo**: Soporte VST3 / CLAP.