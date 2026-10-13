use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use egui::{self, FontData, FontDefinitions, FontFamily};

const PLEX_REGULAR: &[u8] = include_bytes!("../assets/fonts/IBMPlexSans-Regular.ttf");
const PLEX_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/IBMPlexSans-SemiBold.ttf");
const JETBRAINS_MONO: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf");

pub const SEMIBOLD: &str = "semibold";

const PLEX: &str = "plex";
const PLEX_SEMIBOLD_NAME: &str = "plex-semibold";
const JETBRAINS: &str = "jetbrains-mono";
const FALLBACK: &str = "fallback";

const FALLBACKS: &[&str] = &[
    "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/noto/NotoSans-Regular.ttf",
];

static FALLBACK_INSTALLED: AtomicBool = AtomicBool::new(false);

/// Replaces the context's fonts with the bundled ones, Plex first, including the `semibold`
/// family. Call once at startup. A host with its own fonts uses [`extend`] instead.
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

/// Adds a system font with wide Unicode coverage the first time `text` needs one. The font joins
/// the definitions the context already has, so a host's own fonts stay. Call it from inside a
/// frame.
pub fn ensure_fallback(ctx: &egui::Context, text: &str) {
    if FALLBACK_INSTALLED.load(Ordering::Relaxed) || !needs_fallback(text) {
        return;
    }
    FALLBACK_INSTALLED.store(true, Ordering::Relaxed);
    let mut fonts = ctx.fonts(|f| f.definitions().clone());
    if extend_fallback(&mut fonts, text) {
        ctx.set_fonts(fonts);
    }
}

/// Adds the system fallback font to the host's `fonts` when `text` needs one and `fonts` has none
/// yet, and returns whether it changed. Nothing else in `fonts` is touched, so the host decides
/// when to pass the result to `Context::set_fonts`.
pub fn extend_fallback(fonts: &mut FontDefinitions, text: &str) -> bool {
    if fonts.font_data.contains_key(FALLBACK) || !needs_fallback(text) {
        return false;
    }
    match system_fallback(text) {
        Some(bytes) => {
            attach_fallback(fonts, bytes);
            true
        }
        None => false,
    }
}

/// Adds the bundled fonts to the host's `fonts` and registers the `semibold` family the widgets
/// draw with. The host's own fonts and its `Proportional` and `Monospace` order are kept, so
/// kelp-ui text follows the host's fonts; `semibold` starts with the bundled semibold face and
/// falls back to the host's `Proportional` chain. Safe to call again.
pub fn extend(fonts: &mut FontDefinitions) {
    for (name, bytes) in [
        (PLEX, PLEX_REGULAR),
        (PLEX_SEMIBOLD_NAME, PLEX_SEMIBOLD),
        (JETBRAINS, JETBRAINS_MONO),
    ] {
        fonts
            .font_data
            .insert(name.into(), Arc::new(FontData::from_static(bytes)));
    }
    let mut semibold = vec![PLEX_SEMIBOLD_NAME.to_string()];
    semibold.extend(
        fonts
            .families
            .get(&FontFamily::Proportional)
            .into_iter()
            .flatten()
            .filter(|name| *name != PLEX)
            .cloned(),
    );
    fonts
        .families
        .insert(FontFamily::Name(SEMIBOLD.into()), semibold);
}

fn attach_fallback(fonts: &mut FontDefinitions, bytes: Vec<u8>) {
    fonts
        .font_data
        .insert(FALLBACK.into(), Arc::new(FontData::from_owned(bytes)));
    for family in fonts.families.values_mut() {
        if !family.iter().any(|name| name == FALLBACK) {
            family.push(FALLBACK.into());
        }
    }
}

fn definitions(fallback: Option<Vec<u8>>) -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    extend(&mut fonts);
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, PLEX.into());
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, JETBRAINS.into());
    if let Some(bytes) = fallback {
        attach_fallback(&mut fonts, bytes);
    }
    fonts
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use egui::{FontData, FontDefinitions, FontFamily};

    use super::{
        FALLBACK, SEMIBOLD, attach_fallback, definitions, extend, extend_fallback, first_uncovered,
        needs_fallback,
    };

    fn host_fonts() -> FontDefinitions {
        let mut fonts = FontDefinitions::default();
        fonts
            .font_data
            .insert("host-sans".into(), Arc::new(FontData::from_static(b"host")));
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "host-sans".into());
        fonts.families.insert(
            FontFamily::Name("host-display".into()),
            vec!["host-sans".into()],
        );
        fonts
    }

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

    #[test]
    fn extend_keeps_the_hosts_fonts_and_adds_the_bundled_ones() {
        let before = host_fonts();
        let mut fonts = host_fonts();
        extend(&mut fonts);

        for (family, names) in &before.families {
            assert_eq!(fonts.families.get(family), Some(names), "{family:?}");
        }
        assert!(fonts.font_data.contains_key("host-sans"));
        for name in ["plex", "plex-semibold", "jetbrains-mono"] {
            assert!(fonts.font_data.contains_key(name), "{name}");
        }
        let semibold = &fonts.families[&FontFamily::Name(SEMIBOLD.into())];
        let mut expected = vec!["plex-semibold".to_string()];
        expected.extend(before.families[&FontFamily::Proportional].iter().cloned());
        assert_eq!(semibold, &expected);
    }

    #[test]
    fn extend_twice_gives_the_same_definitions() {
        let mut once = host_fonts();
        extend(&mut once);
        let mut twice = once.clone();
        extend(&mut twice);
        assert_eq!(once.families, twice.families);
        assert_eq!(once.font_data.len(), twice.font_data.len());
    }

    #[test]
    fn install_puts_the_bundled_fonts_first_and_semibold_follows_the_default_chain() {
        let fonts = definitions(None);
        let default = FontDefinitions::default();
        let proportional = &fonts.families[&FontFamily::Proportional];
        let mut chain = vec!["plex".to_string()];
        chain.extend(default.families[&FontFamily::Proportional].iter().cloned());
        assert_eq!(proportional, &chain);
        let mut mono = vec!["jetbrains-mono".to_string()];
        mono.extend(default.families[&FontFamily::Monospace].iter().cloned());
        assert_eq!(fonts.families[&FontFamily::Monospace], mono);
        let mut semibold = vec!["plex-semibold".to_string()];
        semibold.extend(proportional.iter().skip(1).cloned());
        assert_eq!(fonts.families[&FontFamily::Name(SEMIBOLD.into())], semibold);
    }

    #[test]
    fn the_fallback_joins_every_family_once() {
        let mut fonts = definitions(Some(b"fallback".to_vec()));
        for names in fonts.families.values() {
            assert_eq!(names.last().map(String::as_str), Some(FALLBACK));
        }
        attach_fallback(&mut fonts, b"fallback".to_vec());
        for names in fonts.families.values() {
            assert_eq!(names.iter().filter(|n| *n == FALLBACK).count(), 1);
        }
    }

    #[test]
    fn extend_fallback_leaves_the_hosts_fonts_alone_when_nothing_is_needed() {
        let mut fonts = host_fonts();
        extend(&mut fonts);
        let before = fonts.clone();
        assert!(!extend_fallback(&mut fonts, "plain latin text"));
        assert_eq!(fonts, before);

        attach_fallback(&mut fonts, b"fallback".to_vec());
        let with_fallback = fonts.clone();
        assert!(!extend_fallback(&mut fonts, "日本語"));
        assert_eq!(fonts, with_fallback);
    }
}
