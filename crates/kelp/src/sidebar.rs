use eframe::egui::{self, Color32, RichText, Sense, Ui, vec2};
use kelp_core::refs::{RefKind, RefLabel};

use crate::commands::Command;
use crate::menus;
use crate::repo_view::{Repo, Selection};
use crate::{graph_view, theme};

pub fn ui(ui: &mut Ui, repo: &mut Repo, commands: &mut Vec<Command>) {
    let menu_ctx = repo.menu_context();
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add_space(8.0);
            for (title, kind) in [
                ("LOCAL", RefKind::Local),
                ("REMOTE", RefKind::Remote),
                ("TAGS", RefKind::Tag),
            ] {
                let labels: Vec<&RefLabel> = repo.history.refs.of_kind(kind).collect();
                section(ui, title, labels.len(), kind != RefKind::Tag, None, |ui| {
                    for label in labels {
                        let selected =
                            label.row.map(|r| Selection::Commit(r as usize)) == repo.selected;
                        let dot = label
                            .row
                            .map(|r| theme::lane(repo.history.layout.node_color(r as usize)))
                            .unwrap_or(theme::TEXT_FAINT);
                        let badge = match repo.workspace.ahead_behind.get(&label.name) {
                            Some((a, b)) if kind == RefKind::Local && (*a > 0 || *b > 0) => {
                                let mut s = String::new();
                                if *a > 0 {
                                    s.push_str(&format!("↑{a}"));
                                }
                                if *b > 0 {
                                    s.push_str(&format!(" ↓{b}"));
                                }
                                Some(s.trim().to_string())
                            }
                            _ => None,
                        };
                        let tag = if label.is_head {
                            Some("HEAD".to_string())
                        } else {
                            badge
                        };
                        let response = row(
                            ui,
                            &label.name,
                            None,
                            dot,
                            label.is_head,
                            selected,
                            tag.as_deref(),
                        );
                        if response.clicked()
                            && let Some(r) = label.row
                        {
                            commands.push(Command::Reveal(Selection::Commit(r as usize)));
                        }
                        if response.double_clicked() && kind != RefKind::Tag {
                            if kind == RefKind::Local && !label.is_head {
                                commands.push(Command::Run(kelp_core::ops::Op::Switch(
                                    label.name.clone(),
                                )));
                            } else if kind == RefKind::Remote {
                                commands.push(Command::Run(kelp_core::ops::Op::SwitchTrack(
                                    label.name.clone(),
                                )));
                            }
                        }
                        let forced = kind == RefKind::Local
                            && label.is_head
                            && std::env::var("KELP_OPEN_MENU").as_deref() == Ok("branch");
                        if forced {
                            egui::Popup::from_response(&response)
                                .open(true)
                                .show(|ui| menus::branch(ui, label, &menu_ctx, commands));
                        } else {
                            response
                                .context_menu(|ui| menus::branch(ui, label, &menu_ctx, commands));
                        }
                    }
                });
            }

            let worktrees = &repo.workspace.worktrees;
            let mut manage = false;
            section(
                ui,
                "WORKTREES",
                worktrees.len(),
                true,
                Some(&mut manage),
                |ui| {
                    for wt in worktrees {
                        let current = std::fs::canonicalize(&wt.tree.path).ok()
                            == std::fs::canonicalize(&repo.dir).ok();
                        let branch = wt.tree.branch.clone().unwrap_or_else(|| "detached".into());
                        let sub = match wt.changes {
                            Some(0) | None if current => format!("{branch} · current"),
                            Some(0) | None => branch,
                            Some(n) => format!("{branch} · {n} changed"),
                        };
                        let dot = repo
                            .history
                            .refs
                            .of_kind(RefKind::Local)
                            .find(|l| Some(&l.name) == wt.tree.branch.as_ref())
                            .and_then(|l| l.row)
                            .map(|r| theme::lane(repo.history.layout.node_color(r as usize)))
                            .unwrap_or(theme::TEXT_FAINT);
                        let response =
                            row(ui, &wt.tree.name(), Some(&sub), dot, current, false, None);
                        if response.double_clicked() && !current {
                            commands.push(Command::OpenRepo(wt.tree.path.clone()));
                        } else if response.clicked() {
                            commands.push(Command::ShowWorktrees);
                        }
                        response.context_menu(|ui| menus::worktree(ui, &wt.tree, commands));
                    }
                },
            );
            if manage {
                commands.push(Command::ShowWorktrees);
            }

            let stashes = &repo.workspace.stashes;
            section(ui, "STASHES", stashes.len(), false, None, |ui| {
                for stash in stashes {
                    let response = row(
                        ui,
                        &stash.message,
                        Some(&stash.name),
                        theme::TEXT_FAINT,
                        false,
                        false,
                        None,
                    );
                    response.context_menu(|ui| menus::stash(ui, stash, commands));
                }
            });
        });
}

fn section(
    ui: &mut Ui,
    title: &str,
    count: usize,
    open_by_default: bool,
    manage: Option<&mut bool>,
    body: impl FnOnce(&mut Ui),
) {
    let id = ui.make_persistent_id(title);
    let state = egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        id,
        open_by_default,
    );
    state
        .show_header(ui, |ui| {
            ui.label(
                RichText::new(title)
                    .size(11.0)
                    .family(theme::semibold())
                    .color(theme::TEXT_MUTED),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(12.0);
                ui.label(
                    RichText::new(count.to_string())
                        .size(11.0)
                        .color(theme::TEXT_MUTED),
                );
                if let Some(manage) = manage
                    && ui
                        .add(
                            egui::Button::new(
                                RichText::new("Manage").size(11.0).color(theme::ACCENT),
                            )
                            .frame(false),
                        )
                        .clicked()
                {
                    *manage = true;
                }
            });
        })
        .body(body);
    ui.add_space(6.0);
}

fn row(
    ui: &mut Ui,
    name: &str,
    subtitle: Option<&str>,
    dot: Color32,
    strong: bool,
    selected: bool,
    tag: Option<&str>,
) -> egui::Response {
    let height = if subtitle.is_some() { 40.0 } else { 28.0 };
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    let painter = ui.painter_at(rect);
    if selected {
        painter.rect_filled(rect, 0.0, theme::SIDEBAR_SELECTED);
    } else if response.hovered() {
        painter.rect_filled(rect, 0.0, theme::with_alpha(Color32::WHITE, 0x08));
    }
    let name_y = if subtitle.is_some() {
        rect.top() + 13.0
    } else {
        rect.center().y
    };
    painter.circle_filled(egui::pos2(rect.left() + 12.0, name_y), 4.0, dot);
    let color = if selected || strong {
        theme::TEXT_STRONG
    } else {
        Color32::from_rgb(0xd5, 0xd7, 0xdc)
    };
    let font = egui::FontId::proportional(if strong { 13.5 } else { 13.0 });
    let tag_galley =
        tag.map(|t| painter.layout_no_wrap(t.to_string(), egui::FontId::proportional(11.0), dot));
    let reserved = tag_galley.as_ref().map_or(12.0, |g| g.size().x + 20.0);
    let max_width = (rect.width() - 24.0 - reserved).max(0.0);
    let galley = graph_view::truncated(&painter, name.to_string(), font, color, max_width);
    painter.galley(
        egui::pos2(rect.left() + 24.0, name_y - galley.size().y / 2.0),
        galley,
        color,
    );
    if let Some(sub) = subtitle {
        let sub = graph_view::truncated(
            &painter,
            sub.to_string(),
            egui::FontId::proportional(11.0),
            theme::TEXT_FAINT,
            rect.width() - 36.0,
        );
        painter.galley(
            egui::pos2(rect.left() + 24.0, rect.top() + 22.0),
            sub,
            theme::TEXT_FAINT,
        );
    }
    if let Some(g) = tag_galley {
        let color = if strong { dot } else { theme::TEXT_FAINT };
        painter.galley(
            egui::pos2(rect.right() - 12.0 - g.size().x, name_y - g.size().y / 2.0),
            g,
            color,
        );
    }
    response
}
