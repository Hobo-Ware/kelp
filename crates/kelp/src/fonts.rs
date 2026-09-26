use std::sync::Arc;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

const FALLBACKS: &[&str] = &[
    "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/noto/NotoSans-Regular.ttf",
];

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    if let Some(bytes) = FALLBACKS.iter().find_map(|path| std::fs::read(path).ok()) {
        fonts
            .font_data
            .insert("fallback".into(), Arc::new(FontData::from_owned(bytes)));
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .push("fallback".into());
        }
    }
    ctx.set_fonts(fonts);
}
