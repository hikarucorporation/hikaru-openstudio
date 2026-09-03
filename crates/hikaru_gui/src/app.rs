// Copyright (c) Hikaru Corporation - 2026
// GNU Affero General Public License v3
// Código fuente del App
// crates/hikaru_gui/src/app.rs

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use std::path::PathBuf;

use egui::{CentralPanel, Color32, RichText, ScrollArea, TopBottomPanel, ViewportBuilder, ViewportId};
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
    pub is_looping: bool,
    pub cpu_usage: f32,
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
}

impl HikaruApp {
    pub fn new(
        _cc: &eframe::CreationContext<'_>,
        audio_proxy: AudioProxy,
        audio_stream: Option<cpal::Stream>,
        position_clock: Arc<AtomicU64>,
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
            audio_proxy,
            _audio_stream: audio_stream,
        }
    }

    pub fn current_bar(&self) -> f32 {
        let samples_per_second = self.transport.sample_rate.get() as f64;
        if samples_per_second == 0.0 {
            return 1.0;
        }
        let seconds = self.transport.sample_count as f64 / samples_per_second;
        let beats = seconds * (self.transport.bpm as f64 / 60.0);
        (beats / 4.0) as f32 + 1.0
    }
}

impl eframe::App for HikaruApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.fonts_configured {
            setup_custom_fonts(ctx);
            self.fonts_configured = true;
        }

        // --- LÓGICA DE TRANSPORTE Y REPRODUCCIÓN EN TIEMPO REAL ---
        if self.transport.playback_state == TransportPlaybackState::Playing {
            // 1. Sincronización exacta con el reloj del AudioEngine
            self.transport.sample_count = self.position_clock.load(Ordering::Relaxed);

            // 2. Control de Loop si está activo
            if self.is_looping {
                let samples_per_beat = (self.transport.sample_rate.get() as f64 * 60.0) / self.transport.bpm;
                let samples_per_bar = samples_per_beat * self.transport.beats_per_bar as f64;
                let loop_end_sample = (samples_per_bar * 16.0) as u64;

                if self.transport.sample_count >= loop_end_sample {
                    self.transport.sample_count = 0;
                    self.audio_proxy.send(GuiCommand::Seek { sample_count: 0 });
                }
            }

            // Pedimos a egui redibujar el próximo frame para mantener animado el cursor
            ctx.request_repaint();
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
            match self.mode {
                AppMode::OpenLive => {
                    let ppqn = 960u64;
                    let sample_rate = self.transport.sample_rate.get() as f64;
                    let current_tick = if sample_rate > 0.0 {
                        let seconds_per_beat = 60.0 / self.transport.bpm;
                        let seconds_per_tick = seconds_per_beat / ppqn as f64;
                        let current_seconds = self.transport.sample_count as f64 / sample_rate;
                        (current_seconds / seconds_per_tick) as u64
                    } else {
                        0
                    };

                    matrix::show(
                        ui,
                        &mut self.matrix_state,
                        &mut self.dragged_sample,
                        &self.audio_proxy,
                        current_tick,
                        ppqn,
                        self.transport.bpm,          // <--- Arg 7: f64
                        self.transport.sample_rate.get() as u32, // <--- O .to_u32() / .0 dependiendo del enum
                    );
                }
                AppMode::OpenStudio => {
                    // ...
                    let mut current_bar = self.current_bar();
                    playlist::show(
                        ui,
                        &mut self.playlist_state,
                        &mut self.studio_tracks,
                        &mut current_bar,
                        &mut self.dragged_sample,
                        &self.audio_proxy,
                        self.transport.bpm,
                        self.transport.sample_rate.get() as u32,
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
                        mixer::show(ui, active_tracks, &mut self.selected_track_index, &mut self.mode);
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