use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use eframe::egui::{self, Margin, RichText, Stroke, Ui};
use gix::ObjectId;
use kelp_core::filter::{self, Filter, Period};

use crate::{theme, widgets};

const SETTLE: Duration = Duration::from_millis(300);
const SUGGESTIONS: usize = 6;

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Span {
    #[default]
    Any,
    Day,
    Week,
    Month,
    Custom,
}

#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct Draft {
    pub author: String,
    pub path: String,
    pub span: Span,
    pub from: String,
    pub to: String,
    pub mine: bool,
}

impl Draft {
    fn to_filter(&self, my_email: Option<&str>) -> Filter {
        let period = match self.span {
            Span::Any => Period::Any,
            Span::Day => Period::Day,
            Span::Week => Period::Week,
            Span::Month => Period::Month,
            Span::Custom => Period::Custom {
                from: filter::parse_day(&self.from),
                to: filter::parse_day(&self.to),
            },
        };
        Filter {
            author: self.author.trim().to_string(),
            path: self.path.trim().to_string(),
            period,
            mine: self.mine.then(|| my_email.unwrap_or_default().to_string()),
        }
    }

    pub fn parse(spec: &str) -> Self {
        let mut draft = Draft::default();
        for part in spec.split(',') {
            let (key, value) = part.split_once(':').unwrap_or((part, ""));
            match key.trim() {
                "author" => draft.author = value.to_string(),
                "path" => draft.path = value.to_string(),
                "mine" => draft.mine = true,
                "period" => {
                    draft.span = match value {
                        "day" => Span::Day,
                        "week" => Span::Week,
                        "month" => Span::Month,
                        _ => Span::Any,
                    }
                }
                _ => {}
            }
        }
        draft
    }
}

type Answer = (u64, Option<Vec<bool>>);

#[derive(Default)]
pub struct FilterBar {
    pub open: bool,
    draft: Draft,
    applied: Filter,
    rows: Option<Vec<bool>>,
    matches: usize,
    generation: u64,
    latest: Arc<AtomicU64>,
    answer: Option<Receiver<Answer>>,
    edited_at: Option<Instant>,
    authors: Option<Vec<String>>,
    authors_rx: Option<Receiver<Vec<String>>>,
    my_email: Option<String>,
    focus_author: bool,
    stale: bool,
}

pub enum Outcome {
    None,
    Close,
}

impl FilterBar {
    pub fn from_env() -> Self {
        let mut bar = Self::default();
        if let Ok(spec) = std::env::var("KELP_FILTER") {
            bar.open = true;
            bar.draft = Draft::parse(&spec);
            bar.edited_at = Some(Instant::now() - SETTLE);
        }
        bar
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.focus_author = self.open;
    }

    pub fn is_active(&self) -> bool {
        !self.applied.is_empty()
    }

    pub fn conditions(&self) -> usize {
        self.applied.conditions()
    }

    pub fn matches(&self) -> Option<usize> {
        self.rows.as_ref().map(|_| self.matches)
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn rows(&self, len: usize) -> Option<&[bool]> {
        self.rows.as_deref().filter(|rows| rows.len() == len)
    }

    pub fn clear(&mut self) {
        self.draft = Draft::default();
        self.applied = Filter::default();
        self.rows = None;
        self.edited_at = None;
        self.bump();
    }

    pub fn history_changed(&mut self) {
        self.authors = None;
        self.stale = self.is_active();
    }

    pub fn poll(&mut self, dir: &Path, ids: &[ObjectId], ctx: &egui::Context) {
        if std::mem::take(&mut self.stale) {
            self.spawn(dir, ids, ctx);
        }
        if let Some(rx) = &self.answer
            && let Ok((generation, rows)) = rx.try_recv()
        {
            self.answer = None;
            if generation == self.generation
                && let Some(rows) = rows
            {
                self.matches = rows.iter().filter(|&&keep| keep).count();
                self.rows = Some(rows);
            }
        }
        if let Some(rx) = &self.authors_rx
            && let Ok(names) = rx.try_recv()
        {
            self.authors = Some(names);
            self.authors_rx = None;
        }
        if self.open && self.authors.is_none() && self.authors_rx.is_none() {
            let (tx, rx) = mpsc::channel();
            let (dir, ids, ctx) = (dir.to_path_buf(), ids.to_vec(), ctx.clone());
            std::thread::spawn(move || {
                let _ = tx.send(filter::authors(&dir, &ids).unwrap_or_default());
                ctx.request_repaint();
            });
            self.authors_rx = Some(rx);
        }
        let Some(edited) = self.edited_at else {
            return;
        };
        let waited = edited.elapsed();
        if waited < SETTLE {
            ctx.request_repaint_after(SETTLE - waited);
            return;
        }
        self.edited_at = None;
        if self.draft.mine && self.my_email.is_none() {
            self.my_email = filter::user_email(dir);
        }
        let next = self.draft.to_filter(self.my_email.as_deref());
        if next == self.applied {
            return;
        }
        self.applied = next;
        if self.applied.is_empty() {
            self.rows = None;
            self.bump();
        } else {
            self.spawn(dir, ids, ctx);
        }
    }

    fn bump(&mut self) {
        self.generation += 1;
        self.latest.store(self.generation, Ordering::Relaxed);
    }

    fn spawn(&mut self, dir: &Path, ids: &[ObjectId], ctx: &egui::Context) {
        self.bump();
        self.rows = None;
        let generation = self.generation;
        let latest = self.latest.clone();
        let (tx, rx) = mpsc::channel();
        let (dir, ids, ctx): (PathBuf, Vec<ObjectId>, _) =
            (dir.to_path_buf(), ids.to_vec(), ctx.clone());
        let filter = self.applied.clone();
        std::thread::spawn(move || {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64);
            let cancelled = || latest.load(Ordering::Relaxed) != generation;
            let rows = filter::matching_rows(&dir, &ids, &filter, now, &cancelled)
                .ok()
                .flatten();
            let _ = tx.send((generation, rows));
            ctx.request_repaint();
        });
        self.answer = Some(rx);
    }

    pub fn ui(&mut self, ui: &mut Ui) -> Outcome {
        let mut outcome = Outcome::None;
        let before = self.draft.clone();
        let bar = egui::Frame::new()
            .fill(theme::PANEL)
            .inner_margin(Margin::symmetric(14, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Filter").family(theme::semibold()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::close_button(ui, "Close the filter (Esc)") {
                            outcome = Outcome::Close;
                        }
                        if self.is_active() && ui.button("Clear").clicked() {
                            self.clear();
                        }
                        ui.label(
                            RichText::new(self.status())
                                .size(12.0)
                                .color(theme::TEXT_FAINT),
                        );
                    });
                });
                ui.horizontal(|ui| {
                    let author = ui.add(
                        egui::TextEdit::singleline(&mut self.draft.author)
                            .hint_text("Author name or email")
                            .desired_width(250.0),
                    );
                    if std::mem::take(&mut self.focus_author) {
                        author.request_focus();
                    }
                    self.suggestions(ui, &author);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.draft.path)
                            .hint_text("Path, like src/ or README.md")
                            .desired_width(250.0),
                    );
                });
                ui.horizontal(|ui| {
                    widgets::segmented(
                        ui,
                        &mut self.draft.span,
                        &[
                            (Span::Any, "Any time"),
                            (Span::Day, "Day"),
                            (Span::Week, "Week"),
                            (Span::Month, "Month"),
                            (Span::Custom, "Custom"),
                        ],
                    );
                    if self.draft.span == Span::Custom {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.draft.from)
                                .hint_text("From YYYY-MM-DD")
                                .desired_width(110.0),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut self.draft.to)
                                .hint_text("To YYYY-MM-DD")
                                .desired_width(110.0),
                        );
                    }
                    ui.checkbox(&mut self.draft.mine, "Only my commits");
                });
            });
        ui.painter().hline(
            bar.response.rect.x_range(),
            bar.response.rect.bottom(),
            Stroke::new(1.0, theme::BORDER),
        );
        if self.draft != before {
            self.edited_at = Some(Instant::now());
        }
        outcome
    }

    fn suggestions(&mut self, ui: &mut Ui, field: &egui::Response) {
        let typed = self.draft.author.trim().to_lowercase();
        let Some(authors) = &self.authors else {
            return;
        };
        if typed.is_empty() || !field.has_focus() {
            return;
        }
        let picks: Vec<String> = authors
            .iter()
            .filter(|name| name.to_lowercase().contains(&typed) && name.to_lowercase() != typed)
            .take(SUGGESTIONS)
            .cloned()
            .collect();
        if picks.is_empty() {
            return;
        }
        let mut chosen = None;
        egui::Area::new(egui::Id::new("filter-authors"))
            .fixed_pos(field.rect.left_bottom() + egui::vec2(0.0, 4.0))
            .order(egui::Order::Tooltip)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(field.rect.width());
                    for name in &picks {
                        if ui
                            .add(egui::Button::new(name).frame(false))
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            chosen = Some(name.clone());
                        }
                    }
                });
            });
        if let Some(name) = chosen {
            self.draft.author = name;
            self.edited_at = Some(Instant::now() - SETTLE);
        }
    }

    pub fn status(&self) -> String {
        match (self.is_active(), self.matches()) {
            (false, _) => "Showing every commit".into(),
            (true, None) => "Filtering…".into(),
            (true, Some(n)) => format!("{n} match{}", if n == 1 { "" } else { "es" }),
        }
    }

    pub fn chip_text(&self) -> String {
        let n = self.conditions();
        format!(
            "Filter: {n} condition{} · {}",
            if n == 1 { "" } else { "s" },
            self.status()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs_from_the_dev_switch() {
        let draft = Draft::parse("author:Maya,path:src/,period:week,mine");
        assert_eq!(draft.author, "Maya");
        assert_eq!(draft.path, "src/");
        assert_eq!(draft.span, Span::Week);
        assert!(draft.mine);
    }

    #[test]
    fn drafts_become_filters() {
        let draft = Draft {
            author: "  sam ".into(),
            span: Span::Custom,
            from: "2026-09-01".into(),
            mine: true,
            ..Draft::default()
        };
        let filter = draft.to_filter(Some("me@example.com"));
        assert_eq!(filter.author, "sam");
        assert_eq!(filter.mine.as_deref(), Some("me@example.com"));
        assert_eq!(
            filter.period,
            Period::Custom {
                from: filter::parse_day("2026-09-01"),
                to: None
            }
        );
        assert_eq!(filter.conditions(), 3);
    }
}
