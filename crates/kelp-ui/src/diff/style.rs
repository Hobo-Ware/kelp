use egui::Color32;
use kelp_core::diff::LineKind;

use crate::theme;

const EMBEDDED_ADD: Color32 = Color32::from_rgb(0x8f, 0xd1, 0x6a);
const EMBEDDED_DEL: Color32 = Color32::from_rgb(0xf8, 0x71, 0x71);
const EMBEDDED_EMPHASIS_ALPHA: u8 = 0x30;

/// How the diff body looks. `Default` is the Kelp app's look; [`DiffStyle::embedded`] is a
/// calmer one for a diff shown inside a card of a host app.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DiffStyle {
    /// The hue of added rows, drawn as a faint tint. `None` keeps Kelp's solid fill.
    pub add_tint: Option<Color32>,
    /// The hue of removed rows, drawn as a faint tint. `None` keeps Kelp's solid fill.
    pub del_tint: Option<Color32>,
    /// Alpha of the row tint, used only with a tint hue.
    pub tint_alpha: u8,
    /// Alpha of the bar down the left edge of a tinted row.
    pub bar_alpha: u8,
    /// Width of that bar in points. Zero draws none.
    pub bar_w: f32,
    /// Hunk headers become a quiet divider with a short label instead of the `@@` text.
    pub hunk_divider: bool,
    /// Thread cards, resolved chips and the composer use a small flat card.
    pub note_card: bool,
    /// Replaces Kelp's accent on the selection, the focus ring, the comment button and links.
    pub accent: Option<Color32>,
}

/// The colors of one code row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct LineLook {
    pub fill: Color32,
    pub bar: Option<Color32>,
    pub mark: &'static str,
    pub mark_color: Color32,
    pub number: Color32,
    pub emphasis: Color32,
    pub text: Color32,
}

impl DiffStyle {
    /// Faint green and red row tints with a 2 px bar, hunk headers as a divider, small note
    /// cards, and `accent` in place of Kelp's.
    pub fn embedded(accent: Color32) -> Self {
        Self {
            add_tint: Some(EMBEDDED_ADD),
            del_tint: Some(EMBEDDED_DEL),
            tint_alpha: 0x12,
            bar_alpha: 0x8c,
            bar_w: 2.0,
            hunk_divider: true,
            note_card: true,
            accent: Some(accent),
        }
    }

    pub(super) fn accent(&self) -> Color32 {
        self.accent.unwrap_or_else(theme::accent)
    }

    pub(super) fn note(&self, border: Color32) -> (Color32, Color32) {
        if self.note_card {
            (
                theme::tone(Color32::from_rgb(0x1a, 0x20, 0x29)),
                theme::tone(Color32::from_rgb(0x2a, 0x31, 0x3d)),
            )
        } else {
            (theme::thread(), border)
        }
    }

    pub(super) fn paint_accent(&self, ui: &mut egui::Ui) {
        if let Some(accent) = self.accent {
            let selection = &mut ui.visuals_mut().selection;
            selection.bg_fill = theme::with_alpha(accent, 0x55);
            selection.stroke = egui::Stroke::new(1.0, accent);
        }
    }

    pub(super) fn tinted(&self) -> bool {
        self.add_tint.is_some() || self.del_tint.is_some()
    }

    pub(super) fn look(&self, kind: LineKind) -> LineLook {
        let (tint, solid, mark, mark_color, emphasis) = match kind {
            LineKind::Added => (
                self.add_tint,
                theme::added_bg(),
                "+",
                theme::added(),
                theme::added_emphasis(),
            ),
            LineKind::Removed => (
                self.del_tint,
                theme::removed_bg(),
                "-",
                theme::deleted(),
                theme::removed_emphasis(),
            ),
            LineKind::Context => {
                return LineLook {
                    fill: Color32::TRANSPARENT,
                    bar: None,
                    mark: "",
                    mark_color: theme::text_faint(),
                    number: theme::line_number(),
                    emphasis: theme::added_emphasis(),
                    text: theme::text_body(),
                };
            }
        };
        let text = theme::text();
        let Some(hue) = tint else {
            return LineLook {
                fill: solid,
                bar: None,
                mark,
                mark_color,
                number: theme::line_number(),
                emphasis,
                text,
            };
        };
        LineLook {
            fill: theme::with_alpha(hue, self.tint_alpha),
            bar: (self.bar_w > 0.0).then(|| theme::with_alpha(hue, self.bar_alpha)),
            mark,
            mark_color: hue,
            number: hue.lerp_to_gamma(theme::line_number(), 0.5),
            emphasis: theme::with_alpha(hue, EMBEDDED_EMPHASIS_ALPHA),
            text,
        }
    }
}

/// The label of a hunk divider, from the numbers of a `@@ -a,b +c,d @@` header. `None` when the
/// header does not parse.
pub(super) fn hunk_label(header: &str) -> Option<String> {
    let inner = header.strip_prefix("@@ ")?;
    let (ranges, _) = inner.split_once(" @@")?;
    let (old, new) = ranges.split_once(' ')?;
    let (old_start, old_len) = range(old.strip_prefix('-')?)?;
    let (new_start, new_len) = range(new.strip_prefix('+')?)?;
    Some(if old_len == 0 && old_start == 0 {
        format!("new file · {}", span("lines", new_start, new_len))
    } else if new_len == 0 && new_start == 0 {
        format!("deleted file · {}", span("lines", old_start, old_len))
    } else if new_len == 0 {
        span("removed lines", old_start, old_len)
    } else {
        span("lines", new_start, new_len)
    })
}

fn range(text: &str) -> Option<(u32, u32)> {
    match text.split_once(',') {
        Some((start, len)) => Some((start.parse().ok()?, len.parse().ok()?)),
        None => Some((text.parse().ok()?, 1)),
    }
}

fn span(noun: &str, start: u32, len: u32) -> String {
    match len {
        0 | 1 => format!("{} {start}", noun.trim_end_matches('s')),
        _ => format!("{noun} {start} to {}", start + len - 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_style_has_no_embedded_knobs() {
        let style = DiffStyle::default();
        assert_eq!(style.add_tint, None);
        assert_eq!(style.del_tint, None);
        assert_eq!(style.bar_w, 0.0);
        assert_eq!(style.tint_alpha, 0);
        assert_eq!(style.bar_alpha, 0);
        assert!(!style.hunk_divider);
        assert!(!style.note_card);
        assert_eq!(style.accent, None);
        assert!(!style.tinted());
        assert_eq!(style.accent(), theme::accent());
    }

    #[test]
    fn the_default_rows_are_the_colors_kelp_always_used() {
        let style = DiffStyle::default();
        let added = style.look(LineKind::Added);
        assert_eq!(added.fill, theme::added_bg());
        assert_eq!(added.bar, None);
        assert_eq!((added.mark, added.mark_color), ("+", theme::added()));
        assert_eq!(added.emphasis, theme::added_emphasis());
        assert_eq!(added.number, theme::line_number());
        assert_eq!(added.text, theme::text());
        let removed = style.look(LineKind::Removed);
        assert_eq!(removed.fill, theme::removed_bg());
        assert_eq!((removed.mark, removed.mark_color), ("-", theme::deleted()));
        assert_eq!(removed.emphasis, theme::removed_emphasis());
        let context = style.look(LineKind::Context);
        assert_eq!(context.fill, Color32::TRANSPARENT);
        assert_eq!(context.mark, "");
        assert_eq!(context.mark_color, theme::text_faint());
        assert_eq!(context.text, theme::text_body());
    }

    #[test]
    fn embedded_rows_are_a_faint_tint_with_a_bar_and_normal_text() {
        let style = DiffStyle::embedded(Color32::from_rgb(0x2d, 0xd4, 0xbf));
        let added = style.look(LineKind::Added);
        assert_eq!(added.fill, theme::with_alpha(EMBEDDED_ADD, 0x12));
        assert_eq!(added.bar, Some(theme::with_alpha(EMBEDDED_ADD, 0x8c)));
        assert_eq!(added.mark_color, EMBEDDED_ADD);
        assert_eq!(added.text, theme::text());
        assert_ne!(added.number, theme::line_number());
        let removed = style.look(LineKind::Removed);
        assert_eq!(removed.fill, theme::with_alpha(EMBEDDED_DEL, 0x12));
        assert_eq!(removed.bar, Some(theme::with_alpha(EMBEDDED_DEL, 0x8c)));
        assert_eq!(removed.mark_color, EMBEDDED_DEL);
        let context = style.look(LineKind::Context);
        assert_eq!(context, DiffStyle::default().look(LineKind::Context));
        assert_eq!(style.accent(), Color32::from_rgb(0x2d, 0xd4, 0xbf));
        assert!(style.tinted() && style.hunk_divider && style.note_card);
        assert_eq!(style.bar_w, 2.0);
    }

    #[test]
    fn the_hunk_label_comes_from_the_hunk_numbers() {
        let cases = [
            ("@@ -0,0 +1,3 @@", Some("new file · lines 1 to 3")),
            ("@@ -1,3 +0,0 @@", Some("deleted file · lines 1 to 3")),
            ("@@ -1,6 +1,7 @@", Some("lines 1 to 7")),
            ("@@ -26,6 +26,10 @@ impl Foo {", Some("lines 26 to 35")),
            ("@@ -5 +5 @@", Some("line 5")),
            ("@@ -4,2 +3,0 @@", Some("removed lines 4 to 5")),
            ("@@ -4,1 +3,0 @@", Some("removed line 4")),
            ("@@ -0,0 +1 @@", Some("new file · line 1")),
            ("not a header", None),
            ("@@ -a,b +c,d @@", None),
        ];
        for (header, label) in cases {
            assert_eq!(hunk_label(header).as_deref(), label, "{header}");
        }
    }
}
