use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};

use eframe::egui::{self, FontId, Margin, RichText, Sense, Ui, pos2, vec2};
use gix::ObjectId;
use kelp_core::file_history::{self, Entry};
use kelp_core::review::Review;

use crate::avatars::AvatarStore;
use crate::diff_view::{self, DiffSource, DiffView};
use crate::{graph_view, theme, widgets};

const LIST_W: f32 = 380.0;
const ROW_H: f32 = 50.0;
const AVATAR_R: f32 = 11.0;

enum Message {
    Batch(Vec<Entry>),
    Done(Result<(), String>),
}

pub struct FileHistoryView {
    pub path: String,
    entries: Vec<Entry>,
    incoming: Receiver<Message>,
    cancel: Arc<AtomicBool>,
    finished: Option<Result<(), String>>,
    open: Option<(usize, Box<DiffView>)>,
}

pub enum Event {
    None,
    Close,
    Reveal(ObjectId),
    Diff(diff_view::Event),
}

impl FileHistoryView {
    pub fn open(ctx: &egui::Context, dir: PathBuf, path: &str) -> Self {
        let (tx, incoming) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let (flag, ctx, path_arg) = (cancel.clone(), ctx.clone(), path.to_string());
        std::thread::spawn(move || {
            let result = file_history::stream(&dir, &path_arg, &flag, |batch| {
                if tx.send(Message::Batch(batch)).is_ok() {
                    ctx.request_repaint();
                }
            });
            let _ = tx.send(Message::Done(result.map_err(|e| format!("{e:#}"))));
            ctx.request_repaint();
        });
        Self {
            path: path.to_string(),
            entries: Vec::new(),
            incoming,
            cancel,
            finished: None,
            open: None,
        }
    }

    fn poll(&mut self) {
        for message in self.incoming.try_iter() {
            match message {
                Message::Batch(batch) => self.entries.extend(batch),
                Message::Done(result) => self.finished = Some(result),
            }
        }
    }

    fn select(&mut self, repo: &gix::Repository, workdir: Option<&Path>, index: usize) {
        let Some(entry) = self.entries.get(index) else {
            return;
        };
        if let Ok(diff) = DiffView::load(repo, workdir, DiffSource::Commit(entry.id), &entry.path) {
            self.open = Some((index, Box::new(diff)));
        }
    }

    pub fn ui(
        &mut self,
        ui: &mut Ui,
        repo: &gix::Repository,
        workdir: Option<&Path>,
        review: &mut Review,
        author: &str,
        avatars: &mut AvatarStore,
    ) -> Event {
        self.poll();
        if self.open.is_none() && !self.entries.is_empty() {
            self.select(repo, workdir, 0);
        }
        let mut event = self.header(ui);
        let mut picked = None;
        let now = now();
        egui::Panel::left("file-history-list")
            .exact_size(LIST_W)
            .frame(egui::Frame::new().fill(theme::PANEL))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                egui::ScrollArea::vertical().auto_shrink(false).show_rows(
                    ui,
                    ROW_H,
                    self.entries.len(),
                    |ui, rows| {
                        for i in rows {
                            let active = self.open.as_ref().is_some_and(|(open, _)| *open == i);
                            match entry_row(ui, &self.entries[i], active, avatars, now) {
                                RowClick::Select => picked = Some(i),
                                RowClick::Reveal => event = Event::Reveal(self.entries[i].id),
                                RowClick::None => {}
                            }
                        }
                    },
                );
            });
        if let Some(i) = picked {
            self.select(repo, workdir, i);
        }
        if let Some((_, diff)) = &mut self.open {
            match diff.ui(ui, review, author, avatars) {
                diff_view::Event::Close => event = Event::Close,
                diff_view::Event::None | diff_view::Event::FileHistory => {}
                diff_view::Event::Reveal(id) => event = Event::Reveal(id),
                other => event = Event::Diff(other),
            }
        } else {
            let text = match &self.finished {
                Some(Err(e)) => format!("Could not read the history: {e}"),
                Some(Ok(())) => "No commits touch this file.".to_string(),
                None => "Reading the history…".to_string(),
            };
            ui.centered_and_justified(|ui| ui.label(RichText::new(text).color(theme::TEXT_MUTED)));
        }
        event
    }

    fn header(&self, ui: &mut Ui) -> Event {
        let mut event = Event::None;
        egui::Frame::new()
            .fill(theme::PANEL)
            .inner_margin(Margin::symmetric(16, 10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("History of")
                            .size(13.0)
                            .color(theme::TEXT_MUTED),
                    );
                    ui.label(
                        RichText::new(&self.path)
                            .monospace()
                            .family(theme::semibold())
                            .color(theme::TEXT_STRONG),
                    );
                    let count = self.entries.len();
                    let status = match self.finished {
                        None => format!("{count} commits so far…"),
                        Some(_) => format!("{count} commit{}", if count == 1 { "" } else { "s" }),
                    };
                    ui.label(RichText::new(status).size(12.0).color(theme::TEXT_FAINT));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::close_button(ui, "Close (Esc)") {
                            event = Event::Close;
                        }
                    });
                });
            });
        event
    }
}

impl Drop for FileHistoryView {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

enum RowClick {
    None,
    Select,
    Reveal,
}

fn entry_row(
    ui: &mut Ui,
    entry: &Entry,
    active: bool,
    avatars: &mut AvatarStore,
    now: i64,
) -> RowClick {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
    let painter = ui.painter_at(rect);
    if active {
        painter.rect_filled(rect, 0.0, theme::SIDEBAR_SELECTED);
    } else if response.hovered() {
        painter.rect_filled(rect, 0.0, theme::with_alpha(egui::Color32::WHITE, 0x08));
    }
    let avatar = pos2(rect.left() + 26.0, rect.center().y);
    graph_view::draw_avatar(
        &painter,
        avatar,
        AVATAR_R,
        &entry.author,
        &entry.email,
        avatars.texture(&entry.email, entry.id),
        theme::BORDER,
        false,
    );
    let text_left = rect.left() + 48.0;
    let right = rect.right() - 14.0;
    let counts = counts(&painter, entry);
    let counts_w = counts
        .as_ref()
        .map_or(0.0, |(plus, minus)| plus.size().x + minus.size().x + 14.0);
    let title_color = if active {
        theme::TEXT_STRONG
    } else {
        theme::TEXT
    };
    let title = graph_view::truncated(
        &painter,
        entry.title.clone(),
        FontId::new(13.0, theme::semibold()),
        title_color,
        (right - text_left - counts_w).max(0.0),
    );
    painter.galley(pos2(text_left, rect.top() + 8.0), title, title_color);
    if let Some((plus, minus)) = counts {
        let minus_x = right - minus.size().x;
        let plus_x = minus_x - 4.0 - plus.size().x;
        painter.galley(pos2(plus_x, rect.top() + 9.0), plus, theme::ADDED);
        painter.galley(pos2(minus_x, rect.top() + 9.0), minus, theme::DELETED);
    }
    let mut meta = format!(
        "{} · {}",
        entry.author,
        kelp_core::commit::relative_time(entry.time, now)
    );
    if let Some(from) = &entry.renamed_from {
        meta.push_str(&format!(" · renamed from {from}"));
    }
    let meta = graph_view::truncated(
        &painter,
        meta,
        FontId::proportional(12.0),
        theme::TEXT_FAINT,
        (right - text_left).max(0.0),
    );
    painter.galley(
        pos2(text_left, rect.bottom() - 21.0),
        meta,
        theme::TEXT_FAINT,
    );
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(format!(
            "{}\nDouble-click to show it in the graph",
            entry.id
        ));
    if response.double_clicked() {
        RowClick::Reveal
    } else if response.clicked() {
        RowClick::Select
    } else {
        RowClick::None
    }
}

fn counts(
    painter: &egui::Painter,
    entry: &Entry,
) -> Option<(std::sync::Arc<egui::Galley>, std::sync::Arc<egui::Galley>)> {
    let (added, removed) = (entry.added?, entry.removed?);
    let font = FontId::proportional(11.5);
    Some((
        painter.layout_no_wrap(format!("+{added}"), font.clone(), theme::ADDED),
        painter.layout_no_wrap(format!("−{removed}"), font, theme::DELETED),
    ))
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}
