use eframe::egui::{
    self, Align2, Color32, FontId, Key, Margin, RichText, Sense, Stroke, Ui, pos2, vec2,
};
use kelp_core::commit::{ChangeKind, FileChange};
use kelp_core::ops::Op;

use crate::dialogs::Dialog;
use crate::icons::Icon;
use crate::menus;
use crate::repo_view::{Center, Repo};
use crate::theme;

const ROW_H: f32 = 28.0;
const SUMMARY_LIMIT: usize = 72;

enum Action {
    Open(String, bool),
    Run(Op),
    Confirm(Dialog),
    Copy(String),
}

pub fn ui(ui: &mut Ui, repo: &mut Repo) {
    let mut actions = Vec::new();
    egui::Panel::bottom("commit-box")
        .frame(egui::Frame::NONE)
        .show(ui, |ui| commit_box(ui, repo));
    let list_h = ui.available_height().max(120.0);
    ui.allocate_ui(vec2(ui.available_width(), list_h), |ui| {
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                header(ui, repo);
                ui.spacing_mut().item_spacing.y = 0.0;
                let active = match &repo.center {
                    Center::Diff(view) if view.is_working() => {
                        Some((view.path().to_string(), view.is_staged()))
                    }
                    _ => None,
                };
                if !repo.status.conflicted.is_empty() {
                    section(
                        ui,
                        "Conflicts",
                        repo.status.conflicted.len(),
                        None,
                        &mut actions,
                    );
                    for path in &repo.status.conflicted {
                        let change = FileChange {
                            path: path.clone(),
                            kind: ChangeKind::Modified,
                        };
                        let (row, _) =
                            file_row(ui, &change, false, None, Some("resolve, then stage"));
                        row.on_hover_text(
                            "Fix the conflict markers in your editor, then stage the file.",
                        );
                    }
                    ui.add_space(10.0);
                }
                let has_head = repo.has_head();
                section(
                    ui,
                    "Unstaged",
                    repo.status.unstaged.len(),
                    (!repo.status.unstaged.is_empty()).then_some(("Stage all", Op::StageAll)),
                    &mut actions,
                );
                for change in &repo.status.unstaged {
                    let untracked = repo.status.untracked.contains(&change.path);
                    let is_active = active
                        .as_ref()
                        .is_some_and(|(p, staged)| p == &change.path && !staged);
                    let (row, staged) = file_row(ui, change, is_active, Some("Stage"), None);
                    if staged {
                        actions.push(Action::Run(Op::Stage(vec![change.path.clone()])));
                    } else if row.clicked() {
                        actions.push(Action::Open(change.path.clone(), false));
                    }
                    row.context_menu(|ui| {
                        ui.set_min_width(210.0);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        if menus::row(ui, Some(Icon::Plus), "Stage", None, false) {
                            actions.push(Action::Run(Op::Stage(vec![change.path.clone()])));
                        }
                        if menus::row(ui, Some(Icon::Copy), "Copy path", None, false) {
                            actions.push(Action::Copy(change.path.clone()));
                        }
                        menus::separator(ui);
                        let label = if untracked {
                            "Delete file…"
                        } else {
                            "Discard changes…"
                        };
                        if menus::row(ui, Some(Icon::Trash), label, None, true) {
                            actions.push(Action::Confirm(discard_dialog(&change.path, untracked)));
                        }
                    });
                }
                if repo.status.unstaged.is_empty() {
                    empty_note(ui, "Nothing to stage");
                }
                ui.add_space(10.0);
                section(
                    ui,
                    "Staged",
                    repo.status.staged.len(),
                    (!repo.status.staged.is_empty())
                        .then_some(("Unstage all", Op::UnstageAll { has_head })),
                    &mut actions,
                );
                for change in &repo.status.staged {
                    let is_active = active
                        .as_ref()
                        .is_some_and(|(p, staged)| p == &change.path && *staged);
                    let (row, unstaged) = file_row(ui, change, is_active, Some("Unstage"), None);
                    if unstaged {
                        actions.push(Action::Run(Op::Unstage {
                            paths: vec![change.path.clone()],
                            has_head,
                        }));
                    } else if row.clicked() {
                        actions.push(Action::Open(change.path.clone(), true));
                    }
                    row.context_menu(|ui| {
                        ui.set_min_width(210.0);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        if menus::row(ui, Some(Icon::Minus), "Unstage", None, false) {
                            actions.push(Action::Run(Op::Unstage {
                                paths: vec![change.path.clone()],
                                has_head,
                            }));
                        }
                        if menus::row(ui, Some(Icon::Copy), "Copy path", None, false) {
                            actions.push(Action::Copy(change.path.clone()));
                        }
                    });
                }
                if repo.status.staged.is_empty() {
                    empty_note(ui, "Stage files or hunks to commit them");
                }
            });
    });

    for action in actions {
        match action {
            Action::Open(path, staged) => repo.open_working_diff(&path, staged),
            Action::Run(op) => repo.run_op(op),
            Action::Confirm(dialog) => repo.dialog = Some(dialog),
            Action::Copy(text) => {
                ui.ctx().copy_text(text.clone());
                repo.notify(format!("Copied {text}"), false);
            }
        }
    }
}

fn discard_dialog(path: &str, untracked: bool) -> Dialog {
    let (title, op) = if untracked {
        (
            format!("Delete {path}?"),
            Op::DeleteUntracked(vec![path.to_string()]),
        )
    } else {
        (
            format!("Discard changes to {path}?"),
            Op::DiscardChanges(vec![path.to_string()]),
        )
    };
    Dialog::Confirm {
        title,
        body: "This can't be undone.".into(),
        op,
        danger: true,
    }
}

fn header(ui: &mut Ui, repo: &Repo) {
    egui::Frame::new()
        .inner_margin(Margin {
            left: 16,
            right: 16,
            top: 16,
            bottom: 10,
        })
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.label(
                RichText::new("Uncommitted changes")
                    .size(17.0)
                    .family(theme::semibold())
                    .color(theme::TEXT_STRONG),
            );
            let branch = repo.current_branch().unwrap_or("detached HEAD");
            ui.label(RichText::new(format!("on {branch}")).color(theme::TEXT_MUTED));
        });
}

fn section(
    ui: &mut Ui,
    title: &str,
    count: usize,
    button: Option<(&str, Op)>,
    actions: &mut Vec<Action>,
) {
    egui::Frame::new()
        .inner_margin(Margin::symmetric(16, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(title.to_uppercase())
                        .size(11.0)
                        .family(theme::semibold())
                        .color(theme::TEXT_MUTED),
                );
                ui.label(
                    RichText::new(count.to_string())
                        .size(11.0)
                        .color(theme::TEXT_FAINT),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some((label, op)) = button {
                        let b =
                            egui::Button::new(RichText::new(label).size(11.0).color(theme::ACCENT))
                                .frame(false);
                        if ui.add(b).clicked() {
                            actions.push(Action::Run(op));
                        }
                    }
                });
            });
        });
}

fn empty_note(ui: &mut Ui, text: &str) {
    egui::Frame::new()
        .inner_margin(Margin::symmetric(16, 4))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(12.0).color(theme::TEXT_FAINT));
        });
}

fn file_row(
    ui: &mut Ui,
    change: &FileChange,
    active: bool,
    button: Option<&str>,
    note: Option<&str>,
) -> (egui::Response, bool) {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
    let painter = ui.painter_at(rect);
    let hovered = response.hovered() || ui.rect_contains_pointer(rect);
    if active {
        painter.rect_filled(rect, 0.0, theme::SIDEBAR_SELECTED);
    } else if hovered {
        painter.rect_filled(rect, 0.0, theme::with_alpha(Color32::WHITE, 0x08));
    }
    let y = rect.center().y;
    let (mark, color) = match change.kind {
        ChangeKind::Added => ("A", theme::ADDED),
        ChangeKind::Deleted => ("D", theme::DELETED),
        ChangeKind::Modified => ("M", theme::MODIFIED),
        ChangeKind::Renamed => ("R", theme::MODIFIED),
    };
    let font = FontId::monospace(12.0);
    painter.text(
        pos2(rect.left() + 20.0, y),
        Align2::CENTER_CENTER,
        mark,
        font.clone(),
        color,
    );

    let mut right = rect.right() - 12.0;
    let mut clicked = false;
    if let Some(label) = button
        && hovered
    {
        let g = painter.layout_no_wrap(
            label.to_string(),
            FontId::proportional(11.5),
            theme::TEXT_STRONG,
        );
        let button_rect = egui::Rect::from_min_size(
            pos2(right - g.size().x - 16.0, y - 10.0),
            vec2(g.size().x + 16.0, 20.0),
        );
        let b = ui.interact(button_rect, response.id.with("action"), Sense::click());
        let fill = if b.hovered() {
            theme::ACCENT
        } else {
            Color32::from_rgb(0x2b, 0x32, 0x40)
        };
        let ink = if b.hovered() {
            Color32::from_rgb(0x10, 0x13, 0x1a)
        } else {
            theme::TEXT_STRONG
        };
        painter.rect(
            button_rect,
            5.0,
            fill,
            Stroke::NONE,
            egui::StrokeKind::Inside,
        );
        painter.text(
            button_rect.center(),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(11.5),
            ink,
        );
        clicked = b.clicked();
        right = button_rect.left() - 8.0;
    } else if let Some(note) = note {
        let g = painter.layout_no_wrap(
            note.to_string(),
            FontId::proportional(11.0),
            theme::TEXT_FAINT,
        );
        painter.galley(
            pos2(right - g.size().x, y - g.size().y / 2.0),
            g.clone(),
            theme::TEXT_FAINT,
        );
        right -= g.size().x + 8.0;
    }
    let (dir, name) = change
        .path
        .rsplit_once('/')
        .map_or(("", change.path.as_str()), |(d, n)| (d, n));
    let left = rect.left() + 36.0;
    let name_galley = crate::graph_view::truncated(
        &painter,
        name.to_string(),
        font.clone(),
        theme::TEXT,
        (right - left).max(0.0),
    );
    let name_w = name_galley.size().x;
    let dir_w = (right - left - name_w).max(0.0);
    let mut x = left;
    if !dir.is_empty() && dir_w > 24.0 {
        let g = crate::graph_view::truncated(
            &painter,
            format!("{dir}/"),
            font,
            theme::TEXT_FAINT,
            dir_w,
        );
        painter.galley(pos2(x, y - g.size().y / 2.0), g.clone(), theme::TEXT_FAINT);
        x += g.size().x;
    }
    painter.galley(
        pos2(x, y - name_galley.size().y / 2.0),
        name_galley,
        theme::TEXT,
    );
    (response.on_hover_text(&change.path), clicked)
}

fn commit_box(ui: &mut Ui, repo: &mut Repo) {
    let staged = repo.status.staged.len();
    egui::Frame::new()
        .fill(Color32::from_rgb(0x20, 0x25, 0x2e))
        .stroke(Stroke::new(1.0, Color32::from_rgb(0x2c, 0x33, 0x40)))
        .corner_radius(10)
        .inner_margin(Margin::same(14))
        .outer_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.horizontal(|ui| {
                let title = if repo.amend {
                    "Amend last commit"
                } else {
                    "Commit"
                };
                ui.label(
                    RichText::new(title)
                        .family(theme::semibold())
                        .color(theme::TEXT_STRONG),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let len = repo.commit_summary.chars().count();
                    let color = if len > SUMMARY_LIMIT {
                        theme::MODIFIED
                    } else {
                        theme::TEXT_FAINT
                    };
                    ui.label(
                        RichText::new(format!("{len}/{SUMMARY_LIMIT}"))
                            .size(11.0)
                            .color(color),
                    );
                });
            });
            let summary = ui.add(
                egui::TextEdit::singleline(&mut repo.commit_summary)
                    .hint_text("Summary (required)")
                    .desired_width(f32::INFINITY)
                    .margin(Margin::symmetric(10, 8)),
            );
            let body = ui.add(
                egui::TextEdit::multiline(&mut repo.commit_body)
                    .hint_text("Description")
                    .desired_rows(3)
                    .desired_width(f32::INFINITY)
                    .margin(Margin::symmetric(10, 8)),
            );
            let mut amend = repo.amend;
            if ui
                .checkbox(
                    &mut amend,
                    RichText::new("Amend previous commit")
                        .size(12.0)
                        .color(theme::TEXT_MUTED),
                )
                .changed()
            {
                repo.toggle_amend();
            }
            let ready = !repo.commit_summary.trim().is_empty()
                && (staged > 0 || repo.amend)
                && !repo.committing();
            let label = match (repo.amend, staged) {
                (true, _) => "Amend commit".to_string(),
                (false, 1) => "Commit 1 file".to_string(),
                (false, n) => format!("Commit {n} files"),
            };
            let ink = if ready {
                Color32::from_rgb(0x10, 0x13, 0x1a)
            } else {
                theme::TEXT_FAINT
            };
            let button =
                egui::Button::new(RichText::new(label).family(theme::semibold()).color(ink))
                    .fill(if ready {
                        theme::ACCENT
                    } else {
                        Color32::from_rgb(0x2b, 0x32, 0x40)
                    })
                    .corner_radius(6)
                    .min_size(vec2(ui.available_width(), 34.0));
            let shortcut = (summary.has_focus() || body.has_focus())
                && ui.input(|i| i.modifiers.command && i.key_pressed(Key::Enter));
            let hint = if staged == 0 && !repo.amend {
                "Stage changes first"
            } else {
                "Commit (⌘ Enter)"
            };
            if (ui.add_enabled(ready, button).on_hover_text(hint).clicked() || shortcut) && ready {
                repo.commit();
            }
        });
}
