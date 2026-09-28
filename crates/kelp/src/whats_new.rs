use eframe::egui::{self, FontId, Margin, RichText, Sense, Stroke, Ui, pos2, vec2};
use kelp_core::update::is_newer;

use crate::help::Page;
use crate::settings::{Settings, is_dev_run};
use crate::theme;

pub const CHANGELOG: &str = include_str!("../../../CHANGELOG.md");
const MAX_SECTIONS: usize = 3;
const CURRENT: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Section<'a> {
    pub version: &'a str,
    pub heading: &'a str,
    pub body: &'a str,
}

pub fn sections(changelog: &str) -> Vec<Section<'_>> {
    let mut starts: Vec<usize> = changelog
        .match_indices("\n## ")
        .map(|(i, _)| i + 1)
        .collect();
    if changelog.starts_with("## ") {
        starts.insert(0, 0);
    }
    starts
        .iter()
        .enumerate()
        .map(|(n, &start)| {
            let end = starts.get(n + 1).copied().unwrap_or(changelog.len());
            let chunk = &changelog[start..end];
            let (heading, body) = chunk.split_once('\n').unwrap_or((chunk, ""));
            let heading = heading.trim_start_matches("## ").trim();
            Section {
                version: heading.split_whitespace().next().unwrap_or(heading),
                heading,
                body: body.trim(),
            }
        })
        .collect()
}

fn newest_first(a: &Section, b: &Section) -> std::cmp::Ordering {
    if is_newer(a.version, b.version) {
        std::cmp::Ordering::Less
    } else if is_newer(b.version, a.version) {
        std::cmp::Ordering::Greater
    } else {
        std::cmp::Ordering::Equal
    }
}

pub fn between<'a>(changelog: &'a str, after: Option<&str>, current: &str) -> Vec<Section<'a>> {
    let mut picked: Vec<Section> = sections(changelog)
        .into_iter()
        .filter(|s| !is_newer(s.version, current))
        .filter(|s| after.is_none_or(|seen| is_newer(s.version, seen)))
        .collect();
    picked.sort_by(newest_first);
    picked.truncate(MAX_SECTIONS);
    picked
}

pub fn previous_version<'a>(changelog: &'a str, current: &str) -> Option<&'a str> {
    sections(changelog)
        .into_iter()
        .map(|s| s.version)
        .filter(|v| is_newer(current, v))
        .reduce(|best, v| if is_newer(v, best) { v } else { best })
}

pub fn unseen_since<'a>(
    changelog: &'a str,
    last_seen: Option<&'a str>,
    fresh_install: bool,
    current: &str,
) -> Option<&'a str> {
    match last_seen {
        Some(seen) => is_newer(current, seen).then_some(seen),
        None if fresh_install => None,
        None => previous_version(changelog, current),
    }
}

pub fn on_launch(settings: &mut Settings) -> Option<WhatsNew> {
    if is_dev_run() {
        return WhatsNew::from_env();
    }
    let seen = settings.last_seen_version.clone();
    let since = unseen_since(
        CHANGELOG,
        seen.as_deref(),
        settings.is_fresh_install(),
        CURRENT,
    )
    .map(str::to_string);
    if seen.as_deref() != Some(CURRENT) {
        settings.last_seen_version = Some(CURRENT.to_string());
        settings.save();
    }
    since.and_then(|from| WhatsNew::since(Some(&from)))
}

pub struct WhatsNew {
    sections: Vec<Section<'static>>,
}

impl WhatsNew {
    pub fn since(after: Option<&str>) -> Option<Self> {
        let sections = between(CHANGELOG, after, CURRENT);
        (!sections.is_empty()).then_some(Self { sections })
    }
    fn from_env() -> Option<Self> {
        std::env::var("KELP_WHATS_NEW")
            .ok()
            .and_then(|from| Self::since(Some(from.as_str()).filter(|f| !f.is_empty())))
    }

    pub fn show(&mut self, ctx: &egui::Context) -> bool {
        let modal = egui::Modal::new(egui::Id::new("kelp-whats-new"))
            .backdrop_color(theme::backdrop())
            .frame(
                egui::Frame::new()
                    .fill(theme::modal())
                    .stroke(Stroke::new(1.0, theme::modal_border()))
                    .corner_radius(10)
                    .inner_margin(Margin::same(22)),
            )
            .show(ctx, |ui| {
                ui.set_width(520.0);
                let mut close = false;
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("What's new in Kelp {CURRENT}"))
                            .size(17.0)
                            .family(theme::semibold())
                            .color(theme::text_strong()),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        close = crate::widgets::close_button(ui, "Close (Esc)");
                    });
                });
                ui.add_space(8.0);
                let max_height = ui.ctx().content_rect().height() * 0.6;
                egui::ScrollArea::vertical()
                    .max_height(max_height)
                    .show(ui, |ui| {
                        for (n, section) in self.sections.iter().enumerate() {
                            if n > 0 {
                                ui.add_space(14.0);
                            }
                            show_section(ui, section);
                        }
                    });
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.link("See the full changelog").clicked() {
                        let _ = Page::Changelog.open();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        close |= ui
                            .add(
                                egui::Button::new("Done")
                                    .corner_radius(6)
                                    .min_size(egui::vec2(80.0, 32.0)),
                            )
                            .clicked();
                    });
                });
                close
            });
        !(modal.should_close() || modal.inner)
    }
}

const NOTE_FONT: f32 = 14.0;
const BULLET_INDENT: f32 = 18.0;

fn show_section(ui: &mut Ui, section: &Section) {
    ui.label(
        RichText::new(section.heading)
            .size(15.0)
            .family(theme::semibold())
            .color(theme::text_strong()),
    );
    ui.add_space(6.0);
    for line in section.body.lines().filter(|l| !l.trim().is_empty()) {
        match line.strip_prefix("- ") {
            Some(item) => bullet(ui, item),
            None => {
                ui.label(RichText::new(line).size(NOTE_FONT).color(theme::text()));
            }
        }
        ui.add_space(4.0);
    }
}

fn bullet(ui: &mut Ui, text: &str) {
    let wrap = (ui.available_width() - BULLET_INDENT).max(0.0);
    let galley = ui.painter().layout(
        text.to_owned(),
        FontId::proportional(NOTE_FONT),
        theme::text(),
        wrap,
    );
    let (rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), galley.size().y), Sense::hover());
    let first_line_mid = galley
        .rows
        .first()
        .map_or(rect.height() / 2.0, |row| row.rect().center().y);
    ui.painter().circle_filled(
        pos2(rect.left() + 5.0, rect.top() + first_line_mid),
        2.5,
        theme::text_muted(),
    );
    ui.painter().galley(
        pos2(rect.left() + BULLET_INDENT, rect.top()),
        galley,
        theme::text(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "# Changelog\n\nIntro.\n\n\
        ## v0.3.0 - 2026-03-01\n\n- Three.\n\n\
        ## v0.2.1 - 2026-02-15\n\n- Two point one.\n\n\
        ## v0.2.0 - 2026-02-01\n\n- Two.\n- More two.\n\n\
        ## v0.1.0 - 2026-01-01\n\n- One.\n";

    fn versions<'a>(picked: &[Section<'a>]) -> Vec<&'a str> {
        picked.iter().map(|s| s.version).collect()
    }

    #[test]
    fn sections_split_on_version_headings() {
        let all = sections(LOG);
        assert_eq!(versions(&all), ["v0.3.0", "v0.2.1", "v0.2.0", "v0.1.0"]);
        assert_eq!(all[2].heading, "v0.2.0 - 2026-02-01");
        assert_eq!(all[2].body, "- Two.\n- More two.");
        assert_eq!(all[3].body, "- One.");
    }

    #[test]
    fn between_picks_versions_after_the_last_seen_newest_first() {
        assert_eq!(
            versions(&between(LOG, Some("0.2.0"), "0.3.0")),
            ["v0.3.0", "v0.2.1"]
        );
        assert_eq!(
            versions(&between(LOG, Some("0.2.1"), "0.2.1")),
            [] as [&str; 0]
        );
    }

    #[test]
    fn between_skips_versions_newer_than_the_running_one() {
        assert_eq!(versions(&between(LOG, Some("0.1.0"), "0.2.0")), ["v0.2.0"]);
    }

    #[test]
    fn between_caps_at_three_sections() {
        assert_eq!(
            versions(&between(LOG, None, "0.3.0")),
            ["v0.3.0", "v0.2.1", "v0.2.0"]
        );
        assert_eq!(
            versions(&between(LOG, Some("0.0.1"), "9.0.0")),
            ["v0.3.0", "v0.2.1", "v0.2.0"]
        );
    }

    #[test]
    fn between_orders_by_version_not_by_file_order() {
        let shuffled =
            "## v0.1.0 - a\n\n- One.\n\n## v0.3.0 - c\n\n- Three.\n\n## v0.2.0 - b\n\n- Two.\n";
        assert_eq!(
            versions(&between(shuffled, None, "1.0.0")),
            ["v0.3.0", "v0.2.0", "v0.1.0"]
        );
    }

    #[test]
    fn first_install_shows_nothing() {
        assert_eq!(unseen_since(LOG, None, true, "0.3.0"), None);
    }

    #[test]
    fn upgrade_shows_what_came_after_the_last_seen_version() {
        assert_eq!(
            unseen_since(LOG, Some("0.2.0"), false, "0.3.0"),
            Some("0.2.0")
        );
        assert_eq!(unseen_since(LOG, Some("0.3.0"), false, "0.3.0"), None);
        assert_eq!(unseen_since(LOG, Some("0.4.0"), false, "0.3.0"), None);
    }

    #[test]
    fn upgrade_from_before_the_setting_existed_shows_the_current_release() {
        let since = unseen_since(LOG, None, false, "0.3.0");
        assert_eq!(since, Some("v0.2.1"));
        assert_eq!(versions(&between(LOG, since, "0.3.0")), ["v0.3.0"]);
    }

    #[test]
    fn changelog_page_lists_every_release_and_note_in_order() {
        let page = include_str!("../../../site/changelog.html");
        let escape = |text: &str| {
            text.replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
        };
        let mut from = 0;
        for section in sections(CHANGELOG) {
            let heading = format!("<h2>{} <time", section.version);
            let at = page[from..].find(&heading).unwrap_or_else(|| {
                panic!(
                    "{} missing or out of order; run scripts/make-changelog-page.py",
                    section.version
                )
            });
            from += at;
            for note in section.body.lines().filter_map(|l| l.strip_prefix("- ")) {
                let mut html = String::new();
                for (n, piece) in note.split('`').enumerate() {
                    match n % 2 {
                        0 => html.push_str(&escape(piece)),
                        _ => html.push_str(&format!("<code>{}</code>", escape(piece))),
                    }
                }
                let item = format!("<li>{html}</li>");
                let at = page[from..].find(&item).unwrap_or_else(|| {
                    panic!(
                        "{item} missing from {}; run scripts/make-changelog-page.py",
                        section.version
                    )
                });
                from += at;
            }
        }
    }

    #[test]
    fn the_embedded_changelog_covers_the_running_version() {
        let all = sections(CHANGELOG);
        assert!(
            all.iter()
                .any(|s| !is_newer(s.version, CURRENT) && !is_newer(CURRENT, s.version)),
            "CHANGELOG.md has no section for v{CURRENT}"
        );
        for s in &all {
            assert!(!s.body.is_empty(), "{} has no notes", s.version);
            assert!(s.heading.starts_with('v'), "{}", s.heading);
        }
    }
}
