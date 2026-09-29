use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use eframe::egui::{self, Margin, RichText, Sense, Stroke, Ui, vec2};
use kelp_core::console::{self, Entry, Kind};

use crate::{theme, widgets};

const ROW_H: f32 = 30.0;
const OUTPUT_MAX_H: f32 = 320.0;

pub enum Event {
    None,
    Close,
}

pub struct ConsoleView {
    repo_dir: PathBuf,
    filter: String,
    show_background: bool,
    all_repos: bool,
    expanded: HashSet<u64>,
    cursor: Option<u64>,
    scroll_to: Option<u64>,
    seen: u64,
    entries: Vec<Entry>,
}

impl ConsoleView {
    pub fn open(repo_dir: &Path, focus: Option<u64>) -> Self {
        let mut view = Self {
            repo_dir: repo_dir.to_path_buf(),
            filter: String::new(),
            show_background: focus.is_some_and(|id| {
                console::entries()
                    .iter()
                    .any(|e| e.id == id && e.kind == Kind::Background)
            }),
            all_repos: false,
            expanded: focus.into_iter().collect(),
            cursor: focus,
            scroll_to: focus,
            seen: 0,
            entries: Vec::new(),
        };
        view.refresh();
        view
    }

    fn refresh(&mut self) {
        let latest = console::latest_id();
        if latest != self.seen {
            self.entries = console::entries();
            self.seen = latest;
        }
    }

    pub fn show_background(mut self) -> Self {
        self.show_background = true;
        self
    }

    fn in_scope(&self, entry: &Entry) -> bool {
        self.all_repos
            || entry.dir.as_os_str().is_empty()
            || entry.dir.starts_with(&self.repo_dir)
            || self.repo_dir.starts_with(&entry.dir)
    }

    fn visible(&self) -> Vec<usize> {
        let needle = self.filter.trim().to_lowercase();
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| self.show_background || e.kind == Kind::Action || e.failed())
            .filter(|(_, e)| self.in_scope(e))
            .filter(|(_, e)| needle.is_empty() || e.command_line().to_lowercase().contains(&needle))
            .map(|(i, _)| i)
            .collect()
    }

    fn hidden_background(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.kind == Kind::Background && !e.failed() && self.in_scope(e))
            .count()
    }

    pub fn ui(&mut self, ui: &mut Ui) -> Event {
        self.refresh();
        let mut event = Event::None;
        egui::Frame::new()
            .inner_margin(Margin::symmetric(24, 18))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Git console")
                            .size(17.0)
                            .family(theme::semibold())
                            .color(theme::text_strong()),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::close_button(ui, "Close (Esc)") {
                            event = Event::Close;
                        }
                    });
                });
                ui.label(
                    RichText::new(
                        "Every git and gh command Kelp ran in this session, newest first. \
                         Reads done in-process through gitoxide are not listed.",
                    )
                    .size(12.0)
                    .color(theme::text_faint()),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.filter)
                            .hint_text("Filter commands")
                            .desired_width(280.0),
                    );
                    ui.checkbox(
                        &mut self.show_background,
                        RichText::new("Background checks")
                            .size(12.0)
                            .color(theme::text_muted()),
                    )
                    .on_hover_text("Status polls, watcher checks and other housekeeping");
                    ui.checkbox(
                        &mut self.all_repos,
                        RichText::new("All repositories")
                            .size(12.0)
                            .color(theme::text_muted()),
                    );
                });
            });
        let rows = self.visible();
        if rows.is_empty() {
            let hidden = self.hidden_background();
            let text = if hidden > 0 && !self.show_background {
                format!(
                    "No actions yet. {hidden} background check{} hidden.",
                    if hidden == 1 { " is" } else { "s are" }
                )
            } else {
                "Nothing ran yet.".to_string()
            };
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new(text).color(theme::text_muted()));
            });
            return event;
        }
        let now = SystemTime::now();
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let ctx = ui.ctx().clone();
                let owns_keys = crate::list_keys::nothing_focused(&ctx);
                let toggle_cursor = owns_keys && crate::list_keys::pressed(&ctx, egui::Key::Enter);
                let mut hits = Vec::new();
                for &index in &rows {
                    let entry = &self.entries[index];
                    if toggle_cursor && self.cursor == Some(entry.id) {
                        toggle(&mut self.expanded, entry.id);
                    }
                    let open = self.expanded.contains(&entry.id);
                    let at_cursor = self.cursor == Some(entry.id);
                    let response = entry_row(ui, entry, open, at_cursor, now);
                    if self.scroll_to == Some(entry.id) {
                        response.scroll_to_me(Some(egui::Align::Center));
                        self.scroll_to = None;
                    }
                    if response.clicked() {
                        self.cursor = Some(entry.id);
                        toggle(&mut self.expanded, entry.id);
                    }
                    hits.push((response.id, response.rect));
                    if self.expanded.contains(&entry.id) {
                        entry_details(ui, entry);
                    }
                }
                let at = rows
                    .iter()
                    .position(|&index| Some(self.entries[index].id) == self.cursor);
                if let Some(i) = crate::list_keys::step(ui, &hits, at, owns_keys) {
                    self.cursor = Some(self.entries[rows[i]].id);
                }
            });
        event
    }
}

fn toggle(expanded: &mut HashSet<u64>, id: u64) {
    if !expanded.remove(&id) {
        expanded.insert(id);
    }
}

fn entry_row(
    ui: &mut Ui,
    entry: &Entry,
    open: bool,
    at_cursor: bool,
    now: SystemTime,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let painter = ui.painter_at(rect);
    if at_cursor {
        painter.rect_filled(rect, 0.0, theme::sidebar_selected());
    } else if response.hovered() || open {
        painter.rect_filled(rect, 0.0, theme::overlay(0x06));
    }
    widgets::focus_ring(ui, &response, 0.0);
    crate::focus_areas::offer(
        ui.ctx(),
        crate::focus_areas::Area::Graph,
        response.id,
        at_cursor,
    );
    let state = if open { "expanded" } else { "collapsed" };
    widgets::describe(
        &response,
        egui::WidgetType::CollapsingHeader,
        format!("{}, {state}", entry.command_line()),
    );
    let color = match entry.exit {
        Some(0) => theme::added(),
        None => theme::modified(),
        Some(_) => theme::deleted(),
    };
    let left = rect.left() + 24.0;
    painter.circle_filled(egui::pos2(left, rect.center().y), 3.5, color);
    let right_text = format!(
        "{}  ·  {}",
        duration_text(entry.duration),
        ago(entry.started, now)
    );
    let right = painter.layout_no_wrap(
        right_text,
        egui::FontId::proportional(11.0),
        theme::text_faint(),
    );
    let right_w = right.size().x;
    painter.galley(
        egui::pos2(
            rect.right() - 24.0 - right_w,
            rect.center().y - right.size().y / 2.0,
        ),
        right,
        theme::text_faint(),
    );
    let mut x = left + 14.0;
    if entry.kind == Kind::Background {
        let tag = painter.layout_no_wrap(
            "bg".into(),
            egui::FontId::proportional(10.0),
            theme::text_faint(),
        );
        let tag_rect = egui::Rect::from_min_size(
            egui::pos2(x, rect.center().y - 8.0),
            vec2(tag.size().x + 10.0, 16.0),
        );
        painter.rect_stroke(
            tag_rect,
            4.0,
            Stroke::new(1.0, theme::border()),
            egui::StrokeKind::Inside,
        );
        painter.galley(
            tag_rect.center() - tag.size() / 2.0,
            tag,
            theme::text_faint(),
        );
        x = tag_rect.right() + 8.0;
    }
    let command = crate::graph_view::truncated(
        &painter,
        entry.command_line(),
        egui::FontId::monospace(12.0),
        theme::text(),
        (rect.right() - 48.0 - right_w - x).max(0.0),
    );
    painter.galley(
        egui::pos2(x, rect.center().y - command.size().y / 2.0),
        command,
        theme::text(),
    );
    response
}

fn entry_details(ui: &mut Ui, entry: &Entry) {
    egui::Frame::new()
        .fill(theme::field())
        .inner_margin(Margin {
            left: 38,
            right: 24,
            top: 10,
            bottom: 12,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.0;
            let exit = match entry.exit {
                Some(code) => format!("exit {code}"),
                None => "stopped".to_string(),
            };
            let dir = if entry.dir.as_os_str().is_empty() {
                String::new()
            } else {
                format!("  ·  in {}", entry.dir.display())
            };
            ui.label(
                RichText::new(format!("{exit}{dir}"))
                    .size(12.0)
                    .color(theme::text_muted()),
            );
            ui.horizontal(|ui| {
                if ui.button("Copy command").clicked() {
                    ui.ctx().copy_text(entry.command_line());
                }
                if ui.button("Copy output").clicked() {
                    ui.ctx()
                        .copy_text(format!("{}{}", entry.stdout.text, entry.stderr.text));
                }
            });
            output_block(ui, "stdout", &entry.stdout, entry.id);
            output_block(ui, "stderr", &entry.stderr, entry.id);
        });
}

fn output_block(ui: &mut Ui, label: &str, output: &console::Output, id: u64) {
    if output.text.trim().is_empty() {
        return;
    }
    let caption = if output.truncated {
        format!("{label} (first 64 KB)")
    } else {
        label.to_string()
    };
    ui.label(RichText::new(caption).size(11.0).color(theme::text_faint()));
    egui::ScrollArea::vertical()
        .id_salt((label, id))
        .max_height(OUTPUT_MAX_H)
        .show(ui, |ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(output.text.trim_end())
                        .monospace()
                        .size(12.0)
                        .color(theme::text()),
                )
                .selectable(true),
            );
        });
}

fn duration_text(duration: Duration) -> String {
    let ms = duration.as_millis();
    if ms < 1000 {
        format!("{ms} ms")
    } else {
        format!("{:.1} s", duration.as_secs_f32())
    }
}

fn ago(started: SystemTime, now: SystemTime) -> String {
    let secs = now.duration_since(started).map_or(0, |d| d.as_secs());
    match secs {
        0..=4 => "just now".into(),
        5..=59 => format!("{secs}s ago"),
        60..=3599 => format!("{}m ago", secs / 60),
        _ => format!("{}h ago", secs / 3600),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_and_ages_read_naturally() {
        assert_eq!(duration_text(Duration::from_millis(42)), "42 ms");
        assert_eq!(duration_text(Duration::from_millis(1500)), "1.5 s");
        let now = SystemTime::now();
        assert_eq!(ago(now, now), "just now");
        assert_eq!(ago(now - Duration::from_secs(90), now), "1m ago");
        assert_eq!(ago(now - Duration::from_secs(7200), now), "2h ago");
    }

    fn frame(ctx: &egui::Context, view: &mut ConsoleView, events: Vec<egui::Event>) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(1000.0, 800.0),
            )),
            events,
            ..Default::default()
        };
        let _ = ctx.run_ui(input, |ui| {
            view.ui(ui);
        });
    }

    #[test]
    fn arrows_move_a_cursor_and_enter_expands_the_entry() {
        let dir = std::env::temp_dir().join(format!("kelp-console-keys-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["status", "--short"],
            &["log", "--oneline"],
        ] {
            let _ = console::as_action(|| kelp_core::git_cli::run(&dir, args));
        }

        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let mut view = ConsoleView::open(&dir, None);
        let rows: Vec<u64> = view
            .visible()
            .into_iter()
            .map(|i| view.entries[i].id)
            .collect();
        assert_eq!(rows.len(), 3);
        let press = |view: &mut ConsoleView, key| {
            frame(&ctx, view, crate::list_keys::tap(key));
            frame(&ctx, view, vec![]);
        };
        frame(&ctx, &mut view, vec![]);
        assert_eq!(view.cursor, None);
        press(&mut view, egui::Key::ArrowDown);
        assert_eq!(view.cursor, Some(rows[0]));
        press(&mut view, egui::Key::ArrowDown);
        assert_eq!(view.cursor, Some(rows[1]));
        press(&mut view, egui::Key::Enter);
        assert!(view.expanded.contains(&rows[1]));
        press(&mut view, egui::Key::End);
        assert_eq!(view.cursor, Some(rows[2]));
        press(&mut view, egui::Key::Enter);
        assert!(view.expanded.contains(&rows[2]));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
