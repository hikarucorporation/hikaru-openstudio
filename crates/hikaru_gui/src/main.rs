// Copyright (C) Hikaru Corporation - 2026
// GNU Affero General Public License v3
// Hikaru OpenStudio - Código fuente del Main
// crates/hikaru_gui/src/main.rs

use std::sync::mpsc::channel;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use hikaru_core::SampleRate;
use hikaru_audio_engine::EngineMode; // <--- Importamos EngineMode
use hikaru_gui::app::HikaruApp;
use hikaru_gui::audio_proxy::{AudioProxy, GuiCommand};

static DEFAULT_WAVETABLE: [f32; 2048] = [0.0; 2048];

fn main() -> Result<(), eframe::Error> {
    let (tx, rx) = channel();
    let audio_proxy = AudioProxy::new(tx);

    let sample_rate = SampleRate::new(44100.0);
    
    // 1. Instanciamos el reloj atómico compartido
    let position_clock = Arc::new(AtomicU64::new(0));

    // 2. Se lo pasamos a AudioEngine::new
    let engine = hikaru_audio_engine::AudioEngine::new(
        sample_rate, 
        &DEFAULT_WAVETABLE, 
        position_clock.clone()
    );
    let engine_arc = Arc::new(Mutex::new(engine));

    let engine_for_commands = engine_arc.clone();
    std::thread::spawn(move || {
        while let Ok(command) = rx.recv() {
            match command {
                // Sincronización del modo GUI -> Motor
                GuiCommand::SetAppMode(is_studio) => {
                    if let Ok(mut engine) = engine_for_commands.lock() {
                        if is_studio {
                            engine.set_mode(EngineMode::OpenStudio);
                        } else {
                            engine.set_mode(EngineMode::OpenLive);
                        }
                    }
                }
                GuiCommand::Play => {
                    if let Ok(mut engine) = engine_for_commands.lock() {
                        engine.play();
                    }
                }
                GuiCommand::Pause => {
                    if let Ok(mut engine) = engine_for_commands.lock() {
                        engine.pause();
                    }
                }
                GuiCommand::Stop => {
                    if let Ok(mut engine) = engine_for_commands.lock() {
                        engine.stop();
                    }
                }
                GuiCommand::Seek { sample_count } => {
                    if let Ok(mut engine) = engine_for_commands.lock() {
                        let secs = sample_count as f32 / engine.sample_rate;
                        engine.seek(secs);
                    }
                }
                GuiCommand::UpdateClipBounds { clip_id, track_index, scene_index, position_secs, duration_secs, offset_secs } => {
                    if let Ok(mut engine) = engine_for_commands.lock() {
                        engine.update_clip_bounds(clip_id, track_index, scene_index, position_secs, duration_secs, offset_secs);
                    }
                }
                GuiCommand::LoadClip { clip_id, path, position_secs, duration_secs, offset_secs, track_index, scene_index } => {
                    println!("[Hikaru Engine] Cargando clip de Matrix/Playlist: {} en Track {}", path, track_index);

                    if let Ok(mut reader) = hound::WavReader::open(&path) {
                        let spec = reader.spec();
                        let file_sr = spec.sample_rate as f32;
                        let channels = spec.channels as usize;

                        let raw_samples: Vec<f32> = match spec.sample_format {
                            hound::SampleFormat::Float => reader.samples::<f32>().filter_map(Result::ok).collect(),
                            hound::SampleFormat::Int => {
                                let max_val = (1 << (spec.bits_per_sample - 1)) as f32;
                                reader.samples::<i32>()
                                    .filter_map(Result::ok)
                                    .map(|s| s as f32 / max_val)
                                    .collect()
                            }
                        };

                        if let Ok(mut engine) = engine_for_commands.lock() {
                            let target_sr = engine.sample_rate;

                            let final_samples = if (file_sr - target_sr).abs() > 1.0 {
                                resample_linear(&raw_samples, channels, file_sr, target_sr)
                            } else {
                                raw_samples
                            };

                            engine.add_clip(
                                clip_id,
                                track_index,
                                scene_index,
                                final_samples,
                                position_secs,
                                duration_secs,
                                offset_secs,
                                channels,
                            );
                        }
                    } else {
                        eprintln!("[Hikaru Engine Error] No se pudo abrir el WAV: {}", path);
                    }
                }
                GuiCommand::PreviewSample { path, volume, speed: _ } => {
                    println!("[Hikaru Engine] Cargando preview: {}", path);

                    if let Ok(mut reader) = hound::WavReader::open(&path) {
                        let spec = reader.spec();
                        let file_sr = spec.sample_rate as f32;
                        let channels = spec.channels as usize;

                        let raw_samples: Vec<f32> = match spec.sample_format {
                            hound::SampleFormat::Float => reader.samples::<f32>().filter_map(Result::ok).collect(),
                            hound::SampleFormat::Int => {
                                let max_val = (1 << (spec.bits_per_sample - 1)) as f32;
                                reader.samples::<i32>()
                                    .filter_map(Result::ok)
                                    .map(|s| s as f32 / max_val)
                                    .collect()
                            }
                        };

                        if let Ok(mut engine) = engine_for_commands.lock() {
                            let target_sr = engine.sample_rate;

                            let final_samples = if (file_sr - target_sr).abs() > 1.0 {
                                resample_linear(&raw_samples, channels, file_sr, target_sr)
                            } else {
                                raw_samples
                            };

                            let sample_count = final_samples.len();
                            engine.preview_player.set_volume(volume);
                            engine.preview_player.play(final_samples);
                            // ── DIAGNÓSTICO TEMPORAL ──
                            eprintln!(
                                "[AUDIO DIAG] PreviewSample loaded: {} samples, vol={:.2}, transport={:?}, preview_playing={}",
                                sample_count, volume,
                                engine.transport.playback_state,
                                engine.preview_player.is_playing()
                            );
                        }
                    } else {
                        eprintln!("[Hikaru Engine Error] No se pudo abrir el archivo WAV: {}", path);
                    }
                }
                GuiCommand::StopPreview => {
                    if let Ok(mut engine) = engine_for_commands.lock() {
                        // ── DIAGNÓSTICO TEMPORAL ──
                        eprintln!(
                            "[AUDIO DIAG] StopPreview called, preview_was_playing={}",
                            engine.preview_player.is_playing()
                        );
                        // Corte categórico: stop() limpia buffer/posición/flag.
                        // `play(Vec::new())` dejaba la semántica ambigua.
                        engine.preview_player.stop();
                    }
                }
                GuiCommand::SetPreviewVolume(vol) => {
                    if let Ok(mut engine) = engine_for_commands.lock() {
                        engine.preview_player.set_volume(vol);
                    }
                }

                GuiCommand::SyncPlaylistClips { clips } => {
                    println!("[Hikaru Engine] Sincronizando {} clips de Playlist...", clips.len());

                    if let Ok(mut engine) = engine_for_commands.lock() {
                        engine.clips.clear();
                        let target_sr = engine.sample_rate;

                        for clip_data in clips.into_iter() {
                            if let Ok(mut reader) = hound::WavReader::open(&clip_data.path) {
                                let spec = reader.spec();
                                let file_sr = spec.sample_rate as f32;
                                let channels = spec.channels as usize;

                                let raw_samples: Vec<f32> = match spec.sample_format {
                                    hound::SampleFormat::Float => {
                                        reader.samples::<f32>().filter_map(Result::ok).collect()
                                    }
                                    hound::SampleFormat::Int => {
                                        let max_val = (1 << (spec.bits_per_sample - 1)) as f32;
                                        reader.samples::<i32>()
                                            .filter_map(Result::ok)
                                            .map(|s| s as f32 / max_val)
                                            .collect()
                                    }
                                };

                                let final_samples = if (file_sr - target_sr).abs() > 1.0 {
                                    resample_linear(&raw_samples, channels, file_sr, target_sr)
                                } else {
                                    raw_samples
                                };

                                engine.add_clip(
                                    clip_data.clip_id,
                                    clip_data.track_index, // slot real de la pista en el Playlist
                                    0,                      // scene_index: Playlist es lineal, no usa escenas
                                    final_samples,
                                    clip_data.start_secs,
                                    clip_data.duration_secs,
                                    clip_data.offset_secs,
                                    channels,
                                );
                            } else {
                                eprintln!("[Hikaru Engine Error] No se pudo abrir: {}", clip_data.path);
                            }
                        }
                    }
                }
                GuiCommand::TriggerClip { track_idx, scene_idx } => {
                    if let Ok(mut engine) = engine_for_commands.lock() {
                        engine.trigger_clip(track_idx, scene_idx);
                    }
                }
                GuiCommand::SetClipLoop { track_idx, scene_idx, loop_start_secs, loop_end_secs, enabled } => {
                    if let Ok(mut engine) = engine_for_commands.lock() {
                        engine.set_clip_loop(track_idx, scene_idx, loop_start_secs, loop_end_secs, enabled);
                    }
                }
                GuiCommand::SetGlobalLoop { start_samples, end_samples, enabled } => {
                    if let Ok(mut engine) = engine_for_commands.lock() {
                        engine.set_global_loop(start_samples, end_samples, enabled);
                    }
                }
                GuiCommand::TriggerScene { scene_idx } => {
                    if let Ok(mut engine) = engine_for_commands.lock() {
                        engine.trigger_scene(scene_idx);
                    }
                }
                GuiCommand::SetBpm(bpm) => {
                    if let Ok(mut engine) = engine_for_commands.lock() {
                        engine.transport.set_bpm(bpm as f64);
                    }
                }
                _ => {}
            }
        }
    });

    let audio_stream = match init_cpal_stream(engine_arc.clone()) {
        Ok(stream) => {
            println!("[Hikaru] Stream de audio CPAL iniciado correctamente.");
            Some(stream)
        }
        Err(e) => {
            eprintln!("[Hikaru Error] No se pudo iniciar el dispositivo de audio: {}", e);
            None
        }
    };

    // SR real del hardware (44100/48000/...): el engine ya se actualizó en
    // `init_cpal_stream`; la GUI debe usar el MISMO valor o el cursor corre
    // desincronizado del audio. Se lee del engine y se inyecta en el
    // transporte GUI al crear la app.
    let hardware_sr: f32 = engine_arc
        .lock()
        .map(|engine| engine.sample_rate)
        .unwrap_or(44100.0);

    // Pico real de salida para el vúmetro (0.0 == -inf dB en silencio).
    // Se clona el Arc del engine: el callback publica, la GUI solo lee.
    let output_level_bits = engine_arc
        .lock()
        .map(|engine| engine.output_level_bits.clone())
        .unwrap_or_else(|_| std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)));

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Hikaru OpenStudio")
            .with_inner_size([1280.0, 720.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Hikaru OpenStudio",
        native_options,
        Box::new(move |cc| {
            // Handle compartido al engine para el polling por frame de la
            // Session Matrix: la GUI lee `is_playing` / `VoiceState` con
            // `try_lock` y apaga el pad en verde al terminar la voz.
            let mut app = HikaruApp::new(
                cc,
                audio_proxy,
                audio_stream,
                position_clock,
                output_level_bits,
                Some(engine_arc.clone()),
            );
            // Transporte GUI con el SR real del motor (no 44100 fijo).
            app.sync_hardware_sample_rate(hardware_sr);
            Box::new(app)
        }),
    )
}

fn init_cpal_stream(
    engine: Arc<Mutex<hikaru_audio_engine::AudioEngine<'static>>>,
) -> Result<cpal::Stream, Box<dyn std::error::Error>> {
    // ── 1. Seleccionar host PulseAudio explícitamente ──
    // cpal::default_host() puede caer en ALSA directo en PipeWire,
    // provocando que la placa ignore las muestras.
    // PulseAudio es el backend que PipeWire emula, así que es el
    // que registra la app como "sink-input" en `pactl`.
    let hosts = cpal::available_hosts();
    println!("[Hikaru] Hosts disponibles: {:?}", hosts);

    let host = cpal::host_from_id(cpal::HostId::PulseAudio)
        .unwrap_or_else(|_| cpal::default_host());

    println!("[Hikaru] Host de audio: {}", host.id());

    // ── 1. Enumerar dispositivos de salida ──
    // El default de PipeWire/ALSA puede ser un sink dummy, HDMI
    // desconectado, o un monitor. Iteramos para encontrar la placa real.
    let devices: Vec<_> = host
        .output_devices()?
        .collect();

    println!("[Hikaru] Dispositivos de salida encontrados: {}", devices.len());
    for (i, d) in devices.iter().enumerate() {
        println!("  [{}] {}", i, d);
    }

    // Seleccionar el default; si es el único, usarlo directo.
    // Si hay varios, preferir el default pero loguear la lista.
    let device = host
        .default_output_device()
        .ok_or("No se encontró dispositivo de salida de audio")?;

    println!("[Hikaru] Dispositivo SELECCIONADO: {}", device);

    // ── 2. Configuración del dispositivo ──
    let supported_config = device.default_output_config()?;
    let device_format = supported_config.sample_format();
    let device_channels = supported_config.channels();
    let device_sr = supported_config.sample_rate();

    println!(
        "[Hikaru] Config del dispositivo: format={:?}, channels={}, sr={}, buffer={:?}",
        device_format, device_channels, device_sr,
        supported_config.buffer_size()
    );

    // Usar EXACTAMENTE la config soportada por el dispositivo.
    // No forzar SR ni buffer — el dispositivo sabe cuál es correcto.
    let stream_config: cpal::StreamConfig = supported_config.into();

    let hardware_sr = device_sr as f32;
    println!("[Hikaru] Hardware SR para engine: {}Hz", hardware_sr);

    if let Ok(mut lock) = engine.lock() {
        lock.set_sample_rate(hardware_sr);
    }

    // ── 3. Construir el stream ──
    let engine_cb = engine.clone();
    let stream = device.build_output_stream(
        stream_config,
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            data.fill(0.0);
            match engine_cb.try_lock() {
                Ok(mut lock) => {
                    let mut buffer = hikaru_core::AudioBuffer { samples: data };
                    lock.process(&mut buffer);
                }
                Err(_) => {
                    data.fill(0.0);
                }
            }
        },
        |err| {
            let err_str = err.to_string();
            if !err_str.contains("underrun") && !err_str.contains("overrun") {
                eprintln!("[Hikaru Error] CPAL stream error: {}", err);
            }
        },
        None,
    )?;

    stream.play()?;
    println!("[Hikaru] Stream CPAL: play() OK — {}", device);

    Ok(stream)
}

fn resample_linear(samples: &[f32], channels: usize, from_sr: f32, to_sr: f32) -> Vec<f32> {
    if samples.is_empty() || channels == 0 {
        return Vec::new();
    }

    let ratio = from_sr / to_sr;
    let input_frames = samples.len() / channels;
    let output_frames = ((input_frames as f32) / ratio) as usize;
    let mut output = Vec::with_capacity(output_frames * channels);

    for frame in 0..output_frames {
        let input_index = frame as f32 * ratio;
        let index_floor = input_index.floor() as usize;
        let index_ceil = (index_floor + 1).min(input_frames - 1);
        let t = input_index - index_floor as f32;

        for ch in 0..channels {
            let s1 = samples[index_floor * channels + ch];
            let s2 = samples[index_ceil * channels + ch];
            let interpolated = s1 + t * (s2 - s1);
            output.push(interpolated);
        }
    }

    output
}