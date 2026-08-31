use egui::{Ui, Grid, Vec2, Color32, Button, ScrollArea};

pub fn show(ui: &mut Ui, is_playing: bool) {
    ui.heading("SESSION MATRIX");
    ui.add_space(10.0);

    ScrollArea::both().show(ui, |ui| {
        Grid::new("clip_matrix_grid")
            .spacing(Vec2::new(8.0, 8.0))
            .show(ui, |ui| {
                for _row in 0..8 {
                    for col in 0..8 {
                        let base_color = if is_playing && col == 0 {
                            Color32::from_rgb(50, 150, 50)
                        } else {
                            Color32::from_gray(40)
                        };

                        let btn = Button::new("")
                            .fill(base_color)
                            .min_size(Vec2::new(60.0, 40.0));
                        
                        ui.add(btn);
                    }
                    ui.end_row();
                }
            });
    });
}
