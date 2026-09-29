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
            .fill(theme::panel())
            .inner_margin(Margin::symmetric(16, 10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(&self.name)
                            .monospace()
                            .color(theme::text_muted()),
                    );
                    ui.label(
                        RichText::new(&self.message)
                            .family(theme::semibold())
                            .color(theme::text_strong()),
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
            .frame(egui::Frame::new().fill(theme::panel()))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let open = self.open.as_ref().map(|(open, _)| *open);
                    let mut rows = Vec::new();
                    for (i, file) in self.files.iter().enumerate() {
                        let row = file_row(ui, file, open == Some(i));
                        if row.clicked() {
                            picked = Some(i);
                        }
                        rows.push((row.id, row.rect));
                    }
                    let owns_keys = crate::list_keys::nothing_focused(ui.ctx());
                    if let Some(i) = crate::list_keys::step(ui, &rows, open, owns_keys) {
                        picked = Some(i);
                    }
                    if self.files.is_empty() {
                        ui.label(RichText::new("No file changes").color(theme::text_faint()));
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

fn file_row(ui: &mut Ui, file: &StashFile, active: bool) -> egui::Response {
    let change = &file.change;
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
    if active {
        ui.painter()
            .rect_filled(rect, 0.0, theme::sidebar_selected());
    } else if response.hovered() {
        ui.painter().rect_filled(rect, 0.0, theme::overlay(0x08));
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
            theme::text_faint(),
        );
        let w = tag.size().x;
        ui.painter().galley(
            egui::pos2(
                rect.right() - 10.0 - w,
                rect.center().y - tag.size().y / 2.0,
            ),
            tag,
            theme::text_faint(),
        );
        w + 12.0
    } else {
        0.0
    };
    let name = crate::graph_view::truncated(
        ui.painter(),
        change.path.clone(),
        FontId::monospace(12.0),
        theme::text(),
        rect.width() - 44.0 - tag_w,
    );
    ui.painter().galley(
        rect.left_center() + vec2(32.0, -name.size().y / 2.0),
        name,
        theme::text(),
    );
    widgets::focus_ring(ui, &response, 0.0);
    crate::focus_areas::offer(
        ui.ctx(),
        crate::focus_areas::Area::Graph,
        response.id,
        active,
    );
    widgets::describe_selected(&response, format!("file {}", change.path), active);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
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

    fn frame(
        ctx: &egui::Context,
        view: &mut StashView,
        repo: &gix::Repository,
        dir: &Path,
        events: Vec<egui::Event>,
    ) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(1000.0, 700.0),
            )),
            events,
            ..Default::default()
        };
        let mut review = Review::default();
        let mut avatars = AvatarStore::new(ctx.clone(), None);
        let _ = ctx.run_ui(input, |ui| {
            view.ui(ui, repo, Some(dir), &mut review, "T", &mut avatars);
        });
    }

    #[test]
    fn arrows_walk_the_stashed_files() {
        let dir = std::env::temp_dir().join(format!("kelp-stash-keys-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str]| kelp_core::git_cli::run(&dir, args).unwrap();
        git(&["init", "-q", "-b", "main"]);
        git(&["config", "user.email", "test@example.com"]);
        git(&["config", "user.name", "Test"]);
        for name in ["a.txt", "b.txt", "c.txt"] {
            std::fs::write(dir.join(name), "one\n").unwrap();
        }
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "first"]);
        for name in ["a.txt", "b.txt", "c.txt"] {
            std::fs::write(dir.join(name), "two\n").unwrap();
        }
        git(&["stash", "push", "-q"]);

        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let repo = gix::open(&dir).unwrap();
        let mut view = StashView::open(&repo, Some(&dir), "stash@{0}").unwrap();
        let open = |view: &StashView| view.open.as_ref().map(|(i, _)| *i);
        let press = |view: &mut StashView, key| {
            frame(&ctx, view, &repo, &dir, crate::list_keys::tap(key));
            frame(&ctx, view, &repo, &dir, vec![]);
        };
        frame(&ctx, &mut view, &repo, &dir, vec![]);
        assert_eq!(open(&view), Some(0));
        press(&mut view, egui::Key::ArrowDown);
        assert_eq!(open(&view), Some(1));
        press(&mut view, egui::Key::End);
        assert_eq!(open(&view), Some(2));
        press(&mut view, egui::Key::Home);
        assert_eq!(open(&view), Some(0));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
