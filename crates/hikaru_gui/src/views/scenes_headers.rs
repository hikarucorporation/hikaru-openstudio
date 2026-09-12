use egui::*;

pub fn show(ui: &mut Ui, scenes_count: usize, active_scene: &mut Option<usize>) {
    ui.horizontal(|ui| {
        // Espacio reservado para alinear con la columna del Master a la izquierda
        ui.allocate_ui(Vec2::new(110.0, 24.0), |_| {});
        ui.separator();

        for i in 0..scenes_count {
            let is_selected = *active_scene == Some(i);
            let btn = Button::new(RichText::new(format!("▶ Scene {}", i + 1)).size(11.0))
                .min_size(Vec2::new(100.0, 24.0))
                .fill(if is_selected {
                    Color32::from_rgb(60, 90, 130)
                } else {
                    Color32::from_rgb(45, 45, 45)
                });

            if ui.add(btn).clicked() {
                *active_scene = Some(i);
            }
            ui.add_space(4.0);
        }
    });
}