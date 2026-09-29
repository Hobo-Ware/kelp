use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};

use eframe::egui::{self, Color32, FontId, Margin, RichText, Sense, Ui, pos2, vec2};
use gix::ObjectId;
use kelp_core::commit::ChangeKind;
use kelp_core::compare::{RangeChange, Side};
use kelp_core::history::History;

use crate::details::FileRow;
use crate::{theme, widgets};

pub const MARK_A: &str = "A";
pub const MARK_B: &str = "B";

enum Files {
    Loading(Receiver<Result<Vec<RangeChange>, String>>),
    Ready(Vec<RangeChange>),
    Failed(String),
}

pub struct CompareView {
    pub base: ObjectId,
    pub target: Side,
    base_title: String,
    target_title: String,
    files: Files,
}

pub enum Event {
    None,
    Close,
    Swap,
    Open(RangeChange),
}

#[derive(Clone, Copy, Default)]
pub struct Marks {
    pub base: Option<usize>,
    pub target: Option<usize>,
    pub work_tree: bool,
}

impl Marks {
    pub fn mark(&self, row: usize) -> Option<&'static str> {
        if self.base == Some(row) {
            Some(MARK_A)
        } else if self.target == Some(row) {
            Some(MARK_B)
        } else {
            None
        }
    }
}

impl CompareView {
    pub fn new(
        ctx: &egui::Context,
        repo: &gix::Repository,
        dir: PathBuf,
        base: ObjectId,
        target: Side,
    ) -> Self {
        let title = |id: ObjectId| {
            kelp_core::commit::summary(repo, id)
                .map(|s| s.title)
                .unwrap_or_default()
        };
        let target_title = match target {
            Side::Commit(id) => title(id),
            Side::WorkTree => "Uncommitted changes".into(),
        };
        Self {
            base_title: title(base),
            target_title,
            files: list(ctx, dir, base, target),
            base,
            target,
        }
    }

    pub fn refresh(&mut self, ctx: &egui::Context, dir: PathBuf) {
        if self.target == Side::WorkTree {
            self.files = list(ctx, dir, self.base, self.target);
        }
    }

    pub fn marks(&self, history: &History) -> Marks {
        Marks {
            base: history.row(&self.base),
            target: match self.target {
                Side::Commit(id) => history.row(&id),
                Side::WorkTree => None,
            },
            work_tree: self.target == Side::WorkTree,
        }
    }

    pub fn target_id(&self) -> Option<ObjectId> {
        match self.target {
            Side::Commit(id) => Some(id),
            Side::WorkTree => None,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, active: Option<&str>) -> Event {
        if let Files::Loading(rx) = &self.files
            && let Ok(result) = rx.try_recv()
        {
            self.files = match result {
                Ok(list) => Files::Ready(list),
                Err(e) => Files::Failed(e),
            };
        }
        let mut event = Event::None;
        egui::Frame::new()
            .inner_margin(Margin {
                left: 16,
                right: 12,
                top: 14,
                bottom: 10,
            })
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("COMPARING")
                            .size(11.0)
                            .family(theme::semibold())
                            .color(theme::text_muted()),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::close_button(ui, "Stop comparing (Esc)") {
                            event = Event::Close;
                        }
                        if self.target != Side::WorkTree
                            && ui
                                .add(egui::Button::new(RichText::new("Swap").size(12.0)))
                                .on_hover_text("Swap A and B")
                                .clicked()
                        {
                            event = Event::Swap;
                        }
                    });
                });
                ui.add_space(6.0);
                end_line(
                    ui,
                    MARK_A,
                    &self.base.to_hex_with_len(7).to_string(),
                    &self.base_title,
                );
                let target_hash = match self.target {
                    Side::Commit(id) => id.to_hex_with_len(7).to_string(),
                    Side::WorkTree => "working tree".into(),
                };
                end_line(ui, MARK_B, &target_hash, &self.target_title);
                ui.add_space(6.0);
                let summary = match &self.files {
                    Files::Loading(_) => "Listing changes…".to_string(),
                    Files::Failed(e) => e.clone(),
                    Files::Ready(files) if files.is_empty() => "No differences".to_string(),
                    Files::Ready(files) => format!(
                        "{} file{} changed from A to B",
                        files.len(),
                        if files.len() == 1 { "" } else { "s" }
                    ),
                };
                ui.label(RichText::new(summary).size(12.0).color(theme::text_faint()));
            });
        ui.separator();
        let Files::Ready(files) = &self.files else {
            return event;
        };
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let mut rows = Vec::with_capacity(files.len());
                for change in files {
                    let row = FileRow {
                        kind: Some(change.kind),
                        path: &change.path,
                        label: None,
                        depth: 0,
                        active: active == Some(change.path.as_str()),
                        comments: 0,
                    }
                    .show(ui);
                    let row = match (&change.old_path, change.kind) {
                        (Some(old), ChangeKind::Renamed) => {
                            row.on_hover_text(format!("Renamed from {old}"))
                        }
                        _ => row,
                    };
                    rows.push((row.id, row.rect));
                    if row.clicked() {
                        event = Event::Open(change.clone());
                    }
                }
                let open = active.and_then(|a| files.iter().position(|c| c.path == a));
                let owns_keys = open.is_some() && crate::list_keys::nothing_focused(ui.ctx());
                if let Some(at) = crate::list_keys::step(ui, &rows, open, owns_keys) {
                    event = Event::Open(files[at].clone());
                }
            });
        event
    }
}

pub fn paint_mark(painter: &egui::Painter, center: egui::Pos2, mark: &str) {
    painter.circle_filled(center, 7.0, theme::with_alpha(theme::accent(), 0x40));
    painter.circle_stroke(
        center,
        7.0,
        egui::Stroke::new(1.0, theme::with_alpha(theme::accent(), 0xc0)),
    );
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        mark,
        FontId::new(9.5, theme::semibold()),
        theme::accent(),
    );
}

fn end_line(ui: &mut Ui, mark: &str, hash: &str, title: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
    let painter = ui.painter_at(rect);
    paint_mark(&painter, pos2(rect.left() + 8.0, rect.center().y), mark);
    let hash = painter.layout_no_wrap(
        hash.to_string(),
        FontId::monospace(12.0),
        theme::text_strong(),
    );
    let hash_w = hash.size().x;
    painter.galley(
        pos2(rect.left() + 22.0, rect.center().y - hash.size().y / 2.0),
        hash,
        Color32::PLACEHOLDER,
    );
    let title = crate::graph_view::truncated(
        &painter,
        title.to_string(),
        FontId::proportional(13.0),
        theme::text(),
        (rect.width() - 32.0 - hash_w).max(0.0),
    );
    painter.galley(
        pos2(
            rect.left() + 30.0 + hash_w,
            rect.center().y - title.size().y / 2.0,
        ),
        title,
        Color32::PLACEHOLDER,
    );
}

fn list(ctx: &egui::Context, dir: PathBuf, base: ObjectId, target: Side) -> Files {
    let (tx, rx) = mpsc::channel();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let result =
            kelp_core::compare::changed_files(&dir, base, target).map_err(|e| format!("{e:#}"));
        let _ = tx.send(result);
        ctx.request_repaint();
    });
    Files::Loading(rx)
}

#[cfg(test)]
mod tests {
    use eframe::egui::{self, Key, Modifiers, Pos2, RawInput, Rect, vec2};
    use kelp_core::commit::ChangeKind;
    use kelp_core::compare::{RangeChange, Side};

    use super::{CompareView, Event, Files};

    fn view(paths: &[&str]) -> CompareView {
        let files = paths
            .iter()
            .map(|p| RangeChange {
                path: p.to_string(),
                old_path: None,
                kind: ChangeKind::Modified,
            })
            .collect();
        CompareView {
            base: gix::ObjectId::null(gix::hash::Kind::Sha1),
            target: Side::WorkTree,
            base_title: String::new(),
            target_title: String::new(),
            files: Files::Ready(files),
        }
    }

    fn frame(ctx: &egui::Context, view: &mut CompareView, active: &str, key: Option<Key>) -> Event {
        let events = key
            .map(|key| {
                [true, false]
                    .map(|pressed| egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers: Modifiers::NONE,
                    })
                    .to_vec()
            })
            .unwrap_or_default();
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 700.0))),
            events,
            ..Default::default()
        };
        let mut event = Event::None;
        let _ = ctx.run_ui(input, |ui| event = view.ui(ui, Some(active)));
        event
    }

    fn opened(event: Event) -> Option<String> {
        match event {
            Event::Open(change) => Some(change.path),
            _ => None,
        }
    }

    #[test]
    fn arrows_open_the_next_and_previous_changed_file() {
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let mut view = view(&["a.rs", "b.rs", "c.rs"]);
        frame(&ctx, &mut view, "a.rs", None);
        let down = frame(&ctx, &mut view, "a.rs", Some(Key::ArrowDown));
        assert_eq!(opened(down).as_deref(), Some("b.rs"));
        let end = frame(&ctx, &mut view, "b.rs", Some(Key::End));
        assert_eq!(opened(end).as_deref(), Some("c.rs"));
        let up = frame(&ctx, &mut view, "c.rs", Some(Key::ArrowUp));
        assert_eq!(opened(up).as_deref(), Some("b.rs"));
    }
}
