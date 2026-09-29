use std::cell::Cell;
use std::sync::OnceLock;

use eframe::egui::{self, Color32};

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub dark: bool,
    pub bg: Color32,
    pub panel: Color32,
    pub border: Color32,
    pub header: Color32,
    pub chrome: Color32,
    pub text: Color32,
    pub text_strong: Color32,
    pub text_muted: Color32,
    pub text_faint: Color32,
    pub text_body: Color32,
    pub text_control: Color32,
    pub text_label: Color32,
    pub accent: Color32,
    pub on_accent: Color32,
    pub on_danger: Color32,
    pub danger: Color32,
    pub selected_row: Color32,
    pub sidebar_selected: Color32,
    pub modified: Color32,
    pub added: Color32,
    pub deleted: Color32,
    pub popup: Color32,
    pub popup_border: Color32,
    pub modal: Color32,
    pub modal_border: Color32,
    pub control: Color32,
    pub control_hover: Color32,
    pub control_active: Color32,
    pub control_stroke: Color32,
    pub control_stroke_hover: Color32,
    pub field: Color32,
    pub field_deep: Color32,
    pub menu_hover: Color32,
    pub separator: Color32,
    pub card: Color32,
    pub card_hover: Color32,
    pub card_active: Color32,
    pub toast: Color32,
    pub thread: Color32,
    pub inset: Color32,
    pub line_number: Color32,
    pub added_bg: Color32,
    pub removed_bg: Color32,
    pub added_emphasis: Color32,
    pub removed_emphasis: Color32,
    pub hunk_bg: Color32,
    pub hunk_text: Color32,
    pub wip_grey: Color32,
    pub band: Color32,
    pub checker_light: Color32,
    pub overlay: Color32,
    pub shadow: u8,
    pub backdrop: u8,
    pub lanes: [Color32; 8],
    pub avatars: [Color32; 8],
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

pub const DARK: Palette = Palette {
    dark: true,
    bg: rgb(0x1b, 0x1e, 0x25),
    panel: rgb(0x1f, 0x23, 0x2a),
    border: rgb(0x2c, 0x31, 0x3a),
    header: rgb(0x1d, 0x20, 0x27),
    chrome: rgb(0x0f, 0x11, 0x15),
    text: rgb(0xcd, 0xd0, 0xd5),
    text_strong: rgb(0xe1, 0xe3, 0xe6),
    text_muted: rgb(0x8f, 0x96, 0xa2),
    text_faint: rgb(0x72, 0x79, 0x86),
    text_body: rgb(0xb4, 0xb9, 0xc2),
    text_control: rgb(0xd5, 0xd7, 0xdc),
    text_label: rgb(0xc9, 0xcc, 0xd2),
    accent: rgb(0xa6, 0xd1, 0x89),
    on_accent: rgb(0x10, 0x13, 0x1a),
    on_danger: rgb(0x1a, 0x10, 0x12),
    danger: rgb(0xc2, 0x4a, 0x40),
    selected_row: rgb(0x26, 0x2e, 0x3b),
    sidebar_selected: rgb(0x25, 0x2e, 0x33),
    modified: rgb(0xe5, 0xb8, 0x7e),
    added: rgb(0x8f, 0xc9, 0xa8),
    deleted: rgb(0xe0, 0x8c, 0x8c),
    popup: rgb(0x23, 0x27, 0x30),
    popup_border: rgb(0x37, 0x3d, 0x49),
    modal: rgb(0x1f, 0x24, 0x2d),
    modal_border: rgb(0x3a, 0x42, 0x50),
    control: rgb(0x2a, 0x2f, 0x38),
    control_hover: rgb(0x32, 0x38, 0x43),
    control_active: rgb(0x3a, 0x41, 0x4d),
    control_stroke: rgb(0x3a, 0x41, 0x4d),
    control_stroke_hover: rgb(0x44, 0x4c, 0x5a),
    field: rgb(0x17, 0x1a, 0x20),
    field_deep: rgb(0x11, 0x13, 0x18),
    menu_hover: rgb(0x2f, 0x37, 0x45),
    separator: rgb(0x2c, 0x32, 0x3d),
    card: rgb(0x22, 0x26, 0x2e),
    card_hover: rgb(0x2b, 0x32, 0x40),
    card_active: rgb(0x2c, 0x33, 0x40),
    toast: rgb(0x23, 0x28, 0x33),
    thread: rgb(0x1e, 0x24, 0x2e),
    inset: rgb(0x20, 0x25, 0x2e),
    line_number: rgb(0x5e, 0x65, 0x73),
    added_bg: rgb(0x16, 0x30, 0x2a),
    removed_bg: rgb(0x3a, 0x1f, 0x22),
    added_emphasis: rgb(0x22, 0x55, 0x44),
    removed_emphasis: rgb(0x66, 0x2d, 0x33),
    hunk_bg: rgb(0x1a, 0x22, 0x30),
    hunk_text: rgb(0x8f, 0xb4, 0xe8),
    wip_grey: rgb(0x3a, 0x41, 0x50),
    band: rgb(0x20, 0x24, 0x2c),
    checker_light: rgb(0x27, 0x2c, 0x34),
    overlay: Color32::WHITE,
    shadow: 120,
    backdrop: 90,
    lanes: [
        rgb(0x7f, 0xc8, 0xbc),
        rgb(0xe8, 0xa0, 0x78),
        rgb(0xb9, 0x9c, 0xe0),
        rgb(0x86, 0xa9, 0xe6),
        rgb(0xe0, 0x9e, 0xc4),
        rgb(0xdc, 0xc3, 0x86),
        rgb(0x9c, 0xcc, 0x86),
        rgb(0xe0, 0x8a, 0x8a),
    ],
    avatars: [
        rgb(0xb9, 0xe2, 0xa4),
        rgb(0xf5, 0xc6, 0xa5),
        rgb(0xa9, 0xcd, 0xf2),
        rgb(0xe2, 0xc6, 0xf5),
        rgb(0xf2, 0xdf, 0x93),
        rgb(0x9f, 0xe0, 0xd3),
        rgb(0xf5, 0xb3, 0xc8),
        rgb(0xc9, 0xcf, 0xd8),
    ],
};

pub fn light() -> &'static Palette {
    static LIGHT: OnceLock<Palette> = OnceLock::new();
    LIGHT.get_or_init(|| {
        let f = flip;
        let d = DARK;
        Palette {
            dark: false,
            bg: rgb(0xf4, 0xf3, 0xef),
            panel: rgb(0xee, 0xed, 0xe8),
            border: rgb(0xda, 0xd8, 0xd0),
            header: rgb(0xeb, 0xea, 0xe4),
            chrome: rgb(0xe3, 0xe1, 0xda),
            text: rgb(0x3a, 0x3e, 0x45),
            text_strong: rgb(0x23, 0x26, 0x2c),
            text_muted: rgb(0x5f, 0x65, 0x6e),
            text_faint: rgb(0x78, 0x7d, 0x85),
            text_body: rgb(0x48, 0x4d, 0x55),
            text_control: rgb(0x33, 0x37, 0x3e),
            text_label: rgb(0x3f, 0x44, 0x4b),
            accent: rgb(0x4a, 0x7f, 0x2e),
            on_accent: rgb(0xfb, 0xfb, 0xf7),
            on_danger: rgb(0xfb, 0xfb, 0xf7),
            danger: rgb(0xb8, 0x3b, 0x31),
            selected_row: rgb(0xdd, 0xe5, 0xf0),
            sidebar_selected: rgb(0xdc, 0xe6, 0xe2),
            modified: rgb(0x9a, 0x60, 0x12),
            added: rgb(0x2b, 0x7a, 0x55),
            deleted: rgb(0xb0, 0x42, 0x42),
            popup: rgb(0xfb, 0xfa, 0xf7),
            popup_border: rgb(0xd2, 0xcf, 0xc6),
            modal: rgb(0xf8, 0xf7, 0xf3),
            modal_border: rgb(0xcc, 0xc9, 0xc0),
            control: rgb(0xe6, 0xe4, 0xde),
            control_hover: rgb(0xdd, 0xdb, 0xd4),
            control_active: rgb(0xd2, 0xd0, 0xc8),
            control_stroke: rgb(0xc9, 0xc6, 0xbd),
            control_stroke_hover: rgb(0xb9, 0xb6, 0xad),
            field: rgb(0xfb, 0xfa, 0xf8),
            field_deep: rgb(0xff, 0xff, 0xfd),
            menu_hover: rgb(0xe2, 0xe7, 0xef),
            separator: rgb(0xdd, 0xdb, 0xd4),
            card: rgb(0xf8, 0xf7, 0xf3),
            card_hover: rgb(0xe8, 0xe9, 0xe6),
            card_active: rgb(0xe1, 0xe4, 0xe6),
            toast: rgb(0xfd, 0xfc, 0xf9),
            thread: rgb(0xfa, 0xfa, 0xf7),
            inset: rgb(0xf9, 0xf8, 0xf4),
            line_number: rgb(0x98, 0x9c, 0xa3),
            added_bg: rgb(0xdf, 0xf1, 0xe4),
            removed_bg: rgb(0xf8, 0xe1, 0xe0),
            added_emphasis: rgb(0xb4, 0xe0, 0xc1),
            removed_emphasis: rgb(0xf0, 0xbf, 0xbc),
            hunk_bg: rgb(0xe4, 0xec, 0xf6),
            hunk_text: rgb(0x2f, 0x5c, 0x9a),
            wip_grey: rgb(0xb3, 0xb8, 0xc0),
            band: rgb(0xec, 0xeb, 0xe6),
            checker_light: rgb(0xe2, 0xe0, 0xda),
            overlay: Color32::BLACK,
            shadow: 38,
            backdrop: 40,
            lanes: d.lanes.map(|c| deepen(f(c))),
            avatars: d.avatars.map(|c| deepen(f(c))),
        }
    })
}

thread_local! {
    static LIGHT_ACTIVE: Cell<bool> = const { Cell::new(false) };
}

pub fn p() -> &'static Palette {
    if LIGHT_ACTIVE.get() { light() } else { &DARK }
}

pub fn is_dark() -> bool {
    !LIGHT_ACTIVE.get()
}

pub fn set_dark(dark: bool) -> bool {
    LIGHT_ACTIVE.replace(!dark) == dark
}

macro_rules! tokens {
    ($($name:ident),* $(,)?) => {
        $(
            pub fn $name() -> Color32 {
                p().$name
            }
        )*

        #[cfg(test)]
        fn named_tokens(palette: &Palette) -> Vec<(&'static str, Color32)> {
            vec![$((stringify!($name), palette.$name)),*]
        }
    };
}

tokens!(
    bg,
    panel,
    border,
    header,
    chrome,
    text,
    text_strong,
    text_muted,
    text_faint,
    text_body,
    text_control,
    text_label,
    accent,
    on_accent,
    on_danger,
    danger,
    selected_row,
    sidebar_selected,
    modified,
    added,
    deleted,
    popup,
    popup_border,
    modal,
    modal_border,
    control,
    control_hover,
    control_active,
    field,
    field_deep,
    menu_hover,
    separator,
    card,
    card_hover,
    card_active,
    toast,
    thread,
    inset,
    line_number,
    added_bg,
    removed_bg,
    added_emphasis,
    removed_emphasis,
    hunk_bg,
    hunk_text,
    wip_grey,
    band,
    checker_light,
);

pub fn lanes() -> [Color32; 8] {
    p().lanes
}

pub fn avatars() -> [Color32; 8] {
    p().avatars
}

pub fn backdrop() -> Color32 {
    Color32::from_black_alpha(p().backdrop)
}

pub fn overlay(alpha: u8) -> Color32 {
    with_alpha(p().overlay, alpha)
}

pub fn agent_brand(agent: kelp_core::agents::Agent) -> Color32 {
    use kelp_core::agents::Agent;
    match agent {
        Agent::Claude => rgb(0xD9, 0x77, 0x57),
        Agent::Codex => rgb(0x10, 0xA3, 0x7F),
        Agent::Gemini => rgb(0x4C, 0x8D, 0xF6),
        Agent::Copilot => rgb(0x8A, 0x8A, 0x8A),
        Agent::Aider => rgb(0xC2, 0x6B, 0xD1),
        Agent::Cursor => rgb(0xE6, 0xB4, 0x50),
    }
}

pub fn tone(dark: Color32) -> Color32 {
    if is_dark() { dark } else { flip(dark) }
}

pub fn semibold() -> egui::FontFamily {
    egui::FontFamily::Name(crate::fonts::SEMIBOLD.into())
}

pub fn generated_avatar(key: &str) -> (Color32, Color32) {
    let avatars = avatars();
    let hue = avatars[kelp_core::avatar::color_index(key, avatars.len())];
    let bg = bg();
    let mix = |bg: u8, fg: u8| (bg as f32 + (fg as f32 - bg as f32) * 0.2).round() as u8;
    let fill = Color32::from_rgb(
        mix(bg.r(), hue.r()),
        mix(bg.g(), hue.g()),
        mix(bg.b(), hue.b()),
    );
    (fill, hue)
}

pub fn lane(color: u8) -> Color32 {
    let lanes = lanes();
    lanes[color as usize % lanes.len()]
}

pub fn with_alpha(c: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), alpha)
}

fn to_linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn to_srgb(c: f32) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let s = if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (s * 255.0).round() as u8
}

fn oklab(c: Color32) -> [f32; 3] {
    let (r, g, b) = (to_linear(c.r()), to_linear(c.g()), to_linear(c.b()));
    let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

fn from_oklab([l, a, b]: [f32; 3], alpha: u8) -> Color32 {
    let l_ = (l + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
    let m_ = (l - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
    let s_ = (l - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
    let r = 4.076_741_7 * l_ - 3.307_711_6 * m_ + 0.230_969_94 * s_;
    let g = -1.268_438 * l_ + 2.609_757_4 * m_ - 0.341_319_38 * s_;
    let bl = -0.004_196_086_3 * l_ - 0.703_418_6 * m_ + 1.707_614_7 * s_;
    Color32::from_rgba_unmultiplied(to_srgb(r), to_srgb(g), to_srgb(bl), alpha)
}

fn flip(c: Color32) -> Color32 {
    let [l, a, b] = oklab(c);
    let flipped = (1.0 - (l - 0.2) * 0.95).clamp(0.0, 1.0);
    from_oklab([flipped, a, b], c.a())
}

fn deepen(c: Color32) -> Color32 {
    let [l, a, b] = oklab(c);
    from_oklab([l.min(0.56), a * 1.15, b * 1.15], c.a())
}

#[cfg(test)]
fn relative_luminance(c: Color32) -> f32 {
    0.2126 * to_linear(c.r()) + 0.7152 * to_linear(c.g()) + 0.0722 * to_linear(c.b())
}

#[cfg(test)]
fn contrast(a: Color32, b: Color32) -> f32 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

fn visuals_for(palette: &Palette) -> egui::Visuals {
    use egui::{CornerRadius, Shadow, Stroke};

    let radius = CornerRadius::same(4);
    let mut visuals = if palette.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.panel_fill = palette.panel;
    visuals.window_fill = palette.popup;
    visuals.window_stroke = Stroke::new(1.0, palette.popup_border);
    visuals.window_corner_radius = CornerRadius::same(10);
    visuals.menu_corner_radius = CornerRadius::same(10);
    visuals.window_shadow = Shadow {
        offset: [0, 14],
        blur: 36,
        spread: 0,
        color: Color32::from_black_alpha(palette.shadow),
    };
    visuals.popup_shadow = Shadow {
        offset: [0, 10],
        blur: 28,
        spread: 0,
        color: Color32::from_black_alpha(palette.shadow.saturating_sub(10)),
    };
    visuals.extreme_bg_color = palette.field;
    visuals.faint_bg_color = palette.control;
    visuals.override_text_color = Some(palette.text);
    visuals.selection.bg_fill = with_alpha(palette.accent, 0x55);
    visuals.selection.stroke = Stroke::new(1.0, palette.accent);
    visuals.hyperlink_color = palette.accent;
    visuals.text_cursor.stroke = Stroke::new(2.0, palette.accent);

    let w = &mut visuals.widgets;
    w.noninteractive.bg_fill = palette.panel;
    w.noninteractive.weak_bg_fill = palette.panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0, palette.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, palette.text);
    w.noninteractive.corner_radius = radius;
    for (state, fill, stroke, text) in [
        (
            &mut w.inactive,
            palette.control,
            palette.control_stroke,
            palette.text_control,
        ),
        (
            &mut w.hovered,
            palette.control_hover,
            palette.control_stroke_hover,
            palette.text_strong,
        ),
        (
            &mut w.active,
            palette.control_active,
            with_alpha(palette.accent, 0x99),
            palette.text_strong,
        ),
        (
            &mut w.open,
            palette.control_hover,
            palette.control_stroke_hover,
            palette.text_strong,
        ),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::new(1.0, stroke);
        state.fg_stroke = Stroke::new(1.5, text);
        state.corner_radius = radius;
        state.expansion = 0.0;
    }
    visuals
}

fn style_spacing(style: &mut egui::Style) {
    use egui::{Margin, vec2};

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
    style.interaction.tooltip_delay = 0.4;
}

pub fn apply(ctx: &egui::Context, preference: egui::ThemePreference) {
    ctx.set_visuals_of(egui::Theme::Dark, visuals_for(&DARK));
    ctx.set_visuals_of(egui::Theme::Light, visuals_for(light()));
    ctx.style_mut_of(egui::Theme::Dark, style_spacing);
    ctx.style_mut_of(egui::Theme::Light, style_spacing);
    ctx.set_theme(preference);
    sync(ctx);
}

pub fn sync(ctx: &egui::Context) {
    if set_dark(ctx.theme() == egui::Theme::Dark) {
        ctx.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_text_is_readable_on_light_surfaces() {
        let l = light();
        for surface in [l.bg, l.panel, l.popup, l.modal, l.field, l.card] {
            assert!(contrast(l.text, surface) >= 4.5, "text on {surface:?}");
            assert!(contrast(l.text_strong, surface) >= 4.5);
            assert!(contrast(l.text_body, surface) >= 4.5);
            assert!(
                contrast(l.text_muted, surface) >= 3.0,
                "muted on {surface:?}"
            );
            assert!(
                contrast(l.text_faint, surface) >= 3.0,
                "faint on {surface:?}"
            );
        }
        assert!(contrast(l.on_accent, l.accent) >= 4.5);
        assert!(contrast(l.on_danger, l.deleted) >= 4.5);
        assert!(contrast(DARK.on_danger, DARK.deleted) >= 4.5);
        assert!(contrast(l.text, l.selected_row) >= 4.5);
        assert!(contrast(l.text, l.added_bg) >= 4.5);
        assert!(contrast(l.text, l.removed_bg) >= 4.5);
        assert!(contrast(l.hunk_text, l.hunk_bg) >= 4.5);
        for c in [l.modified, l.added, l.deleted, l.accent, l.danger] {
            assert!(contrast(c, l.panel) >= 3.0, "{c:?}");
        }
    }

    #[test]
    fn light_lanes_stand_out_on_the_background() {
        let l = light();
        for (dark, lane) in DARK.lanes.iter().zip(l.lanes) {
            assert!(contrast(lane, l.bg) >= 3.0, "{lane:?}");
            let (hd, hl) = (oklab(*dark), oklab(lane));
            let hue = |c: [f32; 3]| c[2].atan2(c[1]);
            let delta = (hue(hd) - hue(hl)).abs();
            assert!(
                !(0.35..=std::f32::consts::TAU - 0.35).contains(&delta),
                "hue drift {delta}"
            );
        }
    }

    #[test]
    fn dark_keeps_the_existing_colors() {
        assert_eq!(DARK.bg, Color32::from_rgb(0x1b, 0x1e, 0x25));
        assert!(contrast(DARK.text, DARK.bg) >= 4.5);
    }

    #[test]
    fn switching_changes_what_tokens_return() {
        set_dark(false);
        assert_eq!(bg(), light().bg);
        assert_eq!(overlay(10).r(), 0);
        set_dark(true);
        assert_eq!(bg(), DARK.bg);
        assert_eq!(tone(DARK.bg), DARK.bg);
    }

    #[test]
    fn every_token_has_its_own_light_value() {
        let dark = named_tokens(&DARK);
        for ((name, d), (_, l)) in dark.iter().zip(named_tokens(light())) {
            assert_ne!(*d, l, "{name} is the same in both themes");
        }
    }

    #[test]
    fn views_take_colors_from_the_palette() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        for entry in std::fs::read_dir(src).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if ["theme.rs", "mascot.rs"].contains(&name.as_str()) {
                continue;
            }
            let Ok(code) = std::fs::read_to_string(&path) else {
                continue;
            };
            for (n, line) in code.lines().enumerate() {
                let wrapped = line.contains("theme::tone(");
                let traffic_light = ["0xff, 0x5f", "0xfe, 0xbc", "0x28, 0xc8"]
                    .iter()
                    .any(|c| line.contains(c));
                assert!(
                    !line.contains("Color32::from_rgb(") || wrapped || traffic_light,
                    "{name}:{} hardcodes a color: {line}",
                    n + 1
                );
            }
        }
    }

    #[test]
    fn switching_the_theme_changes_the_visuals() {
        let ctx = egui::Context::default();
        apply(&ctx, egui::ThemePreference::Dark);
        assert_eq!(ctx.global_style().visuals.panel_fill, DARK.panel);
        assert!(is_dark());
        ctx.set_theme(egui::ThemePreference::Light);
        sync(&ctx);
        assert_eq!(ctx.global_style().visuals.panel_fill, light().panel);
        assert!(!is_dark());
        assert_eq!(panel(), light().panel);
        set_dark(true);
    }

    #[test]
    fn flipping_turns_dark_surfaces_light_and_light_text_dark() {
        assert!(relative_luminance(flip(DARK.bg)) > 0.8);
        assert!(relative_luminance(flip(DARK.text)) < 0.2);
    }
}
