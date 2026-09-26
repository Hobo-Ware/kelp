use eframe::egui::{self, Color32};

pub const BG: Color32 = Color32::from_rgb(0x15, 0x18, 0x1e);
pub const PANEL: Color32 = Color32::from_rgb(0x1a, 0x1d, 0x24);
pub const BORDER: Color32 = Color32::from_rgb(0x25, 0x29, 0x32);
pub const HEADER: Color32 = Color32::from_rgb(0x17, 0x1a, 0x20);
pub const TEXT: Color32 = Color32::from_rgb(0xe8, 0xe6, 0xe1);
pub const TEXT_STRONG: Color32 = Color32::from_rgb(0xf3, 0xf1, 0xec);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x9a, 0xa1, 0xad);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(0x7d, 0x84, 0x91);
pub const ACCENT: Color32 = Color32::from_rgb(0x8f, 0xd1, 0x6a);
pub const SELECTED_ROW: Color32 = Color32::from_rgb(0x1f, 0x2c, 0x40);
pub const SIDEBAR_SELECTED: Color32 = Color32::from_rgb(0x1f, 0x2f, 0x33);

pub const MODIFIED: Color32 = Color32::from_rgb(0xf0, 0xb0, 0x60);
pub const ADDED: Color32 = Color32::from_rgb(0x5f, 0xd0, 0xa0);
pub const DELETED: Color32 = Color32::from_rgb(0xff, 0x8a, 0x80);

pub const LANES: [Color32; 8] = [
    Color32::from_rgb(0x2d, 0xd4, 0xbf),
    Color32::from_rgb(0xfb, 0x92, 0x3c),
    Color32::from_rgb(0xc0, 0x84, 0xfc),
    Color32::from_rgb(0x60, 0xa5, 0xfa),
    Color32::from_rgb(0xf4, 0x72, 0xb6),
    Color32::from_rgb(0xfa, 0xcc, 0x15),
    Color32::from_rgb(0x4a, 0xde, 0x80),
    Color32::from_rgb(0xf8, 0x71, 0x71),
];

pub const AVATARS: [Color32; 8] = [
    Color32::from_rgb(0xb9, 0xe2, 0xa4),
    Color32::from_rgb(0xf5, 0xc6, 0xa5),
    Color32::from_rgb(0xa9, 0xcd, 0xf2),
    Color32::from_rgb(0xe2, 0xc6, 0xf5),
    Color32::from_rgb(0xf2, 0xdf, 0x93),
    Color32::from_rgb(0x9f, 0xe0, 0xd3),
    Color32::from_rgb(0xf5, 0xb3, 0xc8),
    Color32::from_rgb(0xc9, 0xcf, 0xd8),
];
pub const AVATAR_INK: Color32 = Color32::from_rgb(0x16, 0x19, 0x1f);

pub fn lane(color: u8) -> Color32 {
    LANES[color as usize % LANES.len()]
}

pub fn with_alpha(c: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), alpha)
}

pub fn apply(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = PANEL;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = BG;
    visuals.override_text_color = Some(TEXT);
    visuals.widgets.noninteractive.bg_stroke.color = BORDER;
    visuals.selection.bg_fill = SELECTED_ROW;
    ctx.set_visuals(visuals);
}
