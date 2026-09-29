use eframe::egui::{
    self, Align2, Color32, FontId, Key, Margin, RichText, Sense, Stroke, Ui, pos2, vec2,
};
use kelp_core::commit;
use kelp_core::ops::Op;
use kelp_core::refs::RefKind;

use crate::commands::Command;
use crate::dialogs::{Dialog, NewWorktree};
use crate::repo_view::{Repo, Selection};
use crate::{graph_view, menus, theme};

const ROW_H: f32 = 52.0;
const BRANCH_ROW_H: f32 = 44.0;
const WORKTREE_COLS: [f32; 5] = [0.17, 0.2, 0.27, 0.11, 0.25];
const BRANCH_COLS: [f32; 5] = [0.26, 0.13, 0.33, 0.15, 0.13];

#[derive(Clone, PartialEq, Eq, Debug)]
enum Picked {
    Worktree(std::path::PathBuf),
    Branch(String),
}

enum Enter {
    Open(std::path::PathBuf),
    Reveal(usize),
    Nothing,
}

fn picked_id(repo: &Repo) -> egui::Id {
    egui::Id::new(("worktrees-picked", &repo.dir))
}

fn paint_row_state(ui: &Ui, response: &egui::Response, picked: bool) {
    let painter = ui.painter_at(response.rect);
    if picked {
        painter.rect_filled(response.rect, 0.0, theme::sidebar_selected());
    } else if response.hovered() {
        painter.rect_filled(response.rect, 0.0, theme::overlay(0x06));
    }
    crate::widgets::focus_ring(ui, response, 0.0);
}

pub fn ui(ui: &mut Ui, repo: &mut Repo, commands: &mut Vec<Command>) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let mut picked: Option<Picked> = ui.data(|d| d.get_temp(picked_id(repo)));
    let mut rows: Vec<(egui::Id, egui::Rect)> = Vec::new();
    let mut items: Vec<(Picked, Enter)> = Vec::new();
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(Margin::symmetric(32, 28))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 12.0;
                    header(
                        ui,
                        "Worktrees",
                        &format!("{} checked out", repo.workspace.worktrees.len()),
                        |ui| {
                            if crate::widgets::close_button(ui, "Close (Esc)") {
                                commands.push(match repo.selected {
                                    Some(s) => Command::Reveal(s),
                                    None => Command::Reveal(Selection::Commit(0)),
                                });
                            }
                            ui.add_space(8.0);
                            if ui.add(accent_button("New worktree")).clicked() {
                                let ctx = repo.menu_context();
                                let start = repo.current_branch().unwrap_or("HEAD").to_string();
                                commands.push(Command::Open(Dialog::NewWorktree(
                                    NewWorktree::new(
                                        ctx.repo_dir_name.to_string(),
                                        start,
                                        None,
                                        ctx.local_branches,
                                    ),
                                )));
                            }
                            if ui
                                .add(plain_button("Prune stale"))
                                .on_hover_text("git worktree prune")
                                .clicked()
                            {
                                commands.push(Command::Run(Op::WorktreePrune));
                            }
                        },
                    );
                    table(
                        ui,
                        &["NAME", "BRANCH", "FOLDER", "STATUS", ""],
                        &WORKTREE_COLS,
                        |ui| {
                            for wt in &repo.workspace.worktrees {
                                let current = std::fs::canonicalize(&wt.tree.path).ok()
                                    == std::fs::canonicalize(&repo.dir).ok();
                                let (rect, response) = ui.allocate_exact_size(
                                    vec2(ui.available_width(), ROW_H),
                                    Sense::click(),
                                );
                                let cols = columns(rect, &WORKTREE_COLS);
                                let item = Picked::Worktree(wt.tree.path.clone());
                                paint_row_state(ui, &response, picked.as_ref() == Some(&item));
                                let openable = !current && !wt.tree.prunable;
                                if response.clicked() {
                                    picked = Some(item.clone());
                                }
                                if openable && response.double_clicked() {
                                    commands.push(Command::OpenWorktree(wt.tree.path.clone()));
                                }
                                rows.push((response.id, rect));
                                items.push((
                                    item,
                                    if openable {
                                        Enter::Open(wt.tree.path.clone())
                                    } else {
                                        Enter::Nothing
                                    },
                                ));
                                let painter = ui.painter_at(rect);
                                painter.hline(
                                    rect.x_range(),
                                    rect.bottom() - 0.5,
                                    Stroke::new(1.0, theme::card()),
                                );
                                let y = rect.center().y;
                                let name = painter.text(
                                    pos2(cols[0].left(), y),
                                    Align2::LEFT_CENTER,
                                    wt.tree.name(),
                                    FontId::proportional(13.5),
                                    theme::text_strong(),
                                );
                                if current {
                                    badge(
                                        &painter,
                                        pos2(name.right() + 8.0, y),
                                        "current",
                                        theme::lanes()[0],
                                    );
                                }
                                let branch =
                                    wt.tree.branch.clone().unwrap_or_else(|| "detached".into());
                                let dot = lane_of(repo, wt.tree.branch.as_deref());
                                painter.circle_filled(pos2(cols[1].left() + 4.0, y), 4.0, dot);
                                let g = graph_view::truncated(
                                    &painter,
                                    branch,
                                    FontId::proportional(13.0),
                                    theme::text_control(),
                                    cols[1].width() - 24.0,
                                );
                                painter.galley(
                                    pos2(cols[1].left() + 16.0, y - g.size().y / 2.0),
                                    g,
                                    theme::text_control(),
                                );
                                let folder = graph_view::truncated(
                                    &painter,
                                    tilde(&wt.tree.path),
                                    FontId::monospace(12.0),
                                    theme::text_muted(),
                                    cols[2].width() - 16.0,
                                );
                                painter.galley(
                                    pos2(cols[2].left(), y - folder.size().y / 2.0),
                                    folder,
                                    theme::text_muted(),
                                );
                                let (status, color) = match (wt.tree.prunable, wt.changes) {
                                    (true, _) => ("missing".to_string(), theme::deleted()),
                                    (_, Some(0)) => ("clean".to_string(), theme::added()),
                                    (_, Some(n)) => (format!("{n} changed"), theme::modified()),
                                    (_, None) => ("…".to_string(), theme::text_faint()),
                                };
                                painter.text(
                                    pos2(cols[3].left(), y),
                                    Align2::LEFT_CENTER,
                                    status,
                                    FontId::proportional(13.0),
                                    color,
                                );

                                let mut actions = ui.new_child(
                                    egui::UiBuilder::new()
                                        .max_rect(cols[4])
                                        .layout(egui::Layout::right_to_left(egui::Align::Center)),
                                );
                                let remove = actions.add_enabled(
                                    !wt.tree.is_main && !current,
                                    plain_button("Remove"),
                                );
                                if remove.clicked() {
                                    commands.push(menus::remove_worktree(&wt.tree.path));
                                }
                                if actions
                                    .add_enabled(!wt.tree.is_main && !current, plain_button("Move"))
                                    .on_hover_text("git worktree move")
                                    .clicked()
                                {
                                    commands.push(menus::move_worktree(&wt.tree.path));
                                }
                                if actions
                                    .add_enabled(
                                        !current && !wt.tree.prunable,
                                        plain_button("Open"),
                                    )
                                    .on_hover_text("Open in this tab")
                                    .clicked()
                                {
                                    commands.push(Command::OpenWorktree(wt.tree.path.clone()));
                                }
                                crate::menus::context_menu(&response, |ui| {
                                    menus::worktree(ui, &wt.tree, commands)
                                });
                            }
                        },
                    );

                    ui.add_space(16.0);
                    let locals: Vec<_> =
                        repo.history.refs.of_kind(RefKind::Local).cloned().collect();
                    header(
                        ui,
                        "Local branches",
                        &format!("{} branches · compared to upstream", locals.len()),
                        |ui| {
                            if ui.add(plain_button("New branch")).clicked() {
                                let start = repo.current_branch().unwrap_or("HEAD").to_string();
                                commands.push(Command::Open(Dialog::NewBranch {
                                    name: String::new(),
                                    start_label: start.clone(),
                                    start,
                                    switch: true,
                                }));
                            }
                        },
                    );
                    let menu_ctx = repo.menu_context();
                    table(
                        ui,
                        &[
                            "BRANCH",
                            "AHEAD / BEHIND",
                            "LAST COMMIT",
                            "WORKTREE",
                            "UPDATED",
                        ],
                        &BRANCH_COLS,
                        |ui| {
                            for label in &locals {
                                let (rect, response) = ui.allocate_exact_size(
                                    vec2(ui.available_width(), BRANCH_ROW_H),
                                    Sense::click(),
                                );
                                let cols = columns(rect, &BRANCH_COLS);
                                let item = Picked::Branch(label.name.clone());
                                paint_row_state(ui, &response, picked.as_ref() == Some(&item));
                                rows.push((response.id, rect));
                                items.push((
                                    item,
                                    label
                                        .row
                                        .map_or(Enter::Nothing, |r| Enter::Reveal(r as usize)),
                                ));
                                let painter = ui.painter_at(rect);
                                painter.hline(
                                    rect.x_range(),
                                    rect.bottom() - 0.5,
                                    Stroke::new(1.0, theme::card()),
                                );
                                let y = rect.center().y;
                                let dot = lane_of(repo, Some(&label.name));
                                painter.circle_filled(pos2(cols[0].left() + 4.0, y), 4.0, dot);
                                let name_color = if label.is_head {
                                    theme::text_strong()
                                } else {
                                    theme::text_control()
                                };
                                let g = graph_view::truncated(
                                    &painter,
                                    label.name.clone(),
                                    FontId::proportional(13.0),
                                    name_color,
                                    cols[0].width() - 24.0,
                                );
                                painter.galley(
                                    pos2(cols[0].left() + 16.0, y - g.size().y / 2.0),
                                    g,
                                    name_color,
                                );
                                let ab = match repo.workspace.ahead_behind.get(&label.name) {
                                    Some((a, b)) => format!("{a} / {b}"),
                                    None => "no upstream".into(),
                                };
                                painter.text(
                                    pos2(cols[1].left(), y),
                                    Align2::LEFT_CENTER,
                                    ab,
                                    FontId::monospace(12.0),
                                    theme::text_muted(),
                                );
                                let summary = label.row.and_then(|r| {
                                    commit::summary(&repo.repo, repo.history.id(r as usize)).ok()
                                });
                                if let Some(summary) = &summary {
                                    let g = graph_view::truncated(
                                        &painter,
                                        summary.title.clone(),
                                        FontId::proportional(13.0),
                                        theme::text_body(),
                                        cols[2].width() - 16.0,
                                    );
                                    painter.galley(
                                        pos2(cols[2].left(), y - g.size().y / 2.0),
                                        g,
                                        theme::text_body(),
                                    );
                                    painter.text(
                                        pos2(cols[4].left(), y),
                                        Align2::LEFT_CENTER,
                                        commit::relative_time(summary.time, now),
                                        FontId::proportional(12.0),
                                        theme::text_faint(),
                                    );
                                }
                                let tree = repo
                                    .workspace
                                    .worktrees
                                    .iter()
                                    .find(|w| w.tree.branch.as_deref() == Some(label.name.as_str()))
                                    .map(|w| w.tree.name())
                                    .unwrap_or_else(|| "—".into());
                                painter.text(
                                    pos2(cols[3].left(), y),
                                    Align2::LEFT_CENTER,
                                    tree,
                                    FontId::proportional(13.0),
                                    theme::text_muted(),
                                );
                                if response.clicked()
                                    && let Some(r) = label.row
                                {
                                    commands.push(Command::Reveal(Selection::Commit(r as usize)));
                                }
                                crate::menus::context_menu(&response, |ui| {
                                    menus::branch(ui, label, &menu_ctx, commands)
                                });
                            }
                        },
                    );
                    let at = picked
                        .as_ref()
                        .and_then(|p| items.iter().position(|(item, _)| item == p));
                    let ctx = ui.ctx().clone();
                    let free = crate::list_keys::nothing_focused(&ctx);
                    if let Some(next) = crate::list_keys::step(ui, &rows, at, free) {
                        picked = Some(items[next].0.clone());
                    } else if let Some(at) = crate::list_keys::current(&ctx, &rows, at, free)
                        && crate::list_keys::pressed(&ctx, Key::Enter)
                    {
                        match &items[at].1 {
                            Enter::Open(path) => commands.push(Command::OpenWorktree(path.clone())),
                            Enter::Reveal(row) => {
                                commands.push(Command::Reveal(Selection::Commit(*row)))
                            }
                            Enter::Nothing => {}
                        }
                    }
                });
        });
    ui.data_mut(|d| match picked {
        Some(p) => {
            d.insert_temp(picked_id(repo), p);
        }
        None => d.remove::<Picked>(picked_id(repo)),
    });
}

fn lane_of(repo: &Repo, branch: Option<&str>) -> Color32 {
    repo.history
        .refs
        .of_kind(RefKind::Local)
        .find(|l| Some(l.name.as_str()) == branch)
        .and_then(|l| l.row)
        .map(|r| theme::lane(repo.history.layout.node_color(r as usize)))
        .unwrap_or(theme::text_faint())
}

fn tilde(path: &std::path::Path) -> String {
    let text = path.display().to_string();
    match std::env::var("HOME") {
        Ok(home) if text.starts_with(&home) => format!("~{}", &text[home.len()..]),
        _ => text,
    }
}

fn header(ui: &mut Ui, title: &str, subtitle: &str, actions: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(title)
                .size(18.0)
                .family(theme::semibold())
                .color(theme::text_strong()),
        );
        ui.add_space(8.0);
        ui.label(RichText::new(subtitle).color(theme::text_faint()));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), actions);
    });
}

fn table(ui: &mut Ui, headers: &[&str], widths: &[f32], rows: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(theme::panel())
        .stroke(Stroke::new(1.0, theme::control()))
        .corner_radius(8)
        .inner_margin(Margin::symmetric(16, 0))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let (rect, _) =
                ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::hover());
            let painter = ui.painter_at(rect);
            for (col, title) in columns(rect, widths).iter().zip(headers) {
                painter.text(
                    pos2(col.left(), rect.center().y),
                    Align2::LEFT_CENTER,
                    *title,
                    FontId::monospace(10.0),
                    theme::text_faint(),
                );
            }
            painter.hline(
                rect.x_range(),
                rect.bottom() - 0.5,
                Stroke::new(1.0, theme::control()),
            );
            rows(ui);
        });
}

fn columns(rect: egui::Rect, fractions: &[f32]) -> Vec<egui::Rect> {
    let mut x = rect.left();
    fractions
        .iter()
        .map(|f| {
            let w = rect.width() * f;
            let col = egui::Rect::from_min_size(pos2(x, rect.top()), vec2(w, rect.height()));
            x += w;
            col
        })
        .collect()
}

fn badge(painter: &egui::Painter, left_center: egui::Pos2, text: &str, color: Color32) {
    let g = painter.layout_no_wrap(text.to_string(), FontId::proportional(11.0), color);
    let rect = egui::Rect::from_min_size(
        pos2(left_center.x, left_center.y - 9.0),
        vec2(g.size().x + 12.0, 18.0),
    );
    painter.rect_filled(rect, 3.0, theme::with_alpha(color, 0x26));
    painter.galley(
        pos2(rect.left() + 6.0, left_center.y - g.size().y / 2.0),
        g,
        color,
    );
}

fn plain_button(text: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(text).size(12.0))
        .corner_radius(5)
        .min_size(vec2(0.0, 30.0))
}

fn accent_button(text: &str) -> egui::Button<'_> {
    egui::Button::new(
        RichText::new(text)
            .size(12.0)
            .family(theme::semibold())
            .color(theme::on_accent()),
    )
    .fill(theme::accent())
    .corner_radius(5)
    .min_size(vec2(0.0, 30.0))
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use eframe::egui::{self, Event, Key, Modifiers, Pos2, RawInput, Rect, vec2};
    use kelp_core::git_cli::run;

    use super::Picked;
    use crate::commands::Command;
    use crate::repo_view::Repo;

    fn press(ctx: &egui::Context, repo: &mut Repo, key: Option<Key>) -> Vec<Command> {
        let events = key
            .map(|key| {
                [true, false]
                    .map(|pressed| Event::Key {
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
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1100.0, 900.0))),
            events,
            ..Default::default()
        };
        let mut commands = Vec::new();
        let _ = ctx.run_ui(input, |ui| super::ui(ui, repo, &mut commands));
        commands
    }

    fn picked(ctx: &egui::Context, repo: &Repo) -> Option<Picked> {
        ctx.data(|d| d.get_temp(super::picked_id(repo)))
    }

    #[test]
    fn arrows_walk_worktrees_then_branches_and_enter_opens_a_worktree() {
        let root = std::env::temp_dir().join(format!("kelp-wt-keys-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("main");
        std::fs::create_dir_all(&dir).unwrap();
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.email", "t@example.com"],
            &["config", "user.name", "T"],
            &["commit", "-q", "--allow-empty", "-m", "one"],
            &["worktree", "add", "-q", "-b", "side", "../side"],
        ] {
            run(&dir, args).unwrap();
        }
        let side = std::fs::canonicalize(root.join("side")).unwrap();
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let (git, history) = kelp_core::history::History::open(&dir).unwrap();
        let mut repo = Repo::new(&ctx, git, history, Duration::ZERO);
        let started = Instant::now();
        while repo.workspace.worktrees.len() < 2 && started.elapsed() < Duration::from_secs(5) {
            repo.poll(&ctx, None);
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(repo.workspace.worktrees.len(), 2);
        press(&ctx, &mut repo, None);

        press(&ctx, &mut repo, Some(Key::ArrowDown));
        press(&ctx, &mut repo, Some(Key::ArrowDown));
        let second = match picked(&ctx, &repo) {
            Some(Picked::Worktree(path)) => std::fs::canonicalize(path).unwrap(),
            other => panic!("expected the second worktree, got {other:?}"),
        };
        assert_eq!(second, side);
        let opened = press(&ctx, &mut repo, Some(Key::Enter));
        let opens_side = |c: &Command| matches!(c, Command::OpenWorktree(p) if std::fs::canonicalize(p).ok().as_ref() == Some(&side));
        assert!(opened.iter().any(opens_side));

        press(&ctx, &mut repo, Some(Key::ArrowDown));
        assert!(matches!(picked(&ctx, &repo), Some(Picked::Branch(_))));
        press(&ctx, &mut repo, Some(Key::Home));
        assert!(matches!(picked(&ctx, &repo), Some(Picked::Worktree(_))));
        let _ = std::fs::remove_dir_all(&root);
    }
}
