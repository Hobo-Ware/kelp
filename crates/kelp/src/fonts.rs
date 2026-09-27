use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

const PLEX_REGULAR: &[u8] = include_bytes!("../assets/fonts/IBMPlexSans-Regular.ttf");
const PLEX_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/IBMPlexSans-SemiBold.ttf");
const JETBRAINS_MONO: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf");

pub const SEMIBOLD: &str = "semibold";

const FALLBACKS: &[&str] = &[
    "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/noto/NotoSans-Regular.ttf",
];

static FALLBACK_INSTALLED: AtomicBool = AtomicBool::new(false);

pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(definitions(false));
}

pub fn needs_fallback(text: &str) -> bool {
    text.chars().any(|c| {
        let c = c as u32;
        c > 0x024F
            && !(0x2000..=0x2BFF).contains(&c)
            && !(0x1F000..=0x1FAFF).contains(&c)
            && c != 0xFE0F
    })
}

pub fn ensure_fallback(ctx: &egui::Context, text: &str) {
    if FALLBACK_INSTALLED.load(Ordering::Relaxed) || !needs_fallback(text) {
        return;
    }
    FALLBACK_INSTALLED.store(true, Ordering::Relaxed);
    ctx.set_fonts(definitions(true));
}

fn definitions(with_fallback: bool) -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let mut add = |name: &str, bytes: FontData| {
        fonts.font_data.insert(name.into(), Arc::new(bytes));
    };
    add("plex", FontData::from_static(PLEX_REGULAR));
    add("plex-semibold", FontData::from_static(PLEX_SEMIBOLD));
    add("jetbrains-mono", FontData::from_static(JETBRAINS_MONO));
    let fallback = with_fallback
        .then(|| FALLBACKS.iter().find_map(|path| std::fs::read(path).ok()))
        .flatten();
    if let Some(bytes) = fallback {
        add("fallback", FontData::from_owned(bytes));
    }

    let proportional = fonts.families.entry(FontFamily::Proportional).or_default();
    proportional.insert(0, "plex".into());
    let mut semibold = vec!["plex-semibold".to_string()];
    semibold.extend(proportional.iter().skip(1).cloned());
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, "jetbrains-mono".into());
    fonts
        .families
        .insert(FontFamily::Name(SEMIBOLD.into()), semibold);

    if fonts.font_data.contains_key("fallback") {
        for family in fonts.families.values_mut() {
            family.push("fallback".into());
        }
    }
    fonts
}

#[cfg(test)]
mod tests {
    use super::needs_fallback;

    #[test]
    fn latin_punctuation_and_emoji_do_not_need_the_fallback() {
        assert!(!needs_fallback(
            "feat(graph): draw merge curves — café ✔ 🚀"
        ));
        assert!(needs_fallback("\"button_label\": \"ابدأ\""));
        assert!(needs_fallback("日本語"));
    }
}
