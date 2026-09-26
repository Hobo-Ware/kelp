use eframe::egui::{self, Color32};

pub const BG: Color32 = Color32::from_rgb(0x15, 0x18, 0x1e);
pub const PANEL: Color32 = Color32::from_rgb(0x1a, 0x1d, 0x24);
pub const BORDER: Color32 = Color32::from_rgb(0x25, 0x29, 0x32);
pub const TEXT: Color32 = Color32::from_rgb(0xe8, 0xe6, 0xe1);
pub const ACCENT: Color32 = Color32::from_rgb(0x8f, 0xd1, 0x6a);

pub fn apply(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = PANEL;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = BG;
    visuals.override_text_color = Some(TEXT);
    visuals.widgets.noninteractive.bg_stroke.color = BORDER;
    visuals.selection.bg_fill = Color32::from_rgb(0x1f, 0x2c, 0x40);
    ctx.set_visuals(visuals);
}
