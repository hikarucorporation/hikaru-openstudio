use eframe::egui;

pub const DARK_DEF: egui::Color32 = egui::Color32::from_rgb(16, 18, 22);
pub const PANEL_BG: egui::Color32 = egui::Color32::from_rgb(24, 26, 32);
pub const BORDER_COLOR: egui::Color32 = egui::Color32::from_rgb(42, 45, 55);
pub const ACCENT_CYAN: egui::Color32 = egui::Color32::from_rgb(0, 240, 255);
pub const NEON_GREEN: egui::Color32 = egui::Color32::from_rgb(57, 255, 20);
pub const NEON_AMBER: egui::Color32 = egui::Color32::from_rgb(255, 183, 0);

pub fn setup_custom_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    
    visuals.panel_fill = DARK_DEF;
    visuals.window_fill = PANEL_BG;
    visuals.widgets.noninteractive.bg_fill = PANEL_BG;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, BORDER_COLOR);
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, egui::Color32::from_gray(180));
    
    visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(38, 41, 51);
    visuals.widgets.inactive.rounding = egui::Rounding::same(4.0);
    
    visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(48, 51, 61);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, ACCENT_CYAN);
    
    visuals.widgets.active.bg_fill = egui::Color32::from_rgb(58, 61, 71);
    visuals.selection.bg_fill = ACCENT_CYAN.linear_multiply(0.3);
    
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(10.0, 6.0);
    ctx.set_style(style);
}
