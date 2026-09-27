use std::collections::BTreeMap;

use eframe::egui::{
    self, Align2, Color32, FontId, Margin, RichText, Sense, Stroke, Ui, pos2, vec2,
};
use kelp_core::commit::{self, ChangeKind, FileChange};
use kelp_core::review::Review;

use crate::repo_view::{Center, FileListMode, Repo, Selection};
use crate::theme;

const FILE_ROW_H: f32 = 28.0;
const INDENT: f32 = 16.0;

pub fn ui(ui: &mut Ui, repo: &mut Repo) {
    if repo.selected == Some(Selection::Wip) {
        crate::staging::ui(ui, repo);
        return;
    }
    let mut open = None;
    let mut reveal = None;
    let review_card_h = if repo.review.threads.is_empty() {
        0.0
    } else {
        104.0
    };
    let list_h = (ui.available_height() - review_card_h).max(100.0);
    ui.allocate_ui(vec2(ui.available_width(), list_h), |ui| {
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                let changes = if let Some(details) = repo.details.clone() {
                    reveal = commit_header(ui, repo, &details);
                    details.changes.clone()
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.label(RichText::new("Select a commit").color(theme::TEXT_MUTED))
                    });
                    return;
                };
                file_controls(ui, repo, &changes);
                ui.spacing_mut().item_spacing.y = 0.0;
                let active = open_diff_path(repo);
                open = match repo.file_list_mode {
                    FileListMode::Path => path_list(ui, &changes, active.as_deref(), &repo.review),
                    FileListMode::Tree => tree_list(ui, &changes, active.as_deref(), &repo.review),
                };
                if repo.show_all_files
                    && let Some(path) = all_files(ui, repo, &changes, active.as_deref())
                {
                    open = Some(path);
                }
            });
    });
    if !repo.review.threads.is_empty() {
        review_card(ui, repo);
    }
    if let Some(row) = reveal {
        repo.reveal(Selection::Commit(row));
    }
    if let Some(path) = open {
        repo.open_diff(&path);
    }
}

fn commit_header(ui: &mut Ui, repo: &mut Repo, details: &commit::Details) -> Option<usize> {
    let now = now();
    let lane = repo
        .history
        .row(&details.id)
        .map(|r| theme::lane(repo.history.layout.node_color(r)))
        .unwrap_or(theme::ACCENT);
    let mut reveal = None;
    egui::Frame::new()
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 10.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new("commit").color(theme::TEXT_MUTED));
                let short = details.id.to_hex_with_len(7).to_string();
                let label =
                    egui::Label::new(RichText::new(&short).monospace().color(theme::TEXT_STRONG))
                        .sense(Sense::click());
                if ui.add(label).on_hover_text("Copy full hash").clicked() {
                    ui.ctx().copy_text(details.id.to_string());
                }
            });
            ui.label(
                RichText::new(&details.title)
                    .size(17.0)
                    .family(theme::semibold())
                    .color(theme::TEXT_STRONG),
            );
            if !details.body.is_empty() {
                ui.label(RichText::new(&details.body).color(Color32::from_rgb(0xb4, 0xb9, 0xc2)));
            }
            ui.separator();
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(38.0, 38.0), Sense::hover());
                let texture = repo.avatars.texture(&details.email, details.id);
                crate::graph_view::draw_avatar(
                    ui.painter(),
                    rect.center(),
                    18.0,
                    &details.author,
                    &details.email,
                    texture,
                    lane,
                    false,
                );
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(RichText::new(&details.author).color(theme::TEXT_STRONG));
                    let when = format!("authored {}", commit::relative_time(details.time, now));
                    ui.label(RichText::new(when).size(12.0).color(theme::TEXT_MUTED));
                    if details.committer != details.author {
                        let by = format!("committed by {}", details.committer);
                        ui.label(RichText::new(by).size(12.0).color(theme::TEXT_MUTED));
                    }
                });
            });
            if !details.parents.is_empty() {
                ui.horizontal(|ui| {
                    let label = if details.parents.len() > 1 {
                        "parents"
                    } else {
                        "parent"
                    };
                    ui.label(RichText::new(label).size(12.0).color(theme::TEXT_MUTED));
                    for parent in &details.parents {
                        let short = parent.to_hex_with_len(7).to_string();
                        if ui
                            .link(
                                RichText::new(short)
                                    .monospace()
                                    .size(12.0)
                                    .color(theme::ACCENT),
                            )
                            .clicked()
                        {
                            reveal = repo.history.row(parent);
                        }
                    }
                });
            }
            ui.separator();
            stats(ui, &details.changes);
        });
    reveal
}

fn file_controls(ui: &mut Ui, repo: &mut Repo, changes: &[FileChange]) {
    egui::Frame::new()
        .inner_margin(Margin {
            left: 16,
            right: 16,
            top: 0,
            bottom: 8,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgb(0x12, 0x15, 0x1a))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(0x2c, 0x31, 0x3b)))
                    .corner_radius(6)
                    .inner_margin(Margin::same(2))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        for (mode, label) in
                            [(FileListMode::Path, "Path"), (FileListMode::Tree, "Tree")]
                        {
                            let active = repo.file_list_mode == mode;
                            let color = if active {
                                theme::TEXT_STRONG
                            } else {
                                theme::TEXT_MUTED
                            };
                            let fill = if active {
                                Color32::from_rgb(0x2b, 0x32, 0x40)
                            } else {
                                Color32::TRANSPARENT
                            };
                            let button =
                                egui::Button::new(RichText::new(label).size(12.0).color(color))
                                    .fill(fill)
                                    .corner_radius(4);
                            if ui.add(button).clicked() {
                                repo.file_list_mode = mode;
                            }
                        }
                    });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.checkbox(
                        &mut repo.show_all_files,
                        RichText::new("All files")
                            .size(12.0)
                            .color(theme::TEXT_MUTED),
                    );
                    if changes.is_empty() {
                        ui.label(
                            RichText::new("no file changes")
                                .size(12.0)
                                .color(theme::TEXT_FAINT),
                        );
                    }
                });
            });
        });
}

fn open_diff_path(repo: &Repo) -> Option<String> {
    match &repo.center {
        Center::Diff(view) => Some(view.path().to_string()),
        Center::Graph | Center::Worktrees => None,
    }
}

fn stats(ui: &mut Ui, changes: &[FileChange]) {
    let count = |k: ChangeKind| changes.iter().filter(|c| c.kind == k).count();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 14.0;
        let modified = count(ChangeKind::Modified) + count(ChangeKind::Renamed);
        ui.label(
            RichText::new(format!("{modified} modified"))
                .size(12.0)
                .color(theme::MODIFIED),
        );
        ui.label(
            RichText::new(format!("{} added", count(ChangeKind::Added)))
                .size(12.0)
                .color(theme::ADDED),
        );
        ui.label(
            RichText::new(format!("{} deleted", count(ChangeKind::Deleted)))
                .size(12.0)
                .color(theme::DELETED),
        );
    });
}

fn path_list(
    ui: &mut Ui,
    changes: &[FileChange],
    active: Option<&str>,
    review: &Review,
) -> Option<String> {
    let mut clicked = None;
    for change in changes {
        let row = FileRow {
            kind: Some(change.kind),
            path: &change.path,
            label: None,
            depth: 0,
            active: active == Some(change.path.as_str()),
            comments: review.count_for(&change.path),
        };
        if row.show(ui).clicked() {
            clicked = Some(change.path.clone());
        }
    }
    clicked
}

#[derive(Default)]
struct Folder<'a> {
    folders: BTreeMap<&'a str, Folder<'a>>,
    files: Vec<&'a FileChange>,
}

fn tree_list(
    ui: &mut Ui,
    changes: &[FileChange],
    active: Option<&str>,
    review: &Review,
) -> Option<String> {
    let mut root = Folder::default();
    for change in changes {
        let mut folder = &mut root;
        let mut parts: Vec<&str> = change.path.split('/').collect();
        parts.pop();
        for part in parts {
            folder = folder.folders.entry(part).or_default();
        }
        folder.files.push(change);
    }
    let mut clicked = None;
    show_folder(ui, &root, "", 0, active, review, &mut clicked);
    clicked
}

fn show_folder(
    ui: &mut Ui,
    folder: &Folder<'_>,
    prefix: &str,
    depth: usize,
    active: Option<&str>,
    review: &Review,
    clicked: &mut Option<String>,
) {
    for (name, child) in &folder.folders {
        let mut path = format!("{prefix}{name}");
        let mut node = child;
        while node.files.is_empty() && node.folders.len() == 1 {
            let (next_name, next) = node.folders.iter().next().unwrap();
            path = format!("{path}/{next_name}");
            node = next;
        }
        let id = ui.make_persistent_id(("changed-dir", &path));
        let mut open = ui.data_mut(|d| *d.get_persisted_mut_or(id, true));
        let label = path.strip_prefix(prefix).unwrap_or(&path).to_string();
        if folder_row(ui, &label, depth, open).clicked() {
            open = !open;
            ui.data_mut(|d| d.insert_persisted(id, open));
        }
        if open {
            show_folder(
                ui,
                node,
                &format!("{path}/"),
                depth + 1,
                active,
                review,
                clicked,
            );
        }
    }
    for change in &folder.files {
        let name = change.path.rsplit('/').next().unwrap_or(&change.path);
        let row = FileRow {
            kind: Some(change.kind),
            path: &change.path,
            label: Some(name),
            depth,
            active: active == Some(change.path.as_str()),
            comments: review.count_for(&change.path),
        };
        if row.show(ui).clicked() {
            *clicked = Some(change.path.clone());
        }
    }
}

fn all_files(
    ui: &mut Ui,
    repo: &mut Repo,
    changes: &[FileChange],
    active: Option<&str>,
) -> Option<String> {
    let Some(Selection::Commit(row)) = repo.selected else {
        return None;
    };
    let id = repo.history.id(row);
    ui.add_space(10.0);
    egui::Frame::new()
        .inner_margin(Margin::symmetric(16, 6))
        .show(ui, |ui| {
            ui.label(
                RichText::new("ALL FILES")
                    .size(11.0)
                    .family(theme::semibold())
                    .color(theme::TEXT_MUTED),
            );
        });
    let mut clicked = None;
    browse(ui, repo, id, "", 0, changes, active, &mut clicked);
    clicked
}

#[allow(clippy::too_many_arguments)]
fn browse(
    ui: &mut Ui,
    repo: &mut Repo,
    id: gix::ObjectId,
    dir: &str,
    depth: usize,
    changes: &[FileChange],
    active: Option<&str>,
    clicked: &mut Option<String>,
) {
    let entries = match repo.tree_cache.get(dir) {
        Some(entries) => entries.clone(),
        None => {
            let entries = commit::list_dir(&repo.repo, id, dir).unwrap_or_default();
            repo.tree_cache.insert(dir.to_string(), entries.clone());
            entries
        }
    };
    for entry in entries {
        if entry.is_dir {
            let key = ui.make_persistent_id(("tree-dir", id, &entry.path));
            let mut open = ui.data_mut(|d| *d.get_persisted_mut_or(key, false));
            if folder_row(ui, &entry.name, depth, open).clicked() {
                open = !open;
                ui.data_mut(|d| d.insert_persisted(key, open));
            }
            if open {
                browse(
                    ui,
                    repo,
                    id,
                    &entry.path,
                    depth + 1,
                    changes,
                    active,
                    clicked,
                );
            }
        } else {
            let kind = changes
                .iter()
                .find(|c| c.path == entry.path)
                .map(|c| c.kind);
            let row = FileRow {
                kind,
                path: &entry.path,
                label: Some(&entry.name),
                depth,
                active: active == Some(entry.path.as_str()),
                comments: repo.review.count_for(&entry.path),
            };
            if row.show(ui).clicked() {
                *clicked = Some(entry.path.clone());
            }
        }
    }
}

fn folder_row(ui: &mut Ui, name: &str, depth: usize, open: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), FILE_ROW_H), Sense::click());
    let painter = ui.painter_at(rect);
    if response.hovered() {
        painter.rect_filled(rect, 0.0, theme::with_alpha(Color32::WHITE, 0x08));
    }
    let x = rect.left() + 14.0 + depth as f32 * INDENT;
    let y = rect.center().y;
    painter.text(
        pos2(x, y),
        Align2::LEFT_CENTER,
        if open { "▾" } else { "▸" },
        FontId::proportional(11.0),
        theme::TEXT_FAINT,
    );
    painter.text(
        pos2(x + 16.0, y),
        Align2::LEFT_CENTER,
        name,
        FontId::monospace(12.0),
        Color32::from_rgb(0xb4, 0xb9, 0xc2),
    );
    response
}

struct FileRow<'a> {
    kind: Option<ChangeKind>,
    path: &'a str,
    label: Option<&'a str>,
    depth: usize,
    active: bool,
    comments: usize,
}

impl FileRow<'_> {
    fn show(&self, ui: &mut Ui) -> egui::Response {
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), FILE_ROW_H), Sense::click());
        let painter = ui.painter_at(rect);
        if self.active {
            painter.rect_filled(rect, 0.0, theme::SIDEBAR_SELECTED);
        } else if response.hovered() {
            painter.rect_filled(rect, 0.0, theme::with_alpha(Color32::WHITE, 0x08));
        }
        let y = rect.center().y;
        let font = FontId::monospace(12.0);
        let left = rect.left() + 14.0 + self.depth as f32 * INDENT;
        if let Some(kind) = self.kind {
            let (mark, color) = match kind {
                ChangeKind::Added => ("A", theme::ADDED),
                ChangeKind::Deleted => ("D", theme::DELETED),
                ChangeKind::Modified => ("M", theme::MODIFIED),
                ChangeKind::Renamed => ("R", theme::MODIFIED),
            };
            painter.text(
                pos2(left + 6.0, y),
                Align2::CENTER_CENTER,
                mark,
                font.clone(),
                color,
            );
        }
        let mut right = rect.right() - 12.0;
        if self.comments > 0 {
            let g = painter.layout_no_wrap(
                self.comments.to_string(),
                FontId::proportional(11.0),
                Color32::from_rgb(0x10, 0x13, 0x1a),
            );
            let badge = egui::Rect::from_center_size(
                pos2(right - 9.0, y),
                vec2((g.size().x + 10.0).max(18.0), 18.0),
            );
            painter.rect_filled(badge, 9.0, theme::ACCENT);
            painter.galley(
                badge.center() - g.size() / 2.0,
                g,
                Color32::from_rgb(0x10, 0x13, 0x1a),
            );
            right = badge.left() - 8.0;
        }
        let text_left = left + 20.0;
        let name_color = if self.kind == Some(ChangeKind::Deleted) {
            theme::TEXT_FAINT
        } else {
            theme::TEXT
        };
        match self.label {
            Some(label) => {
                let g = crate::graph_view::truncated(
                    &painter,
                    label.to_string(),
                    font,
                    name_color,
                    (right - text_left).max(0.0),
                );
                painter.galley(pos2(text_left, y - g.size().y / 2.0), g, name_color);
            }
            None => {
                let (dir, name) = self
                    .path
                    .rsplit_once('/')
                    .map_or(("", self.path), |(d, n)| (d, n));
                let name_galley =
                    painter.layout_no_wrap(name.to_string(), font.clone(), name_color);
                let dir_space = (right - text_left - name_galley.size().x).max(0.0);
                let mut x = text_left;
                if !dir.is_empty() {
                    let dir_text = elide_end(&painter, &format!("{dir}/"), &font, dir_space);
                    let g = painter.layout_no_wrap(dir_text, font.clone(), theme::TEXT_FAINT);
                    x += g.size().x;
                    painter.galley(pos2(text_left, y - g.size().y / 2.0), g, theme::TEXT_FAINT);
                }
                painter.galley(
                    pos2(x, y - name_galley.size().y / 2.0),
                    name_galley,
                    name_color,
                );
            }
        }
        response.on_hover_text(self.path)
    }
}

fn review_card(ui: &mut Ui, repo: &mut Repo) {
    let total = repo.review.threads.len();
    let open = repo.review.open_count();
    egui::Frame::new()
        .fill(Color32::from_rgb(0x20, 0x25, 0x2e))
        .stroke(Stroke::new(1.0, Color32::from_rgb(0x2c, 0x33, 0x40)))
        .corner_radius(8)
        .inner_margin(Margin::same(14))
        .outer_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 10.0;
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Your review")
                        .family(theme::semibold())
                        .color(theme::TEXT_STRONG),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let text = format!(
                        "{total} comment{} · {open} open",
                        if total == 1 { "" } else { "s" }
                    );
                    ui.label(RichText::new(text).size(12.0).color(theme::TEXT_MUTED));
                });
            });
            let copy = egui::Button::new(RichText::new("Copy as Markdown").size(12.0))
                .corner_radius(5)
                .min_size(vec2(ui.available_width(), 32.0));
            if ui.add(copy).clicked() {
                ui.ctx().copy_text(repo.review.to_markdown());
                repo.notify("Review copied as Markdown", false);
            }
        });
}

fn elide_end(painter: &egui::Painter, text: &str, font: &FontId, max_width: f32) -> String {
    let width = |t: &str| {
        painter
            .layout_no_wrap(t.to_string(), font.clone(), theme::TEXT)
            .size()
            .x
    };
    if width(text) <= max_width {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut lo, mut hi) = (0, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let candidate: String = chars[..mid].iter().collect::<String>() + "…/";
        if width(&candidate) <= max_width {
            lo = mid
        } else {
            hi = mid - 1
        }
    }
    if lo == 0 {
        String::new()
    } else {
        chars[..lo].iter().collect::<String>() + "…/"
    }
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}
