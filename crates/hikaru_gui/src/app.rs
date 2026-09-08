// Copyright (c) Hikaru Corporation - 2026
// GNU Affero General Public License v3
// Código fuente del App
// crates/hikaru_gui/src/app.rs

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use std::path::PathBuf;

use egui::{CentralPanel, Color32, RichText, ScrollArea, TopBottomPanel, ViewportBuilder, ViewportId};
use hikaru_audio_engine::AudioEngine;
use hikaru_core::SampleRate;
use hikaru_transport::{TransportPlaybackState, TransportPosition};

use crate::audio_proxy::{AudioProxy, GuiCommand};
use crate::views::{
    about, audio_settings, dsp_rack, explorer, footer, header, matrix, menu_bar, mixer, open_wavetable, playlist,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    OpenLive,
    OpenStudio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanMode {
    Stereo,
    MidSide,
}

pub struct HikaruApp {
    pub mode: AppMode,
    pub transport: TransportPosition,
    pub position_clock: Arc<AtomicU64>,
    /// Pico real de salida del engine (bits de f32, 0.0 == -inf dB).
    /// El vúmetro del mixer lee ESTO, no el fader: en compases sin audio
    /// marca -inf aunque el fader siga alto.
    pub output_level_bits: Arc<AtomicU32>,
    pub is_looping: bool,
    pub cpu_usage: f32,
    /// Último BPM propagado al motor vía `GuiCommand::SetBpm`.
    /// Evita spamear al engine cada frame: solo se envía al cambiar.
    bpm_synced_to_engine: f64,
    /// Último loop global propagado al motor vía `SetGlobalLoop`.
    /// Evita spamear al engine cada frame: solo se envía al cambiar.
    global_loop_synced_to_engine: Option<(bool, u64, u64)>,
    pub show_mixer: bool,
    pub show_dsp_rack: bool,
    pub show_about: bool,
    pub is_recording: bool,
    pub show_explorer: bool,
    pub explorer_state: explorer::FileExplorerState,

    pub audio_settings_state: audio_settings::AudioSettingsState,
    pub playlist_state: playlist::PlaylistState,
    pub matrix_state: matrix::SessionMatrixState,
    pub dragged_sample: Option<PathBuf>,

    pub live_tracks: Vec<mixer::Track>,
    pub studio_tracks: Vec<mixer::Track>,

    pub selected_track_index: usize,
    pub selected_slot_index: usize,
    fonts_configured: bool,

    pub audio_proxy: AudioProxy,
    pub _audio_stream: Option<cpal::Stream>,
    /// Handle compartido al engine para el polling por frame de la Session
    /// Matrix (`Playing` → `Stopped` cuando la voz termina). Se lee con
    /// `try_lock` para no bloquear el callback de audio.
    pub engine_handle: Option<Arc<Mutex<AudioEngine<'static>>>>,
}

impl HikaruApp {
    pub fn new(
        _cc: &eframe::CreationContext<'_>,
        audio_proxy: AudioProxy,
        audio_stream: Option<cpal::Stream>,
        position_clock: Arc<AtomicU64>,
        output_level_bits: Arc<AtomicU32>,
        engine_handle: Option<Arc<Mutex<AudioEngine<'static>>>>,
    ) -> Self {
        let sample_rate = SampleRate::new(44100.0);
        let transport = TransportPosition::new(sample_rate, 140.0);

        let mut live_tracks = vec![
            mixer::Track::new(0, "MASTER".to_string(), true),
            mixer::Track::new(1, "BASS 01".to_string(), false),
            mixer::Track::new(2, "DRUMS".to_string(), false),
        ];
        for t in &mut live_tracks {
            t.volume = 0.70;
        }

        let mut studio_tracks = vec![
            mixer::Track::new(0, "MASTER".to_string(), true),
            mixer::Track::new(1, "TRACK 01".to_string(), false),
        ];
        for t in &mut studio_tracks {
            t.volume = 0.70;
        }

        Self {
            mode: AppMode::OpenLive,
            transport,
            position_clock,
            output_level_bits,
            is_looping: false,
            cpu_usage: 0.12,
            show_mixer: false,
            show_dsp_rack: false,
            show_about: false,

            audio_settings_state: audio_settings::AudioSettingsState::default(),
            dragged_sample: None,

            show_explorer: false,
            explorer_state: explorer::FileExplorerState::default(),

            is_recording: false,

            playlist_state: playlist::PlaylistState::default(),
            matrix_state: matrix::SessionMatrixState::default(),

            live_tracks,
            studio_tracks,

            selected_track_index: 1,
            selected_slot_index: 0,
            fonts_configured: false,
            // FUGA #2 (BPM): arrancar en valor imposible (-1.0) para forzar
            // el primer `SetBpm` al engine en el primer frame. Con 140.0
            // inicial nunca se sincronizaba (engine arranca en 128.0) y el
            // cursor corría a un tempo y el audio a otro.
            bpm_synced_to_engine: -1.0,
            global_loop_synced_to_engine: None,
            audio_proxy,
            _audio_stream: audio_stream,
            engine_handle,
        }
    }

    /// Sincroniza el Sample Rate real del hardware con el transporte GUI.
    /// Debe llamarse tras `init_cpal_stream` (p. ej. 48000 Hz): sin esto la
    /// GUI seguiría calculando con 44100 y el cursor correría desincronizado.
    pub fn sync_hardware_sample_rate(&mut self, hardware_sr: f32) {
        if hardware_sr > 0.0 {
            self.transport.sample_rate = SampleRate::new(hardware_sr);
        }
    }

    pub fn current_bar(&self) -> f32 {
        // Bar 1-based con BPM activo + SR real + beats_per_bar del transporte.
        // Cálculo en f64 (el f32 solo al final para la UI).
        let samples_per_bar = self.transport.samples_per_bar();
        if samples_per_bar <= 0.0 {
            return 1.0;
        }
        (1.0 + self.transport.sample_count as f64 / samples_per_bar) as f32
    }

    /// Playhead exacto en ticks con el PPQN único del motor.
    /// Inversa exacta del tiempo real del audio: no pasa por `f32`.
    pub fn current_tick(&self) -> u64 {
        self.transport
            .samples_to_ticks(self.transport.sample_count)
    }
}

impl eframe::App for HikaruApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.fonts_configured {
            setup_custom_fonts(ctx);
            self.fonts_configured = true;
        }

        // --- LÓGICA DE TRANSPORTE Y REPRODUCCIÓN EN TIEMPO REAL ---
        // Siempre sincronizar el reloj del engine para que la posición esté
        // actualizada tanto en OpenStudio como en OpenLive (clips individuales).
        self.transport.sample_count = self.position_clock.load(Ordering::Relaxed);

        // PPQN único: la playlist nunca diverge del motor.
        // `transport.ppqn()` == `DEFAULT_PPQN` es la única fuente de verdad.
        self.playlist_state.ppqn = self.transport.ppqn();

        // BPM dinámico: si el header cambió el BPM de la GUI, propagarlo al
        // motor UNA vez (no cada frame) para que audio y cursor usen el mismo
        // tempo. Sin esto el cursor corre a un tempo y el audio a otro.
        // FUGA #2: el envío es sincrónico al cambio (antes del loop global,
        // que deriva `ticks_to_samples` del mismo BPM), así `clip_length`
        // en frames del engine coincide con los compases visuales.
        // Ej.: clip a 135 BPM vs proyecto a 120 BPM -> la duración en
        // frames se calcula con el tempo REAL del transporte
        // (`ticks_to_samples`), no con un tempo rancio del engine.
        if (self.transport.bpm - self.bpm_synced_to_engine).abs() > f64::EPSILON {
            self.bpm_synced_to_engine = self.transport.bpm;
            self.audio_proxy
                .send(GuiCommand::SetBpm(self.transport.bpm as f32));
        }

        // FUGA #1 (espejo GUI): si el transporte global está en Playing el
        // preview del explorer ya fue detenido en el engine (`play()->stop()`
        // + gate en `process()`). Apagar también el flag visual para que la
        // waveform no siga animando el playhead sobre el Master Mixer.
        if self.transport.playback_state == TransportPlaybackState::Playing
            && self.explorer_state.is_playing_preview
        {
            self.explorer_state.is_playing_preview = false;
            self.explorer_state.preview_position = 0.0;
        }

        if self.transport.playback_state == TransportPlaybackState::Playing {
            // Loop global: el ENGINE es el dueño único del wrap
            // (sample-accurate en `AudioEngine::process`, frame a frame).
            // La GUI solo configura la región y la envía UNA vez por cambio
            // vía `SetGlobalLoop`; nunca reescribe `sample_count` ni manda
            // `Seek` para loopear. El cursor sigue al engine vía
            // `position_clock`, así `loop_end_ticks` de la barra coincide
            // exactamente con el wrap del motor.
            //
            // OpenStudio: sin región válida se auto-selecciona
            // 0..fin_proyecto (mínimo 1 compás), estilo REAPER.
            // OpenLive: la playlist está vacía y NO debe inventarse un loop
            // de 1 compás (ese era el "reset en el compás 2"): solo hay loop
            // global si la región es explícita y válida; si no, se propaga
            // desactivado al engine y los clips loopean su duración real.
            let is_live = self.mode == AppMode::OpenLive;
            if self.is_looping && !is_live {
                let ppqn = self.playlist_state.ppqn.max(1);
                let mut start = self.playlist_state.loop_start_ticks;
                let mut end = self.playlist_state.loop_end_ticks;
                let len = end.saturating_sub(start);
                if !self.playlist_state.loop_region_active
                    || end <= start
                    || len < ppqn
                {
                    // Sin selección válida: estilo REAPER, 0..fin_proyecto.
                    let total = self.playlist_state.total_project_ticks();
                    start = 0u64;
                    end = total.max(4u64.saturating_mul(ppqn));
                }
                // Verifica el rango en ticks antes de enviar.
                if end <= start {
                    end = start.saturating_add(4u64.saturating_mul(ppqn));
                }
                if end.saturating_sub(start) < ppqn {
                    end = start.saturating_add(ppqn);
                }
                // SOLO confirma en la GUI si la región es válida.
                if end > start {
                    self.playlist_state.loop_start_ticks = start;
                    self.playlist_state.loop_end_ticks = end;
                    self.playlist_state.loop_region_active = true;
                    self.playlist_state.loop_preview_start_ticks = start;
                    self.playlist_state.loop_preview_end_ticks = end;
                }
            }
            ctx.request_repaint();
        }

        // Sincronización del loop global GUI -> engine (una vez por cambio).
        // Corre siempre (sonando o no) para que activar/desactivar 🔁 se
        // propague sin esperar al play.
        {
            let ppqn = self.playlist_state.ppqn.max(1);
            let (want_enabled, want_start, want_end) =
                if self.is_looping && self.playlist_state.is_loop_region_valid() {
                    let start = self.playlist_state.loop_start_ticks;
                    let mut end = self.playlist_state.loop_end_ticks;
                    if end <= start {
                        end = start.saturating_add(4u64.saturating_mul(ppqn));
                    }
                    if end.saturating_sub(start) < ppqn {
                        end = start.saturating_add(ppqn);
                    }
                    (end > start, start, end)
                } else {
                    (false, 0, 0)
                };
            let want_start_samples = self.transport.ticks_to_samples(want_start);
            let want_end_samples = self.transport.ticks_to_samples(want_end);
            let want = (want_enabled, want_start_samples, want_end_samples);
            if self.global_loop_synced_to_engine != Some(want) {
                self.global_loop_synced_to_engine = Some(want);
                // Espejo local para que la GUI lea lo mismo que el engine.
                self.transport
                    .set_loop_region_samples(want_start_samples, want_end_samples);
                self.transport.set_loop_enabled(want_enabled);
                self.audio_proxy.send(GuiCommand::SetGlobalLoop {
                    start_samples: want_start_samples,
                    end_samples: want_end_samples,
                    enabled: want_enabled,
                });
            }
        }

        // En OpenLive, pedir repaint continuo si hay clips disparados en la matriz
        if self.mode == AppMode::OpenLive {
            let any_clip_playing = self.matrix_state.grid.iter().flatten()
                .any(|slot| slot.state == matrix::SlotState::Playing);
            if any_clip_playing {
                ctx.request_repaint();
            }
        }

        ctx.input(|i| {
            if i.key_pressed(egui::Key::F9) {
                self.show_mixer = !self.show_mixer;
            }
            if i.key_pressed(egui::Key::F10) {
                self.show_dsp_rack = !self.show_dsp_rack;
            }
            if i.key_pressed(egui::Key::F11) {
                self.show_explorer = !self.show_explorer;
            }
        });

        TopBottomPanel::top("menu_bar_panel").resizable(false).show(ctx, |ui| {
            menu_bar::show(ui, self);
        });

        TopBottomPanel::top("header_panel").resizable(false).show(ctx, |ui| {
            ScrollArea::horizontal()
                .id_source("header_scroll_area")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    header::show(
                        ui,
                        &mut self.transport,
                        &self.position_clock,
                        &mut self.is_looping,
                        &mut self.is_recording,
                        &mut self.mode,
                        &mut self.show_mixer,
                        &mut self.show_dsp_rack,
                        &mut self.show_explorer,
                        &self.playlist_state,
                        &self.audio_proxy,
                    );
                });
        });

        TopBottomPanel::bottom("footer_panel").resizable(false).show(ctx, |ui| {
            footer::show(ui, self.cpu_usage);
        });

        CentralPanel::default().show(ctx, |ui| {
            // Clon barato del handle (Arc) para el polling por frame sin
            // pelear borrows con `matrix_state` dentro del closure.
            let engine_handle = self.engine_handle.clone();
            match self.mode {
                AppMode::OpenLive => {
                    // Tick exacto con PPQN único + BPM activo + SR real.
                    // Sin hardcodear 960 ni fórmulas manuales divergentes.
                    let ppqn = self.transport.ppqn();
                    let current_tick = self.current_tick();

                    matrix::show(
                        ui,
                        &mut self.matrix_state,
                        &mut self.dragged_sample,
                        &self.audio_proxy,
                        current_tick,
                        ppqn,
                        self.transport.bpm,          // <--- Arg 7: f64
                        self.transport.sample_rate.get() as u32, // <--- O .to_u32() / .0 dependiendo del enum
                        self.transport.sample_count,
                        // Loop global: la Session Matrix respeta la misma
                        // región `[loop_start_ticks, loop_end_ticks]` y la
                        // bandera `is_looping` (`loop_enabled`) que la
                        // Playlist; el wrap se aplica en el bloque de
                        // transporte (arriba) y en el display de matrix.rs.
                        self.is_looping,
                        self.playlist_state.loop_start_ticks,
                        self.playlist_state.loop_end_ticks,
                        engine_handle.as_ref(),
                    );
                }
                AppMode::OpenStudio => {
                    // ...
                    let mut current_bar = self.current_bar();
                    let transport_sample_count = self.transport.sample_count;
                    let beats_per_bar = self.transport.beats_per_bar;
                    // REAPER: pasar el estado del botón (`is_looping`), no el
                    // `transport.loop_enabled` ya saneado (que es false cuando
                    // aún no hay región). Así `ensure_minimum_global_loop`
                    // puede auto-seleccionar 0..fin_proyecto al activar el loop.
                    playlist::show(
                        ui,
                        &mut self.playlist_state,
                        &mut self.studio_tracks,
                        &mut current_bar,
                        &mut self.dragged_sample,
                        &self.audio_proxy,
                        self.transport.bpm,
                        self.transport.sample_rate.get() as u32,
                        self.is_looping,
                        transport_sample_count,
                        beats_per_bar,
                    );
                }
            }

            if let Some(ref sample_path) = self.dragged_sample {
                if let Some(pointer_pos) = ctx.pointer_latest_pos() {
                    egui::Area::new(egui::Id::new("drag_sample_preview"))
                        .fixed_pos(pointer_pos + egui::vec2(14.0, 14.0))
                        .order(egui::Order::Tooltip)
                        .interactable(false)
                        .show(ctx, |ui| {
                            egui::Frame::popup(ui.style())
                                .fill(Color32::from_rgb(20, 22, 28))
                                .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(0, 255, 255)))
                                .rounding(4.0)
                                .inner_margin(6.0)
                                .show(ui, |ui| {
                                    let name = sample_path.file_name().unwrap_or_default().to_string_lossy();
                                    ui.label(RichText::new(format!("🎵 {}", name)).size(11.0).color(Color32::WHITE));
                                });
                        });
                }
            }

            if self.show_explorer {
                egui::Window::new("File Explorer")
                    .resizable(true)
                    .collapsible(true)
                    .default_size([300.0, 400.0])
                    .drag_to_scroll(false)
                    .show(ctx, |ui| {
                        crate::views::explorer::show(
                            ui,
                            &mut self.explorer_state,
                            &mut self.dragged_sample,
                            &self.audio_proxy,
                            self.transport.bpm as f32,
                        );
                    });
            }
        });

        if self.show_mixer {
            // Nivel real del engine: 0.0 en silencio == -inf dB.
            let output_level =
                f32::from_bits(self.output_level_bits.load(Ordering::Relaxed));
            let output_level = if output_level.is_finite() { output_level } else { 0.0 };
            ctx.show_viewport_immediate(
                ViewportId::from_hash_of("hikaru_mixer_viewport"),
                ViewportBuilder::default()
                    .with_title("Hikaru Mixer")
                    .with_inner_size([800.0, 450.0])
                    .with_min_inner_size([300.0, 250.0]),
                |ctx, _class| {
                    CentralPanel::default().show(ctx, |ui| {
                        let active_tracks = match self.mode {
                            AppMode::OpenLive => &mut self.live_tracks,
                            AppMode::OpenStudio => &mut self.studio_tracks,
                        };
                        mixer::show(ui, active_tracks, &mut self.selected_track_index, &mut self.mode, output_level);
                    });
                },
            );
        }

        if self.show_dsp_rack {
            ctx.show_viewport_immediate(
                ViewportId::from_hash_of("hikaru_dsp_rack_viewport"),
                ViewportBuilder::default()
                    .with_title("DSP FX Rack")
                    .with_inner_size([400.0, 500.0])
                    .with_min_inner_size([250.0, 250.0]),
                |ctx, _class| {
                    CentralPanel::default().show(ctx, |ui| {
                        let active_tracks = match self.mode {
                            AppMode::OpenLive => &mut self.live_tracks,
                            AppMode::OpenStudio => &mut self.studio_tracks,
                        };
                        dsp_rack::show(
                            ui,
                            active_tracks,
                            self.selected_track_index,
                            &mut self.selected_slot_index,
                        );
                    });
                },
            );
        }

        let active_tracks = match self.mode {
            AppMode::OpenLive => &mut self.live_tracks,
            AppMode::OpenStudio => &mut self.studio_tracks,
        };

        for track in active_tracks.iter_mut() {
            for slot in track.effects.iter_mut() {
                if slot.name == "OpenWavetable" && slot.is_open {
                    let viewport_title = format!("OpenWavetable - TRK: {} (Slot {})", track.name, slot.id + 1);
                    let viewport_id = ViewportId::from_hash_of(&(track.id, slot.id, "open_wavetable_instance"));

                    ctx.show_viewport_immediate(
                        viewport_id,
                        ViewportBuilder::default()
                            .with_title(viewport_title)
                            .with_inner_size([750.0, 520.0])
                            .with_min_inner_size([350.0, 300.0]),
                        |ctx, _class| {
                            if ctx.input(|i| i.viewport().close_requested()) {
                                slot.is_open = false;
                            }

                            CentralPanel::default().show(ctx, |ui| {
                                open_wavetable::show(
                                    ui,
                                    &mut slot.wavetable_oscillators,
                                    &mut slot.modulators,
                                    &mut slot.cam_x,
                                    &mut slot.cam_y,
                                    &mut slot.cam_z,
                                );
                            });
                        },
                    );
                }
            }
        }

        if self.show_about {
            let about_title = match self.mode {
                AppMode::OpenLive => "About Hikaru OpenLive",
                AppMode::OpenStudio => "About Hikaru OpenStudio",
            };

            ctx.show_viewport_immediate(
                ViewportId::from_hash_of("hikaru_about_viewport"),
                ViewportBuilder::default()
                    .with_title(about_title)
                    .with_inner_size([380.0, 300.0])
                    .with_resizable(false),
                |ctx, _class| {
                    if ctx.input(|i| i.viewport().close_requested()) {
                        self.show_about = false;
                    }

                    CentralPanel::default().show(ctx, |ui| {
                        about::show(ui, self.mode);
                        audio_settings::show(ctx, &mut self.audio_settings_state);
                    });
                },
            );
        }
    }
}

use egui::{FontData, FontDefinitions, FontFamily};

fn setup_custom_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    fonts.font_data.insert(
        "Arimo-Regular".to_owned(),
        FontData::from_static(include_bytes!("../assets/fonts/Arimo/static/Arimo-Regular.ttf")),
    );

    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "Arimo-Regular".to_owned());

    ctx.set_fonts(fonts);
}