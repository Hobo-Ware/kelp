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

pub fn semibold() -> egui::FontFamily {
    egui::FontFamily::Name(crate::fonts::SEMIBOLD.into())
}

pub fn generated_avatar(key: &str) -> (Color32, Color32) {
    let hue = AVATARS[kelp_core::avatar::color_index(key, AVATARS.len())];
    let mix = |bg: u8, fg: u8| (bg as f32 + (fg as f32 - bg as f32) * 0.2).round() as u8;
    let fill = Color32::from_rgb(
        mix(BG.r(), hue.r()),
        mix(BG.g(), hue.g()),
        mix(BG.b(), hue.b()),
    );
    (fill, hue)
}

pub fn lane(color: u8) -> Color32 {
    LANES[color as usize % LANES.len()]
}

pub fn with_alpha(c: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), alpha)
}

pub const POPUP: Color32 = Color32::from_rgb(0x1e, 0x23, 0x2c);
pub const POPUP_BORDER: Color32 = Color32::from_rgb(0x34, 0x3b, 0x48);
pub const CONTROL: Color32 = Color32::from_rgb(0x25, 0x2a, 0x33);
pub const CONTROL_HOVER: Color32 = Color32::from_rgb(0x2e, 0x35, 0x42);
pub const CONTROL_ACTIVE: Color32 = Color32::from_rgb(0x36, 0x3e, 0x4d);
pub const FIELD: Color32 = Color32::from_rgb(0x12, 0x15, 0x1a);
pub const MENU_HOVER: Color32 = Color32::from_rgb(0x2b, 0x34, 0x45);

pub fn apply(ctx: &egui::Context) {
    use egui::{CornerRadius, Margin, Shadow, Stroke, vec2};

    let radius = CornerRadius::same(4);
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = PANEL;
    visuals.window_fill = POPUP;
    visuals.window_stroke = Stroke::new(1.0, POPUP_BORDER);
    visuals.window_corner_radius = CornerRadius::same(10);
    visuals.menu_corner_radius = CornerRadius::same(10);
    visuals.window_shadow = Shadow {
        offset: [0, 14],
        blur: 36,
        spread: 0,
        color: Color32::from_black_alpha(120),
    };
    visuals.popup_shadow = Shadow {
        offset: [0, 10],
        blur: 28,
        spread: 0,
        color: Color32::from_black_alpha(110),
    };
    visuals.extreme_bg_color = FIELD;
    visuals.faint_bg_color = CONTROL;
    visuals.override_text_color = Some(TEXT);
    visuals.selection.bg_fill = with_alpha(ACCENT, 0x55);
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    visuals.hyperlink_color = ACCENT;
    visuals.text_cursor.stroke = Stroke::new(2.0, ACCENT);

    let w = &mut visuals.widgets;
    w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    w.noninteractive.corner_radius = radius;
    for (state, fill, stroke, text) in [
        (
            &mut w.inactive,
            CONTROL,
            Color32::from_rgb(0x3a, 0x41, 0x4d),
            Color32::from_rgb(0xd5, 0xd7, 0xdc),
        ),
        (
            &mut w.hovered,
            CONTROL_HOVER,
            Color32::from_rgb(0x44, 0x4c, 0x5a),
            TEXT_STRONG,
        ),
        (
            &mut w.active,
            CONTROL_ACTIVE,
            with_alpha(ACCENT, 0x99),
            TEXT_STRONG,
        ),
        (
            &mut w.open,
            CONTROL_HOVER,
            Color32::from_rgb(0x44, 0x4c, 0x5a),
            TEXT_STRONG,
        ),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::new(1.0, stroke);
        state.fg_stroke = Stroke::new(1.5, text);
        state.corner_radius = radius;
        state.expansion = 0.0;
    }
    ctx.set_visuals(visuals);

    ctx.global_style_mut(|style| {
        let s = &mut style.spacing;
        s.item_spacing = vec2(8.0, 6.0);
        s.button_padding = vec2(12.0, 6.0);
        s.interact_size = vec2(40.0, 28.0);
        s.menu_margin = Margin::same(6);
        s.menu_spacing = 2.0;
        s.window_margin = Margin::same(16);
        s.icon_width = 16.0;
        s.icon_spacing = 8.0;
        s.combo_width = 200.0;
        s.scroll = egui::style::ScrollStyle::floating();
        s.scroll.bar_width = 8.0;
        s.scroll.floating_allocated_width = 0.0;
    });
}
