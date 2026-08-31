// Copyright (C) Hikaru Corporation - 2026
// GNU Affero General Public License v3
// Hikaru OpenStudio - Código fuente del Main
// crates/hikaru_gui/src/main.rs

use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use hikaru_core::SampleRate;
use hikaru_gui::app::HikaruApp;
use hikaru_gui::audio_proxy::{AudioProxy, GuiCommand};

static DEFAULT_WAVETABLE: [f32; 2048] = [0.0; 2048];

fn main() -> Result<(), eframe::Error> {
    let (tx, rx) = channel();
    let audio_proxy = AudioProxy::new(tx);

    let sample_rate = SampleRate::new(44100.0);
    let engine = hikaru_audio_engine::AudioEngine::new(sample_rate, &DEFAULT_WAVETABLE);
    let engine_arc = Arc::new(Mutex::new(engine));

    let engine_for_commands = engine_arc.clone();
    std::thread::spawn(move || {
        while let Ok(command) = rx.recv() {
            match command {
                GuiCommand::Play => {
                    if let Ok(mut engine) = engine_for_commands.lock() {
                        // No hacemos seek(0.0) acá: si viene de un Pause, tiene que
                        // retomar desde donde estaba. El reseteo a 0 lo hace Stop().
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
                GuiCommand::LoadClip { path, position_secs, .. } => {
                    println!("[Hikaru Engine] Cargando clip de Playlist: {} en {:.2}s", path, position_secs);

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

                            // Insertamos el clip en la lista global del motor con su offset temporal real
                            engine.add_clip(final_samples, position_secs, channels);
                        }
                    } else {
                        eprintln!("[Hikaru Engine Error] No se pudo abrir el WAV para la Playlist: {}", path);
                    }
                }
                GuiCommand::PreviewSample { path, .. } => {
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

                            engine.clips.clear();
                            engine.add_clip(final_samples, 0.0, channels);
                            engine.seek(0.0);
                            engine.play();
                        }
                    } else {
                        eprintln!("[Hikaru Engine Error] No se pudo abrir el archivo WAV: {}", path);
                    }
                }

                GuiCommand::SyncPlaylistClips { clips } => {
                    println!("[Hikaru Engine] Sincronizando {} clips de Playlist...", clips.len());

                    if let Ok(mut engine) = engine_for_commands.lock() {
                        // 1. Limpiamos las voces anteriores para no acumular duplicados
                        engine.clips.clear();
                        let target_sr = engine.sample_rate;

                        // 2. Cargamos todos los clips actuales
                        for clip_data in clips {
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

                                // Insertamos cada clip con su `start_secs` correcto
                                engine.add_clip(final_samples, clip_data.start_secs, channels);
                            } else {
                                eprintln!("[Hikaru Engine Error] No se pudo abrir: {}", clip_data.path);
                            }
                        }
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
            Box::new(HikaruApp::new(cc, audio_proxy, audio_stream))
        }),
    )
}

fn init_cpal_stream(
    engine: Arc<Mutex<hikaru_audio_engine::AudioEngine<'static>>>,
) -> Result<cpal::Stream, Box<dyn std::error::Error>> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or("No se encontró dispositivo de salida de audio")?;

    let supported_config = device.default_output_config()?;
    let mut stream_config: cpal::StreamConfig = supported_config.into();

    stream_config.buffer_size = cpal::BufferSize::Fixed(2048);

    let hardware_sr = stream_config.sample_rate as f32;
    println!("[Hikaru] Hardware SR detectado: {}Hz", hardware_sr);

    // Ajustamos el sample rate del engine con la referencia al argumento `engine`
    if let Ok(mut lock) = engine.lock() {
        lock.set_sample_rate(hardware_sr);
    }

    let stream = device.build_output_stream(
        stream_config,
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            if let Ok(mut lock) = engine.lock() {
                let mut buffer = hikaru_core::AudioBuffer { samples: data };
                lock.process(&mut buffer);
            }
        },
        |err| {
            let err_str = err.to_string();
            if !err_str.contains("underrun") && !err_str.contains("overrun") {
                eprintln!("Error en stream de CPAL: {}", err);
            }
        },
        None,
    )?;

    stream.play()?;
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