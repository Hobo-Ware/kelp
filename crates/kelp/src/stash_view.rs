use std::path::Path;

use eframe::egui::{self, FontId, Margin, RichText, Sense, Ui, vec2};
use gix::ObjectId;
use kelp_core::commit::{self, ChangeKind, FileChange};
use kelp_core::review::Review;

use crate::avatars::AvatarStore;
use crate::diff_view::{self, DiffSource, DiffView};
use crate::{theme, widgets};

const LIST_W: f32 = 260.0;
const ROW_H: f32 = 28.0;

pub struct StashView {
    pub name: String,
    message: String,
    files: Vec<StashFile>,
    open: Option<(usize, Box<DiffView>)>,
}

struct StashFile {
    change: FileChange,
    commit: ObjectId,
    untracked: bool,
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
        let mut files: Vec<StashFile> = details
            .changes
            .into_iter()
            .map(|change| StashFile {
                change,
                commit: id,
                untracked: false,
            })
            .collect();
        let dir = workdir.unwrap_or(repo.path());
        if let Some((untracked, paths)) = kelp_core::reflog::stash_untracked(dir, name) {
            files.extend(paths.into_iter().map(|path| StashFile {
                change: FileChange {
                    path,
                    kind: ChangeKind::Added,
                },
                commit: untracked,
                untracked: true,
            }));
        }
        let mut view = Self {
            name: name.to_string(),
            message: details.title,
            files,
            open: None,
        };
        view.select(repo, workdir, 0);
        Ok(view)
    }

    fn select(&mut self, repo: &gix::Repository, workdir: Option<&Path>, index: usize) {
        let Some(file) = self.files.get(index) else {
            return;
        };
        if let Ok(diff) = DiffView::load(
            repo,
            workdir,
            DiffSource::Commit(file.commit),
            &file.change.path,
        ) {
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
                    for (i, file) in self.files.iter().enumerate() {
                        let active = self.open.as_ref().is_some_and(|(open, _)| *open == i);
                        if file_row(ui, file, active) {
                            picked = Some(i);
                        }
                    }
                    if self.files.is_empty() {
                        ui.label(RichText::new("No file changes").color(theme::TEXT_FAINT));
                    }
                });
            });
        if let Some(i) = picked {
            self.select(repo, workdir, i);
        }
        if let Some((_, diff)) = &mut self.open {
            match diff.ui(ui, review, author, avatars) {
                diff_view::Event::Close => event = Event::Close,
                diff_view::Event::None => {}
                other => event = Event::Diff(other),
            }
        }
        event
    }
}

fn file_row(ui: &mut Ui, file: &StashFile, active: bool) -> bool {
    let change = &file.change;
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
    let tag_w = if file.untracked {
        let tag = ui.painter().layout_no_wrap(
            "untracked".into(),
            FontId::proportional(10.5),
            theme::TEXT_FAINT,
        );
        let w = tag.size().x;
        ui.painter().galley(
            egui::pos2(
                rect.right() - 10.0 - w,
                rect.center().y - tag.size().y / 2.0,
            ),
            tag,
            theme::TEXT_FAINT,
        );
        w + 12.0
    } else {
        0.0
    };
    let name = crate::graph_view::truncated(
        ui.painter(),
        change.path.clone(),
        FontId::monospace(12.0),
        theme::TEXT,
        rect.width() - 44.0 - tag_w,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untracked_files_are_listed_and_open_as_added() {
        let dir = std::env::temp_dir().join(format!("kelp-stash-view-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str]| kelp_core::git_cli::run(&dir, args).unwrap();
        git(&["init", "-q", "-b", "main"]);
        git(&["config", "user.email", "test@example.com"]);
        git(&["config", "user.name", "Test"]);
        std::fs::write(dir.join("a.txt"), "one\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "first"]);
        std::fs::write(dir.join("a.txt"), "two\n").unwrap();
        std::fs::write(dir.join("new.txt"), "fresh\n").unwrap();
        git(&["stash", "push", "-q", "-u"]);

        let repo = gix::open(&dir).unwrap();
        let mut view = StashView::open(&repo, Some(&dir), "stash@{0}").unwrap();
        let paths: Vec<(&str, bool)> = view
            .files
            .iter()
            .map(|f| (f.change.path.as_str(), f.untracked))
            .collect();
        assert_eq!(paths, [("a.txt", false), ("new.txt", true)]);
        view.select(&repo, Some(&dir), 1);
        let (index, diff) = view.open.as_ref().unwrap();
        assert_eq!(*index, 1);
        assert_eq!(diff.diff.added, 1);
        assert_eq!(diff.diff.removed, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
