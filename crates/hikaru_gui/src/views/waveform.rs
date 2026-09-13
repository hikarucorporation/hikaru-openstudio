// crates/hikaru_gui/src/views/waveform.rs
use egui::{Color32, Rect, Stroke, Ui};

pub fn draw_waveform(ui: &mut Ui, rect: Rect, pcm_data: &[f32], color: Color32) {
    if pcm_data.is_empty() || rect.width() <= 0.0 {
        return;
    }

    let painter = ui.painter_at(rect);
    let center_y = rect.center().y;
    let half_height = rect.height() / 2.0;
    let width_px = rect.width() as usize;

    // Sub-muestreo rápido para dibujar los picos dentro del ancho en píxeles
    let samples_per_pixel = (pcm_data.len() / width_px).max(1);

    for x_idx in 0..width_px {
        let start_sample = x_idx * samples_per_pixel;
        let end_sample = (start_sample + samples_per_pixel).min(pcm_data.len());

        let mut min_val = 0.0f32;
        let mut max_val = 0.0f32;

        for &sample in &pcm_data[start_sample..end_sample] {
            if sample < min_val { min_val = sample; }
            if sample > max_val { max_val = sample; }
        }

        let x_pos = rect.min.x + x_idx as f32;
        let y_min = center_y + (min_val * half_height);
        let y_max = center_y + (max_val * half_height);

        painter.line_segment(
            [egui::pos2(x_pos, y_min), egui::pos2(x_pos, y_max)],
            Stroke::new(1.0, color),
        );
    }
}