# PROGRESO.md — Hikaru OpenStudio

FASE 1 ---> *(En Progreso)* **UI First / Mockup Interactivo en `egui`:**
- [x] Limpieza de dependencias externas no deseadas (`Slint` descartado).
- [ ] Reestructuración de layout con `egui` (`TopBottomPanel`, `SidePanel`, `CentralPanel`).
- [ ] Maquetado del Header Bar (Transporte, Modos, BPM).
- [ ] Maquetado de la Matriz de Clips (Modo `OPENLIVE`).
- [ ] Maquetado de la Línea de Tiempo / Arranger (Modo `OPENSTUDIO`).
- [ ] Maquetado del Mixer Multipista & DSP Rack (Faders, Knobs, Mute/Solo).
- [ ] Paleta de colores Oscura / Pro DAW (Tema Dark personalizable).

FASE 2 ---> *(Pendiente)* **Tipos Base y Reloj (`hikaru_core` & `hikaru_transport`):**
- [ ] Definición de tipos de datos de audio.
- [ ] Sincronización del BPM de la UI con el reloj sample-accurate.

FASE 3 ---> *(Pendiente)* **Conexión IPC / Engine (`hikaru_audio_engine`):**
- [ ] RingBuffers Lock-free para enviar comandos desde la GUI al motor.
- [ ] Generación de sonido real en tiempo real.

FASE 4 ---> *(Pendiente)* **Matriz de Clips Real & DSP (`hikaru_sequencer` & `hikaru_dsp`).**