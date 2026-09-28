use std::path::Path;

use eframe::egui::{self, FontId, Margin, RichText, Sense, Ui, vec2};
use gix::ObjectId;
use kelp_core::commit::{self, FileChange};
use kelp_core::review::Review;

use crate::diff_view::{self, DiffSource, DiffView};
use crate::{theme, widgets};

const LIST_W: f32 = 260.0;
const ROW_H: f32 = 28.0;

pub struct StashView {
    pub name: String,
    message: String,
    id: ObjectId,
    changes: Vec<FileChange>,
    open: Option<(usize, Box<DiffView>)>,
}

pub enum Event {
    None,
    Close,
    Diff(diff_view::Event),
}

impl StashView {
    pub fn open(
        repo: &gix::Repository,
        workdir: Option<&Path>,
        name: &str,
    ) -> anyhow::Result<Self> {
        let id = repo
            .rev_parse_single(name)
            .map_err(|e| anyhow::anyhow!("{name}: {e}"))?
            .detach();
        let details = commit::details(repo, id)?;
        let mut view = Self {
            name: name.to_string(),
            message: details.title,
            id,
            changes: details.changes,
            open: None,
        };
        view.select(repo, workdir, 0);
        Ok(view)
    }

    fn select(&mut self, repo: &gix::Repository, workdir: Option<&Path>, index: usize) {
        let Some(change) = self.changes.get(index) else {
            return;
        };
        if let Ok(diff) = DiffView::load(repo, workdir, DiffSource::Commit(self.id), &change.path) {
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
    ) -> Event {
        let mut event = Event::None;
        egui::Frame::new()
            .fill(theme::PANEL)
            .inner_margin(Margin::symmetric(16, 10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(&self.name)
                            .monospace()
                            .color(theme::TEXT_MUTED),
                    );
                    ui.label(
                        RichText::new(&self.message)
                            .family(theme::semibold())
                            .color(theme::TEXT_STRONG),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::close_button(ui, "Close (Esc)") {
                            event = Event::Close;
                        }
                    });
                });
            });
        let mut picked = None;
        egui::Panel::left("stash-files")
            .exact_size(LIST_W)
            .frame(egui::Frame::new().fill(theme::PANEL))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for (i, change) in self.changes.iter().enumerate() {
                        let active = self.open.as_ref().is_some_and(|(open, _)| *open == i);
                        if file_row(ui, change, active) {
                            picked = Some(i);
                        }
                    }
                    if self.changes.is_empty() {
                        ui.label(RichText::new("No file changes").color(theme::TEXT_FAINT));
                    }
                });
            });
        if let Some(i) = picked {
            self.select(repo, workdir, i);
        }
        if let Some((_, diff)) = &mut self.open {
            match diff.ui(ui, review, author) {
                diff_view::Event::Close => event = Event::Close,
                diff_view::Event::None => {}
                other => event = Event::Diff(other),
            }
        }
        event
    }
}

fn file_row(ui: &mut Ui, change: &FileChange, active: bool) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
    if active {
        ui.painter().rect_filled(rect, 0.0, theme::SIDEBAR_SELECTED);
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, 0.0, theme::with_alpha(egui::Color32::WHITE, 0x08));
    }
    let (letter, color) = crate::details::change_letter(change.kind);
    ui.painter().text(
        rect.left_center() + vec2(14.0, 0.0),
        egui::Align2::LEFT_CENTER,
        letter,
        FontId::monospace(12.0),
        color,
    );
    let name = crate::graph_view::truncated(
        ui.painter(),
        change.path.clone(),
        FontId::monospace(12.0),
        theme::TEXT,
        rect.width() - 44.0,
    );
    ui.painter().galley(
        rect.left_center() + vec2(32.0, -name.size().y / 2.0),
        name,
        theme::TEXT,
    );
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}
