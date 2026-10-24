use std::ops::Range;

use egui::Color32;
use egui::text::{LayoutJob, TextFormat};

/// Text to mark in the commit titles of the graph, such as a search hit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Highlight {
    pub needle: String,
    pub fill: Color32,
}

pub(super) fn match_ranges(text: &str, needle: &str) -> Vec<Range<usize>> {
    let needle = needle.trim().to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    let lower = text.to_lowercase();
    if lower.len() != text.len() {
        return Vec::new();
    }
    lower
        .match_indices(&needle)
        .map(|(at, hit)| at..at + hit.len())
        .collect()
}

pub(super) fn append(
    job: &mut LayoutJob,
    text: &str,
    format: TextFormat,
    highlight: Option<&Highlight>,
) {
    let Some(highlight) = highlight else {
        job.append(text, 0.0, format);
        return;
    };
    let mut from = 0;
    for range in match_ranges(text, &highlight.needle) {
        job.append(&text[from..range.start], 0.0, format.clone());
        let marked = TextFormat {
            background: highlight.fill,
            ..format.clone()
        };
        job.append(&text[range.clone()], 0.0, marked);
        from = range.end;
    }
    job.append(&text[from..], 0.0, format);
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::FontId;

    #[test]
    fn matches_ignore_case_and_pad_whitespace() {
        assert_eq!(
            match_ranges("Fix the Parser, fix more", "  fix "),
            vec![0..3, 16..19]
        );
        assert_eq!(
            match_ranges("nothing here", "zzz"),
            Vec::<Range<usize>>::new()
        );
        assert_eq!(match_ranges("anything", "   "), Vec::<Range<usize>>::new());
    }

    #[test]
    fn a_text_whose_lowercase_changes_length_has_no_ranges() {
        assert_eq!(match_ranges("İstanbul", "stan"), Vec::<Range<usize>>::new());
    }

    #[test]
    fn only_the_matching_parts_get_the_fill() {
        let fill = Color32::from_rgb(1, 2, 3);
        let highlight = Highlight {
            needle: "pars".into(),
            fill,
        };
        let format = TextFormat::simple(FontId::proportional(13.0), Color32::WHITE);
        let mut job = LayoutJob::default();
        append(&mut job, "Fix parser", format.clone(), Some(&highlight));
        let parts: Vec<(&str, bool)> = job
            .sections
            .iter()
            .map(|s| {
                (
                    &job.text[s.byte_range.start.0..s.byte_range.end.0],
                    s.format.background == fill,
                )
            })
            .collect();
        assert_eq!(parts, vec![("Fix ", false), ("pars", true), ("er", false)]);
        let mut plain = LayoutJob::default();
        append(&mut plain, "Fix parser", format, None);
        assert_eq!(plain.sections.len(), 1);
    }
}
