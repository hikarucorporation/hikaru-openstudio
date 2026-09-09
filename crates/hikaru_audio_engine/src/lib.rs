// Copyright (C) Hikaru Corporation - 2026
// Miyu's Audio Engine
// GNU Affero General Public License v3
// crates/hikaru_audio_engine/src/lib.rs

pub mod preview_player;

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use hikaru_core::{AudioBuffer, SampleRate};
use hikaru_dsp::effects::filter::StateVariableFilter;
use hikaru_dsp::synth::wavetable::WavetableOscillator;
use hikaru_sequencer::TrackMatrix;
use hikaru_transport::{TransportPlaybackState, TransportPosition};

/// Máximo de frames stereo por bloque de audio. 2048 frames × 2 canales =
/// 8192 floats, cubre 48kHz @ ~42ms (BlockSize estándar de CPAL).
/// Pre-asignado una vez, cero heap alloc en el audio thread.
const MAX_TRACK_BUF: usize = 2048 * 2;

// ── Denormal Protection (FTZ/DAZ) ──────────────────────────────────────
// Los denormals (< 1.18e-38 en f36) causan penalizaciones severas de CPU
// en x86 anteriores a Haswell. Protegemos con:
// 1. MXCSR FTZ+DAZ en x86/x86_64 ( hardware flush )
// 2. Software flush como fallback (banco de bits)

/// Save original MXCSR to restore later (if needed).
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
static ORIGINAL_MXCSR: std::sync::OnceLock<u32> = std::sync::OnceLock::new();

/// Habilitar FTZ (Flush-to-Zero) y DAZ (Denormals-Are-Zero) en MXCSR.
/// Llamar UNA VEZ al inicio del thread de audio. Restaura al final.
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub fn enable_ftz_daz() {
    unsafe {
        let mut mxcsr: u32 = 0;
        std::arch::asm!("stmxcsr [{}]", in(reg) &mut mxcsr as *mut u32, options(nostack));
        let _ = ORIGINAL_MXCSR.set(mxcsr);
        // Bit 15: Flush-to-Zero
        // Bit 6:  Denormals-Are-Zero
        mxcsr |= (1 << 15) | (1 << 6);
        std::arch::asm!("ldmxcsr [{}]", in(reg) &mxcsr as *const u32, options(nostack));
    }
}

/// Restaurar MXCSR original.
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub fn restore_mxcsr() {
    if let Some(&original) = ORIGINAL_MXCSR.get() {
        unsafe {
            std::arch::asm!("ldmxcsr [{}]", in(reg) &original as *const u32);
        }
    }
}

/// Software denormal flush para plataformas no-x86.
/// Reemplaza valores subnormales (< f32::MIN_POSITIVE) por 0.0.
/// Operación branchless: compara bits del exponente.
#[inline(always)]
pub fn flush_denormals(buf: &mut [f32]) {
    for s in buf.iter_mut() {
        let bits = s.to_bits();
        // Exponente = 0 → denormal o cero. Pre-1.0 → preserva signo.
        if (bits & 0x7F800000) == 0 && bits != 0 {
            *s = 0.0;
        }
    }
}

use crate::preview_player::PreviewPlayer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineMode {
    OpenLive,
    OpenStudio,
}

/// Estado runtime de la voz del clip en el render callback.
/// Categórico: sin loop de clip activo y pasado el final, la voz es
/// `Finished` (dormida) y solo emite silencio absoluto (0.0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceState {
    Active,
    Finished,
}

pub struct AudioClipInstance {
    pub id: usize,
    pub track_index: usize,
    pub scene_index: usize,
    pub samples: Vec<f32>,
    pub start_frame: u64,
    /// Reloj LINEAL de disparo (reloj absoluto del engine, sin wrap del
    /// loop global). La vida de la voz OpenLive se mide SIEMPRE contra
    /// este reloj: el Time Selection solo mueve el cursor, jamás extiende
    /// ni reinicia una voz. Solo `clip_loop_enabled == true` loopea.
    pub start_absolute: u64,
    pub duration_frames: u64,
    pub sample_offset: usize,
    pub channels: usize,
    pub is_playing: bool,
    /// Loop individual del clip (OpenLive), en frames mono.
    /// Independiente del loop global del transporte. Si no es válido
    /// (`!has_valid_clip_loop`), el clip loopea su duración real completa,
    /// nunca un largo hardcodeado en compases.
    pub clip_loop_enabled: bool,
    pub clip_loop_start: u64,
    pub clip_loop_end: u64,
    /// Clip cargado desde la Session Matrix (OpenLive).
    /// `SyncPlaylistClips` NO debe eliminar estos clips al sincronizar.
    pub is_matrix_clip: bool,
    /// Frame anterior renderizado (para detectar loop wrap y aplicar crossfade).
    /// Se inicializa en `usize::MAX` (sin frame previo válido).
    pub prev_frame: usize,
    /// Crossfade restante en samples (0 = sin crossfade activo).
    pub xfade_remaining: u32,
    /// Longitud del crossfade configurada (para calcular la ganancia).
    pub xfade_len: u32,
}

impl AudioClipInstance {
    /// Longitud real del audio en frames mono (dinámica, del archivo).
    pub fn natural_frames(&self) -> u64 {
        (self.samples.len() / self.channels.max(1)) as u64
    }

    /// Región de loop individual válida y clampeada a la longitud real.
    pub fn has_valid_clip_loop(&self) -> bool {
        self.clip_loop_enabled
            && self.clip_loop_end > self.clip_loop_start
            && self.clip_loop_start < self.natural_frames()
    }

    /// Longitud efectiva de reproducción: loop individual si es válido,
    /// si no la duración real del audio. Nunca 1 compás hardcodeado.
    pub fn playback_length_frames(&self) -> u64 {
        if self.has_valid_clip_loop() {
            let end = self.clip_loop_end.min(self.natural_frames());
            end.saturating_sub(self.clip_loop_start)
        } else {
            self.natural_frames()
        }
    }

    /// Resuelve el frame del buffer fuente para una posición global del
    /// transporte (en samples, ya con wrap del loop global aplicado).
    ///
    /// 1. Gating por longitud real (`natural_frames`) / ventana de emisión
    ///    (`duration_frames`): sin loop explícito válido, si `elapsed`
    ///    supera la muestra se retorna `None` (la voz muteará con 0.0) en
    ///    lugar de hacer free-run (`elapsed % natural`).
    /// 2. Con loop explícito, el wraparound se sincroniza con el Time
    ///    Selection global (`transport.loop_start_samples` /
    ///    `loop_end_samples`): la fase se ancla a `loop_start_samples`
    ///    para que coincida con el wrap del transporte.
    ///
    /// CATEGÓRICO (bug compás 5→7): este `None` significa silencio
    /// absoluto. El caller NO debe hacer `%`, debe dejar el 0.0 del
    /// `buffer.fill(0.0)` y dormir la voz (`VoiceState::Finished` /
    /// `is_playing = false`).
    pub fn voice_frame(
        &self,
        global_pos: u64,
        transport: &TransportPosition,
    ) -> Option<usize> {
        if global_pos < self.start_frame {
            return None;
        }
        let natural = self.natural_frames();
        if natural == 0 {
            return None;
        }
        let elapsed = global_pos - self.start_frame;

        if self.has_valid_clip_loop() {
            let loop_start = self.clip_loop_start.min(natural);
            let loop_end = self.clip_loop_end.min(natural.max(1)).max(loop_start + 1);
            let loop_len = loop_end.saturating_sub(loop_start).max(1);
            if elapsed < loop_end {
                return Some(elapsed as usize);
            }
            // Loopeo explícito: sincronizar wraparound con el loop global
            // (Time Selection) cuando la región es válida.
            if transport.has_valid_loop() {
                let since_global = global_pos.saturating_sub(transport.loop_start_samples);
                return Some((loop_start + (since_global % loop_len)) as usize);
            }
            return Some((loop_start + ((elapsed - loop_start) % loop_len)) as usize);
        }

        // Sin loop por clip: gating estricto. La ventana de emisión válida
        // es el mínimo entre la duración en timeline y la muestra real.
        // CATEGÓRICO: `elapsed >= emission_len` → None (0.0 + Finished).
        let emission_len = self.emission_len_frames();
        if elapsed >= emission_len {
            return None;
        }
        Some(elapsed as usize)
    }

    /// Fase de la voz OpenLive sobre el reloj LINEAL del engine
    /// (`absolute_frame`, sin wrap del loop global).
    ///
    /// REGLA DEL CLIP MANDA: el wraparound/loopeo ocurre SI Y SÓLO SI
    /// `clip_loop_enabled` es válido. El Time Selection (loop global) NO
    /// re-emite ni extiende la voz: con el cursor loopeando en una región
    /// más corta que el clip, `elapsed` lineal igual supera `emission_len`
    /// y la voz muta a `(0.0, 0.0)` y se duerme. Sin `%` global.
    pub fn voice_frame_linear(&self, abs_pos: u64) -> Option<usize> {
        if abs_pos < self.start_absolute {
            return None;
        }
        let natural = self.natural_frames();
        if natural == 0 {
            return None;
        }
        let elapsed = abs_pos - self.start_absolute;

        if self.has_valid_clip_loop() {
            let loop_start = self.clip_loop_start.min(natural);
            let loop_end = self.clip_loop_end.min(natural.max(1)).max(loop_start + 1);
            let loop_len = loop_end.saturating_sub(loop_start).max(1);
            if elapsed < loop_end {
                return Some(elapsed as usize);
            }
            // Loop EXPLÍCITO del clip, sobre tiempo lineal propio.
            // Sin ancla al Time Selection: el clip manda sobre el timeline.
            return Some((loop_start + ((elapsed - loop_start) % loop_len)) as usize);
        }

        // Sin loop por clip: gating estricto sobre tiempo lineal.
        // Compases 10..15 de un clip de 9 (dentro o fuera del Time
        // Selection) → None (0.0 + Finished).
        let emission_len = self.emission_len_frames();
        if elapsed >= emission_len {
            return None;
        }
        Some(elapsed as usize)
    }

    /// Estado runtime OpenLive sobre el reloj lineal.
    pub fn voice_state_linear(&self, abs_pos: u64) -> VoiceState {
        match self.voice_frame_linear(abs_pos) {
            Some(_) => VoiceState::Active,
            None => VoiceState::Finished,
        }
    }

    /// Ventana de emisión válida en frames: mínimo entre timeline y muestra.
    pub fn emission_len_frames(&self) -> u64 {
        let natural = self.natural_frames();
        if self.duration_frames > 0 {
            self.duration_frames.min(natural)
        } else {
            natural
        }
    }

    /// Estado runtime de la voz para `global_pos`.
    /// `None` de `voice_frame` (área sin datos) → `Finished` (dormida).
    pub fn voice_state(&self, global_pos: u64, transport: &TransportPosition) -> VoiceState {
        match self.voice_frame(global_pos, transport) {
            Some(_) => VoiceState::Active,
            None => VoiceState::Finished,
        }
    }

    /// Lectura categórica del buffer fuente (frame mono → sample L).
    ///
    /// Exigencia #1 del render callback:
    /// si `current_sample_index >= total` (natural/offset) y NO hay loop
    /// de clip activo, retorna `0.0` (silencio absoluto) en lugar de
    /// avanzar o hacer módulo. Cualquier OOB, NaN o infinito → 0.0.
    pub fn read_sample(&self, current_sample_index: usize) -> f32 {
        // Corte categórico sin loop: pasado el natural → silencio.
        if !self.has_valid_clip_loop() {
            if (current_sample_index as u64) >= self.natural_frames() {
                return 0.0;
            }
        }
        let ch = self.channels.max(1);
        let idx = current_sample_index.saturating_mul(ch);
        match self.samples.get(idx) {
            Some(&s) if s.is_finite() => s,
            _ => 0.0,
        }
    }

    /// Lectura del canal derecho (o duplicado de L si es mono), con el
    /// mismo corte categórico a 0.0 que `read_sample`.
    pub fn read_sample_r(&self, current_sample_index: usize) -> f32 {
        if !self.has_valid_clip_loop() {
            if (current_sample_index as u64) >= self.natural_frames() {
                return 0.0;
            }
        }
        let ch = self.channels.max(1);
        let idx = current_sample_index.saturating_mul(ch);
        if ch > 1 {
            match self.samples.get(idx + 1) {
                Some(&s) if s.is_finite() => s,
                _ => 0.0,
            }
        } else {
            self.read_sample(current_sample_index)
        }
    }
}

pub struct AudioEngine<'a> {
    pub transport: TransportPosition,
    pub mode: EngineMode,
    pub matrix: TrackMatrix,
    pub main_filter: StateVariableFilter,
    pub oscillator: WavetableOscillator<'a>,
    pub sample_rate: f32,
    pub clips: Vec<AudioClipInstance>,
    pub preview_player: PreviewPlayer,
    // Referencia compartida del reloj de muestras con la GUI
    pub position_clock: Arc<AtomicU64>,
    /// Reloj LINEAL monótono en frames (solo avanza en `Playing`, nunca
    /// hace wrap). Base de la vida de las voces OpenLive: el cursor
    /// (`transport.sample_count`) puede loopear en el Time Selection, pero
    /// este reloj no, así el Time Selection jamás extiende una voz.
    pub absolute_frame: u64,
    /// Pico de salida real del último bloque procesado (bits de f32).
    /// La GUI lo lee para el vúmetro: 0.0 == silencio == -inf dB.
    /// Sin esto el vúmetro dibujaba el fader y mentía en áreas sin audio.
    pub output_level_bits: Arc<AtomicU32>,
    /// Pico por pista (16 canales). Cada `AtomicU32` almacena el peak
    /// (f32 bits) de ESA pista específica en el último bloque. El GUI los
    /// lee para los VU meters individuales de la Mixer.
    pub track_peak_bits: [Arc<AtomicU32>; 16],
    /// Buffers scratch por pista para acumular señal aislada antes de la
    /// mezcla final. Cada buffer es stereo (frames * 2).
    /// Se pre-asigna UNA VEZ con capacidad fija en `new()`. El `resize`
    /// en `process()` es no-op cuando `buffer_frames` no cambia (producción
    /// CPAL usa block size fijo → cero heap alloc en el audio thread).
    track_output_bufs: Vec<Vec<f32>>,
    /// Per-track volume (0.0..1.0), indexado por track_index.
    /// 16 pistas máximas (TrackMatrix limit).
    /// AtomicU32 para reads lock-free desde el audio thread.
    pub track_volumes: [AtomicU32; 16],
    /// Per-track pan (-100.0..100.0), indexado por track_index.
    pub track_pans: [AtomicU32; 16],
    /// Per-track mute flag, indexado por track_index.
    pub track_mutes: [AtomicBool; 16],
    /// Per-track solo flag, indexado por track_index.
    pub track_solos: [AtomicBool; 16],
}

impl<'a> AudioEngine<'a> {
    pub fn new(sr: SampleRate, wavetable: &'a [f32], position_clock: Arc<AtomicU64>) -> Self {
        let mut filter = StateVariableFilter::new();
        filter.set_params(2000.0, 0.707, sr.get());

        Self {
            transport: TransportPosition::new(sr, 128.0),
            mode: EngineMode::OpenStudio,
            matrix: TrackMatrix::new(),
            main_filter: filter,
            oscillator: WavetableOscillator::new(wavetable, sr),
            sample_rate: sr.get(),
            clips: Vec::new(),
            preview_player: PreviewPlayer::new(),
            position_clock,
            absolute_frame: 0,
            output_level_bits: Arc::new(AtomicU32::new(0.0f32.to_bits())),
            track_peak_bits: std::array::from_fn(|_| Arc::new(AtomicU32::new(0.0f32.to_bits()))),
            track_output_bufs: (0..16).map(|_| Vec::with_capacity(MAX_TRACK_BUF)).collect(),
            track_volumes: std::array::from_fn(|_| AtomicU32::new(0.75f32.to_bits())),
            track_pans: std::array::from_fn(|_| AtomicU32::new(0.0f32.to_bits())),
            track_mutes: std::array::from_fn(|_| AtomicBool::new(false)),
            track_solos: std::array::from_fn(|_| AtomicBool::new(false)),
        }
    }

    /// Pico real del último bloque (0.0 == silencio absoluto).
    pub fn output_peak(&self) -> f32 {
        f32::from_bits(self.output_level_bits.load(Ordering::Relaxed))
    }

    /// Pico de una pista específica (0.0 si silencio).
    pub fn track_peak(&self, track_idx: usize) -> f32 {
        if track_idx < 16 {
            f32::from_bits(self.track_peak_bits[track_idx].load(Ordering::Relaxed))
        } else {
            0.0
        }
    }

    /// Nivel en dB del último bloque (-inf si es silencio).
    pub fn output_db(&self) -> f32 {
        let peak = self.output_peak();
        if peak <= 0.0 || !peak.is_finite() {
            f32::NEG_INFINITY
        } else {
            20.0 * peak.log10()
        }
    }

    pub fn set_mode(&mut self, mode: EngineMode) {
        self.mode = mode;
    }

    pub fn set_sample_rate(&mut self, new_sr: f32) {
        self.sample_rate = new_sr;
        self.transport.sample_rate = hikaru_core::SampleRate::new(new_sr);
        self.main_filter.set_params(2000.0, 0.707, new_sr);
    }

    pub fn set_track_volume(&self, track_idx: usize, volume: f32) {
        if track_idx < 16 {
            self.track_volumes[track_idx].store(volume.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
        }
    }

    pub fn set_track_pan(&self, track_idx: usize, pan: f32) {
        if track_idx < 16 {
            self.track_pans[track_idx].store(pan.clamp(-100.0, 100.0).to_bits(), Ordering::Relaxed);
        }
    }

    pub fn set_track_mute(&self, track_idx: usize, mute: bool) {
        if track_idx < 16 {
            self.track_mutes[track_idx].store(mute, Ordering::Relaxed);
        }
    }

    pub fn set_track_solo(&self, track_idx: usize, solo: bool) {
        if track_idx < 16 {
            self.track_solos[track_idx].store(solo, Ordering::Relaxed);
        }
    }

    /// Limpia los clips anteriores y sincroniza la nueva lista que llega desde la GUI
    pub fn sync_clips(&mut self, new_clips: Vec<AudioClipInstance>) {
        self.clips = new_clips;
    }

    pub fn update_clip_bounds(
        &mut self,
        clip_id: usize,
        track_index: usize,
        scene_index: usize,
        start_secs: f32,
        duration_secs: f32,
        offset_secs: f32,
    ) {
        let is_studio = self.mode == EngineMode::OpenStudio;
        if let Some(clip) = self
            .clips
            .iter_mut()
            .find(|c| {
                if is_studio {
                    c.id == clip_id
                } else {
                    c.track_index == track_index && c.scene_index == scene_index
                }
            })
        {
            clip.start_frame = (start_secs * self.sample_rate) as u64;
            // Si la GUI manda duración 0/desconocida (ej. matriz OpenLive al
            // cargar), derivarla de la duración REAL del audio decodificado,
            // nunca de un largo hardcodeado en compases.
            let natural = clip.natural_frames();
            clip.duration_frames = if duration_secs > 0.0 {
                (duration_secs * self.sample_rate) as u64
            } else {
                natural
            };
            clip.sample_offset = (offset_secs * self.sample_rate) as usize * clip.channels;
        }
    }

    /// Puntos de loop individual del clip (OpenLive, en segundos GUI).
    /// Se convierten a frames con el SR real del engine y se clampan a la
    /// longitud real del audio. `enabled == false` o `end <= start`
    /// desactiva el loop individual (el clip loopea completo).
    pub fn set_clip_loop(
        &mut self,
        track_index: usize,
        scene_index: usize,
        start_secs: f32,
        end_secs: f32,
        enabled: bool,
    ) {
        let sr = self.sample_rate.max(1.0);
        if let Some(clip) = self
            .clips
            .iter_mut()
            .find(|c| c.track_index == track_index && c.scene_index == scene_index)
        {
            let natural = clip.natural_frames();
            let start = (start_secs.max(0.0) * sr) as u64;
            let mut end = (end_secs.max(0.0) * sr) as u64;
            if natural > 0 {
                end = end.min(natural);
            }
            if enabled && end > start && start < natural {
                clip.clip_loop_enabled = true;
                clip.clip_loop_start = start;
                clip.clip_loop_end = end;
            } else {
                clip.clip_loop_enabled = false;
                clip.clip_loop_start = 0;
                clip.clip_loop_end = 0;
            }
        }
    }

    pub fn add_clip(
        &mut self,
        id: usize,
        track_index: usize,
        scene_index: usize,
        samples: Vec<f32>,
        start_secs: f32,
        duration_secs: f32,
        offset_secs: f32,
        channels: usize,
        is_matrix_clip: bool,
    ) {
        let ch = channels.max(1);
        let start_frame = (start_secs * self.sample_rate) as u64;
        // Duración dinámica: si la GUI manda 0 (matriz OpenLive al cargar),
        // usar la longitud REAL del audio decodificado, nunca 1 compás fijo.
        let natural_frames = (samples.len() / ch) as u64;
        let duration_frames = if duration_secs > 0.0 {
            (duration_secs * self.sample_rate) as u64
        } else {
            natural_frames
        };
        let sample_offset = (offset_secs * self.sample_rate) as usize * ch;

        // Voces OpenLive arrancan SIEMPRE detenidas (`is_playing = false`).
        // Antes se infería de `self.mode == OpenStudio`: si el clip de la
        // matriz se cargaba antes del `SetAppMode(false)` (engine aún en
        // OpenStudio), la voz quedaba en `playing` invisible (GUI en
        // `Stopped`) y sonaba sin pad verde hasta el compás 11 con el
        // vúmetro marcando. En OpenStudio el flag se ignora (la mezcla es
        // por ventana temporal), así que `false` inicial es seguro en ambos.
        let instance = AudioClipInstance {
            id,
            track_index,
            scene_index,
            samples,
            start_frame,
            start_absolute: 0,
            duration_frames,
            sample_offset,
            channels: ch,
            is_playing: false,
            clip_loop_enabled: false,
            clip_loop_start: 0,
            clip_loop_end: 0,
            is_matrix_clip,
            prev_frame: usize::MAX,
            xfade_remaining: 0,
            xfade_len: 0,
        };

        let is_studio = self.mode == EngineMode::OpenStudio;
        if let Some(existing) = self
            .clips
            .iter_mut()
            .find(|c| {
                if is_studio {
                    c.id == id
                } else {
                    c.track_index == track_index && c.scene_index == scene_index
                }
            })
        {
            *existing = instance;
        } else {
            self.clips.push(instance);
        }
    }

    pub fn remove_clip(&mut self, clip_id: usize, track_index: usize, scene_index: usize) {
        let is_studio = self.mode == EngineMode::OpenStudio;
        self.clips.retain(|c| {
            if is_studio {
                c.id != clip_id
            } else {
                !(c.track_index == track_index && c.scene_index == scene_index)
            }
        });
    }

    pub fn play(&mut self) {
        // Anti-fuga: detener el preview del explorer al arrancar el
        // transporte. El preview NO suena en process() cuando el flag
        // is_playing baja acá. Si un PreviewSample tardío (mpsc) llega
        // después, suena hasta que el usuario lo frena o clickea Play.
        self.preview_player.stop();
        self.transport.playback_state = TransportPlaybackState::Playing;
    }

    pub fn pause(&mut self) {
        self.transport.playback_state = TransportPlaybackState::Paused;
    }

    pub fn stop(&mut self) {
        self.transport.playback_state = TransportPlaybackState::Stopped;
        self.transport.sample_count = 0;
    }

    /// Configura el loop global del transporte (dueño único del wrap).
    /// `start`/`end` en SAMPLES, convertidos en la GUI con los mismos ticks
    /// de la barra (`ticks_to_samples`), así engine y GUI coinciden exacto.
    /// El wrap se aplica en `process()`; la GUI no reescribe `sample_count`.
    pub fn set_global_loop(&mut self, start_samples: u64, end_samples: u64, enabled: bool) {
        self.transport.set_loop_region_samples(start_samples, end_samples);
        self.transport.set_loop_enabled(enabled);
    }

    pub fn seek(&mut self, position_secs: f32) {
        self.transport.sample_count = (position_secs * self.sample_rate) as u64;
    }

    /// Dispara (o frena, toggle) el clip de un pad.
    /// Si el clip ya estaba sonando se frena; si no, suena en exclusiva en
    /// su pista y su fase arranca en el transporte actual (`start_frame`),
    /// para que el re-disparo reinicie el sample en lugar de enganchar una
    /// fase arbitraria del reloj global.
    pub fn trigger_clip(&mut self, track_index: usize, scene_index: usize) {
        let now = self.transport.sample_count;
        let now_abs = self.absolute_frame;
        let target_was_playing = self
            .clips
            .iter()
            .find(|c| c.track_index == track_index && c.scene_index == scene_index)
            .map(|c| c.is_playing)
            .unwrap_or(false);
        for clip in self.clips.iter_mut().filter(|c| c.track_index == track_index) {
            if clip.scene_index == scene_index {
                clip.is_playing = !target_was_playing;
                if clip.is_playing {
                    clip.start_frame = now;
                    clip.start_absolute = now_abs;
                }
            } else {
                clip.is_playing = false;
            }
        }
    }

    /// Dispara una escena completa: en cada pista suena el clip de esa
    /// escena (si existe) y se frenan los demás. Las fases arrancan en el
    /// transporte actual para un ataque en conjunto.
    /// Idempotente ante duplicados del mismo batch (la GUI envía UN solo
    /// `TriggerScene` por click): si la voz ya está activa con
    /// `start_frame == now`, se ignora sin cambiar estado ni reiniciar fase,
    /// para que un reenvío no extienda la emisión hasta el compás 11.
    pub fn trigger_scene(&mut self, scene_index: usize) {
        let now = self.transport.sample_count;
        let now_abs = self.absolute_frame;
        // Pistas que tienen clip en la escena.
        let mut tracks_with_clip = std::collections::HashSet::new();
        for clip in self.clips.iter() {
            if clip.scene_index == scene_index {
                tracks_with_clip.insert(clip.track_index);
            }
        }
        for clip in self.clips.iter_mut() {
            if clip.scene_index == scene_index {
                // Duplicado del mismo instante lineal: no reiniciar.
                // (Se compara el reloj ABSOLUTO, no el cursor con wrap:
                // con el loop global el cursor repite valores y un
                // relanzamiento legítimo se tragaría como duplicado.)
                if clip.is_playing && clip.start_absolute == now_abs {
                    continue;
                }
                clip.is_playing = true;
                clip.start_frame = now;
                clip.start_absolute = now_abs;
            } else if tracks_with_clip.contains(&clip.track_index) {
                clip.is_playing = false;
            }
        }
    }

    /// ¿Sigue audible la voz OpenLive de un pad? Base LINEAL
    /// (`absolute_frame`), no cursor con wrap: el polling GUI apaga el pad
    /// cuando la emisión lineal se agota aunque el cursor siga loopeando.
    pub fn voice_active(&self, track_index: usize, scene_index: usize) -> bool {
        match self
            .clips
            .iter()
            .find(|c| c.track_index == track_index && c.scene_index == scene_index)
        {
            None => false,
            Some(clip) => {
                if !clip.is_playing {
                    return false;
                }
                clip.voice_state_linear(self.absolute_frame) == VoiceState::Active
            }
        }
    }

    /// Frames lineales elapsed de la voz de un pad (`absolute_frame` menos
    /// `start_absolute`). `None` si no hay voz activa: la GUI usa el cursor
    /// global como fallback. Con esto la aguja del Clip Track Editor deriva
    /// de la VOZ (0 al disparar → tick 0 → línea del compás 1), nunca del
    /// cursor global, así aguja visual y audio coinciden en el golpe a 0 ms.
    pub fn voice_elapsed_frames(
        &self,
        track_index: usize,
        scene_index: usize,
    ) -> Option<u64> {
        let clip = self
            .clips
            .iter()
            .find(|c| c.track_index == track_index && c.scene_index == scene_index)?;
        if !clip.is_playing {
            return None;
        }
        if clip.voice_state_linear(self.absolute_frame) != VoiceState::Active {
            return None;
        }
        Some(self.absolute_frame.saturating_sub(clip.start_absolute))
    }

    pub fn stop_track(&mut self, track_index: usize) {
        for clip in self.clips.iter_mut().filter(|c| c.track_index == track_index) {
            clip.is_playing = false;
        }
    }

    pub fn process(&mut self, out_buffer: &mut AudioBuffer<'_>) {
        let samples = out_buffer.get_samples_mut();
        let num_channels = 2; // Estéreo

        // El buffer del bloque actual se rellena con ceros.
        samples.fill(0.0);

        // ── PREVIEW: solo si hay preview activo (evita Arc::clone innecesario) ──
        if self.preview_player.is_active() {
            let preview = self.preview_player.shared_buffer();
            preview.process(samples);
        }

        // ── CLIP MIXING con per-track peaks ──
        if self.transport.playback_state == TransportPlaybackState::Playing {
            let buffer_frames = (samples.len() / num_channels) as u64;
            if buffer_frames == 0 {
                self.output_level_bits.store(0.0f32.to_bits(), Ordering::Relaxed);
                for p in &self.track_peak_bits {
                    p.store(0.0f32.to_bits(), Ordering::Relaxed);
                }
                return;
            }
            let buf_len = samples.len();
            let frame_start = self.transport.sample_count;
            let frame_end = frame_start + buffer_frames;
            let abs_start = self.absolute_frame;
            let transport = self.transport;
            let mode = self.mode;

            // Pre-compute per-track gain (volume + pan + mute/solo).
            // Lecturas atómicas lock-free: el audio thread lee sin mutex.
            let any_solo = self.track_solos.iter().any(|s| s.load(Ordering::Relaxed));
            let mut track_gain_l = [0.0f32; 16];
            let mut track_gain_r = [0.0f32; 16];
            for t in 0..16 {
                let vol = f32::from_bits(self.track_volumes[t].load(Ordering::Relaxed));
                let muted = self.track_mutes[t].load(Ordering::Relaxed);
                let soloed = self.track_solos[t].load(Ordering::Relaxed);
                let effective_gain = if muted {
                    0.0
                } else if any_solo && !soloed {
                    0.0
                } else {
                    vol
                };
                let pan_norm = f32::from_bits(self.track_pans[t].load(Ordering::Relaxed)) / 100.0;
                track_gain_l[t] = effective_gain * if pan_norm > 0.0 {
                    1.0 - pan_norm
                } else {
                    1.0
                };
                track_gain_r[t] = effective_gain * if pan_norm < 0.0 {
                    1.0 + pan_norm
                } else {
                    1.0
                };
            }

            // ── FASE 1: Zero la región usada de cada scratch buffer ──
            for buf in self.track_output_bufs.iter_mut() {
                if buf.len() != buf_len {
                    buf.resize(buf_len, 0.0);
                }
                buf.fill(0.0);
            }

            // ── FASE 2: Acumular clips en scratch buffers por pista ──
            for clip in self.clips.iter_mut() {
                let ti = clip.track_index.min(15);
                let gl = track_gain_l[ti];
                let gr = track_gain_r[ti];
                let track_buf = &mut self.track_output_bufs[ti];

                // ── Precomputar por clip (una sola vez, no por sample) ──
                let ch = clip.channels.max(1);
                let buf_ptr = clip.samples.as_ptr();
                let buf_len = clip.samples.len();
                let offset_frames = clip.sample_offset / ch;
                let has_loop = clip.has_valid_clip_loop();

                if mode == EngineMode::OpenStudio {
                    let clip_frame_end = clip.start_frame.saturating_add(clip.duration_frames);
                    for f in 0..buffer_frames as usize {
                        let global_frame = transport.wrap_sample_count(frame_start + f as u64);
                        if global_frame < clip.start_frame || global_frame >= clip_frame_end {
                            continue;
                        }
                        let relative_frame = (global_frame - clip.start_frame) as usize;
                        let idx = (offset_frames + relative_frame) * ch;

                        // Fast-path: direct indexing sin bounds check repetido.
                        if idx + ch <= buf_len {
                            let l = unsafe { *buf_ptr.add(idx) } * gl;
                            let r = if ch > 1 {
                                (unsafe { *buf_ptr.add(idx + 1) }) * gr
                            } else {
                                l
                            };
                            track_buf[f * 2] += l;
                            track_buf[f * 2 + 1] += r;
                        }
                    }
                } else {
                    if !clip.is_playing {
                        continue;
                    }

                    let emission = clip.emission_len_frames();
                    let xfade_frames: usize = if has_loop { 32 } else { 0 };
                    let mut clip_finished = false;
                    for f in 0..buffer_frames as usize {
                        let abs = abs_start + f as u64;

                        if !has_loop && abs >= clip.start_absolute {
                            let elapsed = abs - clip.start_absolute;
                            if elapsed > 0 && elapsed >= emission {
                                clip_finished = true;
                                break;
                            }
                        }

                        let Some(relative_frame) = clip.voice_frame_linear(abs) else {
                            continue;
                        };

                        let idx = (offset_frames + relative_frame) * ch;

                        // ── MICRO-CROSSFADE en loop boundary ──
                        if xfade_frames > 0
                            && clip.prev_frame != usize::MAX
                            && relative_frame < clip.prev_frame
                            && clip.prev_frame - relative_frame > (emission / 2) as usize
                        {
                            clip.xfade_remaining = xfade_frames as u32;
                            clip.xfade_len = xfade_frames as u32;
                        }

                        if clip.xfade_remaining > 0 && idx + ch <= buf_len {
                            let fade_pos = clip.xfade_len - clip.xfade_remaining;
                            let fade_total = clip.xfade_len as f32;
                            let out_gain = fade_pos as f32 / fade_total;
                            let in_gain = 1.0 - out_gain;

                            let l_new = unsafe { *buf_ptr.add(idx) } * gl;
                            let r_new = if ch > 1 {
                                (unsafe { *buf_ptr.add(idx + 1) }) * gr
                            } else {
                                l_new
                            };

                            let prev_idx = (offset_frames + clip.prev_frame) * ch;
                            let (l_old, r_old) = if prev_idx + ch <= buf_len {
                                let lo = unsafe { *buf_ptr.add(prev_idx) } * gl;
                                let ro = if ch > 1 {
                                    (unsafe { *buf_ptr.add(prev_idx + 1) }) * gr
                                } else {
                                    lo
                                };
                                (lo, ro)
                            } else {
                                (0.0, 0.0)
                            };

                            track_buf[f * 2] += l_old * in_gain + l_new * out_gain;
                            track_buf[f * 2 + 1] += r_old * in_gain + r_new * out_gain;
                            clip.xfade_remaining -= 1;
                        } else if idx + ch <= buf_len {
                            let l = unsafe { *buf_ptr.add(idx) } * gl;
                            let r = if ch > 1 {
                                (unsafe { *buf_ptr.add(idx + 1) }) * gr
                            } else {
                                l
                            };
                            track_buf[f * 2] += l;
                            track_buf[f * 2 + 1] += r;
                        }

                        clip.prev_frame = relative_frame;
                    }
                    if clip_finished {
                        clip.is_playing = false;
                    }
                }
            }

            // ── FASE 3: Mix directo al output + peak branchless ──
            // Sin `if v != 0.0`: add 0.0 es identidad, zero branch mispredict.
            // Peak branchless: `abs(x) = x.abs()` + `max(a,b) = if a>b{a}else{b}`
            // sin conditional sobre datos de audio (branchless por CPU pipeline).
            for t in 0..16 {
                let track_buf = &self.track_output_bufs[t];

                let mut track_peak = 0.0f32;
                for i in 0..buf_len {
                    let v = track_buf[i];
                    // Branchless peak: NaN/Inf → 0.0, finito → abs.
                    let a = if v.is_finite() { v.abs() } else { 0.0 };
                    if a > track_peak {
                        track_peak = a;
                    }
                    // Mix directo: siempre sumo, sin chequeo.
                    samples[i] += v;
                }

                self.track_peak_bits[t].store(track_peak.to_bits(), Ordering::Relaxed);
            }

            self.transport.sample_count = self.transport.wrap_sample_count(frame_end);
            self.absolute_frame = self.absolute_frame.saturating_add(buffer_frames);
        } else {
            // Transporte detenido: peaks en silencio.
            for p in &self.track_peak_bits {
                p.store(0.0f32.to_bits(), Ordering::Relaxed);
            }
        }

        // ── Denormal flush ──
        // Previene CPU stalls en x86 pre-Haswell por subnormales.
        // Flush una sola vez sobre el buffer master (no por track).
        flush_denormals(samples);

        // Pico real del bloque (preview + clips mix) para el vúmetro master.
        // Soft-clipper: tanh() satura suavemente sin hard-clip artifacts.
        // Mantiene la forma de onda y previene clipping de la placa.
        let mut peak = 0.0f32;
        for s in samples.iter_mut() {
            // Soft-clip: tanh() mapea (-∞,+∞) → (-1,+1) con curva suave.
            *s = s.tanh();
            // Branchless peak.
            let a = (*s).abs();
            if a > peak {
                peak = a;
            }
        }
        self.output_level_bits.store(peak.to_bits(), Ordering::Relaxed);
        self.position_clock.store(self.transport.sample_count, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;

    static TABLE: [f32; 2048] = [0.0; 2048];

    fn test_engine() -> AudioEngine<'static> {
        let clock = Arc::new(AtomicU64::new(0));
        let engine = AudioEngine::new(SampleRate::new(44100.0), &TABLE, clock);
        // Unity gain for all tracks in tests (bypass per-track volume)
        for t in 0..16 {
            engine.track_volumes[t].store(1.0f32.to_bits(), Ordering::Relaxed);
        }
        engine
    }

    fn run_frames(engine: &mut AudioEngine, frames: u64) {
        // Buffer estéreo: 2 samples por frame.
        let mut raw = vec![0.0f32; (frames * 2) as usize];
        let mut buf = AudioBuffer::new(&mut raw);
        engine.process(&mut buf);
    }

    #[test]
    fn openlive_clip_duration_is_real_audio_length() {
        let mut engine = test_engine();
        engine.set_mode(EngineMode::OpenLive);
        // 1s estéreo @44100: 44100 frames. duration_secs = 0 (como manda la
        // matriz al cargar) debe derivarse del audio real, no de 1 compás.
        let samples = vec![0.5f32; 44100 * 2];
        engine.add_clip(1, 0, 0, samples, 0.0, 0.0, 0.0, 2, false);
        assert_eq!(engine.clips[0].duration_frames, 44100);
        assert_eq!(engine.clips[0].playback_length_frames(), 44100);
    }

    #[test]
    fn trigger_clip_toggles_and_restarts_phase() {
        let mut engine = test_engine();
        engine.set_mode(EngineMode::OpenLive);
        engine.transport.sample_count = 8000;
        engine.add_clip(1, 0, 0, vec![0.5f32; 100 * 2], 0.0, 0.0, 0.0, 2, false);
        engine.trigger_clip(0, 0);
        assert!(engine.clips[0].is_playing);
        assert_eq!(engine.clips[0].start_frame, 8000);
        // Segundo toque al pad que suena: frena (toggle).
        engine.trigger_clip(0, 0);
        assert!(!engine.clips[0].is_playing);
    }

    #[test]
    fn trigger_scene_plays_whole_row() {
        let mut engine = test_engine();
        engine.set_mode(EngineMode::OpenLive);
        engine.add_clip(1, 0, 0, vec![0.5f32; 100 * 2], 0.0, 0.0, 0.0, 2, false);
        engine.add_clip(2, 1, 0, vec![0.5f32; 100 * 2], 0.0, 0.0, 0.0, 2, false);
        engine.add_clip(3, 0, 1, vec![0.5f32; 100 * 2], 0.0, 0.0, 0.0, 2, false);
        engine.trigger_scene(1);
        assert!(!engine.clips[0].is_playing);
        assert!(!engine.clips[1].is_playing);
        assert!(engine.clips[2].is_playing);
    }

    #[test]
    fn global_loop_wraps_in_engine_not_gui() {
        let mut engine = test_engine();
        engine.play();
        engine.set_global_loop(0, 44100, true);
        // 100 buffers de 512 frames = 51200 > 44100 → wrap a 7100.
        for _ in 0..100 {
            run_frames(&mut engine, 512);
        }
        assert_eq!(engine.transport.sample_count, 51200 - 44100);
        assert_eq!(
            engine.position_clock.load(Ordering::Relaxed),
            engine.transport.sample_count
        );
    }

    #[test]
    fn global_loop_disabled_never_wraps() {
        let mut engine = test_engine();
        engine.play();
        for _ in 0..100 {
            run_frames(&mut engine, 512);
        }
        // Sin loop (OpenLive sin región): el cursor avanza libre, jamás
        // vuelve a 0 en el compás 2.
        assert_eq!(engine.transport.sample_count, 51200);
    }

    #[test]
    fn voice_frame_mutes_past_natural_without_clip_loop() {
        // Exigencia #1: sin loop por clip → None al superar la muestra
        // (el runner suma 0.0), nunca `elapsed % natural`.
        let mut engine = test_engine();
        engine.set_mode(EngineMode::OpenLive);
        engine.add_clip(1, 0, 0, vec![0.5f32; 1000 * 2], 0.0, 0.0, 0.0, 2, false);
        let t = engine.transport;
        assert!(!engine.clips[0].has_valid_clip_loop());
        assert_eq!(engine.clips[0].voice_frame(0, &t), Some(0));
        assert_eq!(engine.clips[0].voice_frame(999, &t), Some(999));
        assert_eq!(engine.clips[0].voice_frame(1000, &t), None);
        assert_eq!(engine.clips[0].voice_frame(5000, &t), None);
    }

    #[test]
    fn voice_frame_with_explicit_clip_loop_wraps() {
        // Exigencia #2: CON loop explícito sí se repite dentro de su región.
        let mut engine = test_engine();
        engine.set_mode(EngineMode::OpenLive);
        engine.add_clip(1, 0, 0, vec![0.5f32; 1000 * 2], 0.0, 0.0, 0.0, 2, false);
        engine.set_clip_loop(0, 0, 0.0, 0.25, true);
        assert!(engine.clips[0].has_valid_clip_loop());
        let t = engine.transport;
        let wrapped = engine.clips[0].voice_frame(5000, &t).unwrap();
        assert!((0..1000).contains(&(wrapped as u64)));
    }

    #[test]
    fn clip_loop_region_is_honored() {
        let mut engine = test_engine();
        engine.set_mode(EngineMode::OpenLive);
        engine.add_clip(1, 0, 0, vec![0.5f32; 44100 * 2], 0.0, 0.0, 0.0, 2, false);
        engine.set_clip_loop(0, 0, 0.0, 0.5, true);
        let clip = &engine.clips[0];
        assert!(clip.has_valid_clip_loop());
        assert_eq!(clip.clip_loop_start, 0);
        assert_eq!(clip.clip_loop_end, 22050);
        assert_eq!(clip.playback_length_frames(), 22050);
        // Desactivar: vuelve a loopear el audio completo.
        engine.set_clip_loop(0, 0, 0.0, 0.0, false);
        assert!(!engine.clips[0].has_valid_clip_loop());
        assert_eq!(engine.clips[0].playback_length_frames(), 44100);
    }

    #[test]
    fn play_stops_preview_immediately() {
        // FUGA #1: Play global debe hacer stop() del preview (buffer limpio,
        // flag abajo) para que no se mezcle con el Master Mixer.
        let mut engine = test_engine();
        engine.preview_player.play(vec![0.5f32; 1024], 1);
        assert!(engine.preview_player.is_playing());
        engine.play();
        assert!(!engine.preview_player.is_playing());
    }

    #[test]
    fn process_in_playing_always_mixes_preview() {
        // El preview del explorer SIEMPRE suena en el Master Bus, incluso
        // si el transporte está en Playing. La anti-fuga se gestiona en
        // `play()` (stop del preview) y en el command handler (StopPreview
        // antes de Play). Un PreviewSample tardío DESPUÉS del Play sí se
        // mezcla: el usuario pidió pre-escuchar mientras el timeline corre.
        let mut engine = test_engine();
        engine.preview_player.play(vec![0.5f32; 4096], 1);
        engine.play();
        // Simular PreviewSample tardío (mpsc) DESPUÉS del Play.
        engine.preview_player.play(vec![0.5f32; 4096], 1);
        assert!(engine.preview_player.is_playing());
        let mut raw = vec![0.0f32; 512 * 2];
        let mut buf = AudioBuffer::new(&mut raw);
        engine.process(&mut buf);
        let peak = raw.iter().fold(0.0f32, |m, &s| m.max(s.abs()));
        assert!(peak > 0.0, "preview must always mix into Master Bus");
        assert!(engine.output_peak() > 0.0);
    }

    #[test]
    fn clip_length_frames_uses_real_transport_tempo() {
        // FUGA #2: 4 beats (1 compás, 3840 ticks) a 120 BPM vs 135 BPM dan
        // longitudes distintas; el Clip Editor debe usar el tempo real.
        let mut engine = test_engine();
        engine.transport.set_bpm(120.0);
        let len_120 = engine.transport.clip_length_frames(3840);
        engine.transport.set_bpm(135.0);
        let len_135 = engine.transport.clip_length_frames(3840);
        assert_ne!(len_120, len_135);
        // A 44100 Hz: 2s @120 vs 1.777s @135.
        assert_eq!(len_120, 88200);
        assert_eq!(len_135, 78400);
        assert_eq!(len_135, engine.transport.ticks_to_samples(3840));
    }

    /// REGLA DEL CLIP MANDA: clip de 9 compases sin `clip_loop_enabled`,
    /// con Time Selection de 15 compases loopeando el cursor. Del compás
    /// 10 al 15 la voz devuelve (0.0, 0.0): -inf dB y voz dormida.
    #[test]
    fn openlive_bar10_is_silent_with_15bar_time_selection() {
        let mut engine = test_engine();
        engine.set_mode(EngineMode::OpenLive);
        let spb = engine.transport.samples_per_bar();
        let bar_frames = spb.round() as u64;
        let nine_bars = bar_frames * 9;
        let fifteen_bars = bar_frames * 15;
        // Sample audible constante: cualquier re-emisión se detecta.
        engine.add_clip(
            1, 0, 0,
            vec![0.5f32; (nine_bars as usize) * 2],
            0.0, 0.0, 0.0, 2, false,
        );
        assert!(!engine.clips[0].has_valid_clip_loop());
        engine.set_global_loop(0, fifteen_bars, true);
        engine.trigger_clip(0, 0);
        engine.play();
        // Renderizar los 9 compases del clip (voz viva, se descarta).
        let mut remaining = nine_bars;
        while remaining > 0 {
            let step = remaining.min(512);
            run_frames(&mut engine, step);
            remaining -= step;
        }
        // Compás 10: un bloque envenenado con basura debe salir en 0.0.
        let mut raw = vec![0.777f32; 512 * 2];
        let mut buf = AudioBuffer::new(&mut raw);
        engine.process(&mut buf);
        let peak = raw.iter().fold(0.0f32, |m, &s| m.max(s.abs()));
        assert_eq!(peak, 0.0, "compás 10 debe mutar a (0.0, 0.0)");
        assert_eq!(engine.output_peak(), 0.0);
        assert_eq!(engine.output_db(), f32::NEG_INFINITY);
        assert!(
            !engine.clips[0].is_playing,
            "voz agotada sin clip-loop debe dormir aunque el cursor loopee"
        );
    }

    /// Time Selection MÁS CORTO que el clip (4 compases vs 9): el wrap del
    /// cursor no debe mantener la voz viva para siempre. A los 9 compases
    /// lineales la voz muere igual.
    #[test]
    fn openlive_short_time_selection_does_not_extend_voice() {
        let mut engine = test_engine();
        engine.set_mode(EngineMode::OpenLive);
        let spb = engine.transport.samples_per_bar();
        let bar_frames = spb.round() as u64;
        let nine_bars = bar_frames * 9;
        engine.add_clip(
            1, 0, 0,
            vec![0.5f32; (nine_bars as usize) * 2],
            0.0, 0.0, 0.0, 2, false,
        );
        engine.set_global_loop(0, bar_frames * 4, true);
        engine.trigger_clip(0, 0);
        engine.play();
        // El cursor jamás sale de los compases 1..4 (wrap), pero la voz
        // lineal se agota a los 9 compases: procesar 10 compases lineales.
        let mut remaining = bar_frames * 10;
        let mut last_peak = 1.0f32;
        while remaining > 0 {
            let step = remaining.min(512);
            let mut raw = vec![0.0f32; (step * 2) as usize];
            let mut buf = AudioBuffer::new(&mut raw);
            engine.process(&mut buf);
            last_peak = raw.iter().fold(0.0f32, |m, &s| m.max(s.abs()));
            remaining -= step;
        }
        assert_eq!(last_peak, 0.0, "voz lineal agotada: silencio aunque el cursor loopee");
        assert!(!engine.clips[0].is_playing);
        assert_eq!(engine.output_db(), f32::NEG_INFINITY);
    }

    /// Excepción explícita: CON `clip_loop_enabled` la voz sí sigue
    /// sonando pasado el compás 9 (el loop del clip manda).
    #[test]
    fn openlive_explicit_clip_loop_keeps_sounding_past_bar9() {
        let mut engine = test_engine();
        engine.set_mode(EngineMode::OpenLive);
        let spb = engine.transport.samples_per_bar();
        let bar_frames = spb.round() as u64;
        let nine_bars = bar_frames * 9;
        engine.add_clip(
            1, 0, 0,
            vec![0.5f32; (nine_bars as usize) * 2],
            0.0, 0.0, 0.0, 2, false,
        );
        // Loop individual de 1 compás al inicio del clip.
        let sr = engine.sample_rate;
        engine.set_clip_loop(0, 0, 0.0, (bar_frames as f32 / sr) as f32, true);
        assert!(engine.clips[0].has_valid_clip_loop());
        engine.set_global_loop(0, bar_frames * 15, true);
        engine.trigger_clip(0, 0);
        engine.play();
        // Renderizar 10 compases lineales: la voz con loop sigue viva.
        let mut remaining = bar_frames * 10;
        let mut last_peak = 0.0f32;
        while remaining > 0 {
            let step = remaining.min(512);
            let mut raw = vec![0.0f32; (step * 2) as usize];
            let mut buf = AudioBuffer::new(&mut raw);
            engine.process(&mut buf);
            last_peak = raw.iter().fold(0.0f32, |m, &s| m.max(s.abs()));
            remaining -= step;
        }
        // Con tanh() soft-clipper, el peak se satura suavemente.
        // tanh(0.5) ≈ 0.4621.
        let expected_peak = 0.5f32.tanh();
        assert!(
            (last_peak - expected_peak).abs() < 1e-4,
            "peak {} debe ser ≈ tanh(0.5) = {}",
            last_peak, expected_peak
        );
        assert!(engine.clips[0].is_playing);
    }

    /// Bug 1 (alineación): disparo de celda desde el compás 1 → el primer
    /// frame renderizado DEBE ser el sample 0 del clip (rampa distintiva
    /// para detectar cualquier offset). Vale para trigger-then-play,
    /// play-then-trigger a mitad de compás y re-disparo tras Stop.
    #[test]
    fn openlive_trigger_starts_at_sample_zero() {
        for flow in 0..3 {
            let mut engine = test_engine();
            engine.set_mode(EngineMode::OpenLive);
            let n = 8192u64;
            let mut samples = Vec::with_capacity((n * 2) as usize);
            for i in 0..n {
                let v = i as f32 / n as f32;
                samples.push(v);
                samples.push(v);
            }
            engine.add_clip(1, 0, 0, samples, 0.0, 0.0, 0.0, 2, false);
            match flow {
                // 0: trigger y después Play (pad y luego transporte).
                0 => {
                    engine.trigger_clip(0, 0);
                    engine.play();
                }
                // 1: Play primero y trigger a mitad del compás 3.
                1 => {
                    engine.play();
                    run_frames(&mut engine, 200_000);
                    engine.trigger_clip(0, 0);
                }
                // 2: ciclo Stop → trigger → Play de nuevo.
                _ => {
                    engine.trigger_clip(0, 0);
                    engine.play();
                    run_frames(&mut engine, 1000);
                    engine.stop();
                    engine.trigger_clip(0, 0);
                    engine.trigger_clip(0, 0);
                    engine.play();
                }
            }
            let mut raw = vec![0.0f32; 512 * 2];
            let mut buf = AudioBuffer::new(&mut raw);
            engine.process(&mut buf);
            for f in 0..512usize {
                let expect = f as f32 / n as f32;
                // Tolerancia ampliada para tanh() soft-clipper en la salida master.
                // tanh(x) ≈ x - x³/3 para valores pequeños; la máxima distorsión
                // en el rango de este test (0..0.125) es ~6.5e-4.
                assert!(
                    (raw[f * 2] - expect).abs() < 2e-3,
                    "flow {}: frame L{} = {} (esperado {})",
                    flow, f, raw[f * 2], expect
                );
                assert!(
                    (raw[f * 2 + 1] - expect).abs() < 2e-3,
                    "flow {}: frame R{} = {} (esperado {})",
                    flow, f, raw[f * 2 + 1], expect
                );
            }
        }
    }

    /// Bug 2 (cierre estricto): `current_frame >= total` sin loop → la voz
    /// pasa a Finished, el puntero se detiene y todo frame posterior es
    /// (0.0, 0.0), con -inf dB publicado.
    #[test]
    fn openlive_strict_close_past_native_length() {
        let mut engine = test_engine();
        engine.set_mode(EngineMode::OpenLive);
        let n = 4096u64;
        engine.add_clip(1, 0, 0, vec![0.5f32; (n * 2) as usize], 0.0, 0.0, 0.0, 2, false);
        engine.trigger_clip(0, 0);
        engine.play();
        // Renderizar EXACTAMENTE la longitud nativa.
        let mut remaining = n;
        while remaining > 0 {
            let step = remaining.min(512);
            run_frames(&mut engine, step);
            remaining -= step;
        }
        // Tres bloques más allá del final: silencio total y voz dormida.
        for _ in 0..3 {
            let mut raw = vec![0.777f32; 512 * 2];
            let mut buf = AudioBuffer::new(&mut raw);
            engine.process(&mut buf);
            let peak = raw.iter().fold(0.0f32, |m, &s| m.max(s.abs()));
            assert_eq!(peak, 0.0, "más allá del nativo debe ser (0.0, 0.0)");
        }
        assert!(!engine.clips[0].is_playing);
        assert_eq!(engine.output_db(), f32::NEG_INFINITY);
    }

    /// OpenStudio desde el compás 1: clip en tick 0 + cursor en 0 → el
    /// primer frame es el sample 0, y pasado el final timeline hay silencio.
    #[test]
    fn openstudio_bar1_starts_at_sample_zero_and_ends_silent() {
        let mut engine = test_engine();
        engine.set_mode(EngineMode::OpenStudio);
        let n = 4096u64;
        let mut samples = Vec::with_capacity((n * 2) as usize);
        for i in 0..n {
            let v = i as f32 / n as f32;
            samples.push(v);
            samples.push(v);
        }
        let duration_secs = n as f32 / engine.sample_rate;
        engine.add_clip(1, 0, 0, samples, 0.0, duration_secs, 0.0, 2, false);
        engine.transport.sample_count = 0;
        engine.play();
        let mut raw = vec![0.0f32; 512 * 2];
        let mut buf = AudioBuffer::new(&mut raw);
        engine.process(&mut buf);
        for f in 0..512usize {
            let expect = f as f32 / n as f32;
            // Tolerancia ampliada para tanh() soft-clipper.
            assert!(
                (raw[f * 2] - expect).abs() < 2e-3,
                "frame L{} = {} (esperado {})",
                f, raw[f * 2], expect
            );
        }
        // Agotar el clip y confirmar silencio posterior.
        let mut remaining = n;
        while remaining > 0 {
            let step = remaining.min(512);
            run_frames(&mut engine, step);
            remaining -= step;
        }
        let mut raw = vec![0.777f32; 512 * 2];
        let mut buf = AudioBuffer::new(&mut raw);
        engine.process(&mut buf);
        let peak = raw.iter().fold(0.0f32, |m, &s| m.max(s.abs()));
        assert_eq!(peak, 0.0);
        assert_eq!(engine.output_db(), f32::NEG_INFINITY);
    }

    /// El elapsed de la voz es lineal desde el trigger (0 al disparar),
    /// independiente del cursor global: base de la aguja del editor.
    #[test]
    fn voice_elapsed_frames_tracks_linear_voice_clock() {
        let mut engine = test_engine();
        engine.set_mode(EngineMode::OpenLive);
        engine.add_clip(1, 0, 0, vec![0.5f32; 8192 * 2], 0.0, 0.0, 0.0, 2, false);
        // Sin voz: None (fallback al cursor global).
        assert_eq!(engine.voice_elapsed_frames(0, 0), None);
        assert_eq!(engine.voice_elapsed_frames(9, 9), None);
        engine.trigger_clip(0, 0);
        // Recién disparada: 0 aunque el cursor global esté en otro lado.
        engine.transport.sample_count = 200_000;
        assert_eq!(engine.voice_elapsed_frames(0, 0), Some(0));
        engine.play();
        run_frames(&mut engine, 512);
        // Avanza con los frames renderizados, no con el cursor.
        assert_eq!(engine.voice_elapsed_frames(0, 0), Some(512));
        // Cursor con wrap del Time Selection no la mueve.
        engine.transport.sample_count = 0;
        assert_eq!(engine.voice_elapsed_frames(0, 0), Some(512));
    }
}