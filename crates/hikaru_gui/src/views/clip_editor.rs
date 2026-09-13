// crates/hikaru_gui/src/views/clip_editor.rs
use egui::{Color32, Frame, Rect, Sense, Stroke, Ui, Vec2};
use crate::views::matrix::MatrixClip;
use crate::views::waveform;

pub fn show(
    ui: &mut Ui,
    clip: &mut MatrixClip,
    elapsed_frames: Option<u64>,
    sample_rate: u32,
) {
    Frame::none()
        .fill(Color32::from_rgb(18, 18, 22))
        .stroke(Stroke::new(1.0, Color32::from_gray(45)))
        .inner_margin(6.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("🎵 {}", clip.name));
                ui.weak(format!("({:.2}s)", clip.duration_secs));
            });

            ui.separator();

            let available_size = ui.available_size();
            let (rect, response) = ui.allocate_exact_size(available_size, Sense::click_and_drag());

            if ui.is_rect_visible(rect) {
                // 1. Fondo del canvas
                ui.painter().rect_filled(rect, 4.0, Color32::from_rgb(12, 12, 15));

                // 2. Renderizar waveform (Pasar pcm_data del sample guardado)
                // waveform::draw_waveform(ui, rect, &clip.pcm_data, Color32::from_rgb(0, 200, 255));

                // 3. Renderizar la aguja (Playhead) local basándose en el tiempo de la voz
                if let Some(frames) = elapsed_frames {
                    let total_frames = (clip.duration_secs * sample_rate as f64) as u64;
                    if total_frames > 0 {
                        // Módulo para que la aguja loopee limpia sin importar los ciclos
                        let local_frame = frames % total_frames;
                        let progress = local_frame as f32 / total_frames as f32;
                        let playhead_x = rect.min.x + (progress * rect.width());

                        ui.painter().line_segment(
                            [egui::pos2(playhead_x, rect.min.y), egui::pos2(playhead_x, rect.max.y)],
                            Stroke::new(2.0, Color32::from_rgb(0, 255, 255)),
                        );
                    }
                }

                // Borde exterior
                ui.painter().rect_stroke(rect, 4.0, Stroke::new(1.0, Color32::from_gray(50)));
            }
        });
}