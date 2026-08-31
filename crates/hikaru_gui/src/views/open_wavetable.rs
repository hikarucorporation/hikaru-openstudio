/*
 * Hikaru OpenStudio - OpenWavetable GUI Canvas
 * License: AGPL-3.0-only
 */

use egui::{Ui, RichText, Color32, Sense, Pos2, Stroke, Vec2, CursorIcon, Slider, Frame, Rounding, Window, Id, ComboBox};
use std::f32::consts::PI;

const GRID_SIZE: f32 = 20.0;

#[derive(Clone, Debug)]
pub enum EffectModule {
    WarpBend(f32),
    WarpSync(f32),
    WarpPW(f32),
    WarpAsymmetry(f32),
    ModRing(f32),
    ModFM(f32),
    ModAM(f32),
    CrossfadeSmooth(f32),
}

impl EffectModule {
    pub fn name_and_range(&mut self) -> (&'static str, &mut f32, f32, f32) {
        match self {
            EffectModule::WarpBend(val) => ("Bend", val, -100.0, 100.0),
            EffectModule::WarpSync(val) => ("Sync", val, 0.0, 100.0),
            EffectModule::WarpPW(val) => ("PW", val, -50.0, 50.0),
            EffectModule::WarpAsymmetry(val) => ("Asym", val, -100.0, 100.0),
            EffectModule::ModRing(val) => ("Ring", val, 0.0, 100.0),
            EffectModule::ModFM(val) => ("FM", val, 0.0, 100.0),
            EffectModule::ModAM(val) => ("AM", val, 0.0, 100.0),
            EffectModule::CrossfadeSmooth(val) => ("Smooth", val, 0.0, 100.0),
        }
    }
}

#[derive(Clone, Debug)]
pub struct WavetableOscillator {
    pub id: usize,
    pub name: String,
    pub wt_pos: f32,
    pub pos: Pos2,
    pub size: Vec2,
    pub colors: Vec<Color32>,
}

impl WavetableOscillator {
    pub fn new(id: usize, name: &str, initial_pos: Pos2) -> Self {
        Self {
            id,
            name: name.to_string(),
            wt_pos: 0.0,
            pos: initial_pos,
            size: Vec2::new(320.0, 200.0),
            colors: vec![
                Color32::from_rgb(0, 255, 255),
                Color32::from_rgb(255, 0, 180),
            ],
        }
    }
}

fn evaluate_gradient(colors: &[Color32], t: f32) -> Color32 {
    if colors.is_empty() {
        return Color32::WHITE;
    }
    if colors.len() == 1 {
        return colors[0];
    }

    let t = t.clamp(0.0, 1.0);
    let num_segments = (colors.len() - 1) as f32;
    let scaled_t = t * num_segments;
    let index = (scaled_t.floor() as usize).min(colors.len() - 2);
    let local_t = scaled_t - index as f32;

    let c1 = colors[index];
    let c2 = colors[index + 1];

    let r = (c1.r() as f32 + (c2.r() as f32 - c1.r() as f32) * local_t) as u8;
    let g = (c1.g() as f32 + (c2.g() as f32 - c1.g() as f32) * local_t) as u8;
    let b = (c1.b() as f32 + (c2.b() as f32 - c1.b() as f32) * local_t) as u8;

    Color32::from_rgb(r, g, b)
}

#[derive(Clone, Debug)]
pub struct ModulatorNode {
    pub id: usize,
    pub target_osc_id: usize,
    pub effect: EffectModule,
    pub pos: Pos2,
}

impl ModulatorNode {
    pub fn new(id: usize, target_osc_id: usize, effect: EffectModule, pos: Pos2) -> Self {
        Self {
            id,
            target_osc_id,
            effect,
            pos,
        }
    }
}

fn snap_to_grid(val: f32, grid_size: f32) -> f32 {
    (val / grid_size).round() * grid_size
}

pub fn show(
    ui: &mut Ui, 
    oscillators: &mut Vec<WavetableOscillator>,
    modulators: &mut Vec<ModulatorNode>,
    cam_x: &mut f32,
    cam_y: &mut f32,
    cam_z: &mut f32,
) {
    ui.vertical(|ui| {
        ui.add_space(4.0);

        // --- 1. HEADER PRINCIPAL ---
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.label(RichText::new("3D VIEW:").strong().size(11.0).color(Color32::from_rgb(0, 255, 255)));
            ui.add_space(4.0);

            ui.label(RichText::new("X").small().strong().color(Color32::from_rgb(255, 100, 100)));
            ui.add(Slider::new(cam_x, -PI * 0.45..=PI * 0.45).show_value(false));

            ui.add_space(4.0);

            ui.label(RichText::new("Y").small().strong().color(Color32::from_rgb(100, 180, 100)));
            ui.add(Slider::new(cam_y, -100.0..=100.0).show_value(false));

            ui.add_space(4.0);

            ui.label(RichText::new("Z").small().strong().color(Color32::from_rgb(100, 180, 255)));
            ui.add(Slider::new(cam_z, -PI..=PI).show_value(false));

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button(RichText::new("[ + ] AÑADIR NODO").strong().color(Color32::from_rgb(0, 255, 150)), |ui| {
                    ui.set_min_width(200.0);
                    
                    ui.label(RichText::new("Generadores").small().strong().color(Color32::from_rgb(180, 180, 180)));
                    ui.separator();

                    if ui.button("+ Nuevo Oscilador Wavetable").clicked() {
                        let count = oscillators.len();
                        let next_letter = (b'A' + count as u8) as char;
                        let grid_pos = Pos2::new(
                            snap_to_grid(20.0 + (count as f32 * 30.0), GRID_SIZE),
                            snap_to_grid(50.0 + (count as f32 * 30.0), GRID_SIZE),
                        );
                        oscillators.push(WavetableOscillator::new(count, &format!("OSC {}", next_letter), grid_pos));
                        ui.close_menu();
                    }

                    ui.add_space(4.0);
                    ui.label(RichText::new("Moduladores Independientes").small().strong().color(Color32::from_rgb(180, 180, 180)));
                    ui.separator();

                    let target_id = oscillators.first().map(|o| o.id).unwrap_or(0);

                    let add_mod = |effect: EffectModule, mods: &mut Vec<ModulatorNode>| {
                        let count = mods.len();
                        let pos = Pos2::new(
                            snap_to_grid(360.0 + (count as f32 * 20.0), GRID_SIZE),
                            snap_to_grid(50.0 + (count as f32 * 20.0), GRID_SIZE),
                        );
                        mods.push(ModulatorNode::new(count, target_id, effect, pos));
                    };

                    if ui.button("Bend +/-").clicked() { add_mod(EffectModule::WarpBend(0.0), modulators); ui.close_menu(); }
                    if ui.button("Sync Hard").clicked() { add_mod(EffectModule::WarpSync(0.0), modulators); ui.close_menu(); }
                    if ui.button("Pulse Width (PW)").clicked() { add_mod(EffectModule::WarpPW(0.0), modulators); ui.close_menu(); }
                    if ui.button("FM Modulation").clicked() { add_mod(EffectModule::ModFM(30.0), modulators); ui.close_menu(); }
                });
            });
        });

        ui.add_space(4.0);
        ui.separator();

        // --- 2. CANVA CON GRILLA FONDO ---
        let canvas_rect = ui.available_rect_before_wrap();
        let painter = ui.painter_at(canvas_rect);

        painter.rect_filled(canvas_rect, 0.0, Color32::from_rgb(12, 13, 17));

        let start_x = (canvas_rect.min.x / GRID_SIZE).floor() * GRID_SIZE;
        let start_y = (canvas_rect.min.y / GRID_SIZE).floor() * GRID_SIZE;
        let grid_stroke = Stroke::new(1.0_f32, Color32::from_rgb(22, 25, 33));

        let mut x = start_x;
        while x < canvas_rect.max.x {
            painter.line_segment([Pos2::new(x, canvas_rect.min.y), Pos2::new(x, canvas_rect.max.y)], grid_stroke);
            x += GRID_SIZE;
        }

        let mut y = start_y;
        while y < canvas_rect.max.y {
            painter.line_segment([Pos2::new(canvas_rect.min.x, y), Pos2::new(canvas_rect.max.x, y)], grid_stroke);
            y += GRID_SIZE;
        }

        // --- 3. SUBVENTANAS: OSCILADORES ---
        let mut osc_to_remove = None;

        for (osc_idx, osc) in oscillators.iter_mut().enumerate() {
            let mut is_open = true;

            let win_response = Window::new(&osc.name)
                .id(Id::new(format!("osc_win_{}", osc.id)))
                .default_pos(osc.pos)
                .resizable(false)
                .collapsible(true)
                .open(&mut is_open)
                .frame(
                    Frame::window(ui.style())
                        .fill(Color32::from_rgb(18, 20, 26))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(45, 50, 65)))
                        .rounding(Rounding::same(6.0))
                )
                .show(ui.ctx(), |ui| {
                    ui.horizontal(|ui| {
                        let canvas_width = 210.0;
                        let canvas_height = 140.0;

                        let display_size = egui::vec2(canvas_width, canvas_height);
                        let (rect, _response) = ui.allocate_exact_size(display_size, Sense::hover());
                        let painter = ui.painter_at(rect);

                        painter.rect_filled(rect, 4.0, Color32::from_rgb(12, 12, 16));
                        painter.rect_stroke(rect, 4.0, Stroke::new(1.0_f32, Color32::from_rgb(35, 40, 50)));

                        let total_table_frames = 256.0;
                        let rendered_slices = 24;

                        let center_x = rect.center().x;
                        let center_y = rect.center().y - (*cam_y * 0.4);

                        let angle_pitch = *cam_x;
                        let cos_p = angle_pitch.cos();
                        let sin_p = angle_pitch.sin();

                        let angle_yaw = *cam_z;
                        let cos_y = angle_yaw.cos();
                        let sin_y = angle_yaw.sin();

                        let wave_width = 110.0;
                        let frame_depth_total = 90.0;
                        let height_scale = 32.0;

                        let project_3d = |x_norm: f32, y_val: f32, z_idx: f32| -> Pos2 {
                            let x_local = (x_norm - 0.5) * wave_width;
                            let z_local = (z_idx / (rendered_slices - 1) as f32 - 0.5) * frame_depth_total;
                            let y_local = y_val * height_scale;

                            let rot_x = x_local * cos_y - z_local * sin_y;
                            let rot_z = x_local * sin_y + z_local * cos_y;

                            let final_y = y_local * cos_p - rot_z * sin_p;
                            let final_z = y_local * sin_p + rot_z * cos_p;

                            Pos2::new(center_x + rot_x, center_y - final_y + (final_z * 0.25))
                        };

                        let active_slice = ((osc.wt_pos / (total_table_frames - 1.0)) * (rendered_slices - 1) as f32).round() as usize;

                        let draw_order: Vec<usize> = if cos_y >= 0.0 {
                            (0..rendered_slices).rev().collect()
                        } else {
                            (0..rendered_slices).collect()
                        };

                        for &z in &draw_order {
                            let z_f = z as f32;
                            let is_selected = z == active_slice;
                            let real_frame_morph = z_f / (rendered_slices - 1) as f32;

                            let points_count = 32;
                            let mut screen_points = Vec::with_capacity(points_count);

                            for i in 0..=points_count {
                                let norm_x = i as f32 / points_count as f32;
                                let phase = norm_x * PI * 2.0;

                                let wave = (phase).sin() * (1.0 - real_frame_morph) 
                                         + (phase * 3.0).sin() * 0.3 * (real_frame_morph * PI).sin()
                                         + (phase * 5.0).sin() * 0.2 * real_frame_morph;

                                screen_points.push(project_3d(norm_x, wave, z_f));
                            }

                            let (stroke_color, stroke_width) = if is_selected {
                                (Color32::WHITE, 2.2_f32)
                            } else {
                                (evaluate_gradient(&osc.colors, real_frame_morph), 0.9_f32)
                            };

                            for win in screen_points.windows(2) {
                                painter.line_segment([win[0], win[1]], Stroke::new(stroke_width, stroke_color));
                            }
                        }

                        ui.add_space(6.0);

                        // --- PANEL DERECHO: KNOB WT POS + GRADIENTE DINÁMICO ---
                        ui.allocate_ui_with_layout(
                            egui::vec2(60.0, canvas_height),
                            egui::Layout::top_down(egui::Align::Center),
                            |ui| {
                                ui.add_space(4.0);
                                ui.label(RichText::new("WT POS").small().color(Color32::from_rgb(180, 180, 180)));
                                ui.add_space(2.0);

                                let knob_radius = 16.0;
                                let knob_size = Vec2::splat(knob_radius * 2.0);
                                let (knob_rect, knob_response) = ui.allocate_exact_size(knob_size, Sense::click_and_drag());

                                if knob_response.dragged() {
                                    ui.output_mut(|o| o.cursor_icon = CursorIcon::ResizeVertical);
                                    let delta = -knob_response.drag_delta().y;
                                    osc.wt_pos = (osc.wt_pos + delta * 0.8).clamp(0.0, 255.0);
                                }

                                let knob_painter = ui.painter_at(knob_rect);
                                let center = knob_rect.center();

                                knob_painter.circle_filled(center, knob_radius, Color32::from_rgb(22, 24, 30));
                                knob_painter.circle_stroke(center, knob_radius, Stroke::new(1.5_f32, Color32::from_rgb(60, 65, 80)));
                                knob_painter.circle_filled(center, knob_radius - 3.0, Color32::from_rgb(32, 36, 46));

                                let angle_min = -PI * 0.75;
                                let angle_max = PI * 0.75;
                                let current_angle = angle_min + (osc.wt_pos / 255.0) * (angle_max - angle_min);

                                let pointer_pos = Pos2::new(
                                    center.x + current_angle.sin() * (knob_radius - 5.0),
                                    center.y - current_angle.cos() * (knob_radius - 5.0),
                                );
                                knob_painter.line_segment([center, pointer_pos], Stroke::new(2.5_f32, Color32::from_rgb(255, 110, 0)));

                                ui.add_space(2.0);
                                ui.label(RichText::new(format!("{:.0}", osc.wt_pos)).strong().size(11.0).color(Color32::from_rgb(0, 255, 255)));

                                ui.add_space(4.0);
                                ui.label(RichText::new("GRADIENT").size(8.0).color(Color32::GRAY));

                                ui.horizontal_wrapped(|ui| {
                                    let mut color_to_remove = None;
                                    let total_colors = osc.colors.len();

                                    for (c_idx, color) in osc.colors.iter_mut().enumerate() {
                                        ui.color_edit_button_srgba(color);

                                        if total_colors > 2 && ui.small_button("x").clicked() {
                                            color_to_remove = Some(c_idx);
                                        }
                                    }

                                    if let Some(idx) = color_to_remove {
                                        osc.colors.remove(idx);
                                    }

                                    if total_colors < 10 && ui.small_button("+").clicked() {
                                        let last_color = *osc.colors.last().unwrap_or(&Color32::WHITE);
                                        osc.colors.push(last_color);
                                    }
                                });
                            },
                        );
                    });
                });

            if let Some(res) = win_response {
                if res.response.drag_stopped() {
                    osc.pos = Pos2::new(
                        snap_to_grid(res.response.rect.min.x, GRID_SIZE),
                        snap_to_grid(res.response.rect.min.y, GRID_SIZE),
                    );
                }
            }

            if !is_open {
                osc_to_remove = Some(osc_idx);
            }
        }

        if let Some(idx) = osc_to_remove {
            oscillators.remove(idx);
        }

        // --- 4. SUBVENTANAS: MODULADORES INDEPENDIENTES ---
        let mut mod_to_remove = None;

        for (m_idx, mod_node) in modulators.iter_mut().enumerate() {
            let mut is_open = true;
            let (name, val, min_val, max_val) = mod_node.effect.name_and_range();

            let win_response = Window::new(format!("mod_win_{}", mod_node.id))
                .id(Id::new(format!("mod_win_{}", mod_node.id)))
                .default_pos(mod_node.pos)
                .fixed_size(egui::vec2(80.0, 80.0))
                .resizable(false)
                .collapsible(false)
                .title_bar(false)
                .frame(
                    Frame::window(ui.style())
                        .fill(Color32::from_rgb(24, 20, 16))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(80, 60, 30)))
                        .rounding(Rounding::same(4.0))
                        .inner_margin(2.0)
                )
                .show(ui.ctx(), |ui| {
                    ui.vertical_centered(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(name).small().strong().color(Color32::from_rgb(255, 180, 0)));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("×").clicked() {
                                    is_open = false;
                                }
                            });
                        });

                        ComboBox::from_id_source(format!("combo_target_{}", mod_node.id))
                            .width(65.0)
                            .selected_text(
                                oscillators
                                    .iter()
                                    .find(|o| o.id == mod_node.target_osc_id)
                                    .map(|o| o.name.as_str())
                                    .unwrap_or("---")
                            )
                            .show_ui(ui, |ui| {
                                for o in oscillators.iter() {
                                    ui.selectable_value(&mut mod_node.target_osc_id, o.id, &o.name);
                                }
                            });

                        let knob_radius = 12.0;
                        let knob_size = Vec2::splat(knob_radius * 2.0);
                        let (knob_rect, knob_response) = ui.allocate_exact_size(knob_size, Sense::click_and_drag());

                        if knob_response.dragged() {
                            ui.output_mut(|o| o.cursor_icon = CursorIcon::ResizeVertical);
                            let delta = -knob_response.drag_delta().y;
                            let speed = (max_val - min_val) / 100.0;
                            *val = (*val + delta * speed).clamp(min_val, max_val);
                        }

                        let painter = ui.painter_at(knob_rect);
                        let center = knob_rect.center();

                        painter.circle_filled(center, knob_radius, Color32::from_rgb(22, 24, 30));
                        painter.circle_stroke(center, knob_radius, Stroke::new(1.5_f32, Color32::from_rgb(80, 65, 40)));
                        painter.circle_filled(center, knob_radius - 2.0, Color32::from_rgb(32, 36, 46));

                        let norm_val = (*val - min_val) / (max_val - min_val);
                        let angle_min = -PI * 0.75;
                        let angle_max = PI * 0.75;
                        let current_angle = angle_min + norm_val * (angle_max - angle_min);

                        let pointer_pos = Pos2::new(
                            center.x + current_angle.sin() * (knob_radius - 4.0),
                            center.y - current_angle.cos() * (knob_radius - 4.0),
                        );
                        painter.line_segment([center, pointer_pos], Stroke::new(2.0_f32, Color32::from_rgb(255, 180, 0)));

                        ui.label(RichText::new(format!("{:.0}", *val)).size(9.0).color(Color32::from_rgb(255, 180, 0)));
                    });
                });

            if let Some(res) = win_response {
                if res.response.drag_stopped() {
                    mod_node.pos = Pos2::new(
                        snap_to_grid(res.response.rect.min.x, GRID_SIZE),
                        snap_to_grid(res.response.rect.min.y, GRID_SIZE),
                    );
                }
            }

            if !is_open {
                mod_to_remove = Some(m_idx);
            }
        }

        if let Some(idx) = mod_to_remove {
            modulators.remove(idx);
        }
    });
}