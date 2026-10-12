use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use egui::{self, FontData, FontDefinitions, FontFamily};

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

/// Registers the bundled fonts, including the `semibold` family. Call once at startup.
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(definitions(None));
}

fn char_needs_fallback(c: char) -> bool {
    let c = c as u32;
    c > 0x024F
        && !(0x2000..=0x2BFF).contains(&c)
        && !(0x1F000..=0x1FAFF).contains(&c)
        && c != 0xFE0F
}

fn first_uncovered(text: &str) -> Option<char> {
    text.chars().find(|&c| char_needs_fallback(c))
}

pub fn needs_fallback(text: &str) -> bool {
    first_uncovered(text).is_some()
}

fn fontconfig_font(missing: char) -> Option<Vec<u8>> {
    let output = Command::new("fc-match")
        .args(["-f", "%{file}", &format!(":charset={:x}", missing as u32)])
        .output()
        .ok()?;
    let path = String::from_utf8(output.stdout).ok()?;
    output
        .status
        .success()
        .then(|| std::fs::read(path.trim()).ok())
        .flatten()
}

fn listed_font() -> Option<Vec<u8>> {
    FALLBACKS.iter().find_map(|path| std::fs::read(path).ok())
}

fn system_fallback(text: &str) -> Option<Vec<u8>> {
    if cfg!(target_os = "macos") {
        listed_font()
    } else {
        first_uncovered(text)
            .and_then(fontconfig_font)
            .or_else(listed_font)
    }
}

/// Adds a system font with wide Unicode coverage the first time `text` needs one.
pub fn ensure_fallback(ctx: &egui::Context, text: &str) {
    if FALLBACK_INSTALLED.load(Ordering::Relaxed) || !needs_fallback(text) {
        return;
    }
    FALLBACK_INSTALLED.store(true, Ordering::Relaxed);
    ctx.set_fonts(definitions(system_fallback(text)));
}

fn definitions(fallback: Option<Vec<u8>>) -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let mut add = |name: &str, bytes: FontData| {
        fonts.font_data.insert(name.into(), Arc::new(bytes));
    };
    add("plex", FontData::from_static(PLEX_REGULAR));
    add("plex-semibold", FontData::from_static(PLEX_SEMIBOLD));
    add("jetbrains-mono", FontData::from_static(JETBRAINS_MONO));
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
    use super::{first_uncovered, needs_fallback};

    #[test]
    fn latin_punctuation_and_emoji_do_not_need_the_fallback() {
        assert!(!needs_fallback(
            "feat(graph): draw merge curves — café ✔ 🚀"
        ));
        assert!(needs_fallback("\"button_label\": \"ابدأ\""));
        assert!(needs_fallback("日本語"));
    }

    #[test]
    fn the_first_character_outside_the_bundled_fonts_picks_the_system_font() {
        assert_eq!(first_uncovered("café 日本語"), Some('日'));
        assert_eq!(first_uncovered("café ✔"), None);
    }
}
